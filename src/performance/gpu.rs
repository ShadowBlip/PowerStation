use std::fs::{self, File};
use std::io::{prelude::*, BufReader};

use crate::performance::gpu::amd::amdgpu::AMDGPU;
use crate::performance::gpu::intel::intelgpu::IntelGPU;

pub mod amd;
pub mod intel;

const DRM_PATH: &str = "/sys/class/drm";
const PCI_IDS_PATH: &str = "/usr/share/hwdata/pci.ids";

// Defines a GPU of any type
pub trait GraphicsCard {
    fn name(&self) -> String;
    fn vendor(&self) -> String;
}

// Container for different types of supported GPUs
// https://stackoverflow.com/questions/53216593/vec-of-generics-of-different-concrete-types
pub enum GPU {
    AMD(AMDGPU),
    Intel(IntelGPU),
}

// Returns a list of all detected gpu devices
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

        println!("Discovered gpu: {}", file_path);
        let gpu = get_gpu(file_path);
        if gpu.is_err() {
            continue;
        }

        gpus.push(gpu.unwrap());
    }

    return gpus;
}

// Returns the GPU instance for the given path in /sys/class/drm
pub fn get_gpu(path: String) -> Result<GPU, std::io::Error> {
    let file_prefix = format!("{0}/{1}", path, "device");
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

    // Lookup the card details by parsing the lines of the file
    let mut vendor_found = false;
    let mut device_found = false;
    let mut vendor: &str;
    let mut device: &str;
    let mut subdevice: &str;
    println!(
        "Getting device info from: {} {} {} {}",
        vendor_id, device_id, revision_id, subvendor_id
    );
    for line in reader.lines() {
        let line = line?;
        let line_clean = line.trim();

        if line.starts_with("\t") && !vendor_found {
            continue;
        }
        if line.starts_with(&vendor_id) {
            vendor_found = true;
            vendor = line.trim_start_matches(&vendor_id).trim();
            println!("Found vendor: {}", vendor);
            continue;
        }
        if vendor_found && !line.starts_with("\t") {
            if line.starts_with("#") {
                continue;
            }
            println!("Got to end of vendor list. Device not found.");
            break;
        }

        if line.starts_with("\t\t") && !device_found {
            continue;
        }

        if line_clean.starts_with(&device_id) {
            device_found = true;
            device = line_clean.trim_start_matches(&device_id).trim();
            println!("Found device name: {}", device);
        }

        if device_found && !line.starts_with("\t\t") {
            println!("Got to end of device list. Subdevice not found");
            break;
        }

        let prefix = format!("{0} {1}", subvendor_id, subdevice_id);
        if line_clean.starts_with(&prefix) {
            subdevice = line_clean.trim_start_matches(&prefix);
            println!("Found subdevice name: {}", subdevice);
            break;
        }
    }

    let gpu = AMDGPU {};
    return Ok(GPU::AMD(gpu));
}
