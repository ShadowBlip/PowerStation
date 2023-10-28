use zbus::fdo;

use super::amd;

/// [TDP] contains all possible TDP imeplementations
pub enum TDP {
    AMD(amd::tdp::TDP),
}

/// [DBusInterface] is a TDP DBus implementation
pub trait DBusInterface {
    fn tdp(&self) -> fdo::Result<f64>;
    fn set_tdp(&mut self, value: f64) -> fdo::Result<()>;
}

/// Returns a [TDP] implementation for the given GPU and vendor string
pub fn get_interface(gpu_path: String, vendor: String) -> Result<TDP, String> {
    match vendor.as_str() {
        "AMD" => Ok(TDP::AMD(amd::tdp::TDP::new(gpu_path))),
        _ => Err(String::from("No TDP control found for vendor")),
    }
}
