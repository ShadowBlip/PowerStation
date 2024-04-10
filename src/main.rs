use simple_logger::SimpleLogger;
use std::{error::Error, future::pending};
use zbus::Connection;

use crate::constants::{BUS_NAME, CPU_PATH, GPU_PATH};
use crate::dbus::gpu::get_connectors;
use crate::dbus::gpu::get_gpus;
use crate::dbus::gpu::GPUBus;
use crate::performance::cpu::cpu;
use crate::performance::gpu::dbus;

mod constants;
mod performance;
mod platform;

#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() -> Result<(), Box<dyn Error>> {
    SimpleLogger::new().init().unwrap();
    const VERSION: &str = env!("CARGO_PKG_VERSION");
    log::info!("Starting PowerStation v{}", VERSION);

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
    for mut card in get_gpus().await {
        // Build the DBus object path for this card
        let gpu_name = card.name().await;
        let card_name = gpu_name.as_str().title();
        let gpu_path = card.gpu_path().await;
        gpu_obj_paths.push(gpu_path.clone());

        // Get the TDP interface from the card and serve it on DBus
        match card.get_tdp_interface().await {
            Some(tdp) => {
                log::debug!("Discovered TDP interface on card: {}", card_name);
                connection.object_server().at(gpu_path.clone(), tdp).await?;
            },
            None => {
                log::warn!("Card {} does not have a TDP interface", card_name);
            }
        }

        // Get GPU connectors from the card and serve them on DBus
        let mut connector_paths: Vec<String> = Vec::new();
        let connectors = get_connectors(gpu_name);
        for connector in connectors {
            let name = connector.name.clone().replace('-', "/");
            let port_path = format!("{0}/{1}", gpu_path, name);
            connector_paths.push(port_path.clone());
            log::debug!("Discovered connector on {}: {}", card_name, port_path);
            connection.object_server().at(port_path, connector).await?;
        }
        card.set_connector_paths(connector_paths).await;

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
