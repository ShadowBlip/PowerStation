use simple_logger::SimpleLogger;
use std::{error::Error, future::pending};
use zbus::Connection;

use crate::performance::cpu::cpu;
use crate::performance::gpu;

mod performance;

const BUS_NAME: &str = "org.shadowblip.PowerStation";
const PREFIX: &str = "/org/shadowblip/Performance";

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    SimpleLogger::new().init().unwrap();
    log::info!("Starting PowerStation");

    // Discover all CPUs
    let cpu = cpu::CPU::new();
    let cores = cpu::get_cores();

    // Discover all GPUs
    let gpus = gpu::get_gpus();

    // Configure the connection
    let connection = Connection::system().await?;

    // Generate CPU objects to serve
    let cpu_path = format!("{0}/CPU", PREFIX);
    connection.object_server().at(cpu_path, cpu).await?;
    for core in cores {
        let core_path = format!("{0}/CPU/Core{1}", PREFIX, core.number());
        connection.object_server().at(core_path, core).await?;
    }

    // Generate GPU objects to serve
    // TODO: There must be a better way to do this
    for gpu in gpus {
        match gpu {
            gpu::GPU::AMD(card) => {
                // Build the DBus object path for this card
                let card_name = card.name.clone().as_str().title();
                let gpu_path = format!("{0}/GPU/{1}", PREFIX, card_name);

                // Get the TDP interface from the card and serve it on DBus
                let tdp = card.get_tdp_interface();
                if tdp.is_some() {
                    log::debug!("Discovered TDP interface on card: {}", card_name);
                    let tdp = tdp.unwrap();
                    connection.object_server().at(gpu_path.clone(), tdp).await?;
                }

                // Get GPU connectors from the card and serve them on DBus
                let connectors = gpu::get_connectors(card.name.clone());
                for connector in connectors {
                    let name = connector.name.clone().replace("-", "/");
                    let port_path = format!("{0}/{1}", gpu_path, name);
                    log::debug!("Discovered connector on {}: {}", card_name, port_path);
                    connection.object_server().at(port_path, connector).await?;
                }

                // Serve the GPU interface on DBus
                connection
                    .object_server()
                    .at(gpu_path.clone(), card)
                    .await?;
            }

            gpu::GPU::Intel(card) => {
                // Build the DBus object path for this card
                let card_name = card.name.clone().as_str().title();
                let gpu_path = format!("{0}/GPU/{1}", PREFIX, card_name);

                // Get the TDP interface from the card and serve it on DBus
                let tdp = card.get_tdp_interface();
                if tdp.is_some() {
                    log::debug!("Discovered TDP interface on card: {}", card_name);
                    let tdp = tdp.unwrap();
                    connection.object_server().at(gpu_path.clone(), tdp).await?;
                }

                // Get GPU connectors from the card and serve them on DBus
                let connectors = gpu::get_connectors(card.name.clone());
                for connector in connectors {
                    let name = connector.name.clone().replace("-", "/");
                    let port_path = format!("{0}/{1}", gpu_path, name);
                    connection.object_server().at(port_path, connector).await?;
                }

                // Serve the GPU interface on DBus
                connection
                    .object_server()
                    .at(gpu_path.clone(), card)
                    .await?;
            }
        };
    }

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


