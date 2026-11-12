pub mod dbus;
pub mod hwmon;
pub mod manager;
pub mod profile;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum FanError {
    #[error("invalid profile: {0}")]
    InvalidProfile(String),
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("invalid JSON profile: {0}")]
    Json(#[from] serde_json::Error),
    #[error("fan manager is not running")]
    ManagerStopped,
    #[error("invalid sysfs value: {0}")]
    Parse(#[from] std::num::ParseIntError),
    #[error("invalid TOML profile: {0}")]
    Toml(#[from] toml::de::Error),
}

impl From<FanError> for zbus::fdo::Error {
    fn from(error: FanError) -> Self {
        zbus::fdo::Error::Failed(error.to_string())
    }
}
