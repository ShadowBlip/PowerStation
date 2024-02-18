// TODO: totally remove dbus from here

use std::{
    fs::{self, OpenOptions},
    io::Write,
    sync::{
        Arc, Mutex
    }
};

use zbus::{fdo, zvariant::ObjectPath};

use crate::performance::gpu::interface::GPUIface;
use crate::performance::gpu::{intel, tdp::TDPDevice};

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


impl GPUIface for IntelGPU {
    
    /// Returns the TDP DBus interface for this GPU
    fn get_tdp_interface(&self) -> Option<Arc<Mutex<dyn TDPDevice>>> {
        match self.class.as_str() {
            "integrated" => Some(
                Arc::new(
                    Mutex::new(
                        intel::tdp::TDP::new(
                            self.path.clone()
                        )
                    )
                )
            ),
            _ => None,
        }
    }

    fn name(&self) -> String {
        self.name.clone()
    }

    fn path(&self) -> String {
        self.path.clone()
    }

    fn class(&self) -> String {
        self.class.clone()
    }

    fn class_id(&self) -> String {
        self.class_id.clone()
    }

    fn vendor(&self) -> String {
        self.vendor.clone()
    }

    fn vendor_id(&self) -> String {
        self.vendor_id.clone()
    }

    fn device(&self) -> String {
        self.device.clone()
    }

    fn device_id(&self) -> String {
        self.device_id.clone()
    }

    fn subdevice(&self) -> String {
        self.subdevice.clone()
    }

    fn subdevice_id(&self) -> String {
        self.subdevice_id.clone()
    }

    fn subvendor_id(&self) -> String {
        self.subvendor_id.clone()
    }

    fn revision_id(&self) -> String {
        self.revision_id.clone()
    }

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

    fn manual_clock(&self) -> fdo::Result<bool> {
        return Ok(self.manual_clock.clone());
    }

    fn set_manual_clock(&mut self, enabled: bool) -> fdo::Result<()> {
        self.manual_clock = enabled;
        return Ok(());
    }
}
