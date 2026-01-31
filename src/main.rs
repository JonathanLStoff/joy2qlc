use log::{info, warn};
use std::sync::{Arc, RwLock};
mod config;
mod midi;
mod osc;
mod term_status;

#[cfg(all(target_os = "macos", feature = "tray"))]
mod tray;

fn main() {
    // Initialize logger; default to warn to keep the terminal status UI clean.
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();
    info!("joy2qlc starting");

    // Load config (mappings) and start a watcher so mappings can change without restart.
    let cfg_path = "mappings.toml";
    let cfg = match config::Config::load_from(cfg_path) {
        Ok(c) => Arc::new(RwLock::new(c)),
        Err(e) => {
            warn!("Failed to load {}: {:?}. Starting with empty config.", cfg_path, e);
            Arc::new(RwLock::new(config::Config { mappings: vec![] }))
        }
    };

    if let Err(e) = config::Config::watch(cfg_path, cfg.clone()) {
        warn!("Failed to start config watcher: {:?}", e);
    }

    // Initialize OSC subsystem (we send OSC to QLC+ on localhost by default)
    {
        if let Err(e) = osc::init("127.0.0.1:7702") {
            warn!("OSC init failed: {}", e);
        } else {
            warn!("OSC destination: {}", osc::destination());
            // Test send a message to verify OSC works
            if let Err(e) = osc::send_control_change(0, 0, 42) {
                warn!("Test OSC send failed: {}", e);
            } else {
                info!("Test OSC message sent to /cc/0 with value 42");
            }
            // Start OSC listener for incoming messages on port 9002
            if let Err(e) = osc::start_listener("0.0.0.0:9002") {
                warn!("OSC listener failed to start: {}", e);
            } else {
                warn!("OSC listener bound on 0.0.0.0:9002");
            }
        }
    }

    // High-level structure: there are feature-gated modules below that
    // demonstrate how to read joystick events and either simulate keypresses
    // or send REST calls. Enable the appropriate features in Cargo.toml.

    if cfg!(all(target_os = "macos", feature = "tray")) {
        // If tray is in use, run the tray event loop on the main thread.
        // Spawn joystick on a background thread so it doesn't block the event loop.
        #[cfg(feature = "joystick")]
        {
            let cfg_for_joy = cfg.clone();
            std::thread::spawn(move || {
                info!("Joystick feature enabled — scanning for controllers...");
                joystick::run(cfg_for_joy);
            });
        }
        #[cfg(not(feature = "joystick"))]
        {
            warn!("No joystick feature enabled. Build with `--features joystick` to enable input.");
        }

        if let Err(e) = tray::start_tray(cfg_path) {
            warn!("Tray failed to start: {:?}", e);
        }
    } else {
        #[cfg(feature = "joystick")]
        {
            info!("Joystick feature enabled — scanning for controllers...");
            joystick::run(cfg.clone());
        }

        #[cfg(not(feature = "joystick"))]
        {
            warn!("No joystick feature enabled. Build with `--features joystick` to enable input.");
        }
    }
}

#[cfg(feature = "joystick")]
mod joystick {
    use log::info;
    use std::sync::{Arc, RwLock};

