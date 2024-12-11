use crate::performance::gpu::tdp::{TDPError, TDPResult};

use rog_platform::{error::PlatformError, platform::RogPlatform};

impl From<PlatformError> for TDPError {
    fn from(value: PlatformError) -> Self {
        Self::FailedOperation(value.to_string())
    }
}

/// Implementation of asus-wmi sysfs
/// See https://www.kernel.org/doc/html/v6.8-rc4/admin-guide/abi-testing.html#abi-sys-devices-platform-platform-ppt-apu-sppt
pub struct AsusWmi {
    platform: RogPlatform,
}

impl AsusWmi {
    /// test if we are in an asus system with asus-wmi loaded
    pub async fn new() -> Option<Self> {
        match RogPlatform::new() {
            Ok(platform) => {
                log::info!("Module asus-wmi found");
                Some(Self { platform })
            }
            Err(err) => {
                log::info!("Module asus-wmi not found: {err:?}");
                None
            }
        }
    }

    /// Returns the currently set STAPM value
    pub async fn tdp(&self) -> TDPResult<f64> {
        Ok(self.platform.get_ppt_pl1_spl()? as f64)
    }

    /// Sets STAPM to the given value and adjusts the SPPT/FPPT to maintaing the current boost
    /// ratio
    pub async fn set_tdp(&mut self, value: f64) -> TDPResult<()> {
        if value < 1.0 || value > u8::MAX as f64 {
            return Err(TDPError::InvalidArgument(
                "Value must be between 1 and 255".to_string(),
            ));
        }

        // Get the current Boost value
        let boost = self.boost().await?;

        // Set the STAPM value to the given TDP value
        self.platform.set_ppt_pl1_spl(value as u8)?;

        // Set the boost back to the expected value with the new TDP
        self.set_boost(boost).await?;

        Ok(())
    }

    /// Returns the current difference between STAPM and SPPT
    pub async fn boost(&self) -> TDPResult<f64> {
        let stapm = match self.tdp().await {
            Ok(val) => val,
            Err(e) => {
                return Err(e);
            }
        };
        let slow_ppt = self.platform.get_ppt_platform_sppt()? as f64;
        let boost = slow_ppt - stapm;
        log::info!("Found current boost: {boost}");

        Ok(boost)
    }

    /// Sets SPPT and FPPT to the current STAPM plus the given value
    pub async fn set_boost(&mut self, value: f64) -> TDPResult<()> {
        let stapm = match self.tdp().await {
            Ok(val) => val,
            Err(e) => {
                return Err(e);
            }
        };
        if (stapm + value) < 1.0 || (stapm + value) > u8::MAX as f64 {
            return Err(TDPError::InvalidArgument(
                "Combined TDP + Boost value must be between 1 and 255".to_string(),
            ));
        }
        let boost = (stapm + value) as u8;

        // ppt_platform_sppt will set sppt to value and fppt to value + 25%
        self.platform.set_ppt_platform_sppt(boost)?;

        Ok(())
    }
}
