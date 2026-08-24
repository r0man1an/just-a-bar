// Uses systemD, needs alternative for non-systemD
use std::os::fd::OwnedFd;

use gio::glib;
use gio::prelude::*;
use glib::variant::Handle;

const LOGIND_BUS: &str = "org.freedesktop.login1";
const MANAGER_PATH: &str = "/org/freedesktop/login1";
const MANAGER_IFACE: &str = "org.freedesktop.login1.Manager";

#[derive(Clone)]
pub struct CaffeineClient {
    connection: gio::DBusConnection,
}

pub fn init() -> Option<CaffeineClient> {
    let connection = gio::functions::bus_get_sync(gio::BusType::System, gio::Cancellable::NONE).ok()?;
    Some(CaffeineClient { connection })
}

impl CaffeineClient {
    pub fn inhibit(&self) -> Option<OwnedFd> {
        let args = glib::Variant::tuple_from_iter([
            "idle:sleep".to_variant(),
            "jbar".to_variant(),
            "Caffeine enabled from JustABar".to_variant(),
            "block".to_variant(),
        ]);
        let (reply, fd_list) = self
            .connection
            .call_with_unix_fd_list_sync(
                Some(LOGIND_BUS),
                MANAGER_PATH,
                MANAGER_IFACE,
                "Inhibit",
                Some(&args),
                None,
                gio::DBusCallFlags::NONE,
                3000,
                None::<&gio::UnixFDList>,
                gio::Cancellable::NONE,
            )
            .ok()?;
        let handle: Handle = reply.child_value(0).get()?;
        fd_list?.get(handle.0).ok()
    }
}
