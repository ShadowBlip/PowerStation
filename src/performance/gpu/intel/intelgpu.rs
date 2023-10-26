use zbus_macros::dbus_interface;

use crate::performance::gpu::GraphicsCard;

pub struct IntelGPU {}

#[dbus_interface(name = "org.shadowblip.GPU")]
impl GraphicsCard for IntelGPU {
    #[dbus_interface(property)]
    fn name(&self) -> String {
        return "amd".to_string();
    }

    #[dbus_interface(property)]
    fn vendor(&self) -> String {
        todo!()
    }
}
