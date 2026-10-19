use super::config::{
    CurvePoint, FanConfig, ManualWriteOrder, FIRMWARE_HANDOFF_TEMPERATURE_C,
    FULL_SPEED_TEMPERATURE_C,
};
use super::discovery::{discover_fan, discover_fan_enable, BackendKind, FanPaths};
use super::profile::{FanMode, FanProfile};
use super::FanError;
use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq)]
pub struct FanSnapshot {
    pub id: String,
    pub name: String,
    pub mode: String,
    pub available_modes: Vec<String>,
    pub temperature_c: Option<f64>,
    pub rpm: Option<u32>,
    pub effective_percent: Option<u8>,
    pub fault: Option<String>,
    pub automatic_verified: bool,
    pub suspended: bool,
}

#[derive(Debug, Clone, Copy)]
struct Telemetry {
    temperature_c: f64,
    curve_temperature_c: f64,
    rpm: u32,
    hardware_mode: i64,
}

pub struct FanController {
    pub config: FanConfig,
    sysfs_root: PathBuf,
    id: String,
    selection: FanProfile,
    paths: FanPaths,
    device_identity: String,
    effective_percent: Option<u8>,
    temperature_c: Option<f64>,
    sensor_temperatures: BTreeMap<String, f64>,
    rpm: Option<u32>,
    fault: Option<String>,
    automatic_verified: bool,
    owns_control: bool,
    suspended: bool,
    custom_control_allowed: Arc<AtomicBool>,
    fan_running: bool,
    stall_seconds: u32,
    thermal_override: bool,
}

impl FanController {
    pub fn new(
        device_id: String,
        config: FanConfig,
        sysfs_root: PathBuf,
    ) -> Result<Self, FanError> {
        let paths = discover_fan(&sysfs_root, &config)?;
        let id = format!("{device_id}/{}", config.id);
        Ok(Self {
            config,
            sysfs_root,
            selection: FanProfile::automatic(id.clone()),
            id,
            device_identity: paths.device_identity.clone(),
            paths,
            effective_percent: None,
            temperature_c: None,
            sensor_temperatures: BTreeMap::new(),
            rpm: None,
            fault: None,
            automatic_verified: false,
            owns_control: false,
            suspended: false,
            custom_control_allowed: Arc::new(AtomicBool::new(true)),
            fan_running: false,
            stall_seconds: 0,
            thermal_override: false,
        })
    }

    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn selection(&self) -> &FanProfile {
        &self.selection
    }
    pub fn sensor_temperature(&self, id: &str) -> Option<f64> {
        self.sensor_temperatures.get(id).copied()
    }
    pub(crate) fn set_control_gate(&mut self, gate: Arc<AtomicBool>) {
        self.custom_control_allowed = gate;
    }

    pub fn start(&mut self) -> Result<(), FanError> {
        // No on-disk selection is ever read. Every new process starts in firmware.
        self.selection = FanProfile::automatic(self.id.clone());
        self.set_automatic()?;
        match self.read_telemetry() {
            Ok(telemetry) => {
                self.update_telemetry(telemetry);
                Ok(())
            }
            Err(error) => Err(self.fail_safe(format!(
                "unable to validate fan telemetry at startup: {error}"
            ))),
        }
    }

    pub fn tick(&mut self) -> Result<(), FanError> {
        if self.suspended {
            return if self.automatic_verified {
                Ok(())
            } else {
                Err(FanError(
                    "suspended fan has no verified automatic handoff".into(),
                ))
            };
        }
        if !self.custom_control_allowed.load(Ordering::Acquire)
            && self.selection.mode != FanMode::Automatic
        {
            return Err(FanError(
                "custom fan monitor is paused for a lifecycle handoff".into(),
            ));
        }
        if let Err(error) = self.rediscover() {
            return Err(self.fail_safe(error.to_string()));
        }
        let telemetry = match self.read_telemetry() {
            Ok(telemetry) => telemetry,
            Err(error) => return Err(self.fail_safe(error.to_string())),
        };
        self.update_telemetry(telemetry);
        if let Err(error) = self.apply_selected(telemetry, false) {
            return Err(self.fail_safe(error.to_string()));
        }
        Ok(())
    }

    pub fn set_automatic(&mut self) -> Result<(), FanError> {
        // Releasing control bypasses lifecycle and telemetry checks.
        if let Err(error) = self.restore_automatic() {
            return Err(
                self.fail_safe(format!("unable to select firmware automatic mode: {error}"))
            );
        }
        self.selection = FanProfile::automatic(self.id.clone());
        self.thermal_override = false;
        self.fault = None;
        Ok(())
    }

