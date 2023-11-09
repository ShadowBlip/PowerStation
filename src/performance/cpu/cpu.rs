use std::collections::HashMap;
use std::{fs::OpenOptions, io::Write};
use tokio::fs;
use zbus::fdo;
use zbus::zvariant::ObjectPath;
use zbus_macros::dbus_interface;

use crate::constants::CPU_PATH;
use crate::performance::cpu::core::CPUCore;

// Path to discover the number of CPUs the system has
const CPUID_PATH: &str = "/sys/bus/cpu/devices";
const SMT_PATH: &str = "/sys/devices/system/cpu/smt/control";
const BOOST_PATH: &str = "/sys/devices/system/cpu/cpufreq/boost";

// Instance of the CPU on the host machine
pub struct CPU {
    core_map: HashMap<u32, Vec<CPUCore>>,
    core_count: u32,
}

impl CPU {
    // Returns a new CPU instance
    pub fn new() -> CPU {
        // Create a hashmap to organize the cores by their core ID
        let mut core_map: HashMap<u32, Vec<CPUCore>> = HashMap::new();
        let mut cores = get_cores();

        // Ensure all cores are online
        for core in cores.iter_mut() {
            let _ = core.set_online(true);
        }

        // Organize cores by core id
        let mut core_count = 0;
        for core in cores {
            core_count += 1;
            let core_id = core.core_id().unwrap();
            if core_map.get(&core_id).is_none() {
                let list: Vec<CPUCore> = Vec::new();
                core_map.insert(core_id, list);
            }

            let list = core_map.get_mut(&core_id).unwrap();
            list.push(core);
        }

        CPU {
            core_map,
            core_count,
        }
    }
}

#[dbus_interface(name = "org.shadowblip.CPU")]
impl CPU {
    // Returns whether or not boost is enabled
    #[dbus_interface(property)]
    pub async fn boost_enabled(&self) -> fdo::Result<bool> {
        if !has_feature("cpb".to_string()).await? {
            return Ok(false);
        }
        let result = fs::read_to_string(BOOST_PATH);
        let status = result
            .await
            // convert the std::io::Error to a zbus::fdo::Error
            .map_err(|err| fdo::Error::IOError(err.to_string()))?
            .trim()
            .to_lowercase();

        Ok(status == "1" || status == "on")
    }

    // Set whether or not boost is enabled
    #[dbus_interface(property)]
    pub async fn set_boost_enabled(&mut self, enabled: bool) -> fdo::Result<()> {
        log::info!("Setting boost enabled to {}", enabled);
        let status = if enabled { "1" } else { "0" };

        // Open the sysfs file to write to
        let file = OpenOptions::new().write(true).open(BOOST_PATH);

        // Write the value
        file
            // convert the std::io::Error to a zbus::fdo::Error
            .map_err(|err| fdo::Error::Failed(err.to_string()))?
            .write_all(status.as_bytes())
            // convert the std::io::Error to a zbus::fdo::Error
            .map_err(|err| fdo::Error::IOError(err.to_string()))?;

        Ok(())
    }

    // Returns whether or not SMT is currently enabled
    #[dbus_interface(property)]
    pub async fn smt_enabled(&self) -> fdo::Result<bool> {
        if !has_feature("ht".to_string()).await? {
            return Ok(false);
        }
        let result = fs::read_to_string(SMT_PATH);
        let status = result
            .await
            // convert the std::io::Error to a zbus::fdo::Error
            .map_err(|err| fdo::Error::IOError(err.to_string()))?
            .trim()
            .to_lowercase();

        Ok(status == "1" || status == "on")
    }

    // Set whether or not SMT is enabled
    #[dbus_interface(property)]
    pub async fn set_smt_enabled(&mut self, enabled: bool) -> fdo::Result<()> {
        log::info!("Setting smt enabled to {}", enabled);
        let status = if enabled { "on" } else { "off" };

        // Open the sysfs file to write to
        let file = OpenOptions::new().write(true).open(SMT_PATH);

        // Write the value
        file
            // convert the std::io::Error to a zbus::fdo::Error
            .map_err(|err| fdo::Error::Failed(err.to_string()))?
            .write_all(status.as_bytes())
            // convert the std::io::Error to a zbus::fdo::Error
            .map_err(|err| fdo::Error::IOError(err.to_string()))?;

        Ok(())
    }

    // Returns a list of features that the CPU supports
    #[dbus_interface(property)]
    pub async fn features(&self) -> fdo::Result<Vec<String>> {
        get_features().await
    }

