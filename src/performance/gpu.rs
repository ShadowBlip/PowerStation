use std::fs::{self, File};
use std::io::{prelude::*, BufReader};
use zbus::fdo;
use zbus::zvariant::ObjectPath;
use zbus_macros::dbus_interface;

use crate::performance::gpu::amd::amdgpu::AMDGPU;
use crate::performance::gpu::connector::Connector;
use crate::performance::gpu::intel::intelgpu::IntelGPU;

pub mod connector;
pub mod tdp;

pub mod amd;
pub mod intel;

const DRM_PATH: &str = "/sys/class/drm";
const PCI_IDS_PATH: &str = "/usr/share/hwdata/pci.ids";

/// Container for different types of supported GPUs
/// https://stackoverflow.com/questions/53216593/vec-of-generics-of-different-concrete-types
pub enum GPU {
    AMD(AMDGPU),
    Intel(IntelGPU),
}

/// Represents the data contained in /sys/class/drm/cardX
pub trait DBusInterface {
    fn name(&self) -> String;
    fn path(&self) -> String;
    fn class(&self) -> String;
    fn class_id(&self) -> String;
    fn vendor(&self) -> String;
    fn vendor_id(&self) -> String;
    fn device(&self) -> String;
    fn device_id(&self) -> String;
    fn subdevice(&self) -> String;
    fn subdevice_id(&self) -> String;
    fn subvendor_id(&self) -> String;
    fn revision_id(&self) -> String;
    fn manual_clock(&self) -> fdo::Result<bool>;
    fn set_manual_clock(&mut self, enabled: bool) -> fdo::Result<()>;
    fn clock_limit_mhz_min(&self) -> fdo::Result<f64>;
    fn clock_limit_mhz_max(&self) -> fdo::Result<f64>;
    fn clock_value_mhz_min(&self) -> fdo::Result<f64>;
    fn set_clock_value_mhz_min(&mut self, value: f64) -> fdo::Result<()>;
    fn clock_value_mhz_max(&self) -> fdo::Result<f64>;
    fn set_clock_value_mhz_max(&mut self, value: f64) -> fdo::Result<()>;
    fn enumerate_connectors(&self) -> fdo::Result<Vec<ObjectPath>>;
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
    pub fn enumerate_cards(&self) -> fdo::Result<Vec<ObjectPath>> {
        let mut paths: Vec<ObjectPath> = Vec::new();

        for item in &self.gpu_object_paths {
            let path = ObjectPath::from_string_unchecked(item.clone());
            paths.push(path);
        }

        return Ok(paths);
    }
}

/// Returns a list of all detected gpu devices
pub fn get_gpus() -> Vec<GPU> {
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
        let gpu = get_gpu(file_path);
        if gpu.is_err() {
            continue;
        }

        gpus.push(gpu.unwrap());
    }

    return gpus;
}

/// Returns the GPU instance for the given path in /sys/class/drm
pub fn get_gpu(path: String) -> Result<GPU, std::io::Error> {
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
    let hw_ids_file = File::open(PCI_IDS_PATH)?;
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
            device = Some(
                line_clean
                    .clone()
                    .trim_start_matches(&device_id)
                    .trim()
                    .to_string(),
            );
            log::debug!("Found device name: {}", device.clone().unwrap());
        }

        if device.is_some() && !line.starts_with("\t\t") {
            log::debug!("Got to end of device list. Subdevice not found");
            break;
        }

        let prefix = format!("{0} {1}", subvendor_id, subdevice_id);
        if line_clean.starts_with(&prefix) {
            subdevice = Some(
                line_clean
                    .clone()
                    .trim_start_matches(&prefix)
                    .trim()
                    .to_string(),
            );
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
        | "Advanced Micro Devices, Inc. [AMD/ATI]" => {
            let gpu = AMDGPU {
                connector_paths: Vec::new(),
                name: filename.to_string(),
                path: path.clone(),
                class: class.to_string(),
                class_id,
                vendor: "AMD".to_string(),
                vendor_id,
                device: device.unwrap_or("".to_string()),
                device_id,
                device_type: "".to_string(),
                subdevice: subdevice.unwrap_or("".to_string()),
                subdevice_id,
                subvendor_id,
                revision_id,
            };
            return Ok(GPU::AMD(gpu));
        }
        // Intel Implementation
        "Intel" | "GenuineIntel" | "Intel Corporation" => {
            let gpu = IntelGPU {
                connector_paths: Vec::new(),
                name: filename.to_string(),
                path: path.clone(),
                class: class.to_string(),
                class_id,
                vendor: "Intel".to_string(),
                vendor_id,
                device: device.unwrap_or("".to_string()),
                device_id,
                device_type: "".to_string(),
                subdevice: subdevice.unwrap_or("".to_string()),
                subdevice_id,
                subvendor_id,
                revision_id,
                manual_clock: true,
            };
            return Ok(GPU::Intel(gpu));
        }
        _ => {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                "Unsupported vendor",
            ))
        }
    }
}

/// Returns a [Connector] instance that represents the given path in /sys/class/drm
pub fn get_connector(gpu_name: String, path: String) -> Connector {
    let prefix = format!("{}-", &gpu_name);
    let name = path.trim_start_matches(&prefix);

    return Connector {
        name: name.to_string(),
        path: format!("{0}/{1}", DRM_PATH, path),
    };
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
    return connectors;
}
