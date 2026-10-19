use super::config::{BackendPreference, FanConfig, TemperatureSource};
use super::FanError;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HardwareCurvePointPaths {
    pub index: u16,
    pub temperature: PathBuf,
    pub pwm: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BackendKind {
    SoftwareCurve,
    HardwareCurve(Vec<HardwareCurvePointPaths>),
}

impl BackendKind {
    pub fn name(&self) -> &'static str {
        match self {
            Self::SoftwareCurve => "software_curve",
            Self::HardwareCurve(_) => "hardware_curve",
        }
    }

    pub fn curve_point_count(&self) -> u16 {
        match self {
            Self::SoftwareCurve => 0,
            Self::HardwareCurve(points) => points.len() as u16,
        }
    }
}

#[derive(Debug, Clone)]
pub struct FanPaths {
    pub device_identity: String,
    pub pwm: PathBuf,
    pub enable: PathBuf,
    pub rpm: PathBuf,
    pub temperature_inputs: BTreeMap<String, PathBuf>,
    pub backend: BackendKind,
}

pub fn discover_fan(sysfs_root: &Path, config: &FanConfig) -> Result<FanPaths, FanError> {
    let hwmon_root = sysfs_root.join("class/hwmon");
    let entries = read_hwmon_entries(&hwmon_root)?;
    let fan_dir = discover_fan_directory(&entries, config)?;

    let channel = config.hwmon.channel;
    let pwm = checked_attribute(&fan_dir, &format!("pwm{channel}"))?;
    let enable = checked_attribute(&fan_dir, &format!("pwm{channel}_enable"))?;
    let rpm = checked_attribute(&fan_dir, &format!("fan{channel}_input"))?;
    let temperature_inputs = discover_temperatures(&entries, &config.temperature_sources)?;
    let auto_points = discover_auto_points(&fan_dir, channel)?;
    let backend = select_backend(config, auto_points)?;
    let device_identity = device_identity(&fan_dir);

    Ok(FanPaths {
        device_identity,
        pwm,
        enable,
        rpm,
        temperature_inputs,
        backend,
    })
}

/// Find only the mode attribute needed for an emergency firmware-auto restore.
/// This deliberately does not depend on PWM, RPM, temperature, or auto-point
/// attributes, because those may be the reason the full discovery path failed.
pub fn discover_fan_enable(sysfs_root: &Path, config: &FanConfig) -> Result<PathBuf, FanError> {
    let entries = read_hwmon_entries(&sysfs_root.join("class/hwmon"))?;
    let fan_dir = discover_fan_directory(&entries, config)?;
    checked_attribute(&fan_dir, &format!("pwm{}_enable", config.hwmon.channel))
}

fn discover_fan_directory(entries: &[PathBuf], config: &FanConfig) -> Result<PathBuf, FanError> {
    let mut fan_matches = Vec::new();
    for path in entries {
        if read_trimmed(path.join("name")).as_deref() != Some(config.hwmon.name.as_str()) {
            continue;
        }
        if driver_name(path).as_deref() != Some(config.hwmon.parent_driver.as_str()) {
            continue;
        }
        fan_matches.push(path.clone());
    }
    let fan_dir = exactly_one(
        fan_matches,
        &format!(
            "fan hwmon name={} driver={}",
            config.hwmon.name, config.hwmon.parent_driver
        ),
    )?;
    Ok(canonical_or_original(&fan_dir))
}

fn read_hwmon_entries(root: &Path) -> Result<Vec<PathBuf>, FanError> {
    let directory = fs::read_dir(root).map_err(|error| {
        FanError(format!(
            "unable to read hwmon directory {}: {error}",
            root.display()
        ))
    })?;
    let mut entries = Vec::new();
    for entry in directory {
        let path = entry
            .map_err(|error| {
                FanError(format!(
                    "unable to enumerate hwmon directory {}: {error}",
                    root.display()
                ))
            })?
            .path();
        if path.is_dir() {
            entries.push(path);
        }
    }
    entries.sort();
    Ok(entries)
}

