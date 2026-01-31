use log::{error, info, warn};
use serde::Deserialize;
use serde::de::{Deserializer, Error as DeError};
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
    #[serde(default)]
    pub init: Option<InitSpec>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct InitSpec {
    #[serde(rename = "type")]
    pub typ: String,
    pub channel: Option<u8>,
    pub controller: Option<u8>,
    #[serde(default)]
    pub value: Option<InitValue>,
    #[serde(default)]
    pub count: Option<u8>,
    #[serde(default)]
    pub delay_ms: Option<u64>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(untagged)]
pub enum InitValue {
    Int(i32),
    IntList(Vec<i32>),
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
    #[serde(default, deserialize_with = "deserialize_release_value")]
    pub release_value: Option<ReleaseValue>,

    /// Optional input range for axis mappings, e.g. [1, -1] or [-1, 1]
    #[serde(default, alias = "range")]
    pub input_range: Option<Vec<f32>>,

    /// Optional output range for axis mappings, e.g. [0, 127] or [-50, 50]
    #[serde(default, alias = "range")]
    pub output_range: Option<Vec<i32>>,
}

#[derive(Debug, Clone)]
pub enum ReleaseValue {
    Int(i32),
    Float(f32),
    SendNil,
}

fn deserialize_release_value<'de, D>(deserializer: D) -> Result<Option<ReleaseValue>, D::Error>
where
    D: Deserializer<'de>,
{
    struct RVVisitor;

    impl<'de> serde::de::Visitor<'de> for RVVisitor {
        type Value = Option<ReleaseValue>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            write!(formatter, "an integer, the string 'null' to omit, the string 'None' to send OSC Nil, or nothing")
        }

        fn visit_i64<E>(self, v: i64) -> Result<Self::Value, E>
        where
            E: DeError,
        {
            Ok(Some(ReleaseValue::Int(v as i32)))
        }

        fn visit_u64<E>(self, v: u64) -> Result<Self::Value, E>
        where
            E: DeError,
        {
            Ok(Some(ReleaseValue::Int(v as i32)))
        }

        fn visit_f64<E>(self, v: f64) -> Result<Self::Value, E>
        where
            E: DeError,
        {
            Ok(Some(ReleaseValue::Float(v as f32)))
        }

        fn visit_str<E>(self, s: &str) -> Result<Self::Value, E>
        where
            E: DeError,
        {
            if s.eq_ignore_ascii_case("null") || s.is_empty() {
                Ok(None)
            } else if s.eq_ignore_ascii_case("none") {
                Ok(Some(ReleaseValue::SendNil))
            } else {
                s.parse::<i32>().map(|i| Some(ReleaseValue::Int(i))).map_err(|_| DeError::custom("invalid string for release_value"))
            }
        }

        fn visit_string<E>(self, s: String) -> Result<Self::Value, E>
        where
            E: DeError,
        {
            self.visit_str(&s)
        }

        fn visit_none<E>(self) -> Result<Self::Value, E>
        where
            E: DeError,
        {
            Ok(None)
        }

        fn visit_unit<E>(self) -> Result<Self::Value, E>
        where
            E: DeError,
        {
            Ok(None)
        }
    }

    deserializer.deserialize_any(RVVisitor)
}

#[derive(Debug, Clone)]
pub enum ActionRange {
    Range(Vec<f32>),
    Passthrough,
}

fn deserialize_action_range<'de, D>(deserializer: D) -> Result<Option<ActionRange>, D::Error>
where
    D: Deserializer<'de>,
{
    struct ARVisitor;

    impl<'de> serde::de::Visitor<'de> for ARVisitor {
        type Value = Option<ActionRange>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            write!(formatter, "an integer array like [0,127], the string 'null', or nothing")
        }

        fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
        where
            A: serde::de::SeqAccess<'de>,
        {
            let mut vals = Vec::new();
            while let Some(v) = seq.next_element::<f64>()? {
                vals.push(v as f32);
            }
            Ok(Some(ActionRange::Range(vals)))
        }

        fn visit_str<E>(self, s: &str) -> Result<Self::Value, E>
        where
            E: DeError,
        {
            if s.eq_ignore_ascii_case("null") || s.is_empty() {
                Ok(Some(ActionRange::Passthrough))
            } else {
                Err(DeError::custom("invalid string for action.range; expected 'null'"))
            }
        }

        fn visit_none<E>(self) -> Result<Self::Value, E>
        where
            E: DeError,
        {
            Ok(None)
        }

        fn visit_unit<E>(self) -> Result<Self::Value, E>
        where
            E: DeError,
        {
            Ok(None)
        }
    }

    deserializer.deserialize_any(ARVisitor)
}

