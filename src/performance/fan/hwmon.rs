use std::{
    fs,
    path::{Path, PathBuf},
};

use super::FanError;

const HWMON_PATH: &str = "/sys/class/hwmon";

/// Value of pwmN_enable for manual control
pub const PWM_MANUAL: u32 = 1;
/// Value of pwmN_enable for firmware control
pub const PWM_AUTOMATIC: u32 = 2;

/// A fan that can be controlled through hwmon pwmN and pwmN_enable
#[derive(Debug, Clone)]
pub struct Fan {
    /// Stable ID used in profiles, e.g. "ayaneo_ec/fan1"
    pub id: String,
    pwm: PathBuf,
    enable: PathBuf,
    /// Firmware curve points as (temperature, pwm) attribute paths
    auto_points: Vec<(PathBuf, PathBuf)>,
}

/// A temperature sensor exposed through hwmon tempN_input
#[derive(Debug, Clone)]
pub struct Sensor {
    /// Stable ID used in profiles, e.g. "k10temp/Tctl"
    pub id: String,
    input: PathBuf,
}

impl Fan {
    pub fn firmware_curve_points(&self) -> usize {
        self.auto_points.len()
    }

    pub fn percent(&self) -> Result<u8, FanError> {
        let raw = read_number(&self.pwm)?.min(255);
        Ok(((raw * 100 + 127) / 255) as u8)
    }

    pub fn set_percent(&self, percent: u8) -> Result<(), FanError> {
        write_number(&self.pwm, percent_to_pwm(percent))
    }

    pub fn enable(&self) -> Result<u32, FanError> {
        read_number(&self.enable)
    }

    pub fn set_enable(&self, value: u32) -> Result<(), FanError> {
        write_number(&self.enable, value)
    }

    /// Program the firmware curve. Temperatures are in °C, values in percent.
    pub fn set_firmware_curve(&self, curve: &[(u8, u8)]) -> Result<(), FanError> {
        for ((temp_path, pwm_path), (temp, percent)) in self.auto_points.iter().zip(curve) {
            write_number(temp_path, *temp as u32 * 1000)?;
            write_number(pwm_path, percent_to_pwm(*percent))?;
        }
        Ok(())
    }
}

impl Sensor {
    /// Returns the temperature in °C
    pub fn temperature(&self) -> Result<f64, FanError> {
        let millidegrees: i64 = fs::read_to_string(&self.input)?.trim().parse()?;
        Ok(millidegrees as f64 / 1000.0)
    }
}

/// Discover all controllable fans and temperature sensors
pub fn discover() -> (Vec<Fan>, Vec<Sensor>) {
    let mut fans: Vec<Fan> = Vec::new();
    let mut sensors: Vec<Sensor> = Vec::new();

    for dir in sorted_entries(Path::new(HWMON_PATH)) {
        let Some(name) = read_trimmed(&dir.join("name")) else {
            continue;
        };

        for entry in sorted_entries(&dir) {
            let Some(file_name) = entry.file_name().and_then(|n| n.to_str()) else {
                continue;
            };

            if let Some(index) = attribute_index(file_name, "pwm", "") {
                let enable = dir.join(format!("pwm{index}_enable"));
                if !enable.exists() {
                    continue;
                }
                let id = format!("{name}/fan{index}");
                if fans.iter().any(|fan| fan.id == id) {
                    log::warn!("Ignoring duplicate fan {id} at {}", dir.display());
                    continue;
                }
                log::info!("Discovered fan {id}");
                fans.push(Fan {
                    id,
                    pwm: entry.clone(),
                    enable,
                    auto_points: auto_points(&dir, index),
                });
                continue;
            }

            if let Some(index) = attribute_index(file_name, "temp", "_input") {
                let label = read_trimmed(&dir.join(format!("temp{index}_label")))
                    .unwrap_or(format!("temp{index}"));
                let id = format!("{name}/{label}");
                if sensors.iter().any(|sensor| sensor.id == id) {
                    log::warn!("Ignoring duplicate sensor {id} at {}", dir.display());
                    continue;
                }
                sensors.push(Sensor {
                    id,
                    input: entry.clone(),
                });
            }
        }
    }

    (fans, sensors)
}

fn auto_points(dir: &Path, index: u32) -> Vec<(PathBuf, PathBuf)> {
    let mut points = Vec::new();
    for point in 1.. {
        let temp = dir.join(format!("pwm{index}_auto_point{point}_temp"));
        let pwm = dir.join(format!("pwm{index}_auto_point{point}_pwm"));
        if !temp.exists() || !pwm.exists() {
            break;
        }
        points.push((temp, pwm));
    }
    points
}

/// Returns N for file names like "{prefix}N{suffix}"
fn attribute_index(file_name: &str, prefix: &str, suffix: &str) -> Option<u32> {
    file_name
        .strip_prefix(prefix)?
        .strip_suffix(suffix)?
        .parse()
        .ok()
}

fn percent_to_pwm(percent: u8) -> u32 {
    (percent.min(100) as u32 * 255 + 50) / 100
}

fn sorted_entries(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut paths: Vec<PathBuf> = entries.flatten().map(|entry| entry.path()).collect();
    paths.sort();
    paths
}

fn read_trimmed(path: &Path) -> Option<String> {
    let value = fs::read_to_string(path).ok()?;
    Some(value.trim().to_string())
}

fn read_number(path: &Path) -> Result<u32, FanError> {
    Ok(fs::read_to_string(path)?.trim().parse()?)
}

fn write_number(path: &Path, value: u32) -> Result<(), FanError> {
    Ok(fs::write(path, value.to_string())?)
}
