//! Untrusted selections. Hardware addressing and safety policy live elsewhere.
use super::config::{ConfigError, CurvePoint};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::os::fd::AsRawFd;
use std::os::unix::fs::FileExt;

const MAX_PROFILE_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum FanMode {
    #[default]
    Automatic,
    Manual,
    FirmwareCurve,
    SoftwareCurve,
}

impl FanMode {
    pub fn name(self) -> &'static str {
        match self {
            Self::Automatic => "automatic",
            Self::Manual => "manual",
            Self::FirmwareCurve => "firmware_curve",
            Self::SoftwareCurve => "software_curve",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FanProfile {
    pub fan: String,
    pub mode: FanMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub percent: Option<u8>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sensors: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub curve: Vec<(f64, u8)>,
}

impl FanProfile {
    pub fn automatic(fan: String) -> Self {
        Self {
            fan,
            mode: FanMode::Automatic,
            percent: None,
            sensors: Vec::new(),
            curve: Vec::new(),
        }
    }

    pub fn points(&self) -> Vec<CurvePoint> {
        self.curve
            .iter()
            .map(|&(temperature_c, percent)| CurvePoint {
                temperature_c,
                percent,
            })
            .collect()
    }

    pub(crate) fn validate_shape(&self) -> Result<(), ConfigError> {
        let valid = match self.mode {
            FanMode::Automatic => {
                self.percent.is_none() && self.sensors.is_empty() && self.curve.is_empty()
            }
            FanMode::Manual => {
                self.percent.is_some_and(|percent| percent <= 100)
                    && self.sensors.is_empty()
                    && self.curve.is_empty()
            }
            FanMode::FirmwareCurve | FanMode::SoftwareCurve => {
                self.percent.is_none() && !self.sensors.is_empty() && self.curve.len() >= 2
            }
        };
        let mut sensors = HashSet::new();
        if self.fan.is_empty()
            || !valid
            || self
                .sensors
                .iter()
                .any(|id| id.is_empty() || !sensors.insert(id))
        {
            return Err(ConfigError(format!(
                "invalid fields for fan {} mode {}",
                self.fan,
                self.mode.name()
            )));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    pub name: String,
    pub fans: Vec<FanProfile>,
}

impl Profile {
    pub fn from_json(raw: &str) -> Result<Self, ConfigError> {
        check_size(raw.len())?;
        let profile: Self =
            serde_json::from_str(raw).map_err(|error| ConfigError(error.to_string()))?;
        profile.validate()?;
        Ok(profile)
    }

    pub fn from_yaml(raw: &str) -> Result<Self, ConfigError> {
        check_size(raw.len())?;
        let profile: Self = serde_yaml::from_str(raw)?;
        profile.validate()?;
        Ok(profile)
    }

    fn validate(&self) -> Result<(), ConfigError> {
        let mut fans = HashSet::new();
        if self.name.trim().is_empty() || self.name.len() > 128 || self.fans.is_empty() {
            return Err(ConfigError(
                "profile requires a name and at least one fan".into(),
            ));
        }
        for fan in &self.fans {
            fan.validate_shape()?;
            if !fans.insert(&fan.fan) {
                return Err(ConfigError(format!("duplicate fan ID {}", fan.fan)));
            }
        }
        Ok(())
    }
}

fn check_size(size: usize) -> Result<(), ConfigError> {
    if size > MAX_PROFILE_BYTES {
        return Err(ConfigError("profile exceeds 64 KiB".into()));
    }
    Ok(())
}

/// Read only the caller's open file. Never reopen its path as the service user.
pub fn load_profile(file: &fs::File) -> Result<(Profile, String), ConfigError> {
    let metadata = file.metadata()?;
    let flags = unsafe { libc::fcntl(file.as_raw_fd(), libc::F_GETFL) };
    if flags < 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    if !metadata.is_file() || flags & libc::O_PATH != 0 || flags & libc::O_ACCMODE == libc::O_WRONLY
    {
        return Err(ConfigError(
            "profile descriptor must be a readable regular file".into(),
        ));
    }
    if metadata.len() > MAX_PROFILE_BYTES as u64 {
        return Err(ConfigError("profile exceeds 64 KiB".into()));
    }
    // pread leaves the caller's shared file offset alone. Parse an owned,
    // bounded copy, including when the caller replaces or changes its file.
    let mut bytes = vec![0; MAX_PROFILE_BYTES + 1];
    let mut size = 0;
    while size < bytes.len() {
        match file.read_at(&mut bytes[size..], size as u64) {
            Ok(0) => break,
            Ok(count) => size += count,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error.into()),
        }
    }
    check_size(size)?;
    let raw =
        std::str::from_utf8(&bytes[..size]).map_err(|error| ConfigError(error.to_string()))?;
    let profile = Profile::from_yaml(raw)?;
    let path = fs::read_link(format!("/proc/self/fd/{}", file.as_raw_fd()))
        .ok()
        .filter(|path| {
            path.is_absolute() && !path.as_os_str().as_encoded_bytes().ends_with(b" (deleted)")
        })
        .and_then(|path| path.into_os_string().into_string().ok())
        .unwrap_or_default();
    Ok((profile, path))
}

#[cfg(test)]
mod tests {
    use super::*;

    const YAML: &str = "name: Quiet\nfans:\n  - fan: ayaneo-3/system\n    mode: software_curve\n    sensors: [ayaneo-3/system/cpu]\n    curve: [[45, 0], [95, 100]]\n";

    #[test]
    fn yaml_json_equivalence_and_closed_fields() {
        let profile = Profile::from_yaml(YAML).unwrap();
        assert_eq!(
            Profile::from_json(&serde_json::to_string(&profile).unwrap()).unwrap(),
            profile
        );
        assert!(Profile::from_yaml(&format!("{YAML}sysfs: /etc/shadow\n")).is_err());
        assert!(Profile::from_json(&" ".repeat(MAX_PROFILE_BYTES + 1)).is_err());
    }

    #[test]
    fn rejects_wrong_mode_fields_duplicates_and_bad_percentages() {
        for raw in [
            r#"{"name":"x","fans":[{"fan":"f","mode":"manual","percent":101}]}"#,
            r#"{"name":"x","fans":[{"fan":"f","mode":"automatic","percent":10}]}"#,
            r#"{"name":"x","fans":[{"fan":"f","mode":"manual"}]}"#,
            r#"{"name":"x","fans":[{"fan":"f","mode":"automatic"},{"fan":"f","mode":"automatic"}]}"#,
            r#"{"name":"x","fans":[{"fan":"f","mode":"software_curve","sensors":["s","s"],"curve":[[45,10],[95,100]]}]}"#,
            r#"{"name":"x","fans":[{"fan":"f","mode":"preset"}]}"#,
        ] {
            assert!(Profile::from_json(raw).is_err(), "{raw}");
        }
    }

    #[test]
    fn descriptor_uses_open_file_not_path_or_shared_offset() {
        use std::io::{Seek, SeekFrom};
        let root =
            std::env::temp_dir().join(format!("powerstation-profile-fd-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("profile.yaml");
        let link = root.join("nix-profile.yaml");
        fs::write(&path, YAML).unwrap();
        std::os::unix::fs::symlink(&path, &link).unwrap();
        let mut file = fs::File::open(&link).unwrap();
        file.seek(SeekFrom::End(0)).unwrap();
        let (profile, reported) = load_profile(&file).unwrap();
        assert_eq!(profile.name, "Quiet");
        assert_eq!(reported, path.to_str().unwrap());
        assert_eq!(file.stream_position().unwrap(), YAML.len() as u64);
        fs::rename(&path, root.join("old.yaml")).unwrap();
        fs::write(&path, "invalid replacement").unwrap();
        assert_eq!(load_profile(&file).unwrap().0.name, "Quiet");
        fs::write(root.join("old.yaml"), "name: incomplete").unwrap();
        assert!(load_profile(&file).is_err());
        fs::write(root.join("old.yaml"), " ".repeat(MAX_PROFILE_BYTES + 1)).unwrap();
        assert!(load_profile(&file).is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn descriptor_rejects_special_files_and_unreadable_handles() {
        use std::os::fd::{FromRawFd, OwnedFd};
        use std::os::unix::fs::OpenOptionsExt;
        use std::os::unix::net::UnixStream;
        for path in ["/tmp", "/dev/null"] {
            assert!(load_profile(&fs::File::open(path).unwrap()).is_err());
        }
        let (socket, _peer) = UnixStream::pair().unwrap();
        assert!(load_profile(&fs::File::from(OwnedFd::from(socket))).is_err());
        let mut pipe = [0; 2];
        assert_eq!(
            unsafe { libc::pipe2(pipe.as_mut_ptr(), libc::O_CLOEXEC) },
            0
        );
        let reader = unsafe { fs::File::from_raw_fd(pipe[0]) };
        let _writer = unsafe { fs::File::from_raw_fd(pipe[1]) };
        assert!(load_profile(&reader).is_err());
        let path =
            std::env::temp_dir().join(format!("powerstation-unreadable-fd-{}", std::process::id()));
        let writer = fs::File::create(&path).unwrap();
        assert!(load_profile(&writer).is_err());
        let path_only = fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_PATH)
            .open(&path)
            .unwrap();
        assert!(load_profile(&path_only).is_err());
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn packaged_quiet_curve_is_preserved() {
        let profile = Profile::from_yaml(include_str!(
            "../../../rootfs/usr/powerstation/platform/fan_curves/quiet.yaml"
        ))
        .unwrap();
        assert_eq!(profile.name, "Quiet");
        assert_eq!(
            profile.fans[0].curve,
            vec![
                (45.0, 0),
                (55.0, 15),
                (65.0, 25),
                (75.0, 35),
                (85.0, 55),
                (90.0, 75),
                (95.0, 100)
            ]
        );
    }
}
