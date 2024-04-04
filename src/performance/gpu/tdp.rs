pub enum TDPError {
    FeatureUnsupported,
    FailedOperation(String),
    InvalidArgument(String),
    IOError(String),
}

impl Into<String> for TDPError {
    fn into(self) -> std::string::String {
        todo!()
    }
}

pub type TDPResult<T> = Result<T, TDPError>;

pub trait TDPDevice : Sync + Send {
    async fn tdp(&self) -> TDPResult<f64>;
    async fn set_tdp(&mut self, value: f64) -> TDPResult<()>;
    async fn boost(&self) -> TDPResult<f64>;
    async fn set_boost(&mut self, value: f64) -> TDPResult<()>;
    async fn thermal_throttle_limit_c(&self) -> TDPResult<f64>;
    async fn set_thermal_throttle_limit_c(&mut self, limit: f64) -> TDPResult<()>;
    async fn power_profile(&self) -> TDPResult<String>;
    async fn set_power_profile(&mut self, profile: String) -> TDPResult<()>;
}