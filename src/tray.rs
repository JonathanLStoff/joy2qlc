#![cfg(all(target_os = "macos", feature = "tray"))]

use log::{error, info, warn};
use std::env;
use std::path::PathBuf;
use std::process::Command;

pub fn start_tray(cfg_path: &str) -> Result<(), tray_item::TIError> {
    // tray-item creates a system tray/menu bar item on macOS
    use tray_item::TrayItem;

    let mut tray = TrayItem::new("joy2qlc", "")?;

    // Open config in VSCode (or fallback)
    let cfg_path_owned = cfg_path.to_string();
    tray.add_menu_item("Open Config", move || {
        info!("Opening config: {}", cfg_path_owned);
        if Command::new("code").arg(&cfg_path_owned).spawn().is_err() {
            let _ = Command::new("open").arg("-a").arg("Visual Studio Code").arg(&cfg_path_owned).spawn();
        }
    })?;

    // Restart the app (spawn new instance and exit)
    tray.add_menu_item("Restart", || {
        info!("Restart requested from tray");
        match env::current_exe() {
            Ok(exe) => {
                if Command::new(exe).spawn().is_ok() {
                    std::process::exit(0);
                } else {
                    warn!("Failed to spawn new process on restart");
                }
            }
            Err(e) => error!("Failed to locate current exe: {:?}", e),
        }
    })?;

    // Toggle open at login using LaunchAgents
    let plist_name = format!("com.{}.joy2qlc.plist", env::var("USER").unwrap_or_else(|_| "joy2qlc".into()));
    let home = env::var("HOME").unwrap_or_else(|_| String::from("/Users/Shared"));
    let plist_path = PathBuf::from(format!("{}/Library/LaunchAgents/{}", home, plist_name));
    let plist_path_open = plist_path.clone();
    let plist_path_toggle = plist_path.clone();

    // Display checked state in label by updating the menu item title when toggled
    tray.add_menu_item("Toggle Open at Login", move || {
        if plist_path_toggle.exists() {
            if let Err(e) = std::fs::remove_file(&plist_path_toggle) {
                warn!("Failed to remove LaunchAgent: {:?}", e);
            } else {
                info!("LaunchAgent removed. Open at login disabled.");
            }
        } else {
            if let Ok(exe) = env::current_exe() {
                if let Some(parent) = plist_path_open.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                let plist = format!(r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>com.joy2qlc.agent</string>
  <key>ProgramArguments</key>
  <array>
    <string>{}</string>
  </array>
  <key>RunAtLoad</key>
  <true/>
  <key>KeepAlive</key>
  <false/>
</dict>
</plist>"#, exe.display());
                if let Err(e) = std::fs::write(&plist_path_open, plist) {
                    warn!("Failed to write LaunchAgent plist: {:?}", e);
                } else {
                    info!("LaunchAgent created: {}", plist_path_open.display());
                }
            } else {
                warn!("Cannot register launch agent: failed to locate exe");
            }
        }
    })?;

    // Quit
    tray.add_menu_item("Quit", || {
        info!("Quit requested from tray");
        std::process::exit(0);
    })?;

    Ok(())
}
