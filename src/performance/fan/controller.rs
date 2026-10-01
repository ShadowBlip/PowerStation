use super::config::{CurvePoint, FanConfig, ManualWriteOrder};
use super::discovery::{discover_fan, discover_fan_enable, BackendKind, FanPaths};
use super::persistence::{SavedFanState, SavedMode, StateStore};
use std::fmt::{Display, Formatter};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct ControllerError(pub String);

impl Display for ControllerError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ControllerError {}

#[derive(Debug, Clone, PartialEq)]
pub struct FanSnapshot {
    pub id: String,
    pub name: String,
    pub mode: String,
    pub backend: String,
    pub temperature_c: Option<f64>,
    pub rpm: Option<u32>,
    pub effective_percent: Option<u8>,
    pub manual_percent: u8,
    pub curve: Vec<CurvePoint>,
    pub presets: Vec<String>,
    pub active_preset: Option<String>,
    pub fault: Option<String>,
    pub automatic_verified: bool,
    pub controlling: bool,
    pub suspended: bool,
    pub curve_point_count: u16,
}

#[derive(Debug, Clone, Copy)]
struct Telemetry {
    temperature_c: f64,
    rpm: u32,
    hardware_mode: i64,
}

pub struct FanController {
    pub config: FanConfig,
    sysfs_root: PathBuf,
    state_key: String,
    store: StateStore,
    state: SavedFanState,
    paths: FanPaths,
    device_identity: String,
    effective_percent: Option<u8>,
    temperature_c: Option<f64>,
    rpm: Option<u32>,
    fault: Option<String>,
    automatic_verified: bool,
    owns_control: bool,
    suspended: bool,
    fan_running: bool,
    stall_seconds: u32,
}

impl FanController {
    pub fn new(
        device_id: String,
        config: FanConfig,
        sysfs_root: PathBuf,
        store: StateStore,
    ) -> Result<Self, ControllerError> {
        let paths = discover_fan(&sysfs_root, &config)
            .map_err(|error| ControllerError(error.to_string()))?;
        let state_key = format!("{device_id}/{}", config.id);
        let mut state = store.get(&state_key);
        if let Err(error) = validate_saved_state(&config, &state) {
            log::error!("Discarding invalid saved fan state for {state_key}: {error}");
            state.mode = SavedMode::Automatic;
            state.preset = None;
            store
                .set(&state_key, state.clone())
                .map_err(|error| ControllerError(error.to_string()))?;
        }
        Ok(Self {
            config,
            sysfs_root,
            state_key,
            store,
            state,
            device_identity: paths.device_identity.clone(),
            paths,
            effective_percent: None,
            temperature_c: None,
            rpm: None,
            fault: None,
            automatic_verified: false,
            owns_control: false,
            suspended: false,
            fan_running: false,
            stall_seconds: 0,
        })
    }

    pub fn state_key(&self) -> &str {
        &self.state_key
    }

    pub fn start(&mut self) -> Result<(), ControllerError> {
        if let Err(error) = self.restore_automatic() {
            return Err(self.fail_safe(format!(
                "unable to establish firmware automatic mode at startup: {error}"
            )));
        }
        let telemetry = match self.read_telemetry() {
            Ok(telemetry) => telemetry,
            Err(error) => {
                return Err(self.fail_safe(format!(
                    "unable to validate fan telemetry at startup: {error}"
                )))
            }
        };
        self.update_telemetry(telemetry);
        if !self.suspended && self.state.mode != SavedMode::Automatic {
            if let Err(error) = self.apply_selected(telemetry, true) {
                return Err(self.fail_safe(format!("unable to restore saved fan mode: {error}")));
            }
        }
        self.fault = None;
        Ok(())
    }

