use std::{
    fs::{self, OpenOptions},
    io::Write,
};

use zbus::fdo;
use zbus_macros::dbus_interface;

use crate::performance::gpu::intel;
use crate::performance::gpu::DBusInterface;

pub struct IntelGPU {
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
    pub manual_clock: bool,
}

impl IntelGPU {
    /// Returns the TDP DBus interface for this GPU
    pub fn get_tdp_interface(&self) -> Option<intel::tdp::TDP> {
        match self.class.as_str() {
            "integrated" => Some(intel::tdp::TDP::new(self.path.clone())),
            _ => None,
        }
    }
}

#[dbus_interface(name = "org.shadowblip.GPU")]
impl DBusInterface for IntelGPU {
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
        let path = format!("{0}/{1}", self.path(), "gt_RPn_freq_mhz");
        let result = fs::read_to_string(path);
        let limit = result
            // convert the std::io::Error to a zbus::fdo::Error
            .map_err(|err| fdo::Error::IOError(err.to_string()))?
            .trim()
            .parse::<f64>()
            // convert the ParseIntError to a zbus::fdo::Error
            .map_err(|err| fdo::Error::Failed(err.to_string()))?;

        return Ok(limit);
    }

    #[dbus_interface(property)]
    fn clock_limit_mhz_max(&self) -> fdo::Result<f64> {
        let path = format!("{0}/{1}", self.path(), "gt_RP0_freq_mhz");
        let result = fs::read_to_string(path);
        let limit = result
            // convert the std::io::Error to a zbus::fdo::Error
            .map_err(|err| fdo::Error::IOError(err.to_string()))?
            .trim()
            .parse::<f64>()
            // convert the ParseIntError to a zbus::fdo::Error
            .map_err(|err| fdo::Error::Failed(err.to_string()))?;

        return Ok(limit);
    }

    #[dbus_interface(property)]
    fn clock_value_mhz_min(&self) -> fdo::Result<f64> {
        let path = format!("{0}/{1}", self.path(), "gt_min_freq_mhz");
        let result = fs::read_to_string(path);
        let value = result
            // convert the std::io::Error to a zbus::fdo::Error
            .map_err(|err| fdo::Error::IOError(err.to_string()))?
            .trim()
            .parse::<f64>()
            // convert the ParseIntError to a zbus::fdo::Error
            .map_err(|err| fdo::Error::Failed(err.to_string()))?;

        return Ok(value);
    }

    #[dbus_interface(property)]
    fn set_clock_value_mhz_min(&mut self, value: f64) -> fdo::Result<()> {
        if value == 0.0 {
            return Err(fdo::Error::InvalidArgs(
                "Cowardly refusing to set clock to 0MHz".to_string(),
            ));
        }

        // Open the sysfs file to write to
        let path = format!("{0}/{1}", self.path(), "gt_min_freq_mhz");
        let file = OpenOptions::new().write(true).open(path);

        // Write the value
        file
            // convert the std::io::Error to a zbus::fdo::Error
            .map_err(|err| fdo::Error::Failed(err.to_string()))?
            .write_all(value.to_string().as_bytes())
            // convert the std::io::Error to a zbus::fdo::Error
            .map_err(|err| fdo::Error::IOError(err.to_string()))?;

        return Ok(());
    }

    #[dbus_interface(property)]
    fn clock_value_mhz_max(&self) -> fdo::Result<f64> {
        let path = format!("{0}/{1}", self.path(), "gt_max_freq_mhz");
        let result = fs::read_to_string(path);
        let value = result
            // convert the std::io::Error to a zbus::fdo::Error
            .map_err(|err| fdo::Error::IOError(err.to_string()))?
            .trim()
            .parse::<f64>()
            // convert the ParseIntError to a zbus::fdo::Error
            .map_err(|err| fdo::Error::Failed(err.to_string()))?;

        return Ok(value);
    }

    #[dbus_interface(property)]
    fn set_clock_value_mhz_max(&mut self, value: f64) -> fdo::Result<()> {
        if value == 0.0 {
            return Err(fdo::Error::InvalidArgs(
                "Cowardly refusing to set clock to 0MHz".to_string(),
            ));
        }

        // Open the sysfs file to write to
        let path = format!("{0}/{1}", self.path(), "gt_max_freq_mhz");
        let file = OpenOptions::new().write(true).open(path);

        // Write the value
        file
            // convert the std::io::Error to a zbus::fdo::Error
            .map_err(|err| fdo::Error::Failed(err.to_string()))?
            .write_all(value.to_string().as_bytes())
            // convert the std::io::Error to a zbus::fdo::Error
            .map_err(|err| fdo::Error::IOError(err.to_string()))?;

        return Ok(());
    }

    #[dbus_interface(property)]
    fn manual_clock(&self) -> fdo::Result<bool> {
        return Ok(self.manual_clock.clone());
    }

    #[dbus_interface(property)]
    fn set_manual_clock(&mut self, enabled: bool) -> fdo::Result<()> {
        self.manual_clock = enabled;
        return Ok(());
    }
}