    /// Pure validation, including platform capability checks; no hardware writes.
    pub fn validate_selection(&self, next: &FanProfile) -> Result<(), FanError> {
        next.validate_shape()?;
        let fail = |message: &str| FanError(message.to_string());
        if next.fan != self.id {
            return Err(fail("unknown fan ID"));
        }
        if next.mode == FanMode::Automatic {
            return Ok(());
        }
        if self.suspended || !self.custom_control_allowed.load(Ordering::Acquire) {
            return Err(fail(
                "custom fan control is unavailable during a lifecycle transition",
            ));
        }
        if next.mode == FanMode::Manual {
            if !next.percent.is_some_and(|percent| {
                percent >= self.config.limits.minimum_percent && percent <= 100
            }) {
                return Err(fail("manual percentage is outside device limits"));
            }
            return Ok(());
        }
        for id in &next.sensors {
            let Some(local) = id.strip_prefix(&format!("{}/", self.id)) else {
                return Err(fail("unknown sensor ID"));
            };
            if !self.paths.temperature_inputs.contains_key(local) {
                return Err(fail("unknown or unavailable sensor ID"));
            }
        }
        self.config.validate_curve("profile", &next.curve)?;
        if next.mode == FanMode::FirmwareCurve {
            let BackendKind::HardwareCurve(points) = &self.paths.backend else {
                return Err(fail("firmware_curve is unavailable"));
            };
            validate_hardware_curve(&next.curve, points.len())?;
            if next
                .curve
                .iter()
                .any(|&(_, percent)| percent < self.config.limits.minimum_running_percent)
            {
                return Err(fail("native curves must stay at or above the running floor; use software_curve for fan stop"));
            }
            let bound = self
                .config
                .firmware_curve_sensor
                .as_ref()
                .map(|id| format!("{}/{}", self.id, id));
            if next.sensors.len() != 1 || bound.as_ref() != next.sensors.first() {
                return Err(fail("native curve must use its trusted hardware sensor"));
            }
        }
        Ok(())
    }

    pub fn configure(&mut self, next: FanProfile) -> Result<(), FanError> {
        self.validate_selection(&next)?;
        if next.mode == FanMode::Automatic {
            return self.set_automatic();
        }
        if let Err(error) = self.rediscover() {
            return Err(self.fail_safe(error.to_string()));
        }
        // Recheck capabilities after fresh discovery, still under the lock.
        self.validate_selection(&next)?;
        self.selection = next;
        let telemetry = match self.read_telemetry() {
            Ok(telemetry) => telemetry,
            Err(error) => return Err(self.fail_safe(error.to_string())),
        };
        self.update_telemetry(telemetry);
        if let Err(error) = self.apply_selected(telemetry, true) {
            return Err(self.fail_safe(error.to_string()));
        }
        self.fault = None;
        Ok(())
    }

    pub fn prepare_for_sleep(&mut self) -> Result<(), FanError> {
        self.suspended = true;
        self.invalidate_telemetry();
        self.restore_automatic().map_err(|error| {
            self.fail_safe(format!(
                "unable to enter firmware automatic mode before sleep: {error}"
            ))
        })
    }

    pub fn resume(&mut self) -> Result<(), FanError> {
        self.suspended = false;
        if let Err(error) = self.rediscover() {
            return Err(self.fail_safe(format!("resume discovery failed: {error}")));
        }
        let telemetry = match self.read_telemetry() {
            Ok(telemetry) => telemetry,
            Err(error) => return Err(self.fail_safe(format!("resume telemetry failed: {error}"))),
        };
        self.update_telemetry(telemetry);
        if let Err(error) = self.apply_selected(telemetry, true) {
            return Err(self.fail_safe(format!("resume restore failed: {error}")));
        }
        Ok(())
    }

    pub fn shutdown(&mut self) -> Result<(), FanError> {
        self.suspended = true;
        self.invalidate_telemetry();
        self.restore_automatic()
    }

    pub(crate) fn fail_to_automatic(&mut self, reason: &str) -> FanError {
        self.fail_safe(reason.to_string())
    }

    pub fn snapshot(&self) -> FanSnapshot {
        let mut modes = vec!["automatic".into(), "manual".into(), "software_curve".into()];
        if matches!(self.paths.backend, BackendKind::HardwareCurve(_)) {
            modes.push("firmware_curve".into());
        }
        FanSnapshot {
            id: self.id.clone(),
            name: self.config.name.clone(),
            mode: if self.suspended {
                "automatic"
            } else {
                self.selection.mode.name()
            }
            .into(),
            available_modes: modes,
            temperature_c: self.temperature_c,
            rpm: self.rpm,
            effective_percent: self.effective_percent,
            fault: self.fault.clone(),
            automatic_verified: self.automatic_verified,
            suspended: self.suspended,
        }
    }

    fn apply_selected(
        &mut self,
        telemetry: Telemetry,
        allow_transition: bool,
    ) -> Result<(), FanError> {
        if self.selection.mode != FanMode::Automatic
            && telemetry.temperature_c >= FIRMWARE_HANDOFF_TEMPERATURE_C
        {
            return Err(FanError(format!(
                "temperature {:.1} C reached the {:.1} C firmware handoff limit",
                telemetry.temperature_c, FIRMWARE_HANDOFF_TEMPERATURE_C
            )));
        }
        match self.selection.mode {
            FanMode::Automatic => {
                if !allow_transition && telemetry.hardware_mode != self.config.mode_values.automatic
                {
                    return Err(FanError(format!(
                        "unexpected hardware mode change to {}",
                        telemetry.hardware_mode
                    )));
                }
                if allow_transition {
                    self.restore_automatic()
                } else {
                    self.owns_control = false;
                    self.automatic_verified = true;
                    self.effective_percent = None;
                    self.fan_running = false;
                    self.stall_seconds = 0;
                    Ok(())
                }
            }
            FanMode::Manual => {
                self.apply_manual(self.selection.percent.unwrap(), telemetry, allow_transition)
            }
            FanMode::SoftwareCurve => {
                let percent = interpolate(&self.selection.curve, telemetry.curve_temperature_c);
                self.apply_manual(percent, telemetry, allow_transition)
            }
            FanMode::FirmwareCurve => {
                let BackendKind::HardwareCurve(attributes) = self.paths.backend.clone() else {
                    return Err(FanError("native curve attributes disappeared".into()));
                };
                let expected = if self.thermal_override {
                    self.config.mode_values.manual
                } else {
                    self.config.mode_values.hardware_curve.unwrap()
                };
                if !allow_transition && self.owns_control && telemetry.hardware_mode != expected {
                    return Err(FanError(format!(
                        "unexpected hardware mode change to {}",
                        telemetry.hardware_mode
                    )));
                }
                if telemetry.temperature_c >= FULL_SPEED_TEMPERATURE_C {
                    self.apply_manual(100, telemetry, true)?;
                    self.check_stall(100, telemetry.rpm, !allow_transition)?;
                    self.thermal_override = true;
                    return Ok(());
                }
                if allow_transition || self.thermal_override {
                    self.program_hardware_curve(&attributes)?;
                }
                self.thermal_override = false;
                self.owns_control = true;
                self.automatic_verified = false;
                // Native duty is not a readback. Do not present an estimate as applied control.
                self.effective_percent = None;
                self.check_stall(
                    interpolate(&self.selection.curve, telemetry.curve_temperature_c),
                    telemetry.rpm,
                    !allow_transition,
                )
            }
        }
    }

