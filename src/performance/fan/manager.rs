use super::config::{load_databases, select_device, DmiInfo, FanConfig};
use super::controller::{restore_automatic_for_config, ControllerError, FanController};
use super::dbus::{FanBus, FanDevice};
use super::persistence::{SavedMode, StateStore};
use crate::constants::FAN_PATH;
use futures_util::{FutureExt, StreamExt};
use std::fmt::{Display, Formatter};
use std::panic::AssertUnwindSafe;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::{Duration, Instant};
use tokio::sync::{oneshot, Mutex};
use tokio::task::JoinHandle;
use zbus::zvariant::OwnedFd;
use zbus::{proxy, Connection};

const DEFAULT_PLATFORM_DIR: &str = "/usr/share/powerstation/platform";
const DEFAULT_SYSFS_ROOT: &str = "/sys";
const DEFAULT_STATE_PATH: &str = "/var/lib/powerstation/fan-state.yaml";
const LEGACY_AY3_STATE_PATH: &str = "/var/lib/ay3-fancontrol/config.json";
const PLATFORM_DIR_ENV: &str = "POWERSTATION_FAN_PLATFORM_DIR";
const SYSFS_ROOT_ENV: &str = "POWERSTATION_FAN_SYSFS_ROOT";
const STATE_PATH_ENV: &str = "POWERSTATION_FAN_STATE_PATH";
const LEGACY_STATE_PATH_ENV: &str = "POWERSTATION_FAN_LEGACY_STATE_PATH";

#[derive(Debug)]
pub struct ManagerError(pub String);

impl Display for ManagerError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ManagerError {}

impl From<ControllerError> for ManagerError {
    fn from(value: ControllerError) -> Self {
        Self(value.to_string())
    }
}

#[proxy(
    interface = "org.freedesktop.login1.Manager",
    default_service = "org.freedesktop.login1",
    default_path = "/org/freedesktop/login1"
)]
trait LoginManager {
    fn inhibit(&self, what: &str, who: &str, why: &str, mode: &str) -> zbus::Result<OwnedFd>;

    #[zbus(signal)]
    fn prepare_for_sleep(&self, sleeping: bool) -> zbus::Result<()>;
}

#[derive(Clone)]
pub struct FanManager {
    controllers: Vec<(String, Arc<Mutex<FanController>>)>,
    monitor_health: Arc<Vec<StdMutex<Instant>>>,
    sleep_monitor_healthy: Arc<AtomicBool>,
}

impl FanManager {
    pub fn empty() -> Self {
        Self {
            controllers: Vec::new(),
            monitor_health: Arc::new(Vec::new()),
            sleep_monitor_healthy: Arc::new(AtomicBool::new(true)),
        }
    }

    pub fn discover_default() -> Result<Self, ManagerError> {
        Self::discover(
            platform_directory(),
            configured_path(SYSFS_ROOT_ENV, DEFAULT_SYSFS_ROOT),
            configured_path(STATE_PATH_ENV, DEFAULT_STATE_PATH),
            configured_path(LEGACY_STATE_PATH_ENV, LEGACY_AY3_STATE_PATH),
        )
    }

