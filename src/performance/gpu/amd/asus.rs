use tokio::{fs, io::{AsyncReadExt, AsyncWriteExt}};

use crate::performance::gpu::tdp::{TDPDevice, TDPResult, TDPError};

use rog_dbus::RogDbusClient;
use rog_platform::platform::ThrottlePolicy;

/// Implementation of asusd with a fallback to asus-wmi sysfs
/// See https://www.kernel.org/doc/html/v6.8-rc4/admin-guide/abi-testing.html#abi-sys-devices-platform-platform-ppt-apu-sppt
pub struct ASUS {
    tdp: u8,
    boost: u8,
}

impl ASUS {

    /// test if we are in an asus system with asus-wmi loaded
    pub async fn new() -> Option<Self> {
        let asus_nb_wmi = std::path::Path::new("/sys/devices/platform/asus-nb-wmi");

        match fs::metadata(asus_nb_wmi).await.is_ok() {
            true => {
                Some(Self {
                    tdp: 5,
                    boost: 0
                })
            },
            false => None,
        }

    }

    async fn write(&self, var: &str, value: u8) -> TDPResult<()> {
        match fs::File::create(format!("/sys/devices/platform/asus-nb-wmi/{}", var)).await {
            Ok(mut file) => {
                match file.write(value.to_string().as_bytes()).await {
                    Ok(_) => Ok(()),
                    Err(_) => {
                        log::warn!("Unable to use asus-wmi interface");
                        Err(TDPError::FailedOperation(format!("")))
                    },
                }
            },
            Err(_) => {
                log::warn!("Unable to use asus-wmi interface");
                Err(TDPError::FailedOperation(format!("")))
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
                            Err(_) => {
                                log::warn!("Unable to use asus-wmi interface");
                                Err(TDPError::FailedOperation(format!("")))
                            },
                        }
                    },
                    Err(_) => {
                        log::warn!("Unable to use asus-wmi interface");
                        Err(TDPError::FailedOperation(format!("")))
                    },
                }
            },
            Err(_) => {
                log::warn!("Unable to use asus-wmi interface");
                Err(TDPError::FailedOperation(format!("")))
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
                        self.write("ppt_fppt", self.tdp + self.boost).await
                    },
                }
            },
            Err(_) => {
                log::warn!("Unable to use asusd to read tdp, asus-wmi interface will be used");
                self.write("ppt_fppt", self.tdp + self.boost).await
            }
        }
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
            Err(_) => {
                log::warn!("Unable to use asus-wmi interface");
                Err(TDPError::FailedOperation(format!("")))
            }
        }
    }

    async fn set_throttle_thermal_policy(&self, throttle_policy: ThrottlePolicy) -> TDPResult<()> {
        match RogDbusClient::new().await {
            Ok((dbus, _)) => {
                let platform = dbus.proxies().rog_bios();

                match platform.set_throttle_thermal_policy(throttle_policy).await {
                    Ok(()) => Ok(()),
                    Err(_) => {
                        log::warn!("Unable to use asusd to read tdp, asus-wmi interface will be used");
                        self.write("throttle_thermal_policy", throttle_policy as u8).await
                    },
                }
            },
            Err(_) => {
                log::warn!("Unable to use asusd to read tdp, asus-wmi interface will be used");
                self.write("throttle_thermal_policy", throttle_policy as u8).await
            }
        }
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
        Ok(0.0)
    }

    async fn set_thermal_throttle_limit_c(&mut self, _limit: f64) -> TDPResult<()> {
        Ok(())
    }

    async fn power_profile(&self) -> TDPResult<String> {
        match RogDbusClient::new().await {
            Ok((dbus, _)) => {
                let platform = dbus.proxies().rog_bios();

                match platform.throttle_thermal_policy().await {
                    Ok(throttle_policy) => {
                        match throttle_policy {
                            ThrottlePolicy::Performance => Ok("max-performance".to_string()),
                            ThrottlePolicy::Balanced => Ok("power-saving".to_string()),
                            ThrottlePolicy::Quiet => Ok("power-saving".to_string()),
                        }
                    },
                    Err(_) => {
                        log::warn!("Unable to use asusd to read tdp, asus-wmi interface will be used");
                        self.throttle_thermal_policy().await
                    },
                }
            },
            Err(_) => {
                log::warn!("Unable to use asusd to read tdp, asus-wmi interface will be used");
                self.throttle_thermal_policy().await
            }
        }
    }

    async fn set_power_profile(&mut self, profile: String) -> TDPResult<()> {
        // possible values - "max-performance", "power-saving"

        match profile.as_str() {
            "max-performance" => {
                self.set_throttle_thermal_policy(ThrottlePolicy::Performance).await
            },
            "power-saving" => {
                self.set_throttle_thermal_policy(ThrottlePolicy::Balanced).await
            },
            _ => {
                Ok(())
            },
        }
    }
}
