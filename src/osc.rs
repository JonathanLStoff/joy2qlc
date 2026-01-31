use once_cell::sync::Lazy;
use rosc::{encoder, OscMessage, OscPacket, OscType};
use std::net::UdpSocket;
use std::sync::Mutex;
use rosc::decoder;

static SOCKET: Lazy<Mutex<Option<UdpSocket>>> = Lazy::new(|| Mutex::new(None));
static DEST: Lazy<Mutex<String>> = Lazy::new(|| Mutex::new(String::from("127.0.0.1:9000")));

/// Initialize the OSC sender. `dest` is the destination address like "127.0.0.1:9000".
pub fn init(dest: &str) -> Result<(), String> {
    let sock = UdpSocket::bind("0.0.0.0:0").map_err(|e| e.to_string())?;
    sock.set_nonblocking(false).map_err(|e| e.to_string())?;
    let mut s = SOCKET.lock().unwrap();
    *s = Some(sock);
    let mut d = DEST.lock().unwrap();
    *d = dest.to_string();
    Ok(())
}

fn send_packet(msg: OscMessage) -> Result<(), String> {
    let packet = OscPacket::Message(msg.clone());
    let buf = encoder::encode(&packet).map_err(|e| e.to_string())?;
    let dest = DEST.lock().unwrap().clone();
    let s_lock = SOCKET.lock().unwrap();
    if let Some(sock) = s_lock.as_ref() {
        log::debug!("Sending OSC packet to {}: addr={}, args={:?}", dest, msg.addr, msg.args);
        sock.send_to(&buf, &dest).map_err(|e| e.to_string())?;
        log::debug!("OSC packet sent successfully");
        Ok(())
    } else {
        Err("OSC socket not initialized".to_string())
    }
}

/// Send a control change-like OSC message. Address: `/cc/<controller>` with integer arg 0..127
pub fn send_control_change(_channel: u8, controller: u8, value: i32) -> Result<(), String> {
    let addr = format!("/cc/{}", controller);
    // Some OSC receivers (like QLC+) prefer float args for signed ranges.
    // Send negative values as `Float` so negative ranges are interpreted correctly,
    // otherwise send as `Int` for small non-negative CC values.
    let arg = if value < 0 {
        OscType::Float(value as f32)
    } else {
        OscType::Int(value)
    };
    let msg = OscMessage { addr, args: vec![arg] };
    send_packet(msg)
}

pub fn send_note_on(_channel: u8, note: u8, velocity: u8) -> Result<(), String> {
    let addr = format!("/note/{}/on", note);
    let msg = OscMessage { addr, args: vec![OscType::Int(velocity as i32)] };
    send_packet(msg)
}

pub fn send_note_off(_channel: u8, note: u8, velocity: u8) -> Result<(), String> {
    let addr = format!("/note/{}/off", note);
    let msg = OscMessage { addr, args: vec![OscType::Int(velocity as i32)] };
    send_packet(msg)
}

pub fn send_program_change(_channel: u8, program: u8) -> Result<(), String> {
    let addr = format!("/program/{}", program);
    let msg = OscMessage { addr, args: vec![OscType::Int(program as i32)] };
    send_packet(msg)
}

/// Send control change with a floating-point argument (preserves sign/precision)
pub fn send_control_change_float(_channel: u8, controller: u8, value: f32) -> Result<(), String> {
    let addr = format!("/cc/{}", controller);
    let arg = OscType::Float(value);
    let msg = OscMessage { addr, args: vec![arg] };
    send_packet(msg)
}

/// Send control change with an explicit OSC Nil argument (type N)
pub fn send_control_change_nil(_channel: u8, controller: u8) -> Result<(), String> {
    let addr = format!("/cc/{}", controller);
    let arg = OscType::Nil;
    let msg = OscMessage { addr, args: vec![arg] };
    send_packet(msg)
}

/// Helper: return the current destination string
pub fn destination() -> String {
    DEST.lock().unwrap().clone()
}

/// Start a background OSC listener bound to `bind_addr` (e.g. "0.0.0.0:9002").
/// Spawns a thread that receives incoming OSC packets and updates the terminal UI.
pub fn start_listener(bind_addr: &str) -> Result<(), String> {
    let bind = bind_addr.to_string();
    let b = bind.clone();
    std::thread::spawn(move || {
        match UdpSocket::bind(&b) {
            Ok(sock) => {
                // Buffer for incoming datagrams
                let mut buf = [0u8; 2048];
                log::info!("OSC listener bound to {}", b);
                loop {
                    match sock.recv_from(&mut buf) {
                        Ok((amt, src)) => {
                            let packet_buf = &buf[..amt];
                            match decoder::decode(packet_buf) {
                                Ok(packet) => {
                                    match packet {
                                        OscPacket::Message(msg) => {
                                            let mut args = Vec::new();
                                            for v in msg.args.into_iter() {
                                                args.push(format!("{:?}", v));
                                            }
                                            let s = format!("IN {} [{}] from {}", msg.addr, args.join(", "), src);
                                            crate::term_status::set_osc_in_line(&s);
                                            log::debug!("Received OSC: {}", s);
                                        }
                                        OscPacket::Bundle(bundle) => {
                                            let s = format!("IN bundle @ {:?} ({} msgs) from {}", bundle.timetag, bundle.content.len(), src);
                                            crate::term_status::set_osc_in_line(&s);
                                            log::debug!("Received OSC bundle: {}", s);
                                        }
                                    }
                                }
                                Err(e) => {
                                    log::warn!("Failed to decode OSC packet from {}: {:?}", src, e);
                                }
                            }
                        }
                        Err(e) => {
                            log::warn!("OSC listener recv error: {}", e);
                            // Sleep briefly to avoid busy loop on repeated errors
                            std::thread::sleep(std::time::Duration::from_millis(100));
                        }
                    }
                }
            }
            Err(e) => {
                log::warn!("Failed to bind OSC listener {}: {}", bind, e);
            }
        }
    });
    Ok(())
}
