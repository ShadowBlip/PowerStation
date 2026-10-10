use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::fmt::{Display, Formatter};
use std::fs;
use std::path::Path;

pub const FAN_SCHEMA_VERSION: u32 = 1;

#[derive(Debug)]
pub struct ConfigError(pub String);

impl Display for ConfigError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ConfigError {}

impl From<std::io::Error> for ConfigError {
    fn from(value: std::io::Error) -> Self {
        Self(value.to_string())
    }
}

impl From<serde_yaml::Error> for ConfigError {
    fn from(value: serde_yaml::Error) -> Self {
        Self(value.to_string())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FanDatabase {
    pub schema_version: u32,
    pub devices: Vec<DeviceConfig>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DeviceConfig {
    pub id: String,
    pub name: String,
    pub dmi: Vec<DmiMatch>,
    pub fans: Vec<FanConfig>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DmiMatch {
    pub sys_vendor: Option<String>,
    pub product_name: Option<String>,
    pub product_version: Option<String>,
    pub board_vendor: Option<String>,
    pub board_name: Option<String>,
    pub board_version: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DmiInfo {
    pub sys_vendor: Option<String>,
    pub product_name: Option<String>,
    pub product_version: Option<String>,
    pub board_vendor: Option<String>,
    pub board_name: Option<String>,
    pub board_version: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BackendPreference {
    #[default]
    Auto,
    SoftwareCurve,
    HardwareCurve,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ManualWriteOrder {
    #[default]
    EnableThenPwm,
    PwmThenEnable,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FanConfig {
    pub id: String,
    pub name: String,
    pub hwmon: HwmonSelector,
    pub temperature_sources: Vec<TemperatureSource>,
    #[serde(default)]
    pub backend: BackendPreference,
    pub mode_values: ModeValues,
    #[serde(default)]
    pub manual_write_order: ManualWriteOrder,
    pub limits: FanLimits,
    #[serde(default)]
    pub presets: BTreeMap<String, Vec<CurvePoint>>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct HwmonSelector {
    pub name: String,
    pub parent_driver: String,
    pub channel: u16,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TemperatureSource {
    pub hwmon_name: String,
    pub label: String,
    #[serde(default = "default_true")]
    pub required: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ModeValues {
    pub automatic: i64,
    pub manual: i64,
    pub hardware_curve: Option<i64>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FanLimits {
    pub pwm_min: u32,
    pub pwm_max: u32,
    pub pwm_readback_tolerance: u32,
    pub minimum_percent: u8,
    pub minimum_running_percent: u8,
    pub stop_temperature_c: f64,
    pub restart_temperature_c: f64,
    pub stall_rpm: u32,
    pub stall_grace_seconds: u32,
    pub full_speed_temperature_c: f64,
    pub emergency_handoff_temperature_c: f64,
    pub minimum_valid_temperature_c: f64,
    pub maximum_valid_temperature_c: f64,
    pub maximum_valid_rpm: u32,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CurvePoint {
    pub temperature_c: f64,
    pub percent: u8,
}

impl DmiInfo {
    pub fn read(sysfs_root: &Path) -> Self {
        let root = sysfs_root.join("class/dmi/id");
        let value = |name: &str| {
            fs::read_to_string(root.join(name))
                .ok()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
        };
        Self {
            sys_vendor: value("sys_vendor"),
            product_name: value("product_name"),
            product_version: value("product_version"),
            board_vendor: value("board_vendor"),
            board_name: value("board_name"),
            board_version: value("board_version"),
        }
    }
}

impl DmiMatch {
    pub fn is_empty(&self) -> bool {
        self.sys_vendor.is_none()
            && self.product_name.is_none()
            && self.product_version.is_none()
            && self.board_vendor.is_none()
            && self.board_name.is_none()
            && self.board_version.is_none()
    }

    pub fn matches(&self, actual: &DmiInfo) -> bool {
        fn matches_field(expected: &Option<String>, actual: &Option<String>) -> bool {
            expected
                .as_ref()
                .map(|expected| actual.as_ref() == Some(expected))
                .unwrap_or(true)
        }
        !self.is_empty()
            && matches_field(&self.sys_vendor, &actual.sys_vendor)
            && matches_field(&self.product_name, &actual.product_name)
            && matches_field(&self.product_version, &actual.product_version)
            && matches_field(&self.board_vendor, &actual.board_vendor)
            && matches_field(&self.board_name, &actual.board_name)
            && matches_field(&self.board_version, &actual.board_version)
    }
}

impl DeviceConfig {
    pub fn matches(&self, dmi: &DmiInfo) -> bool {
        self.dmi.iter().any(|candidate| candidate.matches(dmi))
    }
}

impl FanConfig {
    pub fn validate_curve(&self, name: &str, points: &[CurvePoint]) -> Result<(), ConfigError> {
        if points.len() < 2 {
            return Err(ConfigError(format!(
                "fan {} curve {name} must contain at least two points",
                self.id
            )));
        }
        let mut last_temperature = f64::NEG_INFINITY;
        let mut last_percent = 0;
        for (index, point) in points.iter().enumerate() {
            if !point.temperature_c.is_finite()
                || point.temperature_c < self.limits.minimum_valid_temperature_c
                || point.temperature_c > self.limits.full_speed_temperature_c
            {
                return Err(ConfigError(format!(
                    "fan {} curve {name} point {index} has an invalid temperature",
                    self.id
                )));
            }
            if point.temperature_c <= last_temperature {
                return Err(ConfigError(format!(
                    "fan {} curve {name} temperatures must strictly increase",
                    self.id
                )));
            }
            if index > 0 && point.percent < last_percent {
                return Err(ConfigError(format!(
                    "fan {} curve {name} duty must not decrease",
                    self.id
                )));
            }
            if point.percent < self.limits.minimum_percent {
                return Err(ConfigError(format!(
                    "fan {} curve {name} point {index} is below the {} percent device minimum",
                    self.id, self.limits.minimum_percent
                )));
            }
            last_temperature = point.temperature_c;
            last_percent = point.percent;
        }
        if points.last().map(|point| point.percent) != Some(100) {
            return Err(ConfigError(format!(
                "fan {} curve {name} must end at 100 percent",
                self.id
            )));
        }
        Ok(())
    }

    fn validate(&self) -> Result<(), ConfigError> {
        require_text("fan id", &self.id)?;
        require_text("fan name", &self.name)?;
        require_text("fan hwmon name", &self.hwmon.name)?;
        require_text("fan parent driver", &self.hwmon.parent_driver)?;
        if self.hwmon.channel == 0 {
            return Err(ConfigError(format!(
                "fan {} channel must be greater than zero",
                self.id
            )));
        }
        if self.temperature_sources.is_empty() {
            return Err(ConfigError(format!(
                "fan {} must define at least one temperature source",
                self.id
            )));
        }
        if !self
            .temperature_sources
            .iter()
            .any(|source| source.required)
        {
            return Err(ConfigError(format!(
                "fan {} must have at least one required temperature source",
                self.id
            )));
        }
        for source in &self.temperature_sources {
            require_text("temperature hwmon name", &source.hwmon_name)?;
            require_text("temperature label", &source.label)?;
        }
        let modes = &self.mode_values;
        if modes.automatic < 0
            || modes.manual < 0
            || modes.hardware_curve.is_some_and(|value| value < 0)
            || modes.automatic == modes.manual
            || modes.hardware_curve == Some(modes.automatic)
            || modes.hardware_curve == Some(modes.manual)
        {
            return Err(ConfigError(format!(
                "fan {} mode values must be distinct",
                self.id
            )));
        }
        if self.backend == BackendPreference::HardwareCurve && modes.hardware_curve.is_none() {
            return Err(ConfigError(format!(
                "fan {} requests a hardware curve without a hardware mode value",
                self.id
            )));
        }
        let limits = &self.limits;
        if limits.pwm_min >= limits.pwm_max {
            return Err(ConfigError(format!(
                "fan {} PWM minimum must be below its maximum",
                self.id
            )));
        }
        if limits.pwm_readback_tolerance >= limits.pwm_max - limits.pwm_min {
            return Err(ConfigError(format!(
                "fan {} PWM readback tolerance is invalid",
                self.id
            )));
        }
        if limits.minimum_percent > 100
            || limits.minimum_running_percent > 100
            || limits.minimum_running_percent == 0
            || limits.minimum_percent > limits.minimum_running_percent
        {
            return Err(ConfigError(format!(
                "fan {} percentage limits are invalid",
                self.id
            )));
        }
        if !limits.minimum_valid_temperature_c.is_finite()
            || !limits.stop_temperature_c.is_finite()
            || !limits.restart_temperature_c.is_finite()
            || !limits.full_speed_temperature_c.is_finite()
            || !limits.emergency_handoff_temperature_c.is_finite()
            || !limits.maximum_valid_temperature_c.is_finite()
            || !(limits.minimum_valid_temperature_c <= limits.stop_temperature_c
                && limits.stop_temperature_c < limits.restart_temperature_c
                && limits.restart_temperature_c < limits.full_speed_temperature_c
                && limits.full_speed_temperature_c < limits.emergency_handoff_temperature_c
                && limits.emergency_handoff_temperature_c <= limits.maximum_valid_temperature_c)
        {
            return Err(ConfigError(format!(
                "fan {} temperature limits are invalid",
                self.id
            )));
        }
        if limits.maximum_valid_rpm == 0
            || limits.stall_rpm == 0
            || limits.stall_rpm >= limits.maximum_valid_rpm
            || limits.stall_grace_seconds == 0
        {
            return Err(ConfigError(format!(
                "fan {} RPM and stall limits are invalid",
                self.id
            )));
        }
        for (name, points) in &self.presets {
            require_text("preset name", name)?;
            self.validate_curve(name, points)?;
        }
        Ok(())
    }
}

fn require_text(kind: &str, value: &str) -> Result<(), ConfigError> {
    if value.trim().is_empty() || value.trim() != value {
        return Err(ConfigError(format!("{kind} must be non-empty and trimmed")));
    }
    Ok(())
}

pub fn load_databases(platform_dir: &Path) -> Result<Vec<DeviceConfig>, ConfigError> {
    let directory = fs::read_dir(platform_dir).map_err(|error| {
        ConfigError(format!(
            "unable to read fan platform directory {}: {error}",
            platform_dir.display()
        ))
    })?;
    let mut paths = Vec::new();
    for entry in directory {
        let path = entry
            .map_err(|error| {
                ConfigError(format!(
                    "unable to enumerate fan platform directory {}: {error}",
                    platform_dir.display()
                ))
            })?
            .path();
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("");
        let extension = path.extension().and_then(|ext| ext.to_str()).unwrap_or("");
        if name.starts_with("fan-") && matches!(extension, "yaml" | "yml") {
            paths.push(path);
        }
    }
    paths.sort();

    let mut devices = Vec::new();
    for path in paths {
        let raw = fs::read_to_string(&path)
            .map_err(|error| ConfigError(format!("unable to read {}: {error}", path.display())))?;
        let database: FanDatabase = serde_yaml::from_str(&raw)
            .map_err(|error| ConfigError(format!("invalid {}: {error}", path.display())))?;
        if database.schema_version != FAN_SCHEMA_VERSION {
            return Err(ConfigError(format!(
                "{} uses unsupported fan schema version {}",
                path.display(),
                database.schema_version
            )));
        }
        devices.extend(database.devices);
    }
    validate_devices(&devices)?;
    Ok(devices)
}

pub fn select_device(
    devices: &[DeviceConfig],
    dmi: &DmiInfo,
) -> Result<Option<DeviceConfig>, ConfigError> {
    let matches: Vec<&DeviceConfig> = devices
        .iter()
        .filter(|device| device.matches(dmi))
        .collect();
    match matches.as_slice() {
        [] => Ok(None),
        [device] => Ok(Some((*device).clone())),
        _ => Err(ConfigError(format!(
            "ambiguous fan configuration: DMI matches {}",
            matches
                .iter()
                .map(|device| device.id.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ))),
    }
}

fn validate_devices(devices: &[DeviceConfig]) -> Result<(), ConfigError> {
    let mut device_ids = HashSet::new();
    for device in devices {
        require_text("device id", &device.id)?;
        require_text("device name", &device.name)?;
        if !device_ids.insert(device.id.clone()) {
            return Err(ConfigError(format!("duplicate device id {}", device.id)));
        }
        if device.dmi.is_empty() || device.dmi.iter().any(DmiMatch::is_empty) {
            return Err(ConfigError(format!(
                "device {} must have non-empty DMI alternatives",
                device.id
            )));
        }
        if device.fans.is_empty() {
            return Err(ConfigError(format!(
                "device {} must define at least one fan",
                device.id
            )));
        }
        let mut fan_ids = HashSet::new();
        for fan in &device.fans {
            fan.validate()?;
            if !fan_ids.insert(fan.id.clone()) {
                return Err(ConfigError(format!(
                    "device {} has duplicate fan id {}",
                    device.id, fan.id
                )));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn example_fan() -> FanConfig {
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
            backend: BackendPreference::Auto,
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
                        temperature_c: 95.0,
                        percent: 100,
                    },
                ],
            )]),
        }
    }

    fn devices() -> Vec<DeviceConfig> {
        vec![DeviceConfig {
            id: "example".into(),
            name: "Example".into(),
            dmi: vec![
                DmiMatch {
                    sys_vendor: Some("AYANEO".into()),
                    product_name: Some("AYANEO 3".into()),
                    ..Default::default()
                },
                DmiMatch {
                    board_vendor: Some("AYANEO".into()),
                    board_name: Some("AYANEO 3".into()),
                    ..Default::default()
                },
            ],
            fans: vec![example_fan()],
        }]
    }

    #[test]
    fn dmi_alternatives_are_or_and_fields_are_and() {
        let product = DmiInfo {
            sys_vendor: Some("AYANEO".into()),
            product_name: Some("AYANEO 3".into()),
            ..Default::default()
        };
        assert!(devices()[0].matches(&product));

        let board = DmiInfo {
            board_vendor: Some("AYANEO".into()),
            board_name: Some("AYANEO 3".into()),
            ..Default::default()
        };
        assert!(devices()[0].matches(&board));

        let wrong_product = DmiInfo {
            sys_vendor: Some("AYANEO".into()),
            product_name: Some("AYANEO 2".into()),
            ..Default::default()
        };
        assert!(!devices()[0].matches(&wrong_product));
    }

    #[test]
    fn ambiguous_dmi_is_rejected() {
        let mut candidates = devices();
        let mut duplicate = candidates[0].clone();
        duplicate.id = "other".into();
        candidates.push(duplicate);
        let dmi = DmiInfo {
            sys_vendor: Some("AYANEO".into()),
            product_name: Some("AYANEO 3".into()),
            ..Default::default()
        };
        let error = select_device(&candidates, &dmi).unwrap_err();
        assert!(error.to_string().contains("ambiguous"));
    }

    #[test]
    fn invalid_curve_and_limits_are_rejected() {
        let fan = example_fan();
        let descending = vec![
            CurvePoint {
                temperature_c: 55.0,
                percent: 50,
            },
            CurvePoint {
                temperature_c: 65.0,
                percent: 40,
            },
        ];
        assert!(fan.validate_curve("bad", &descending).is_err());

        let mut invalid = devices();
        invalid[0].fans[0].limits.emergency_handoff_temperature_c = 95.0;
        assert!(validate_devices(&invalid).is_err());

        let mut cannot_stop = example_fan();
        cannot_stop.limits.minimum_percent = 20;
        cannot_stop.limits.minimum_running_percent = 20;
        assert!(cannot_stop
            .validate_curve("zero is unsupported", &cannot_stop.presets["Quiet"])
            .is_err());

        let mut invalid_modes = devices();
        invalid_modes[0].fans[0].mode_values.automatic = -1;
        assert!(validate_devices(&invalid_modes).is_err());

        let mut invalid_stall = devices();
        invalid_stall[0].fans[0].limits.stall_rpm = 20_000;
        assert!(validate_devices(&invalid_stall).is_err());
    }

    #[test]
    fn unsupported_schema_and_unknown_fields_are_rejected() {
        let unsupported = "schema_version: 2\ndevices: []\n";
        let parsed: FanDatabase = serde_yaml::from_str(unsupported).unwrap();
        assert_ne!(parsed.schema_version, FAN_SCHEMA_VERSION);

        let unknown = "schema_version: 1\nunknown: true\ndevices: []\n";
        assert!(serde_yaml::from_str::<FanDatabase>(unknown).is_err());
    }

    #[test]
    fn packaged_ayaneo_configuration_loads_with_requested_policy() {
        let platform =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("rootfs/usr/share/powerstation/platform");
        let devices = load_databases(&platform).unwrap();
        let ayaneo = devices
            .iter()
            .find(|device| device.id == "ayaneo-3")
            .expect("packaged AYANEO 3 fan configuration");
        assert_eq!(ayaneo.dmi.len(), 2);
        let fan = &ayaneo.fans[0];
        assert_eq!(fan.hwmon.name, "ayaneo_ec");
        assert_eq!(fan.hwmon.parent_driver, "ayaneo-ec");
        assert_eq!(fan.limits.minimum_percent, 0);
        assert_eq!(fan.limits.minimum_running_percent, 10);
        assert_eq!(fan.limits.stop_temperature_c, 42.0);
        assert_eq!(fan.limits.restart_temperature_c, 45.0);
        assert_eq!(fan.limits.full_speed_temperature_c, 95.0);
        assert_eq!(fan.limits.emergency_handoff_temperature_c, 98.0);
        assert_eq!(fan.presets["Quiet"].len(), 7);
    }
}
