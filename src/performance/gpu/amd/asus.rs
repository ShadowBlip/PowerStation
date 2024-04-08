use std::sync::Arc;

use crate::performance::gpu::tdp::{TDPDevice, TDPResult, TDPError};
use crate::performance::gpu::dbus::devices::TDPDevices;

use tokio::task::spawn_blocking;
use zbus::{Connection, Result};

use rog_dbus::{DbusProxies, RogDbusClient};
use rog_platform::{platform::RogPlatform, error::PlatformError};
use rog_platform::platform::{GpuMode, Properties, ThrottlePolicy};
use rog_profiles::error::ProfileError;

use std::sync::Mutex;

/// Implementation of asusd with a fallback to asus-wmi sysfs
/// See https://www.kernel.org/doc/html/v6.8-rc4/admin-guide/abi-testing.html#abi-sys-devices-platform-platform-ppt-apu-sppt
pub struct ASUS {
    platform: Arc<Mutex<RogPlatform>>,
    tdp: u8,
    boost: u8,
}

impl ASUS {

    /// test if we are in an asus system with asus-wmi loaded
    pub async fn new() -> Option<Self> {
        match RogPlatform::new() {
            Ok(platform) => {
                log::info!("Module asus-wmi WAS found");
                Some(Self {
                    platform: Arc::new(Mutex::new(platform)),
                    tdp: 5,
                    boost: 0
                })
            },
            Err(err) => {
                log::info!("Module asus-wmi not found: {}", err);
                None
            }
        }
    }

    async fn set_tdp_boost(&self) -> TDPResult<()> {
        match RogDbusClient::new().await {
            Ok((dbus, _)) => {
                let platform = dbus.proxies().rog_bios();

                log::info!("{} + {}", self.tdp, self.boost);

                match platform.set_ppt_fppt(self.tdp + self.boost).await {
                    Ok(()) => Ok(()),
                    Err(_) => {
                        self.set_tdp_boost_wmi().await
                    },
                }
            },
            Err(_) => {
                log::warn!("Unable to use asusd to read tdp, asus-wmi interface will be used");
                self.set_tdp_boost_wmi().await
            }
        }
    }

    async fn set_tdp_boost_wmi(&self) -> TDPResult<()> {
        match self.platform.lock() {
            Ok(platform) => {
                match platform.set_ppt_fppt(self.tdp + self.boost) {
                    Ok(()) => Ok(()),
                    Err(err) => {
                        log::warn!("Unable to set ppt_fppt: {}", err);
                        Err(TDPError::FailedOperation(format!("")))
                    }
                }
            },
            Err(_) => {
                log::warn!("Unable to use asus-wmi interface");
                Err(TDPError::FailedOperation(format!("")))
            },
        }
    }
}

impl TDPDevice for ASUS {
    async fn tdp(&self) -> TDPResult<f64> {
        match RogDbusClient::new().await {
            Ok((dbus, _)) => {
                let platform = dbus.proxies().rog_bios();

                match platform.ppt_fppt().await {
                    Ok(result) => {
                        log::info!("ppt_fppt: {}", result);
                        Ok(self.tdp.into())
                    },
                    Err(err) => {
                        match self.platform.lock() {
                            Ok(platform) => {
                                match platform.get_ppt_fppt() {
                                    Ok(result) => {
                                        log::info!("ppt_fppt: {}", result);
                                        Ok(self.tdp.into())
                                    },
                                    Err(err) => {
                                        log::warn!("Error fetching ppt_fppt: {}", err);
                                        Err(TDPError::FailedOperation(format!("")))
                                    }
                                }
                            },
                            Err(_) => {
                                log::warn!("Unable to use asus-wmi interface");
                                Err(TDPError::FailedOperation(format!("")))
                            },
                        }
                    }
                }
            },
            Err(_) => {
                log::warn!("Unable to use asusd to read tdp, asus-wmi interface will be used");
                match self.platform.lock() {
                    Ok(platform) => {
                        match platform.get_ppt_fppt() {
                            Ok(result) => {
                                log::info!("ppt_fppt: {}", result);
                                Ok(self.tdp.into())
                            },
                            Err(err) => {
                                log::warn!("Error fetching ppt_fppt: {}", err);
                                Err(TDPError::FailedOperation(format!("")))
                            }
                        }
                    },
                    Err(_) => {
                        log::warn!("Unable to use asus-wmi interface");
                        Err(TDPError::FailedOperation(format!("")))
                    },
                }
            }
        }
    }

    async fn set_tdp(&mut self, value: f64) -> TDPResult<()> {
        self.tdp = value.round() as u8;

        self.set_tdp_boost().await
    }

    async fn boost(&self) -> TDPResult<f64> {
        Ok(self.boost.into())
    }

    async fn set_boost(&mut self, value: f64) -> TDPResult<()> {
        self.boost = value.round() as u8;

        self.set_tdp_boost().await
    }

    async fn thermal_throttle_limit_c(&self) -> TDPResult<f64> {
        Ok(0.0)
    }

    async fn set_thermal_throttle_limit_c(&mut self, limit: f64) -> TDPResult<()> {
        Ok(())
    }

    async fn power_profile(&self) -> TDPResult<String> {
        match RogDbusClient::new().await {
            Ok((dbus, _)) => {
                let platform = dbus.proxies().rog_bios();

                Ok("".to_string())
            },
            Err(err) => {
                log::warn!("Unable to use asusd to read tdp, asus-wmi interface will be used");
                Err(TDPError::FailedOperation(format!("")))
            }
        }
    }

    async fn set_power_profile(&mut self, profile: String) -> TDPResult<()> {
        // possible values - "max-performance", "power-saving"

        match RogDbusClient::new().await {
            Ok((dbus, _)) => {
                let platform = dbus.proxies().rog_bios();

                Ok(())
            },
            Err(err) => {
                log::warn!("Unable to use asusd to read tdp, asus-wmi interface will be used");
                Err(TDPError::FailedOperation(format!("")))
            }
        }
    }
}