fn discover_temperatures(
    entries: &[PathBuf],
    sources: &[TemperatureSource],
) -> Result<BTreeMap<String, PathBuf>, FanError> {
    let mut inputs = BTreeMap::new();
    for source in sources {
        let mut matches = Vec::new();
        for path in entries {
            if read_trimmed(path.join("name")).as_deref() != Some(source.hwmon_name.as_str()) {
                continue;
            }
            let directory = fs::read_dir(path).map_err(|error| {
                FanError(format!(
                    "unable to inspect temperature hwmon {}: {error}",
                    path.display()
                ))
            })?;
            for entry in directory {
                let entry = entry.map_err(|error| {
                    FanError(format!(
                        "unable to enumerate temperature hwmon {}: {error}",
                        path.display()
                    ))
                })?;
                let name = entry.file_name();
                let name = name.to_string_lossy();
                let Some(number) = name
                    .strip_prefix("temp")
                    .and_then(|name| name.strip_suffix("_label"))
                else {
                    continue;
                };
                if number.is_empty() || !number.bytes().all(|byte| byte.is_ascii_digit()) {
                    continue;
                }
                if read_trimmed(entry.path()).as_deref() != Some(source.label.as_str()) {
                    continue;
                }
                if !path.join(format!("temp{number}_input")).exists() {
                    continue;
                }
                let input = checked_attribute(
                    &canonical_or_original(path),
                    &format!("temp{number}_input"),
                )?;
                matches.push(input);
            }
        }
        matches.sort();
        matches.dedup();
        if matches.is_empty() {
            continue;
        }
        inputs.insert(
            source.id.clone(),
            exactly_one(
                matches,
                &format!(
                    "temperature hwmon name={} label={}",
                    source.hwmon_name, source.label
                ),
            )?,
        );
    }
    Ok(inputs)
}

fn discover_auto_points(
    fan_dir: &Path,
    channel: u16,
) -> Result<Option<Vec<HardwareCurvePointPaths>>, FanError> {
    let prefix = format!("pwm{channel}_auto_point");
    let mut temperatures = BTreeMap::new();
    let mut pwms = BTreeMap::new();
    let directory = fs::read_dir(fan_dir).map_err(|error| {
        FanError(format!(
            "unable to inspect fan hwmon {}: {error}",
            fan_dir.display()
        ))
    })?;
    for entry in directory {
        let entry = entry.map_err(|error| {
            FanError(format!(
                "unable to enumerate fan hwmon {}: {error}",
                fan_dir.display()
            ))
        })?;
        let name = entry.file_name().to_string_lossy().to_string();
        let Some(rest) = name.strip_prefix(&prefix) else {
            continue;
        };
        let (number, kind) = if let Some(number) = rest.strip_suffix("_temp") {
            (number, "temp")
        } else if let Some(number) = rest.strip_suffix("_pwm") {
            (number, "pwm")
        } else {
            continue;
        };
        let Ok(index) = number.parse::<u16>() else {
            continue;
        };
        if index == 0 {
            return Err(FanError(
                "hardware fan curve point indexes must start at 1".into(),
            ));
        }
        let target = checked_attribute(fan_dir, &name)?;
        let previous = if kind == "temp" {
            temperatures.insert(index, target)
        } else {
            pwms.insert(index, target)
        };
        if previous.is_some() {
            return Err(FanError(format!(
                "duplicate hardware curve point {index} {kind} attribute"
            )));
        }
    }

    if temperatures.is_empty() && pwms.is_empty() {
        return Ok(None);
    }
    let temperature_indexes: BTreeSet<u16> = temperatures.keys().copied().collect();
    let pwm_indexes: BTreeSet<u16> = pwms.keys().copied().collect();
    if temperature_indexes != pwm_indexes {
        return Err(FanError(format!(
            "partial hardware curve attributes: temperature points {temperature_indexes:?}, PWM points {pwm_indexes:?}"
        )));
    }
    let maximum = *temperature_indexes.last().unwrap();
    let expected: BTreeSet<u16> = (1..=maximum).collect();
    if temperature_indexes != expected {
        return Err(FanError(format!(
            "hardware curve points are not contiguous: {temperature_indexes:?}"
        )));
    }

    Ok(Some(
        (1..=maximum)
            .map(|index| HardwareCurvePointPaths {
                index,
                temperature: temperatures.remove(&index).unwrap(),
                pwm: pwms.remove(&index).unwrap(),
            })
            .collect(),
    ))
}