    pub fn tick(&mut self) -> Result<(), ControllerError> {
        if self.suspended {
            return Ok(());
        }
        if let Err(error) = self.rediscover() {
            return Err(self.fail_safe(error.to_string()));
        }
        let telemetry = match self.read_telemetry() {
            Ok(telemetry) => telemetry,
            Err(error) => return Err(self.fail_safe(error.to_string())),
        };
        self.update_telemetry(telemetry);
        if telemetry.temperature_c >= self.config.limits.emergency_handoff_temperature_c {
            return Err(self.fail_safe(format!(
                "temperature {:.1} C reached the {:.1} C firmware handoff limit",
                telemetry.temperature_c, self.config.limits.emergency_handoff_temperature_c
            )));
        }
        if let Err(error) = self.apply_selected(telemetry, false) {
            return Err(self.fail_safe(error.to_string()));
        }
        Ok(())
    }

    pub fn set_automatic(&mut self) -> Result<(), ControllerError> {
        let mut next = self.state.clone();
        next.mode = SavedMode::Automatic;
        next.preset = None;
        self.configure(next)
    }

    pub fn set_manual(&mut self, percent: u8) -> Result<(), ControllerError> {
        if percent < self.config.limits.minimum_percent || percent > 100 {
            return Err(ControllerError(format!(
                "manual duty must be between {} and 100 percent",
                self.config.limits.minimum_percent
            )));
        }
        let mut next = self.state.clone();
        next.mode = SavedMode::Manual;
        next.manual_percent = percent;
        next.preset = None;
        self.configure(next)
    }

    pub fn set_curve(&mut self, points: Vec<CurvePoint>) -> Result<(), ControllerError> {
        self.config
            .validate_curve("custom", &points)
            .map_err(|error| ControllerError(error.to_string()))?;
        if let BackendKind::HardwareCurve(paths) = &self.paths.backend {
            if paths.len() < 2 {
                return Err(ControllerError(
                    "hardware curve backend exposes fewer than two points".into(),
                ));
            }
        }
        let mut next = self.state.clone();
        next.mode = SavedMode::Curve;
        next.curve = points;
        next.preset = None;
        self.configure(next)
    }

    pub fn apply_preset(&mut self, name: &str) -> Result<(), ControllerError> {
        if !self.config.presets.contains_key(name) {
            return Err(ControllerError(format!("unknown fan preset {name}")));
        }
        let mut next = self.state.clone();
        next.mode = SavedMode::Preset;
        next.preset = Some(name.to_string());
        self.configure(next)
    }

    pub fn prepare_for_sleep(&mut self) -> Result<(), ControllerError> {
        match self.restore_automatic() {
            Ok(()) => {
                self.suspended = true;
                Ok(())
            }
            Err(error) => Err(self.fail_safe(format!(
                "unable to enter firmware automatic mode before sleep: {error}"
            ))),
        }
    }

