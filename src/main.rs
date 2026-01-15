use log::{info, warn};
use std::sync::{Arc, RwLock};
mod config;

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

    // High-level structure: there are feature-gated modules below that
    // demonstrate how to read joystick events and either simulate keypresses
    // or send REST calls. Enable the appropriate features in Cargo.toml.

    #[cfg(all(target_os = "macos", feature = "tray"))]
    {
        if let Err(e) = tray::start_tray(cfg_path) {
            warn!("Tray failed to start: {:?}", e);
        }
    }

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

#[cfg(feature = "joystick")]
mod joystick {
    use log::info;
    use std::sync::{Arc, RwLock};

    pub fn run(cfg: Arc<RwLock<crate::config::Config>>) {
        info!("Joystick module starting...");
        // Minimal example using gilrs. This code is compiled only if the
        // "joystick" feature is enabled and `gilrs` is available in Cargo.toml.

        use gilrs::{Gilrs, Event, EventType};

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
                    EventType::ButtonPressed(button, _) => {
                        let btn = format!("{:?}", button);
                        info!("Gamepad {:?}: button pressed: {:?}", id, button);
                        if let Some(action) = cfg.read().unwrap().find_action_for_button(&btn) {
                            info!("Mapped action: {:?}", action);
                            crate::config::execute_action(&action);
                        }
                    }
                    EventType::ButtonReleased(button, _) => {
                        info!("Gamepad {:?}: button released: {:?}", id, button);
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
