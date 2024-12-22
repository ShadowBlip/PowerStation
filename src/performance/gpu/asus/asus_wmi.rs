use crate::performance::gpu::tdp::{TDPError, TDPResult};

use rog_platform::{
    firmware_attributes::{AttrValue, FirmwareAttributes},
    platform::RogPlatform,
};

/// Implementation of asus-wmi sysfs
/// See https://www.kernel.org/doc/html/v6.8-rc4/admin-guide/abi-testing.html#abi-sys-devices-platform-platform-ppt-apu-sppt
pub struct AsusWmi {
    attributes: FirmwareAttributes,
}

impl AsusWmi {
    /// test if we are in an asus system with asus-wmi loaded
    pub async fn new() -> Option<Self> {
        match RogPlatform::new() {
            Ok(_) => {
                log::info!("Module asus-wmi found");
                Some(Self {
                    attributes: FirmwareAttributes::new(),
                })
            }
            Err(err) => {
                log::info!("Module asus-wmi not found: {}", err);
                None
            }
        }
    }

    /// Returns the currently set STAPM value
    pub async fn tdp(&self) -> TDPResult<f64> {
        match self
            .attributes
            .attributes()
            .iter()
            .find(|a| a.name() == "ppt_pl1_spl")
            .unwrap()
            .current_value()
        {
            Ok(attr_value) => match attr_value {
                AttrValue::Integer(value) => {
                    log::info!("Found STAPM value: {value}");
                    Ok(value as f64)
                }
                _ => Err(TDPError::FailedOperation(
                    "Failed to read STAPM value".to_string(),
                )),
            },
            Err(e) => Err(TDPError::FailedOperation(format!(
                "Failed to read STAPM Value: {e:?}"
            ))),
        }
    }

    /// Sets STAPM to the given value and adjusts the SPPT/FPPT to maintaing the current boost
    /// ratio
    pub async fn set_tdp(&mut self, value: f64) -> TDPResult<()> {
        // Get the current Boost value
        let boost = self.boost().await?;

        // Set the STAPM value to the given TDP value
        let val = AttrValue::Integer(value as i32);
        match self
            .attributes
            .attributes_mut()
            .iter()
            .find(|a| a.name() == "ppt_pl1_spl")
            .unwrap()
            .set_current_value(val)
        {
            Ok(_) => {
                log::info!("Set STAPM value to {value}");
            }
            Err(e) => {
                return Err(TDPError::FailedOperation(format!(
                    "Failed to set STAPM value: {e:}"
                )));
            }
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

        let slow_ppt = match self
            .attributes
            .attributes()
            .iter()
            .find(|a| a.name() == "ppt_platform_sppt")
            .unwrap()
            .current_value()
        {
            Ok(attr_value) => match attr_value {
                AttrValue::Integer(value) => {
                    log::info!("Found Slow PPT value: {value}");
                    value as f64
                }
                _ => {
                    return Err(TDPError::FailedOperation(
                        "Failed to read Slow PPT value".to_string(),
                    ))
                }
            },
            Err(e) => {
                return Err(TDPError::FailedOperation(format!(
                    "Failed to read Slow PPT value: {e:?}"
                )))
            }
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
        match self
            .attributes
            .attributes_mut()
            .iter()
            .find(|a| a.name() == "ppt_platform_sppt")
            .unwrap()
            .set_current_value(boost)
        {
            Ok(_) => {
                log::info!("Set Slow PPT to {sppt_val}");
            }
            Err(e) => {
                return Err(TDPError::FailedOperation(format!(
                    "Failed to set Slow PPT: {e:?}"
                )))
            }
        };

        Ok(())
    }
}
