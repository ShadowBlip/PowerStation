use std::{
    collections::{BTreeMap, HashMap},
    ffi::OsStr,
    fs::{self, File},
    io::Read,
    path::{Path, PathBuf},
};

use serde::Deserialize;

use super::FanError;

const MAX_PROFILE_SIZE: u64 = 64 * 1024;
const DEFAULT_CURVES_DIR: &str = "/usr/share/powerstation/platform/fan_curves";
const OVERRIDE_CURVES_DIR: &str = "/etc/powerstation/platform/fan_curves.d";
const DMI_PATH: &str = "/sys/class/dmi/id";
const DEVICE_TREE_PATH: &str = "/proc/device-tree/compatible";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    Automatic,
    FirmwareCurve,
    Manual,
    SoftwareCurve,
}

impl Mode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Mode::Automatic => "automatic",
            Mode::FirmwareCurve => "firmware_curve",
            Mode::Manual => "manual",
            Mode::SoftwareCurve => "software_curve",
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    pub name: String,
    #[serde(default, rename = "match")]
    pub device: Option<DeviceMatch>,
    #[serde(default)]
    pub fans: Vec<FanProfile>,
}

/// Devices a profile was written for. Matches if all DMI fields match or any
/// device tree compatible string matches.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeviceMatch {
    #[serde(default)]
    pub dmi: HashMap<String, String>,
    #[serde(default)]
    pub device_tree: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FanProfile {
    pub fan: String,
    pub mode: Mode,
    #[serde(default)]
    pub percent: Option<u8>,
    #[serde(default)]
    pub sensors: Vec<String>,
    /// List of (temperature in °C, fan percent) points
    #[serde(default)]
    pub curve: Vec<(u8, u8)>,
}

impl Profile {
    /// Read a TOML profile from an open file
    pub fn from_file(file: File) -> Result<Self, FanError> {
        let metadata = file.metadata()?;
        if !metadata.is_file() {
            return Err(FanError::InvalidProfile("not a regular file".into()));
        }
        let mut text = String::new();
        file.take(MAX_PROFILE_SIZE + 1).read_to_string(&mut text)?;
        if text.len() as u64 > MAX_PROFILE_SIZE {
            return Err(FanError::InvalidProfile(
                "file is larger than 64 KiB".into(),
            ));
        }
        Ok(toml::from_str(&text)?)
    }

    pub fn from_json(json: &str) -> Result<Self, FanError> {
        if json.len() as u64 > MAX_PROFILE_SIZE {
            return Err(FanError::InvalidProfile(
                "JSON is larger than 64 KiB".into(),
            ));
        }
        Ok(serde_json::from_str(json)?)
    }

    pub fn matches_device(&self) -> bool {
        let Some(device) = &self.device else {
            return true;
        };
        device.matches()
    }
}

impl DeviceMatch {
    pub fn matches(&self) -> bool {
        let dmi_matches = !self.dmi.is_empty()
            && self.dmi.iter().all(|(key, value)| {
                if !key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                    return false;
                }
                let path = PathBuf::from(DMI_PATH).join(key);
                fs::read_to_string(path).is_ok_and(|actual| actual.trim() == value)
            });
        if dmi_matches {
            return true;
        }

        let Ok(compatible) = fs::read(DEVICE_TREE_PATH) else {
            return false;
        };
        compatible
            .split(|byte| *byte == 0)
            .filter_map(|entry| std::str::from_utf8(entry).ok())
            .any(|entry| self.device_tree.iter().any(|expected| expected == entry))
    }
}

/// Check that a curve has at least two points, rising temperatures and
/// non-decreasing percentages from 0 to 100.
pub fn validate_curve(curve: &[(u8, u8)]) -> Result<(), FanError> {
    if curve.len() < 2 {
        return Err(FanError::InvalidProfile(
            "a curve needs at least two points".into(),
        ));
    }
    for pair in curve.windows(2) {
        let ((temp_a, percent_a), (temp_b, percent_b)) = (pair[0], pair[1]);
        if temp_b <= temp_a {
            return Err(FanError::InvalidProfile(
                "curve temperatures must increase".into(),
            ));
        }
        if percent_b < percent_a {
            return Err(FanError::InvalidProfile(
                "curve percentages must not decrease".into(),
            ));
        }
    }
    if curve.iter().any(|(_, percent)| *percent > 100) {
        return Err(FanError::InvalidProfile(
            "curve percentages must be 0-100".into(),
        ));
    }
    Ok(())
}

/// Returns the fan percent for the given temperature using linear
/// interpolation between curve points.
pub fn interpolate(curve: &[(u8, u8)], temperature: f64) -> u8 {
    let Some(&(first_temp, first_percent)) = curve.first() else {
        return 100;
    };
    if temperature <= first_temp as f64 {
        return first_percent;
    }
    for pair in curve.windows(2) {
        let ((temp_a, percent_a), (temp_b, percent_b)) = (pair[0], pair[1]);
        if temperature > temp_b as f64 {
            continue;
        }
        let ratio = (temperature - temp_a as f64) / (temp_b as f64 - temp_a as f64);
        let percent = percent_a as f64 + ratio * (percent_b as f64 - percent_a as f64);
        return percent.round() as u8;
    }
    curve.last().map(|(_, percent)| *percent).unwrap_or(100)
}

/// Returns the paths of packaged profiles for this device. A file in
/// /etc/powerstation/platform/fan_curves.d replaces a packaged file with the
/// same name.
pub fn packaged_profiles() -> Vec<String> {
    let mut files: BTreeMap<String, PathBuf> = BTreeMap::new();
    for dir in [curves_dir(), PathBuf::from(OVERRIDE_CURVES_DIR)] {
        let Ok(entries) = fs::read_dir(dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension() != Some(OsStr::new("toml")) {
                continue;
            }
            files.insert(entry.file_name().to_string_lossy().to_string(), path);
        }
    }

    let mut profiles = Vec::new();
    for path in files.into_values() {
        match read_profile(&path) {
            Ok(profile) if profile.matches_device() => {
                profiles.push(path.display().to_string());
            }
            Ok(_) => {}
            Err(e) => log::warn!("Ignoring fan profile {}: {e}", path.display()),
        }
    }
    profiles
}

fn read_profile(path: &Path) -> Result<Profile, FanError> {
    Profile::from_file(File::open(path)?)
}

/// Returns the packaged fan curve directory (e.g.
/// "/usr/share/powerstation/platform/fan_curves")
fn curves_dir() -> PathBuf {
    let base_dirs = xdg::BaseDirectories::with_prefix("powerstation");
    for dir in base_dirs.get_data_dirs() {
        let curves_dir = dir.join("platform").join("fan_curves");
        if curves_dir.exists() {
            return curves_dir;
        }
    }
    PathBuf::from(DEFAULT_CURVES_DIR)
}