// Accept either an integer or the literal string "null" (or missing) to indicate no release value.
fn deserialize_nullable_i32<'de, D>(deserializer: D) -> Result<Option<i32>, D::Error>
where
    D: Deserializer<'de>,
{
    struct OptVisitor;

    impl<'de> serde::de::Visitor<'de> for OptVisitor {
        type Value = Option<i32>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            write!(formatter, "an integer, the string 'null', or nothing")
        }

        fn visit_i64<E>(self, v: i64) -> Result<Self::Value, E>
        where
            E: DeError,
        {
            Ok(Some(v as i32))
        }

        fn visit_u64<E>(self, v: u64) -> Result<Self::Value, E>
        where
            E: DeError,
        {
            Ok(Some(v as i32))
        }

        fn visit_str<E>(self, s: &str) -> Result<Self::Value, E>
        where
            E: DeError,
        {
            if s.eq_ignore_ascii_case("null") || s.is_empty() {
                Ok(None)
            } else {
                s.parse::<i32>().map(Some).map_err(|_| DeError::custom("invalid string for release_value"))
            }
        }

        fn visit_string<E>(self, s: String) -> Result<Self::Value, E>
        where
            E: DeError,
        {
            self.visit_str(&s)
        }

        fn visit_none<E>(self) -> Result<Self::Value, E>
        where
            E: DeError,
        {
            Ok(None)
        }

        fn visit_unit<E>(self) -> Result<Self::Value, E>
        where
            E: DeError,
        {
            Ok(None)
        }
    }

    deserializer.deserialize_any(OptVisitor)
}

