use std::sync::Arc;

use tokio::sync::Mutex;

use crate::performance::gpu::{interface::GPUDevice, tdp::{TDPDevice, TDPResult}};
use crate::performance::gpu::interface::GPUResult;

pub enum TDPDevices {
    ASUS(crate::performance::gpu::amd::asus::ASUS),
    AMD(crate::performance::gpu::amd::tdp::TDP),
    INTEL(crate::performance::gpu::intel::tdp::TDP)
}

impl TDPDevices {
    pub async fn tdp(&self) -> TDPResult<f64> {
        match self {
            Self::ASUS(dev) => dev.tdp().await,
            Self::AMD(dev) => dev.tdp().await,
            Self::INTEL(dev) => dev.tdp().await,
        }
    }

    pub async fn set_tdp(&mut self, value: f64) -> TDPResult<()> {
        match self {
            Self::ASUS(dev) => dev.set_tdp(value).await,
            Self::AMD(dev) => dev.set_tdp(value).await,
            Self::INTEL(dev) => dev.set_tdp(value).await,
        }
    }

    pub async fn boost(&self) -> TDPResult<f64> {
        match self {
            Self::ASUS(dev) => dev.boost().await,
            Self::AMD(dev) => dev.boost().await,
            Self::INTEL(dev) => dev.boost().await,
        }
    }

    pub async fn set_boost(&mut self, value: f64) -> TDPResult<()> {
        match self {
            Self::ASUS(dev) => dev.set_boost(value).await,
            Self::AMD(dev) => dev.set_boost(value).await,
            Self::INTEL(dev) => dev.set_boost(value).await,
        }
    }

    pub async fn thermal_throttle_limit_c(&self) -> TDPResult<f64> {
        match self {
            Self::ASUS(dev) => dev.thermal_throttle_limit_c().await,
            Self::AMD(dev) => dev.thermal_throttle_limit_c().await,
            Self::INTEL(dev) => dev.thermal_throttle_limit_c().await,
        }
    }

    pub async fn set_thermal_throttle_limit_c(&mut self, limit: f64) -> TDPResult<()> {
        match self {
            Self::ASUS(dev) => dev.set_thermal_throttle_limit_c(limit).await,
            Self::AMD(dev) => dev.set_thermal_throttle_limit_c(limit).await,
            Self::INTEL(dev) => dev.set_thermal_throttle_limit_c(limit).await,
        }
    }

    pub async fn power_profile(&self) -> TDPResult<String> {
        match self {
            Self::ASUS(dev) => dev.power_profile().await,
            Self::AMD(dev) => dev.power_profile().await,
            Self::INTEL(dev) => dev.power_profile().await,
        }
    }

    pub async fn set_power_profile(&mut self, profile: String) -> TDPResult<()> {
        match self {
            Self::ASUS(dev) => dev.set_power_profile(profile).await,
            Self::AMD(dev) => dev.set_power_profile(profile).await,
            Self::INTEL(dev) => dev.set_power_profile(profile).await,
        }
    }

}

pub enum GPUDevices {
    AMDGPU(crate::performance::gpu::amd::amdgpu::AMDGPU),
    INTELGPU(crate::performance::gpu::intel::intelgpu::IntelGPU),
}

impl GPUDevices {
    pub async fn get_tdp_interface(&self) -> Option<Arc<Mutex<TDPDevices>>> {
        match self {
            Self::AMDGPU(dev) => dev.get_tdp_interface().await,
            Self::INTELGPU(dev) => dev.get_tdp_interface().await,
        }
    }

    pub async fn get_gpu_path(&self) -> String {
        match self {
            Self::AMDGPU(dev) => dev.get_gpu_path().await,
            Self::INTELGPU(dev) => dev.get_gpu_path().await,
        }
    }
    
    pub async fn name(&self) -> String {
        match self {
            Self::AMDGPU(dev) => dev.name().await,
            Self::INTELGPU(dev) => dev.name().await,
        }
    }

    pub async fn path(&self) -> String {
        match self {
            Self::AMDGPU(dev) => dev.path().await,
            Self::INTELGPU(dev) => dev.path().await,
        }
    }

