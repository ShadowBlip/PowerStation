use simple_logger::SimpleLogger;
use std::{error::Error, future::pending};
use zbus::Connection;

use crate::constants::{BUS_NAME, CPU_PATH, GPU_PATH};
use crate::performance::cpu::cpu;
use crate::performance::gpu::dbus;
use crate::dbus::gpu::GPUBus;
use crate::dbus::gpu::get_connectors;
use crate::dbus::gpu::get_gpus;

mod constants;
mod performance;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    SimpleLogger::new().init().unwrap();
    log::info!("Starting PowerStation");

    // Discover all CPUs
    let cpu = cpu::CPU::new();
    let cores = cpu::get_cores();

    // Configure the connection
    let connection = Connection::system().await?;

    // Generate CPU objects to serve
    connection.object_server().at(CPU_PATH, cpu).await?;
    for core in cores {
        let core_path = format!("{0}/Core{1}", CPU_PATH, core.number());
        connection.object_server().at(core_path, core).await?;
    }

    // Discover all GPUs and Generate GPU objects to serve
    let mut gpu_obj_paths: Vec<String> = Vec::new();
    // TODO: There must be a better way to do this
    for mut card in get_gpus() {
        // Build the DBus object path for this card
        let card_name = card.name().as_str().title();
        let gpu_path = format!("{0}/{1}", GPU_PATH, card_name);
        gpu_obj_paths.push(gpu_path.clone());

        // Get the TDP interface from the card and serve it on DBus
        let tdp = card.get_tdp_interface();
        if tdp.is_some() {
            log::debug!("Discovered TDP interface on card: {}", card_name);
            let tdp = tdp.unwrap();
            connection.object_server().at(gpu_path.clone(), tdp).await?;
        }

        // Get GPU connectors from the card and serve them on DBus
        let mut connector_paths: Vec<String> = Vec::new();
        let connectors = get_connectors(card.name());
        for connector in connectors {
            let name = connector.name.clone().replace('-', "/");
            let port_path = format!("{0}/{1}", gpu_path, name);
            connector_paths.push(port_path.clone());
            log::debug!("Discovered connector on {}: {}", card_name, port_path);
            connection.object_server().at(port_path, connector).await?;
        }
        card.set_connector_paths(connector_paths);

        // Serve the GPU interface on DBus
        connection
            .object_server()
            .at(gpu_path.clone(), card)
            .await?;
    }

    // Create a GPU Bus instance which allows card enumeration
    let gpu_bus = GPUBus::new(gpu_obj_paths);
    connection.object_server().at(GPU_PATH, gpu_bus).await?;

    // Request a name
    connection.request_name(BUS_NAME).await?;

    // Do other things or go to wait forever
    pending::<()>().await;

    Ok(())
}

trait TitleCase {
    fn title(&self) -> String;
}

impl TitleCase for &str {
    fn title(&self) -> String {
        if !self.is_ascii() || self.is_empty() {
            return String::from(*self);
        }
        let (head, tail) = self.split_at(1);
        head.to_uppercase() + tail
    }
}
