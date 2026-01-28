use log::{error, info, warn};
use serde::Deserialize;
use std::fs;
use std::sync::{Arc, RwLock};

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub mappings: Vec<Mapping>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Mapping {
    pub input: InputSpec,
    pub action: Action,
}

#[derive(Debug, Deserialize, Clone)]
pub struct InputSpec {
    #[serde(rename = "type")]
    pub typ: String, // "button" | "axis"
    pub code: String, // eg. "South" or "LeftStickX"

    /// Optional button name that must be held for this input to be active
    #[serde(default)]
    pub while_button: Option<String>,

    /// Optional value to send when the condition button is released (default: 0)
    #[serde(default)]
    pub release_value: Option<u8>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(tag = "type")]
pub enum Action {
    #[serde(rename = "note_on")]
    NoteOn { channel: Option<u8>, note: u8, velocity: Option<u8> },

    #[serde(rename = "note_off")]
    NoteOff { channel: Option<u8>, note: u8, velocity: Option<u8> },

    #[serde(rename = "control_change")]
    ControlChange { channel: Option<u8>, controller: u8, value: u8 },

    #[serde(rename = "program_change")]
    ProgramChange { channel: Option<u8>, program: u8 },

    /// Dynamic control change driven by axis value (-1.0..1.0 mapped to 0..127)
    #[serde(rename = "control_change_from_axis")]
    ControlChangeFromAxis { channel: Option<u8>, controller: u8, invert: Option<bool> },
}

impl Config {
    pub fn load_from(path: &str) -> Result<Self, anyhow::Error> {
        let s = fs::read_to_string(path)?;
        let cfg: Config = toml::from_str(&s)?;
        // Warn if any axis mapping has release_value set to 0 which is likely unintended
        for m in &cfg.mappings {
            if m.input.typ == "axis" {
                if let Some(rv) = m.input.release_value {
                    if rv == 0 {
                        warn!("Mapping for axis {} specifies release_value=0 — this will send 0 on release. Consider setting 64 (center) or removing the field to use the default 64.", m.input.code);
                    }
                }
            }
        }
        Ok(cfg)
    }

    /// Start a background watcher that reloads the config into the provided Arc<RwLock<_>>
    pub fn watch(path: &str, shared: Arc<RwLock<Self>>) -> notify::Result<()> {
        use notify::{EventKind, RecursiveMode, Watcher};
        use std::sync::mpsc::channel;

        let (tx, rx) = channel();
        let mut watcher = notify::recommended_watcher(move |res| {
            if let Err(e) = tx.send(res) {
                error!("watcher send error: {:?}", e);
            }
        })?;

        watcher.watch(path.as_ref(), RecursiveMode::NonRecursive)?;

        // Spawn a thread to receive watch events and reload config
        let path = path.to_string();
        std::thread::spawn(move || {
            info!("Started config watcher for {}", path);
            for res in rx {
                match res {
                    Ok(event) => {
                        // We only care about modify/create events
                        if matches!(event.kind, EventKind::Modify(_) | EventKind::Create(_)) {
                            info!("Config change detected; reloading {}", path);
                            match Self::load_from(&path) {
                                Ok(new_cfg) => {
                                    let mut g = shared.write().unwrap();
                                    *g = new_cfg;
                                    info!("Config reloaded ({} mappings)", g.mappings.len());
                                }
                                Err(e) => {
                                    error!("Failed to load config {}: {:?}", path, e);
                                }
                            }
                        }
                    }
                    Err(e) => {
                        warn!("watch error: {:?}", e);
                    }
                }
            }
        });

        Ok(())
    }

    pub fn find_action_for_button(&self, button_name: &str) -> Option<Action> {
        for m in &self.mappings {
            if m.input.typ == "button" && m.input.code == button_name {
                return Some(m.action.clone());
            }
        }
        None
    }

    pub fn find_action_for_axis(&self, axis_name: &str) -> Option<Action> {
        for m in &self.mappings {
            if m.input.typ == "axis" && m.input.code == axis_name {
                return Some(m.action.clone());
            }
        }
        None
    }

    /// Return the matching mapping (clone) for an axis if present. This includes
    /// the optional `while_button` and `release_value` fields.
    pub fn find_mapping_for_axis(&self, axis_name: &str) -> Option<Mapping> {
        for m in &self.mappings {
            if m.input.typ == "axis" && m.input.code == axis_name {
                return Some(m.clone());
            }
        }
        None
    }
}