    fn apply_manual(
        &mut self,
        requested_percent: u8,
        telemetry: Telemetry,
        allow_transition: bool,
    ) -> Result<(), FanError> {
        let percent = self.safe_percent(requested_percent, telemetry.temperature_c);
        let raw = percent_to_raw(percent, &self.config);
        let expected_mode = self.config.mode_values.manual;
        if !allow_transition && self.owns_control && telemetry.hardware_mode != expected_mode {
            return Err(FanError(format!(
                "unexpected fan owner changed hardware mode to {}",
                telemetry.hardware_mode
            )));
        }

        match self.config.manual_write_order {
            ManualWriteOrder::PwmThenEnable => {
                // Some drivers allow a PWM write in firmware-auto mode but do
                // not allow that register to be read until manual mode is set.
                // Write duty first, switch ownership, then verify both values.
                write_value(&self.paths.pwm, raw as i64)?;
                write_verify(&self.paths.enable, expected_mode)?;
                read_verify_tolerance(
                    &self.paths.pwm,
                    raw as i64,
                    self.config.limits.pwm_readback_tolerance as i64,
                )?;
            }
            ManualWriteOrder::EnableThenPwm => {
                write_verify(&self.paths.enable, expected_mode)?;
                write_verify_tolerance(
                    &self.paths.pwm,
                    raw as i64,
                    self.config.limits.pwm_readback_tolerance as i64,
                )?;
            }
        }
        self.owns_control = true;
        self.automatic_verified = false;
        self.effective_percent = Some(percent);
        self.check_stall(percent, telemetry.rpm, !allow_transition)
    }

    fn program_hardware_curve(
        &mut self,
        attributes: &[super::discovery::HardwareCurvePointPaths],
    ) -> Result<(), FanError> {
        validate_hardware_curve(&self.selection.curve, attributes.len())?;
        self.restore_automatic()?;
        for (&(temperature_c, percent), paths) in self.selection.curve.iter().zip(attributes) {
            let temperature = (temperature_c * 1000.0).round() as i64;
            let pwm = percent_to_raw(percent, &self.config) as i64;
            write_verify(&paths.temperature, temperature)?;
            write_verify_tolerance(
                &paths.pwm,
                pwm,
                self.config.limits.pwm_readback_tolerance as i64,
            )?;
        }
        let mode = self
            .config
            .mode_values
            .hardware_curve
            .ok_or_else(|| FanError("hardware curve mode value is unavailable".into()))?;
        write_verify(&self.paths.enable, mode)?;
        Ok(())
    }

    fn safe_percent(&mut self, requested: u8, temperature_c: f64) -> u8 {
        if temperature_c >= FULL_SPEED_TEMPERATURE_C {
            self.fan_running = true;
            return 100;
        }
        if self.config.limits.minimum_percent > 0 {
            self.fan_running = true;
            return requested.max(self.config.limits.minimum_running_percent);
        }
        if requested > 0 {
            self.fan_running = true;
            return requested.max(self.config.limits.minimum_running_percent);
        }
        if self.fan_running {
            if temperature_c <= self.config.limits.stop_temperature_c {
                self.fan_running = false;
                return 0;
            }
            return self.config.limits.minimum_running_percent;
        }
        if temperature_c > self.config.limits.restart_temperature_c {
            self.fan_running = true;
            return self.config.limits.minimum_running_percent;
        }
        0
    }

    fn check_stall(
        &mut self,
        percent: u8,
        rpm: u32,
        advance_interval: bool,
    ) -> Result<(), FanError> {
        if percent >= self.config.limits.minimum_running_percent
            && rpm < self.config.limits.stall_rpm
        {
            if advance_interval {
                self.stall_seconds += 1;
            }
            if self.stall_seconds >= self.config.limits.stall_grace_seconds {
                return Err(FanError(format!(
                    "fan stalled below {} RPM at {} percent",
                    self.config.limits.stall_rpm, percent
                )));
            }
        } else {
            self.stall_seconds = 0;
        }
        Ok(())
    }

    fn rediscover(&mut self) -> Result<(), FanError> {
        let discovered = discover_fan(&self.sysfs_root, &self.config)?;
        if discovered.device_identity != self.device_identity {
            let _ = restore_automatic_at(&self.paths, self.config.mode_values.automatic);
            let _ = restore_automatic_at(&discovered, self.config.mode_values.automatic);
            return Err(FanError(format!(
                "fan hardware identity changed from {} to {}",
                self.device_identity, discovered.device_identity
            )));
        }
        if discovered.backend.name() != self.paths.backend.name()
            || discovered.backend.curve_point_count() != self.paths.backend.curve_point_count()
        {
            let _ = restore_automatic_at(&self.paths, self.config.mode_values.automatic);
            let _ = restore_automatic_at(&discovered, self.config.mode_values.automatic);
            return Err(FanError(
                "fan backend attributes changed during control".into(),
            ));
        }
        self.paths = discovered;
        Ok(())
    }

