use std::io::Write;

use crossterm::{cursor, execute, terminal};

use crate::device::Device;
use crate::emulatedCommand::getCommandDescription;

// Redraws the whole screen. Lines end in "\r\n" because the terminal is in raw mode.
pub fn render(device: &Device, brokerInfo: &str) {
    let mut stdout = std::io::stdout();
    let _ = execute!(stdout, terminal::Clear(terminal::ClearType::All), cursor::MoveTo(0, 0));

    let mut out = String::new();
    out.push_str("Doorbell device emulator - type to emulate:\r\n");
    out.push_str(&format!("  {}\r\n", getCommandDescription()));
    out.push_str("\r\n");
    out.push_str(&format!(
        "  Broker: {}   |   Auto-buzz: {}   |   Ring active: {}\r\n",
        brokerInfo,
        if device.autoBuzz { "ON" } else { "OFF" },
        if device.ringActive { "yes" } else { "no" },
    ));
    out.push_str("\r\n");
    out.push_str("  Recent events (newest first):\r\n");

    let lines = device.logLines();
    if lines.is_empty() {
        out.push_str("    (none yet)\r\n");
    } else {
        for line in lines {
            out.push_str(&format!("    {}\r\n", line));
        }
    }

    let _ = write!(stdout, "{}", out);
    let _ = stdout.flush();
}
