use std::sync::Arc;
use std::sync::Mutex;
use zbus::fdo;
use zbus::fdo::Error;
use zbus_macros::dbus_interface;

use crate::performance::apu::tdp::TDPDevice;

use super::tdp::TDPResult;

pub struct DBusIface {
    dev: Arc<Mutex<dyn TDPDevice>>
}

#[dbus_interface(name = "org.shadowblip.GPU.Card.TDP")]
impl DBusIface {

    fn get_ppt_fppt(&self) -> fdo::Result<u64> {
        match self.dev.lock() {
            Ok(mut lck) => {
                match lck.get_ppt_fppt() {
                    TDPResult::Ok(ppt_fppt_res) => {
                        Ok(ppt_fppt_res)
                    },
                    TDPResult::Err(err) => {
                        Err(Error::Failed(err.into()))
                    }
                }
            },
            Err(err) => {
                todo!()
            }
        }
    }

    fn set_ppt_fppt(&self) -> fdo::Result<()> {
        match self.dev.lock() {
            Ok(lck) => {
                match lck.set_ppt_fppt() {
                    TDPResult::Ok(()) => {
                        Ok(())
                    },
                    TDPResult::Err(err) => {
                        Err(Error::Failed(err.into()))
                    }
                }
            },
            Err(err) => {
                todo!()
            }
        }
    }

}