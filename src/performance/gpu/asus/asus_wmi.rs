use crate::performance::gpu::tdp::{TDPError, TDPResult};

use rog_platform::{
    error::PlatformError,
    firmware_attributes::{AttrValue, FirmwareAttributes},
    platform::RogPlatform,
};

impl From<PlatformError> for TDPError {
    fn from(value: PlatformError) -> Self {
        Self::FailedOperation(value.to_string())
    }
}

/// Implementation of asus-wmi sysfs
/// See https://www.kernel.org/doc/html/v6.8-rc4/admin-guide/abi-testing.html#abi-sys-devices-platform-platform-ppt-apu-sppt
pub struct AsusWmi {
    attributes: Option<FirmwareAttributes>,
    platform: RogPlatform,
}

impl AsusWmi {
    /// test if we are in an asus system with asus-wmi loaded
    pub async fn new() -> Option<Self> {
        match RogPlatform::new() {
            Ok(platform) => {
                log::info!("Module asus-wmi found");
                let attributes: Option<FirmwareAttributes> = Some(FirmwareAttributes::new());
                Some(Self {
                    attributes,
                    platform,
                })
            }
            Err(err) => {
                log::info!("Module asus-wmi not found: {err:?}");
                None
            }
        }
    }

    /// Returns the currently set STAPM value
    pub async fn tdp(&self) -> TDPResult<f64> {
        if let Some(firmware_attributes) = &self.attributes {
            match firmware_attributes
                .attributes()
                .iter()
                .find(|a| a.name() == "ppt_pl1_spl")
            {
                Some(attribute) => match attribute.current_value() {
                    Ok(attr_value) => match attr_value {
                        AttrValue::Integer(value) => Ok(value as f64),
                        _ => Err(TDPError::FailedOperation("Failed to read SPL.".to_string())),
                    },
                    Err(_) => Err(TDPError::FailedOperation("Failed to read SPL.".to_string())),
                },
                None => Err(TDPError::FailedOperation("Failed to read SPL.".to_string())),
            }
        } else {
            Ok(self.platform.get_ppt_pl1_spl()? as f64)
        }
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
        let val = AttrValue::Integer(value as i32);
        if let Some(firmware_attributes) = &self.attributes {
            match firmware_attributes
                .attributes()
                .iter()
                .find(|a| a.name() == "ppt_pl1_spl")
            {
                Some(attribute) => match attribute.set_current_value(val) {
                    Ok(_) => {
                        log::info!("Set SPL to {value}");
                    }
                    Err(e) => {
                        return Err(TDPError::FailedOperation(format!(
                            "Failed to set SPL: {e:}"
                        )));
                    }
                },

                None => return Err(TDPError::FailedOperation("Failed to set SPL.".to_string())),
            }
        } else {
            self.platform.set_ppt_pl1_spl(value as u8)?;
        }

        // Set the boost back to the expeted value with the new TDP
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

        let slow_ppt = if let Some(firmware_attributes) = &self.attributes {
            match firmware_attributes
                .attributes()
                .iter()
                .find(|a| a.name() == "ppt_platform_sppt")
            {
                Some(attribute) => match attribute.current_value() {
                    Ok(attr_value) => match attr_value {
                        AttrValue::Integer(value) => value as f64,
                        _ => {
                            return Err(TDPError::FailedOperation(
                                "Failed to read SPPT.".to_string(),
                            ))
                        }
                    },
                    Err(_) => {
                        return Err(TDPError::FailedOperation(
                            "Failed to read SPPT.".to_string(),
                        ))
                    }
                },
                None => {
                    return Err(TDPError::FailedOperation(
                        "Failed to read SPPT.".to_string(),
                    ))
                }
            }
        } else {
            self.platform.get_ppt_platform_sppt()? as f64
        };

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

        let sppt_val = (stapm + value) as i32;
        let boost = AttrValue::Integer(sppt_val);

        // ppt_platform_sppt will set sppt to value and fppt to value + 25%
        if let Some(firmware_attributes) = &self.attributes {
            match firmware_attributes
                .attributes()
                .iter()
                .find(|a| a.name() == "ppt_platform_sppt")
            {
                Some(attribute) => match attribute.set_current_value(boost) {
                    Ok(_) => {
                        log::info!("Set SPPT to {value}");
                    }
                    Err(e) => {
                        return Err(TDPError::FailedOperation(format!(
                            "Failed to set SPPT: {e:}"
                        )));
                    }
                },

                None => return Err(TDPError::FailedOperation("Failed to set SPPT.".to_string())),
            }
        } else {
            self.platform.set_ppt_platform_sppt(sppt_val as u8)?;
        }

        Ok(())
    }
}
