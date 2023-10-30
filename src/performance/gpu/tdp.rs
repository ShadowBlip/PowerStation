use zbus::fdo;

/// [DBusInterface] is a TDP DBus implementation for "org.shadowblip.GPU.TDP"
pub trait DBusInterface {
    fn tdp(&self) -> fdo::Result<f64>;
    fn set_tdp(&mut self, value: f64) -> fdo::Result<()>;
    fn boost(&self) -> fdo::Result<f64>;
    fn set_boost(&mut self, value: f64) -> fdo::Result<()>;
    fn thermal_profile(&self) -> fdo::Result<u32>;
    fn set_thermal_profile(&mut self, profile: u32) -> fdo::Result<()>;
    fn thermal_throttle_limit_c(&self) -> fdo::Result<f64>;
    fn set_thermal_throttle_limit_c(&mut self, limit: f64) -> fdo::Result<()>;
}