    pub fn discover(
        platform_dir: PathBuf,
        sysfs_root: PathBuf,
        state_path: PathBuf,
        legacy_state_path: PathBuf,
    ) -> Result<Self, ManagerError> {
        let devices =
            load_databases(&platform_dir).map_err(|error| ManagerError(error.to_string()))?;
        let dmi = DmiInfo::read(&sysfs_root);
        let Some(device) =
            select_device(&devices, &dmi).map_err(|error| ManagerError(error.to_string()))?
        else {
            log::info!("No fan platform configuration matches this system");
            return Ok(Self {
                controllers: Vec::new(),
                monitor_health: Arc::new(Vec::new()),
                sleep_monitor_healthy: Arc::new(AtomicBool::new(true)),
            });
        };
        log::info!(
            "Matched fan platform configuration {} ({})",
            device.id,
            device.name
        );
        let device_id = device.id.clone();
        let fans = device.fans;
        let store = match StateStore::open(state_path) {
            Ok(store) => store,
            Err(error) => {
                return Err(initialization_failure(
                    error.to_string(),
                    &sysfs_root,
                    &fans,
                ))
            }
        };
        let mut controllers = Vec::new();
        for (index, config) in fans.iter().cloned().enumerate() {
            let key = format!("{device_id}/{}", config.id);
            let migrated = match store.migrate_legacy_quiet(
                &key,
                &legacy_state_path,
                config.presets.contains_key("Quiet"),
            ) {
                Ok(migrated) => migrated,
                Err(error) => {
                    return Err(initialization_failure(
                        error.to_string(),
                        &sysfs_root,
                        &fans,
                    ))
                }
            };
            if migrated {
                log::info!("Migrated legacy Quiet fan mode for {key}");
            }
            let controller = match FanController::new(
                device_id.clone(),
                config,
                sysfs_root.clone(),
                store.clone(),
            ) {
                Ok(controller) => controller,
                Err(error) => {
                    return Err(initialization_failure(
                        error.to_string(),
                        &sysfs_root,
                        &fans,
                    ))
                }
            };
            let path = format!("{FAN_PATH}/Fan{index}");
            controllers.push((path, Arc::new(Mutex::new(controller))));
        }
        let monitor_health = (0..controllers.len())
            .map(|_| StdMutex::new(Instant::now()))
            .collect();
        Ok(Self {
            controllers,
            monitor_health: Arc::new(monitor_health),
            sleep_monitor_healthy: Arc::new(AtomicBool::new(true)),
        })
    }

    pub fn is_empty(&self) -> bool {
        self.controllers.is_empty()
    }

    pub async fn start(&self) -> Result<(), ManagerError> {
        let mut errors = Vec::new();
        for (path, controller) in &self.controllers {
            if let Err(error) = controller.lock().await.start() {
                errors.push(format!("failed to start fan {path}: {error}"));
            }
        }
        if errors.is_empty() {
            return Ok(());
        }
        for (_, controller) in &self.controllers {
            let mut controller = controller.lock().await;
            if controller.snapshot().fault.is_none() {
                controller.fail_to_automatic("another configured fan failed to start");
            }
        }
        Err(ManagerError(errors.join("; ")))
    }

    pub async fn automatic_fallback_verified(&self) -> bool {
        for (_, controller) in &self.controllers {
            if !controller.lock().await.snapshot().automatic_verified {
                return false;
            }
        }
        true
    }

    pub async fn register(&self, connection: &Connection) -> Result<(), ManagerError> {
        let mut paths = Vec::new();
        for (path, controller) in &self.controllers {
            connection
                .object_server()
                .at(path.clone(), FanDevice::new(Arc::clone(controller)))
                .await
                .map_err(|error| ManagerError(error.to_string()))?;
            paths.push(path.clone());
        }
        connection
            .object_server()
            .at(FAN_PATH, FanBus::new(paths))
            .await
            .map_err(|error| ManagerError(error.to_string()))?;
        Ok(())
    }

    pub fn spawn_monitor(&self, connection: Connection) -> Vec<JoinHandle<()>> {
        self.controllers
            .iter()
            .enumerate()
            .map(|(index, (path, controller))| {
                let path = path.clone();
                let controller = Arc::clone(controller);
                let connection = connection.clone();
                let health = Arc::clone(&self.monitor_health);
                tokio::spawn(async move {
                    let mut interval = tokio::time::interval(Duration::from_secs(1));
                    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                    interval.tick().await;
                    loop {
                        interval.tick().await;
                        let (result, automatic_verified) = {
                            let mut controller = controller.lock().await;
                            let result = controller.tick();
                            let automatic_verified = controller.snapshot().automatic_verified;
                            (result, automatic_verified)
                        };
                        if let Err(error) = &result {
                            log::error!("Fan controller fault at {path}: {error}");
                        }
                        if controller_iteration_is_safe(&result, automatic_verified) {
                            if let Ok(mut last_tick) = health[index].lock() {
                                *last_tick = Instant::now();
                            }
                        } else {
                            log::error!(
                                "Fan controller at {path} could not verify automatic fallback; withholding watchdog progress"
                            );
                        }
                        match connection
                            .object_server()
                            .interface::<_, FanDevice>(path.as_str())
                            .await
                        {
                            Ok(interface) => {
                                let device = interface.get().await;
                                if let Err(error) =
                                    device.emit_all_changes(interface.signal_emitter()).await
                                {
                                    log::error!(
                                        "Unable to emit fan property changes at {path}: {error}"
                                    );
                                }
                            }
                            Err(error) => {
                                log::error!("Unable to access fan D-Bus object {path}: {error}");
                            }
                        }
                    }
                })
            })
            .collect()
    }

