use std::fs;

use gio::glib;
use gio::prelude::*;
use glib::variant::ObjectPath;

const LOGIND_BUS: &str = "org.freedesktop.login1";
const MANAGER_PATH: &str = "/org/freedesktop/login1";
const MANAGER_IFACE: &str = "org.freedesktop.login1.Manager";
const SESSION_IFACE: &str = "org.freedesktop.login1.Session";

pub struct BrightnessClient {
    connection: gio::DBusConnection,
    session_path: String,
    device: String,
    max: u32,
}

pub fn init() -> Option<BrightnessClient> {
    let device = find_backlight()?;
    let max = read_max(&device)?;
    if max == 0 {
        return None;
    }

    let connection = gio::functions::bus_get_sync(gio::BusType::System, gio::Cancellable::NONE).ok()?;

    let args = glib::Variant::tuple_from_iter([std::process::id().to_variant()]);
    let reply = connection
        .call_sync(
            Some(LOGIND_BUS),
            MANAGER_PATH,
            MANAGER_IFACE,
            "GetSessionByPID",
            Some(&args),
            None,
            gio::DBusCallFlags::NONE,
            3000,
            gio::Cancellable::NONE,
        )
        .ok()?;
    let session_path: ObjectPath = reply.child_value(0).get()?;

    Some(BrightnessClient {
        connection,
        session_path: session_path.to_string(),
        device,
        max,
    })
}

fn find_backlight() -> Option<String> {
    let entries = fs::read_dir("/sys/class/backlight").ok()?;
    entries.flatten().next().map(|e| e.file_name().to_string_lossy().into_owned())
}

fn read_max(device: &str) -> Option<u32> {
    fs::read_to_string(format!("/sys/class/backlight/{device}/max_brightness"))
        .ok()?
        .trim()
        .parse()
        .ok()
}

impl BrightnessClient {
    pub fn get(&self) -> Option<f64> {
        let raw: u32 = fs::read_to_string(format!("/sys/class/backlight/{}/brightness", self.device))
            .ok()?
            .trim()
            .parse()
            .ok()?;
        Some(raw as f64 / self.max as f64)
    }

    pub fn set(&self, fraction: f64) {
        let value = (fraction.clamp(0.0, 1.0) * self.max as f64).round() as u32;
        let args = glib::Variant::tuple_from_iter([
            "backlight".to_variant(),
            self.device.as_str().to_variant(),
            value.to_variant(),
        ]);
        let _ = self.connection.call_sync(
            Some(LOGIND_BUS),
            &self.session_path,
            SESSION_IFACE,
            "SetBrightness",
            Some(&args),
            None,
            gio::DBusCallFlags::NONE,
            3000,
            gio::Cancellable::NONE,
        );
    }
}
