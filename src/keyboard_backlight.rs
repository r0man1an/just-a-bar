use std::fs;

use gio::glib;
use gio::prelude::*;
use glib::variant::ObjectPath;

const LOGIND_BUS: &str = "org.freedesktop.login1";
const MANAGER_PATH: &str = "/org/freedesktop/login1";
const MANAGER_IFACE: &str = "org.freedesktop.login1.Manager";
const SESSION_IFACE: &str = "org.freedesktop.login1.Session";

pub struct KeyboardBacklightClient {
    connection: gio::DBusConnection,
    session_path: String,
    device: String,
    max: u32,
}

pub fn init() -> Option<KeyboardBacklightClient> {
    let device = find_kbd_backlight()?;
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

    Some(KeyboardBacklightClient {
        connection,
        session_path: session_path.to_string(),
        device,
        max,
    })
}

fn find_kbd_backlight() -> Option<String> {
    let entries = fs::read_dir("/sys/class/leds").ok()?;
    entries
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .find(|name| name.contains("kbd_backlight"))
}

fn read_max(device: &str) -> Option<u32> {
    fs::read_to_string(format!("/sys/class/leds/{device}/max_brightness"))
        .ok()?
        .trim()
        .parse()
        .ok()
}

impl KeyboardBacklightClient {
    pub fn max(&self) -> u32 {
        self.max
    }

    pub fn get(&self) -> Option<u32> {
        fs::read_to_string(format!("/sys/class/leds/{}/brightness", self.device))
            .ok()?
            .trim()
            .parse()
            .ok()
    }

    pub fn set(&self, value: u32) {
        let value = value.min(self.max);
        let args = glib::Variant::tuple_from_iter([
            "leds".to_variant(),
            self.device.as_str().to_variant(),
            value.to_variant(),
        ]);
        if let Err(err) = self.connection.call_sync(
            Some(LOGIND_BUS),
            &self.session_path,
            SESSION_IFACE,
            "SetBrightness",
            Some(&args),
            None,
            gio::DBusCallFlags::NONE,
            3000,
            gio::Cancellable::NONE,
        ) {
            eprintln!("jbar: SetBrightness({value}) on {} failed: {err}", self.device);
        }
    }
}
