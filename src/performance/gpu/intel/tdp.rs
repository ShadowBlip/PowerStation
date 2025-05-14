use std::fs::{self, OpenOptions};
use std::io::Write;

use crate::performance::gpu::{
    platform::hardware::Hardware,
    tdp::{HardwareAccess, TDPDevice, TDPError, TDPResult},
};

/// Implementation of TDP control for Intel GPUs
pub struct Tdp {
    //pub path: String,
    hardware: Option<Hardware>,
}

// Implement HardwareAccess for Tdp
impl HardwareAccess for Tdp {
    fn hardware(&self) -> Option<&Hardware> {
        self.hardware.as_ref()
    }
}

impl Tdp {
    pub fn new(_path: String) -> Tdp {
        let hardware = match Hardware::new() {
            Some(hardware) => {
                log::info!("Found Hardware interface for TDP control");
                Some(hardware)
            }
            None => None,
        };

        Tdp {
            hardware,
            //path
        }
    }
}

impl TDPDevice for Tdp {
    async fn tdp(&self) -> TDPResult<f64> {
        let path = "/sys/class/powercap/intel-rapl/intel-rapl:0/constraint_0_power_limit_uw";
        let result = fs::read_to_string(path);
        let content = result.map_err(|err| TDPError::IOError(err.to_string()))?;
        let content = content.trim();

        // Parse the output to get the long TDP
        let long_tdp = match content.parse::<f64>() {
            Ok(v) => v,
            Err(e) => {
                log::error!("{}", e);
                return Err(TDPError::FailedOperation(e.to_string()));
            }
        };

        Ok(long_tdp / 1000000.0)
    }

    async fn set_tdp(&mut self, value: f64) -> TDPResult<()> {
        if value < 1.0 {
            let err = "Cowardly refusing to set TDP less than 1";
            log::warn!("{}", err);
            return Err(TDPError::InvalidArgument(String::from(err)));
        }

        // Get the current boost value so the peak tdp can be set *boost*
        // distance away.
        let mut boost = self.boost().await?;
        if boost < 0.0 {
            log::warn!("Boost is less than 0, setting to 0");
            boost = 0.0;
        }

        // Open the sysfs file to write to
        let path = "/sys/class/powercap/intel-rapl/intel-rapl:0/constraint_0_power_limit_uw";
        let file = OpenOptions::new().write(true).open(path);

        // Convert the value to a writable string
        let value = format!("{}", value * 1000000.0);

        // Write the value
        file.map_err(|err| TDPError::FailedOperation(err.to_string()))?
            .write_all(value.as_bytes())
            .map_err(|err| TDPError::IOError(err.to_string()))?;

        // Update the boost value
        self.set_boost(boost).await
    }

    async fn boost(&self) -> TDPResult<f64> {
        let path = "/sys/class/powercap/intel-rapl/intel-rapl:0/constraint_1_power_limit_uw";
        let result = fs::read_to_string(path);
        let content = result.map_err(|err| TDPError::IOError(err.to_string()))?;
        let content = content.trim();

        // Parse the output to get the peak TDP
        let peak_tdp = match content.parse::<f64>() {
            Ok(v) => v,
            Err(e) => {
                log::error!("{}", e);
                return Err(TDPError::FailedOperation(e.to_string()));
            }
        };

        let tdp = self.tdp().await?;
        Ok((peak_tdp / 1000000.0) - tdp)
    }

    async fn set_boost(&mut self, value: f64) -> TDPResult<()> {
        log::debug!("Setting Boost: {}", value);
        if value < 0.0 {
            let err = "Cowardly refusing to set TDP Boost less than 0";
            log::warn!("{}", err);
            return Err(TDPError::InvalidArgument(String::from(err)));
        }

        let tdp = self.tdp().await?;
        let boost = value;
        let short_tdp = if boost > 0.0 {
            (boost + tdp) * 1000000.0
        } else {
            tdp * 1000000.0
        };

        // Write the short tdp
        let path = "/sys/class/powercap/intel-rapl/intel-rapl:0/constraint_1_power_limit_uw";
        let file = OpenOptions::new().write(true).open(path);
        let value = format!("{}", short_tdp);
        file.map_err(|err| TDPError::FailedOperation(err.to_string()))?
            .write_all(value.as_bytes())
            .map_err(|err| TDPError::IOError(err.to_string()))
    }

    async fn thermal_throttle_limit_c(&self) -> TDPResult<f64> {
        log::error!("Thermal throttling not supported on intel gpu");
        Err(TDPError::FeatureUnsupported)
    }

    async fn set_thermal_throttle_limit_c(&mut self, _limit: f64) -> TDPResult<()> {
        log::error!("Thermal throttling not supported on intel gpu");
        Err(TDPError::FeatureUnsupported)
    }

    async fn power_profile(&self) -> TDPResult<String> {
        log::error!("Power profiles not supported on intel gpu");
        Err(TDPError::FeatureUnsupported)
    }

    async fn set_power_profile(&mut self, _profile: String) -> TDPResult<()> {
        log::error!("Power profiles not supported on intel gpu");
        Err(TDPError::FeatureUnsupported)
    }

    async fn power_profiles_available(&self) -> TDPResult<Vec<String>> {
        log::error!("Power profiles not supported on intel gpu");
        Err(TDPError::FeatureUnsupported)
    }
}
