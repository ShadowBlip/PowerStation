use std::{collections::HashMap, fs, io, ops::Add, path::PathBuf, str::FromStr};

use udev::Device;

use crate::performance::gpu::{
    platform::hardware::Hardware,
    tdp::{HardwareAccess, TDPDevice, TDPError, TDPResult},
};

/// Amount to scale the TDP values by. E.g. 15 == 15000000
const TDP_SCALE: f64 = 1000000.0;

/// Hwmon implementation of TDP control
pub struct Hwmon {
    /// Detected hardware TDP limits
    hardware: Option<Hardware>,
    /// Udev device used to set/get sysfs properties
    device: Device,
    /// Mapping of attribute labels to their attribute path. In the hwmon
    /// interface there are typically "*_label" attributes which name a particular
    /// set of attributes that denotes its function. For example, an interface
    /// with the attributes:
    ///   ["power1_cap", "power1_label, power2_cap, power2_label"]
    /// Would have this mapping created:
    ///   {"slowPPT": "power1", "fastPPT": "power2"}
    label_map: HashMap<String, String>,
}

impl Hwmon {
    pub fn new(path: &str) -> Result<Self, io::Error> {
        // Discover the hwmon path for the device
        let mut hwmon_path = None;
        let search_path = PathBuf::from(format!("{path}/device/hwmon"));
        let dir = fs::read_dir(search_path.as_path())?;
        for entry in dir {
            let path = entry?.path();
            if !path.is_dir() {
                continue;
            }
            hwmon_path = Some(search_path.join(path));
        }

        // Ensure a valid hwmon path was found
        let Some(hwmon_path) = hwmon_path else {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "No valid hwmon interface found",
            ));
        };
        log::debug!("Found hwmon interface: {hwmon_path:?}");

        // Use udev to read/write attributes
        let device = Device::from_syspath(hwmon_path.as_path())?;

        // Create a mapping of attribute labels to their corresponding attribute.
        let mut label_map = HashMap::new();
        for attrib in device.attributes() {
            log::debug!(
                "Found device attribute: {:?}: {:?}",
                attrib.name(),
                attrib.value()
            );
            let name = attrib.name().to_string_lossy();
            if !name.ends_with("_label") {
                continue;
            }

            let key = attrib.value().to_string_lossy().to_string();
            let Some(value) = name.strip_suffix("_label").map(String::from) else {
                continue;
            };

            label_map.insert(key, value);
        }

        // Get the hardware limits
        let hardware = Self::get_limits(&device, &label_map);

        let hwmon = Self {
            hardware,
            device,
            label_map,
        };

        Ok(hwmon)
    }

    /// Returns the detected TDP limits
    fn get_limits(device: &Device, label_map: &HashMap<String, String>) -> Option<Hardware> {
        let prefix = label_map.get("fastPPT")?;

        let cap_max = format!("{prefix}_cap_max");
        let max_value = device.attribute_value(cap_max)?.to_str()?;
        let max_value: f64 = max_value.parse().ok()?;
        let cap_min = format!("{prefix}_cap_min");
        let min_value = device.attribute_value(cap_min)?.to_str()?;
        let min_value: f64 = min_value.parse().ok()?;

        let hardware = Hardware {
            min_tdp: (min_value / TDP_SCALE),
            max_tdp: (max_value / TDP_SCALE),
            max_boost: 0.0,
        };

        Some(hardware)
    }

    /// Returns the current slowPPT value
    fn get_slow_ppt_cap<F>(&self) -> Option<F>
    where
        F: FromStr,
    {
        self.get_label_value("slowPPT", "cap")
    }

    /// Set the slowPPT to the given value
    fn set_slow_ppt_cap<S>(&mut self, value: S) -> io::Result<()>
    where
        S: ToString + Add,
    {
        self.set_label_value("slowPPT", "cap", value)
    }

    /// Returns the current fastPPT value
    fn get_fast_ppt_cap<F>(&self) -> Option<F>
    where
        F: FromStr,
    {
        self.get_label_value("fastPPT", "cap")
    }

    /// Set the fastPPT to the given value
    fn set_fast_ppt_cap<S>(&mut self, value: S) -> io::Result<()>
    where
        S: ToString + Add,
    {
        self.set_label_value("fastPPT", "cap", value)
    }

    /// Returns the value of the attribute with the given label name found
    /// in the `*_label` attribute. For example, if `power1_label` is "slowPPT",
    /// and you want to get the value of `power1_cap`, you can use this method
    /// to get the value using the label instead of the attribute name:
    /// E.g. `self.get_label_value::<f64>("slowPPT", "cap")`
    fn get_label_value<F>(&self, label: &str, attribute: &str) -> Option<F>
    where
        F: FromStr,
    {
        let prefix = self.label_map.get(label)?;
        let attribute = format!("{prefix}_{attribute}");
        let value = self.device.attribute_value(attribute)?.to_str()?;
        value.parse().ok()
    }

    /// Similar to [Hwmon::get_label_value], this method can be used to write
    /// a value to the attribute with the given label.
    fn set_label_value<S>(&mut self, label: &str, attribute: &str, value: S) -> io::Result<()>
    where
        S: ToString,
    {
        let prefix = self.label_map.get(label).unwrap();
        let attribute = format!("{prefix}_{attribute}");
        self.device
            .set_attribute_value(attribute, value.to_string().as_str())
    }
}

