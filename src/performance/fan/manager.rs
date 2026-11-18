use std::time::Duration;

use tokio::sync::{mpsc, oneshot};

use super::{
    hwmon::{Fan, Sensor, PWM_AUTOMATIC, PWM_MANUAL},
    profile::{interpolate, validate_curve, Mode, Profile},
    FanError,
};

/// Fans run at 100% when any of their sensors reaches this temperature,
/// regardless of the loaded profile.
const CRITICAL_TEMPERATURE: f64 = 95.0;
const UPDATE_INTERVAL: Duration = Duration::from_secs(1);
/// Failed updates in a row before a fan returns to automatic. Some embedded
/// controllers return an occasional one-off I/O error.
const MAX_FAILURES: u8 = 3;
/// Largest drop in percent per update for software curves, so the fan winds
/// down smoothly instead of following every temperature spike.
const MAX_STEP_DOWN: u8 = 2;

pub enum Command {
    Load {
        profile: Profile,
        path: String,
        reply: oneshot::Sender<Result<(), FanError>>,
    },
    State {
        reply: oneshot::Sender<State>,
    },
}

/// Current profile name and path, and the mode of each fan
#[derive(Debug, Clone, Default)]
pub struct State {
    pub name: String,
    pub path: String,
    pub modes: Vec<Mode>,
}

/// Handle used by DBus interfaces to talk to the [FanManager]
#[derive(Debug, Clone)]
pub struct FanClient {
    tx: mpsc::Sender<Command>,
}

impl FanClient {
    pub async fn load(&self, profile: Profile, path: String) -> Result<(), FanError> {
        let (reply, rx) = oneshot::channel();
        let command = Command::Load {
            profile,
            path,
            reply,
        };
        self.tx
            .send(command)
            .await
            .or(Err(FanError::ManagerStopped))?;
        rx.await.or(Err(FanError::ManagerStopped))?
    }

    pub async fn state(&self) -> Result<State, FanError> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(Command::State { reply })
            .await
            .or(Err(FanError::ManagerStopped))?;
        rx.await.or(Err(FanError::ManagerStopped))
    }
}

/// Settings applied to a fan by the current profile
#[derive(Debug, Clone)]
struct Control {
    mode: Mode,
    percent: u8,
    sensors: Vec<Sensor>,
    curve: Vec<(u8, u8)>,
    /// Last percent written, None until the first update
    last_percent: Option<u8>,
    failures: u8,
}

/// Owns all fans and applies profiles. Every fan returns to firmware control
/// when the manager is dropped.
pub struct FanManager {
    fans: Vec<Fan>,
    sensors: Vec<Sensor>,
    controls: Vec<Option<Control>>,
    name: String,
    path: String,
    rx: mpsc::Receiver<Command>,
}

impl FanManager {
    pub fn new(fans: Vec<Fan>, sensors: Vec<Sensor>) -> (Self, FanClient) {
        let (tx, rx) = mpsc::channel(8);
        let manager = Self {
            controls: vec![None; fans.len()],
            fans,
            sensors,
            name: String::new(),
            path: String::new(),
            rx,
        };
        (manager, FanClient { tx })
    }

    pub async fn run(mut self) {
        self.set_all_automatic();
        let mut interval = tokio::time::interval(UPDATE_INTERVAL);
        loop {
            tokio::select! {
                _ = interval.tick() => self.update_all(),
                command = self.rx.recv() => {
                    let Some(command) = command else {
                        break;
                    };
                    self.handle(command);
                }
            }
        }
    }

    fn handle(&mut self, command: Command) {
        match command {
            Command::Load {
                profile,
                path,
                reply,
            } => {
                let result = self.load(profile, path);
                if let Err(e) = &result {
                    log::error!("Failed to load fan profile: {e}");
                }
                let _ = reply.send(result);
            }
            Command::State { reply } => {
                let modes = self
                    .controls
                    .iter()
                    .map(|control| control.as_ref().map_or(Mode::Automatic, |c| c.mode))
                    .collect();
                let state = State {
                    name: self.name.clone(),
                    path: self.path.clone(),
                    modes,
                };
                let _ = reply.send(state);
            }
        }
    }

    fn load(&mut self, profile: Profile, path: String) -> Result<(), FanError> {
        let controls = self.validate(&profile)?;

        self.controls = controls;
        self.name = profile.name;
        self.path = path;
        log::info!("Loading fan profile '{}'", self.name);

        for index in 0..self.fans.len() {
            let Err(e) = self.apply(index) else {
                continue;
            };
            self.set_all_automatic();
            return Err(e);
        }
        Ok(())
    }