    /// The systemd watchdog heartbeat is emitted only while every fan loop is
    /// making progress. A blocked hwmon read/write therefore causes systemd to
    /// terminate the service and run the out-of-process automatic-mode helper.
    pub fn watchdog_healthy(&self, maximum_age: Duration) -> bool {
        self.sleep_monitor_healthy.load(Ordering::Acquire)
            && self.monitor_health.iter().all(|last_tick| {
                last_tick
                    .lock()
                    .map(|last_tick| last_tick.elapsed() <= maximum_age)
                    .unwrap_or(false)
            })
    }

    pub async fn spawn_sleep_monitor(
        &self,
        connection: Connection,
    ) -> Result<JoinHandle<()>, ManagerError> {
        let manager = self.clone();
        let (ready_tx, ready_rx) = oneshot::channel();
        let handle = tokio::spawn(async move {
            let failure = match AssertUnwindSafe(manager.monitor_sleep(connection, ready_tx))
                .catch_unwind()
                .await
            {
                Ok(Ok(())) => "ended unexpectedly".to_string(),
                Ok(Err(error)) => error.to_string(),
                Err(_) => "panicked".to_string(),
            };
            log::error!("Fan sleep monitor stopped: {failure}");
            manager
                .sleep_monitor_healthy
                .store(false, Ordering::Release);
            manager.fail_all_to_automatic("sleep monitor failure").await;
        });
        match ready_rx.await {
            Ok(Ok(())) => Ok(handle),
            Ok(Err(error)) => Err(ManagerError(error)),
            Err(_) => Err(ManagerError(
                "fan sleep monitor stopped before initialization".into(),
            )),
        }
    }