    pub async fn class(&self) -> String {
        match self {
            Self::AMDGPU(dev) => dev.class().await,
            Self::INTELGPU(dev) => dev.class().await,
        }
    }

    pub async fn class_id(&self) -> String {
        match self {
            Self::AMDGPU(dev) => dev.class_id().await,
            Self::INTELGPU(dev) => dev.class_id().await,
        }
    }

    pub async fn vendor(&self) -> String {
        match self {
            Self::AMDGPU(dev) => dev.vendor().await,
            Self::INTELGPU(dev) => dev.vendor().await,
        }
    }

    pub async fn vendor_id(&self) -> String {
        match self {
            Self::AMDGPU(dev) => dev.vendor_id().await,
            Self::INTELGPU(dev) => dev.vendor_id().await,
        }
    }

    pub async fn device(&self) -> String {
        match self {
            Self::AMDGPU(dev) => dev.device().await,
            Self::INTELGPU(dev) => dev.device().await,
        }
    }

    pub async fn device_id(&self) -> String {
        match self {
            Self::AMDGPU(dev) => dev.device_id().await,
            Self::INTELGPU(dev) => dev.device_id().await,
        }
    }

    pub async fn subdevice(&self) -> String {
        match self {
            Self::AMDGPU(dev) => dev.subdevice().await,
            Self::INTELGPU(dev) => dev.subdevice().await,
        }
    }

    pub async fn subdevice_id(&self) -> String {
        match self {
            Self::AMDGPU(dev) => dev.subdevice_id().await,
            Self::INTELGPU(dev) => dev.subdevice_id().await,
        }
    }

    pub async fn subvendor_id(&self) -> String {
        match self {
            Self::AMDGPU(dev) => dev.subvendor_id().await,
            Self::INTELGPU(dev) => dev.subvendor_id().await,
        }
    }

    pub async fn revision_id(&self) -> String {
        match self {
            Self::AMDGPU(dev) => dev.revision_id().await,
            Self::INTELGPU(dev) => dev.revision_id().await,
        }
    }

    pub async fn clock_limit_mhz_min(&self) -> GPUResult<f64> {
        match self {
            Self::AMDGPU(dev) => dev.clock_limit_mhz_min().await,
            Self::INTELGPU(dev) => dev.clock_limit_mhz_min().await,
        }
    }

    pub async fn clock_limit_mhz_max(&self) -> GPUResult<f64> {
        match self {
            Self::AMDGPU(dev) => dev.clock_limit_mhz_max().await,
            Self::INTELGPU(dev) => dev.clock_limit_mhz_max().await,
        }
    }

    pub async fn clock_value_mhz_min(&self) -> GPUResult<f64> {
        match self {
            Self::AMDGPU(dev) => dev.clock_value_mhz_min().await,
            Self::INTELGPU(dev) => dev.clock_value_mhz_min().await,
        }
    }

    pub async fn set_clock_value_mhz_min(&mut self, value: f64) -> GPUResult<()> {
        match self {
            Self::AMDGPU(dev) => dev.set_clock_value_mhz_min(value).await,
            Self::INTELGPU(dev) => dev.set_clock_value_mhz_min(value).await,
        }
    }

    pub async fn clock_value_mhz_max(&self) -> GPUResult<f64> {
        match self {
            Self::AMDGPU(dev) => dev.clock_value_mhz_max().await,
            Self::INTELGPU(dev) => dev.clock_value_mhz_max().await,
        }
    }

    pub async fn set_clock_value_mhz_max(&mut self, value: f64) -> GPUResult<()> {
        match self {
            Self::AMDGPU(dev) => dev.set_clock_value_mhz_max(value).await,
            Self::INTELGPU(dev) => dev.set_clock_value_mhz_max(value).await,
        }
    }

    pub async fn manual_clock(&self) -> GPUResult<bool> {
        match self {
            Self::AMDGPU(dev) => dev.manual_clock().await,
            Self::INTELGPU(dev) => dev.manual_clock().await,
        }
    }

    pub async fn set_manual_clock(&mut self, enabled: bool) -> GPUResult<()> {
        match self {
            Self::AMDGPU(dev) => dev.set_manual_clock(enabled).await,
            Self::INTELGPU(dev) => dev.set_manual_clock(enabled).await,
        }
    }

}
