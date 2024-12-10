use std::fs::{self, File};
use std::io::{prelude::*, BufReader};
use std::path::PathBuf;
use std::sync::Arc;
use zbus::fdo;
use zbus::zvariant::ObjectPath;
use zbus_macros::dbus_interface;

use tokio::sync::Mutex;

use crate::performance::gpu::amd::amdgpu::AmdGpu;
use crate::performance::gpu::connector::Connector;
use crate::performance::gpu::dbus::devices::GPUDevices;
use crate::performance::gpu::dbus::tdp::GPUTDPDBusIface;
use crate::performance::gpu::intel::intelgpu::IntelGPU;
use crate::performance::gpu::interface::GPUError;

const DRM_PATH: &str = "/sys/class/drm";
const PCI_IDS_PATH: &str = "/usr/share/hwdata/pci.ids";

impl From<GPUError> for fdo::Error {
    fn from(val: GPUError) -> Self {
        match &val {
            GPUError::FailedOperation(err) => fdo::Error::Failed(err.to_string()),
            //Self::FeatureUnsupported => fdo::Error::Failed(String::from("Unsupported feature")),
            GPUError::InvalidArgument(err) => fdo::Error::Failed(err.to_string()),
            GPUError::IOError(err) => fdo::Error::IOError(err.to_string()),
        }
    }
}

/// Represents the DBus for GPUs in the system
#[derive(Clone)]
pub struct GPUDBusInterface {
    connector_paths: Vec<String>,
    gpu_obj: Arc<Mutex<GPUDevices>>,
}

impl GPUDBusInterface {
    pub async fn new(gpu: Arc<Mutex<GPUDevices>>) -> Self {
        Self {
            gpu_obj: gpu,
            connector_paths: vec![],
        }
    }

    pub async fn gpu_path(&self) -> String {
        self.gpu_obj.lock().await.get_gpu_path().await
    }

    pub async fn set_connector_paths(&mut self, connector_paths: Vec<String>) {
        self.connector_paths = connector_paths
    }

    pub async fn get_tdp_interface(&self) -> Option<GPUTDPDBusIface> {
        self.gpu_obj
            .lock()
            .await
            .get_tdp_interface()
            .await
            .map(GPUTDPDBusIface::new)
    }
}

#[dbus_interface(name = "org.shadowblip.GPU.Card")]
impl GPUDBusInterface {
    /// Returns a list of DBus paths to all connectors
    pub fn enumerate_connectors(&self) -> fdo::Result<Vec<ObjectPath>> {
        Ok(self
            .connector_paths
            .iter()
            .map(|path| ObjectPath::from_string_unchecked(path.clone()))
            .collect())
    }

    #[dbus_interface(property)]
    pub async fn name(&self) -> String {
        self.gpu_obj.lock().await.name().await
    }

    #[dbus_interface(property)]
    async fn path(&self) -> String {
        self.gpu_obj.lock().await.path().await
    }

    #[dbus_interface(property)]
    async fn class(&self) -> String {
        self.gpu_obj.lock().await.class().await
    }

    #[dbus_interface(property)]
    async fn class_id(&self) -> String {
        self.gpu_obj.lock().await.class_id().await
    }

    #[dbus_interface(property)]
    async fn vendor(&self) -> String {
        self.gpu_obj.lock().await.vendor().await
    }

    #[dbus_interface(property)]
    async fn vendor_id(&self) -> String {
        self.gpu_obj.lock().await.vendor_id().await
    }

    #[dbus_interface(property)]
    async fn device(&self) -> String {
        self.gpu_obj.lock().await.device().await
    }

    #[dbus_interface(property)]
    async fn device_id(&self) -> String {
        self.gpu_obj.lock().await.device_id().await
    }

    #[dbus_interface(property)]
    async fn subdevice(&self) -> String {
        self.gpu_obj.lock().await.subdevice().await
    }

    #[dbus_interface(property)]
    async fn subdevice_id(&self) -> String {
        self.gpu_obj.lock().await.subdevice_id().await
    }

    #[dbus_interface(property)]
    async fn subvendor_id(&self) -> String {
        self.gpu_obj.lock().await.subvendor_id().await
    }

    #[dbus_interface(property)]
    async fn revision_id(&self) -> String {
        self.gpu_obj.lock().await.revision_id().await
    }

    #[dbus_interface(property)]
    async fn clock_limit_mhz_min(&self) -> fdo::Result<f64> {
        self.gpu_obj
            .lock()
            .await
            .clock_limit_mhz_min()
            .await
            .map_err(|err| err.into())
    }

    #[dbus_interface(property)]
    async fn clock_limit_mhz_max(&self) -> fdo::Result<f64> {
        self.gpu_obj
            .lock()
            .await
            .clock_limit_mhz_max()
            .await
            .map_err(|err| err.into())
    }

    #[dbus_interface(property)]
    async fn clock_value_mhz_min(&self) -> fdo::Result<f64> {
        self.gpu_obj
            .lock()
            .await
            .clock_value_mhz_min()
            .await
            .map_err(|err| err.into())
    }

