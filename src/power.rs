use gio::glib;
use gio::prelude::*;
use glib::variant::ObjectPath;

const LOGIND_BUS: &str = "org.freedesktop.login1";
const MANAGER_PATH: &str = "/org/freedesktop/login1";
const MANAGER_IFACE: &str = "org.freedesktop.login1.Manager";
const SESSION_IFACE: &str = "org.freedesktop.login1.Session";

#[derive(Clone)]
pub struct PowerClient {
    connection: gio::DBusConnection,
    session_path: String,
}

pub fn init() -> Option<PowerClient> {
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

    Some(PowerClient {
        connection,
        session_path: session_path.to_string(),
    })
}

impl PowerClient {
    pub fn suspend(&self) {
        self.call_manager("Suspend");
    }

    pub fn reboot(&self) {
        self.call_manager("Reboot");
    }

    pub fn power_off(&self) {
        self.call_manager("PowerOff");
    }

    pub fn log_out(&self) {
        let _ = self.connection.call_sync(
            Some(LOGIND_BUS),
            &self.session_path,
            SESSION_IFACE,
            "Terminate",
            None,
            None,
            gio::DBusCallFlags::NONE,
            3000,
            gio::Cancellable::NONE,
        );
    }

    fn call_manager(&self, method: &str) {
        let args = glib::Variant::tuple_from_iter([true.to_variant()]);
        let _ = self.connection.call_sync(
            Some(LOGIND_BUS),
            MANAGER_PATH,
            MANAGER_IFACE,
            method,
            Some(&args),
            None,
            gio::DBusCallFlags::NONE,
            3000,
            gio::Cancellable::NONE,
        );
    }
}
