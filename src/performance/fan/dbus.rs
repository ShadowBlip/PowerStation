use std::{
    fs::{self, File},
    os::fd::{AsRawFd, OwnedFd},
};

use zbus::{fdo, object_server::SignalEmitter, zvariant};
use zbus_macros::interface;

use super::{
    hwmon::{Fan, Sensor},
    manager::FanClient,
    profile::{packaged_profiles, Mode, Profile},
};

/// DBus interface for loading fan profiles and listing fans and sensors
pub struct FanBus {
    client: FanClient,
    fan_paths: Vec<String>,
    sensors: Vec<String>,
}

impl FanBus {
    pub fn new(client: FanClient, fan_paths: Vec<String>, sensors: &[Sensor]) -> Self {
        let sensors = sensors.iter().map(|sensor| sensor.id.clone()).collect();
        Self {
            client,
            fan_paths,
            sensors,
        }
    }

    async fn load(
        &self,
        profile: Profile,
        path: String,
        emitter: &SignalEmitter<'_>,
    ) -> fdo::Result<()> {
        self.client.load(profile, path).await?;
        self.profile_name_changed(emitter).await?;
        self.profile_path_changed(emitter).await?;
        Ok(())
    }
}

#[interface(name = "org.shadowblip.Fan")]
impl FanBus {
    /// Load a TOML profile from an open file.
    async fn load_profile(
        &self,
        file: zvariant::OwnedFd,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> fdo::Result<()> {
        let fd = OwnedFd::from(file);
        let path = fs::read_link(format!("/proc/self/fd/{}", fd.as_raw_fd()))
            .map(|path| path.display().to_string())
            .unwrap_or_default();
        let profile = Profile::from_file(File::from(fd))?;
        self.load(profile, path, &emitter).await
    }

    /// Load a profile from a JSON string.
    async fn load_profile_json(
        &self,
        json: &str,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> fdo::Result<()> {
        let profile = Profile::from_json(json)?;
        self.load(profile, String::new(), &emitter).await
    }

    /// Returns a list of DBus paths to all fans.
    fn enumerate_fans(&self) -> Vec<zvariant::ObjectPath<'_>> {
        self.fan_paths
            .iter()
            .map(|path| zvariant::ObjectPath::from_str_unchecked(path))
            .collect()
    }

    /// IDs of all temperature sensors that can be used in profiles.
    #[zbus(property)]
    fn sensors(&self) -> Vec<String> {
        self.sensors.clone()
    }

    /// Paths of the installed profiles for this device.
    #[zbus(property)]
    fn profiles(&self) -> Vec<String> {
        packaged_profiles()
    }

    /// Name of the loaded profile. Empty when no profile is loaded.
    #[zbus(property)]
    async fn profile_name(&self) -> fdo::Result<String> {
        Ok(self.client.state().await?.name)
    }

    /// Path of the loaded profile. Empty for JSON profiles or when no
    /// profile is loaded.
    #[zbus(property)]
    async fn profile_path(&self) -> fdo::Result<String> {
        Ok(self.client.state().await?.path)
    }
}

/// DBus interface for a single fan
pub struct FanDevice {
    client: FanClient,
    index: usize,
    fan: Fan,
}

impl FanDevice {
    pub fn new(client: FanClient, index: usize, fan: Fan) -> Self {
        Self { client, index, fan }
    }
}

#[interface(name = "org.shadowblip.Fan.Device")]
impl FanDevice {
    /// Fan ID used in profiles.
    #[zbus(property)]
    fn id(&self) -> String {
        self.fan.id.clone()
    }

    /// Modes this fan supports.
    #[zbus(property)]
    fn available_modes(&self) -> Vec<String> {
        let mut modes = vec![Mode::Automatic];
        if self.fan.firmware_curve_points() > 0 {
            modes.push(Mode::FirmwareCurve);
        }
        modes.push(Mode::Manual);
        modes.push(Mode::SoftwareCurve);
        modes.iter().map(|mode| mode.as_str().to_string()).collect()
    }

    /// Current mode of the fan.
    #[zbus(property)]
    async fn mode(&self) -> fdo::Result<String> {
        let state = self.client.state().await?;
        let mode = state.modes.get(self.index).unwrap_or(&Mode::Automatic);
        Ok(mode.as_str().to_string())
    }

    /// Current fan speed from 0 to 100 percent.
    #[zbus(property)]
    fn percent(&self) -> fdo::Result<u8> {
        Ok(self.fan.percent()?)
    }
}
