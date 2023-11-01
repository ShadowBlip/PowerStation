use libryzenadj::RyzenAdj;
use zbus::fdo;
use zbus_macros::dbus_interface;

use crate::performance::gpu::tdp::DBusInterface;

/// Implementation of TDP control for AMD GPUs
pub struct TDP {
    pub path: String,
    pub profile: String,
}

unsafe impl Sync for TDP {} // implementor (RyzenAdj) may be unsafe
unsafe impl Send for TDP {} // implementor (RyzenAdj) may be unsafe

impl TDP {
    /// Create a new TDP instance
    pub fn new(path: String) -> TDP {
        let profile = String::from("power-saving");
        return TDP { path, profile };
    }

    /// Set the current Slow PPT limit using ryzenadj
    fn set_ppt_limit_slow(&mut self, value: u32) -> Result<(), String> {
        log::debug!("Setting slow ppt limit to {}", value);
        let ryzenadj = RyzenAdj::new().map_err(|err| err.to_string())?;
        match ryzenadj.set_slow_limit(value) {
            Ok(x) => return Ok(x),
            Err(e) => {
                let err = format!("Failed to set slow ppt limit: {}", e);
                log::error!("{}", err);
                return Err(String::from(err));
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
        let ryzenadj = RyzenAdj::new().map_err(|err| err.to_string())?;
        match ryzenadj.set_fast_limit(value) {
            Ok(x) => return Ok(x),
            Err(e) => {
                let err = format!("Failed to set fast ppt limit: {}", e);
                log::error!("{}", err);
                return Err(String::from(err));
            }
        }
    }

    /// Get the PPT fast limit
    fn get_ppt_limit_fast(&self) -> Result<f32, String> {
        log::debug!("Getting ppt fast limit");
        let ryzenadj = RyzenAdj::new().map_err(|err| err.to_string())?;
        match ryzenadj.get_fast_limit() {
            Ok(x) => {
                log::debug!("Got fast limit: {}", x);
                Ok(x)
            }
            Err(e) => {
                let err = format!("Failed to get ppt fast limit: {}", e);
                log::error!("{}", err);
                Err(String::from(err))
            }
        }
    }

    /// Set the current TDP value using ryzenadj
    fn set_stapm_limit(&mut self, value: u32) -> Result<(), String> {
        log::debug!("Setting stapm limit to {}", value);
        let ryzenadj = RyzenAdj::new().map_err(|err| err.to_string())?;
        match ryzenadj.set_stapm_limit(value) {
            Ok(x) => {
                log::debug!("Set stapm limit to {}", value);
                Ok(x)
            }
            Err(e) => {
                let err = format!("Failed to set stapm limit: {}", e);
                log::error!("{}", err);
                Err(String::from(err))
            }
        }
    }

    /// Returns the current TDP value using ryzenadj
    fn get_stapm_limit(&self) -> Result<f32, String> {
        log::debug!("Getting stapm limit");
        let ryzenadj = RyzenAdj::new().map_err(|err| err.to_string())?;
        match ryzenadj.get_stapm_limit() {
            Ok(x) => {
                log::debug!("Got stapm limit: {}", x);
                Ok(x)
            }
            Err(e) => {
                let err = format!("Failed to get stapm limit: {}", e);
                log::error!("{}", err);
                Err(String::from(err))
            }
        }
    }

    // Sets the thermal limit value using ryzenadj
    fn set_thm_limit(&mut self, value: u32) -> Result<(), String> {
        log::debug!("Setting thm limit to: {}", value);
        let ryzenadj = RyzenAdj::new().map_err(|err| err.to_string())?;
        match ryzenadj.set_tctl_temp(value) {
            Ok(x) => Ok(x),
            Err(e) => {
                let err = format!("Failed to set tctl limit: {}", e);
                log::error!("{}", err);
                Err(String::from(err))
            }
        }
    }

    /// Returns the current thermal limit value using ryzenadj
    fn get_thm_limit(&self) -> Result<f32, String> {
        log::debug!("Getting thm limit");
        let ryzenadj = RyzenAdj::new().map_err(|err| err.to_string())?;
        match ryzenadj.get_tctl_temp() {
            Ok(x) => Ok(x),
            Err(e) => {
                let err = format!("Failed to get tctl temp: {}", e);
                log::error!("{}", err);
                Err(String::from(err))
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

#[dbus_interface(name = "org.shadowblip.GPU.TDP")]
impl DBusInterface for TDP {
    /// Get the currently set TDP value
    #[dbus_interface(property, name = "TDP")]
    fn tdp(&self) -> fdo::Result<f64> {
        // Get the current stapm limit from ryzenadj
        let stapm_limit =
            TDP::get_stapm_limit(&self).map_err(|err| fdo::Error::Failed(err.to_string()))?;
        return Ok(stapm_limit.into());
    }

    /// Sets the given TDP value
    #[dbus_interface(property, name = "TDP")]
    fn set_tdp(&mut self, value: f64) -> fdo::Result<()> {
        if value < 1.0 {
            let err = "Cowardly refusing to set TDP less than 1";
            log::warn!("{}", err);
            return Err(fdo::Error::InvalidArgs(String::from(err)));
        }

        // Get the current boost value before updating the STAPM limit. We will
        // use this value to also adjust the Fast PPT Limit.
        let fast_ppt_limit =
            TDP::get_ppt_limit_fast(&self).map_err(|err| fdo::Error::Failed(String::from(err)))?;
        let fast_ppt_limit = fast_ppt_limit as f64;
        let stapm_limit =
            TDP::get_stapm_limit(&self).map_err(|err| fdo::Error::Failed(String::from(err)))?;
        let stapm_limit = stapm_limit as f64;

        let boost = fast_ppt_limit - stapm_limit;
        log::debug!("Current boost value is: {}", boost);

        // Update the STAPM limit with the TDP value
        let limit: u32 = (value * 1000.0) as u32;
        TDP::set_stapm_limit(self, limit).map_err(|err| fdo::Error::Failed(String::from(err)))?;

        // Also update the slow PPT limit
        TDP::set_ppt_limit_slow(self, limit)
            .map_err(|err| fdo::Error::Failed(String::from(err)))?;

        // After successfully setting the STAPM limit, we also need to adjust the
        // Fast PPT Limit accordingly so it is *boost* distance away.
        let fast_ppt_limit = ((value + boost) * 1000.0) as u32;
        TDP::set_ppt_limit_fast(self, fast_ppt_limit)
            .map_err(|err| fdo::Error::Failed(String::from(err)))?;

        return Ok(());
    }

    /// The TDP boost for AMD is the total difference between the Fast PPT Limit
    /// and the STAPM limit.
    #[dbus_interface(property)]
    fn boost(&self) -> fdo::Result<f64> {
        let fast_ppt_limit =
            TDP::get_ppt_limit_fast(&self).map_err(|err| fdo::Error::Failed(String::from(err)))?;
        let fast_ppt_limit = fast_ppt_limit as f64;
        let stapm_limit =
            TDP::get_stapm_limit(&self).map_err(|err| fdo::Error::Failed(String::from(err)))?;
        let stapm_limit = stapm_limit as f64;

        let boost = fast_ppt_limit - stapm_limit;

        return Ok(boost);
    }

    #[dbus_interface(property)]
    fn set_boost(&mut self, value: f64) -> fdo::Result<()> {
        if value < 0.0 {
            let err = "Cowardly refusing to set TDP Boost less than 0";
            log::warn!("{}", err);
            return Err(fdo::Error::InvalidArgs(String::from(err)));
        }

        // Get the STAPM Limit so we can calculate what Fast PPT Limit to set.
        let stapm_limit =
            TDP::get_stapm_limit(&self).map_err(|err| fdo::Error::Failed(String::from(err)))?;
        let stapm_limit = stapm_limit as f64;

        // Set the new fast ppt limit
        let fast_ppt_limit = ((stapm_limit + value) * 1000.0) as u32;
        TDP::set_ppt_limit_fast(self, fast_ppt_limit)
            .map_err(|err| fdo::Error::Failed(String::from(err)))?;

        return Ok(());
    }

    #[dbus_interface(property)]
    fn thermal_throttle_limit_c(&self) -> fdo::Result<f64> {
        let limit = TDP::get_thm_limit(&self).map_err(|err| fdo::Error::Failed(err.to_string()))?;
        return Ok(limit.into());
    }

    #[dbus_interface(property)]
    fn set_thermal_throttle_limit_c(&mut self, limit: f64) -> fdo::Result<()> {
        let limit = limit as u32;
        TDP::set_thm_limit(self, limit).map_err(|err| fdo::Error::Failed(err.to_string()))
    }

    #[dbus_interface(property)]
    fn power_profile(&self) -> fdo::Result<String> {
        Ok(self.profile.clone())
    }

    #[dbus_interface(property)]
    fn set_power_profile(&mut self, profile: String) -> fdo::Result<()> {
        TDP::set_power_profile(&self, profile.clone())
            .map_err(|err| fdo::Error::Failed(err.to_string()))?;
        self.profile = profile;
        Ok(())
    }
}
