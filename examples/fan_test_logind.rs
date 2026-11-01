//! Private-bus logind fixture for tests/fan_lifecycle.py. Never uses the host bus.
use std::collections::HashMap;
use std::error::Error;
use std::os::fd::OwnedFd as StdOwnedFd;
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::AsyncReadExt;
use tokio::sync::Notify;
use zbus::object_server::SignalEmitter;
use zbus::zvariant::OwnedFd;
use zbus::zvariant::OwnedValue;
use zbus::{fdo, interface};

struct LoginManager {
    active: Arc<AtomicUsize>,
    changed: Arc<Notify>,
    delay: Duration,
}

struct TestControl {
    active: Arc<AtomicUsize>,
    changed: Arc<Notify>,
    delay: Duration,
    root: PathBuf,
}

// Reproduce a real logind where one unrelated property makes GetAll fail,
// while the individual inhibitor-budget property and Inhibit still work.
struct FaultyProperties {
    active: Arc<AtomicUsize>,
    delay: Duration,
}

#[interface(name = "org.freedesktop.DBus.Properties")]
impl FaultyProperties {
    fn get(&self, interface_name: &str, property_name: &str) -> fdo::Result<OwnedValue> {
        match (interface_name, property_name) {
            ("org.freedesktop.login1.Manager", "InhibitDelayMaxUSec") => {
                Ok((self.delay.as_micros() as u64).into())
            }
            ("org.shadowblip.Fan.TestLogind", "ActiveInhibitors") => {
                Ok((self.active.load(Ordering::Acquire) as u32).into())
            }
            _ => Err(fdo::Error::UnknownProperty(property_name.into())),
        }
    }

    fn get_all(&self, interface_name: &str) -> fdo::Result<HashMap<String, OwnedValue>> {
        if interface_name == "org.freedesktop.login1.Manager" {
            return Err(fdo::Error::Failed(
                "unrelated BootLoaderEntries property failed".into(),
            ));
        }
        let mut values = HashMap::new();
        if interface_name == "org.shadowblip.Fan.TestLogind" {
            values.insert(
                "ActiveInhibitors".into(),
                (self.active.load(Ordering::Acquire) as u32).into(),
            );
        }
        Ok(values)
    }
}

#[interface(name = "org.freedesktop.login1.Manager")]
impl LoginManager {
    fn inhibit(&self, what: &str, _who: &str, _why: &str, mode: &str) -> fdo::Result<OwnedFd> {
        if what != "sleep" || mode != "delay" {
            return Err(fdo::Error::InvalidArgs(
                "expected a sleep delay inhibitor".into(),
            ));
        }
        let (reader, writer) = UnixStream::pair().map_err(|e| fdo::Error::Failed(e.to_string()))?;
        reader
            .set_nonblocking(true)
            .map_err(|e| fdo::Error::Failed(e.to_string()))?;
        let mut reader = tokio::net::UnixStream::from_std(reader)
            .map_err(|e| fdo::Error::Failed(e.to_string()))?;
        self.active.fetch_add(1, Ordering::AcqRel);
        let active = Arc::clone(&self.active);
        let changed = Arc::clone(&self.changed);
        tokio::spawn(async move {
            let _ = reader.read(&mut [0; 1]).await;
            active.fetch_sub(1, Ordering::AcqRel);
            changed.notify_one();
        });
        let descriptor: StdOwnedFd = writer.into();
        Ok(descriptor.into())
    }

    #[zbus(property(emits_changed_signal = "const"), name = "InhibitDelayMaxUSec")]
    fn inhibit_delay_max_usec(&self) -> u64 {
        self.delay.as_micros() as u64
    }

    #[zbus(signal)]
    async fn prepare_for_sleep(emitter: &SignalEmitter<'_>, sleeping: bool) -> zbus::Result<()>;
}

#[interface(name = "org.shadowblip.Fan.TestLogind")]
impl TestControl {
    async fn prepare_sleep(
        &self,
        sleeping: bool,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> fdo::Result<()> {
        if sleeping {
            let active = Arc::clone(&self.active);
            let changed = Arc::clone(&self.changed);
            let delay = self.delay;
            let root = self.root.clone();
            tokio::spawn(async move {
                let outcome = tokio::time::timeout(delay, async {
                    while active.load(Ordering::Acquire) != 0 {
                        changed.notified().await;
                    }
                })
                .await;
                let outcome = if outcome.is_ok() {
                    "released"
                } else {
                    "expired"
                };
                let _ = std::fs::write(root.join("suspend-outcome"), outcome);
            });
        }
        LoginManager::prepare_for_sleep(&emitter, sleeping)
            .await
            .map_err(|e| fdo::Error::Failed(e.to_string()))
    }

    #[zbus(property)]
    fn active_inhibitors(&self) -> u32 {
        self.active.load(Ordering::Acquire) as u32
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let mut arguments = std::env::args().skip(1);
    let address = arguments.next().ok_or("missing private D-Bus address")?;
    let root = PathBuf::from(arguments.next().ok_or("missing fixture directory")?);
    let active = Arc::new(AtomicUsize::new(0));
    let changed = Arc::new(Notify::new());
    let delay = Duration::from_millis(250);
    let _connection = zbus::connection::Builder::address(address.as_str())?
        .name("org.freedesktop.login1")?
        .serve_at(
            "/org/freedesktop/login1",
            LoginManager {
                active: Arc::clone(&active),
                changed: Arc::clone(&changed),
                delay,
            },
        )?
        .serve_at(
            "/org/freedesktop/login1",
            TestControl {
                active: Arc::clone(&active),
                changed,
                delay,
                root,
            },
        )?
        .build()
        .await?;
    _connection
        .object_server()
        .remove::<zbus::fdo::Properties, _>("/org/freedesktop/login1")
        .await?;
    _connection
        .object_server()
        .at(
            "/org/freedesktop/login1",
            FaultyProperties { active, delay },
        )
        .await?;
    std::future::pending::<()>().await;
    Ok(())
}
