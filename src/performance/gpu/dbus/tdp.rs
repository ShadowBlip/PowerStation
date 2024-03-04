use std::sync::Arc;
use std::sync::Mutex;
use zbus::fdo::Error;
use zbus::fdo;
use zbus_macros::dbus_interface;

use crate::performance::gpu::tdp::TDPError;
use crate::performance::gpu::tdp::{TDPDevice, TDPResult};

pub struct GPUTDPDBusIface {
    dev: Arc<Mutex<dyn TDPDevice>>
}

impl Into<fdo::Error> for TDPError {
    fn into(self) -> zbus::fdo::Error {
        match &self {
            Self::FailedOperation(err) => fdo::Error::Failed(err.to_string()),
            Self::FeatureUnsupported => fdo::Error::Failed(String::from("Unsupported feature")),
            Self::InvalidArgument(err) => fdo::Error::Failed(err.to_string()),
            Self::IOError(err) => fdo::Error::IOError(err.to_string())
        }
    }
}

impl GPUTDPDBusIface {
    pub fn new(dev: Arc<Mutex<dyn TDPDevice>>) -> GPUTDPDBusIface {
        GPUTDPDBusIface {
            dev
        }
    }
}

#[dbus_interface(name = "org.shadowblip.GPU.Card.TDP")]
impl GPUTDPDBusIface {

    /// Get the currently set TDP value
    #[dbus_interface(property, name = "TDP")]
    fn tdp(&self) -> fdo::Result<f64> {
        match self.dev.lock() {
            Ok(lck) => {
                match lck.tdp() {
                    TDPResult::Ok(result) => Ok(result),
                    TDPResult::Err(err) => Err(err.into())
                }
            },
            Err(err) => {
                Err(Error::Failed(format!("Unable to lock mutex: {}", err)))
            }
        }
    }

    /// Sets the given TDP value
    #[dbus_interface(property, name = "TDP")]
    fn set_tdp(&mut self, value: f64) -> fdo::Result<()> {
        match self.dev.lock() {
            Ok(mut lck) => {
                match lck.set_tdp(value) {
                    TDPResult::Ok(result) => Ok(result),
                    TDPResult::Err(err) => Err(err.into())
                }
            },
            Err(err) => {
                Err(Error::Failed(format!("Unable to lock mutex: {}", err)))
            }
        }
    }

    /// The TDP boost for AMD is the total difference between the Fast PPT Limit
    /// and the STAPM limit.
    #[dbus_interface(property)]
    fn boost(&self) -> fdo::Result<f64> {
        match self.dev.lock() {
            Ok(lck) => {
                match lck.boost() {
                    TDPResult::Ok(result) => Ok(result),
                    TDPResult::Err(err) => Err(err.into())
                }
            },
            Err(err) => {
                Err(Error::Failed(format!("Unable to lock mutex: {}", err)))
            }
        }
    }

    #[dbus_interface(property)]
    fn set_boost(&mut self, value: f64) -> fdo::Result<()> {
        match self.dev.lock() {
            Ok(mut lck) => {
                match lck.set_boost(value) {
                    TDPResult::Ok(result) => Ok(result),
                    TDPResult::Err(err) => Err(err.into())
                }
            },
            Err(err) => {
                Err(Error::Failed(format!("Unable to lock mutex: {}", err)))
            }
        }
    }

    #[dbus_interface(property)]
    fn thermal_throttle_limit_c(&self) -> fdo::Result<f64> {
        match self.dev.lock() {
            Ok(lck) => {
                match lck.thermal_throttle_limit_c() {
                    TDPResult::Ok(result) => Ok(result),
                    TDPResult::Err(err) => Err(err.into())
                }
            },
            Err(err) => {
                Err(Error::Failed(format!("Unable to lock mutex: {}", err)))
            }
        }
    }

    #[dbus_interface(property)]
    fn set_thermal_throttle_limit_c(&mut self, limit: f64) -> fdo::Result<()> {
        match self.dev.lock() {
            Ok(mut lck) => {
                match lck.set_thermal_throttle_limit_c(limit) {
                    TDPResult::Ok(result) => Ok(result),
                    TDPResult::Err(err) => Err(err.into())
                }
            },
            Err(err) => {
                Err(Error::Failed(format!("Unable to lock mutex: {}", err)))
            }
        }
    }

    #[dbus_interface(property)]
    fn power_profile(&self) -> fdo::Result<String> {
        match self.dev.lock() {
            Ok(lck) => {
                match lck.power_profile() {
                    TDPResult::Ok(result) => Ok(result),
                    TDPResult::Err(err) => Err(err.into())
                }
            },
            Err(err) => {
                Err(Error::Failed(format!("Unable to lock mutex: {}", err)))
            }
        }
    }

    #[dbus_interface(property)]
    fn set_power_profile(&mut self, profile: String) -> fdo::Result<()> {
        match self.dev.lock() {
            Ok(mut lck) => {
                match lck.set_power_profile(profile) {
                    TDPResult::Ok(result) => Ok(result),
                    TDPResult::Err(err) => Err(err.into())
                }
            },
            Err(err) => {
                Err(Error::Failed(format!("Unable to lock mutex: {}", err)))
            }
        }
    }

}