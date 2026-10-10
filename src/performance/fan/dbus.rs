use super::config::CurvePoint;
use super::controller::{ControllerError, FanController, FanSnapshot};
use std::sync::Arc;
use tokio::sync::Mutex;
use zbus::fdo;
use zbus::object_server::SignalEmitter;
use zbus::zvariant::ObjectPath;
use zbus_macros::interface;

pub struct FanBus {
    object_paths: Vec<String>,
}

impl FanBus {
    pub fn new(object_paths: Vec<String>) -> Self {
        Self { object_paths }
    }
}

#[interface(name = "org.shadowblip.Fan")]
impl FanBus {
    pub async fn enumerate_fans(&self) -> fdo::Result<Vec<ObjectPath>> {
        Ok(self
            .object_paths
            .iter()
            .map(|path| ObjectPath::from_string_unchecked(path.clone()))
            .collect())
    }
}

#[derive(Clone)]
pub struct FanDevice {
    pub controller: Arc<Mutex<FanController>>,
}

impl FanDevice {
    pub fn new(controller: Arc<Mutex<FanController>>) -> Self {
        Self { controller }
    }

    async fn snapshot(&self) -> FanSnapshot {
        self.controller.lock().await.snapshot()
    }

    async fn update<F>(&self, operation: F) -> fdo::Result<()>
    where
        F: FnOnce(&mut FanController) -> Result<(), ControllerError>,
    {
        let mut controller = self.controller.lock().await;
        operation(&mut controller).map_err(|error| fdo::Error::Failed(error.to_string()))
    }

    pub async fn emit_all_changes(&self, emitter: &SignalEmitter<'_>) -> zbus::Result<()> {
        self.mode_changed(emitter).await?;
        self.backend_changed(emitter).await?;
        self.temperature_c_changed(emitter).await?;
        self.telemetry_valid_changed(emitter).await?;
        self.rpm_changed(emitter).await?;
        self.effective_percent_changed(emitter).await?;
        self.manual_percent_changed(emitter).await?;
        self.curve_changed(emitter).await?;
        self.active_preset_changed(emitter).await?;
        self.fault_changed(emitter).await?;
        self.automatic_verified_changed(emitter).await?;
        self.controlling_changed(emitter).await?;
        self.suspended_changed(emitter).await?;
        self.curve_point_count_changed(emitter).await?;
        Ok(())
    }

    async fn emit_configuration_changes(&self, emitter: &SignalEmitter<'_>) -> fdo::Result<()> {
        self.emit_all_changes(emitter)
            .await
            .map_err(|error| fdo::Error::Failed(error.to_string()))
    }
}

