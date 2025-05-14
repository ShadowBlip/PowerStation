use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct Config {
    pub models: Vec<ModelConfig>,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct ModelConfig {
    pub model_name: String,
    pub min_tdp: f64,
    pub max_tdp: f64,
    pub max_boost: f64,
}
