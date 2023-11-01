use zbus::fdo;

/// [DBusInterface] is a TDP DBus implementation for "org.shadowblip.GPU.TDP"
pub trait DBusInterface {
    fn tdp(&self) -> fdo::Result<f64>;
    fn set_tdp(&mut self, value: f64) -> fdo::Result<()>;
    fn boost(&self) -> fdo::Result<f64>;
    fn set_boost(&mut self, value: f64) -> fdo::Result<()>;
    fn thermal_throttle_limit_c(&self) -> fdo::Result<f64>;
    fn set_thermal_throttle_limit_c(&mut self, limit: f64) -> fdo::Result<()>;
    fn power_profile(&self) -> fdo::Result<String>;
    fn set_power_profile(&mut self, profile: String) -> fdo::Result<()>;
}