    fn read_telemetry(&mut self) -> Result<Telemetry, FanError> {
        let hardware_mode = read_integer(&self.paths.enable)?;
        let rpm = read_integer(&self.paths.rpm)?;
        if rpm < 0 || rpm as u64 > self.config.limits.maximum_valid_rpm as u64 {
            return Err(FanError(format!("implausible fan RPM {rpm}")));
        }
        let mut readings = BTreeMap::new();
        let mut hottest = f64::NEG_INFINITY;
        let mut curve_temperature = f64::NEG_INFINITY;
        for source in &self.config.temperature_sources {
            let id = format!("{}/{}", self.id, source.id);
            let selected = self.selection.sensors.contains(&id);
            let reading = self
                .paths
                .temperature_inputs
                .get(&source.id)
                .ok_or_else(|| FanError(format!("sensor {id} is unavailable")))
                .and_then(|path| read_integer(path))
                .map(|raw| raw as f64 / 1000.0)
                .and_then(|temperature| {
                    if temperature < self.config.limits.minimum_valid_temperature_c
                        || temperature > self.config.limits.maximum_valid_temperature_c
                    {
                        Err(FanError(format!(
                            "implausible temperature {temperature:.1} C from {id}"
                        )))
                    } else {
                        Ok(temperature)
                    }
                });
            match reading {
                Ok(temperature) => {
                    readings.insert(id, temperature);
                    if source.required {
                        hottest = hottest.max(temperature);
                    }
                    if selected {
                        curve_temperature = curve_temperature.max(temperature);
                    }
                }
                Err(error) if source.required || selected => return Err(error),
                Err(_) => (),
            }
        }
        if !hottest.is_finite() {
            return Err(FanError("no valid safety temperature telemetry".into()));
        }
        self.sensor_temperatures = readings;
        Ok(Telemetry {
            temperature_c: hottest.max(curve_temperature),
            curve_temperature_c: curve_temperature,
            rpm: rpm as u32,
            hardware_mode,
        })
    }

    fn update_telemetry(&mut self, telemetry: Telemetry) {
        self.temperature_c = Some(telemetry.temperature_c);
        self.rpm = Some(telemetry.rpm);
        self.automatic_verified = telemetry.hardware_mode == self.config.mode_values.automatic;
    }

    fn invalidate_telemetry(&mut self) {
        self.sensor_temperatures.clear();
        self.temperature_c = None;
        self.rpm = None;
        self.effective_percent = None;
    }

    fn restore_automatic(&mut self) -> Result<(), FanError> {
        if let Err(original) = restore_automatic_at(&self.paths, self.config.mode_values.automatic)
        {
            restore_automatic_for_config(&self.sysfs_root, &self.config).map_err(|fallback| {
                FanError(format!(
                    "automatic restore at the last known path failed: {original}; emergency rediscovery failed: {fallback}"
                ))
            })?;
        }
        self.owns_control = false;
        self.automatic_verified = true;
        self.effective_percent = None;
        self.fan_running = false;
        self.stall_seconds = 0;
        Ok(())
    }

    fn fail_safe(&mut self, reason: String) -> FanError {
        self.invalidate_telemetry();
        let restore_error = self.restore_automatic().err();
        self.selection = FanProfile::automatic(self.id.clone());
        self.thermal_override = false;
        let mut message = reason;
        if let Some(error) = restore_error {
            message.push_str(&format!("; firmware automatic restore failed: {error}"));
            self.automatic_verified = false;
        }
        self.fault = Some(message.clone());
        FanError(message)
    }
}

fn interpolate(points: &[CurvePoint], temperature_c: f64) -> u8 {
    let (first_temperature, first_percent) = points[0];
    if temperature_c <= first_temperature {
        return first_percent;
    }
    for pair in points.windows(2) {
        let (low_temperature, low_percent) = pair[0];
        let (high_temperature, high_percent) = pair[1];
        if temperature_c <= high_temperature {
            let ratio = (temperature_c - low_temperature) / (high_temperature - low_temperature);
            return (low_percent as f64 + ratio * (high_percent as f64 - low_percent as f64))
                .round()
                .clamp(0.0, 100.0) as u8;
        }
    }
    points.last().unwrap().1
}

fn validate_hardware_curve(points: &[CurvePoint], count: usize) -> Result<(), FanError> {
    if count < 2 || points.len() != count {
        return Err(FanError(format!(
            "hardware curve requires exactly {count} points; received {}",
            points.len()
        )));
    }
    Ok(())
}

fn percent_to_raw(percent: u8, config: &FanConfig) -> u32 {
    let span = config.limits.pwm_max - config.limits.pwm_min;
    config.limits.pwm_min + ((span as f64 * percent as f64 / 100.0).round() as u32)
}

pub(crate) fn restore_automatic_at(paths: &FanPaths, value: i64) -> Result<(), FanError> {
    write_verify(&paths.enable, value)
}

pub(crate) fn restore_automatic_for_config(
    sysfs_root: &Path,
    config: &FanConfig,
) -> Result<(), FanError> {
    let enable = discover_fan_enable(sysfs_root, config)?;
    write_verify(&enable, config.mode_values.automatic)
}

fn write_verify(path: &Path, value: i64) -> Result<(), FanError> {
    write_verify_tolerance(path, value, 0)
}

