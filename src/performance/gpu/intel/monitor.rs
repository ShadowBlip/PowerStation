use std::{
    collections::{HashMap, HashSet, VecDeque},
    path::PathBuf,
    time::{Duration, Instant},
};

use tokio::{
    fs,
    sync::mpsc::{channel, Sender},
    time::interval,
};

/// How often GPU metrics should be collected from running processes
const SAMPLE_INTERVAL: Duration = Duration::from_millis(1000);

/// Identifier of a DRM client
type DrmClientId = u16;

/// Commands that can be sent to a running monitor
#[derive(Debug)]
pub enum MonitorCommand {
    GetBusyPercentage {
        response: Sender<f64>,
    },
    #[allow(dead_code)]
    Stop,
}

#[derive(Default)]
pub enum MonitorStrategy {
    #[default]
    DrmClientUsage,
    #[allow(dead_code)]
    GtIdle,
}

/// IntelMonitorGPU can be used to monitor the GPU metrics
pub struct IntelMonitorGPU {
    card_path: PathBuf,
    strategy: MonitorStrategy,
    last_get_metrics: Option<Instant>,
    metrics: HashMap<DrmClientId, MetricSamples>,
    idle_ts: VecDeque<Instant>,
    idle_metrics: VecDeque<u64>,
    should_collect_metrics: bool,
}

impl IntelMonitorGPU {
    /// Creates a new instance of [IntelMonitorGPU]
    pub fn new(path: PathBuf, strategy: MonitorStrategy) -> Self {
        Self {
            card_path: path,
            strategy,
            last_get_metrics: None,
            metrics: Default::default(),
            idle_ts: Default::default(),
            idle_metrics: Default::default(),
            should_collect_metrics: Default::default(),
        }
    }

    /// Run the [IntelMonitorGPU] in a task. Returns a transmitter channel that
    /// can be used to interact with the running monitor. Stops if the transmitter
    /// channel is dropped.
    pub fn run(mut self) -> Sender<MonitorCommand> {
        let (tx, mut rx) = channel(48);
        tokio::task::spawn(async move {
            log::debug!("Intel GPU Monitor started");
            let mut sample_interval = interval(SAMPLE_INTERVAL);
            loop {
                tokio::select! {
                    cmd = rx.recv() => {
                        let Some(cmd) = cmd else {
                            break;
                        };
                        let should_process = self.process(cmd).await;
                        if !should_process {
                            break;
                        }
                    }
                    _ = sample_interval.tick(), if self.should_collect_metrics => {
                        if let Err(e) = self.collect_metrics().await {
                            log::warn!("Failed to collect metrics: {e}");
                        }
                    }
                }
            }
        });

        tx
    }

    /// Process a single [MonitorCommand] sent over a channel
    async fn process(&mut self, cmd: MonitorCommand) -> bool {
        match cmd {
            MonitorCommand::Stop => false,
            MonitorCommand::GetBusyPercentage { response } => {
                let percent_busy = self.get_busy_percentage();
                if let Err(e) = response.send(percent_busy).await {
                    log::warn!("Failed to send busy percent reply: {e}");
                }
                true
            }
        }
    }

    /// Calculate the total GPU busy time using the given strategy
    fn get_busy_percentage(&mut self) -> f64 {
        if !self.should_collect_metrics {
            log::debug!("Started monitoring GPU usage");
            self.should_collect_metrics = true;
        }
        self.last_get_metrics = Some(Instant::now());
        match self.strategy {
            MonitorStrategy::DrmClientUsage => self.get_busy_percentage_from_drm_client_usage(),
            MonitorStrategy::GtIdle => self.get_busy_percentage_from_gtidle(),
        }
    }

    /// Calculate the GPU busy time by measuring the time spent in gtidle and
    /// comparing it to the actual elapsed time.
    fn get_busy_percentage_from_gtidle(&mut self) -> f64 {
        if self.idle_metrics.len() < 2 {
            return 0.0;
        }
        let time_elapsed = self.idle_ts.delta(0, 1).unwrap();
        let idle_time_ms = self.idle_metrics.delta(0, 1).unwrap();
        let percent_idle = (idle_time_ms as f64 / time_elapsed as f64) * 100.0;
        let busy_percentage = 100.0 - percent_idle;
        busy_percentage.clamp(0.0, 100.0)
    }

    /// Calculate the GPU busy time by measuring the number of GPU engine ticks
    /// each DRM client has used compared to the total number of ticks.
    fn get_busy_percentage_from_drm_client_usage(&mut self) -> f64 {
        let mut busy_percentage = 0.0;
        for (_client_id, metric) in self.metrics.iter() {
            let Some(client_busy) = metric.rcs_busy_percent() else {
                // Needs more samples
                continue;
            };
            busy_percentage += client_busy;
        }

        busy_percentage.clamp(0.0, 100.0)
    }

