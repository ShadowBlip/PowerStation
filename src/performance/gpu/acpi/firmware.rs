use crate::performance::gpu::tdp::{TDPError, TDPResult};

use std::fs;

const PLATFORM_PROFILE_PATH: &str = "/sys/firmware/acpi/platform_profile";
const PLATFORM_PROFILES_AVAIAL_PATH: &str = "/sys/firmware/acpi/platform_profile_choices";

/// Implementation of acpi sysfs
pub struct Acpi {}

impl Acpi {
    /// Check if ACPI supports platform profiles on this device
    pub async fn new() -> Option<Self> {
        if fs::metadata(PLATFORM_PROFILE_PATH).is_err()
            || fs::metadata(PLATFORM_PROFILES_AVAIAL_PATH).is_err()
        {
            return None;
        }
        Some(Self {})
    }

    /// Reads the currently set power profile
    pub async fn power_profile(&self) -> TDPResult<String> {
        match fs::read(PLATFORM_PROFILE_PATH) {
            Ok(data) => {
                let profile = match String::from_utf8(data) {
                    Ok(profile) => profile.split_whitespace().collect(),
                    Err(e) => {
                        return Err(TDPError::IOError(format!(
                            "Failed to convert utf8 data  to string: {e:?}"
                        )))
                    }
                };

                log::info!("Platform profile is currently set to {profile}");
                Ok(profile)
            }
            Err(e) => Err(TDPError::IOError(format!(
                "Failed to read platform profile: {e:?}"
            ))),
        }
    }

    /// Returns a list of valid power profiles for this interface
    pub async fn power_profiles_available(&self) -> TDPResult<Vec<String>> {
        match fs::read(PLATFORM_PROFILES_AVAIAL_PATH) {
            Ok(data) => {
                let profiles_raw = match String::from_utf8(data) {
                    Ok(profile) => profile,
                    Err(e) => {
                        return Err(TDPError::IOError(format!(
                            "Failed to convert utf8 data  to string: {e:?}"
                        )))
                    }
                };
                let mut profiles = Vec::new();
                for profile in profiles_raw.split_whitespace() {
                    profiles.push(profile.to_string());
                }

                log::info!("Available platform profiles: {profiles:?}");
                Ok(profiles)
            }
            Err(e) => Err(TDPError::IOError(format!(
                "Failed to read platform profile: {e:?}"
            ))),
        }
    }

    /// Sets the power profile to the given value
    pub async fn set_power_profile(&mut self, profile: String) -> TDPResult<()> {
        // Get the current profile so we can check if it needs to be set.
        let current = self.power_profile().await?;
        if current == profile {
            return Ok(());
        }

        let valid_profiles = self.power_profiles_available().await?;
        //TODO: This supports a legacy interface from when only RyzenAdj was supported. Once
        //OpenGamepadUI is updated to use the new power_profiles_available methods, switch to
        //only returning and error here.
        if !valid_profiles.contains(&profile) {
            log::warn!("Incompatible profile requested: {profile}. Attempting to translate to valid profile.");
            match profile.as_str() {
                "max-performance" => match fs::write(PLATFORM_PROFILE_PATH, "performance") {
                    Ok(_) => {
                        log::info!("Set platform perfomance profile to performance");
                        return Ok(());
                    }
                    Err(e) => {
                        return Err(TDPError::IOError(format!(
                            "Failed to set power profile: {e:?}"
                        )))
                    }
                },
                "power-saving" => match fs::write(PLATFORM_PROFILE_PATH, "balanced") {
                    Ok(_) => {
                        log::info!("Set platform perfomance profile to balanced");
                        return Ok(());
                    }
                    Err(e) => {
                        return Err(TDPError::IOError(format!(
                            "Failed to set power profile: {e:?}"
                        )))
                    }
                },
                _ => {
                    return Err(TDPError::InvalidArgument(format!(
                        "{profile} is not a valid profile for Asus WMI."
                    )))
                }
            };
        };

        match fs::write(PLATFORM_PROFILE_PATH, profile.clone()) {
            Ok(_) => {
                log::info!("Set platform perfomance profile to {profile}");
                Ok(())
            }
            Err(e) => Err(TDPError::IOError(format!(
                "Failed to set power profile: {e:?}"
            ))),
        }
    }
}
