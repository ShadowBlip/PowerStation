use zbus::fdo;

/// [DBusInterface] is a TDP DBus implementation
pub trait DBusInterface {
    fn tdp(&self) -> fdo::Result<f64>;
    fn set_tdp(&mut self, value: f64) -> fdo::Result<()>;
}