    pub async fn shutdown(&self) -> Result<(), ManagerError> {
        let mut errors = Vec::new();
        for (_, controller) in &self.controllers {
            if let Err(error) = controller.lock().await.shutdown() {
                errors.push(error.to_string());
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(ManagerError(format!(
                "failed to restore all fans to automatic: {}",
                errors.join("; ")
            )))
        }
    }

    pub fn restore_default() -> Result<(), ManagerError> {
        let platform_dir = platform_directory();
        let sysfs_root = configured_path(SYSFS_ROOT_ENV, DEFAULT_SYSFS_ROOT);
        Self::restore_all(&platform_dir, &sysfs_root)?;
        if std::env::var("SERVICE_RESULT")
            .map(|result| result != "success")
            .unwrap_or(false)
        {
            Self::persist_automatic(
                &platform_dir,
                &sysfs_root,
                configured_path(STATE_PATH_ENV, DEFAULT_STATE_PATH),
            )?;
        }
        Ok(())
    }

    pub fn restore_all(platform_dir: &Path, sysfs_root: &Path) -> Result<(), ManagerError> {
        let devices =
            load_databases(platform_dir).map_err(|error| ManagerError(error.to_string()))?;
        let dmi = DmiInfo::read(sysfs_root);
        let Some(device) =
            select_device(&devices, &dmi).map_err(|error| ManagerError(error.to_string()))?
        else {
            return Ok(());
        };
        let errors = restore_configurations(sysfs_root, &device.fans);
        if errors.is_empty() {
            Ok(())
        } else {
            Err(ManagerError(errors.join("; ")))
        }
    }

    fn persist_automatic(
        platform_dir: &Path,
        sysfs_root: &Path,
        state_path: PathBuf,
    ) -> Result<(), ManagerError> {
        let devices =
            load_databases(platform_dir).map_err(|error| ManagerError(error.to_string()))?;
        let dmi = DmiInfo::read(sysfs_root);
        let Some(device) =
            select_device(&devices, &dmi).map_err(|error| ManagerError(error.to_string()))?
        else {
            return Ok(());
        };
        let store =
            StateStore::open(state_path).map_err(|error| ManagerError(error.to_string()))?;
        for config in device.fans {
            let key = format!("{}/{}", device.id, config.id);
            let mut state = store.get(&key);
            state.mode = SavedMode::Automatic;
            state.preset = None;
            store
                .set(&key, state)
                .map_err(|error| ManagerError(error.to_string()))?;
        }
        log::warn!("Persisted firmware automatic fan mode after failed service run");
        Ok(())
    }

    async fn monitor_sleep(
        &self,
        connection: Connection,
        ready: oneshot::Sender<Result<(), String>>,
    ) -> Result<(), ManagerError> {
        let proxy = match LoginManagerProxy::new(&connection).await {
            Ok(proxy) => proxy,
            Err(error) => {
                let error = ManagerError(error.to_string());
                let _ = ready.send(Err(error.to_string()));
                return Err(error);
            }
        };
        let mut owner_changes = match proxy.inner().receive_owner_changed().await {
            Ok(owner_changes) => owner_changes,
            Err(error) => {
                let _ = ready.send(Err(error.to_string()));
                return Err(ManagerError(error.to_string()));
            }
        };
        let mut signals = match proxy.receive_prepare_for_sleep().await {
            Ok(signals) => signals,
            Err(error) => {
                let error = ManagerError(error.to_string());
                let _ = ready.send(Err(error.to_string()));
                return Err(error);
            }
        };
        let first_inhibitor = match acquire_inhibitor(&proxy).await {
            Ok(inhibitor) => inhibitor,
            Err(error) => {
                let _ = ready.send(Err(error.to_string()));
                return Err(error);
            }
        };
        let mut inhibitor = Some(first_inhibitor);
        let _ = ready.send(Ok(()));
        loop {
            tokio::select! {
                signal = signals.next() => {
                    let signal = signal.ok_or_else(|| {
                        ManagerError("logind PrepareForSleep signal stream ended".into())
                    })?;
                    let arguments = signal
                        .args()
                        .map_err(|error| ManagerError(error.to_string()))?;
                    if *arguments.sleeping() {
                        let mut errors = Vec::new();
                        for (_, controller) in &self.controllers {
                            if let Err(error) = controller.lock().await.prepare_for_sleep() {
                                errors.push(error.to_string());
                            }
                        }
                        if errors.is_empty() {
                            log::info!("All fans verified in firmware automatic mode before sleep");
                            inhibitor.take();
                        } else {
                            let reason = format!(
                                "unable to verify firmware automatic mode before sleep: {}",
                                errors.join("; ")
                            );
                            log::error!("{reason}; retaining the delay inhibitor for watchdog recovery");
                            self.sleep_monitor_healthy.store(false, Ordering::Release);
                            self.fail_all_to_automatic(&reason).await;
                            std::future::pending::<()>().await;
                            drop(inhibitor);
                            unreachable!();
                        }
                    } else {
                        inhibitor = Some(acquire_inhibitor(&proxy).await?);
                        for (_, controller) in &self.controllers {
                            let mut controller = controller.lock().await;
                            if let Err(error) = controller.resume() {
                                let automatic_verified = controller.snapshot().automatic_verified;
                                log::error!("Unable to restore fan after resume: {error}");
                                if !automatic_verified {
                                    return Err(ManagerError(format!(
                                        "resume failed without a verified automatic fallback: {error}"
                                    )));
                                }
                            }
                        }
                    }
                }
                owner = owner_changes.next() => {
                    return match owner {
                        Some(Some(owner)) => Err(ManagerError(format!(
                            "logind D-Bus owner changed to {owner}"
                        ))),
                        Some(None) => Err(ManagerError("logind D-Bus owner disappeared".into())),
                        None => Err(ManagerError("logind D-Bus owner monitor ended".into())),
                    };
                }
            }
        }
    }

    async fn fail_all_to_automatic(&self, reason: &str) {
        log::error!("Returning all fans to automatic because of {reason}");
        for (_, controller) in &self.controllers {
            let mut controller = controller.lock().await;
            let error = controller.fail_to_automatic(reason);
            log::error!(
                "Fan {} entered automatic fallback: {error}",
                controller.state_key()
            );
        }
    }
}

fn controller_iteration_is_safe(
    result: &Result<(), ControllerError>,
    automatic_verified: bool,
) -> bool {
    result.is_ok() || automatic_verified
}

fn restore_configurations(sysfs_root: &Path, fans: &[FanConfig]) -> Vec<String> {
    fans.iter()
        .filter_map(|config| {
            restore_automatic_for_config(sysfs_root, config)
                .err()
                .map(|error| format!("fan {}: {error}", config.id))
        })
        .collect()
}

fn initialization_failure(reason: String, sysfs_root: &Path, fans: &[FanConfig]) -> ManagerError {
    let recovery_errors = restore_configurations(sysfs_root, fans);
    if recovery_errors.is_empty() {
        ManagerError(format!(
            "{reason}; all configured fans were restored to firmware automatic"
        ))
    } else {
        ManagerError(format!(
            "{reason}; firmware automatic recovery also failed: {}",
            recovery_errors.join("; ")
        ))
    }
}

async fn acquire_inhibitor(proxy: &LoginManagerProxy<'_>) -> Result<OwnedFd, ManagerError> {
    proxy
        .inhibit(
            "sleep",
            "PowerStation",
            "Restore controlled fans to firmware automatic mode",
            "delay",
        )
        .await
        .map_err(|error| ManagerError(format!("unable to acquire sleep delay inhibitor: {error}")))
}

fn platform_directory() -> PathBuf {
    configured_path(PLATFORM_DIR_ENV, DEFAULT_PLATFORM_DIR)
}

fn configured_path(variable: &str, default: &str) -> PathBuf {
    std::env::var_os(variable)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(default))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::AtomicU64;
    use tokio::net::UnixStream;
    use zbus::zvariant::OwnedObjectPath;

