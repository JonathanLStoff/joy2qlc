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
}

#[derive(Debug, Deserialize, Clone)]
#[serde(tag = "type")]
pub enum Action {
    #[serde(rename = "key")]
    Key { key: String },

    #[serde(rename = "rest")]
    Rest { method: Option<String>, url: String, body: Option<String> },

    #[serde(rename = "exec")]
    Exec { cmd: String, args: Option<Vec<String>> },
}

impl Config {
    pub fn load_from(path: &str) -> Result<Self, anyhow::Error> {
        let s = fs::read_to_string(path)?;
        let cfg: Config = toml::from_str(&s)?;
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
}

/// Execute an action (may be no-op if feature is not enabled)
pub fn execute_action(action: &Action) {
    match action {
        Action::Key { key } => {
            #[cfg(feature = "simulate-keys")]
            {
                crate::keys::send_key(key);
            }
            #[cfg(not(feature = "simulate-keys"))]
            {
                warn!("Key action requested but `simulate-keys` feature is disabled: {}", key);
            }
        }
        Action::Rest { method, url, body } => {
            #[cfg(feature = "rest-client")]
            {
                // For convenience do a POST with JSON payload containing the body or name
                let meth = method.as_deref().unwrap_or("POST");
                match meth {
                    "POST" => {
                        if let Some(b) = body {
                            let client = reqwest::blocking::Client::new();
                            if let Err(e) = client.post(url).body(b.clone()).send() {
                                warn!("REST request failed: {:?}", e);
                            }
                        } else {
                            let client = reqwest::blocking::Client::new();
                            if let Err(e) = client.post(url).send() {
                                warn!("REST request failed: {:?}", e);
                            }
                        }
                    }
                    "GET" => {
                        let client = reqwest::blocking::Client::new();
                        if let Err(e) = client.get(url).send() {
                            warn!("REST request failed: {:?}", e);
                        }
                    }
                    _ => warn!("Unsupported HTTP method in mapping: {:?}", method),
                }
            }
            #[cfg(not(feature = "rest-client"))]
            {
                warn!("REST action requested but `rest-client` feature is disabled: {}", url);
            }
        }
        Action::Exec { cmd, args } => {
            info!("Running command: {} {:?}", cmd, args);
            let mut c = std::process::Command::new(cmd);
            if let Some(a) = args {
                c.args(a);
            }
            match c.spawn() {
                Ok(mut child) => {
                    if let Err(e) = child.wait() {
                        warn!("Command failed: {:?}", e);
                    }
                }
                Err(e) => {
                    warn!("Failed to spawn command {}: {:?}", cmd, e);
                }
            }
        }
    }
}
