use std::sync::Arc;

use tokio::{
    sync::Mutex,
    fs,
    io::{AsyncReadExt, AsyncWriteExt},
    task::spawn_blocking
};

use crate::performance::gpu::tdp::{TDPDevice, TDPResult, TDPError};

use super::tdp::TDP;
use crate::platform::asus::DaemonProxy;
use zbus::Connection;

struct RogDbusClient;

impl<'a> RogDbusClient {
    pub async fn new() -> zbus::Result<(DaemonProxy<'a>, Connection)> {
        let connection = Connection::session().await?;
        let proxy = DaemonProxy::new(&connection).await?;
        
        Ok((proxy, connection))
    }
}

/// Implementation of asusd with a fallback to asus-wmi sysfs
/// See https://www.kernel.org/doc/html/v6.8-rc4/admin-guide/abi-testing.html#abi-sys-devices-platform-platform-ppt-apu-sppt
pub struct ASUS {
    tdp: u8,
    boost: u8,
    ryzenadj: Arc<Mutex<TDP>>,
}

impl ASUS {

    /// test if we are in an asus system with asus-wmi loaded
    pub async fn new(path: String, device_id: String) -> Option<Self> {
        let asus_nb_wmi = std::path::Path::new("/sys/devices/platform/asus-nb-wmi");

        match fs::metadata(asus_nb_wmi).await.is_ok() {
            true => {
                log::info!("ASUS device detected, using asus-wmi");

                match spawn_blocking(|| Arc::new(Mutex::new(TDP::new(path, device_id)))).await {
                    Ok(ryzenadj) => {
                        Some(Self {
                            tdp: 5,
                            boost: 0,
                            ryzenadj,
                        })
                    },
                    Err(err) => {
                        log::error!("{}", err);
                        None
                    },
                }
            },
            false => None,
        }

    }

    async fn write(&self, var: &str, value: u8) -> TDPResult<()> {
        match fs::File::create(format!("/sys/devices/platform/asus-nb-wmi/{}", var)).await {
            Ok(mut file) => {
                match file.write(value.to_string().as_bytes()).await {
                    Ok(_) => Ok(()),
                    Err(err) => {
                        log::warn!("Unable to use asus-wmi interface");
                        Err(TDPError::FailedOperation(format!("{}", err)))
                    },
                }
            },
            Err(err) => {
                log::warn!("Unable to use asus-wmi interface");
                Err(TDPError::FailedOperation(format!("{}", err)))
            },
        }
    }

    async fn read(&self, var: &str) -> TDPResult<u8> {
        match fs::File::open(format!("/sys/devices/platform/asus-nb-wmi/{}", var)).await {
            Ok(mut file) => {
                let mut buf = String::new();
                match file.read_to_string(&mut buf).await {
                    Ok(_) => {
                        match buf.parse::<u8>() {
                            Ok(value) => Ok(value),
                            Err(err) => {
                                log::warn!("Unable to use asus-wmi interface");
                                Err(TDPError::FailedOperation(format!("{}", err)))
                            },
                        }
                    },
                    Err(err) => {
                        log::warn!("Unable to use asus-wmi interface");
                        Err(TDPError::FailedOperation(format!("{}", err)))
                    },
                }
            },
            Err(err) => {
                log::warn!("Unable to use asus-wmi interface");
                Err(TDPError::FailedOperation(format!("{}", err)))
            }
        }
    }

    async fn set_tdp_boost(&self) -> TDPResult<()> {
        match RogDbusClient::new().await {
            Ok((platform, _)) => {
                match platform.set_ppt_fppt(self.tdp + self.boost).await {
                    Ok(()) => Ok(()),
                    Err(err) => {
                        log::error!("{}", err);
                        log::warn!("Unable to use asusd to set ppt_fppt, asus-wmi interface will be used");
                        self.write("ppt_fppt", self.tdp + self.boost).await
                    },
                }
            },
            Err(err) => {
                log::error!("{}", err);
                log::warn!("Unable to use asusd to set ppt_fppt, asus-wmi interface will be used");
                self.write("ppt_fppt", self.tdp + self.boost).await
            }
        }.and_then(|_| {
            log::debug!("Set TDP: {}", self.tdp);
            log::debug!("Set boost: {}", self.boost);
            Ok(())
        })
    }

    async fn throttle_thermal_policy(&self) -> TDPResult<String> {
        match self.read("throttle_thermal_policy").await {
            Ok(value) => {
                let res = match value {
                    1 => "max-performance",
                    0 => "power-saving",
                    _ => "power-saving"
                };

                Ok(String::from(res))
            },
            Err(err) => {
                log::warn!("Unable to use asus-wmi interface");
                Err(err)
            }
        }
    }

    async fn set_throttle_thermal_policy(&self, throttle_policy: u32) -> TDPResult<()> {
        // Balanced = 0
        // Performance = 1
        // Quiet = 2
        match RogDbusClient::new().await {
            Ok((platform, _)) => {
                match platform.set_throttle_thermal_policy(throttle_policy).await {
                    Ok(()) => Ok(()),
                    Err(err) => {
                        log::error!("{}", err);
                        log::warn!("Unable to use asusd to set throttle_thermal_policy, asus-wmi interface will be used");
                        self.write("throttle_thermal_policy", throttle_policy as u8).await
                    },
                }
            },
            Err(err) => {
                log::error!("{}", err);
                log::warn!("Unable to use asusd to set throttle_thermal_policy, asus-wmi interface will be used");
                self.write("throttle_thermal_policy", throttle_policy as u8).await
            }
        }.and_then(|_| {
            log::debug!("Set power profile: {:?}", throttle_policy);
            Ok(())
        })
    }
}

impl TDPDevice for ASUS {
    async fn tdp(&self) -> TDPResult<f64> {
        Ok(self.tdp.into())
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
        self.ryzenadj.lock().await.thermal_throttle_limit_c().await
    }

    async fn set_thermal_throttle_limit_c(&mut self, limit: f64) -> TDPResult<()> {
        self.ryzenadj.lock().await.set_thermal_throttle_limit_c(limit).await
    }

    async fn power_profile(&self) -> TDPResult<String> {
        match RogDbusClient::new().await {
            Ok((platform, _)) => {
                match platform.throttle_thermal_policy().await {
                    Ok(throttle_policy) => {
                        match throttle_policy {
                            1 => Ok("max-performance".to_string()),
                            0 => Ok("power-saving".to_string()),
                            2 => Ok("power-saving".to_string()),
                            _ => Ok("max-performance".to_string()),
                        }
                    },
                    Err(err) => {
                        log::error!("{}", err);
                        log::warn!("Unable to use asusd to read throttle_thermal_policy, asus-wmi interface will be used");
                        self.throttle_thermal_policy().await
                    },
                }
            },
            Err(err) => {
                log::error!("{}", err);
                log::warn!("Unable to use asusd to read throttle_thermal_policy, asus-wmi interface will be used");
                self.throttle_thermal_policy().await
            }
        }
    }

    async fn set_power_profile(&mut self, profile: String) -> TDPResult<()> {
        // possible values - "max-performance", "power-saving"

        match profile.as_str() {
            "max-performance" => {
                self.set_throttle_thermal_policy(1).await
            },
            "power-saving" => {
                self.set_throttle_thermal_policy(0).await
            },
            _ => {
                Ok(())
            },
        }
    }
}
