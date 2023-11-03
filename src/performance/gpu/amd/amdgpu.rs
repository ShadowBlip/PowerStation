use std::{
    fs::{self, OpenOptions},
    io::Write,
};
use zbus::{fdo, zvariant::ObjectPath};
use zbus_macros::dbus_interface;

use crate::performance::gpu::amd;
use crate::performance::gpu::DBusInterface;

pub struct AMDGPU {
    pub connector_paths: Vec<String>,
    pub name: String,
    pub path: String,
    pub class: String,
    pub class_id: String,
    pub vendor: String,
    pub vendor_id: String,
    pub device: String,
    pub device_id: String,
    pub device_type: String,
    pub subdevice: String,
    pub subdevice_id: String,
    pub subvendor_id: String,
    pub revision_id: String,
}

impl AMDGPU {
    /// Returns the TDP DBus interface for this GPU
    pub fn get_tdp_interface(&self) -> Option<amd::tdp::TDP> {
        match self.class.as_str() {
            "integrated" => Some(amd::tdp::TDP::new(self.path.clone())),
            _ => None,
        }
    }
}

#[dbus_interface(name = "org.shadowblip.GPU.Card")]
impl DBusInterface for AMDGPU {
    /// Returns a list of DBus paths to all connectors
    fn enumerate_connectors(&self) -> fdo::Result<Vec<ObjectPath>> {
        let mut paths: Vec<ObjectPath> = Vec::new();

        for path in &self.connector_paths {
            let path = ObjectPath::from_string_unchecked(path.clone());
            paths.push(path);
        }

        return Ok(paths);
    }

    #[dbus_interface(property)]
    fn name(&self) -> String {
        self.name.clone()
    }

    #[dbus_interface(property)]
    fn path(&self) -> String {
        self.path.clone()
    }

    #[dbus_interface(property)]
    fn class(&self) -> String {
        self.class.clone()
    }

    #[dbus_interface(property)]
    fn class_id(&self) -> String {
        self.class_id.clone()
    }

    #[dbus_interface(property)]
    fn vendor(&self) -> String {
        self.vendor.clone()
    }

    #[dbus_interface(property)]
    fn vendor_id(&self) -> String {
        self.vendor_id.clone()
    }

    #[dbus_interface(property)]
    fn device(&self) -> String {
        self.device.clone()
    }

    #[dbus_interface(property)]
    fn device_id(&self) -> String {
        self.device_id.clone()
    }

    #[dbus_interface(property)]
    fn subdevice(&self) -> String {
        self.subdevice.clone()
    }

    #[dbus_interface(property)]
    fn subdevice_id(&self) -> String {
        self.subdevice_id.clone()
    }

    #[dbus_interface(property)]
    fn subvendor_id(&self) -> String {
        self.subvendor_id.clone()
    }

    #[dbus_interface(property)]
    fn revision_id(&self) -> String {
        self.revision_id.clone()
    }

    #[dbus_interface(property)]
    fn clock_limit_mhz_min(&self) -> fdo::Result<f64> {
        let limits = get_clock_limits(self.path())
            // convert the std::io::Error to a zbus::fdo::Error
            .map_err(|err| fdo::Error::IOError(err.to_string()))?;

        let (min, _) = limits;
        return Ok(min);
    }

    #[dbus_interface(property)]
    fn clock_limit_mhz_max(&self) -> fdo::Result<f64> {
        let limits = get_clock_limits(self.path())
            // convert the std::io::Error to a zbus::fdo::Error
            .map_err(|err| fdo::Error::IOError(err.to_string()))?;

        let (_, max) = limits;
        return Ok(max);
    }

    #[dbus_interface(property)]
    fn clock_value_mhz_min(&self) -> fdo::Result<f64> {
        let values = get_clock_values(self.path())
            // convert the std::io::Error to a zbus::fdo::Error
            .map_err(|err| fdo::Error::IOError(err.to_string()))?;

        let (min, _) = values;
        return Ok(min);
    }

    #[dbus_interface(property)]
    fn set_clock_value_mhz_min(&mut self, value: f64) -> fdo::Result<()> {
        // Build the clock command to send
        // https://www.kernel.org/doc/html/v5.9/gpu/amdgpu.html#pp-od-clk-voltage
        let command = format!("s 0 {}\n", value);

        // Open the sysfs file to write to
        let path = format!("{0}/{1}", self.path(), "device/pp_od_clk_voltage");
        let file = OpenOptions::new().write(true).open(path.clone());

        // Write the value
        log::debug!(
            "Writing value '{}' to: {}",
            command.clone().trim(),
            path.clone()
        );
        file
            // convert the std::io::Error to a zbus::fdo::Error
            .map_err(|err| fdo::Error::Failed(err.to_string()))?
            .write_all(command.as_bytes())
            // convert the std::io::Error to a zbus::fdo::Error
            .map_err(|err| fdo::Error::IOError(err.to_string()))?;

        // Write the "commit" command
        log::debug!("Writing value '{}' to: {}", "c", path.clone());
        let file = OpenOptions::new().write(true).open(path);
        file
            // convert the std::io::Error to a zbus::fdo::Error
            .map_err(|err| fdo::Error::Failed(err.to_string()))?
            .write_all("c\n".as_bytes())
            // convert the std::io::Error to a zbus::fdo::Error
            .map_err(|err| fdo::Error::IOError(err.to_string()))?;

        return Ok(());
    }

