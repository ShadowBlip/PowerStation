use crate::performance::gpu::{
    acpi::firmware::Acpi,
    asus::asus_wmi::AsusWmi,
    platform::hardware::Hardware,
    tdp::{TDPDevice, TDPError, TDPResult},
};

#[cfg(target_arch = "x86_64")]
use super::ryzenadj::RyzenAdjTdp;

/// Implementation of TDP control for AMD GPUs
pub struct Tdp {
    asus_wmi: Option<AsusWmi>,
    acpi: Option<Acpi>,
    #[cfg(target_arch = "x86_64")]
    ryzenadj: Option<RyzenAdjTdp>,
    hardware: Option<Hardware>,
}

impl Tdp {
    pub async fn new(path: &str, device_id: &str) -> Tdp {
        let asus_wmi = match AsusWmi::new().await {
            Some(asus_wmi) => {
                log::info!("Found Asus WMI interface for TDP control");
                Some(asus_wmi)
            }
            None => None,
        };

        let acpi = match Acpi::new().await {
            Some(acpi) => {
                log::info!("Found ACPI interface for platform profile control");
                Some(acpi)
            }
            None => None,
        };

        #[cfg(target_arch = "x86_64")]
        let ryzenadj = match RyzenAdjTdp::new(path.to_string(), device_id.to_string()) {
            Ok(ryzenadj) => {
                log::info!("Found RyzenAdj interface for TDP control");
                Some(ryzenadj)
            }
            Err(e) => {
                log::warn!("Failed to create Ryzenadj Instance: {e:?}");
                None
            }
        };

        let hardware = match Hardware::new() {
            Some(hardware) => {
                log::info!("Found Hardware interface for TDP control");
                Some(hardware)
            }
            None => None,
        };

        Tdp {
            asus_wmi,
            acpi,
            #[cfg(target_arch = "x86_64")]
            ryzenadj,
            hardware,
        }
    }
}

impl TDPDevice for Tdp {
    async fn tdp(&self) -> TDPResult<f64> {
        log::info!("Get TDP");

        // TODO: set platform profile based on % of max TDP.
        if self.asus_wmi.is_some() {
            let asus_wmi = self.asus_wmi.as_ref().unwrap();
            match asus_wmi.tdp().await {
                Ok(tdp) => {
                    log::info!("TDP is currently {tdp}");
                    return Ok(tdp);
                }
                Err(e) => {
                    log::warn!("Failed to read current TDP using Asus WMI: {e:?}");
                }
            };
        };
        // TODO: set platform profile based on % of max TDP.
        #[cfg(target_arch = "x86_64")]
        if self.ryzenadj.is_some() {
            let ryzenadj = self.ryzenadj.as_ref().unwrap();
            match ryzenadj.tdp().await {
                Ok(tdp) => {
                    log::info!("TDP is currently {tdp}");
                    return Ok(tdp);
                }
                Err(e) => {
                    log::warn!("Failed to read current TDP using RyzenAdj: {e:?}");
                }
            };
        };
        Err(TDPError::FailedOperation(
            "No TDP Interface available to read TDP.".into(),
        ))
    }

    async fn set_tdp(&mut self, value: f64) -> TDPResult<()> {
        log::info!("Set TDP");
        if self.asus_wmi.is_some() {
            let asus_wmi = self.asus_wmi.as_mut().unwrap();
            match asus_wmi.set_tdp(value).await {
                Ok(_) => {
                    log::info!("TDP set to {value}");
                    return Ok(());
                }
                Err(e) => {
                    log::warn!("Failed to set TDP using Asus WMI: {e:?}");
                }
            };
        };
        #[cfg(target_arch = "x86_64")]
        if self.ryzenadj.is_some() {
            let ryzenadj = self.ryzenadj.as_mut().unwrap();
            match ryzenadj.set_tdp(value).await {
                Ok(_) => {
                    log::info!("TDP set to {value}");
                    return Ok(());
                }
                Err(e) => {
                    log::warn!("Failed to set TDP using RyzenAdj: {e:?}");
                }
            };
        };
        Err(TDPError::FailedOperation(
            "No TDP Interface available to set TDP.".into(),
        ))
    }

