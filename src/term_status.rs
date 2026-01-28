use once_cell::sync::Lazy;
use std::io::{stdout, Write};
use std::sync::Mutex;

struct Status {
    axis: String,
    button: String,
    midi: String,
    initialized: bool,
}

impl Status {
    fn new() -> Self {
        Self {
            axis: String::new(),
            button: String::new(),
            midi: String::new(),
            initialized: false,
        }
    }

    fn redraw(&mut self) {
        // Ensure we have three lines reserved
        if !self.initialized {
            println!();
            println!();
            println!();
            self.initialized = true;
        }

        let mut out = stdout();
        // Move cursor up 3 lines
        let _ = write!(out, "\x1b[3A");
        // Clear and write axis line
        let _ = write!(out, "\x1b[2K{}\n", self.axis);
        // Clear and write button line
        let _ = write!(out, "\x1b[2K{}\n", self.button);
        // Clear and write midi line (bottom)
        let _ = write!(out, "\x1b[2K{}\n", self.midi);
        let _ = out.flush();
    }
}

static STATUS: Lazy<Mutex<Status>> = Lazy::new(|| Mutex::new(Status::new()));

/// Set the axis status line (top line)
pub fn set_axis_line(s: &str) {
    if let Ok(mut st) = STATUS.lock() {
        st.axis = s.to_string();
        st.redraw();
    }
}

/// Set the button status line (middle line)
pub fn set_button_line(s: &str) {
    if let Ok(mut st) = STATUS.lock() {
        st.button = s.to_string();
        st.redraw();
    }
}

/// Set the midi status line (bottom line)
pub fn set_midi_line(s: &str) {
    if let Ok(mut st) = STATUS.lock() {
        st.midi = s.to_string();
        st.redraw();
    }
}