    pub fn run(cfg: Arc<RwLock<crate::config::Config>>) {
        info!("Joystick module starting...");
        // Minimal example using gilrs. This code is compiled only if the
        // "joystick" feature is enabled and `gilrs` is available in Cargo.toml.

        use gilrs::{Gilrs, Event, EventType};
        use std::collections::HashSet;

        let mut pressed: HashSet<String> = HashSet::new();

        let mut gilrs = match Gilrs::new() {
            Ok(g) => g,
            Err(e) => {
                eprintln!("Failed to initialize gilrs: {:?}", e);
                return;
            }
        };

        info!("Waiting for events (press joystick buttons/move axes)...");

        loop {
            while let Some(Event { id, event, time }) = gilrs.next_event() {
                match event {
                    EventType::ButtonPressed(button, code) => {
                        let btn = format!("{:?}", button);
                        // Update status UI (middle line)
                        crate::term_status::set_button_line(&format!("Joystick button pressed: {:?} (Code: {:?})", button, code));
                        info!("Gamepad {:?}: button pressed: {:?} (Code: {:?})", id, button, code);
                        // Track pressed buttons for gating axis-driven actions
                        pressed.insert(btn.clone());

                        // Also trigger any mapped button action (if present)
                        if let Some(action) = cfg.read().unwrap().find_action_for_button(&btn) {
                            info!("Mapped action: {:?}", action);
                            crate::config::execute_action(&action);
                        }

                        // Additionally, when a button is pressed, send any configured `init` messages for
                        // axis mappings that are gated by this button. The spec in `mappings.toml` places
                        // an `init = { ... }` table next to a mapping; here we send the `count` value if
                        // present, otherwise the `value` field.
                        let cfg_read = cfg.read().unwrap();
                        for m in &cfg_read.mappings {
                            if m.input.typ == "axis" {
                                if let Some(req) = &m.input.while_button {
                                    if req == &btn {
                                        if let Some(init) = &m.init {
                                            // Only handle control-change-like init types for now
                                            if init.typ == "control_change_from_axis" || init.typ == "control_change" {
                                                if let Some(ctrl) = init.controller {
                                                    let ch = init.channel.unwrap_or(0);
                                                    // Build the sequence of values to send
                                                    let mut values: Vec<i32> = Vec::new();
                                                    if let Some(iv) = &init.value {
                                                        match iv {
                                                            crate::config::InitValue::Int(x) => values.push(*x),
                                                            crate::config::InitValue::IntList(vs) => values.extend_from_slice(&vs),
                                                        }
                                                    }
                                                    let count = init.count.map(|c| c as usize).unwrap_or(values.len());
                                                    let delay = init.delay_ms.unwrap_or(0);
                                                    if values.is_empty() {
                                                        info!("Init spec for axis {} has no value to send", m.input.code);
                                                    } else {
                                                        // Clone for moving into thread
                                                        let vals_to_send = values.into_iter().take(count).collect::<Vec<i32>>();
                                                        let code_clone = m.input.code.clone();
                                                        std::thread::spawn(move || {
                                                            for v in vals_to_send {
                                                                let _ = crate::osc::send_control_change(ch, ctrl, v);
                                                                crate::config::append_to_actions_log(&format!("InitControlChange axis={} ch={} ctrl={} val={}", code_clone, ch, ctrl, v));
                                                                crate::term_status::set_osc_out_line(&format!("Init: sent /cc/{} {}", ctrl, v));
                                                                if delay > 0 {
                                                                    std::thread::sleep(std::time::Duration::from_millis(delay));
                                                                }
                                                            }
                                                        });
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    EventType::ButtonReleased(button, code) => {
                        let btn = format!("{:?}", button);
                        // Update status UI (middle line) for release
                        crate::term_status::set_button_line(&format!("Joystick button released: {:?} (Code: {:?})", button, code));
                        info!("Gamepad {:?}: button released: {:?} (Code: {:?})", id, button, code);
                        // Remove from pressed set — stopping axis-driven updates when button released
                        pressed.remove(&btn);

                        // Send a single center (middle) CC for any axis mappings gated by this button
                        // so the controlled parameter returns to centre instead of drifting.
                        // Use `release_value` from the mapping if provided, otherwise default to 64.
                        let cfg_read = cfg.read().unwrap();
                        for m in &cfg_read.mappings {
                            if m.input.typ == "axis" {
                                if let Some(req) = &m.input.while_button {
                                    if req == &btn {
                                        match &m.action {
                                            crate::config::Action::ControlChangeFromAxis { channel, controller, .. } => {
                                                let ch = channel.unwrap_or(0);
                                                // Update terminal bottom line and then send via OSC according to configured release_value
                                                    match &m.input.release_value {
                                                        Some(crate::config::ReleaseValue::Int(v)) => {
                                                            crate::term_status::set_osc_out_line(&format!("South release: sending OSC {} {}", format!("/cc/{}", controller), v));
                                                            info!("South release: sending OSC /cc/{} val={} (ch={})", controller, v, ch);
                                                            let _ = crate::osc::send_control_change(ch, *controller, *v);
                                                            info!("Sent center OSC CC for axis {}: ch={} ctrl={} val={}", m.input.code, ch, controller, v);
                                                            crate::config::append_to_actions_log(&format!("CenterControlChange axis={} ch={} ctrl={} val={}", m.input.code, ch, controller, v));
                                                        }
                                                        Some(crate::config::ReleaseValue::Float(f)) => {
                                                            crate::term_status::set_osc_out_line(&format!("South release: sending OSC {} {}", format!("/cc/{}", controller), f));
                                                            info!("South release: sending OSC /cc/{} float={} (ch={})", controller, f, ch);
                                                            let _ = crate::osc::send_control_change_float(ch, *controller, *f);
                                                            info!("Sent center OSC FLOAT for axis {}: ch={} ctrl={} val={}", m.input.code, ch, controller, f);
                                                            crate::config::append_to_actions_log(&format!("CenterControlChangeFloat axis={} ch={} ctrl={} val={}", m.input.code, ch, controller, f));
                                                        }
                                                        Some(crate::config::ReleaseValue::SendNil) => {
                                                            crate::term_status::set_osc_out_line(&format!("South release: sending OSC {} NIL", format!("/cc/{}", controller)));
                                                            info!("South release: sending OSC /cc/{} NIL (ch={})", controller, ch);
                                                            let _ = crate::osc::send_control_change_nil(ch, *controller);
                                                            info!("Sent center OSC NIL for axis {}: ch={} ctrl={}", m.input.code, ch, controller);
                                                            crate::config::append_to_actions_log(&format!("CenterControlChangeNil axis={} ch={} ctrl={}", m.input.code, ch, controller));
                                                        }
                                                        None => {
                                                            info!("Release value for axis {} is unset — not sending center OSC", m.input.code);
                                                        }
                                                    }
                                            }
                                            _ => {}
                                        }
                                    }
                                }
                            }
                        }
                    }
                    EventType::AxisChanged(axis, value, code) => {
                        // Update top status line
                        crate::term_status::set_axis_line(&format!("Joystick axis changed: {:?} Value: {} (Code: {:?})", axis, value, code));
                        // Dead zone to avoid triggering on small noisy movements
                        let dead_zone = 0.1_f32;
                        if value.abs() <= dead_zone {
                            // Ignore small noisy movements
                            continue;
                        }
                        let axis_name = format!("{:?}", axis);
                        // Retrieve all mappings for this axis and execute each one whose
                        // gating button (if any) is currently pressed.
                        let cfg_read = cfg.read().unwrap();
                        let mappings = cfg_read.find_mappings_for_axis(&axis_name);
                        for mapping in mappings {
                            if let Some(req) = &mapping.input.while_button {
                                if !pressed.contains(req) {
                                    // gating button not held for this mapping
                                    continue;
                                }
                            }
                            info!("Mapped axis action: {:?}", mapping.action);
                            crate::config::Config::execute_axis_mapping(&mapping, value);
                        }
                    }
                    _ => {}
                }
            }
            // Simple sleep to avoid busy loop
            std::thread::sleep(std::time::Duration::from_millis(16));
        }
    }
}

#[cfg(feature = "simulate-keys")]
mod keys {
    use log::warn;
    use enigo::{Enigo, Key, KeyboardControllable};

    pub fn send_space() {
        let mut enigo = Enigo::new();
        enigo.key_click(Key::Space);
    }

    pub fn send_key(name: &str) {
        let mut enigo = Enigo::new();
        match name {
            "Space" => enigo.key_click(Key::Space),
            "Enter" | "Return" => enigo.key_click(Key::Return),
            "Up" => enigo.key_click(Key::UpArrow),
            "Down" => enigo.key_click(Key::DownArrow),
            "Left" => enigo.key_click(Key::LeftArrow),
            "Right" => enigo.key_click(Key::RightArrow),
            s if s.len() == 1 => {
                let c = s.chars().next().unwrap();
                enigo.key_click(Key::Layout(c));
            }
            other => warn!("Unknown key requested: {}", other),
        }
    }
}

#[cfg(not(feature = "simulate-keys"))]
mod keys {
    // Provide no-op send_key when feature disabled (so callers don't need cfg everywhere)
    pub fn send_space() {}
    pub fn send_key(_: &str) {}
}

#[cfg(feature = "rest-client")]
mod rest_client {
    use reqwest::blocking::Client;
    use serde::Serialize;

    #[derive(Serialize)]
    struct EventPayload<'a> {
        source: &'a str,
        name: &'a str,
    }

    pub fn post_event(url: &str, name: &str) -> Result<(), reqwest::Error> {
        let client = Client::new();
        let payload = EventPayload { source: "joystick", name };
        client.post(url).json(&payload).send()?.error_for_status()?;
        Ok(())
    }
}

#[cfg(feature = "rest-server")]
mod rest_server {
    use warp::Filter;

    pub async fn run_server(addr: &str) {
        let routes = warp::post()
            .and(warp::path("event"))
            .and(warp::body::json())
            .map(|body: serde_json::Value| {
                println!("Received event: {:?}", body);
                warp::reply::json(&serde_json::json!({"status":"ok"}))
            });

        warp::serve(routes).run(addr.parse().unwrap()).await;
    }
}