    pub fn resume(&mut self) -> Result<(), ControllerError> {
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

    pub fn shutdown(&mut self) -> Result<(), ControllerError> {
        self.suspended = true;
        self.restore_automatic()
    }

    pub(crate) fn fail_to_automatic(&mut self, reason: &str) -> ControllerError {
        self.fail_safe(reason.to_string())
    }

    pub fn snapshot(&self) -> FanSnapshot {
        let active_preset = if self.state.mode == SavedMode::Preset {
            self.state.preset.clone()
        } else {
            None
        };
        FanSnapshot {
            id: self.config.id.clone(),
            name: self.config.name.clone(),
            mode: self.state.mode.name().to_string(),
            backend: self.paths.backend.name().to_string(),
            temperature_c: self.temperature_c,
            rpm: self.rpm,
            effective_percent: self.effective_percent,
            manual_percent: self.state.manual_percent,
            curve: self.selected_curve().unwrap_or_default(),
            presets: self.config.presets.keys().cloned().collect(),
            active_preset,
            fault: self.fault.clone(),
            automatic_verified: self.automatic_verified,
            controlling: self.owns_control,
            suspended: self.suspended,
            curve_point_count: self.paths.backend.curve_point_count(),
        }
    }

    fn configure(&mut self, next: SavedFanState) -> Result<(), ControllerError> {
        validate_saved_state(&self.config, &next)?;
        if let Err(error) = self.rediscover() {
            return Err(self.fail_safe(error.to_string()));
        }
        let telemetry = match self.read_telemetry() {
            Ok(telemetry) => telemetry,
            Err(error) => return Err(self.fail_safe(error.to_string())),
        };
        if telemetry.temperature_c >= self.config.limits.emergency_handoff_temperature_c {
            return Err(self.fail_safe(format!(
                "temperature {:.1} C is at the firmware handoff limit",
                telemetry.temperature_c
            )));
        }
        self.state = next;
        self.update_telemetry(telemetry);
        if let Err(error) = self.apply_selected(telemetry, true) {
            return Err(self.fail_safe(error.to_string()));
        }
        if let Err(error) = self.store.set(&self.state_key, self.state.clone()) {
            return Err(self.fail_safe(format!("unable to persist fan configuration: {error}")));
        }
        self.fault = None;
        Ok(())
    }

    fn apply_selected(
        &mut self,
        telemetry: Telemetry,
        allow_transition: bool,
    ) -> Result<(), ControllerError> {
        if self.state.mode != SavedMode::Automatic
            && telemetry.temperature_c >= self.config.limits.emergency_handoff_temperature_c
        {
            return Err(ControllerError(format!(
                "temperature {:.1} C reached the {:.1} C firmware handoff limit",
                telemetry.temperature_c, self.config.limits.emergency_handoff_temperature_c
            )));
        }
        match self.state.mode {
            SavedMode::Automatic => {
                if !allow_transition && telemetry.hardware_mode != self.config.mode_values.automatic
                {
                    return Err(ControllerError(format!(
                        "unexpected fan owner changed hardware mode to {}",
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
            SavedMode::Manual => {
                self.apply_manual(self.state.manual_percent, telemetry, allow_transition)
            }
            SavedMode::Curve | SavedMode::Preset => {
                let points = self
                    .selected_curve()
                    .ok_or_else(|| ControllerError("selected fan curve is unavailable".into()))?;
                match self.paths.backend.clone() {
                    BackendKind::SoftwareCurve => {
                        let percent = interpolate(&points, telemetry.temperature_c);
                        self.apply_manual(percent, telemetry, allow_transition)
                    }
                    BackendKind::HardwareCurve(auto_points) => {
                        let expected = self.config.mode_values.hardware_curve.ok_or_else(|| {
                            ControllerError("hardware curve mode value is unavailable".into())
                        })?;
                        if !allow_transition && self.owns_control {
                            if telemetry.hardware_mode != expected {
                                return Err(ControllerError(format!(
                                    "unexpected fan owner changed hardware mode to {}",
                                    telemetry.hardware_mode
                                )));
                            }
                        } else {
                            self.program_hardware_curve(&points, &auto_points)?;
                        }
                        self.owns_control = true;
                        self.automatic_verified = false;
                        self.effective_percent =
                            Some(interpolate(&points, telemetry.temperature_c));
                        self.check_stall(
                            self.effective_percent.unwrap(),
                            telemetry.rpm,
                            !allow_transition,
                        )?;
                        Ok(())
                    }
                }
            }
        }
    }

    fn apply_manual(
        &mut self,
        requested_percent: u8,
        telemetry: Telemetry,
        allow_transition: bool,
    ) -> Result<(), ControllerError> {
        let percent = self.safe_percent(requested_percent, telemetry.temperature_c);
        let raw = percent_to_raw(percent, &self.config);
        let expected_mode = self.config.mode_values.manual;
        if !allow_transition && self.owns_control && telemetry.hardware_mode != expected_mode {
            return Err(ControllerError(format!(
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
        points: &[CurvePoint],
        attributes: &[super::discovery::HardwareCurvePointPaths],
    ) -> Result<(), ControllerError> {
        if attributes.len() < 2 {
            return Err(ControllerError(
                "hardware curve requires at least two auto points".into(),
            ));
        }
        self.restore_automatic()?;
        let programmed = resample_curve(points, attributes.len());
        for (point, paths) in programmed.iter().zip(attributes) {
            let temperature = (point.temperature_c * 1000.0).round() as i64;
            let pwm = percent_to_raw(point.percent, &self.config) as i64;
            write_verify(&paths.temperature, temperature)?;
            write_verify_tolerance(
                &paths.pwm,
                pwm,
                self.config.limits.pwm_readback_tolerance as i64,
            )?;
        }
        let mode =
            self.config.mode_values.hardware_curve.ok_or_else(|| {
                ControllerError("hardware curve mode value is unavailable".into())
            })?;
        write_verify(&self.paths.enable, mode)?;
        Ok(())
    }

    fn selected_curve(&self) -> Option<Vec<CurvePoint>> {
        match self.state.mode {
            SavedMode::Preset => self
                .state
                .preset
                .as_ref()
                .and_then(|name| self.config.presets.get(name))
                .cloned(),
            SavedMode::Curve => Some(self.state.curve.clone()),
            _ if !self.state.curve.is_empty() => Some(self.state.curve.clone()),
            _ => None,
        }
    }

    fn safe_percent(&mut self, requested: u8, temperature_c: f64) -> u8 {
        if temperature_c >= self.config.limits.full_speed_temperature_c {
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
    ) -> Result<(), ControllerError> {
        if percent >= self.config.limits.minimum_running_percent
            && rpm < self.config.limits.stall_rpm
        {
            if advance_interval {
                self.stall_seconds += 1;
            }
            if self.stall_seconds >= self.config.limits.stall_grace_seconds {
                return Err(ControllerError(format!(
                    "fan stalled below {} RPM at {} percent",
                    self.config.limits.stall_rpm, percent
                )));
            }
        } else {
            self.stall_seconds = 0;
        }
        Ok(())
    }

    fn rediscover(&mut self) -> Result<(), ControllerError> {
        let discovered = discover_fan(&self.sysfs_root, &self.config)
            .map_err(|error| ControllerError(error.to_string()))?;
        if discovered.device_identity != self.device_identity {
            let _ = restore_automatic_at(&self.paths, self.config.mode_values.automatic);
            let _ = restore_automatic_at(&discovered, self.config.mode_values.automatic);
            return Err(ControllerError(format!(
                "fan hardware identity changed from {} to {}",
                self.device_identity, discovered.device_identity
            )));
        }
        if discovered.backend.name() != self.paths.backend.name()
            || discovered.backend.curve_point_count() != self.paths.backend.curve_point_count()
        {
            let _ = restore_automatic_at(&self.paths, self.config.mode_values.automatic);
            let _ = restore_automatic_at(&discovered, self.config.mode_values.automatic);
            return Err(ControllerError(
                "fan backend attributes changed during control".into(),
            ));
        }
        self.paths = discovered;
        Ok(())
    }

    fn read_telemetry(&self) -> Result<Telemetry, ControllerError> {
        let hardware_mode = read_integer(&self.paths.enable)?;
        let rpm = read_integer(&self.paths.rpm)?;
        if rpm < 0 || rpm as u64 > self.config.limits.maximum_valid_rpm as u64 {
            return Err(ControllerError(format!("implausible fan RPM {rpm}")));
        }
        let mut hottest = f64::NEG_INFINITY;
        for path in &self.paths.temperature_inputs {
            let raw = read_integer(path)?;
            let temperature = raw as f64 / 1000.0;
            if !temperature.is_finite()
                || temperature < self.config.limits.minimum_valid_temperature_c
                || temperature > self.config.limits.maximum_valid_temperature_c
            {
                return Err(ControllerError(format!(
                    "implausible temperature {temperature:.1} C from {}",
                    path.display()
                )));
            }
            hottest = hottest.max(temperature);
        }
        if !hottest.is_finite() {
            return Err(ControllerError("no valid temperature telemetry".into()));
        }
        Ok(Telemetry {
            temperature_c: hottest,
            rpm: rpm as u32,
            hardware_mode,
        })
    }

    fn update_telemetry(&mut self, telemetry: Telemetry) {
        self.temperature_c = Some(telemetry.temperature_c);
        self.rpm = Some(telemetry.rpm);
        self.automatic_verified = telemetry.hardware_mode == self.config.mode_values.automatic;
    }

    fn restore_automatic(&mut self) -> Result<(), ControllerError> {
        if let Err(original) = restore_automatic_at(&self.paths, self.config.mode_values.automatic)
        {
            restore_automatic_for_config(&self.sysfs_root, &self.config).map_err(|fallback| {
                ControllerError(format!(
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

    fn fail_safe(&mut self, reason: String) -> ControllerError {
        let restore_error = self.restore_automatic().err();
        self.state.mode = SavedMode::Automatic;
        self.state.preset = None;
        let persist_error = self.store.set(&self.state_key, self.state.clone()).err();
        let mut message = reason;
        if let Some(error) = restore_error {
            message.push_str(&format!("; firmware automatic restore failed: {error}"));
            self.automatic_verified = false;
        }
        if let Some(error) = persist_error {
            message.push_str(&format!("; automatic state persistence failed: {error}"));
        }
        self.fault = Some(message.clone());
        ControllerError(message)
    }
}

fn validate_saved_state(config: &FanConfig, state: &SavedFanState) -> Result<(), ControllerError> {
    if state.manual_percent < config.limits.minimum_percent || state.manual_percent > 100 {
        return Err(ControllerError(
            "saved manual duty is outside device limits".into(),
        ));
    }
    match state.mode {
        SavedMode::Curve => config
            .validate_curve("saved custom", &state.curve)
            .map_err(|error| ControllerError(error.to_string())),
        SavedMode::Preset => {
            let name = state
                .preset
                .as_deref()
                .ok_or_else(|| ControllerError("saved preset name is missing".into()))?;
            if config.presets.contains_key(name) {
                Ok(())
            } else {
                Err(ControllerError(format!(
                    "saved preset {name} is unavailable"
                )))
            }
        }
        _ => Ok(()),
    }
}

fn interpolate(points: &[CurvePoint], temperature_c: f64) -> u8 {
    if temperature_c <= points[0].temperature_c {
        return points[0].percent;
    }
    for pair in points.windows(2) {
        let low = pair[0];
        let high = pair[1];
        if temperature_c <= high.temperature_c {
            let span = high.temperature_c - low.temperature_c;
            let ratio = (temperature_c - low.temperature_c) / span;
            return (low.percent as f64 + ratio * (high.percent as f64 - low.percent as f64))
                .round()
                .clamp(0.0, 100.0) as u8;
        }
    }
    points.last().unwrap().percent
}

fn resample_curve(points: &[CurvePoint], count: usize) -> Vec<CurvePoint> {
    if points.len() == count {
        return points.to_vec();
    }
    let first = points.first().unwrap().temperature_c;
    let last = points.last().unwrap().temperature_c;
    (0..count)
        .map(|index| {
            let ratio = index as f64 / (count - 1) as f64;
            let temperature_c = first + ratio * (last - first);
            CurvePoint {
                temperature_c,
                percent: interpolate(points, temperature_c),
            }
        })
        .collect()
}

fn percent_to_raw(percent: u8, config: &FanConfig) -> u32 {
    let span = config.limits.pwm_max - config.limits.pwm_min;
    config.limits.pwm_min + ((span as f64 * percent as f64 / 100.0).round() as u32)
}

pub(crate) fn restore_automatic_at(paths: &FanPaths, value: i64) -> Result<(), ControllerError> {
    write_verify(&paths.enable, value)
}

pub(crate) fn restore_automatic_for_config(
    sysfs_root: &Path,
    config: &FanConfig,
) -> Result<(), ControllerError> {
    let enable = discover_fan_enable(sysfs_root, config)
        .map_err(|error| ControllerError(error.to_string()))?;
    write_verify(&enable, config.mode_values.automatic)
}

fn write_verify(path: &Path, value: i64) -> Result<(), ControllerError> {
    write_verify_tolerance(path, value, 0)
}

fn write_verify_tolerance(path: &Path, value: i64, tolerance: i64) -> Result<(), ControllerError> {
    write_value(path, value)?;
    read_verify_tolerance(path, value, tolerance)
}

fn write_value(path: &Path, value: i64) -> Result<(), ControllerError> {
    let mut file = OpenOptions::new()
        .write(true)
        .truncate(true)
        .open(path)
        .map_err(|error| ControllerError(format!("unable to open {}: {error}", path.display())))?;
    file.write_all(value.to_string().as_bytes())
        .map_err(|error| ControllerError(format!("unable to write {}: {error}", path.display())))?;
    drop(file);
    Ok(())
}

fn read_verify_tolerance(path: &Path, value: i64, tolerance: i64) -> Result<(), ControllerError> {
    let actual = read_integer(path)?;
    if actual.abs_diff(value) > tolerance as u64 {
        return Err(ControllerError(format!(
            "readback failed for {}: wrote {value}, read {actual}, tolerance {tolerance}",
            path.display(),
        )));
    }
    Ok(())
}

fn read_integer(path: &Path) -> Result<i64, ControllerError> {
    let raw = fs::read_to_string(path)
        .map_err(|error| ControllerError(format!("unable to read {}: {error}", path.display())))?;
    raw.trim().parse::<i64>().map_err(|error| {
        ControllerError(format!(
            "invalid integer telemetry in {}: {error}",
            path.display()
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::performance::fan::config::{
        BackendPreference, FanLimits, HwmonSelector, ModeValues, TemperatureSource,
    };
    use std::collections::BTreeMap;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_ID: AtomicU64 = AtomicU64::new(0);

    struct Fixture {
        root: PathBuf,
        state: PathBuf,
    }

    impl Fixture {
        fn new(hardware_points: usize) -> Self {
            let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
            let root = std::env::temp_dir().join(format!(
                "powerstation-fan-controller-{}-{id}",
                std::process::id()
            ));
            let _ = fs::remove_dir_all(&root);
            let fan = root.join("sys/class/hwmon/hwmon2");
            write_file(fan.join("name"), "example_ec\n");
            write_file(
                fan.join("device/uevent"),
                "DRIVER=example-ec\nDEVPATH=/devices/platform/example-ec\n",
            );
            write_file(fan.join("pwm1"), "100\n");
            write_file(fan.join("pwm1_enable"), "2\n");
            write_file(fan.join("fan1_input"), "1200\n");
            for index in 1..=hardware_points {
                write_file(
                    fan.join(format!("pwm1_auto_point{index}_temp")),
                    &format!("{}000\n", 40 + index * 10),
                );
                write_file(
                    fan.join(format!("pwm1_auto_point{index}_pwm")),
                    &format!("{}\n", index * 30),
                );
            }
            let sensor = root.join("sys/class/hwmon/hwmon9");
            write_file(sensor.join("name"), "k10temp\n");
            write_file(sensor.join("temp1_label"), "Tctl\n");
            write_file(sensor.join("temp1_input"), "40000\n");
            Self {
                state: root.join("state/fan-state.yaml"),
                root,
            }
        }

        fn sysfs(&self) -> PathBuf {
            self.root.join("sys")
        }

        fn fan(&self, name: &str) -> PathBuf {
            self.root.join("sys/class/hwmon/hwmon2").join(name)
        }

        fn temperature(&self, value: f64) {
            write_file(
                self.root.join("sys/class/hwmon/hwmon9/temp1_input"),
                &format!("{}\n", (value * 1000.0).round() as i64),
            );
        }

        fn rpm(&self, value: u32) {
            write_file(self.fan("fan1_input"), &format!("{value}\n"));
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    fn write_file(path: impl AsRef<Path>, value: &str) {
        let path = path.as_ref();
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, value).unwrap();
    }

    fn config(backend: BackendPreference) -> FanConfig {
        FanConfig {
            id: "system".into(),
            name: "System Fan".into(),
            hwmon: HwmonSelector {
                name: "example_ec".into(),
                parent_driver: "example-ec".into(),
                channel: 1,
            },
            temperature_sources: vec![TemperatureSource {
                hwmon_name: "k10temp".into(),
                label: "Tctl".into(),
                required: true,
            }],
            backend,
            mode_values: ModeValues {
                automatic: 2,
                manual: 1,
                hardware_curve: Some(5),
            },
            manual_write_order: ManualWriteOrder::PwmThenEnable,
            limits: FanLimits {
                pwm_min: 0,
                pwm_max: 255,
                pwm_readback_tolerance: 3,
                minimum_percent: 0,
                minimum_running_percent: 10,
                stop_temperature_c: 42.0,
                restart_temperature_c: 45.0,
                stall_rpm: 100,
                stall_grace_seconds: 3,
                full_speed_temperature_c: 95.0,
                emergency_handoff_temperature_c: 98.0,
                minimum_valid_temperature_c: 0.0,
                maximum_valid_temperature_c: 110.0,
                maximum_valid_rpm: 20_000,
            },
            presets: BTreeMap::from([(
                "Quiet".into(),
                vec![
                    CurvePoint {
                        temperature_c: 45.0,
                        percent: 0,
                    },
                    CurvePoint {
                        temperature_c: 55.0,
                        percent: 15,
                    },
                    CurvePoint {
                        temperature_c: 65.0,
                        percent: 25,
                    },
                    CurvePoint {
                        temperature_c: 75.0,
                        percent: 35,
                    },
                    CurvePoint {
                        temperature_c: 85.0,
                        percent: 55,
                    },
                    CurvePoint {
                        temperature_c: 90.0,
                        percent: 75,
                    },
                    CurvePoint {
                        temperature_c: 95.0,
                        percent: 100,
                    },
                ],
            )]),
        }
    }

    fn controller(fixture: &Fixture, backend: BackendPreference) -> FanController {
        let store = StateStore::open(fixture.state.clone()).unwrap();
        let mut controller =
            FanController::new("example".into(), config(backend), fixture.sysfs(), store).unwrap();
        controller.start().unwrap();
        controller
    }

    #[test]
    fn quiet_interpolation_zero_hysteresis_and_thermal_limits() {
        let fixture = Fixture::new(0);
        let mut controller = controller(&fixture, BackendPreference::Auto);
        controller.apply_preset("Quiet").unwrap();
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
        fs::remove_file(fixture.root.join("sys/class/hwmon/hwmon9/temp1_input")).unwrap();
        assert!(controller.tick().is_err());
        assert_eq!(read_integer(&fixture.fan("pwm1_enable")).unwrap(), 2);
    }

    #[test]
    fn hardware_curve_writes_and_verifies_every_point_before_enable() {
        let fixture = Fixture::new(3);
        let mut controller = controller(&fixture, BackendPreference::Auto);
        controller.apply_preset("Quiet").unwrap();
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
        assert_eq!(controller.snapshot().backend, "hardware_curve");
    }

    #[test]
    fn suspend_releases_and_resume_rediscovers_and_restores() {
        let fixture = Fixture::new(0);
        let mut controller = controller(&fixture, BackendPreference::Auto);
        controller.set_manual(30).unwrap();
        controller.prepare_for_sleep().unwrap();
        assert_eq!(read_integer(&fixture.fan("pwm1_enable")).unwrap(), 2);
        fs::rename(
            fixture.root.join("sys/class/hwmon/hwmon2"),
            fixture.root.join("sys/class/hwmon/hwmon7"),
        )
        .unwrap();
        controller.resume().unwrap();
        assert_eq!(
            read_integer(&fixture.root.join("sys/class/hwmon/hwmon7/pwm1_enable")).unwrap(),
            1
        );
        assert_eq!(controller.snapshot().effective_percent, Some(30));
    }

    #[test]
    fn failed_startup_forgets_saved_control_and_persists_automatic() {
        let fixture = Fixture::new(0);
        let store = StateStore::open(fixture.state.clone()).unwrap();
        store
            .set(
                "example/system",
                SavedFanState {
                    mode: SavedMode::Manual,
                    manual_percent: 40,
                    ..Default::default()
                },
            )
            .unwrap();
        let mut controller = FanController::new(
            "example".into(),
            config(BackendPreference::Auto),
            fixture.sysfs(),
            store,
        )
        .unwrap();
        fs::remove_file(fixture.root.join("sys/class/hwmon/hwmon9/temp1_input")).unwrap();
        assert!(controller.start().is_err());
        assert_eq!(read_integer(&fixture.fan("pwm1_enable")).unwrap(), 2);
        assert_eq!(controller.snapshot().mode, "automatic");
        let persisted = StateStore::open(fixture.state.clone()).unwrap();
        assert_eq!(persisted.get("example/system").mode, SavedMode::Automatic);
    }

    #[test]
    fn startup_and_resume_never_reacquire_control_at_the_handoff_temperature() {
        let startup = Fixture::new(0);
        startup.temperature(98.0);
        let store = StateStore::open(startup.state.clone()).unwrap();
        store
            .set(
                "example/system",
                SavedFanState {
                    mode: SavedMode::Manual,
                    manual_percent: 40,
                    ..Default::default()
                },
            )
            .unwrap();
        let mut at_start = FanController::new(
            "example".into(),
            config(BackendPreference::Auto),
            startup.sysfs(),
            store,
        )
        .unwrap();
        assert!(at_start.start().is_err());
        assert_eq!(read_integer(&startup.fan("pwm1_enable")).unwrap(), 2);
        assert_eq!(at_start.snapshot().mode, "automatic");

        let resume = Fixture::new(0);
        let mut after_sleep = controller(&resume, BackendPreference::Auto);
        after_sleep.set_manual(40).unwrap();
        after_sleep.prepare_for_sleep().unwrap();
        resume.temperature(98.0);
        assert!(after_sleep.resume().is_err());
        assert_eq!(read_integer(&resume.fan("pwm1_enable")).unwrap(), 2);
        assert_eq!(after_sleep.snapshot().mode, "automatic");
    }

    #[test]
    fn malformed_telemetry_and_write_failure_restore_automatic() {
        let fixture = Fixture::new(0);
        let mut controller = controller(&fixture, BackendPreference::Auto);
        controller.set_manual(20).unwrap();
        write_file(
            fixture.root.join("sys/class/hwmon/hwmon9/temp1_input"),
            "not-a-temperature\n",
        );
        assert!(controller.tick().is_err());
        assert_eq!(read_integer(&fixture.fan("pwm1_enable")).unwrap(), 2);

        write_file(
            fixture.root.join("sys/class/hwmon/hwmon9/temp1_input"),
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
        let old_fan = fixture.root.join("sys/class/hwmon/hwmon2");
        let new_fan = fixture.root.join("sys/class/hwmon/hwmon7");
        fs::rename(&old_fan, &new_fan).unwrap();
        fs::remove_file(fixture.root.join("sys/class/hwmon/hwmon9/temp1_input")).unwrap();

        assert!(controller.tick().is_err());
        assert_eq!(read_integer(&new_fan.join("pwm1_enable")).unwrap(), 2);
        assert_eq!(controller.snapshot().mode, "automatic");
    }

    #[test]
    fn hottest_configured_sensor_drives_the_curve() {
        let fixture = Fixture::new(0);
        let second = fixture.root.join("sys/class/hwmon/hwmon10");
        write_file(second.join("name"), "acpitz\n");
        write_file(second.join("temp2_label"), "Board\n");
        write_file(second.join("temp2_input"), "60000\n");
        let mut fan = config(BackendPreference::Auto);
        fan.temperature_sources.push(TemperatureSource {
            hwmon_name: "acpitz".into(),
            label: "Board".into(),
            required: true,
        });
        let store = StateStore::open(fixture.state.clone()).unwrap();
        let mut controller =
            FanController::new("example".into(), fan, fixture.sysfs(), store).unwrap();
        controller.start().unwrap();
        controller.apply_preset("Quiet").unwrap();
        assert_eq!(controller.snapshot().temperature_c, Some(60.0));
        assert_eq!(controller.snapshot().effective_percent, Some(20));
    }
}
