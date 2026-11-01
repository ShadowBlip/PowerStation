//! Shared fake hardware for fan tests; each test owns its temporary tree.
use super::config::*;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_ID: AtomicU64 = AtomicU64::new(0);

pub struct TempTree(PathBuf);

impl TempTree {
    pub fn new() -> Self {
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("powerstation-fan-{}-{id}", std::process::id()));
        fs::create_dir(&path).unwrap();
        Self(path)
    }

    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempTree {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

pub fn write(path: impl AsRef<Path>, value: &str) {
    let path = path.as_ref();
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, value).unwrap();
}

pub fn example_fan() -> FanConfig {
    FanConfig {
        id: "system".into(),
        name: "System Fan".into(),
        hwmon: HwmonSelector {
            name: "example_ec".into(),
            parent_driver: "example-ec".into(),
            channel: 1,
        },
        temperature_sources: vec![TemperatureSource {
            id: "cpu".into(),
            name: "CPU".into(),
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
            minimum_valid_temperature_c: 0.0,
            maximum_valid_temperature_c: 110.0,
            maximum_valid_rpm: 20_000,
        },
        firmware_curve_sensor: Some("cpu".into()),
    }
}

pub fn hwmon(root: &Path, config: &FanConfig, index: u8, points: usize, temperature: i64) {
    let fan = root.join(format!("class/hwmon/hwmon{index}"));
    write(fan.join("name"), &config.hwmon.name);
    write(
        fan.join("device/uevent"),
        &format!(
            "DRIVER={}\nDEVPATH=/devices/platform/{}\n",
            config.hwmon.parent_driver, config.hwmon.parent_driver
        ),
    );
    write(fan.join("pwm1"), "100\n");
    write(fan.join("pwm1_enable"), "2\n");
    write(fan.join("fan1_input"), "1200\n");
    for index in 1..=points {
        write(
            fan.join(format!("pwm1_auto_point{index}_temp")),
            &format!("{}000\n", 40 + index * 10),
        );
        write(
            fan.join(format!("pwm1_auto_point{index}_pwm")),
            &format!("{}\n", index * 30),
        );
    }
    let source = &config.temperature_sources[0];
    let sensor = root.join("class/hwmon/hwmon9");
    write(sensor.join("name"), &source.hwmon_name);
    write(sensor.join("temp1_label"), &source.label);
    write(sensor.join("temp1_input"), &format!("{temperature}\n"));
}
