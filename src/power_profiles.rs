use std::collections::HashMap;

use gio::glib;
use gio::prelude::*;

const BUS_NAME: &str = "net.hadess.PowerProfiles";
const OBJECT_PATH: &str = "/net/hadess/PowerProfiles";
const IFACE: &str = "net.hadess.PowerProfiles";
const PROPERTIES_IFACE: &str = "org.freedesktop.DBus.Properties";

#[derive(Clone)]
pub struct PowerProfilesClient {
    connection: gio::DBusConnection,
}

pub fn init() -> Option<PowerProfilesClient> {
    let connection = gio::functions::bus_get_sync(gio::BusType::System, gio::Cancellable::NONE).ok()?;

    get_all_props(&connection)?;
    Some(PowerProfilesClient { connection })
}

fn get_all_props(connection: &gio::DBusConnection) -> Option<HashMap<String, glib::Variant>> {
    let args = glib::Variant::tuple_from_iter([IFACE.to_variant()]);
    let reply = connection
        .call_sync(
            Some(BUS_NAME),
            OBJECT_PATH,
            PROPERTIES_IFACE,
            "GetAll",
            Some(&args),
            None,
            gio::DBusCallFlags::NONE,
            3000,
            gio::Cancellable::NONE,
        )
        .ok()?;
    reply.child_value(0).get::<HashMap<String, glib::Variant>>()
}

impl PowerProfilesClient {
    pub fn active_profile(&self) -> Option<String> {
        get_all_props(&self.connection)?.get("ActiveProfile")?.get::<String>()
    }

    pub fn available_profiles(&self) -> Vec<String> {
        let Some(props) = get_all_props(&self.connection) else {
            return Vec::new();
        };
        let Some(list) = props
            .get("Profiles")
            .and_then(|v| v.get::<Vec<HashMap<String, glib::Variant>>>())
        else {
            return Vec::new();
        };
        list.into_iter()
            .filter_map(|entry| entry.get("Profile").and_then(|v| v.get::<String>()))
            .collect()
    }

    pub fn set_active_profile(&self, profile: &str) {
        let args = glib::Variant::tuple_from_iter([
            IFACE.to_variant(),
            "ActiveProfile".to_variant(),
            profile.to_variant().to_variant(),
        ]);
        let _ = self.connection.call_sync(
            Some(BUS_NAME),
            OBJECT_PATH,
            PROPERTIES_IFACE,
            "Set",
            Some(&args),
            None,
            gio::DBusCallFlags::NONE,
            3000,
            gio::Cancellable::NONE,
        );
    }

    pub fn subscribe(&self, on_change: impl Fn() + 'static) {
        let on_change = MainThreadOnly(on_change);
        self.connection.signal_subscribe(
            Some(BUS_NAME),
            None,
            None,
            None,
            None,
            gio::DBusSignalFlags::NONE,
            move |_conn, _sender, _path, _iface, _signal, _params| {
                on_change.call();
            },
        );
    }
}

struct MainThreadOnly<F>(F);
unsafe impl<F> Send for MainThreadOnly<F> {}
unsafe impl<F> Sync for MainThreadOnly<F> {}

impl<F: Fn()> MainThreadOnly<F> {
    fn call(&self) {
        (self.0)()
    }
}