#[interface(name = "org.shadowblip.Fan.Device")]
impl FanDevice {
    pub async fn set_automatic(
        &self,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> fdo::Result<()> {
        self.update(FanController::set_automatic).await?;
        self.emit_configuration_changes(&emitter).await
    }

    pub async fn set_manual(
        &self,
        percent: u8,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> fdo::Result<()> {
        self.update(|controller| controller.set_manual(percent))
            .await?;
        self.emit_configuration_changes(&emitter).await
    }

    pub async fn set_curve(
        &self,
        points: Vec<(f64, u8)>,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> fdo::Result<()> {
        let points = points
            .into_iter()
            .map(|(temperature_c, percent)| CurvePoint {
                temperature_c,
                percent,
            })
            .collect();
        self.update(|controller| controller.set_curve(points))
            .await?;
        self.emit_configuration_changes(&emitter).await
    }

    pub async fn apply_preset(
        &self,
        name: &str,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> fdo::Result<()> {
        self.update(|controller| controller.apply_preset(name))
            .await?;
        self.emit_configuration_changes(&emitter).await
    }

    #[zbus(property)]
    pub async fn api_version(&self) -> u32 {
        1
    }

    #[zbus(property)]
    pub async fn id(&self) -> String {
        self.snapshot().await.id
    }

    #[zbus(property)]
    pub async fn name(&self) -> String {
        self.snapshot().await.name
    }

    #[zbus(property)]
    pub async fn mode(&self) -> String {
        self.snapshot().await.mode
    }

    #[zbus(property)]
    pub async fn backend(&self) -> String {
        self.snapshot().await.backend
    }

    #[zbus(property)]
    pub async fn available_modes(&self) -> Vec<String> {
        let snapshot = self.snapshot().await;
        let mut modes = vec!["automatic".into(), "manual".into(), "curve".into()];
        if !snapshot.presets.is_empty() {
            modes.push("preset".into());
        }
        modes
    }

    #[zbus(property)]
    pub async fn temperature_c(&self) -> f64 {
        self.snapshot().await.temperature_c.unwrap_or(0.0)
    }

    #[zbus(property)]
    pub async fn telemetry_valid(&self) -> bool {
        let snapshot = self.snapshot().await;
        snapshot.temperature_c.is_some() && snapshot.rpm.is_some()
    }

    #[zbus(property)]
    pub async fn rpm(&self) -> u32 {
        self.snapshot().await.rpm.unwrap_or(0)
    }

    #[zbus(property)]
    pub async fn effective_percent(&self) -> i16 {
        self.snapshot()
            .await
            .effective_percent
            .map(i16::from)
            .unwrap_or(-1)
    }

    #[zbus(property)]
    pub async fn manual_percent(&self) -> u8 {
        self.snapshot().await.manual_percent
    }

    #[zbus(property)]
    pub async fn curve(&self) -> Vec<(f64, u8)> {
        self.snapshot()
            .await
            .curve
            .into_iter()
            .map(|point| (point.temperature_c, point.percent))
            .collect()
    }

    #[zbus(property)]
    pub async fn presets(&self) -> Vec<String> {
        self.snapshot().await.presets
    }

    #[zbus(property)]
    pub async fn active_preset(&self) -> String {
        self.snapshot().await.active_preset.unwrap_or_default()
    }

    #[zbus(property)]
    pub async fn fault(&self) -> String {
        self.snapshot().await.fault.unwrap_or_default()
    }

    #[zbus(property)]
    pub async fn automatic_verified(&self) -> bool {
        self.snapshot().await.automatic_verified
    }

    #[zbus(property)]
    pub async fn controlling(&self) -> bool {
        self.snapshot().await.controlling
    }

    #[zbus(property)]
    pub async fn suspended(&self) -> bool {
        self.snapshot().await.suspended
    }

    #[zbus(property)]
    pub async fn curve_point_count(&self) -> u16 {
        self.snapshot().await.curve_point_count
    }

    #[zbus(property)]
    pub async fn minimum_percent(&self) -> u8 {
        self.controller.lock().await.config.limits.minimum_percent
    }

    #[zbus(property)]
    pub async fn minimum_running_percent(&self) -> u8 {
        self.controller
            .lock()
            .await
            .config
            .limits
            .minimum_running_percent
    }

    #[zbus(property)]
    pub async fn pwm_min(&self) -> u32 {
        self.controller.lock().await.config.limits.pwm_min
    }

    #[zbus(property)]
    pub async fn pwm_max(&self) -> u32 {
        self.controller.lock().await.config.limits.pwm_max
    }

    #[zbus(property)]
    pub async fn pwm_readback_tolerance(&self) -> u32 {
        self.controller
            .lock()
            .await
            .config
            .limits
            .pwm_readback_tolerance
    }

    #[zbus(property)]
    pub async fn stop_temperature_c(&self) -> f64 {
        self.controller
            .lock()
            .await
            .config
            .limits
            .stop_temperature_c
    }

    #[zbus(property)]
    pub async fn restart_temperature_c(&self) -> f64 {
        self.controller
            .lock()
            .await
            .config
            .limits
            .restart_temperature_c
    }

    #[zbus(property)]
    pub async fn stall_rpm(&self) -> u32 {
        self.controller.lock().await.config.limits.stall_rpm
    }

    #[zbus(property)]
    pub async fn stall_grace_seconds(&self) -> u32 {
        self.controller
            .lock()
            .await
            .config
            .limits
            .stall_grace_seconds
    }

    #[zbus(property)]
    pub async fn full_speed_temperature_c(&self) -> f64 {
        self.controller
            .lock()
            .await
            .config
            .limits
            .full_speed_temperature_c
    }

    #[zbus(property)]
    pub async fn emergency_handoff_temperature_c(&self) -> f64 {
        self.controller
            .lock()
            .await
            .config
            .limits
            .emergency_handoff_temperature_c
    }

    #[zbus(property)]
    pub async fn minimum_valid_temperature_c(&self) -> f64 {
        self.controller
            .lock()
            .await
            .config
            .limits
            .minimum_valid_temperature_c
    }

    #[zbus(property)]
    pub async fn maximum_valid_temperature_c(&self) -> f64 {
        self.controller
            .lock()
            .await
            .config
            .limits
            .maximum_valid_temperature_c
    }

    #[zbus(property)]
    pub async fn maximum_valid_rpm(&self) -> u32 {
        self.controller.lock().await.config.limits.maximum_valid_rpm
    }
}
