// MIDI module: uses midir when feature "midi" is enabled, otherwise no-op stubs.
use log::{info, warn};

#[cfg(feature = "midi")]
mod impl_midi {
    use super::*;
    use midir::{MidiOutput, MidiOutputConnection, MidiInput, MidiInputConnection, Ignore};
    use midir::os::unix::{VirtualOutput, VirtualInput};

    use once_cell::sync::Lazy;
    use std::sync::Mutex;

    static OUTPUT_CONNECTION: Lazy<Mutex<Option<MidiOutputConnection>>> = Lazy::new(|| Mutex::new(None));
    static INPUT_CONNECTION: Lazy<Mutex<Option<MidiInputConnection<()>>>> = Lazy::new(|| Mutex::new(None));

    fn ensure_connection() -> Result<(), String> {
        // Ensure an output connection (physical or virtual)
        let mut out_guard = OUTPUT_CONNECTION.lock().unwrap();
        if out_guard.is_none() {
            let midi_out = MidiOutput::new("joy2qlc").map_err(|e| format!("Failed to create MidiOutput: {:?}", e))?;
            let ports = midi_out.ports();
            if ports.is_empty() {
                match midi_out.create_virtual("joy2qlc (virtual out)") {
                    Ok(conn) => {
                        *out_guard = Some(conn);
                        info!("Created virtual MIDI output");
                        warn!("Created virtual MIDI output (visible as 'joy2qlc (virtual out)')");
                    }
                    Err(e) => {
                        warn!("Failed to create virtual MIDI output: {:?}", e);
                    }
                }
            } else {
                let conn = midi_out.connect(&ports[0], "joy2qlc").map_err(|e| format!("Failed to connect to port: {:?}", e))?;
                *out_guard = Some(conn);
                info!("Connected to first MIDI output port");
            }
        }

        // Ensure a virtual input exists so the bus appears in other apps as an input source
        let mut in_guard = INPUT_CONNECTION.lock().unwrap();
        if in_guard.is_none() {
            let mut midi_in = MidiInput::new("joy2qlc-in").map_err(|e| format!("Failed to create MidiInput: {:?}", e))?;
            midi_in.ignore(Ignore::None);
            match midi_in.create_virtual("joy2qlc (virtual in)", move |_stamp, message, _| {
                info!("Received MIDI on virtual input (ignored): {:?}", message);
            }, ()) {
                Ok(conn) => {
                    *in_guard = Some(conn);
                    info!("Created virtual MIDI input");
                    warn!("Created virtual MIDI input (visible as 'joy2qlc (virtual in)')");
                }
                Err(e) => {
                    warn!("Failed to create virtual MIDI input: {:?}", e);
                }
            }
        }

        if out_guard.is_some() || in_guard.is_some() {
            Ok(())
        } else {
            Err("No MIDI connection available".into())
        }
    }

    fn send_raw(bytes: &[u8]) -> Result<(), String> {
        ensure_connection()?;
        let mut guard = OUTPUT_CONNECTION.lock().unwrap();
        if let Some(ref mut conn) = *guard {
            conn.send(bytes).map_err(|e| format!("MIDI send error: {:?}", e))
        } else {
            Err("MIDI connection not available".into())
        }
    }

    /// Return the available output port names (for display/UI)
    pub fn list_output_ports() -> Vec<String> {
        match MidiOutput::new("joy2qlc-list") {
            Ok(mout) => mout.ports().iter().filter_map(|p| mout.port_name(p).ok()).collect(),
            Err(_) => vec![],
        }
    }

    /// Return the available input port names (for display/UI)
    pub fn list_input_ports() -> Vec<String> {
        match MidiInput::new("joy2qlc-list") {
            Ok(min) => min.ports().iter().filter_map(|p| min.port_name(p).ok()).collect(),
            Err(_) => vec![],
        }
    }

    pub fn send_note_on(channel: u8, note: u8, velocity: u8) -> Result<(), String> {
        let status = 0x90u8 | (channel & 0x0f);
        send_raw(&[status, note, velocity])
    }

    pub fn send_note_off(channel: u8, note: u8, velocity: u8) -> Result<(), String> {
        let status = 0x80u8 | (channel & 0x0f);
        send_raw(&[status, note, velocity])
    }

    pub fn send_control_change(channel: u8, controller: u8, value: u8) -> Result<(), String> {
        let status = 0xB0u8 | (channel & 0x0f);
        send_raw(&[status, controller, value])
    }

    pub fn send_program_change(channel: u8, program: u8) -> Result<(), String> {
        let status = 0xC0u8 | (channel & 0x0f);
        send_raw(&[status, program])
    }

    /// Initialize MIDI subsystem (create virtual ports if needed)
    pub fn init() -> Result<(), String> {
        ensure_connection()
    }
}

#[cfg(feature = "midi")]
pub use impl_midi::{send_control_change, send_note_off, send_note_on, send_program_change, init, list_output_ports, list_input_ports};

#[cfg(not(feature = "midi"))]
mod stub {
    use super::*;
    pub fn send_note_on(_channel: u8, _note: u8, _velocity: u8) -> Result<(), String> {
        warn!("MIDI feature disabled: would send note on {} {} {}", _channel, _note, _velocity);
        Ok(())
    }
    pub fn send_note_off(_channel: u8, _note: u8, _velocity: u8) -> Result<(), String> {
        warn!("MIDI feature disabled: would send note off {} {} {}", _channel, _note, _velocity);
        Ok(())
    }
    pub fn send_control_change(_channel: u8, _controller: u8, _value: u8) -> Result<(), String> {
        warn!("MIDI feature disabled: would send control change {} {} {}", _channel, _controller, _value);
        Ok(())
    }
    pub fn send_program_change(_channel: u8, _program: u8) -> Result<(), String> {
        warn!("MIDI feature disabled: would send program change {} {}", _channel, _program);
        Ok(())
    }
}

#[cfg(not(feature = "midi"))]
pub use stub::{send_control_change, send_note_off, send_note_on, send_program_change};
