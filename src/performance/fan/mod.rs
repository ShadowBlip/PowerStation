pub mod config;
pub mod controller;
pub mod dbus;
pub mod discovery;
pub mod manager;
pub mod profile;

pub use manager::FanManager;

use std::fmt::{Display, Formatter};

#[derive(Debug, Clone)]
pub struct FanError(pub String);

impl FanError {
    pub fn new(error: impl Display) -> Self {
        Self(error.to_string())
    }
}

impl Display for FanError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for FanError {}

impl From<std::io::Error> for FanError {
    fn from(error: std::io::Error) -> Self {
        Self::new(error)
    }
}

impl From<serde_yaml::Error> for FanError {
    fn from(error: serde_yaml::Error) -> Self {
        Self::new(error)
    }
}

impl From<zbus::Error> for FanError {
    fn from(error: zbus::Error) -> Self {
        Self::new(error)
    }
}

impl From<zbus::fdo::Error> for FanError {
    fn from(error: zbus::fdo::Error) -> Self {
        Self::new(error)
    }
}

#[cfg(test)]
mod test_support;
