use libryzenadj::RyzenAdj;

use crate::performance::gpu::tdp::{TDPDevice, TDPError, TDPResult};

/// Steam Deck GPU ID
const DEV_ID_VANGOGH: &str = "163f";

/// Implementation of TDP control for AMD GPUs
pub struct TDP {
    pub path: String,
    pub device_id: String,
    pub profile: String,
    pub unsupported_stapm_limit: f32,
    pub unsupported_ppt_limit_fast: f32,
    pub unsupported_thm_limit: f32,
}

unsafe impl Sync for TDP {} // implementor (RyzenAdj) may be unsafe
unsafe impl Send for TDP {} // implementor (RyzenAdj) may be unsafe

impl TDP {
    /// Create a new TDP instance
    pub fn new(path: String, device_id: String) -> TDP {
        // Currently there is no known way to read this value
        let profile = String::from("power-saving");

        // Set fake TDP limits for GPUs that don't support ryzenadj monitoring (e.g. Steam Deck)
        let unsupported_stapm_limit: f32 = match device_id.as_str() {
            DEV_ID_VANGOGH => 12.0,
            _ => 10.0,
        };
        let unsupported_ppt_limit_fast: f32 = match device_id.as_str() {
            DEV_ID_VANGOGH => 15.0,
            _ => 10.0,
        };
        let unsupported_thm_limit: f32 = match device_id.as_str() {
            DEV_ID_VANGOGH => 95.0,
            _ => 95.0,
        };

        TDP {
            path,
            device_id,
            profile,
            unsupported_stapm_limit,
            unsupported_ppt_limit_fast,
            unsupported_thm_limit,
        }
    }

    /// Returns true if ryzenadj cannot read values from the given GPU
    fn is_unsupported_gpu(&self) -> bool {
        self.device_id == DEV_ID_VANGOGH
    }

    /// Set the current Slow PPT limit using ryzenadj
    fn set_ppt_limit_slow(&mut self, value: u32) -> Result<(), String> {
        log::debug!("Setting slow ppt limit to {}", value);
        let ryzenadj = RyzenAdj::new().map_err(|err| err.to_string())?;
        match ryzenadj.set_slow_limit(value) {
            Ok(x) => Ok(x),
            Err(e) => {
                let err = format!("Failed to set slow ppt limit: {}", e);
                log::error!("{}", err);
                Err(err)
            }
        }
    }

    //// Get the PPT slow limit
    //fn get_ppt_limit_slow(&self) -> Result<f32, String> {
    //    log::debug!("Getting ppt slow limit");
    //    let ryzenadj = RyzenAdj::new().map_err(|err| err.to_string())?;
    //    match ryzenadj.get_slow_limit() {
    //        Ok(x) => Ok(x),
    //        Err(e) => {
    //            let err = format!("Failed to get ppt slow limit: {}", e);
    //            log::error!("{}", err);
    //            Err(String::from(err))
    //        }
    //    }
    //}

    /// Set the current Fast PPT limit using ryzenadj
    fn set_ppt_limit_fast(&mut self, value: u32) -> Result<(), String> {
        log::debug!("Setting fast ppt limit to {}", value);
        if self.is_unsupported_gpu() {
            self.unsupported_ppt_limit_fast = value as f32;
        }
        let ryzenadj = RyzenAdj::new().map_err(|err| err.to_string())?;
        match ryzenadj.set_fast_limit(value) {
            Ok(x) => Ok(x),
            Err(e) => {
                let err = format!("Failed to set fast ppt limit: {}", e);
                log::error!("{}", err);
                Err(err)
            }
        }
    }

