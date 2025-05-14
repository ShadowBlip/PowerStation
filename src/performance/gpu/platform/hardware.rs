use crate::performance::gpu::platform::model_config::{Config, ModelConfig};
use std::collections::HashMap;
use std::fs;
use std::io::ErrorKind;
use std::path::Path;

pub struct Hardware {
    pub min_tdp: f64,
    pub max_tdp: f64,
    pub max_boost: f64,
}

impl Hardware {
    pub const PLATFORM_DIR: &str = "/usr/share/powerstation/platform";
    pub const AMD_APU_DATABASE: &str = "amd_apu_database.toml";
    pub const INTEL_APU_DATABASE: &str = "intel_apu_database.toml";
    pub const DMI_OVERRIDES_APU_DATABASE: &str = "dmi_overrides_apu_database.toml";

    // Enhanced new method that loads and parses configurations
    pub fn new() -> Option<Self> {
        match Self::load_and_apply_configs() {
            Ok(hardware) => Some(hardware),
            Err(e) => {
                log::error!("Failed to load hardware configuration: {}", e);
                Some(Self {
                    min_tdp: 0.0,
                    max_tdp: 0.0,
                    max_boost: 0.0,
                })
            }
        }
    }

    // Internal method to load and apply configurations
    fn load_and_apply_configs() -> Result<Self, Box<dyn std::error::Error>> {
        let platform_dir = Path::new(Self::PLATFORM_DIR);

        // Read each database file
        let amd_db_path = platform_dir.join(Self::AMD_APU_DATABASE);
        let intel_db_path = platform_dir.join(Self::INTEL_APU_DATABASE);
        let dmi_overrides_path = platform_dir.join(Self::DMI_OVERRIDES_APU_DATABASE);

        let amd_config = Self::load_config_file(&amd_db_path)?;
        let intel_config = Self::load_config_file(&intel_db_path)?;
        let dmi_overrides_config = Self::load_config_file(&dmi_overrides_path)?;

        // Merge configurations with priority: DMI overrides > Intel > AMD
        let merged_config =
            Self::merge_configs(vec![amd_config, intel_config, dmi_overrides_config]);

        log::info!(
            "Merged configuration contains {} model configs",
            merged_config.models.len()
        );

        // Create default configuration
        let mut hardware = Self {
            min_tdp: 0.0,
            max_tdp: 0.0,
            max_boost: 0.0,
        };

        // Get current model with two-level matching strategy
        let current_model = Self::get_current_model()?;

        // Find matching model in merged configuration
        for model in &merged_config.models {
            if model.model_name == current_model {
                hardware.min_tdp = model.min_tdp;
                hardware.max_tdp = model.max_tdp;
                hardware.max_boost = model.max_boost;
                log::info!(
                    "Applied configuration for model {}: min_tdp={}, max_tdp={}, max_boost={}",
                    model.model_name,
                    model.min_tdp,
                    model.max_tdp,
                    model.max_boost
                );
                break;
            }
        }

        Ok(hardware)
    }

    // Read and parse a single TOML file
    fn load_config_file(file_path: &Path) -> Result<Config, Box<dyn std::error::Error>> {
        match fs::read_to_string(file_path) {
            Ok(config_str) => {
                let config: Config = toml::from_str(&config_str)?;
                log::info!(
                    "Loaded configuration file: {:?}, containing {} models",
                    file_path,
                    config.models.len()
                );
                Ok(config)
            }
            Err(e) if e.kind() == ErrorKind::NotFound => {
                log::warn!("Configuration file does not exist: {:?}", file_path);
                Ok(Config { models: vec![] })
            }
            Err(e) => Err(Box::new(e)),
        }
    }

    // Merge multiple Config objects
    fn merge_configs(configs: Vec<Config>) -> Config {
        let mut merged_models: HashMap<String, ModelConfig> = HashMap::new();

        // Merge in order, later models with same name will override earlier ones
        for config in configs {
            for model in config.models {
                merged_models.insert(model.model_name.clone(), model);
            }
        }

        Config {
            models: merged_models.into_values().collect(),
        }
    }

    // Get current device model with two-level matching strategy
    fn get_current_model() -> Result<String, Box<dyn std::error::Error>> {
        // First try: Match by product_name
        let product_name_path = Path::new("/sys/class/dmi/id/product_name");
        if product_name_path.exists() {
            let model = fs::read_to_string(product_name_path)?.trim().to_string();

            log::info!("Found product name: {}", model);

            // Check if this model exists in our merged configs
            if Self::check_model_exists(&model)? {
                log::info!("Found matching configuration for product name: {}", model);
                return Ok(model);
            } else {
                log::info!(
                    "No matching configuration found for product name: {}",
                    model
                );
            }
        }

        // Second try: Match by CPU model
        let cpu_info_path = Path::new("/proc/cpuinfo");
        if cpu_info_path.exists() {
            let cpu_info = fs::read_to_string(cpu_info_path)?;

            // Extract CPU model name
            for line in cpu_info.lines() {
                if line.starts_with("model name") {
                    if let Some(model) = line.split(':').nth(1) {
                        let model = model.trim().to_string();
                        log::info!("Found CPU model: {}", model);

                        // Check if this CPU model exists in our merged configs
                        if Self::check_model_exists(&model)? {
                            log::info!("Found matching configuration for CPU model: {}", model);
                            return Ok(model);
                        } else {
                            log::info!("No matching configuration found for CPU model: {}", model);
                        }
                    }
                    break;
                }
            }
        }

        // If we reach here, no matching configuration was found
        // Return the product name as fallback if available
        if product_name_path.exists() {
            let model = fs::read_to_string(product_name_path)?.trim().to_string();
            log::warn!(
                "No matching configuration found, using product name as fallback: {}",
                model
            );
            return Ok(model);
        }

        // Last resort fallback
        log::warn!("Could not determine model name, using default");
        Ok("Unknown Model".to_string())
    }

    // Helper method to check if a model exists in our configuration
    fn check_model_exists(model: &str) -> Result<bool, Box<dyn std::error::Error>> {
        let platform_dir = Path::new(Self::PLATFORM_DIR);

        // Read each database file
        let amd_db_path = platform_dir.join(Self::AMD_APU_DATABASE);
        let intel_db_path = platform_dir.join(Self::INTEL_APU_DATABASE);
        let dmi_overrides_path = platform_dir.join(Self::DMI_OVERRIDES_APU_DATABASE);

        let amd_config = Self::load_config_file(&amd_db_path)?;
        let intel_config = Self::load_config_file(&intel_db_path)?;
        let dmi_overrides_config = Self::load_config_file(&dmi_overrides_path)?;

        // Merge configurations
        let merged_config =
            Self::merge_configs(vec![amd_config, intel_config, dmi_overrides_config]);

        // Check if model exists in merged config
        for config_model in &merged_config.models {
            if config_model.model_name == model {
                return Ok(true);
            }
        }

        Ok(false)
    }

    pub fn min_tdp(&self) -> f64 {
        self.min_tdp
    }

    pub fn max_tdp(&self) -> f64 {
        self.max_tdp
    }

    pub fn max_boost(&self) -> f64 {
        self.max_boost
    }
}