impl HardwareAccess for Hwmon {
    fn hardware(&self) -> Option<&Hardware> {
        self.hardware.as_ref()
    }
}

impl TDPDevice for Hwmon {
    async fn tdp(&self) -> TDPResult<f64> {
        let Some(value) = self.get_slow_ppt_cap::<f64>() else {
            return Err(TDPError::FeatureUnsupported);
        };

        // 15000000 == 15
        Ok(value / TDP_SCALE)
    }

    async fn set_tdp(&mut self, value: f64) -> TDPResult<()> {
        log::debug!("Setting TDP to: {value}");
        if value < 1.0 {
            log::warn!("Cowardly refusing to set TDP less than 1W");
            return Err(TDPError::InvalidArgument(format!(
                "Cowardly refusing to set TDP less than 1W: provided {value}W",
            )));
        }

        // Get the current boost value before updating. We will
        // use this value to also adjust the Fast PPT Limit.
        let boost = self.boost().await? as u64;
        let slow_ppt = (value * TDP_SCALE) as u64; // 15 == 15000000
        let fast_ppt = (value * TDP_SCALE) as u64 + boost;

        self.set_slow_ppt_cap(slow_ppt)?;
        self.set_fast_ppt_cap(fast_ppt)?;

        Ok(())
    }

    async fn boost(&self) -> TDPResult<f64> {
        let Some(slow_ppt) = self.get_slow_ppt_cap::<f64>() else {
            return Err(TDPError::FeatureUnsupported);
        };
        let Some(fast_ppt) = self.get_fast_ppt_cap::<f64>() else {
            return Err(TDPError::FeatureUnsupported);
        };

        // Boost is the difference between fastPPT and slowPPT
        let boost = (fast_ppt - slow_ppt).max(0.0);

        Ok(boost / TDP_SCALE)
    }

    async fn set_boost(&mut self, value: f64) -> TDPResult<()> {
        log::debug!("Setting boost to: {value}");
        if value < 0.0 {
            log::warn!("Cowardly refusing to set TDP Boost less than 0W");
            return Err(TDPError::InvalidArgument(format!(
                "Cowardly refusing to set TDP Boost less than 0W: {}W provided",
                value
            )));
        }

        let Some(slow_ppt_raw) = self.get_slow_ppt_cap::<f64>() else {
            return Err(TDPError::FeatureUnsupported);
        };
        let slow_ppt_scaled = slow_ppt_raw / TDP_SCALE;
        let fast_ppt_scaled = slow_ppt_scaled + value;
        let fast_ppt_raw = (fast_ppt_scaled * TDP_SCALE) as u64;

        self.set_fast_ppt_cap(fast_ppt_raw)?;

        Ok(())
    }

    async fn thermal_throttle_limit_c(&self) -> TDPResult<f64> {
        Err(TDPError::FeatureUnsupported)
    }

    async fn set_thermal_throttle_limit_c(&mut self, _limit: f64) -> TDPResult<()> {
        Err(TDPError::FeatureUnsupported)
    }

    async fn power_profile(&self) -> TDPResult<String> {
        Err(TDPError::FeatureUnsupported)
    }

    async fn power_profiles_available(&self) -> TDPResult<Vec<String>> {
        Err(TDPError::FeatureUnsupported)
    }

    async fn set_power_profile(&mut self, _profile: String) -> TDPResult<()> {
        Err(TDPError::FeatureUnsupported)
    }
}