    static NEXT_ID: AtomicU64 = AtomicU64::new(0);

    struct TempTree(PathBuf);

    impl TempTree {
        fn new() -> Self {
            let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "powerstation-fan-manager-{}-{id}",
                std::process::id()
            ));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempTree {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn write(path: impl AsRef<Path>, value: &str) {
        let path = path.as_ref();
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, value).unwrap();
    }

    fn ayaneo_fixture(tree: &TempTree) {
        let root = tree.path();
        write(root.join("class/dmi/id/sys_vendor"), "AYANEO\n");
        write(root.join("class/dmi/id/product_name"), "AYANEO 3\n");

        let fan = root.join("class/hwmon/hwmon2");
        write(fan.join("name"), "ayaneo_ec\n");
        write(
            fan.join("device/uevent"),
            "DRIVER=ayaneo-ec\nDEVPATH=/devices/platform/ayaneo-ec\n",
        );
        write(fan.join("pwm1"), "128\n");
        write(fan.join("pwm1_enable"), "2\n");
        write(fan.join("fan1_input"), "1200\n");

        let sensor = root.join("class/hwmon/hwmon9");
        write(sensor.join("name"), "k10temp\n");
        write(sensor.join("temp1_label"), "Tctl\n");
        write(sensor.join("temp1_input"), "50000\n");
    }

    #[test]
    fn watchdog_requires_recent_progress_from_every_controller() {
        let manager = FanManager {
            controllers: Vec::new(),
            monitor_health: Arc::new(vec![
                StdMutex::new(Instant::now()),
                StdMutex::new(Instant::now() - Duration::from_secs(10)),
            ]),
            sleep_monitor_healthy: Arc::new(AtomicBool::new(true)),
        };
        assert!(!manager.watchdog_healthy(Duration::from_secs(3)));
        *manager.monitor_health[1].lock().unwrap() = Instant::now();
        assert!(manager.watchdog_healthy(Duration::from_secs(3)));
        manager
            .sleep_monitor_healthy
            .store(false, Ordering::Release);
        assert!(!manager.watchdog_healthy(Duration::from_secs(3)));
        assert!(FanManager::empty().watchdog_healthy(Duration::from_secs(3)));
    }

    #[test]
    fn controller_fault_requires_verified_automatic_fallback() {
        let failed = Err(ControllerError("test failure".into()));
        assert!(!controller_iteration_is_safe(&failed, false));
        assert!(controller_iteration_is_safe(&failed, true));
        assert!(controller_iteration_is_safe(&Ok(()), false));
    }

    #[test]
    fn recovery_restore_does_not_depend_on_telemetry_or_curve_discovery() {
        let tree = TempTree::new();
        ayaneo_fixture(&tree);
        let fan = tree.path().join("class/hwmon/hwmon2");
        write(fan.join("pwm1_enable"), "1\n");
        fs::remove_file(tree.path().join("class/hwmon/hwmon9/temp1_input")).unwrap();
        write(fan.join("pwm1_auto_point1_temp"), "50000\n");
        let platform =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("rootfs/usr/share/powerstation/platform");

        FanManager::restore_all(&platform, tree.path()).unwrap();
        assert_eq!(fs::read_to_string(fan.join("pwm1_enable")).unwrap(), "2");
    }

