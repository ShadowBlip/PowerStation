use std::fs;
use zbus_macros::dbus_interface;

// Instance of the CPU on the host machine
pub struct CPU {
    pub smt_capable: bool,
}

#[dbus_interface(name = "org.shadowblip.CPU")]
impl CPU {
    pub fn is_smt_enabled(&mut self) -> bool {
        return true;
    }
}

impl CPU {
    pub fn new() -> CPU {
        CPU { smt_capable: false }
    }

    // Returns a list of all detected cores
    pub fn get_cores() -> Vec<CPUCore> {
        let mut cores: Vec<CPUCore> = Vec::new();
        let paths = fs::read_dir("/sys/class/cpuid").unwrap();
        let mut i = 0;
        for path in paths {
            println!("Name: {}", path.unwrap().path().display());
            cores.push(CPUCore { number: i });
            i += 1;
        }

        return cores;
    }
}

// Instance of a single CPU core
pub struct CPUCore {
    number: u32,
}

#[dbus_interface(name = "org.shadowblip.CPU.Core")]
impl CPUCore {
    pub fn get_num(&mut self) -> u32 {
        self.number
    }

    /// Returns true if the given core is online
    ///
    /// # Panics
    ///
    /// Panics if there is no online path in sysfs
    pub fn is_online(&mut self) -> bool {
        if self.number == 0 {
            return true;
        }
        let path = format!("/sys/bus/cpu/devices/cpu{0}/online", self.number);
        let contents = fs::read_to_string(path);
        let binding = contents.unwrap();
        let online = binding.trim();

        return online == "1";
    }
}
