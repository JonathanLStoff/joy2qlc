use log::{info, warn};
use std::sync::{Arc, RwLock};
mod config;
mod midi;
mod term_status;

#[cfg(all(target_os = "macos", feature = "tray"))]
mod tray;

fn main() {
    env_logger::init();
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

    // Initialize MIDI subsystem early so virtual ports show up in the system MIDI lists
    #[cfg(feature = "midi")]
    {
        if let Err(e) = midi::init() {
            warn!("MIDI init failed: {}", e);
        }
        // Print available ports so it's easy to verify the virtual bus presence
        let outs = midi::list_output_ports();
        let ins = midi::list_input_ports();
        warn!("MIDI outputs: {:?}", outs);
        warn!("MIDI inputs: {:?}", ins);
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
                                                let val = m.input.release_value.unwrap_or(64);
                                                // Log the raw MIDI bytes that will be sent (status, controller, value)
                                                let status = 0xB0u8 | (ch & 0x0f);
                                                let bytes = [status, *controller, val];
                                                // Update terminal bottom line and then send
                                                crate::term_status::set_midi_line(&format!("South release: sending MIDI bytes [{}, {}, {}] (status=0x{:02X}, ch={}, ctrl={}, val={})", bytes[0], bytes[1], bytes[2], status, ch, controller, val));
                                                info!("South release: sending MIDI bytes {:?} (status=0x{:02X}, ch={}, ctrl={}, val={})", bytes, status, ch, controller, val);
                                                let _ = crate::midi::send_control_change(ch, *controller, val);
                                                info!("Sent center CC for axis {}: ch={} ctrl={} val={}", m.input.code, ch, controller, val);
                                                crate::config::append_to_actions_log(&format!("CenterControlChange axis={} ch={} ctrl={} val={}", m.input.code, ch, controller, val));
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
                        if let Some(mapping) = cfg.read().unwrap().find_mapping_for_axis(&axis_name) {
                            // If the mapping requires a button, only act when it's pressed
                            if let Some(req) = &mapping.input.while_button {
                                if !pressed.contains(req) {
                                    // Not pressed => ignore axis
                                    continue;
                                }
                            }
                            info!("Mapped axis action: {:?}", mapping.action);
                            // Axis-driven actions get the raw value so they can map to CC ranges, etc.
                            // Also update bottom (midi) status line inside execute_axis_action via term_status
                            crate::config::execute_axis_action(&mapping.action, value);
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
