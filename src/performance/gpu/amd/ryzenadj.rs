#![cfg(target_arch = "x86_64")]
use std::error::Error;

use libryzenadj::RyzenAdj;

use crate::performance::gpu::{
    platform::hardware::Hardware, 
    tdp::{HardwareAccess, TDPDevice, TDPError, TDPResult}
};

/// Steam Deck GPU ID
const DEV_ID_VANGOGH: &str = "163f";
const DEV_ID_SEPHIROTH: &str = "1435";

/// Implementation of TDP control for AMD GPUs
pub struct RyzenAdjTdp {
    //pub path: String,
    pub device_id: String,
    pub profile: String,
    ryzenadj: RyzenAdj,
    pub unsupported_stapm_limit: f32,
    pub unsupported_ppt_limit_fast: f32,
    pub unsupported_thm_limit: f32,
    // We need Hardware for the TDPDevice trait's default methods
    hardware: Option<Hardware>,
}

// Implement HardwareAccess for RyzenAdjTdp
impl HardwareAccess for RyzenAdjTdp {
    fn hardware(&self) -> Option<&Hardware> {
        self.hardware.as_ref()
    }
}

unsafe impl Sync for RyzenAdjTdp {} // implementor (RyzenAdj) may be unsafe
unsafe impl Send for RyzenAdjTdp {} // implementor (RyzenAdj) may be unsafe

impl RyzenAdjTdp {
    /// Create a new TDP instance
    pub fn new(_path: String, device_id: String) -> Result<RyzenAdjTdp, Box<dyn Error>> {
        // Currently there is no known way to read this value
        let profile = String::from("power-saving");

        // Set fake TDP limits for GPUs that don't support ryzenadj monitoring (e.g. Steam Deck)
        let unsupported_stapm_limit: f32 = match device_id.as_str() {
            DEV_ID_VANGOGH => 12.0,
            DEV_ID_SEPHIROTH => 12.0,
            _ => 10.0,
        };
        let unsupported_ppt_limit_fast: f32 = match device_id.as_str() {
            DEV_ID_VANGOGH => 15.0,
            DEV_ID_SEPHIROTH => 15.0,
            _ => 10.0,
        };
        let unsupported_thm_limit: f32 = match device_id.as_str() {
            DEV_ID_VANGOGH => 95.0,
            DEV_ID_SEPHIROTH => 95.0,
            _ => 95.0,
        };
        let ryzenadj = RyzenAdj::new().map_err(|err| err.to_string())?;
        
        // Get hardware instance for min/max TDP values
        let hardware = match Hardware::new() {
            Some(hardware) => {
                log::info!("Found Hardware interface for RyzenAdj TDP control");
                Some(hardware)
            }
            None => None,
        };

        Ok(RyzenAdjTdp {
            //path,
            device_id,
            profile,
            ryzenadj,
            unsupported_stapm_limit,
            unsupported_ppt_limit_fast,
            unsupported_thm_limit,
            hardware,
        })
    }

    /// Returns true if ryzenadj cannot read values from the given GPU
    fn is_unsupported_gpu(&self) -> bool {
        matches!(self.device_id.as_str(), DEV_ID_VANGOGH | DEV_ID_SEPHIROTH)
    }

    /// Set the current Slow PPT limit using ryzenadj
    fn set_ppt_limit_slow(&mut self, value: u32) -> Result<(), String> {
        log::debug!("Setting slow ppt limit to {}", value);
        match self.ryzenadj.set_slow_limit(value) {
            Ok(x) => Ok(x),
            Err(e) => {
                let err = format!("Failed to set slow ppt limit: {}", e);
                log::error!("{}", err);
                Err(err)
            }
        }
    }

    // Get the PPT slow limit
    fn get_ppt_limit_slow(&self) -> Result<f32, String> {
        log::debug!("Getting ppt slow limit");
        
        if let Err(e) = self.ryzenadj.refresh() {
            log::error!("Failed to refresh ryzenadj: {}", e);
        }

        match self.ryzenadj.get_slow_limit() {
            Ok(x) => Ok(x),
            Err(e) => {
                let err = format!("Failed to get ppt slow limit: {}", e);
                log::error!("{}", err);
                Err(err)
            }
        }
    }