    async fn min_tdp(&self) -> TDPResult<f64> {
        log::info!("Get TDP Min");
        if self.hardware.is_some() {
            let hardware = self.hardware.as_ref().unwrap();
            return Ok(hardware.min_tdp());
        }
        Err(TDPError::FailedOperation(
            "No Hardware interface available to read min TDP.".into(),
        ))
    }

    async fn max_tdp(&self) -> TDPResult<f64> {
        log::info!("Get TDP Max");
        if self.hardware.is_some() {
            let hardware = self.hardware.as_ref().unwrap();
            return Ok(hardware.max_tdp());
        }
        Err(TDPError::FailedOperation(
            "No Hardware interface available to read max TDP.".into(),
        ))
    }

    async fn boost(&self) -> TDPResult<f64> {
        log::info!("Get TDP Boost");
        if self.asus_wmi.is_some() {
            let asus_wmi = self.asus_wmi.as_ref().unwrap();
            match asus_wmi.boost().await {
                Ok(boost) => {
                    log::info!("Boost is currently {boost}");
                    return Ok(boost);
                }
                Err(e) => {
                    log::warn!("Failed to read current boost using Asus WMI: {e:?}");
                }
            };
        };
        #[cfg(target_arch = "x86_64")]
        if self.ryzenadj.is_some() {
            let ryzenadj = self.ryzenadj.as_ref().unwrap();
            match ryzenadj.boost().await {
                Ok(boost) => {
                    log::info!("Boost is currently {boost}");
                    return Ok(boost);
                }
                Err(e) => {
                    log::warn!("Failed to read current boost using RyzenAdj: {e:?}");
                }
            };
        };
        Err(TDPError::FailedOperation(
            "No TDP Interface available to read boost.".into(),
        ))
    }

    async fn set_boost(&mut self, value: f64) -> TDPResult<()> {
        log::info!("Set TDP Boost");
        if self.asus_wmi.is_some() {
            let asus_wmi = self.asus_wmi.as_mut().unwrap();
            match asus_wmi.set_boost(value).await {
                Ok(_) => {
                    log::info!("Boost set to {value}");
                    return Ok(());
                }
                Err(e) => {
                    log::warn!("Failed to set boost using Asus WMI: {e:?}");
                }
            };
        };
        #[cfg(target_arch = "x86_64")]
        if self.ryzenadj.is_some() {
            let ryzenadj = self.ryzenadj.as_mut().unwrap();
            match ryzenadj.set_boost(value).await {
                Ok(_) => {
                    log::info!("Boost set to {value}");
                    return Ok(());
                }
                Err(e) => {
                    log::warn!("Failed to set boost using RyzenAdj: {e:?}");
                }
            };
        };
        Err(TDPError::FailedOperation(
            "No TDP Interface available to set boost.".into(),
        ))
    }

    async fn max_boost(&self) -> TDPResult<f64> {
        log::info!("Get TDP Max Boost");
        if self.hardware.is_some() {
            let hardware = self.hardware.as_ref().unwrap();
            return Ok(hardware.max_boost());
        }
        Err(TDPError::FailedOperation(
            "No Hardware interface available to read max boost.".into(),
        ))
    }

    async fn thermal_throttle_limit_c(&self) -> TDPResult<f64> {
        log::info!("Get tctl limit");
        #[cfg(target_arch = "x86_64")]
        if self.ryzenadj.is_some() {
            let ryzenadj = self.ryzenadj.as_ref().unwrap();
            match ryzenadj.thermal_throttle_limit_c().await {
                Ok(limit) => {
                    log::info!("Thermal throttle limit is currently {limit}");
                    return Ok(limit);
                }
                Err(e) => {
                    log::warn!("Failed to read thermal trottle limit using RyzenAdj: {e:?}");
                }
            };
        };
        Err(TDPError::FailedOperation(
            "No TDP Interface available to read thermal throttle limit.".into(),
        ))
    }

