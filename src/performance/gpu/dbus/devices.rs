use crate::performance::gpu::tdp::{TDPDevice, TDPResult};

pub enum TDPDevices {
    //ASUS(crate::performance::gpu::amd::asus::ASUS),
    AMD(crate::performance::gpu::amd::tdp::TDP),
    INTEL(crate::performance::gpu::intel::tdp::TDP)
}

impl TDPDevices {
    pub async fn tdp(&self) -> TDPResult<f64> {
        match self {
            //Self::ASUS(dev) => dev.tdp().await,
            Self::AMD(dev) => dev.tdp().await,
            Self::INTEL(dev) => dev.tdp().await,
        }
    }

    pub async fn set_tdp(&mut self, value: f64) -> TDPResult<()> {
        match self {
            //Self::ASUS(dev) => dev.set_tdp(value).await,
            Self::AMD(dev) => dev.set_tdp(value).await,
            Self::INTEL(dev) => dev.set_tdp(value).await,
        }
    }

    pub async fn boost(&self) -> TDPResult<f64> {
        match self {
            //Self::ASUS(dev) => dev.boost().await,
            Self::AMD(dev) => dev.boost().await,
            Self::INTEL(dev) => dev.boost().await,
        }
    }

    pub async fn set_boost(&mut self, value: f64) -> TDPResult<()> {
        match self {
            //Self::ASUS(dev) => dev.set_boost(value).await,
            Self::AMD(dev) => dev.set_boost(value).await,
            Self::INTEL(dev) => dev.set_boost(value).await,
        }
    }

    pub async fn thermal_throttle_limit_c(&self) -> TDPResult<f64> {
        match self {
            //Self::ASUS(dev) => dev.thermal_throttle_limit_c().await,
            Self::AMD(dev) => dev.thermal_throttle_limit_c().await,
            Self::INTEL(dev) => dev.thermal_throttle_limit_c().await,
        }
    }

    pub async fn set_thermal_throttle_limit_c(&mut self, limit: f64) -> TDPResult<()> {
        match self {
            //Self::ASUS(dev) => dev.set_thermal_throttle_limit_c(limit).await,
            Self::AMD(dev) => dev.set_thermal_throttle_limit_c(limit).await,
            Self::INTEL(dev) => dev.set_thermal_throttle_limit_c(limit).await,
        }
    }

    pub async fn power_profile(&self) -> TDPResult<String> {
        match self {
            //Self::ASUS(dev) => dev.power_profile().await,
            Self::AMD(dev) => dev.power_profile().await,
            Self::INTEL(dev) => dev.power_profile().await,
        }
    }

    pub async fn set_power_profile(&mut self, profile: String) -> TDPResult<()> {
        match self {
            //Self::ASUS(dev) => dev.set_power_profile(profile).await,
            Self::AMD(dev) => dev.set_power_profile(profile).await,
            Self::INTEL(dev) => dev.set_power_profile(profile).await,
        }
    }

}