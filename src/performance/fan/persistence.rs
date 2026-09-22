use super::config::CurvePoint;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt::{Display, Formatter};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

const STATE_SCHEMA_VERSION: u32 = 1;

#[derive(Debug)]
pub struct PersistenceError(pub String);

impl Display for PersistenceError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for PersistenceError {}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SavedMode {
    Automatic,
    Manual,
    Curve,
    Preset,
}

impl Default for SavedMode {
    fn default() -> Self {
        Self::Automatic
    }
}

impl SavedMode {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Automatic => "automatic",
            Self::Manual => "manual",
            Self::Curve => "curve",
            Self::Preset => "preset",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct SavedFanState {
    pub mode: SavedMode,
    #[serde(default = "default_manual_percent")]
    pub manual_percent: u8,
    #[serde(default)]
    pub curve: Vec<CurvePoint>,
    pub preset: Option<String>,
}

fn default_manual_percent() -> u8 {
    50
}

impl Default for SavedFanState {
    fn default() -> Self {
        Self {
            mode: SavedMode::Automatic,
            manual_percent: default_manual_percent(),
            curve: Vec::new(),
            preset: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StateFile {
    schema_version: u32,
    #[serde(default)]
    fans: BTreeMap<String, SavedFanState>,
}

impl Default for StateFile {
    fn default() -> Self {
        Self {
            schema_version: STATE_SCHEMA_VERSION,
            fans: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Deserialize)]
struct LegacyAy3State {
    mode: Option<String>,
}

#[derive(Clone)]
pub struct StateStore {
    path: PathBuf,
    state: Arc<Mutex<StateFile>>,
}

impl StateStore {
    pub fn open(path: PathBuf) -> Result<Self, PersistenceError> {
        let state = if path.exists() {
            let raw = fs::read_to_string(&path).map_err(|error| {
                PersistenceError(format!("unable to read {}: {error}", path.display()))
            })?;
            let state: StateFile = serde_yaml::from_str(&raw).map_err(|error| {
                PersistenceError(format!("invalid {}: {error}", path.display()))
            })?;
            if state.schema_version != STATE_SCHEMA_VERSION {
                return Err(PersistenceError(format!(
                    "unsupported fan state schema version {}",
                    state.schema_version
                )));
            }
            state
        } else {
            StateFile::default()
        };
        Ok(Self {
            path,
            state: Arc::new(Mutex::new(state)),
        })
    }

    pub fn get(&self, key: &str) -> SavedFanState {
        self.state
            .lock()
            .expect("fan state lock poisoned")
            .fans
            .get(key)
            .cloned()
            .unwrap_or_default()
    }

    pub fn contains(&self, key: &str) -> bool {
        self.state
            .lock()
            .expect("fan state lock poisoned")
            .fans
            .contains_key(key)
    }

    pub fn set(&self, key: &str, mode: SavedFanState) -> Result<(), PersistenceError> {
        let mut state = self.state.lock().expect("fan state lock poisoned");
        state.fans.insert(key.to_string(), mode);
        self.save_locked(&state)
    }

    pub fn migrate_legacy_quiet(
        &self,
        key: &str,
        legacy_path: &Path,
        quiet_available: bool,
    ) -> Result<bool, PersistenceError> {
        if self.contains(key) || !quiet_available || !legacy_path.exists() {
            return Ok(false);
        }
        let raw = fs::read_to_string(legacy_path).map_err(|error| {
            PersistenceError(format!(
                "unable to read legacy fan state {}: {error}",
                legacy_path.display()
            ))
        })?;
        let legacy: LegacyAy3State = serde_json::from_str(&raw).map_err(|error| {
            PersistenceError(format!(
                "invalid legacy fan state {}: {error}",
                legacy_path.display()
            ))
        })?;
        if legacy.mode.as_deref() != Some("quiet") {
            return Ok(false);
        }
        self.set(
            key,
            SavedFanState {
                mode: SavedMode::Preset,
                manual_percent: 15,
                curve: Vec::new(),
                preset: Some("Quiet".into()),
            },
        )?;
        Ok(true)
    }

    fn save_locked(&self, state: &StateFile) -> Result<(), PersistenceError> {
        let parent = self.path.parent().ok_or_else(|| {
            PersistenceError(format!("state path {} has no parent", self.path.display()))
        })?;
        fs::create_dir_all(parent).map_err(|error| {
            PersistenceError(format!(
                "unable to create fan state directory {}: {error}",
                parent.display()
            ))
        })?;
        let raw = serde_yaml::to_string(state)
            .map_err(|error| PersistenceError(format!("unable to encode fan state: {error}")))?;
        let temporary = self.path.with_extension("yaml.tmp");
        let mut options = OpenOptions::new();
        options.create(true).truncate(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temporary).map_err(|error| {
            PersistenceError(format!("unable to create {}: {error}", temporary.display()))
        })?;
        file.write_all(raw.as_bytes()).map_err(|error| {
            PersistenceError(format!("unable to write {}: {error}", temporary.display()))
        })?;
        file.sync_all().map_err(|error| {
            PersistenceError(format!("unable to sync {}: {error}", temporary.display()))
        })?;
        fs::rename(&temporary, &self.path).map_err(|error| {
            let _ = fs::remove_file(&temporary);
            PersistenceError(format!(
                "unable to replace fan state {}: {error}",
                self.path.display()
            ))
        })?;
        #[cfg(unix)]
        fs::File::open(parent)
            .and_then(|directory| directory.sync_all())
            .map_err(|error| {
                PersistenceError(format!(
                    "unable to sync fan state directory {}: {error}",
                    parent.display()
                ))
            })?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_ID: AtomicU64 = AtomicU64::new(0);

    fn temporary_directory() -> PathBuf {
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "powerstation-fan-state-{}-{id}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn state_roundtrips_atomically() {
        let root = temporary_directory();
        let path = root.join("fan-state.yaml");
        let state = StateStore::open(path.clone()).unwrap();
        state
            .set(
                "device/fan",
                SavedFanState {
                    mode: SavedMode::Manual,
                    manual_percent: 42,
                    ..Default::default()
                },
            )
            .unwrap();
        let reopened = StateStore::open(path).unwrap();
        assert_eq!(
            reopened.get("device/fan"),
            SavedFanState {
                mode: SavedMode::Manual,
                manual_percent: 42,
                ..Default::default()
            }
        );
        assert!(!root.join("fan-state.yaml.tmp").exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn legacy_quiet_migrates_once_to_named_preset() {
        let root = temporary_directory();
        let legacy = root.join("config.json");
        fs::write(
            &legacy,
            r#"{"mode":"quiet","percent":15,"curve":[10,30,40,70,100]}"#,
        )
        .unwrap();
        let state = StateStore::open(root.join("fan-state.yaml")).unwrap();
        assert!(state
            .migrate_legacy_quiet("ayaneo-3/system", &legacy, true)
            .unwrap());
        assert_eq!(
            state.get("ayaneo-3/system"),
            SavedFanState {
                mode: SavedMode::Preset,
                manual_percent: 15,
                curve: Vec::new(),
                preset: Some("Quiet".into())
            }
        );
        assert!(!state
            .migrate_legacy_quiet("ayaneo-3/system", &legacy, true)
            .unwrap());
        let _ = fs::remove_dir_all(root);
    }
}
