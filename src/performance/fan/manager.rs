use super::config::{load_databases, read_device_tree, select_device, DmiInfo, FanConfig};
use super::controller::{restore_automatic_for_config, ControllerError, FanController};
use super::dbus::{FanBus, FanDevice, FanSensor};
use super::profile::{load_profile, Profile};
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
const DEVICE_TREE_PATH_ENV: &str = "POWERSTATION_FAN_DEVICE_TREE_PATH";
const DEFAULT_DEVICE_TREE_PATH: &str = "/proc/device-tree/compatible";
const DEFAULT_SYSFS_ROOT: &str = "/sys";
const PLATFORM_DIR_ENV: &str = "POWERSTATION_FAN_PLATFORM_DIR";
const SYSFS_ROOT_ENV: &str = "POWERSTATION_FAN_SYSFS_ROOT";

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

    #[zbus(property, name = "InhibitDelayMaxUSec")]
    fn inhibit_delay_max_usec(&self) -> zbus::Result<u64>;

    #[zbus(signal)]
    fn prepare_for_sleep(&self, sleeping: bool) -> zbus::Result<()>;
}

#[derive(Clone)]
struct LoadedProfile {
    profile: Profile,
    path: String,
}

#[derive(Clone)]
pub struct FanManager {
    profile: Arc<Mutex<Option<LoadedProfile>>>,
    controllers: Vec<(String, Arc<Mutex<FanController>>)>,
    monitor_health: Arc<Vec<StdMutex<Instant>>>,
    sleep_monitor_healthy: Arc<AtomicBool>,
    custom_control_allowed: Arc<AtomicBool>,
}