fn select_backend(
    config: &FanConfig,
    auto_points: Option<Vec<HardwareCurvePointPaths>>,
) -> Result<BackendKind, FanError> {
    if auto_points.as_ref().is_some_and(|points| points.len() < 2) {
        return Err(FanError(
            "hardware curve backend requires at least two complete auto points".into(),
        ));
    }
    if config.backend == BackendPreference::HardwareCurve
        && (auto_points.is_none()
            || config.mode_values.hardware_curve.is_none()
            || config.firmware_curve_sensor.is_none())
    {
        return Err(FanError(
            "native curve requested without complete attributes and a trusted sensor binding"
                .into(),
        ));
    }
    match auto_points {
        Some(points)
            if config.backend != BackendPreference::SoftwareCurve
                && config.mode_values.hardware_curve.is_some()
                && config.firmware_curve_sensor.is_some() =>
        {
            Ok(BackendKind::HardwareCurve(points))
        }
        _ => Ok(BackendKind::SoftwareCurve),
    }
}

fn driver_name(hwmon: &Path) -> Option<String> {
    let driver = hwmon.join("device/driver");
    if let Ok(path) = fs::canonicalize(&driver) {
        if let Some(name) = path.file_name().and_then(|name| name.to_str()) {
            return Some(name.to_string());
        }
    }
    if let Ok(path) = fs::read_link(&driver) {
        if let Some(name) = path.file_name().and_then(|name| name.to_str()) {
            return Some(name.to_string());
        }
    }
    uevent_value(&hwmon.join("device/uevent"), "DRIVER")
}

fn device_identity(hwmon: &Path) -> String {
    if let Some(devpath) = uevent_value(&hwmon.join("device/uevent"), "DEVPATH") {
        return devpath;
    }
    fs::canonicalize(hwmon.join("device"))
        .or_else(|_| fs::canonicalize(hwmon))
        .unwrap_or_else(|_| hwmon.to_path_buf())
        .to_string_lossy()
        .to_string()
}

fn uevent_value(path: &Path, key: &str) -> Option<String> {
    let raw = fs::read_to_string(path).ok()?;
    raw.lines().find_map(|line| {
        let (candidate, value) = line.split_once('=')?;
        (candidate == key).then(|| value.trim().to_string())
    })
}

fn checked_attribute(directory: &Path, name: &str) -> Result<PathBuf, FanError> {
    let path = directory.join(name);
    if !path.exists() {
        return Err(FanError(format!(
            "required hwmon attribute {} is missing",
            path.display()
        )));
    }
    let canonical_directory = canonical_or_original(directory);
    let canonical_path = canonical_or_original(&path);
    if !canonical_path.starts_with(&canonical_directory) {
        return Err(FanError(format!(
            "hwmon attribute {} resolves outside {}",
            path.display(),
            directory.display()
        )));
    }
    Ok(canonical_path)
}

fn exactly_one<T>(mut values: Vec<T>, description: &str) -> Result<T, FanError> {
    match values.len() {
        1 => Ok(values.remove(0)),
        0 => Err(FanError(format!("no {description} match found"))),
        count => Err(FanError(format!(
            "ambiguous {description}: found {count} matches"
        ))),
    }
}

