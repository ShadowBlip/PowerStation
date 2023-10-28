use std::sync::Mutex;

use libryzenadj::RyzenAdj;
use zbus::fdo;
use zbus_macros::dbus_interface;

use crate::performance::gpu::tdp::DBusInterface;

/// Implementation of TDP control for AMD GPUs
pub struct TDP {
    ryzenadj: Option<Mutex<RyzenAdj>>,
    pub path: String,
}

unsafe impl Sync for TDP {} // implementor (RyzenAdj) may be unsafe
unsafe impl Send for TDP {} // implementor (RyzenAdj) may be unsafe

impl TDP {
    /// Create a new TDP instance
    pub fn new(path: String) -> TDP {
        let ryzenadj: Option<Mutex<RyzenAdj>> = match RyzenAdj::new() {
            Ok(x) => Some(Mutex::new(x)),
            Err(e) => {
                log::error!("RyzenAdj failed to init: {}", e);
                None
            }
        };
        return TDP { path, ryzenadj };
    }

    /// Returns the current TDP value using ryzenadj
    fn get_stapm_limit(&self) -> Result<f32, String> {
        log::debug!("Getting stapm limit");
        let mutex = match &self.ryzenadj {
            Some(x) => x,
            None => {
                log::error!("RyzenAdj unavailable");
                return Err(String::from("RyzenAdj unavailable"));
            }
        };

        let lock = match mutex.lock() {
            Ok(x) => x,
            Err(e) => {
                log::error!("RyzenAdj lock acquired failed: {}", e);
                return Err(String::from(format!("RyzenAdj lock acquire failed: {}", e)));
            }
        };

        // TODO: Don't unwrap
        let limit = lock.get_stapm_limit().unwrap();

        return Ok(limit);
    }
}

#[dbus_interface(name = "org.shadowblip.GPU.TDP")]
impl DBusInterface for TDP {
    #[dbus_interface(property, name = "TDP")]
    fn tdp(&self) -> fdo::Result<f64> {
        let limit =
            TDP::get_stapm_limit(&self).map_err(|err| fdo::Error::Failed(err.to_string()))?;
        return Ok(limit.into());
    }

    #[dbus_interface(property, name = "TDP")]
    fn set_tdp(&mut self, value: f64) -> fdo::Result<()> {
        return Ok(());
    }
}
