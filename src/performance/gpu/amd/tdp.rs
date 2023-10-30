use std::sync::{Mutex, MutexGuard};

use libryzenadj::RyzenAdj;
use zbus::fdo;
use zbus_macros::dbus_interface;

use crate::performance::gpu::tdp::DBusInterface;

/// Implementation of TDP control for AMD GPUs
pub struct TDP {
    ryzenadj: Option<Mutex<RyzenAdj>>,
    pub path: String,
}

unsafe impl Sync for TDP {} // implementor (RyzenAdj) may be unsafe
unsafe impl Send for TDP {} // implementor (RyzenAdj) may be unsafe

impl TDP {
    /// Create a new TDP instance
    pub fn new(path: String) -> TDP {
        let ryzenadj: Option<Mutex<RyzenAdj>> = match RyzenAdj::new() {
            Ok(x) => Some(Mutex::new(x)),
            Err(e) => {
                log::error!("RyzenAdj failed to init: {}", e);
                None
            }
        };
        return TDP { path, ryzenadj };
    }

    /// Returns the RyazenAdj instance from the mutex lock
    fn acquire_ryzenadj(&self) -> Result<MutexGuard<'_, RyzenAdj>, String> {
        let mutex = match &self.ryzenadj {
            Some(x) => x,
            None => {
                log::error!("RyzenAdj unavailable");
                return Err(String::from("RyzenAdj unavailable"));
            }
        };

        let lock = match mutex.lock() {
            Ok(x) => x,
            Err(e) => {
                log::error!("RyzenAdj lock acquired failed: {}", e);
                return Err(String::from(format!("RyzenAdj lock acquire failed: {}", e)));
            }
        };

        return Ok(lock);
    }

    /// Set the current Slow PPT limit using ryzenadj
    fn set_ppt_limit_slow(&mut self, value: u32) -> Result<(), String> {
        log::debug!("Setting slow ppt limit to {}", value);
        let lock = self.acquire_ryzenadj()?;
        match lock.set_slow_limit(value) {
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
    //    let lock = self.acquire_ryzenadj()?;
    //    match lock.get_slow_limit() {
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
        let lock = self.acquire_ryzenadj()?;
        match lock.set_fast_limit(value) {
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
        let lock = self.acquire_ryzenadj()?;
        match lock.get_fast_limit() {
            Ok(x) => Ok(x),
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
        let lock = self.acquire_ryzenadj()?;
        match lock.set_stapm_limit(value) {
            Ok(x) => return Ok(x),
            Err(e) => {
                let err = format!("Failed to set stapm limit: {}", e);
                log::error!("{}", err);
                return Err(String::from(err));
            }
        }
    }

    /// Returns the current TDP value using ryzenadj
    fn get_stapm_limit(&self) -> Result<f32, String> {
        log::debug!("Getting stapm limit");
        let lock = self.acquire_ryzenadj()?;
        match lock.get_stapm_limit() {
            Ok(x) => Ok(x),
            Err(e) => {
                let err = format!("Failed to get stapm limit: {}", e);
                log::error!("{}", err);
                Err(String::from(err))
            }
        }
    }

    /// Returns the current thermal limit value using ryzenadj
    fn get_thm_limit(&self) -> Result<f32, String> {
        log::debug!("Getting thm limit");
        let lock = self.acquire_ryzenadj()?;
        match lock.get_tctl_temp() {
            Ok(x) => Ok(x),
            Err(e) => {
                let err = format!("Failed to get tctl temp: {}", e);
                log::error!("{}", err);
                Err(String::from(err))
            }
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
        let boost = self.boost()?;

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

    fn thermal_profile(&self) -> fdo::Result<u32> {
        todo!();
    }

    fn set_thermal_profile(&mut self, _profile: u32) -> fdo::Result<()> {
        todo!();
    }

    fn thermal_throttle_limit_c(&self) -> fdo::Result<f64> {
        let limit = TDP::get_thm_limit(&self).map_err(|err| fdo::Error::Failed(err.to_string()))?;
        return Ok(limit.into());
    }

    fn set_thermal_throttle_limit_c(&mut self, _limit: f64) -> fdo::Result<()> {
        todo!();
    }
}
