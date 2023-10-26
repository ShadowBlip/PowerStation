use std::{fs::{self, OpenOptions}, io::Write};
use zbus::fdo;
use zbus_macros::dbus_interface;

// Instance of a single CPU core
pub struct CPUCore {
    // CPU core number
    pub number: u32,
    // sysfs path to the CPU core
    // E.g. /sys/bus/cpu/devices/cpu{}
    pub path: String, 
}

#[dbus_interface(name = "org.shadowblip.CPU.Core")]
impl CPUCore {
    // Returns the core number of the CPU core
    #[dbus_interface(property)]
    pub fn number(&self) -> u32 {
        self.number
    }

    // Returns the core ID of the CPU core. This ID will be identical for 
    // hyperthread cores.
    #[dbus_interface(property)]
    async fn core_id(&self) -> fdo::Result<u32> {
        let path = format!("{0}/topology/core_id", self.path);
        let result = fs::read_to_string(path);
        let id = result
            // convert the std::io::Error to a zbus::fdo::Error
            .map_err(|err|fdo::Error::IOError(err.to_string()))?
            .trim()
            .to_lowercase();

        // Convert the ID to an integer
        let id = id.parse::<u32>()
            // convert the ParseIntError to a zbus::fdo::Error
            .map_err(|err|fdo::Error::Failed(err.to_string()))?;

        return Ok(id);
    }

    // Returns true if the given core is online
    #[dbus_interface(property)]
    async fn online(&self) -> fdo::Result<bool> {
        if self.number == 0 {
            return Ok(true);
        }
        let path = format!("{0}/online", self.path);
        let result = fs::read_to_string(path);
        let status = result
            // convert the std::io::Error to a zbus::fdo::Error
            .map_err(|err|fdo::Error::IOError(err.to_string()))?
            .trim()
            .to_lowercase();

        return Ok(status == "1" || status == "on");
    }

    // Sets the given core to online
    #[dbus_interface(property)]
    async fn set_online(&mut self, enabled: bool) -> fdo::Result<()> { 
        let status = if enabled { "1" } else {"0"};

        // Open the sysfs file to write to
        let path = format!("{0}/online", self.path);
        let file = OpenOptions::new().write(true).open(path);

        // Write the value
        file
            // convert the std::io::Error to a zbus::fdo::Error
            .map_err(|err|fdo::Error::Failed(err.to_string()))?
            .write_all(status.as_bytes())
            // convert the std::io::Error to a zbus::fdo::Error
            .map_err(|err|fdo::Error::IOError(err.to_string()))?;

        Ok(())
    }
}