    /// Collect GPU metrics to determine GPU busy
    async fn collect_metrics(&mut self) -> Result<(), std::io::Error> {
        // Stop collecting metrics if it's been too long
        if let Some(last_queried) = self.last_get_metrics {
            if last_queried.elapsed().as_secs() > 30 {
                log::debug!("Metrics haven't been queried for a while. Sleeping.");
                self.should_collect_metrics = false;
                self.idle_ts.clear();
                self.idle_metrics.clear();
                self.metrics.clear();
                return Ok(());
            }
        }

        // Collect metrics based on the strategy
        match self.strategy {
            MonitorStrategy::DrmClientUsage => self.collect_metrics_drm_clients().await,
            MonitorStrategy::GtIdle => self.collect_metrics_gtidle().await,
        }
    }

    /// Collect GPU metrics from gtidle
    async fn collect_metrics_gtidle(&mut self) -> Result<(), std::io::Error> {
        let idle_path = self
            .card_path
            .join("device/tile0/gt0/gtidle/idle_residency_ms");
        let idle_residency_ms_str = fs::read_to_string(idle_path).await?;
        let idle_residency_ms: u64 = idle_residency_ms_str.trim().parse().unwrap_or_default();
        if idle_residency_ms == 0 {
            return Ok(());
        }
        self.idle_metrics.push_front(idle_residency_ms);
        self.idle_ts.push_front(Instant::now());
        if self.idle_metrics.len() > 2 {
            self.idle_metrics.pop_back();
            self.idle_ts.pop_back();
        }

        Ok(())
    }

    /// Collect GPU client metrics for every currently running process
    async fn collect_metrics_drm_clients(&mut self) -> Result<(), std::io::Error> {
        // Loop through all running procs
        let mut current_clients = HashSet::new();
        let mut proc_dir = fs::read_dir("/proc").await?;
        while let Ok(Some(entry)) = proc_dir.next_entry().await {
            let Ok(file_type) = entry.file_type().await else {
                continue;
            };
            if !file_type.is_dir() {
                continue;
            }

            // Loop through every fd and look for a DRM handle
            let pid_path = entry.path();
            let fd_path = pid_path.join("fd");
            let Ok(mut fd_dir) = fs::read_dir(fd_path).await else {
                continue;
            };
            let mut pid_current_clients = HashSet::new();
            while let Ok(Some(entry)) = fd_dir.next_entry().await {
                let Ok(file_type) = entry.file_type().await else {
                    continue;
                };
                if !file_type.is_symlink() {
                    continue;
                }
                let Ok(target_path) = fs::read_link(entry.path()).await else {
                    continue;
                };
                let Some(target_path_str) = target_path.as_os_str().to_str() else {
                    continue;
                };
                if !target_path_str.starts_with("/dev/dri") {
                    continue;
                }
                let Some(device_name) = target_path.file_name() else {
                    continue;
                };
                let device_name = device_name.to_string_lossy();

                // The device name (e.g. renderD128), should exist as a DRM device
                // for this card.
                let device_path = self.card_path.join(format!("device/drm/{device_name}"));
                if !device_path.exists() {
                    continue;
                }

                // Read the fdinfo for this file descriptor
                let fd_name = entry.file_name().to_string_lossy().to_string();
                let fdinfo_path = pid_path.join("fdinfo").join(fd_name);
                let Ok(fdinfo) = fs::read_to_string(fdinfo_path).await else {
                    continue;
                };
                let lines = fdinfo.split("\n");
                let mut client_id = None;
                let mut cycles_rcs = None;
                let mut cycles_total_rcs = None;
                for line in lines {
                    if !line.starts_with("drm-") {
                        continue;
                    }
                    if line.starts_with("drm-client-id:") {
                        let parts = line.split(":");
                        let Some(last) = parts.last() else {
                            continue;
                        };
                        let value = last.trim();
                        client_id = Some(value.parse().unwrap());
                    }
                    if line.starts_with("drm-cycles-rcs:") {
                        let parts = line.split(":");
                        let Some(last) = parts.last() else {
                            continue;
                        };
                        let value = last.trim();
                        cycles_rcs = Some(value.parse().unwrap());
                    }
                    if line.starts_with("drm-total-cycles-rcs:") {
                        let parts = line.split(":");
                        let Some(last) = parts.last() else {
                            continue;
                        };
                        let value = last.trim();
                        cycles_total_rcs = Some(value.parse().unwrap());
                    }
                    if client_id.is_some() && cycles_rcs.is_some() && cycles_total_rcs.is_some() {
                        break;
                    }
                }
                let (Some(client_id), Some(cycles_rcs), Some(cycles_total_rcs)) =
                    (client_id, cycles_rcs, cycles_total_rcs)
                else {
                    continue;
                };

                // Skip storing metrics if we have already processed the client id
                // for this pid.
                if pid_current_clients.contains(&client_id) {
                    continue;
                }

                current_clients.insert(client_id);
                pid_current_clients.insert(client_id);
                self.metrics
                    .entry(client_id)
                    .and_modify(|entry| entry.push_rcs_sample(cycles_rcs, cycles_total_rcs))
                    .or_insert({
                        let mut samples = MetricSamples::default();
                        samples.push_rcs_sample(cycles_rcs, cycles_total_rcs);
                        samples
                    });
            }
        }

        // Remove any client metrics that no longer exist
        let mut clients_to_remove = vec![];
        for client_id in self.metrics.keys() {
            if !current_clients.contains(client_id) {
                clients_to_remove.push(*client_id);
            }
        }
        for client_id in clients_to_remove {
            self.metrics.remove(&client_id);
        }

        Ok(())
    }
}

