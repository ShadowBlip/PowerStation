use std::{
    fs::{self, OpenOptions},
    io::Write,
};
use tokio::io::AsyncWriteExt;
use zbus::fdo;
use zbus_macros::interface;

// Instance of a single CPU core
#[derive(Debug)]
pub struct CPUCore {
    // CPU core number
    pub number: u32,
    // sysfs path to the CPU core
    // E.g. /sys/bus/cpu/devices/cpu{}
    pub path: String,
}

impl CPUCore {
    pub fn new(number: u32, path: String) -> CPUCore {
        CPUCore { number, path }
    }

    /// Asyncronously set the core to online
    pub async fn set_online_async(&self, enabled: bool) -> Result<(), std::io::Error> {
        let enabled_str = if enabled { "enabled" } else { "disabled" };
        log::info!("Setting core {} to {}", self.number, enabled_str);
        let status = if enabled { "1" } else { "0" };
        if self.number == 0 {
            return Ok(());
        }

        // Open the sysfs file to write to
        let path = format!("{0}/online", self.path);
        let mut options = tokio::fs::OpenOptions::new();
        let file = options.write(true).open(path);

        // Write the value
        file.await?.write_all(status.as_bytes()).await?;

        Ok(())
    }
}

#[interface(name = "org.shadowblip.CPU.Core")]
impl CPUCore {
    // Returns the core number of the CPU core
    #[zbus(property)]
    pub fn number(&self) -> u32 {
        self.number
    }

    // Returns the core ID of the CPU core. This ID will be identical for
    // hyperthread cores.
    #[zbus(property)]
    pub fn core_id(&self) -> fdo::Result<u32> {
        let path = format!("{0}/topology/core_id", self.path);
        let result = fs::read_to_string(path);
        let id = result
            // convert the std::io::Error to a zbus::fdo::Error
            .map_err(|err| fdo::Error::IOError(err.to_string()))?
            .trim()
            .to_lowercase();

        // Convert the ID to an integer
        let id = id
            .parse::<u32>()
            // convert the ParseIntError to a zbus::fdo::Error
            .map_err(|err| fdo::Error::Failed(err.to_string()))?;

        Ok(id)
    }

    // Returns true if the given core is online
    #[zbus(property)]
    pub fn online(&self) -> fdo::Result<bool> {
        if self.number == 0 {
            return Ok(true);
        }
        let path = format!("{0}/online", self.path);
        let result = fs::read_to_string(path);
        let status = result
            // convert the std::io::Error to a zbus::fdo::Error
            .map_err(|err| fdo::Error::IOError(err.to_string()))?
            .trim()
            .to_lowercase();

        Ok(status == "1" || status == "on")
    }

    // Sets the given core to online
    #[zbus(property)]
    pub fn set_online(&mut self, enabled: bool) -> fdo::Result<()> {
        let enabled_str = if enabled { "enabled" } else { "disabled" };
        log::info!("Setting core {} to {}", self.number, enabled_str);
        let status = if enabled { "1" } else { "0" };
        if self.number == 0 {
            return Ok(());
        }

        // Open the sysfs file to write to
        let path = format!("{0}/online", self.path);
        let file = OpenOptions::new().write(true).open(path);

        // Write the value
        file
            // convert the std::io::Error to a zbus::fdo::Error
            .map_err(|err| fdo::Error::Failed(err.to_string()))?
            .write_all(status.as_bytes())
            // convert the std::io::Error to a zbus::fdo::Error
            .map_err(|err| fdo::Error::IOError(err.to_string()))?;

        Ok(())
    }
}
