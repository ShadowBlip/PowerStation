use std::{path::PathBuf, sync::Arc};

use tokio::fs;
use tokio::sync::Mutex;

use crate::performance::gpu::dbus::devices::TDPDevices;

pub enum GPUError {
    //FeatureUnsupported,
    FailedOperation(String),
    InvalidArgument(String),
    IOError(String),
}

impl From<GPUError> for String {
    fn from(_val: GPUError) -> Self {
        todo!()
    }
}

pub type GPUResult<T> = Result<T, GPUError>;

/// Represents the data contained in /sys/class/drm/cardX
pub trait GPUDevice: Send + Sync {
    async fn get_tdp_interface(&self) -> Option<Arc<Mutex<TDPDevices>>>;
    async fn get_gpu_path(&self) -> String;
    async fn name(&self) -> String;
    async fn path(&self) -> String;
    async fn class(&self) -> String;
    async fn class_id(&self) -> String;
    async fn vendor(&self) -> String;
    async fn vendor_id(&self) -> String;
    async fn device(&self) -> String;
    async fn device_id(&self) -> String;
    async fn subdevice(&self) -> String;
    async fn subdevice_id(&self) -> String;
    async fn subvendor_id(&self) -> String;
    async fn revision_id(&self) -> String;
    async fn clock_limit_mhz_min(&self) -> GPUResult<f64>;
    async fn clock_limit_mhz_max(&self) -> GPUResult<f64>;
    async fn clock_value_mhz_min(&self) -> GPUResult<f64>;
    async fn set_clock_value_mhz_min(&mut self, value: f64) -> GPUResult<()>;
    async fn clock_value_mhz_max(&self) -> GPUResult<f64>;
    async fn set_clock_value_mhz_max(&mut self, value: f64) -> GPUResult<()>;
    async fn manual_clock(&self) -> GPUResult<bool>;
    async fn set_manual_clock(&mut self, enabled: bool) -> GPUResult<()>;
    async fn get_gpu_busy_percent(&self) -> GPUResult<u8> {
        let path: PathBuf = format!("{0}/{1}", self.path().await, "device/gpu_busy_percent").into();
        if !path.exists() {
            return Err(GPUError::FailedOperation(
                "gpu_busy_percent not supported".to_owned(),
            ));
        }

        let percentage = fs::read_to_string(path)
            .await
            .map_err(|err| GPUError::IOError(err.to_string()))?;

        percentage
            .trim()
            .parse::<u8>()
            .map_err(|err| GPUError::IOError(err.to_string()))
    }
}