    /// Check the whole profile against the discovered hardware before any
    /// fan is changed. Fans not listed in the profile use automatic mode.
    fn validate(&self, profile: &Profile) -> Result<Vec<Option<Control>>, FanError> {
        if profile.name.trim().is_empty() {
            return Err(FanError::InvalidProfile("name is empty".into()));
        }
        if !profile.matches_device() {
            return Err(FanError::InvalidProfile(
                "profile is for a different device".into(),
            ));
        }

        let mut controls: Vec<Option<Control>> = vec![None; self.fans.len()];
        let mut listed = vec![false; self.fans.len()];
        for fan_profile in &profile.fans {
            let Some(index) = self.fans.iter().position(|f| f.id == fan_profile.fan) else {
                return Err(FanError::InvalidProfile(format!(
                    "unknown fan '{}'",
                    fan_profile.fan
                )));
            };
            if listed[index] {
                return Err(FanError::InvalidProfile(format!(
                    "fan '{}' is listed twice",
                    fan_profile.fan
                )));
            }
            listed[index] = true;

            let mut sensors = Vec::new();
            for id in &fan_profile.sensors {
                let Some(sensor) = self.sensors.iter().find(|s| &s.id == id) else {
                    return Err(FanError::InvalidProfile(format!("unknown sensor '{id}'")));
                };
                sensors.push(sensor.clone());
            }

            let mode = fan_profile.mode;
            let percent = fan_profile.percent.unwrap_or(0);
            let curve = fan_profile.curve.clone();
            match mode {
                Mode::Automatic => {}
                Mode::FirmwareCurve => {
                    let points = self.fans[index].firmware_curve_points();
                    if points == 0 {
                        return Err(FanError::InvalidProfile(format!(
                            "fan '{}' has no firmware curve",
                            fan_profile.fan
                        )));
                    }
                    if curve.len() != points {
                        return Err(FanError::InvalidProfile(format!(
                            "fan '{}' needs exactly {points} curve points",
                            fan_profile.fan
                        )));
                    }
                    validate_curve(&curve)?;
                    let reaches_full_speed = curve
                        .iter()
                        .any(|(temp, p)| *p == 100 && *temp as f64 <= CRITICAL_TEMPERATURE);
                    if !reaches_full_speed {
                        return Err(FanError::InvalidProfile(format!(
                            "firmware curves must reach 100% by {CRITICAL_TEMPERATURE}°C"
                        )));
                    }
                }
                Mode::Manual => {
                    if fan_profile.percent.is_none_or(|p| p > 100) {
                        return Err(FanError::InvalidProfile(
                            "manual mode needs a percent from 0-100".into(),
                        ));
                    }
                    if sensors.is_empty() {
                        return Err(FanError::InvalidProfile(
                            "manual mode needs at least one sensor".into(),
                        ));
                    }
                }
                Mode::SoftwareCurve => {
                    if sensors.is_empty() {
                        return Err(FanError::InvalidProfile(
                            "software curves need at least one sensor".into(),
                        ));
                    }
                    validate_curve(&curve)?;
                }
            }

            if mode == Mode::Automatic {
                continue;
            }
            controls[index] = Some(Control {
                mode,
                percent,
                sensors,
                curve,
                last_percent: None,
                failures: 0,
            });
        }

        Ok(controls)
    }

    /// Write the current settings of a fan to hardware
    fn apply(&mut self, index: usize) -> Result<(), FanError> {
        let fan = &self.fans[index];
        let Some(control) = &mut self.controls[index] else {
            return fan.set_enable(PWM_AUTOMATIC);
        };

        match control.mode {
            Mode::Automatic => fan.set_enable(PWM_AUTOMATIC),
            Mode::FirmwareCurve => {
                fan.set_firmware_curve(&control.curve)?;
                fan.set_enable(PWM_AUTOMATIC)
            }
            Mode::Manual | Mode::SoftwareCurve => update(fan, control),
        }
    }

    fn update_all(&mut self) {
        for (fan, slot) in self.fans.iter().zip(self.controls.iter_mut()) {
            let Some(control) = slot else {
                continue;
            };
            let Err(e) = update(fan, control) else {
                control.failures = 0;
                continue;
            };
            control.failures += 1;
            if control.failures < MAX_FAILURES {
                log::warn!("Failed to update fan {}: {e}", fan.id);
                continue;
            }
            log::error!("Returning fan {} to automatic: {e}", fan.id);
            *slot = None;
            if let Err(e) = fan.set_enable(PWM_AUTOMATIC) {
                log::error!("Failed to set fan {} to automatic: {e}", fan.id);
            }
        }
    }

    fn set_all_automatic(&mut self) {
        self.controls = vec![None; self.fans.len()];
        self.name.clear();
        self.path.clear();
        for fan in &self.fans {
            if let Err(e) = fan.set_enable(PWM_AUTOMATIC) {
                log::error!("Failed to set fan {} to automatic: {e}", fan.id);
            }
        }
    }
}

impl Drop for FanManager {
    fn drop(&mut self) {
        log::info!("Returning all fans to automatic");
        self.set_all_automatic();
    }
}

/// Set the fan speed for manual and software curve modes. The fan runs at
/// 100% at the critical temperature. Writing pwmN_enable again restores
/// manual control if the firmware took it back, e.g. after resume.
fn update(fan: &Fan, control: &mut Control) -> Result<(), FanError> {
    let mut temperature = f64::MIN;
    for sensor in &control.sensors {
        temperature = temperature.max(sensor.temperature()?);
    }

    let mut percent = match control.mode {
        _ if temperature >= CRITICAL_TEMPERATURE => 100,
        Mode::SoftwareCurve => interpolate(&control.curve, temperature),
        Mode::Manual => control.percent,
        Mode::Automatic | Mode::FirmwareCurve => return Ok(()),
    };

    // Software curves speed up at once but slow down gradually
    if control.mode == Mode::SoftwareCurve && temperature < CRITICAL_TEMPERATURE {
        if let Some(last) = control.last_percent {
            percent = percent.max(last.saturating_sub(MAX_STEP_DOWN));
        }
    }

    if fan.enable()? != PWM_MANUAL {
        fan.set_enable(PWM_MANUAL)?;
    }
    fan.set_percent(percent)?;
    control.last_percent = Some(percent);
    Ok(())
}
