// System-wide dark/light toggle via GNOME's `color-scheme` GSettings key (needs further testing)
use std::process::Command;

use crate::theme::ColorScheme;

const SCHEMA: &str = "org.gnome.desktop.interface";
const KEY: &str = "color-scheme";

pub fn is_available() -> bool {
    Command::new("gsettings")
        .args(["get", SCHEMA, KEY])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

pub fn get() -> ColorScheme {
    let output = Command::new("gsettings").args(["get", SCHEMA, KEY]).output();
    let Ok(output) = output else {
        eprintln!("jbar: failed to run gsettings get {SCHEMA} {KEY}");
        return ColorScheme::Light;
    };
    if !output.status.success() {
        eprintln!(
            "jbar: gsettings get {SCHEMA} {KEY} exited with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        );
        return ColorScheme::Light;
    }

    let value = String::from_utf8_lossy(&output.stdout);
    let value = value.trim().trim_matches('\'');
    if value == "prefer-dark" {
        ColorScheme::Dark
    } else {
        ColorScheme::Light
    }
}

pub fn set_dark(dark: bool) {
    let value = if dark { "prefer-dark" } else { "default" };
    match Command::new("gsettings").args(["set", SCHEMA, KEY, value]).output() {
        Ok(output) if !output.status.success() => {
            eprintln!(
                "jbar: gsettings set {SCHEMA} {KEY} {value} exited with {}: {}",
                output.status,
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }
        Err(err) => eprintln!("jbar: failed to run gsettings ({err})"),
        Ok(_) => {}
    }
}
