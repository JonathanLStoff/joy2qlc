#![cfg(all(target_os = "macos", feature = "tray"))]

use log::warn;
use std::env;
use std::path::PathBuf;
use std::process::Command;

use std::error::Error;

use tray_icon::{TrayIconBuilder, menu::{Menu, MenuItem, CheckMenuItem}};
use winit::event_loop::{ControlFlow, EventLoop};
use image::io::Reader as ImageReader;

fn find_icon_candidates() -> Vec<PathBuf> {
    let exe_parent = std::env::current_exe().ok().and_then(|e| e.parent().map(|p| p.to_path_buf()));
    let mut candidates = vec![];
    if let Some(parent) = exe_parent {
        candidates.push(parent.join("assets/toolbar_icon_template.png"));
        candidates.push(parent.join("assets/toolbar_icon.png"));
        candidates.push(parent.join("assets/toolbar_icon.svg"));
    }
    candidates.push(PathBuf::from("assets/toolbar_icon_template.png"));
    candidates.push(PathBuf::from("assets/toolbar_icon.png"));
    candidates.push(PathBuf::from("assets/toolbar_icon.svg"));
    candidates
}

fn rasterize_svg_to_tmp(svg: &PathBuf) -> Option<PathBuf> {
    let tmp = std::env::temp_dir().join("joy2qlc_toolbar_icon.png");
    let mut converted = false;
    if which::which("rsvg-convert").is_ok() {
        if let Ok(status) = Command::new("rsvg-convert").arg("-o").arg(&tmp).arg(svg).status() {
            converted = status.success();
        }
    } else if which::which("convert").is_ok() {
        if let Ok(status) = Command::new("convert").arg(svg.as_path()).arg("-background").arg("none").arg("-resize").arg("16x16").arg(&tmp).status() {
            converted = status.success();
        }
    }
    if converted && tmp.exists() { Some(tmp) } else { None }
}

fn load_icon() -> Option<tray_icon::icon::Icon> {
    // Find a workable file (prefer PNG), if only SVG present attempt conversion
    let candidates = find_icon_candidates();
    let mut chosen: Option<PathBuf> = None;
    for c in candidates {
        if c.exists() {
            chosen = Some(c);
            break;
        }
    }

    if chosen.is_none() {
        warn!("No toolbar icon found in assets; tray may show a blank icon");
        return None;
    }

    let mut chosen = chosen.unwrap();
    if let Some(ext) = chosen.extension().and_then(|s| s.to_str()) {
        if ext.eq_ignore_ascii_case("svg") {
            if let Some(png) = rasterize_svg_to_tmp(&chosen) {
                chosen = png;
            } else {
                warn!("SVG icon present but could not be rasterized; tray may not display correctly");
            }
        }
    }

    // Read PNG into RGBA bytes
    match ImageReader::open(&chosen) {
        Ok(reader) => match reader.decode() {
            Ok(img) => {
                let rgba = img.to_rgba8();
                let (w, h) = rgba.dimensions();
                let bytes = rgba.into_raw();
                match tray_icon::icon::Icon::from_rgba(bytes, w, h) {
                    Ok(icon) => Some(icon),
                    Err(e) => { warn!("Failed to create Icon from PNG: {:?}", e); None }
                }
            }
            Err(e) => { warn!("Failed to decode image {:?}: {:?}", chosen, e); None }
        },
        Err(e) => { warn!("Failed to open icon {:?}: {:?}", chosen, e); None }
    }
}

