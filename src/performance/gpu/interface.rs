use std::sync::Arc;

use tokio::sync::Mutex;

use crate::performance::gpu::dbus::devices::TDPDevices;

pub enum GPUError {
    //FeatureUnsupported,
    FailedOperation(String),
    InvalidArgument(String),
    IOError(String),
}

impl Into<String> for GPUError {
    fn into(self) -> std::string::String {
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
}