    #[dbus_interface(property)]
    async fn set_clock_value_mhz_min(&mut self, value: f64) -> fdo::Result<()> {
        self.gpu_obj
            .lock()
            .await
            .set_clock_value_mhz_min(value)
            .await
            .map_err(|err| err.into())
    }

    #[dbus_interface(property)]
    async fn clock_value_mhz_max(&self) -> fdo::Result<f64> {
        self.gpu_obj
            .lock()
            .await
            .clock_value_mhz_max()
            .await
            .map_err(|err| err.into())
    }

    #[dbus_interface(property)]
    async fn set_clock_value_mhz_max(&mut self, value: f64) -> fdo::Result<()> {
        self.gpu_obj
            .lock()
            .await
            .set_clock_value_mhz_max(value)
            .await
            .map_err(|err| err.into())
    }

    #[dbus_interface(property)]
    async fn manual_clock(&self) -> fdo::Result<bool> {
        self.gpu_obj
            .lock()
            .await
            .manual_clock()
            .await
            .map_err(|err| err.into())
    }

    #[dbus_interface(property)]
    async fn set_manual_clock(&mut self, enabled: bool) -> fdo::Result<()> {
        self.gpu_obj
            .lock()
            .await
            .set_manual_clock(enabled)
            .await
            .map_err(|err| err.into())
    }
}

/// Used to enumerate all GPU cards over DBus
pub struct GPUBus {
    gpu_object_paths: Vec<String>,
}

impl GPUBus {
    /// Return a new instance of the GPU Bus
    pub fn new(gpu_paths: Vec<String>) -> GPUBus {
        GPUBus {
            gpu_object_paths: gpu_paths,
        }
    }
}

#[dbus_interface(name = "org.shadowblip.GPU")]
impl GPUBus {
    /// Returns a list of DBus paths to all GPU cards
    pub async fn enumerate_cards(&self) -> fdo::Result<Vec<ObjectPath>> {
        let mut paths: Vec<ObjectPath> = Vec::new();

        for item in &self.gpu_object_paths {
            let path = ObjectPath::from_string_unchecked(item.clone());
            paths.push(path);
        }

        Ok(paths)
    }
}

/// Returns a list of all detected gpu devices
pub async fn get_gpus() -> Vec<GPUDBusInterface> {
    let mut gpus = vec![];
    let paths = fs::read_dir(DRM_PATH).unwrap();
    for path in paths {
        let path = path.unwrap();
        let filename = path.file_name().to_str().unwrap().to_string();
        let file_path = path.path().to_str().unwrap().to_string();

        if !filename.starts_with("card") {
            continue;
        }
        if filename.contains("-") {
            continue;
        }

        log::info!("Discovered gpu: {}", file_path);
        match get_gpu(file_path).await {
            Ok(gpu) => gpus.push(gpu),
            Err(err) => {
                log::error!("Error in get_gpu: {}", err);
                continue;
            }
        }
    }

    gpus
}