pub fn actions_log_path() -> std::path::PathBuf {
    // macOS log location
    let home = std::env::var("HOME").unwrap_or_else(|_| String::from("/tmp"));
    let p = std::path::PathBuf::from(format!("{}/Library/Logs/joy2qlc", home));
    let _ = std::fs::create_dir_all(&p);
    p.join("actions.log")
}

fn append_action_log(line: &str) {
    let path = actions_log_path();
    if let Err(e) = std::fs::OpenOptions::new().create(true).append(true).open(&path).and_then(|mut f| {
        use std::io::Write;
        writeln!(f, "{}", line)
    }) {
        warn!("Failed to append to actions log {:?}: {:?}", path, e);
    }
}

/// Public helper to append to the actions log from other modules
pub fn append_to_actions_log(line: &str) {
    append_action_log(line);
}

/// Execute an action (may be no-op if feature is not enabled)
pub fn execute_action(action: &Action) {
    match action {
        Action::NoteOn { channel, note, velocity } => {
            let ch = channel.unwrap_or(0);
            let vel = velocity.unwrap_or(127);
            match crate::midi::send_note_on(ch, *note, vel) {
                Ok(()) => {
                    info!("Note On: ch={} note={} vel={}", ch, note, vel);
                    append_action_log(&format!("NoteOn ch={} note={} vel={}", ch, note, vel));
                }
                Err(e) => warn!("Failed to send note on: {}", e),
            }
        }
        Action::NoteOff { channel, note, velocity } => {
            let ch = channel.unwrap_or(0);
            let vel = velocity.unwrap_or(0);
            match crate::midi::send_note_off(ch, *note, vel) {
                Ok(()) => {
                    info!("Note Off: ch={} note={} vel={}", ch, note, vel);
                    append_action_log(&format!("NoteOff ch={} note={} vel={}", ch, note, vel));
                }
                Err(e) => warn!("Failed to send note off: {}", e),
            }
        }
        Action::ControlChange { channel, controller, value } => {
            let ch = channel.unwrap_or(0);
            match crate::midi::send_control_change(ch, *controller, *value) {
                Ok(()) => {
                    info!("Control Change: ch={} ctrl={} val={}", ch, controller, value);
                    append_action_log(&format!("ControlChange ch={} ctrl={} val={}", ch, controller, value));
                }
                Err(e) => warn!("Failed to send control change: {}", e),
            }
        }
        Action::ProgramChange { channel, program } => {
            let ch = channel.unwrap_or(0);
            match crate::midi::send_program_change(ch, *program) {
                Ok(()) => {
                    info!("Program Change: ch={} prog={}", ch, program);
                    append_action_log(&format!("ProgramChange ch={} prog={}", ch, program));
                }
                Err(e) => warn!("Failed to send program change: {}", e),
            }
        }
        Action::ControlChangeFromAxis { .. } => {
            // Axis-driven control changes are handled separately by execute_axis_action
            warn!("ControlChangeFromAxis received in execute_action; axis events should call execute_axis_action instead");
        }
    }
}

/// Execute an action that depends on an axis value (value in -1.0..1.0)
pub fn execute_axis_action(action: &Action, value: f32) {
    match action {
        Action::ControlChangeFromAxis { channel, controller, invert } => {
            let ch = channel.unwrap_or(0);
            let inv = invert.unwrap_or(false);
            let mut v = value.clamp(-1.0, 1.0);
            if inv { v = -v; }
            // Map -1.0..1.0 -> 0..127
            let cc = (((v + 1.0) / 2.0) * 127.0).round().clamp(0.0, 127.0) as u8;
            match crate::midi::send_control_change(ch, *controller, cc) {
                Ok(()) => {
                    info!("Axis Control Change: ch={} ctrl={} val={} (raw={})", ch, controller, cc, value);
                    append_action_log(&format!("AxisControlChange ch={} ctrl={} val={} raw={}", ch, controller, cc, value));
                    // Update terminal midi status line (bottom)
                    crate::term_status::set_midi_line(&format!("MIDI: bytes [{}, {}, {}] (ch={} ctrl={} val={})", 0xB0u8 | (ch & 0x0f), controller, cc, ch, controller, cc));
                }
                Err(e) => warn!("Failed to send axis control change: {}", e),
            }
        }
        _ => {
            // Not an axis-driven action; ignore or log
            warn!("execute_axis_action called for non-axis action: {:?}", action);
        }
    }
}
