mod performance;
use std::{error::Error, future::pending};
use zbus::Connection;

use crate::performance::cpu::cpu;
use crate::performance::gpu::{self, get_gpu};

const BUS_NAME: &str = "org.shadowblip.LightningBus";
const PREFIX: &str = "/org/shadowblip/Performance";

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    println!("Starting LightningBus");

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
    for gpu in gpus {
        let gpu_path = format!("{0}/GPU", PREFIX);
        match gpu {
            gpu::GPU::AMD(t) => connection.object_server().at(gpu_path, t).await?,
            gpu::GPU::Intel(t) => connection.object_server().at(gpu_path, t).await?,
        };
    }

    // Request a name
    connection
        .request_name(BUS_NAME)
        .await?;

    // Do other things or go to wait forever
    pending::<()>().await;

    Ok(())
}