#[derive(Debug, Deserialize, Clone)]
#[serde(tag = "type")]
pub enum Action {
    #[serde(rename = "note_on")]
    NoteOn { channel: Option<u8>, note: u8, velocity: Option<u8>, #[serde(default)] count: Option<u8>, #[serde(default)] delay_ms: Option<u64> },

    #[serde(rename = "note_off")]
    NoteOff { channel: Option<u8>, note: u8, velocity: Option<u8>, #[serde(default)] count: Option<u8>, #[serde(default)] delay_ms: Option<u64> },

    #[serde(rename = "control_change")]
    ControlChange { channel: Option<u8>, controller: u8, value: u8, #[serde(default)] count: Option<u8>, #[serde(default)] delay_ms: Option<u64> },

    #[serde(rename = "program_change")]
    ProgramChange { channel: Option<u8>, program: u8, #[serde(default)] count: Option<u8>, #[serde(default)] delay_ms: Option<u64> },

    /// Dynamic control change driven by axis value (-1.0..1.0 mapped to 0..127)
    #[serde(rename = "control_change_from_axis")]
    ControlChangeFromAxis { channel: Option<u8>, controller: u8, invert: Option<bool>, #[serde(default, deserialize_with = "deserialize_action_range", alias = "range")] range: Option<ActionRange>, #[serde(default)] count: Option<u8>, #[serde(default)] delay_ms: Option<u64> },
}

impl Config {
    pub fn load_from(path: &str) -> Result<Self, anyhow::Error> {
        let s = fs::read_to_string(path)?;
        let cfg: Config = toml::from_str(&s)?;
        // Warn if any axis mapping has release_value set to 0 which is likely unintended
        for m in &cfg.mappings {
            if m.input.typ == "axis" {
                if let Some(crate::config::ReleaseValue::Int(rv)) = m.input.release_value {
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

    /// Return all mappings for an axis (useful when multiple mappings
    /// exist for the same physical axis gated by different buttons).
    pub fn find_mappings_for_axis(&self, axis_name: &str) -> Vec<Mapping> {
        let mut out = Vec::new();
        for m in &self.mappings {
            if m.input.typ == "axis" && m.input.code == axis_name {
                out.push(m.clone());
            }
        }
        out
    }

    /// Execute an axis mapping given the full `Mapping` (so input/output ranges
    /// and release_value can be consulted). `value` is the raw axis value in -1.0..1.0
    pub fn execute_axis_mapping(mapping: &Mapping, value: f32) {
        use crate::osc;
            if let Action::ControlChangeFromAxis { channel, controller, invert, range, .. } = &mapping.action {
            let ch = channel.unwrap_or(0);
            let inv = invert.unwrap_or(false);

            // If action range explicitly requests passthrough, send the raw float value
            if let Some(ActionRange::Passthrough) = range {
                let mut v = value.clamp(-1.0, 1.0);
                if inv { v = -v; }
                match crate::osc::send_control_change_float(ch, *controller, v) {
                    Ok(()) => {
                        info!("OSC Axis Passthrough: ch={} ctrl={} val={} (raw={})", ch, controller, v, value);
                        append_action_log(&format!("AxisPassthrough ch={} ctrl={} val={} raw={}", ch, controller, v, value));
                        crate::term_status::set_osc_out_line(&format!("OSC: {} {}", format!("/cc/{}", controller), v));
                    }
                    Err(e) => warn!("Failed to send passthrough axis control change via OSC: {}", e),
                }
                return;
            }

            // Determine input range
            let (in_min, in_max) = if let Some(r) = &mapping.input.input_range {
                if r.len() == 2 {
                    (r[0], r[1])
                } else {
                    (-1.0_f32, 1.0_f32)
                }
            } else {
                (-1.0_f32, 1.0_f32)
            };

            // Determine output range as floats: prefer `action.range` (float) if present, otherwise `input.output_range` (ints converted)
            let (out_min, out_max) = {
                // First, try action-provided range
                let action_range: Option<(f32, f32)> = match &mapping.action {
                    Action::ControlChangeFromAxis { range, .. } => {
                        if let Some(ar) = range {
                            match ar {
                                ActionRange::Range(rvec) => {
                                    if rvec.len() == 2 {
                                        Some((rvec[0], rvec[1]))
                                    } else { None }
                                }
                                ActionRange::Passthrough => None,
                            }
                        } else { None }
                    }
                    _ => None,
                };

                if let Some((a, b)) = action_range {
                    (a, b)
                } else if let Some(r) = &mapping.input.output_range {
                    if r.len() == 2 {
                        (r[0] as f32, r[1] as f32)
                    } else {
                        (-50.0_f32, 50.0_f32)
                    }
                } else {
                    (-50.0_f32, 50.0_f32)
                }
            };

            // Apply invert if requested
            let mut v = value.clamp(-1.0, 1.0);
            if inv { v = -v; }

            // Normalize t in 0..1 according to input range (handle reversed ranges)
            let in_span = in_max - in_min;
            let t = if in_span.abs() < f32::EPSILON {
                0.0_f32
            } else {
                (v - in_min) / in_span
            };

            // Interpolate into output range (float)
            let out_span = out_max - out_min;
            let out_val = out_min + t * out_span;
            // Decide whether to send as float: if the action.range was explicitly provided and fits in -1..1
            let send_as_float = match &mapping.action {
                Action::ControlChangeFromAxis { range, .. } => {
                    if let Some(ActionRange::Range(rvec)) = range {
                        rvec.len() == 2 && rvec[0].abs() <= 1.0 && rvec[1].abs() <= 1.0
                    } else { false }
                }
                _ => false,
            };

            if send_as_float {
                let out_f = out_val.clamp(out_min, out_max);
                match osc::send_control_change_float(ch, *controller, out_f) {
                    Ok(()) => {
                        info!("OSC Axis Control Change (float): ch={} ctrl={} val={} (raw={})", ch, controller, out_f, value);
                        append_action_log(&format!("AxisControlChangeFloat ch={} ctrl={} val={} raw={}", ch, controller, out_f, value));
                        crate::term_status::set_osc_out_line(&format!("OSC: {} {}", format!("/cc/{}", controller), out_f));
                    }
                    Err(e) => warn!("Failed to send axis control change (float) via OSC: {}", e),
                }
            } else {
                let out_i = out_val.round().clamp(out_min, out_max) as i32;
                match osc::send_control_change(ch, *controller, out_i) {
                    Ok(()) => {
                        info!("OSC Axis Control Change: ch={} ctrl={} val={} (raw={})", ch, controller, out_i, value);
                        append_action_log(&format!("AxisControlChange ch={} ctrl={} val={} raw={}", ch, controller, out_i, value));
                        crate::term_status::set_osc_out_line(&format!("OSC: {} {}", format!("/cc/{}", controller), out_i));
                    }
                    Err(e) => warn!("Failed to send axis control change via OSC: {}", e),
                }
            }
        } else {
            warn!("execute_axis_mapping called for non-axis action: {:?}", mapping.action);
        }
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
            Action::NoteOn { channel, note, velocity, count, delay_ms } => {
                let ch = channel.unwrap_or(0);
                let vel = velocity.unwrap_or(127);
                let cnt = count.unwrap_or(1) as usize;
                let delay = delay_ms.unwrap_or(0);
                if cnt <= 1 {
                    match crate::osc::send_note_on(ch, *note, vel) {
                        Ok(()) => {
                            info!("OSC Note On: ch={} note={} vel={}", ch, note, vel);
                            append_action_log(&format!("NoteOn ch={} note={} vel={}", ch, note, vel));
                        }
                        Err(e) => warn!("Failed to send OSC note on: {}", e),
                    }
                } else {
                    let n = *note;
                    let v = vel;
                    std::thread::spawn(move || {
                        for _ in 0..cnt {
                            let _ = crate::osc::send_note_on(ch, n, v);
                            if delay > 0 { std::thread::sleep(std::time::Duration::from_millis(delay)); }
                        }
                    });
                }
            }
        Action::NoteOff { channel, note, velocity, count, delay_ms } => {
            let ch = channel.unwrap_or(0);
            let vel = velocity.unwrap_or(0);
            let cnt = count.unwrap_or(1) as usize;
            let delay = delay_ms.unwrap_or(0);
            if cnt <= 1 {
                match crate::osc::send_note_off(ch, *note, vel) {
                    Ok(()) => {
                        info!("OSC Note Off: ch={} note={} vel={}", ch, note, vel);
                        append_action_log(&format!("NoteOff ch={} note={} vel={}", ch, note, vel));
                    }
                    Err(e) => warn!("Failed to send OSC note off: {}", e),
                }
            } else {
                let n = *note;
                let v = vel;
                std::thread::spawn(move || {
                    for _ in 0..cnt {
                        let _ = crate::osc::send_note_off(ch, n, v);
                        if delay > 0 { std::thread::sleep(std::time::Duration::from_millis(delay)); }
                    }
                });
            }
        }
        Action::ControlChange { channel, controller, value, count, delay_ms } => {
            let ch = channel.unwrap_or(0);
            let v = *value as i32;
            let cnt = count.unwrap_or(1) as usize;
            let delay = delay_ms.unwrap_or(0);
            if cnt <= 1 {
                match crate::osc::send_control_change(ch, *controller, v) {
                    Ok(()) => {
                        info!("OSC Control Change: ch={} ctrl={} val={}", ch, controller, v);
                        append_action_log(&format!("ControlChange ch={} ctrl={} val={}", ch, controller, v));
                    }
                    Err(e) => warn!("Failed to send OSC control change: {}", e),
                }
            } else {
                let ctrl = *controller;
                std::thread::spawn(move || {
                    for _ in 0..cnt {
                        let _ = crate::osc::send_control_change(ch, ctrl, v);
                        if delay > 0 { std::thread::sleep(std::time::Duration::from_millis(delay)); }
                    }
                });
            }
        }
        Action::ProgramChange { channel, program, count, delay_ms } => {
            let ch = channel.unwrap_or(0);
            let cnt = count.unwrap_or(1) as usize;
            let delay = delay_ms.unwrap_or(0);
            if cnt <= 1 {
                match crate::osc::send_program_change(ch, *program) {
                    Ok(()) => {
                        info!("OSC Program Change: ch={} prog={}", ch, program);
                        append_action_log(&format!("ProgramChange ch={} prog={}", ch, program));
                    }
                    Err(e) => warn!("Failed to send OSC program change: {}", e),
                }
            } else {
                let prog = *program;
                std::thread::spawn(move || {
                    for _ in 0..cnt {
                        let _ = crate::osc::send_program_change(ch, prog);
                        if delay > 0 { std::thread::sleep(std::time::Duration::from_millis(delay)); }
                    }
                });
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
        Action::ControlChangeFromAxis { channel, controller, invert, .. } => {
            let ch = channel.unwrap_or(0);
            let inv = invert.unwrap_or(false);
            let mut v = value.clamp(-1.0, 1.0);
            if inv { v = -v; }
            // Map -1.0..1.0 -> 0..127
            let cc = (((v + 1.0) / 2.0) * 127.0).round().clamp(0.0, 127.0) as u8;
            match crate::osc::send_control_change(ch, *controller, cc as i32) {
                Ok(()) => {
                    info!("OSC Axis Control Change: ch={} ctrl={} val={} (raw={})", ch, controller, cc, value);
                    append_action_log(&format!("AxisControlChange ch={} ctrl={} val={} raw={}", ch, controller, cc, value));
                    // Update terminal midi status line (bottom) to show OSC address/args
                    crate::term_status::set_osc_out_line(&format!("OSC: {} {}", format!("/cc/{}", controller), cc));
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
