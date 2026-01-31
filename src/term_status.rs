use once_cell::sync::Lazy;
use std::io::{stdout, Write};
use std::sync::Mutex;

struct Status {
    osc_in: String,
    osc_out: String,
    button: String,
    axis: String,
    initialized: bool,
}

impl Status {
    fn new() -> Self {
        Self {
            osc_in: String::new(),
            osc_out: String::new(),
            button: String::new(),
            axis: String::new(),
            initialized: false,
        }
    }

    fn redraw(&mut self) {
        // Ensure we have four lines reserved
        if !self.initialized {
            println!();
            println!();
            println!();
            println!();
            self.initialized = true;
        }

        let mut out = stdout();
        // Move cursor up 4 lines
        let _ = write!(out, "\x1b[4A");
        // Clear and write osc input line (top)
        let _ = write!(out, "\x1b[2K{}\n", self.osc_in);
        // Clear and write osc output line
        let _ = write!(out, "\x1b[2K{}\n", self.osc_out);
        // Clear and write button line
        let _ = write!(out, "\x1b[2K{}\n", self.button);
        // Clear and write axis line (bottom)
        let _ = write!(out, "\x1b[2K{}\n", self.axis);
        let _ = out.flush();
    }
}

static STATUS: Lazy<Mutex<Status>> = Lazy::new(|| Mutex::new(Status::new()));

/// Set the axis status line (bottom line)
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

/// Set the osc incoming status line (top line)
pub fn set_osc_in_line(s: &str) {
    if let Ok(mut st) = STATUS.lock() {
        st.osc_in = s.to_string();
        st.redraw();
    }
}

/// Set the osc outgoing status line (second line)
pub fn set_osc_out_line(s: &str) {
    if let Ok(mut st) = STATUS.lock() {
        st.osc_out = s.to_string();
        st.redraw();
    }
}