    #[test]
    fn state_initialization_failure_restores_firmware_automatic() {
        let tree = TempTree::new();
        ayaneo_fixture(&tree);
        let fan = tree.path().join("class/hwmon/hwmon2");
        write(fan.join("pwm1_enable"), "1\n");
        let state = tree.path().join("state/fan-state.yaml");
        write(&state, "not: [valid\n");
        let platform =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("rootfs/usr/share/powerstation/platform");

        assert!(FanManager::discover(
            platform,
            tree.path().to_path_buf(),
            state,
            tree.path().join("missing-legacy.json"),
        )
        .is_err());
        assert_eq!(fs::read_to_string(fan.join("pwm1_enable")).unwrap(), "2");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn dbus_contract_roundtrips_atomic_configuration() {
        let tree = TempTree::new();
        ayaneo_fixture(&tree);
        let platform =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("rootfs/usr/share/powerstation/platform");
        let state = tree.path().join("state/fan-state.yaml");
        let manager = FanManager::discover(
            platform,
            tree.path().to_path_buf(),
            state.clone(),
            tree.path().join("missing-legacy.json"),
        )
        .unwrap();
        manager.start().await.unwrap();

        let guid = zbus::Guid::generate();
        let (server_stream, client_stream) = UnixStream::pair().unwrap();
        let server = zbus::connection::Builder::unix_stream(server_stream)
            .server(guid)
            .unwrap()
            .p2p()
            .build();
        let client = zbus::connection::Builder::unix_stream(client_stream)
            .p2p()
            .build();
        let (server, client) = tokio::try_join!(server, client).unwrap();
        manager.register(&server).await.unwrap();

        let collection = zbus::Proxy::new(
            &client,
            "org.shadowblip.PowerStation",
            FAN_PATH,
            "org.shadowblip.Fan",
        )
        .await
        .unwrap();
        let fans: Vec<OwnedObjectPath> = collection.call("EnumerateFans", &()).await.unwrap();
        assert_eq!(fans.len(), 1);

        let device: zbus::Proxy<'_> = zbus::proxy::Builder::new(&client)
            .destination("org.shadowblip.PowerStation")
            .unwrap()
            .path(fans[0].as_str())
            .unwrap()
            .interface("org.shadowblip.Fan.Device")
            .unwrap()
            .cache_properties(zbus::proxy::CacheProperties::No)
            .build()
            .await
            .unwrap();
        assert_eq!(device.get_property::<u32>("ApiVersion").await.unwrap(), 1);
        assert_eq!(device.get_property::<u32>("PwmMax").await.unwrap(), 255);
        assert_eq!(device.get_property::<u32>("StallRpm").await.unwrap(), 300);
        let _: () = device.call("ApplyPreset", &("Quiet",)).await.unwrap();
        assert_eq!(
            device.get_property::<String>("Mode").await.unwrap(),
            "preset"
        );
        assert_eq!(
            device.get_property::<String>("ActivePreset").await.unwrap(),
            "Quiet"
        );
        assert_eq!(
            fs::read_to_string(tree.path().join("class/hwmon/hwmon2/pwm1_enable")).unwrap(),
            "1"
        );
        assert_eq!(
            fs::read_to_string(tree.path().join("class/hwmon/hwmon2/pwm1")).unwrap(),
            "26"
        );

        let _: () = device.call("SetManual", &(0u8,)).await.unwrap();
        assert_eq!(
            fs::read_to_string(tree.path().join("class/hwmon/hwmon2/pwm1")).unwrap(),
            "26"
        );
        write(
            tree.path().join("class/hwmon/hwmon9/temp1_input"),
            "40000\n",
        );
        let _: () = device.call("SetManual", &(0u8,)).await.unwrap();
        assert_eq!(
            fs::read_to_string(tree.path().join("class/hwmon/hwmon2/pwm1")).unwrap(),
            "0"
        );

        let _: () = device.call("SetAutomatic", &()).await.unwrap();
        assert!(device
            .get_property::<bool>("AutomaticVerified")
            .await
            .unwrap());
        assert_eq!(
            fs::read_to_string(tree.path().join("class/hwmon/hwmon2/pwm1_enable")).unwrap(),
            "2"
        );
        manager.shutdown().await.unwrap();
        let saved = fs::read_to_string(state).unwrap();
        assert!(saved.contains("mode: automatic"));
    }
}