fn write_verify_tolerance(path: &Path, value: i64, tolerance: i64) -> Result<(), FanError> {
    write_value(path, value)?;
    read_verify_tolerance(path, value, tolerance)
}

fn write_value(path: &Path, value: i64) -> Result<(), FanError> {
    let mut file = OpenOptions::new()
        .write(true)
        .truncate(true)
        .open(path)
        .map_err(|error| FanError(format!("unable to open {}: {error}", path.display())))?;
    file.write_all(value.to_string().as_bytes())
        .map_err(|error| FanError(format!("unable to write {}: {error}", path.display())))?;
    drop(file);
    Ok(())
}

fn read_verify_tolerance(path: &Path, value: i64, tolerance: i64) -> Result<(), FanError> {
    let actual = read_integer(path)?;
    if actual.abs_diff(value) > tolerance as u64 {
        return Err(FanError(format!(
            "readback failed for {}: wrote {value}, read {actual}, tolerance {tolerance}",
            path.display(),
        )));
    }
    Ok(())
}

fn read_integer(path: &Path) -> Result<i64, FanError> {
    let raw = fs::read_to_string(path)
        .map_err(|error| FanError(format!("unable to read {}: {error}", path.display())))?;
    raw.trim().parse::<i64>().map_err(|error| {
        FanError(format!(
            "invalid integer telemetry in {}: {error}",
            path.display()
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::super::config::{BackendPreference, TemperatureSource};
    use super::super::test_support::{example_fan, hwmon, write as write_file, TempTree};
    use super::*;

    struct Fixture {
        root: TempTree,
        state: PathBuf,
    }

    impl Fixture {
        fn new(hardware_points: usize) -> Self {
            let root = TempTree::new();
            hwmon(
                &root.path().join("sys"),
                &example_fan(),
                2,
                hardware_points,
                40000,
            );
            Self {
                state: root.path().join("state/fan-state.yaml"),
                root,
            }
        }

        fn sysfs(&self) -> PathBuf {
            self.root.path().join("sys")
        }

        fn fan(&self, name: &str) -> PathBuf {
            self.sysfs().join("class/hwmon/hwmon2").join(name)
        }

        fn temperature(&self, value: f64) {
            write_file(
                self.sysfs().join("class/hwmon/hwmon9/temp1_input"),
                &format!("{}\n", (value * 1000.0).round() as i64),
            );
        }

        fn rpm(&self, value: u32) {
            write_file(self.fan("fan1_input"), &format!("{value}\n"));
        }
    }

    fn config(backend: BackendPreference) -> FanConfig {
        FanConfig {
            backend,
            ..example_fan()
        }
    }

    fn controller(fixture: &Fixture, backend: BackendPreference) -> FanController {
        let mut controller =
            FanController::new("example".into(), config(backend), fixture.sysfs()).unwrap();
        controller.start().unwrap();
        controller
    }

    fn hardware_controller(fixture: &Fixture) -> FanController {
        let mut config = config(BackendPreference::Auto);
        config.limits.minimum_percent = 10;
        let mut controller = FanController::new("example".into(), config, fixture.sysfs()).unwrap();
        controller.start().unwrap();
        controller
    }

    fn quiet(controller: &FanController) -> FanProfile {
        FanProfile {
            fan: controller.id().into(),
            mode: FanMode::SoftwareCurve,
            percent: None,
            sensors: controller
                .config
                .temperature_sources
                .iter()
                .map(|source| format!("{}/{}", controller.id(), source.id))
                .collect(),
            curve: vec![
                (45.0, 0),
                (55.0, 15),
                (65.0, 25),
                (75.0, 35),
                (85.0, 55),
                (90.0, 75),
                (95.0, 100),
            ],
        }
    }

    fn native(controller: &FanController) -> FanProfile {
        FanProfile {
            curve: vec![(45.0, 10), (70.0, 30), (95.0, 100)],
            mode: FanMode::FirmwareCurve,
            ..quiet(controller)
        }
    }

    impl FanController {
        fn set_manual(&mut self, percent: u8) -> Result<(), FanError> {
            self.configure(FanProfile {
                mode: FanMode::Manual,
                percent: Some(percent),
                ..FanProfile::automatic(self.id().into())
            })
        }
        fn set_curve(&mut self, points: Vec<CurvePoint>) -> Result<(), FanError> {
            let mode = if matches!(self.paths.backend, BackendKind::HardwareCurve(_)) {
                FanMode::FirmwareCurve
            } else {
                FanMode::SoftwareCurve
            };
            self.configure(FanProfile {
                mode,
                curve: points,
                ..quiet(self)
            })
        }
    }

    #[test]
    fn failed_suspend_and_closed_gate_never_reacquire_control() {
        let fixture = Fixture::new(0);
        let mut controller = controller(&fixture, BackendPreference::Auto);
        controller.set_manual(20).unwrap();
        fs::remove_file(fixture.fan("pwm1_enable")).unwrap();
        fs::create_dir(fixture.fan("pwm1_enable")).unwrap();
        assert!(controller.prepare_for_sleep().is_err());
        assert!(controller.snapshot().suspended);
        assert!(!controller.snapshot().automatic_verified);
        assert!(controller.tick().is_err());
        fs::remove_dir(fixture.fan("pwm1_enable")).unwrap();
        write_file(fixture.fan("pwm1_enable"), "1\n");
        assert!(controller.set_manual(20).is_err());
        controller.set_automatic().unwrap();
        assert_eq!(read_integer(&fixture.fan("pwm1_enable")).unwrap(), 2);
        controller.resume().unwrap();
        controller.set_control_gate(Arc::new(AtomicBool::new(false)));
        assert!(controller.set_manual(20).is_err());
        assert!(controller.configure(quiet(&controller)).is_err());
        assert_eq!(read_integer(&fixture.fan("pwm1_enable")).unwrap(), 2);
    }

    #[test]
    fn sensor_loss_invalidates_telemetry_and_automatic_still_works() {
        let fixture = Fixture::new(0);
        let mut controller = controller(&fixture, BackendPreference::Auto);
        controller.set_manual(20).unwrap();
        let sensor = fixture.sysfs().join("class/hwmon/hwmon9/temp1_input");
        fs::remove_file(&sensor).unwrap();
        assert!(controller.tick().is_err());
        assert_eq!(controller.snapshot().temperature_c, None);
        assert_eq!(controller.snapshot().rpm, None);
        assert_eq!(controller.snapshot().effective_percent, None);
        controller.set_automatic().unwrap();
        assert!(controller.snapshot().automatic_verified);
        assert_eq!(controller.snapshot().temperature_c, None);
        write_file(sensor, "42000\n");
        controller.tick().unwrap();
        assert_eq!(controller.snapshot().temperature_c, Some(42.0));
        assert_eq!(controller.snapshot().rpm, Some(1200));
    }

    #[test]
    fn hardware_attributes_do_not_bypass_fan_stop_policy() {
        let fixture = Fixture::new(3);
        let mut controller = controller(&fixture, BackendPreference::Auto);
        assert!(controller
            .snapshot()
            .available_modes
            .contains(&"software_curve".into()));
        controller.configure(quiet(&controller)).unwrap();
        fixture.temperature(46.0);
        controller.tick().unwrap();
        assert_eq!(controller.snapshot().effective_percent, Some(10));
        fixture.temperature(43.0);
        controller.tick().unwrap();
        assert_eq!(controller.snapshot().effective_percent, Some(10));
        fixture.temperature(42.0);
        controller.tick().unwrap();
        assert_eq!(controller.snapshot().effective_percent, Some(0));
    }

    #[test]
    fn hardware_curve_point_count_errors_preserve_active_curve() {
        let fixture = Fixture::new(3);
        let mut controller = hardware_controller(&fixture);
        controller.configure(native(&controller)).unwrap();
        let before = controller.snapshot();
        assert!(controller
            .set_curve(vec![(45.0, 10), (95.0, 100),])
            .is_err());
        assert_eq!(controller.snapshot(), before);
        assert_eq!(read_integer(&fixture.fan("pwm1_enable")).unwrap(), 5);
        assert_eq!(
            read_integer(&fixture.fan("pwm1_auto_point1_pwm")).unwrap(),
            26
        );
        fixture.temperature(95.0);
        controller.tick().unwrap();
        assert_eq!(controller.snapshot().effective_percent, Some(100));
    }

    #[test]
    fn quiet_interpolation_zero_hysteresis_and_thermal_limits() {
        let fixture = Fixture::new(0);
        let mut controller = controller(&fixture, BackendPreference::Auto);
        controller.configure(quiet(&controller)).unwrap();
        assert_eq!(controller.snapshot().effective_percent, Some(0));

        fixture.temperature(46.0);
        controller.tick().unwrap();
        assert_eq!(controller.snapshot().effective_percent, Some(10));

        fixture.temperature(43.0);
        controller.tick().unwrap();
        assert_eq!(controller.snapshot().effective_percent, Some(10));

        fixture.temperature(42.0);
        controller.tick().unwrap();
        assert_eq!(controller.snapshot().effective_percent, Some(0));

        fixture.temperature(95.0);
        controller.tick().unwrap();
        assert_eq!(controller.snapshot().effective_percent, Some(100));

        fixture.temperature(98.0);
        assert!(controller.tick().is_err());
        assert_eq!(read_integer(&fixture.fan("pwm1_enable")).unwrap(), 2);
        assert_eq!(controller.snapshot().mode, "automatic");
    }

    #[test]
    fn ownership_change_sensor_loss_and_stall_fail_to_automatic() {
        let fixture = Fixture::new(0);
        let mut controller = controller(&fixture, BackendPreference::Auto);
        controller.set_manual(20).unwrap();
        write_file(fixture.fan("pwm1_enable"), "2\n");
        assert!(controller.tick().is_err());
        assert_eq!(read_integer(&fixture.fan("pwm1_enable")).unwrap(), 2);

        controller.set_manual(20).unwrap();
        fixture.rpm(0);
        for _ in 0..5 {
            controller.set_manual(20).unwrap();
        }
        assert!(controller.tick().is_ok());
        assert!(controller.tick().is_ok());
        assert!(controller.tick().is_err());
        assert_eq!(read_integer(&fixture.fan("pwm1_enable")).unwrap(), 2);

        controller.set_manual(20).unwrap();
        fs::remove_file(fixture.sysfs().join("class/hwmon/hwmon9/temp1_input")).unwrap();
        assert!(controller.tick().is_err());
        assert_eq!(read_integer(&fixture.fan("pwm1_enable")).unwrap(), 2);
    }

    #[test]
    fn hardware_curve_writes_and_verifies_every_point_before_enable() {
        let fixture = Fixture::new(3);
        let mut controller = hardware_controller(&fixture);
        controller.configure(native(&controller)).unwrap();
        assert_eq!(read_integer(&fixture.fan("pwm1_enable")).unwrap(), 5);
        assert_eq!(
            read_integer(&fixture.fan("pwm1_auto_point1_temp")).unwrap(),
            45_000
        );
        assert_eq!(
            read_integer(&fixture.fan("pwm1_auto_point2_temp")).unwrap(),
            70_000
        );
        assert_eq!(
            read_integer(&fixture.fan("pwm1_auto_point3_temp")).unwrap(),
            95_000
        );
        assert_eq!(controller.snapshot().mode, "firmware_curve");
    }

    #[test]
    fn suspend_releases_and_resume_rediscovers_and_restores() {
        let fixture = Fixture::new(0);
        let mut controller = controller(&fixture, BackendPreference::Auto);
        controller.set_manual(30).unwrap();
        controller.prepare_for_sleep().unwrap();
        assert_eq!(read_integer(&fixture.fan("pwm1_enable")).unwrap(), 2);
        fs::rename(
            fixture.sysfs().join("class/hwmon/hwmon2"),
            fixture.sysfs().join("class/hwmon/hwmon7"),
        )
        .unwrap();
        controller.resume().unwrap();
        assert_eq!(
            read_integer(&fixture.sysfs().join("class/hwmon/hwmon7/pwm1_enable")).unwrap(),
            1
        );
        assert_eq!(controller.snapshot().effective_percent, Some(30));
    }

    #[test]
    fn malformed_telemetry_and_write_failure_restore_automatic() {
        let fixture = Fixture::new(0);
        let mut controller = controller(&fixture, BackendPreference::Auto);
        controller.set_manual(20).unwrap();
        write_file(
            fixture.sysfs().join("class/hwmon/hwmon9/temp1_input"),
            "not-a-temperature\n",
        );
        assert!(controller.tick().is_err());
        assert_eq!(read_integer(&fixture.fan("pwm1_enable")).unwrap(), 2);

        write_file(
            fixture.sysfs().join("class/hwmon/hwmon9/temp1_input"),
            "40000\n",
        );
        controller.set_manual(20).unwrap();
        fs::remove_file(fixture.fan("pwm1")).unwrap();
        fs::create_dir(fixture.fan("pwm1")).unwrap();
        assert!(controller.tick().is_err());
        assert_eq!(read_integer(&fixture.fan("pwm1_enable")).unwrap(), 2);
    }

    #[test]
    fn fallback_rediscovers_the_enable_path_when_full_discovery_fails() {
        let fixture = Fixture::new(0);
        let mut controller = controller(&fixture, BackendPreference::Auto);
        controller.set_manual(20).unwrap();
        let old_fan = fixture.sysfs().join("class/hwmon/hwmon2");
        let new_fan = fixture.sysfs().join("class/hwmon/hwmon7");
        fs::rename(&old_fan, &new_fan).unwrap();
        fs::remove_file(fixture.sysfs().join("class/hwmon/hwmon9/temp1_input")).unwrap();

        assert!(controller.tick().is_err());
        assert_eq!(read_integer(&new_fan.join("pwm1_enable")).unwrap(), 2);
        assert_eq!(controller.snapshot().mode, "automatic");
    }

    #[test]
    fn hottest_configured_sensor_drives_the_curve() {
        let fixture = Fixture::new(0);
        let second = fixture.sysfs().join("class/hwmon/hwmon10");
        write_file(second.join("name"), "acpitz\n");
        write_file(second.join("temp2_label"), "Board\n");
        write_file(second.join("temp2_input"), "60000\n");
        let mut fan = config(BackendPreference::Auto);
        fan.temperature_sources.push(TemperatureSource {
            id: "board".into(),
            name: "Board".into(),
            hwmon_name: "acpitz".into(),
            label: "Board".into(),
            required: true,
        });
        let mut controller = FanController::new("example".into(), fan, fixture.sysfs()).unwrap();
        controller.start().unwrap();
        controller.configure(quiet(&controller)).unwrap();
        assert_eq!(controller.snapshot().temperature_c, Some(60.0));
        assert_eq!(controller.snapshot().effective_percent, Some(20));
    }
    #[test]
    fn suspend_retains_only_live_profile_and_automatic_cancels_restore() {
        let fixture = Fixture::new(0);
        let mut controller = controller(&fixture, BackendPreference::Auto);
        let profile = quiet(&controller);
        controller.configure(profile.clone()).unwrap();
        controller.prepare_for_sleep().unwrap();
        assert_eq!(controller.snapshot().mode, "automatic");
        assert_eq!(controller.selection(), &profile);
        assert!(controller.configure(profile.clone()).is_err());
        assert!(controller.set_manual(30).is_err());
        controller.resume().unwrap();
        assert_eq!(controller.selection(), &profile);
        controller.prepare_for_sleep().unwrap();
        controller.set_automatic().unwrap();
        controller.resume().unwrap();
        assert_eq!(controller.snapshot().mode, "automatic");
        assert!(!fixture.state.exists());
    }

    #[test]
    fn startup_ignores_legacy_state_and_missing_telemetry() {
        let fixture = Fixture::new(0);
        for stale in [
            "schema_version: 1\nfans: {example/system: {mode: manual, manual_percent: 20}}\n",
            "malformed: [",
        ] {
            write_file(&fixture.state, stale);
            write_file(fixture.fan("pwm1_enable"), "1");
            let mut controller = controller(&fixture, BackendPreference::Auto);
            assert_eq!(controller.snapshot().mode, "automatic");
            assert_eq!(read_integer(&fixture.fan("pwm1_enable")).unwrap(), 2);
            controller.set_manual(20).unwrap();
            controller.shutdown().unwrap();
            assert_eq!(fs::read_to_string(&fixture.state).unwrap(), stale);
        }
        fs::remove_file(fixture.sysfs().join("class/hwmon/hwmon9/temp1_input")).unwrap();
        let mut controller = FanController::new(
            "example".into(),
            config(BackendPreference::Auto),
            fixture.sysfs(),
        )
        .unwrap();
        assert!(controller.start().is_err());
        controller.set_automatic().unwrap();
        assert!(controller.snapshot().automatic_verified);
        assert!(!controller.snapshot().effective_percent.is_some());
    }

    #[test]
    fn selected_sensor_does_not_replace_required_safety_sensors() {
        let fixture = Fixture::new(0);
        let board = fixture.sysfs().join("class/hwmon/hwmon10");
        write_file(board.join("name"), "acpitz");
        write_file(board.join("temp2_label"), "Board");
        write_file(board.join("temp2_input"), "60000");
        let mut config = config(BackendPreference::Auto);
        config.temperature_sources.push(TemperatureSource {
            id: "board".into(),
            name: "Board".into(),
            hwmon_name: "acpitz".into(),
            label: "Board".into(),
            required: false,
        });
        let mut controller = FanController::new("example".into(), config, fixture.sysfs()).unwrap();
        controller.start().unwrap();
        let profile = FanProfile {
            sensors: vec!["example/system/board".into()],
            ..quiet(&controller)
        };
        controller.configure(profile).unwrap();
        assert_eq!(controller.snapshot().effective_percent, Some(20));
        fixture.temperature(95.0);
        controller.tick().unwrap();
        assert_eq!(controller.snapshot().effective_percent, Some(100));
        fixture.temperature(98.0);
        assert!(controller.tick().is_err());
        assert_eq!(read_integer(&fixture.fan("pwm1_enable")).unwrap(), 2);
    }

    #[test]
    fn native_constraints_and_safety_override_are_enforced() {
        let fixture = Fixture::new(3);
        let mut controller = hardware_controller(&fixture);
        let mut profile = native(&controller);
        profile.curve[0].1 = 0;
        assert!(controller.configure(profile).is_err());
        let mut profile = native(&controller);
        profile.sensors = vec!["example/system/unknown".into()];
        assert!(controller.configure(profile).is_err());
        controller.configure(native(&controller)).unwrap();
        assert_eq!(controller.snapshot().effective_percent, None);
        fixture.temperature(95.0);
        controller.tick().unwrap();
        assert_eq!(read_integer(&fixture.fan("pwm1")).unwrap(), 255);
        fixture.temperature(70.0);
        controller.tick().unwrap();
        assert_eq!(read_integer(&fixture.fan("pwm1_enable")).unwrap(), 5);
        controller.prepare_for_sleep().unwrap();
        fixture.temperature(98.0);
        assert!(controller.resume().is_err());
        assert_eq!(read_integer(&fixture.fan("pwm1_enable")).unwrap(), 2);
    }

    #[test]
    fn pwm_conversion_and_readback_tolerance_cover_boundaries() {
        let fixture = Fixture::new(0);
        let config = config(BackendPreference::Auto);
        for (case, percent, raw) in [("zero", 0, 0), ("midpoint", 50, 128), ("full", 100, 255)] {
            assert_eq!(percent_to_raw(percent, &config), raw, "{case}");
        }
        write_file(fixture.fan("pwm1"), "131");
        assert!(read_verify_tolerance(&fixture.fan("pwm1"), 128, 3).is_ok());
        assert!(read_verify_tolerance(&fixture.fan("pwm1"), 128, 2).is_err());
    }
    #[test]
    fn invalid_curve_profiles_leave_hardware_and_selection_unchanged() {
        let fixture = Fixture::new(0);
        let mut controller = controller(&fixture, BackendPreference::Auto);
        controller.set_manual(20).unwrap();
        let before = controller.snapshot();
        for (case, points) in [
            ("descending temperature", vec![(55.0, 10), (45.0, 100)]),
            ("duplicate temperature", vec![(45.0, 10), (45.0, 100)]),
            ("descending duty", vec![(45.0, 40), (70.0, 30), (95.0, 100)]),
            ("missing full speed", vec![(45.0, 10), (95.0, 90)]),
            (
                "above full-speed temperature",
                vec![(45.0, 10), (96.0, 100)],
            ),
            ("nonfinite temperature", vec![(f64::NAN, 10), (95.0, 100)]),
            ("duty above 100", vec![(45.0, 101), (95.0, 100)]),
        ] {
            let profile = FanProfile {
                curve: points,
                ..quiet(&controller)
            };
            assert!(controller.configure(profile).is_err(), "{case}");
            assert_eq!(controller.snapshot(), before, "{case}");
            assert_eq!(read_integer(&fixture.fan("pwm1")).unwrap(), 51, "{case}");
        }
        let unsupported = FanProfile {
            mode: FanMode::FirmwareCurve,
            ..quiet(&controller)
        };
        assert!(controller.configure(unsupported).is_err());
    }

    #[test]
    fn native_point_write_failure_leaves_firmware_enabled() {
        let fixture = Fixture::new(3);
        let mut controller = hardware_controller(&fixture);
        let target = fixture.fan("pwm1_auto_point2_pwm");
        fs::remove_file(&target).unwrap();
        fs::create_dir(&target).unwrap();
        assert!(controller.configure(native(&controller)).is_err());
        assert_eq!(read_integer(&fixture.fan("pwm1_enable")).unwrap(), 2);
        // A later point is not written after the failure, nor is native mode enabled.
        assert_eq!(
            read_integer(&fixture.fan("pwm1_auto_point3_temp")).unwrap(),
            70_000
        );
        assert_eq!(controller.snapshot().mode, "automatic");
    }
}