    #[dbus_interface(property)]
    fn clock_value_mhz_max(&self) -> fdo::Result<f64> {
        let values = get_clock_values(self.path())
            // convert the std::io::Error to a zbus::fdo::Error
            .map_err(|err| fdo::Error::IOError(err.to_string()))?;

        let (_, max) = values;
        return Ok(max);
    }

    #[dbus_interface(property)]
    fn set_clock_value_mhz_max(&mut self, value: f64) -> fdo::Result<()> {
        // Build the clock command to send
        // https://www.kernel.org/doc/html/v5.9/gpu/amdgpu.html#pp-od-clk-voltage
        let command = format!("s 1 {}\n", value);

        // Open the sysfs file to write to
        let path = format!("{0}/{1}", self.path(), "device/pp_od_clk_voltage");
        let file = OpenOptions::new().write(true).open(path.clone());

        // Write the value
        file
            // convert the std::io::Error to a zbus::fdo::Error
            .map_err(|err| fdo::Error::Failed(err.to_string()))?
            .write_all(command.as_bytes())
            // convert the std::io::Error to a zbus::fdo::Error
            .map_err(|err| fdo::Error::IOError(err.to_string()))?;

        // Write the "commit" command
        let file = OpenOptions::new().write(true).open(path);
        file
            // convert the std::io::Error to a zbus::fdo::Error
            .map_err(|err| fdo::Error::Failed(err.to_string()))?
            .write_all("c\n".as_bytes())
            // convert the std::io::Error to a zbus::fdo::Error
            .map_err(|err| fdo::Error::IOError(err.to_string()))?;

        return Ok(());
    }

    #[dbus_interface(property)]
    fn manual_clock(&self) -> fdo::Result<bool> {
        let path = format!(
            "{0}/{1}",
            self.path(),
            "device/power_dpm_force_performance_level"
        );

        let result = fs::read_to_string(path);
        let status = result
            // convert the std::io::Error to a zbus::fdo::Error
            .map_err(|err| fdo::Error::IOError(err.to_string()))?
            .trim()
            .to_lowercase();

        return Ok(status == "manual");
    }

    #[dbus_interface(property)]
    fn set_manual_clock(&mut self, enabled: bool) -> fdo::Result<()> {
        let status = if enabled { "manual" } else { "auto" };

        // Open the sysfs file to write to
        let path = format!(
            "{0}/{1}",
            self.path(),
            "device/power_dpm_force_performance_level"
        );
        let file = OpenOptions::new().write(true).open(path);

        // Write the value
        file
            // convert the std::io::Error to a zbus::fdo::Error
            .map_err(|err| fdo::Error::Failed(err.to_string()))?
            .write_all(status.as_bytes())
            // convert the std::io::Error to a zbus::fdo::Error
            .map_err(|err| fdo::Error::IOError(err.to_string()))?;

        Ok(())
    }
}

/// Reads the pp_od_clk_voltage from sysfs and returns the OD_RANGE values.
/// This file will be empty if not in "manual" for pp_od_performance_level.
fn get_clock_limits(gpu_path: String) -> Result<(f64, f64), std::io::Error> {
    let path = format!("{0}/{1}", gpu_path, "device/pp_od_clk_voltage");
    let result = fs::read_to_string(path);
    let result = result?;
    let lines = result.split("\n");

    // Parse the output
    let mut min: Option<f64> = None;
    let mut max: Option<f64> = None;
    for line in lines {
        let mut parts = line.trim().split_whitespace();
        let part1 = parts.next();
        if !part1.is_some_and(|part| part == "SCLK:") {
            continue;
        }

        let part2 = parts.next();
        if part2.is_none() {
            continue;
        }
        let parsed2 = part2.unwrap().trim_end_matches("Mhz").parse::<f64>();
        if parsed2.is_err() {
            continue;
        }
        min = Some(parsed2.unwrap());

        let part3 = parts.next();
        if part3.is_none() {
            continue;
        }
        let parsed3 = part3.unwrap().trim_end_matches("Mhz").parse::<f64>();
        if parsed3.is_err() {
            continue;
        }
        max = Some(parsed3.unwrap());
    }

    if min.is_none() || max.is_none() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "No limits found",
        ));
    }

    return Ok((min.unwrap(), max.unwrap()));
}

/// Reads the pp_od_clk_voltage from sysfs and returns the OD_SCLK values. This file will
/// be empty if not in "manual" for pp_od_performance_level.
fn get_clock_values(gpu_path: String) -> Result<(f64, f64), std::io::Error> {
    let path = format!("{0}/{1}", gpu_path, "device/pp_od_clk_voltage");
    let result = fs::read_to_string(path);
    let result = result?;
    let lines = result.split("\n");

    // Parse the output
    let mut min: Option<f64> = None;
    let mut max: Option<f64> = None;
    for line in lines {
        let mut parts = line.trim().split_whitespace();
        let part1 = parts.next();
        if !part1.is_some_and(|part| part == "0:" || part == "1:") {
            continue;
        }
        let kind = part1.unwrap();

        let part2 = parts.next();
        if part2.is_none() {
            continue;
        }
        let parsed2 = part2.unwrap().trim_end_matches("Mhz").parse::<f64>();
        if parsed2.is_err() {
            continue;
        }

        match kind {
            "0:" => min = Some(parsed2.unwrap()),
            "1:" => max = Some(parsed2.unwrap()),
            _ => continue,
        }
    }

    if min.is_none() || max.is_none() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "No limits found",
        ));
    }

    return Ok((min.unwrap(), max.unwrap()));
}
