use std::sync::Arc;
use zbus::fdo;
use zbus_macros::dbus_interface;

use tokio::sync::Mutex;

use crate::performance::gpu::dbus::devices::TDPDevices;
use crate::performance::gpu::tdp::TDPError;
use crate::performance::gpu::tdp::TDPResult;

pub struct GPUTDPDBusIface {
    dev: Arc<Mutex<TDPDevices>>,
}

impl From<TDPError> for fdo::Error {
    fn from(val: TDPError) -> Self {
        match &val {
            TDPError::FailedOperation(err) => fdo::Error::Failed(err.to_string()),
            TDPError::FeatureUnsupported => fdo::Error::Failed(String::from("Unsupported feature")),
            TDPError::InvalidArgument(err) => fdo::Error::Failed(err.to_string()),
            TDPError::IOError(err) => fdo::Error::IOError(err.to_string()),
        }
    }
}

impl GPUTDPDBusIface {
    pub fn new(dev: Arc<Mutex<TDPDevices>>) -> GPUTDPDBusIface {
        GPUTDPDBusIface { dev }
    }
}

#[dbus_interface(name = "org.shadowblip.GPU.Card.TDP")]
impl GPUTDPDBusIface {
    /// Get the currently set TDP value
    #[dbus_interface(property, name = "TDP")]
    async fn tdp(&self) -> fdo::Result<f64> {
        match self.dev.lock().await.tdp().await {
            TDPResult::Ok(result) => Ok(result),
            TDPResult::Err(err) => Err(err.into()),
        }
    }

    /// Sets the given TDP value
    #[dbus_interface(property, name = "TDP")]
    async fn set_tdp(&mut self, value: f64) -> fdo::Result<()> {
        match self.dev.lock().await.set_tdp(value).await {
            TDPResult::Ok(result) => Ok(result),
            TDPResult::Err(err) => Err(err.into()),
        }
    }

    #[dbus_interface(property)]
    async fn min_tdp(&self) -> fdo::Result<f64> {
        match self.dev.lock().await.min_tdp().await {
            TDPResult::Ok(result) => Ok(result),
            TDPResult::Err(err) => Err(err.into()),
        }
    }

    #[dbus_interface(property)]
    async fn max_tdp(&self) -> fdo::Result<f64> {
        match self.dev.lock().await.max_tdp().await {
            TDPResult::Ok(result) => Ok(result),
            TDPResult::Err(err) => Err(err.into()),
        }
    }

    /// The TDP boost for AMD is the total difference between the Fast PPT Limit
    /// and the STAPM limit.
    #[dbus_interface(property)]
    async fn boost(&self) -> fdo::Result<f64> {
        match self.dev.lock().await.boost().await {
            TDPResult::Ok(result) => Ok(result),
            TDPResult::Err(err) => Err(err.into()),
        }
    }

    #[dbus_interface(property)]
    async fn max_boost(&self) -> fdo::Result<f64> {
        match self.dev.lock().await.max_boost().await {
            TDPResult::Ok(result) => Ok(result),
            TDPResult::Err(err) => Err(err.into()),
        }
    }

    #[dbus_interface(property)]
    async fn set_boost(&mut self, value: f64) -> fdo::Result<()> {
        match self.dev.lock().await.set_boost(value).await {
            TDPResult::Ok(result) => Ok(result),
            TDPResult::Err(err) => Err(err.into()),
        }
    }

    #[dbus_interface(property)]
    async fn thermal_throttle_limit_c(&self) -> fdo::Result<f64> {
        match self.dev.lock().await.thermal_throttle_limit_c().await {
            TDPResult::Ok(result) => Ok(result),
            TDPResult::Err(err) => Err(err.into()),
        }
    }

    #[dbus_interface(property)]
    async fn set_thermal_throttle_limit_c(&mut self, limit: f64) -> fdo::Result<()> {
        match self
            .dev
            .lock()
            .await
            .set_thermal_throttle_limit_c(limit)
            .await
        {
            TDPResult::Ok(result) => Ok(result),
            TDPResult::Err(err) => Err(err.into()),
        }
    }

    #[dbus_interface(property)]
    async fn power_profile(&self) -> fdo::Result<String> {
        match self.dev.lock().await.power_profile().await {
            TDPResult::Ok(result) => Ok(result),
            TDPResult::Err(err) => Err(err.into()),
        }
    }

    #[dbus_interface(property)]
    async fn set_power_profile(&mut self, profile: String) -> fdo::Result<()> {
        match self.dev.lock().await.set_power_profile(profile).await {
            TDPResult::Ok(result) => Ok(result),
            TDPResult::Err(err) => Err(err.into()),
        }
    }

    #[dbus_interface(property)]
    async fn power_profiles_available(&self) -> fdo::Result<Vec<String>> {
        match self.dev.lock().await.power_profiles_available().await {
            TDPResult::Ok(result) => Ok(result),
            TDPResult::Err(err) => Err(err.into()),
        }
    }
}
