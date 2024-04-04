use std::{
    fs::{self, OpenOptions},
    io::Write,
    sync::Arc
};

use tokio::sync::Mutex;

use crate::constants::PREFIX;
use crate::performance::gpu::interface::GPUDevice;
use crate::performance::gpu::intel;
use crate::performance::gpu::interface::{GPUError, GPUResult};
use crate::performance::gpu::dbus::devices::TDPDevices;

#[derive(Debug, Clone)]
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


impl GPUDevice for IntelGPU {
    
    async fn get_gpu_path(&self) -> String {
        format!("{0}/GPU/{1}", PREFIX, self.name().await)
    }

    /// Returns the TDP DBus interface for this GPU
    async fn get_tdp_interface(&self) -> Option<Arc<Mutex<TDPDevices>>> {
        match self.class.as_str() {
            "integrated" => Some(
                Arc::new(
                    Mutex::new(
                        TDPDevices::INTEL(
                            intel::tdp::TDP::new(
                                self.path.clone()
                            )
                        )
                    )
                )
            ),
            _ => None,
        }
    }

    async fn name(&self) -> String {
        self.name.clone()
    }

    async fn path(&self) -> String {
        self.path.clone()
    }

    async fn class(&self) -> String {
        self.class.clone()
    }

    async fn class_id(&self) -> String {
        self.class_id.clone()
    }

    async fn vendor(&self) -> String {
        self.vendor.clone()
    }

    async fn vendor_id(&self) -> String {
        self.vendor_id.clone()
    }

    async fn device(&self) -> String {
        self.device.clone()
    }

    async fn device_id(&self) -> String {
        self.device_id.clone()
    }

    async fn subdevice(&self) -> String {
        self.subdevice.clone()
    }

    async fn subdevice_id(&self) -> String {
        self.subdevice_id.clone()
    }

    async fn subvendor_id(&self) -> String {
        self.subvendor_id.clone()
    }

    async fn revision_id(&self) -> String {
        self.revision_id.clone()
    }

    async fn clock_limit_mhz_min(&self) -> GPUResult<f64> {
        let path = format!("{0}/{1}", self.path().await, "gt_RPn_freq_mhz");
        let result = fs::read_to_string(path);
        let limit = result
            .map_err(|err| GPUError::IOError(err.to_string()))?
            .trim()
            .parse::<f64>()
            .map_err(|err| GPUError::FailedOperation(err.to_string()))?;

        return Ok(limit);
    }

    async fn clock_limit_mhz_max(&self) -> GPUResult<f64> {
        let path = format!("{0}/{1}", self.path().await, "gt_RP0_freq_mhz");
        let limit = fs::read_to_string(path)
            .map_err(|err| GPUError::IOError(err.to_string()))?
            .trim()
            .parse::<f64>()
            .map_err(|err| GPUError::FailedOperation(err.to_string()))?;

        return Ok(limit);
    }

    async fn clock_value_mhz_min(&self) -> GPUResult<f64> {
        let path = format!("{0}/{1}", self.path().await, "gt_min_freq_mhz");
        let result = fs::read_to_string(path);
        let value = result
            .map_err(|err| GPUError::IOError(err.to_string()))?
            .trim()
            .parse::<f64>()
            .map_err(|err| GPUError::FailedOperation(err.to_string()))?;

        return Ok(value);
    }

    async fn set_clock_value_mhz_min(&mut self, value: f64) -> GPUResult<()> {
        if value == 0.0 {
            return Err(GPUError::InvalidArgument(
                "Cowardly refusing to set clock to 0MHz".to_string(),
            ));
        }

        // Open the sysfs file to write to
        let path = format!("{0}/{1}", self.path().await, "gt_min_freq_mhz");
        let file = OpenOptions::new().write(true).open(path);

        // Write the value
        file
            .map_err(|err| GPUError::FailedOperation(err.to_string()))?
            .write_all(value.to_string().as_bytes())
            .map_err(|err| GPUError::IOError(err.to_string()))?;

        return Ok(());
    }

    async fn clock_value_mhz_max(&self) -> GPUResult<f64> {
        let path = format!("{0}/{1}", self.path().await, "gt_max_freq_mhz");
        let result = fs::read_to_string(path);
        let value = result
            .map_err(|err| GPUError::IOError(err.to_string()))?
            .trim()
            .parse::<f64>()
            .map_err(|err| GPUError::FailedOperation(err.to_string()))?;

        return Ok(value);
    }

    async fn set_clock_value_mhz_max(&mut self, value: f64) -> GPUResult<()> {
        if value == 0.0 {
            return Err(GPUError::InvalidArgument(
                "Cowardly refusing to set clock to 0MHz".to_string(),
            ));
        }

        // Open the sysfs file to write to
        let path = format!("{0}/{1}", self.path().await, "gt_max_freq_mhz");
        let file = OpenOptions::new().write(true).open(path);

        // Write the value
        file
            .map_err(|err| GPUError::FailedOperation(err.to_string()))?
            .write_all(value.to_string().as_bytes())
            .map_err(|err| GPUError::IOError(err.to_string()))?;

        return Ok(());
    }

    async fn manual_clock(&self) -> GPUResult<bool> {
        return Ok(self.manual_clock.clone());
    }

    async fn set_manual_clock(&mut self, enabled: bool) -> GPUResult<()> {
        self.manual_clock = enabled;
        return Ok(());
    }
}
