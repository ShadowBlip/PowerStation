use super::controller::{FanController, FanSnapshot};
use super::manager::FanManager;
use std::sync::Arc;
use tokio::sync::Mutex;
use zbus::fdo;
use zbus::object_server::SignalEmitter;
use zbus::zvariant::{ObjectPath, OwnedFd};
use zbus_macros::interface;

pub struct FanBus {
    manager: FanManager,
}

impl FanBus {
    pub fn new(manager: FanManager) -> Self {
        Self { manager }
    }

    pub async fn emit_changes(&self, emitter: &SignalEmitter<'_>) -> zbus::Result<()> {
        self.current_profile_name_changed(emitter).await?;
        self.current_profile_path_changed(emitter).await
    }
}

#[interface(name = "org.shadowblip.Fan")]
impl FanBus {
    /// Load a YAML profile from a readable regular file descriptor. The complete profile is validated before changing any fan.
    pub async fn load_profile(
        &self,
        file: OwnedFd,
        #[zbus(connection)] connection: &zbus::Connection,
    ) -> fdo::Result<()> {
        let result = self.manager.load_profile_fd(file).await;
        self.manager.emit_changes(connection).await;
        result.map_err(|error| fdo::Error::Failed(error.to_string()))
    }

    /// Load a profile as a JSON string. Clients must handle saving and reloading the profile after service startup.
    pub async fn load_profile_json(
        &self,
        json: &str,
        #[zbus(connection)] connection: &zbus::Connection,
    ) -> fdo::Result<()> {
        let result = self.manager.load_profile_json(json).await;
        self.manager.emit_changes(connection).await;
        result.map_err(|error| fdo::Error::Failed(error.to_string()))
    }

    /// Return all discovered fans for this device.
    pub async fn enumerate_fans(&self) -> Vec<ObjectPath<'static>> {
        self.manager
            .fan_paths()
            .into_iter()
            .map(ObjectPath::from_string_unchecked)
            .collect()
    }

    /// Return all discovered temperature sensors for this device.
    pub async fn enumerate_sensors(&self) -> Vec<ObjectPath<'static>> {
        self.manager
            .sensor_paths()
            .await
            .into_iter()
            .map(ObjectPath::from_string_unchecked)
            .collect()
    }

    /// Provisional interface revision; this fan interface is not yet stable.
    #[zbus(property)]
    pub fn api_version(&self) -> u32 {
        2
    }

    /// Active profile name, including during successful suspend. Empty after startup or a controller fault.
    #[zbus(property)]
    pub async fn current_profile_name(&self) -> String {
        self.manager.current_profile().await.0
    }

    /// Informational path of the loaded file descriptor, when available. Empty for JSON profiles, startup, or a controller fault.
    #[zbus(property)]
    pub async fn current_profile_path(&self) -> String {
        self.manager.current_profile().await.1
    }
}

#[derive(Clone)]
pub struct FanDevice {
    controller: Arc<Mutex<FanController>>,
}

impl FanDevice {
    pub fn new(controller: Arc<Mutex<FanController>>) -> Self {
        Self { controller }
    }
    async fn snapshot(&self) -> FanSnapshot {
        self.controller.lock().await.snapshot()
    }
    pub async fn emit_all_changes(&self, emitter: &SignalEmitter<'_>) -> zbus::Result<()> {
        self.mode_changed(emitter).await?;
        self.percent_changed(emitter).await?;
        self.percent_valid_changed(emitter).await?;
        self.fault_changed(emitter).await
    }
}

#[interface(name = "org.shadowblip.Fan.Device")]
impl FanDevice {
    /// Stable fan ID used in profiles.
    #[zbus(property)]
    pub async fn id(&self) -> String {
        self.snapshot().await.id
    }

    /// Human-readable fan name.
    #[zbus(property)]
    pub async fn name(&self) -> String {
        self.snapshot().await.name
    }

    /// Current mode: automatic, manual, software_curve, or firmware_curve. Suspend temporarily reports automatic while retaining the live profile in memory.
    #[zbus(property)]
    pub async fn mode(&self) -> String {
        self.snapshot().await.mode
    }

    /// Modes supported by the trusted platform configuration and discovered hardware.
    #[zbus(property)]
    pub async fn available_modes(&self) -> Vec<String> {
        self.snapshot().await.available_modes
    }

    /// Applied software duty from 0 to 100 percent. Read only when PercentValid is true; firmware duty is unavailable.
    #[zbus(property)]
    pub async fn percent(&self) -> u8 {
        self.snapshot().await.effective_percent.unwrap_or(0)
    }

    /// Whether Percent is available. False in Automatic, native firmware curves, suspend, and faults.
    #[zbus(property)]
    pub async fn percent_valid(&self) -> bool {
        self.snapshot().await.effective_percent.is_some()
    }

    /// Last controller failure, or empty after a successful profile load.
    #[zbus(property)]
    pub async fn fault(&self) -> String {
        self.snapshot().await.fault.unwrap_or_default()
    }
}

pub struct FanSensor {
    controller: Arc<Mutex<FanController>>,
    local_id: String,
    name: String,
}

impl FanSensor {
    pub fn new(controller: Arc<Mutex<FanController>>, local_id: String, name: String) -> Self {
        Self {
            controller,
            local_id,
            name,
        }
    }
    async fn temperature(&self) -> Option<f64> {
        let controller = self.controller.lock().await;
        controller.sensor_temperature(&format!("{}/{}", controller.id(), self.local_id))
    }
    pub async fn emit_changes(&self, emitter: &SignalEmitter<'_>) -> zbus::Result<()> {
        self.temperature_c_changed(emitter).await?;
        self.telemetry_valid_changed(emitter).await
    }
}

#[interface(name = "org.shadowblip.Fan.Sensor")]
impl FanSensor {
    /// Stable sensor ID used in profile sensor lists.
    #[zbus(property)]
    pub async fn id(&self) -> String {
        let controller = self.controller.lock().await;
        format!("{}/{}", controller.id(), self.local_id)
    }

    /// Human-readable sensor name.
    #[zbus(property)]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Last validated temperature in degrees Celsius. Read only when TelemetryValid is true.
    #[zbus(property)]
    pub async fn temperature_c(&self) -> f64 {
        self.temperature().await.unwrap_or(0.0)
    }

    /// Whether a valid reading is available. False during suspend and after telemetry loss.
    #[zbus(property)]
    pub async fn telemetry_valid(&self) -> bool {
        self.temperature().await.is_some()
    }
}
