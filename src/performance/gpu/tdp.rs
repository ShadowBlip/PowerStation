#[derive(Debug)]
pub enum TDPError {
    FeatureUnsupported,
    FailedOperation(String),
    InvalidArgument(String),
    IOError(String),
}

impl From<TDPError> for String {
    fn from(_val: TDPError) -> Self {
        todo!()
    }
}

pub type TDPResult<T> = Result<T, TDPError>;

// Helper trait to simplify access to hardware information
pub trait HardwareAccess {
    fn hardware(&self) -> Option<&crate::performance::gpu::platform::hardware::Hardware>;
}

pub trait TDPDevice: Sync + Send + HardwareAccess {
    async fn tdp(&self) -> TDPResult<f64>;
    async fn set_tdp(&mut self, value: f64) -> TDPResult<()>;
    async fn boost(&self) -> TDPResult<f64>;
    async fn set_boost(&mut self, value: f64) -> TDPResult<()>;
    async fn thermal_throttle_limit_c(&self) -> TDPResult<f64>;
    async fn set_thermal_throttle_limit_c(&mut self, limit: f64) -> TDPResult<()>;
    async fn power_profile(&self) -> TDPResult<String>;
    async fn power_profiles_available(&self) -> TDPResult<Vec<String>>;
    async fn set_power_profile(&mut self, profile: String) -> TDPResult<()>;

    // Default implementations for hardware-based methods
    async fn min_tdp(&self) -> TDPResult<f64> {
        log::info!("Get TDP Min");
        if let Some(hardware) = self.hardware() {
            return Ok(hardware.min_tdp());
        }
        Err(TDPError::FailedOperation(
            "No Hardware interface available to read min TDP.".into(),
        ))
    }

    async fn max_tdp(&self) -> TDPResult<f64> {
        log::info!("Get TDP Max");
        if let Some(hardware) = self.hardware() {
            return Ok(hardware.max_tdp());
        }
        Err(TDPError::FailedOperation(
            "No Hardware interface available to read max TDP.".into(),
        ))
    }

    async fn max_boost(&self) -> TDPResult<f64> {
        log::info!("Get TDP Max Boost");
        if let Some(hardware) = self.hardware() {
            return Ok(hardware.max_boost());
        }
        Err(TDPError::FailedOperation(
            "No Hardware interface available to read max boost.".into(),
        ))
    }
}