    /// Set the current Fast PPT limit using ryzenadj
    fn set_ppt_limit_fast(&mut self, value: u32) -> Result<(), String> {
        log::debug!("Setting fast ppt limit to {}", value);
        match self.ryzenadj.set_fast_limit(value) {
            Ok(x) => {
                // Save the new value for APU's that can't read this attribute.
                if self.is_unsupported_gpu() {
                    self.unsupported_ppt_limit_fast = value as f32;
                }
                Ok(x)
            }
            Err(e) => {
                let err = format!("Failed to set fast ppt limit: {}", e);
                log::error!("{}", err);
                Err(err)
            }
        }
    }

    /// Get the PPT fast limit
    fn _get_ppt_limit_fast(&self) -> Result<f32, String> {
        log::debug!("Getting ppt fast limit");

        // Return what we last set the value to for APU's that can't read this
        // attribute.
        if self.is_unsupported_gpu() {
            return Ok(self.unsupported_ppt_limit_fast);
        }

        if let Err(e) = self.ryzenadj.refresh() {
            log::error!("Failed to refresh ryzenadj: {}", e);
        }

        // Get the fast limit from ryzenadj
        match self.ryzenadj.get_fast_limit() {
            Ok(x) => {
                log::debug!("Got fast limit: {}", x);
                Ok(x)
            }
            Err(e) => {
                let err = format!("Failed to get ppt fast limit: {}", e);
                log::error!("{}", err);
                Err(err)
            }
        }
    }

    /// Set the current TDP value using ryzenadj
    fn set_stapm_limit(&mut self, value: u32) -> Result<(), String> {
        log::debug!("Setting stapm limit to {}", value);
        match self.ryzenadj.set_stapm_limit(value) {
            Ok(x) => {
                log::debug!("Set stapm limit to {}", value);
                // Save the new value for APU's that can't read this attribute.
                if self.is_unsupported_gpu() {
                    self.unsupported_stapm_limit = value as f32;
                }
                Ok(x)
            }
            Err(e) => {
                let err = format!("Failed to set stapm limit: {}", e);
                log::error!("{}", err);
                Err(err)
            }
        }
    }

    /// Returns the current TDP value using ryzenadj
    fn get_stapm_limit(&self) -> Result<f32, String> {
        log::debug!("Getting stapm limit");

        // Return what we last set the value to for APU's that can't read this
        // attribute.
        if self.is_unsupported_gpu() {
            return Ok(self.unsupported_stapm_limit);
        }

        if let Err(e) = self.ryzenadj.refresh() {
            log::error!("Failed to refresh ryzenadj: {}", e);
        }

        // Get the value from ryzenadj
        match self.ryzenadj.get_stapm_limit() {
            Ok(x) => {
                log::debug!("Got stapm limit: {}", x);
                Ok(x)
            }
            Err(e) => {
                let err = format!("Failed to get stapm limit: {}", e);
                log::error!("{}", err);
                Err(err)
            }
        }
    }

    // Sets the thermal limit value using ryzenadj
    fn set_thm_limit(&mut self, value: u32) -> Result<(), String> {
        log::debug!("Setting thm limit to: {}", value);
        match self.ryzenadj.set_tctl_temp(value) {
            Ok(x) => {
                // Save the new value for APU's that can't read this attribute.
                if self.is_unsupported_gpu() {
                    self.unsupported_thm_limit = value as f32;
                }
                Ok(x)
            }

            Err(e) => {
                let err = format!("Failed to set tctl limit: {}", e);
                log::error!("{}", err);
                Err(err)
            }
        }
    }

    /// Returns the current thermal limit value using ryzenadj
    fn get_thm_limit(&self) -> Result<f32, String> {
        log::debug!("Getting thm limit");

        // Return what we last set the value to for APU's that can't read this
        // attribute.
        if self.is_unsupported_gpu() {
            return Ok(self.unsupported_thm_limit);
        }

        if let Err(e) = self.ryzenadj.refresh() {
            log::error!("Failed to refresh ryzenadj: {}", e);
        }

        // Get the value from ryzenadj
        match self.ryzenadj.get_tctl_temp() {
            Ok(x) => Ok(x),
            Err(e) => {
                let err = format!("Failed to get tctl temp: {}", e);
                log::error!("{}", err);
                Err(err)
            }
        }
    }

    /// Set the power profile to the given profile
    fn set_power_profile(&self, profile: String) -> Result<(), String> {
        log::debug!("Setting power profile");
        match profile.as_str() {
            "power-saving" => self
                .ryzenadj
                .set_power_saving()
                .map_err(|err| err.to_string()),
            "max-performance" => self
                .ryzenadj
                .set_max_performance()
                .map_err(|err| err.to_string()),
            _ => Err(String::from(
                "Invalid power profile. Must be in [max-performance, power-saving]",
            )),
        }
    }
}

