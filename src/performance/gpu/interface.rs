use std::sync::{Arc, Mutex};
use crate::performance::gpu::tdp::TDPDevice;

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
pub trait GPUIface: Send + Sync {

    fn get_tdp_interface(&self) -> Option<Arc<Mutex<dyn TDPDevice>>>;

    fn get_gpu_path(&self) -> String;

    fn name(&self) -> String;
    fn path(&self) -> String;
    fn class(&self) -> String;
    fn class_id(&self) -> String;
    fn vendor(&self) -> String;
    fn vendor_id(&self) -> String;
    fn device(&self) -> String;
    fn device_id(&self) -> String;
    fn subdevice(&self) -> String;
    fn subdevice_id(&self) -> String;
    fn subvendor_id(&self) -> String;
    fn revision_id(&self) -> String;
    fn clock_limit_mhz_min(&self) -> GPUResult<f64>;
    fn clock_limit_mhz_max(&self) -> GPUResult<f64>;
    fn clock_value_mhz_min(&self) -> GPUResult<f64>;
    fn set_clock_value_mhz_min(&mut self, value: f64) -> GPUResult<()>;
    fn clock_value_mhz_max(&self) -> GPUResult<f64>;
    fn set_clock_value_mhz_max(&mut self, value: f64) -> GPUResult<()>;
    fn manual_clock(&self) -> GPUResult<bool>;
    fn set_manual_clock(&mut self, enabled: bool) -> GPUResult<()>;
}
