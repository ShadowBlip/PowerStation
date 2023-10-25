mod performance;
use std::{error::Error, future::pending};
use zbus::Connection;
use performance::cpu::CPU;

//const PREFIX: &str = "/org/shadowblip";

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    println!("Starting LightningBus");

    // Discover all CPUs
    let cpu = CPU::new();
    let cores = CPU::get_cores();

    // Configure the connection
    let connection = Connection::session().await?;

    // Generate objects to serve
    connection.object_server().at("/org/shadowblip/Performance/CPU", cpu).await?;
    for mut core in cores {
        let path = format!("/org/shadowblip/Performance/CPU/Core{0}", core.get_num());
        connection.object_server().at(path, core).await?;
    }

    // Request a name
    connection
        .request_name("org.shadowblip.LightningBus")
        .await?;

    // Do other things or go to wait forever
    pending::<()>().await;

    Ok(())
}