impl TDPDevice for RyzenAdjTdp {
    async fn tdp(&self) -> TDPResult<f64> {
        // Get the current stapm limit from ryzenadj
        match RyzenAdjTdp::get_stapm_limit(self) {
            Ok(result) => Ok(result.into()),
            Err(err) => Err(TDPError::FailedOperation(err.to_string())),
        }
    }

    async fn set_tdp(&mut self, value: f64) -> TDPResult<()> {
        log::debug!("Setting TDP to: {}", value);
        if value < 1.0 {
            log::warn!("Cowardly refusing to set TDP less than 1W");
            return Err(TDPError::InvalidArgument(format!(
                "Cowardly refusing to set TDP less than 1W: provided {}W",
                value
            )));
        }

        // Get the current boost value before updating the STAPM limit. We will
        // use this value to also adjust the Fast PPT Limit.
        let boost = match self.boost().await {
            Ok(boost) => boost,
            Err(e) => return Err(e),
        };

        // Update the STAPM limit with the TDP value
        let limit: u32 = (value * 1000.0) as u32;
        RyzenAdjTdp::set_stapm_limit(self, limit).map_err(TDPError::FailedOperation)?;

        // Update the s/fppt values with the new TDP
        self.set_boost(boost).await?;

        Ok(())
    }

    async fn boost(&self) -> TDPResult<f64> {
        let slow_ppt_limit =
            RyzenAdjTdp::get_ppt_limit_slow(self).map_err(TDPError::FailedOperation)? as f64;
        let stapm_limit =
            RyzenAdjTdp::get_stapm_limit(self).map_err(TDPError::FailedOperation)? as f64;

        // TODO: Is this a bug in ryzenadj? Sometimes it is ~0
        if slow_ppt_limit < 1.0 {
            log::warn!("Got a slow limit less than 1. Setting boost to 0");
            return Ok(0.0);
        }

        let boost = slow_ppt_limit - stapm_limit;
        Ok(boost)
    }

    async fn set_boost(&mut self, value: f64) -> TDPResult<()> {
        log::debug!("Setting boost to: {}", value);
        if value < 0.0 {
            log::warn!("Cowardly refusing to set TDP Boost less than 0W");
            return Err(TDPError::InvalidArgument(format!(
                "Cowardly refusing to set TDP Boost less than 0W: {}W provided",
                value
            )));
        }

        // Get the STAPM Limit so we can calculate what S/FPPT limits to set.
        let stapm_limit =
            RyzenAdjTdp::get_stapm_limit(self).map_err(TDPError::FailedOperation)? as f64;

        // Set the new slow PPT limit
        let slow_ppt_limit = ((stapm_limit + value) * 1000.0) as u32;
        RyzenAdjTdp::set_ppt_limit_slow(self, slow_ppt_limit).map_err(TDPError::FailedOperation)?;

        // Set the new fast PPT limit
        let fast_ppt_limit = ((stapm_limit + value) * 1250.0) as u32;
        RyzenAdjTdp::set_ppt_limit_fast(self, fast_ppt_limit).map_err(TDPError::FailedOperation)?;

        Ok(())
    }

    async fn thermal_throttle_limit_c(&self) -> TDPResult<f64> {
        let limit = RyzenAdjTdp::get_thm_limit(self)
            .map_err(|err| TDPError::FailedOperation(err.to_string()))?;
        Ok(limit.into())
    }

    async fn set_thermal_throttle_limit_c(&mut self, limit: f64) -> TDPResult<()> {
        log::debug!("Setting thermal throttle limit to: {}", limit);
        let limit = limit as u32;
        RyzenAdjTdp::set_thm_limit(self, limit)
            .map_err(|err| TDPError::FailedOperation(err.to_string()))
    }

    async fn power_profile(&self) -> TDPResult<String> {
        Ok(self.profile.clone())
    }

    async fn set_power_profile(&mut self, profile: String) -> TDPResult<()> {
        log::debug!("Setting power profile to: {}", profile);
        RyzenAdjTdp::set_power_profile(self, profile.clone())
            .map_err(|err| TDPError::FailedOperation(err.to_string()))?;
        self.profile = profile;
        Ok(())
    }

    async fn power_profiles_available(&self) -> TDPResult<Vec<String>> {
        Ok(vec![
            "max-performance".to_string(),
            "power-saving".to_string(),
        ])
    }
}