    async fn set_thermal_throttle_limit_c(&mut self, limit: f64) -> TDPResult<()> {
        log::info!("Set tctl limit");
        #[cfg(target_arch = "x86_64")]
        if self.ryzenadj.is_some() {
            let ryzenadj = self.ryzenadj.as_mut().unwrap();
            match ryzenadj.set_thermal_throttle_limit_c(limit).await {
                Ok(_) => {
                    log::info!("Thermal throttle limit was set to {:e}", limit as i32);
                    return Ok(());
                }
                Err(e) => {
                    log::warn!("Failed to set thermal trottle limit using RyzenAdj: {e:?}");
                }
            };
        };
        Err(TDPError::FailedOperation(
            "No TDP Interface available to set thermal throttle limit.".into(),
        ))
    }

    async fn power_profile(&self) -> TDPResult<String> {
        log::info!("Get power_profile");
        if self.acpi.is_some() {
            let acpi = self.acpi.as_ref().unwrap();
            match acpi.power_profile().await {
                Ok(profile) => {
                    log::info!("Power profile is currently {profile}");
                    return Ok(profile);
                }
                Err(e) => {
                    log::warn!("Failed to read power profile using ACPI: {e:?}");
                }
            };
        };

        #[cfg(target_arch = "x86_64")]
        if self.ryzenadj.is_some() {
            let ryzenadj = self.ryzenadj.as_ref().unwrap();
            match ryzenadj.power_profile().await {
                Ok(profile) => {
                    log::info!("Power profile is currently {profile}");
                    return Ok(profile);
                }
                Err(e) => {
                    log::warn!("Failed to read power profile using RyzenAdj: {e:?}");
                }
            };
        };
        Err(TDPError::FailedOperation(
            "No TDP Interface available to read power profile.".into(),
        ))
    }

    async fn set_power_profile(&mut self, profile: String) -> TDPResult<()> {
        log::info!("Set power_profile");
        if self.acpi.is_some() {
            let acpi = self.acpi.as_mut().unwrap();
            match acpi.set_power_profile(profile.clone()).await {
                Ok(_) => {
                    log::info!("Power profile was set to {profile}");
                    return Ok(());
                }
                Err(e) => {
                    log::warn!("Failed to set power profile using ACPI: {e:?}");
                }
            };
        };

        #[cfg(target_arch = "x86_64")]
        if self.ryzenadj.is_some() {
            let ryzenadj = self.ryzenadj.as_mut().unwrap();
            match ryzenadj.set_power_profile(profile.clone()).await {
                Ok(_) => {
                    log::info!("Power profile was set to {profile}");
                    return Ok(());
                }
                Err(e) => {
                    log::warn!("Failed to set power profile using RyzenAdj: {e:?}");
                }
            };
        };
        Err(TDPError::FailedOperation(
            "No TDP Interface available to set power profile.".into(),
        ))
    }

    async fn power_profiles_available(&self) -> TDPResult<Vec<String>> {
        if self.acpi.is_some() {
            let acpi = self.acpi.as_ref().unwrap();
            match acpi.power_profiles_available().await {
                Ok(profiles) => {
                    log::info!("Available power profiles are {profiles:?}");
                    return Ok(profiles);
                }
                Err(e) => {
                    log::warn!("Failed to read available power profiles using ACPI: {e:?}");
                }
            };
        };
        #[cfg(target_arch = "x86_64")]
        if self.ryzenadj.is_some() {
            let ryzenadj = self.ryzenadj.as_ref().unwrap();
            match ryzenadj.power_profiles_available().await {
                Ok(profiles) => {
                    log::info!("Available power profiles are {profiles:?}");
                    return Ok(profiles);
                }
                Err(e) => {
                    log::warn!("Failed to read available power profiles using RyzenAdj: {e:?}");
                }
            };
        };
        Err(TDPError::FailedOperation(
            "No TDP Interface available to list available power profiles.".into(),
        ))
    }
}