impl FanManager {
    #[cfg(test)]
    fn empty() -> Self {
        Self {
            profile: Arc::new(Mutex::new(None)),
            controllers: Vec::new(),
            monitor_health: Arc::new(Vec::new()),
            sleep_monitor_healthy: Arc::new(AtomicBool::new(true)),
            custom_control_allowed: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn discover_default() -> Result<Self, ManagerError> {
        Self::discover(
            platform_directory(),
            configured_path(SYSFS_ROOT_ENV, DEFAULT_SYSFS_ROOT),
            configured_path(DEVICE_TREE_PATH_ENV, DEFAULT_DEVICE_TREE_PATH),
        )
    }

    pub fn discover(
        platform_dir: PathBuf,
        sysfs_root: PathBuf,
        device_tree_path: PathBuf,
    ) -> Result<Self, ManagerError> {
        let devices =
            load_databases(&platform_dir).map_err(|error| ManagerError(error.to_string()))?;
        let dmi = DmiInfo::read(&sysfs_root);
        let Some(device) = select_device(
            &devices,
            &dmi,
            &read_device_tree(&device_tree_path)
                .map_err(|error| ManagerError(error.to_string()))?,
        )
        .map_err(|error| ManagerError(error.to_string()))?
        else {
            log::info!("No fan platform configuration matches this system");
            return Ok(Self {
                profile: Arc::new(Mutex::new(None)),
                controllers: Vec::new(),
                monitor_health: Arc::new(Vec::new()),
                sleep_monitor_healthy: Arc::new(AtomicBool::new(true)),
                custom_control_allowed: Arc::new(AtomicBool::new(false)),
            });
        };
        log::info!(
            "Matched fan platform configuration {} ({})",
            device.id,
            device.name
        );
        let device_id = device.id.clone();
        let fans = device.fans;
        let mut controllers = Vec::new();
        let custom_control_allowed = Arc::new(AtomicBool::new(false));
        for (index, config) in fans.iter().cloned().enumerate() {
            let mut controller =
                match FanController::new(device_id.clone(), config, sysfs_root.clone()) {
                    Ok(controller) => controller,
                    Err(error) => {
                        return Err(initialization_failure(
                            error.to_string(),
                            &sysfs_root,
                            &fans,
                        ))
                    }
                };
            controller.set_control_gate(Arc::clone(&custom_control_allowed));
            let path = format!("{FAN_PATH}/Fan{index}");
            controllers.push((path, Arc::new(Mutex::new(controller))));
        }
        let monitor_health = (0..controllers.len())
            .map(|_| StdMutex::new(Instant::now()))
            .collect();
        Ok(Self {
            profile: Arc::new(Mutex::new(None)),
            controllers,
            monitor_health: Arc::new(monitor_health),
            sleep_monitor_healthy: Arc::new(AtomicBool::new(false)),
            custom_control_allowed,
        })
    }

    pub fn fan_paths(&self) -> Vec<String> {
        self.controllers
            .iter()
            .map(|(path, _)| path.clone())
            .collect()
    }

    pub async fn sensor_paths(&self) -> Vec<String> {
        let mut paths = Vec::new();
        for (path, controller) in &self.controllers {
            for index in 0..controller.lock().await.config.temperature_sources.len() {
                paths.push(format!("{path}/Sensor{index}"));
            }
        }
        paths
    }

    pub async fn current_profile(&self) -> (String, String) {
        let loaded = self.profile.lock().await;
        let Some(loaded) = loaded.as_ref() else {
            return (String::new(), String::new());
        };
        for (_, controller) in &self.controllers {
            let controller = controller.lock().await;
            if controller.snapshot().fault.is_some()
                || !loaded.profile.fans.contains(controller.selection())
            {
                return (String::new(), String::new());
            }
        }
        (loaded.profile.name.clone(), loaded.path.clone())
    }

    pub async fn load_profile_fd(&self, fd: OwnedFd) -> Result<(), ManagerError> {
        let (profile, path) = tokio::task::spawn_blocking(move || {
            let file = std::fs::File::from(std::os::fd::OwnedFd::from(fd));
            load_profile(&file)
        })
        .await
        .map_err(|error| ManagerError(error.to_string()))?
        .map_err(|error| ManagerError(error.to_string()))?;
        self.apply_profile(profile, path).await
    }

    pub async fn load_profile_json(&self, json: &str) -> Result<(), ManagerError> {
        let profile = Profile::from_json(json).map_err(|error| ManagerError(error.to_string()))?;
        self.apply_profile(profile, String::new()).await
    }

    async fn apply_profile(&self, profile: Profile, path: String) -> Result<(), ManagerError> {
        // Serialize complete transactions and metadata. Always take controller
        // locks in platform order, then validate every fan before any write.
        let mut loaded = self.profile.lock().await;
        let mut controllers = Vec::new();
        for (_, controller) in &self.controllers {
            controllers.push(controller.lock().await);
        }
        if profile.fans.len() != controllers.len() || controllers.is_empty() {
            return Err(ManagerError(
                "profile must specify every enumerated fan exactly once".into(),
            ));
        }
        for controller in &controllers {
            let entry = profile
                .fans
                .iter()
                .find(|entry| entry.fan == controller.id())
                .ok_or_else(|| {
                    ManagerError(format!("missing fan {} or unknown fan ID", controller.id()))
                })?;
            controller.validate_selection(entry)?;
        }
        for index in 0..controllers.len() {
            let entry = profile
                .fans
                .iter()
                .find(|entry| entry.fan == controllers[index].id())
                .unwrap()
                .clone();
            if let Err(error) = controllers[index].configure(entry) {
                let mut errors = vec![error.to_string()];
                for controller in &mut controllers {
                    let fallback = controller.fail_to_automatic("profile application failed");
                    if !controller.snapshot().automatic_verified {
                        errors.push(fallback.to_string());
                    }
                }
                *loaded = None;
                return Err(ManagerError(errors.join("; ")));
            }
        }
        *loaded = Some(LoadedProfile { profile, path });
        Ok(())
    }

    pub async fn emit_changes(&self, connection: &Connection) {
        for (path, _) in &self.controllers {
            if let Ok(interface) = connection
                .object_server()
                .interface::<_, FanDevice>(path.as_str())
                .await
            {
                if let Err(error) = interface
                    .get()
                    .await
                    .emit_all_changes(interface.signal_emitter())
                    .await
                {
                    log::error!("Unable to emit fan changes: {error}");
                }
            }
            self.emit_sensor_changes(connection, path).await;
        }
        self.emit_profile_changes(connection).await;
    }

    async fn emit_profile_changes(&self, connection: &Connection) {
        if let Ok(interface) = connection
            .object_server()
            .interface::<_, FanBus>(FAN_PATH)
            .await
        {
            if let Err(error) = interface
                .get()
                .await
                .emit_changes(interface.signal_emitter())
                .await
            {
                log::error!("Unable to emit profile changes: {error}");
            }
        }
    }

    async fn emit_sensor_changes(&self, connection: &Connection, fan_path: &str) {
        let Some((_, controller)) = self.controllers.iter().find(|(path, _)| path == fan_path)
        else {
            return;
        };
        let count = controller.lock().await.config.temperature_sources.len();
        for index in 0..count {
            if let Ok(interface) = connection
                .object_server()
                .interface::<_, FanSensor>(format!("{fan_path}/Sensor{index}"))
                .await
            {
                if let Err(error) = interface
                    .get()
                    .await
                    .emit_changes(interface.signal_emitter())
                    .await
                {
                    log::error!("Unable to emit sensor changes: {error}");
                }
            }
        }
    }

    pub fn is_empty(&self) -> bool {
        self.controllers.is_empty()
    }

    pub async fn start(&self) -> Result<(), ManagerError> {
        let protected = self.sleep_monitor_healthy.load(Ordering::Acquire);
        self.custom_control_allowed.store(false, Ordering::Release);
        if !protected {
            return Err(ManagerError(
                "fan sleep protection is unavailable at startup".into(),
            ));
        }
        let mut errors = Vec::new();
        for (path, controller) in &self.controllers {
            if let Err(error) = controller.lock().await.start() {
                errors.push(format!("failed to start fan {path}: {error}"));
            }
        }
        if errors.is_empty() {
            self.custom_control_allowed.store(
                self.sleep_monitor_healthy.load(Ordering::Acquire),
                Ordering::Release,
            );
            return Ok(());
        }
        for (_, controller) in &self.controllers {
            let mut controller = controller.lock().await;
            if controller.snapshot().fault.is_none() {
                controller.fail_to_automatic("another configured fan failed to start");
            }
        }
        self.custom_control_allowed.store(
            self.sleep_monitor_healthy.load(Ordering::Acquire)
                && self.automatic_fallback_verified().await,
            Ordering::Release,
        );
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
        for (path, controller) in &self.controllers {
            connection
                .object_server()
                .at(path.clone(), FanDevice::new(Arc::clone(controller)))
                .await
                .map_err(|error| ManagerError(error.to_string()))?;
            let sensors = controller.lock().await.config.temperature_sources.clone();
            for (index, sensor) in sensors.iter().enumerate() {
                connection
                    .object_server()
                    .at(
                        format!("{path}/Sensor{index}"),
                        FanSensor::new(
                            Arc::clone(controller),
                            sensor.id.clone(),
                            sensor.name.clone(),
                        ),
                    )
                    .await
                    .map_err(|error| ManagerError(error.to_string()))?;
            }
        }
        connection
            .object_server()
            .at(FAN_PATH, FanBus::new(self.clone()))
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
                let manager = self.clone();
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
                        manager.emit_profile_changes(&connection).await;
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
                                drop(device);
                                manager.emit_sensor_changes(&connection, &path).await;
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
                .custom_control_allowed
                .store(false, Ordering::Release);
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
        self.custom_control_allowed.store(false, Ordering::Release);
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
        Self::restore_all(
            &platform_dir,
            &sysfs_root,
            &configured_path(DEVICE_TREE_PATH_ENV, DEFAULT_DEVICE_TREE_PATH),
        )?;
        Ok(())
    }

    pub fn restore_all(
        platform_dir: &Path,
        sysfs_root: &Path,
        device_tree_path: &Path,
    ) -> Result<(), ManagerError> {
        let devices =
            load_databases(platform_dir).map_err(|error| ManagerError(error.to_string()))?;
        let dmi = DmiInfo::read(sysfs_root);
        let Some(device) = select_device(
            &devices,
            &dmi,
            &read_device_tree(device_tree_path).map_err(|error| ManagerError(error.to_string()))?,
        )
        .map_err(|error| ManagerError(error.to_string()))?
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

    async fn monitor_sleep(
        &self,
        connection: Connection,
        ready: oneshot::Sender<Result<(), String>>,
    ) -> Result<(), ManagerError> {
        // logind's GetAll can fail on unrelated properties such as bootloader
        // entries. Read only the inhibitor budget; sleep events are signals.
        let proxy = match LoginManagerProxy::builder(&connection)
            .cache_properties(zbus::proxy::CacheProperties::No)
            .build()
            .await
        {
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
        let delay = match proxy.inhibit_delay_max_usec().await {
            Ok(microseconds) => Duration::from_micros(microseconds),
            Err(error) => {
                let _ = ready.send(Err(error.to_string()));
                return Err(ManagerError(error.to_string()));
            }
        };
        self.sleep_monitor_healthy.store(true, Ordering::Release);
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
                        self.custom_control_allowed.store(false, Ordering::Release);
                        if let Err(error) = self.prepare_for_sleep(delay).await {
                            self.sleep_monitor_healthy.store(false, Ordering::Release);
                            return Err(error);
                        } else {
                            log::info!("All fans verified in firmware automatic mode before sleep");
                            inhibitor.take();
                        }
                        self.emit_changes(&connection).await;
                    } else {
                        inhibitor = Some(acquire_inhibitor(&proxy).await?);
                        self.resume_controllers().await?;
                        self.custom_control_allowed.store(true, Ordering::Release);
                        self.emit_changes(&connection).await;
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

    async fn resume_controllers(&self) -> Result<(), ManagerError> {
        let mut failed = false;
        for (_, controller) in &self.controllers {
            if let Err(error) = controller.lock().await.resume() {
                failed = true;
                log::error!("Unable to restore fan after resume: {error}");
            }
        }
        if failed {
            self.fail_all_to_automatic("profile could not be restored after resume")
                .await;
            if !self.automatic_fallback_verified().await {
                return Err(ManagerError(
                    "resume failed without a verified automatic fallback".into(),
                ));
            }
        }
        Ok(())
    }

    async fn fail_all_to_automatic(&self, reason: &str) {
        self.custom_control_allowed.store(false, Ordering::Release);
        log::error!("Returning all fans to automatic because of {reason}");
        for (_, controller) in &self.controllers {
            let mut controller = controller.lock().await;
            let error = controller.fail_to_automatic(reason);
            log::error!(
                "Fan {} entered automatic fallback: {error}",
                controller.id()
            );
        }
    }

    async fn prepare_for_sleep(&self, delay: Duration) -> Result<(), ManagerError> {
        self.custom_control_allowed.store(false, Ordering::Release);
        let manager = self.clone();
        let deadline = Instant::now() + delay;
        // Sysfs calls can block. Keep them off the runtime so the delay budget
        // and watchdog remain observable; a timed-out worker can only restore
        // automatic mode, never resume custom control.
        let handoff = tokio::task::spawn_blocking(move || loop {
            let mut errors = Vec::new();
            for (_, controller) in &manager.controllers {
                if let Err(error) = controller.blocking_lock().prepare_for_sleep() {
                    errors.push(error.to_string());
                }
            }
            if errors.is_empty() {
                return Ok(());
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(ManagerError(errors.join("; ")));
            }
            std::thread::sleep(remaining.min(Duration::from_millis(100)));
        });
        match tokio::time::timeout(delay, handoff).await {
            Ok(Ok(result)) => result,
            Ok(Err(error)) => Err(ManagerError(format!("fan suspend handoff task failed: {error}"))),
            Err(_) => Err(ManagerError(
                "fan automatic handoff exceeded logind's delay budget; suspend may proceed without verified recovery".into(),
            )),
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
    if let Some(path) = std::env::var_os(PLATFORM_DIR_ENV) {
        return path.into();
    }
    let base_dirs = xdg::BaseDirectories::with_prefix("powerstation");
    for directory in base_dirs.get_data_dirs() {
        let platform = directory.join("platform");
        if platform.is_dir() {
            return platform;
        }
    }
    PathBuf::from(DEFAULT_PLATFORM_DIR)
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
            profile: Arc::new(Mutex::new(None)),
            controllers: Vec::new(),
            monitor_health: Arc::new(vec![
                StdMutex::new(Instant::now()),
                StdMutex::new(Instant::now() - Duration::from_secs(10)),
            ]),
            sleep_monitor_healthy: Arc::new(AtomicBool::new(true)),
            custom_control_allowed: Arc::new(AtomicBool::new(false)),
        };
        // This fixture tests heartbeat age independently of logind readiness.
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

        FanManager::restore_all(&platform, tree.path(), &tree.path().join("compatible")).unwrap();
        assert_eq!(fs::read_to_string(fan.join("pwm1_enable")).unwrap(), "2");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn queued_request_is_rejected_before_suspend_handoff() {
        let tree = TempTree::new();
        ayaneo_fixture(&tree);
        let platform =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("rootfs/usr/share/powerstation/platform");
        let manager =
            FanManager::discover(platform, tree.path().into(), tree.path().join("compatible"))
                .unwrap();
        manager.sleep_monitor_healthy.store(true, Ordering::Release);
        manager.start().await.unwrap();
        let controller = Arc::clone(&manager.controllers[0].1);
        let mut lock = controller.lock().await;
        lock.configure(manual(20)).unwrap();
        let queued_controller = Arc::clone(&controller);
        let queued =
            tokio::spawn(async move { queued_controller.lock().await.configure(manual(50)) });
        tokio::task::yield_now().await;
        let sleeping_manager = manager.clone();
        let sleeping = tokio::spawn(async move {
            sleeping_manager
                .prepare_for_sleep(Duration::from_secs(1))
                .await
        });
        tokio::time::timeout(Duration::from_secs(1), async {
            while manager.custom_control_allowed.load(Ordering::Acquire) {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        drop(lock);
        assert!(queued.await.unwrap().is_err());
        sleeping.await.unwrap().unwrap();
        let snapshot = controller.lock().await.snapshot();
        assert!(snapshot.suspended);
        assert!(snapshot.automatic_verified);
        assert_eq!(controller.lock().await.selection().percent, Some(20));
        assert_eq!(
            fs::read_to_string(tree.path().join("class/hwmon/hwmon2/pwm1_enable")).unwrap(),
            "2"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn suspend_budget_expires_even_when_controller_is_busy() {
        let tree = TempTree::new();
        ayaneo_fixture(&tree);
        let platform =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("rootfs/usr/share/powerstation/platform");
        let manager =
            FanManager::discover(platform, tree.path().into(), tree.path().join("compatible"))
                .unwrap();
        manager.sleep_monitor_healthy.store(true, Ordering::Release);
        manager.start().await.unwrap();
        let controller = Arc::clone(&manager.controllers[0].1);
        let lock = controller.lock().await;
        let error = tokio::time::timeout(
            Duration::from_secs(2),
            manager.prepare_for_sleep(Duration::from_millis(20)),
        )
        .await
        .unwrap()
        .unwrap_err();
        assert!(error.to_string().contains("delay budget"));
        assert!(!manager.custom_control_allowed.load(Ordering::Acquire));
        drop(lock);
        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                if controller.lock().await.snapshot().suspended {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        assert!(controller.lock().await.configure(manual(20)).is_err());
    }

    fn manual(percent: u8) -> super::super::profile::FanProfile {
        use super::super::profile::{FanMode, FanProfile};
        FanProfile {
            mode: FanMode::Manual,
            percent: Some(percent),
            ..FanProfile::automatic("ayaneo-3/system".into())
        }
    }

    fn fixture_manager(tree: &TempTree) -> FanManager {
        ayaneo_fixture(tree);
        let platform =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("rootfs/usr/share/powerstation/platform");
        let manager =
            FanManager::discover(platform, tree.path().into(), tree.path().join("compatible"))
                .unwrap();
        manager.sleep_monitor_healthy.store(true, Ordering::Release);
        manager
    }

    fn json_profile(fans: Vec<super::super::profile::FanProfile>) -> String {
        serde_json::to_string(&Profile {
            name: "Test".into(),
            fans,
        })
        .unwrap()
    }

    #[tokio::test]
    async fn ordinary_requests_queue_in_order_without_client_ownership() {
        let tree = TempTree::new();
        let manager = fixture_manager(&tree);
        manager.start().await.unwrap();
        let lock = manager.profile.lock().await;
        let first_json = json_profile(vec![manual(20)]);
        let second_json = json_profile(vec![manual(60)]);
        let first = manager.load_profile_json(&first_json);
        let second = manager.load_profile_json(&second_json);
        tokio::pin!(first, second);
        assert!(futures_util::poll!(&mut first).is_pending());
        assert!(futures_util::poll!(&mut second).is_pending());
        drop(lock);
        let (first, second) = tokio::join!(first, second);
        first.unwrap();
        second.unwrap();
        assert_eq!(
            fs::read_to_string(tree.path().join("class/hwmon/hwmon2/pwm1")).unwrap(),
            "153"
        );
        manager.load_profile_json(&first_json).await.unwrap();
        assert_eq!(
            fs::read_to_string(tree.path().join("class/hwmon/hwmon2/pwm1")).unwrap(),
            "51"
        );
    }

    #[tokio::test]
    async fn whole_profile_validation_precedes_all_writes_and_rolls_back_all_fans() {
        let tree = TempTree::new();
        let mut manager = fixture_manager(&tree);
        // Add a second physical channel so a failed second write proves that
        // the first fan is rolled back as well.
        let fan = tree.path().join("class/hwmon/hwmon2");
        for (name, value) in [
            ("pwm2", "128"),
            ("pwm2_enable", "2"),
            ("fan2_input", "1200"),
        ] {
            write(fan.join(name), value);
        }
        let mut config = manager.controllers[0].1.lock().await.config.clone();
        config.id = "secondary".into();
        config.hwmon.channel = 2;
        let mut second = FanController::new("ayaneo-3".into(), config, tree.path().into()).unwrap();
        second.set_control_gate(Arc::clone(&manager.custom_control_allowed));
        manager
            .controllers
            .push((format!("{FAN_PATH}/Fan1"), Arc::new(Mutex::new(second))));
        manager.start().await.unwrap();
        let first = manual(20);
        let mut second = manual(30);
        second.fan = "ayaneo-3/secondary".into();
        let mut invalid = second.clone();
        invalid.percent = Some(101);
        assert!(manager
            .load_profile_json(&json_profile(vec![first.clone(), invalid]))
            .await
            .is_err());
        assert_eq!(fs::read_to_string(fan.join("pwm1_enable")).unwrap(), "2");
        assert!(manager
            .load_profile_json(&json_profile(vec![first.clone()]))
            .await
            .is_err());
        let mut unknown = second.clone();
        unknown.fan = "unknown".into();
        assert!(manager
            .load_profile_json(&json_profile(vec![first.clone(), unknown]))
            .await
            .is_err());
        manager
            .load_profile_json(&json_profile(vec![first.clone(), second.clone()]))
            .await
            .unwrap();
        manager
            .prepare_for_sleep(Duration::from_secs(1))
            .await
            .unwrap();
        write(fan.join("fan2_input"), "invalid");
        manager.resume_controllers().await.unwrap();
        for name in ["pwm1_enable", "pwm2_enable"] {
            assert_eq!(fs::read_to_string(fan.join(name)).unwrap(), "2");
        }
        assert_eq!(manager.current_profile().await.0, "");
        write(fan.join("fan2_input"), "1200");
        manager
            .custom_control_allowed
            .store(true, Ordering::Release);
        // Discovery still succeeds; opening PWM for a write fails after fan1
        // was changed. Both enable files must be restored, not only fan2.
        fs::remove_file(fan.join("pwm2")).unwrap();
        fs::create_dir(fan.join("pwm2")).unwrap();
        assert!(manager
            .load_profile_json(&json_profile(vec![first, second]))
            .await
            .is_err());
        for name in ["pwm1_enable", "pwm2_enable"] {
            assert_eq!(fs::read_to_string(fan.join(name)).unwrap(), "2");
        }
        assert_eq!(
            manager.current_profile().await,
            (String::new(), String::new())
        );
    }

    #[tokio::test]
    async fn malformed_legacy_state_is_ignored_without_writes() {
        let tree = TempTree::new();
        let state = tree.path().join("state/fan-state.yaml");
        write(&state, "not: [valid");
        let manager = fixture_manager(&tree);
        manager.start().await.unwrap();
        manager
            .load_profile_json(&json_profile(vec![manual(20)]))
            .await
            .unwrap();
        manager.shutdown().await.unwrap();
        assert_eq!(fs::read_to_string(&state).unwrap(), "not: [valid");
        let restarted = fixture_manager(&tree);
        restarted.start().await.unwrap();
        assert_eq!(
            restarted.controllers[0].1.lock().await.snapshot().mode,
            "automatic"
        );
        assert_eq!(restarted.current_profile().await.0, "");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn dbus_v2_profiles_percentages_sensors_and_fault_notifications() {
        let tree = TempTree::new();
        let manager = fixture_manager(&tree);
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
        let collection: zbus::Proxy<'_> = zbus::proxy::Builder::new(&client)
            .destination("org.shadowblip.PowerStation")
            .unwrap()
            .path(FAN_PATH)
            .unwrap()
            .interface("org.shadowblip.Fan")
            .unwrap()
            .cache_properties(zbus::proxy::CacheProperties::No)
            .build()
            .await
            .unwrap();
        assert_eq!(
            collection.get_property::<u32>("ApiVersion").await.unwrap(),
            2
        );
        let fans: Vec<OwnedObjectPath> = collection.call("EnumerateFans", &()).await.unwrap();
        let sensors: Vec<OwnedObjectPath> = collection.call("EnumerateSensors", &()).await.unwrap();
        assert_eq!(fans.len(), 1);
        assert_eq!(sensors.len(), 1);
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
        let sensor: zbus::Proxy<'_> = zbus::proxy::Builder::new(&client)
            .destination("org.shadowblip.PowerStation")
            .unwrap()
            .path(sensors[0].as_str())
            .unwrap()
            .interface("org.shadowblip.Fan.Sensor")
            .unwrap()
            .cache_properties(zbus::proxy::CacheProperties::No)
            .build()
            .await
            .unwrap();
        assert_eq!(
            device.get_property::<String>("Id").await.unwrap(),
            "ayaneo-3/system"
        );
        assert_eq!(
            sensor.get_property::<String>("Id").await.unwrap(),
            "ayaneo-3/system/cpu"
        );
        assert!(sensor.get_property::<bool>("TelemetryValid").await.unwrap());
        assert!(device.get_property::<u32>("Rpm").await.is_err());
        assert!(device.get_property::<u32>("PwmMax").await.is_err());
        let file = fs::File::open(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("rootfs/usr/powerstation/platform/fan_curves/quiet.yaml"),
        )
        .unwrap();
        let _: () = collection
            .call("LoadProfile", &(zbus::zvariant::Fd::from(&file),))
            .await
            .unwrap();
        assert_eq!(
            collection
                .get_property::<String>("CurrentProfileName")
                .await
                .unwrap(),
            "Quiet"
        );
        assert!(collection
            .get_property::<String>("CurrentProfilePath")
            .await
            .unwrap()
            .ends_with("quiet.yaml"));
        assert_eq!(
            device.get_property::<String>("Mode").await.unwrap(),
            "software_curve"
        );
        assert_eq!(device.get_property::<u8>("Percent").await.unwrap(), 10);
        let json = json_profile(vec![manual(20)]);
        let _: () = collection.call("LoadProfileJson", &(json,)).await.unwrap();
        assert_eq!(device.get_property::<u8>("Percent").await.unwrap(), 20);
        assert_eq!(
            collection
                .get_property::<String>("CurrentProfilePath")
                .await
                .unwrap(),
            ""
        );
        let properties = zbus::fdo::PropertiesProxy::builder(&client)
            .destination("org.shadowblip.PowerStation")
            .unwrap()
            .path(fans[0].as_str())
            .unwrap()
            .build()
            .await
            .unwrap();
        let mut changes = properties.receive_properties_changed().await.unwrap();
        fs::remove_file(tree.path().join("class/hwmon/hwmon9/temp1_input")).unwrap();
        let failed: zbus::Result<()> = collection
            .call("LoadProfileJson", &(json_profile(vec![manual(50)]),))
            .await;
        assert!(failed.is_err());
        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                let change = changes.next().await.unwrap();
                let args = change.args().unwrap();
                if args.interface_name() == "org.shadowblip.Fan.Device"
                    && args.changed_properties().contains_key("PercentValid")
                {
                    break;
                }
            }
        })
        .await
        .unwrap();
        assert_eq!(
            device.get_property::<String>("Mode").await.unwrap(),
            "automatic"
        );
        assert!(!device.get_property::<bool>("PercentValid").await.unwrap());
        assert!(!sensor.get_property::<bool>("TelemetryValid").await.unwrap());
        assert_eq!(
            collection
                .get_property::<String>("CurrentProfileName")
                .await
                .unwrap(),
            ""
        );
        manager
            .custom_control_allowed
            .store(false, Ordering::Release);
        let automatic = super::super::profile::FanProfile::automatic("ayaneo-3/system".into());
        let _: () = collection
            .call("LoadProfileJson", &(json_profile(vec![automatic]),))
            .await
            .unwrap();
        assert_eq!(
            fs::read_to_string(tree.path().join("class/hwmon/hwmon2/pwm1_enable")).unwrap(),
            "2"
        );
        manager.shutdown().await.unwrap();
    }
}