fn canonical_or_original(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

fn read_trimmed(path: PathBuf) -> Option<String> {
    fs::read_to_string(path)
        .ok()
        .map(|value| value.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::super::test_support::{example_fan as config, hwmon, write, TempTree};
    use super::*;

    fn fixture(tree: &TempTree, fan_index: u8) {
        hwmon(tree.path(), &config(), fan_index, 0, 42000);
    }

    #[test]
    fn safely_discovers_software_curve_and_survives_index_change() {
        let tree = TempTree::new();
        fixture(&tree, 2);
        let first = discover_fan(tree.path(), &config()).unwrap();
        assert_eq!(first.backend, BackendKind::SoftwareCurve);
        assert_eq!(first.temperature_inputs.len(), 1);

        fs::rename(
            tree.path().join("class/hwmon/hwmon2"),
            tree.path().join("class/hwmon/hwmon7"),
        )
        .unwrap();
        let second = discover_fan(tree.path(), &config()).unwrap();
        assert_eq!(first.device_identity, second.device_identity);
        assert!(second.pwm.to_string_lossy().contains("hwmon7"));
    }

    #[test]
    fn duplicate_hwmon_match_is_rejected() {
        let tree = TempTree::new();
        fixture(&tree, 2);
        let source = tree.path().join("class/hwmon/hwmon2");
        let duplicate = tree.path().join("class/hwmon/hwmon3");
        fs::create_dir_all(&duplicate).unwrap();
        for path in ["name", "device/uevent", "pwm1", "pwm1_enable", "fan1_input"] {
            write(
                duplicate.join(path),
                &fs::read_to_string(source.join(path)).unwrap(),
            );
        }
        let error = discover_fan(tree.path(), &config()).unwrap_err();
        assert!(error.to_string().contains("ambiguous fan hwmon"));
    }

    #[test]
    fn complete_hardware_points_are_selected() {
        let tree = TempTree::new();
        fixture(&tree, 2);
        let fan = tree.path().join("class/hwmon/hwmon2");
        for index in 1..=3 {
            write(
                fan.join(format!("pwm1_auto_point{index}_temp")),
                &format!("{}000\n", 40 + index * 10),
            );
            write(
                fan.join(format!("pwm1_auto_point{index}_pwm")),
                &format!("{}\n", index * 50),
            );
        }
        let mut config = config();
        config.limits.minimum_percent = config.limits.minimum_running_percent;
        let paths = discover_fan(tree.path(), &config).unwrap();
        assert_eq!(paths.backend.curve_point_count(), 3);
        assert_eq!(paths.backend.name(), "hardware_curve");
    }

    #[test]
    fn emergency_enable_discovery_ignores_missing_telemetry_and_partial_points() {
        let tree = TempTree::new();
        fixture(&tree, 2);
        let fan = tree.path().join("class/hwmon/hwmon2");
        fs::remove_file(tree.path().join("class/hwmon/hwmon9/temp1_input")).unwrap();
        write(fan.join("pwm1_auto_point1_temp"), "50000\n");

        assert!(discover_fan(tree.path(), &config()).is_err());
        assert_eq!(
            discover_fan_enable(tree.path(), &config()).unwrap(),
            canonical_or_original(&fan.join("pwm1_enable"))
        );
    }

    #[test]
    fn a_single_hardware_point_is_not_a_usable_curve_backend() {
        let tree = TempTree::new();
        fixture(&tree, 2);
        let fan = tree.path().join("class/hwmon/hwmon2");
        write(fan.join("pwm1_auto_point1_temp"), "50000\n");
        write(fan.join("pwm1_auto_point1_pwm"), "100\n");
        let error = discover_fan(tree.path(), &config()).unwrap_err();
        assert!(error.to_string().contains("at least two"));
    }

    #[test]
    fn partial_or_noncontiguous_hardware_points_are_rejected() {
        let tree = TempTree::new();
        fixture(&tree, 2);
        let fan = tree.path().join("class/hwmon/hwmon2");
        write(fan.join("pwm1_auto_point1_temp"), "50000\n");
        write(fan.join("pwm1_auto_point1_pwm"), "100\n");
        write(fan.join("pwm1_auto_point3_temp"), "90000\n");
        let error = discover_fan(tree.path(), &config()).unwrap_err();
        assert!(error.to_string().contains("partial hardware curve"));
    }
}