    /// Get the PPT fast limit
    fn get_ppt_limit_fast(&self) -> Result<f32, String> {
        log::debug!("Getting ppt fast limit");

        // Return what we _think_ the value is for unsupported GPUs
        if self.is_unsupported_gpu() {
            return Ok(self.unsupported_ppt_limit_fast);
        }

        // Get the fast limit from ryzenadj
        let ryzenadj = RyzenAdj::new().map_err(|err| err.to_string())?;
        match ryzenadj.get_fast_limit() {
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
        if self.is_unsupported_gpu() {
            self.unsupported_stapm_limit = value as f32;
        }
        let ryzenadj = RyzenAdj::new().map_err(|err| err.to_string())?;
        match ryzenadj.set_stapm_limit(value) {
            Ok(x) => {
                log::debug!("Set stapm limit to {}", value);
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

        // Return what we _think_ the value is for unsupported GPUs
        if self.is_unsupported_gpu() {
            return Ok(self.unsupported_stapm_limit);
        }

        // Get the value from ryzenadj
        let ryzenadj = RyzenAdj::new().map_err(|err| err.to_string())?;
        match ryzenadj.get_stapm_limit() {
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
        if self.is_unsupported_gpu() {
            self.unsupported_thm_limit = value as f32;
        }
        let ryzenadj = RyzenAdj::new().map_err(|err| err.to_string())?;
        match ryzenadj.set_tctl_temp(value) {
            Ok(x) => Ok(x),
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

        // Return what we _think_ the value is for unsupported GPUs
        if self.is_unsupported_gpu() {
            return Ok(self.unsupported_thm_limit);
        }

        // Get the value from ryzenadj
        let ryzenadj = RyzenAdj::new().map_err(|err| err.to_string())?;
        match ryzenadj.get_tctl_temp() {
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
        let ryzenadj = RyzenAdj::new().map_err(|err| err.to_string())?;
        match profile.as_str() {
            "power-saving" => ryzenadj.set_power_saving().map_err(|err| err.to_string()),
            "max-performance" => ryzenadj
                .set_max_performance()
                .map_err(|err| err.to_string()),
            _ => Err(String::from(
                "Invalid power profile. Must be in [max-performance, power-saving]",
            )),
        }
    }
}

impl TDPDevice for TDP {

    fn tdp(&self) -> TDPResult<f64> {
        // Get the current stapm limit from ryzenadj
        match TDP::get_stapm_limit(&self) {
            Ok(result) => Ok(result.into()),
            Err(err) => Err(TDPError::FailedOperation(err.to_string()))
        }
    }

    fn set_tdp(&mut self, value: f64) -> TDPResult<()> {
        log::debug!("Setting TDP to: {}", value);
        if value < 1.0 {
            log::warn!("Cowardly refusing to set TDP less than 1W");
            return Err(TDPError::InvalidArgument(format!("Cowardly refusing to set TDP less than 1W: provided {}W", value)));
        }


        // Get the current boost value before updating the STAPM limit. We will
        // use this value to also adjust the Fast PPT Limit.
        let fast_ppt_limit =
            TDP::get_ppt_limit_fast(&self).map_err(|err| TDPError::FailedOperation(err))?;
        let mut fast_ppt_limit = fast_ppt_limit as f64;
        let stapm_limit = TDP::get_stapm_limit(&self).map_err(|err| TDPError::FailedOperation(err))?;
        let stapm_limit = stapm_limit as f64;

        // TODO: Is this a bug in ryzenadj? Sometimes fast_ppt_limit is ~0
        if fast_ppt_limit < 1.0 {
            log::warn!("Got a fast limit less than 1. Possible ryzenadj bug?");
            fast_ppt_limit = stapm_limit;
        }

        let boost = fast_ppt_limit - stapm_limit;
        log::debug!("Current boost value is: {}", boost);

        // Update the STAPM limit with the TDP value
        let limit: u32 = (value * 1000.0) as u32;
        TDP::set_stapm_limit(self, limit).map_err(|err| TDPError::FailedOperation(err))?;

        // Also update the slow PPT limit
        TDP::set_ppt_limit_slow(self, limit).map_err(|err| TDPError::FailedOperation(err))?;

        // After successfully setting the STAPM limit, we also need to adjust the
        // Fast PPT Limit accordingly so it is *boost* distance away.
        let fast_ppt_limit = ((value + boost) * 1000.0) as u32;
        TDP::set_ppt_limit_fast(self, fast_ppt_limit).map_err(|err| TDPError::FailedOperation(err))?;

        Ok(())
    }

    fn boost(&self) -> TDPResult<f64> {
        let fast_ppt_limit =
            TDP::get_ppt_limit_fast(&self).map_err(|err| TDPError::FailedOperation(String::from(err)))?;
        let fast_ppt_limit = fast_ppt_limit as f64;
        let stapm_limit =
            TDP::get_stapm_limit(&self).map_err(|err| TDPError::FailedOperation(String::from(err)))?;
        let stapm_limit = stapm_limit as f64;

        let boost = fast_ppt_limit - stapm_limit;

        Ok(boost)
    }

    fn set_boost(&mut self, value: f64) -> TDPResult<()> {
        log::debug!("Setting boost to: {}", value);
        if value < 0.0 {
            log::warn!("Cowardly refusing to set TDP Boost less than 0W");
            return Err(TDPError::InvalidArgument(format!("Cowardly refusing to set TDP Boost less than 0W: {}W provided", value)));
        }

        // Get the STAPM Limit so we can calculate what Fast PPT Limit to set.
        let stapm_limit = TDP::get_stapm_limit(&self).map_err(|err| TDPError::FailedOperation(err))?;
        let stapm_limit = stapm_limit as f64;

        // Set the new fast ppt limit
        let fast_ppt_limit = ((stapm_limit + value) * 1000.0) as u32;
        TDP::set_ppt_limit_fast(self, fast_ppt_limit).map_err(|err| TDPError::FailedOperation(err))?;

        Ok(())
    }

    fn thermal_throttle_limit_c(&self) -> TDPResult<f64> {
        let limit = TDP::get_thm_limit(&self).map_err(|err| TDPError::FailedOperation(err.to_string()))?;
        Ok(limit.into())
    }

    fn set_thermal_throttle_limit_c(&mut self, limit: f64) -> TDPResult<()> {
        log::debug!("Setting thermal throttle limit to: {}", limit);
        let limit = limit as u32;
        TDP::set_thm_limit(self, limit).map_err(|err| TDPError::FailedOperation(err.to_string()))
    }

    fn power_profile(&self) -> TDPResult<String> {
        Ok(self.profile.clone())
    }

    fn set_power_profile(&mut self, profile: String) -> TDPResult<()> {
        log::debug!("Setting power profile to: {}", profile);
        TDP::set_power_profile(&self, profile.clone())
            .map_err(|err| TDPError::FailedOperation(err.to_string()))?;
        self.profile = profile;
        Ok(())
    }
}