/// Returns the GPU instance for the given path in /sys/class/drm
pub async fn get_gpu(path: String) -> Result<GPUDBusInterface, std::io::Error> {
    let filename = path.split("/").last().unwrap();
    let file_prefix = format!("{0}/{1}", path, "device");
    let class_id = fs::read_to_string(format!("{0}/{1}", file_prefix, "class"))?
        .trim()
        .replace("0x", "")
        .to_lowercase();
    let vendor_id = fs::read_to_string(format!("{0}/{1}", file_prefix, "vendor"))?
        .trim()
        .replace("0x", "")
        .to_lowercase();
    let device_id = fs::read_to_string(format!("{0}/{1}", file_prefix, "device"))?
        .trim()
        .replace("0x", "")
        .to_lowercase();
    let revision_id = fs::read_to_string(format!("{0}/{1}", file_prefix, "revision"))?
        .trim()
        .replace("0x", "")
        .to_lowercase();
    let subvendor_id = fs::read_to_string(format!("{0}/{1}", file_prefix, "subsystem_vendor"))?
        .trim()
        .replace("0x", "")
        .to_lowercase();
    let subdevice_id = fs::read_to_string(format!("{0}/{1}", file_prefix, "subsystem_device"))?
        .trim()
        .replace("0x", "")
        .to_lowercase();

    // Open the file that contains hardware ID mappings
    let hw_ids_file = File::open(get_pci_ids_path())?;
    let reader = BufReader::new(hw_ids_file);

    // Set the class based on class ID
    let class = match class_id.as_str() {
        "030000" => "integrated",
        "038000" => "dedicated",
        _ => "unknown",
    };

    // Lookup the card details by parsing the lines of the file
    let mut vendor: Option<String> = None;
    let mut device: Option<String> = None;
    let mut subdevice: Option<String> = None;
    log::debug!(
        "Getting device info from: {} {} {} {}",
        vendor_id,
        device_id,
        revision_id,
        subvendor_id
    );
    for line in reader.lines() {
        let line = line?;
        let line_clean = line.trim();

        if line.starts_with("\t") && vendor.is_none() {
            continue;
        }
        if line.starts_with(&vendor_id) {
            vendor = Some(
                line.clone()
                    .trim_start_matches(&vendor_id)
                    .trim()
                    .to_string(),
            );
            log::debug!("Found vendor: {}", vendor.clone().unwrap());
            continue;
        }
        if vendor.is_some() && !line.starts_with("\t") {
            if line.starts_with("#") {
                continue;
            }
            log::debug!("Got to end of vendor list. Device not found.");
            break;
        }

        if line.starts_with("\t\t") && device.is_none() {
            continue;
        }

        if line_clean.starts_with(&device_id) {
            device = Some(line_clean.trim_start_matches(&device_id).trim().to_string());
            log::debug!("Found device name: {}", device.clone().unwrap());
        }

        if device.is_some() && !line.starts_with("\t\t") {
            log::debug!("Got to end of device list. Subdevice not found");
            break;
        }

        let prefix = format!("{0} {1}", subvendor_id, subdevice_id);
        if line_clean.starts_with(&prefix) {
            subdevice = Some(line_clean.trim_start_matches(&prefix).trim().to_string());
            log::debug!("Found subdevice name: {}", subdevice.clone().unwrap());
            break;
        }
    }

    // Return an error if no vendor was found
    if vendor.is_none() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "No vendor found",
        ));
    }

    // Sanitize the vendor strings so they are standard
    match vendor.unwrap().as_str() {
        // AMD Implementation
        "AMD"
        | "AuthenticAMD"
        | "AuthenticAMD Advanced Micro Devices, Inc."
        | "Advanced Micro Devices, Inc. [AMD/ATI]" => Ok(GPUDBusInterface::new(Arc::new(
            Mutex::new(GPUDevices::AmdGpu(AmdGpu {
                name: filename.to_string(),
                path: path.clone(),
                class: class.to_string(),
                class_id,
                vendor: "AMD".to_string(),
                vendor_id,
                device: device.unwrap_or("".to_string()),
                device_id,
                //device_type: "".to_string(),
                subdevice: subdevice.unwrap_or("".to_string()),
                subdevice_id,
                subvendor_id,
                revision_id,
            })),
        ))
        .await),
        // Intel Implementation
        "Intel" | "GenuineIntel" | "Intel Corporation" => Ok(GPUDBusInterface::new(Arc::new(
            Mutex::new(GPUDevices::IntelGpu(IntelGPU {
                name: filename.to_string(),
                path: path.clone(),
                class: class.to_string(),
                class_id,
                vendor: "Intel".to_string(),
                vendor_id,
                device: device.unwrap_or("".to_string()),
                device_id,
                //device_type: "".to_string(),
                subdevice: subdevice.unwrap_or("".to_string()),
                subdevice_id,
                subvendor_id,
                revision_id,
                manual_clock: true,
            })),
        ))
        .await),
        _ => Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "Unsupported vendor",
        )),
    }
}

/// Returns a [Connector] instance that represents the given path in /sys/class/drm
pub fn get_connector(gpu_name: String, path: String) -> Connector {
    let prefix = format!("{}-", &gpu_name);
    let name = path.trim_start_matches(&prefix);

    Connector {
        name: name.to_string(),
        path: format!("{0}/{1}", DRM_PATH, path),
    }
}

/// Returns a list of [Connector] instances for the given graphics card name.
/// E.g. `"card1"`
pub fn get_connectors(gpu_name: String) -> Vec<Connector> {
    log::debug!("Discovering connectors for GPU: {}", gpu_name);
    let mut connectors: Vec<Connector> = Vec::new();
    let paths = fs::read_dir(DRM_PATH).unwrap();
    for path in paths {
        let path = path.unwrap();
        let filename = path.file_name().to_str().unwrap().to_string();

        // Skip paths that do not contain the gpu name
        if !filename.starts_with(&gpu_name) {
            continue;
        }
        if filename == gpu_name {
            continue;
        }

        let connector = get_connector(gpu_name.clone(), filename);
        connectors.push(connector);
    }

    log::debug!("Finished finding connectors");
    connectors
}

/// Returns the path to the PCI id's file from hwdata
fn get_pci_ids_path() -> PathBuf {
    let Ok(base_dirs) = xdg::BaseDirectories::with_prefix("hwdata") else {
        log::warn!("Unable to determine config base path. Using fallback path.");
        return PathBuf::from(PCI_IDS_PATH);
    };

    // Get the data directories in preference order
    let data_dirs = base_dirs.get_data_dirs();
    for dir in data_dirs {
        if dir.exists() {
            let mut path = dir.into_os_string();
            path.push("/pci.ids");
            return path.into();
        }
    }

    log::warn!("Config base path not found. Using fallback path.");
    PathBuf::from(PCI_IDS_PATH)
}