    /// Returns the total number of CPU cores detected
    #[dbus_interface(property)]
    pub async fn cores_count(&self) -> fdo::Result<u32> {
        Ok(self.core_count.clone())
    }

    #[dbus_interface(property)]
    pub async fn cores_enabled(&self) -> fdo::Result<u32> {
        let mut count = 0;
        for core_list in self.core_map.values() {
            for core in core_list {
                let is_online = core.online()?;
                if is_online {
                    count += 1;
                }
            }
        }
        Ok(count)
    }

    #[dbus_interface(property)]
    pub async fn set_cores_enabled(&mut self, num: u32) -> fdo::Result<()> {
        log::info!("Setting core count to {}", num);
        if num < 1 {
            return Err(fdo::Error::InvalidArgs(String::from(
                "Cowardly refusing to set core count to 0",
            )));
        }

        let core_count = self.core_count;
        if num > core_count {
            log::warn!(
                "Unable to set enabled cores to {}. Maximum core count is {}. Enabling all cores.",
                num,
                core_count
            );
        }
        let smt_enabled = self.smt_enabled().await?;

        // If SMT is not enabled and the given core number is greater than what
        // cores would be available, then just set it to half the core count
        let num = if !smt_enabled && num > (core_count / 2) {
            log::warn!(
                "Unable to set enabled cores to {} while SMT is disabled. Enabling all physical cores.",
                num
            );
            core_count / 2
        } else {
            num
        };

        // Collect all core IDs from the core map
        let mut core_ids = self.core_map.keys().cloned().collect::<Vec<u32>>();
        core_ids.sort();

        // Enable/disable cores based on their hyper-threaded sibling
        let mut enabled_count = 1;
        for core_id in core_ids {
            let core_list = self.core_map.get_mut(&core_id).unwrap();
            for core in core_list.iter_mut() {
                if core.number == 0 {
                    continue;
                }
                let should_enable = enabled_count < num;
                if should_enable {
                    enabled_count += 1;
                }
                core.set_online_async(should_enable)
                    .await
                    .map_err(|err| fdo::Error::IOError(err.to_string()))?;
            }
        }

        Ok(())
    }

    /// Returns a list of DBus paths to all CPU cores
    pub async fn enumerate_cores(&mut self) -> fdo::Result<Vec<ObjectPath>> {
        let mut paths: Vec<ObjectPath> = Vec::new();

        for i in 0..self.core_count {
            let path = format!("{}/Core{1}", CPU_PATH, i);
            let path = ObjectPath::from_string_unchecked(path);
            paths.push(path);
        }

        Ok(paths)
    }

    /// Returns true if the CPU has the given feature flag.
    pub async fn has_feature(&mut self, flag: String) -> fdo::Result<bool> {
        has_feature(flag).await
    }
}

// Returns true if the CPU has the given feature flag.
async fn has_feature(flag: String) -> fdo::Result<bool> {
    let features = get_features();
    Ok(features.await?.contains(&flag))
}

// Returns a list of features that the CPU supports
async fn get_features() -> fdo::Result<Vec<String>> {
    let mut features: Vec<String> = Vec::new();

    // Read the data from cpuinfo
    let path = "/proc/cpuinfo";
    let result = fs::read_to_string(path);
    let content = result
        .await
        // convert the std::io::Error to a zbus::fdo::Error
        .map_err(|err| fdo::Error::IOError(err.to_string()))?;

    // Parse the contents to find the flags
    for line in content.split('\n') {
        if !line.starts_with("flags") {
            continue;
        }
        // Split the 'flags' line to get the actual CPU flags
        let parts = line.split(':');
        for part in parts {
            // Only parse the right side of the ":"
            if part.starts_with("flags") {
                continue;
            }
            let flags = part.trim().split(' ');
            for flag in flags {
                features.push(flag.to_string());
            }
        }
        break;
    }

    Ok(features)
}

// Returns a list of all detected cores
pub fn get_cores() -> Vec<CPUCore> {
    let mut cores: Vec<CPUCore> = Vec::new();
    let paths = std::fs::read_dir(CPUID_PATH).unwrap();
    let mut i = 0;
    for path in paths {
        log::info!("Discovered core: {}", path.unwrap().path().display());
        let core_path = format!("/sys/bus/cpu/devices/cpu{0}", i);
        let core = CPUCore::new(i, core_path);
        cores.push(core);
        i += 1;
    }

    cores
}