#[derive(Default, Debug)]
struct MetricSamples {
    /// Number of RCS engine cycles completed by a single DRM client
    drm_cycles_rcs: VecDeque<u64>,
    /// Total number of cycles completed by the RCS engine
    drm_total_cycles_rcs: VecDeque<u64>,
    /// Number of CCS engine cycles completed by a single DRM client
    #[allow(dead_code)]
    drm_cycles_ccs: VecDeque<u64>,
    /// Total number of cycles completed by the CCS engine
    #[allow(dead_code)]
    drm_total_cycles_ccs: VecDeque<u64>,
}

impl MetricSamples {
    fn push_rcs_sample(&mut self, client_usage: u64, total_usage: u64) {
        self.drm_cycles_rcs.push_front(client_usage);
        if self.drm_cycles_rcs.len() > 2 {
            self.drm_cycles_rcs.pop_back();
        }
        self.drm_total_cycles_rcs.push_front(total_usage);
        if self.drm_total_cycles_rcs.len() > 2 {
            self.drm_total_cycles_rcs.pop_back();
        }
    }

    fn rcs_busy_percent(&self) -> Option<f64> {
        self.drm_total_cycles_rcs.busy_percent(&self.drm_cycles_rcs)
    }

    #[allow(dead_code)]
    fn ccs_busy_percent(&self) -> Option<f64> {
        self.drm_total_cycles_ccs.busy_percent(&self.drm_cycles_ccs)
    }
}

trait VecDequeDelta {
    /// Return the delta between two items in a list
    fn delta(&self, from: usize, to: usize) -> Option<u64>;
    /// Returns the percent busy between two sets of samples. The 'other' set
    /// of samples should contain the smaller values.
    fn busy_percent(&self, client_usage: &VecDeque<u64>) -> Option<f64>;
}

impl VecDequeDelta for VecDeque<u64> {
    fn delta(&self, from: usize, to: usize) -> Option<u64> {
        let (first, to) = (self.get(from)?, self.get(to)?);
        Some(*first - *to)
    }

    fn busy_percent(&self, client_usage: &VecDeque<u64>) -> Option<f64> {
        let total_completed = self.delta(0, 1)?;
        let completed = client_usage.delta(0, 1)?;

        Some((completed as f64 / total_completed as f64) * 100.0)
    }
}

impl VecDequeDelta for VecDeque<Instant> {
    fn delta(&self, from: usize, to: usize) -> Option<u64> {
        let (first, to) = (self.get(from)?, self.get(to)?);
        Some(first.duration_since(*to).as_millis() as u64)
    }

    fn busy_percent(&self, _client_usage: &VecDeque<u64>) -> Option<f64> {
        unimplemented!()
    }
}

pub trait IntelMonitorClient {
    async fn get_busy_percentage(&self) -> f64;
    #[allow(dead_code)]
    async fn stop(&self);
}

impl IntelMonitorClient for Sender<MonitorCommand> {
    async fn get_busy_percentage(&self) -> f64 {
        let (tx, mut rx) = channel(1);
        if let Err(e) = self
            .send(MonitorCommand::GetBusyPercentage { response: tx })
            .await
        {
            log::warn!("Failed to send get_busy_percentage request: {e}");
            return 0.0;
        }

        rx.recv().await.unwrap_or_default()
    }

    async fn stop(&self) {
        if let Err(e) = self.send(MonitorCommand::Stop).await {
            log::warn!("Failed to send stop command to monitor: {e}");
        }
    }
}