pub fn start_tray(cfg_path: &str) -> Result<(), Box<dyn Error>> {
    log::info!("TRAY: start_tray called (cfg_path={})", cfg_path);

    // Prepare some paths we will need in handlers
    let plist_name = format!("com.{}.joy2qlc.plist", env::var("USER").unwrap_or_else(|_| "joy2qlc".into()));
    let home = env::var("HOME").unwrap_or_else(|_| String::from("/Users/Shared"));
    let plist_path = PathBuf::from(format!("{}/Library/LaunchAgents/{}", home, plist_name));
    let plist_path_open = plist_path.clone();
    let plist_path_toggle = plist_path.clone();

    // Build a simple menu
    let menu = Menu::new();
    let open_cfg = MenuItem::new("Open Config", true, None);
    menu.append(&open_cfg).ok();
    let open_log = MenuItem::new("Open Actions Log", true, None);
    menu.append(&open_log).ok();
    let restart_item = MenuItem::new("Restart", true, None);
    menu.append(&restart_item).ok();
    // Use a CheckMenuItem for Toggle Open at Login so it can show a check mark
    let toggle_login = CheckMenuItem::new("Toggle Open at Login", true, plist_path.exists(), None);
    menu.append(&toggle_login).ok();
    let quit_item = MenuItem::new("Quit", true, None);
    menu.append(&quit_item).ok();

    // Capture ids for event matching
    let open_cfg_id = open_cfg.id();
    let open_log_id = open_log.id();
    let restart_id = restart_item.id();
    let toggle_login_id = toggle_login.id();
    let quit_id = quit_item.id();

    // Prepare some paths we will need in handlers
    let cfg_path_owned = cfg_path.to_string();
    let log_path = crate::config::actions_log_path();

    let event_loop = EventLoop::new();
    let icon = load_icon();

    let _tray_icon = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip("joy2qlc")
        .with_icon(icon.unwrap_or_else(|| {
            // Fallback minimal 16x16 transparent icon
            let bytes = vec![0u8; 16 * 16 * 4];
            tray_icon::icon::Icon::from_rgba(bytes, 16, 16).expect("failed to create empty icon")
        }))
        .build()?;

    // NOTE: The `tray-icon` crate exposes several ways to handle menu events depending on
    // the installed version and platform. Implementing full menu callbacks is left as a
    // follow-up; the current implementation ensures the icon/menu are created and the
    // event loop runs on the main thread as required by macOS.

    // Run the event loop on the main thread (required on macOS)
    event_loop.run(move |_event, _, control_flow| {
        *control_flow = ControlFlow::Wait;

        // Poll menu events and dispatch to handlers
        use tray_icon::menu::MenuEvent;
        while let Ok(ev) = MenuEvent::receiver().try_recv() {
            match ev.id() {
                id if id == open_cfg_id => {
                    // Open config in VSCode (or platform fallback)
                    warn!("Menu: Open Config selected");
                    if Command::new("code").arg(&cfg_path_owned).spawn().is_err() {
                        let _ = Command::new("open").arg("-a").arg("Visual Studio Code").arg(&cfg_path_owned).spawn();
                    }
                }
                id if id == open_log_id => {
                    warn!("Menu: Open Actions Log selected");
                    if Command::new("open").arg(log_path.as_path()).spawn().is_err() {
                        warn!("Failed to open actions log: {}", log_path.display());
                    }
                }
                id if id == restart_id => {
                    warn!("Menu: Restart selected");
                    if let Ok(exe) = env::current_exe() {
                        if Command::new(exe).spawn().is_err() {
                            warn!("Failed to spawn new process on restart");
                        } else {
                            std::process::exit(0);
                        }
                    } else {
                        warn!("Failed to locate current exe for restart");
                    }
                }
                id if id == toggle_login_id => {
                    warn!("Menu: Toggle Open at Login selected");
                    if plist_path_toggle.exists() {
                        if let Err(e) = std::fs::remove_file(&plist_path_toggle) {
                            warn!("Failed to remove LaunchAgent: {:?}", e);
                        } else {
                            // Uncheck the menu item
                            toggle_login.set_checked(false);
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
                                // Check the menu item
                                toggle_login.set_checked(true);
                            }
                        } else {
                            warn!("Cannot register launch agent: failed to locate exe");
                        }
                    }
                }
                id if id == quit_id => {
                    warn!("Menu: Quit selected");
                    std::process::exit(0);
                }
                _ => {}
            }
        }
    });
}