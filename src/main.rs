use constants::PREFIX;
use sd_notify::NotifyState;
use simple_logger::SimpleLogger;
use std::{error::Error, time::Duration};
use zbus::fdo::ObjectManager;
use zbus::Connection;

use crate::constants::{BUS_NAME, CPU_PATH, GPU_PATH};
use crate::dbus::gpu::{get_connectors, get_gpus, GPUBus};
use crate::performance::{cpu::cpu_features, fan::FanManager, gpu::dbus};

mod constants;
mod performance;

#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() -> Result<(), Box<dyn Error>> {
    SimpleLogger::new().init().unwrap();
    const VERSION: &str = env!("CARGO_PKG_VERSION");
    log::info!("Starting PowerStation v{}", VERSION);

    if std::env::args().any(|argument| argument == "--restore-fans-auto") {
        FanManager::restore_default()?;
        log::info!("Verified all configured fans in firmware automatic mode");
        return Ok(());
    }

    // Discover all CPUs
    let cpu = cpu_features::Cpu::new();
    let cores = cpu_features::get_cores();

    // Configure the connection
    let connection = Connection::system().await?;

    // Create an ObjectManager to signal when objects are added/removed
    let object_manager = ObjectManager {};
    let object_manager_path = String::from(PREFIX);
    connection
        .object_server()
        .at(object_manager_path, object_manager)
        .await?;

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
            }
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

    // Discover configured fans only after an exact DMI match. Ambiguous or
    // unselected configuration never permits a write; after selection, an
    // initialization failure may only restore firmware automatic mode.
    let fan_manager = match FanManager::discover_default() {
        Ok(manager) => manager,
        Err(error) => {
            log::error!("Fan control is unavailable: {error}");
            FanManager::empty()
        }
    };
    // Establish suspend monitoring and its delay inhibitor while every fan is
    // still in firmware automatic. Saved custom control is restored only after
    // that protection is ready.
    let sleep_task = if fan_manager.is_empty() {
        None
    } else {
        Some(fan_manager.spawn_sleep_monitor(connection.clone()).await?)
    };
    fan_manager.register(&connection).await?;

    // Request a name
    connection.request_name(BUS_NAME).await?;

    if let Err(error) = fan_manager.start().await {
        log::error!("Fan control started in automatic fallback: {error}");
        if !fan_manager.automatic_fallback_verified().await {
            return Err(error.into());
        }
    }

    let fan_tasks = fan_manager.spawn_monitor(connection.clone());

    // systemd uses this heartbeat to recover from a hung controller. Its
    // ExecStopPost helper performs the out-of-process automatic-mode restore.
    let _ = sd_notify::notify(false, &[NotifyState::Ready]);
    let watchdog_manager = fan_manager.clone();
    let watchdog = tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(2));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            if watchdog_manager.watchdog_healthy(Duration::from_secs(3)) {
                let _ = sd_notify::notify(false, &[NotifyState::Watchdog]);
            } else {
                log::error!("Fan controller heartbeat is stale; withholding systemd watchdog");
            }
        }
    });

    shutdown_signal().await;
    let _ = sd_notify::notify(false, &[NotifyState::Stopping]);
    watchdog.abort();
    for task in fan_tasks {
        task.abort();
    }
    let shutdown_result = fan_manager.shutdown().await;
    if let Some(task) = sleep_task {
        task.abort();
    }
    shutdown_result?;

    Ok(())
}

async fn shutdown_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};
        let mut terminate = signal(SignalKind::terminate()).expect("install SIGTERM handler");
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {},
            _ = terminate.recv() => {},
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
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
