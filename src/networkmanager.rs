use std::collections::HashMap;

use gio::glib;
use gio::prelude::*;
use glib::variant::ObjectPath;

const NM_BUS_NAME: &str = "org.freedesktop.NetworkManager";
const NM_OBJECT_PATH: &str = "/org/freedesktop/NetworkManager";
const NM_IFACE: &str = "org.freedesktop.NetworkManager";
const DEVICE_IFACE: &str = "org.freedesktop.NetworkManager.Device";
const WIRELESS_IFACE: &str = "org.freedesktop.NetworkManager.Device.Wireless";
const AP_IFACE: &str = "org.freedesktop.NetworkManager.AccessPoint";
const PROPERTIES_IFACE: &str = "org.freedesktop.DBus.Properties";
const SETTINGS_OBJECT_PATH: &str = "/org/freedesktop/NetworkManager/Settings";
const SETTINGS_IFACE: &str = "org.freedesktop.NetworkManager.Settings";
const CONNECTION_IFACE: &str = "org.freedesktop.NetworkManager.Settings.Connection";
const ACTIVE_CONNECTION_IFACE: &str = "org.freedesktop.NetworkManager.Connection.Active";

const DEVICE_TYPE_ETHERNET: u32 = 1;
const DEVICE_TYPE_WIFI: u32 = 2;
const DEVICE_TYPE_WIREGUARD: u32 = 29;

const DEVICE_STATE_ACTIVATED: u32 = 100;

const AP_FLAG_PRIVACY: u32 = 0x1;
const AP_SEC_KEY_MGMT_PSK: u32 = 0x100;
const AP_SEC_KEY_MGMT_SAE: u32 = 0x400;

#[derive(Debug, Clone)]
pub struct ApInfo {
    pub path: String,
    pub ssid: String,
    pub strength: u8,
    pub secured: bool,
    pub sae: bool,
}

#[derive(Debug, Clone)]
pub struct VpnInfo {
    pub name: String,


    pub active_connection_path: String,
}

#[derive(Debug, Clone, Default)]
pub struct NmSnapshot {
    pub wifi_enabled: bool,
    pub airplane_mode: bool,
    pub connected_ap: Option<ApInfo>,
    pub access_points: Vec<ApInfo>,
    pub has_wired: bool,
    pub wired_connected: bool,
    pub vpn: Option<VpnInfo>,
}

pub struct NmClient {
    connection: gio::DBusConnection,
    wifi_device_path: Option<String>,
    wired_device_path: Option<String>,
    wireguard_device_path: Option<String>,
}

pub fn init() -> Option<NmClient> {
    let connection = match gio::functions::bus_get_sync(gio::BusType::System, gio::Cancellable::NONE) {
        Ok(c) => c,
        Err(err) => {
            eprintln!("jbar: no system D-Bus connection ({err}); wifi applet disabled");
            return None;
        }
    };

    let device_paths: Vec<String> = match call(&connection, NM_OBJECT_PATH, NM_IFACE, "GetDevices", None) {
        Ok(reply) => reply
            .child_value(0)
            .get::<Vec<ObjectPath>>()
            .unwrap_or_default()
            .iter()
            .map(|p| p.to_string())
            .collect(),
        Err(err) => {
            eprintln!("jbar: NetworkManager unavailable ({err}); wifi applet disabled");
            return None;
        }
    };

    let mut wifi_device_path = None;
    let mut wired_device_path = None;
    let mut wireguard_device_path = None;
    for path in device_paths {
        let props = get_all_props(&connection, &path, DEVICE_IFACE);
        match props.get("DeviceType").and_then(|v| v.get::<u32>()) {
            Some(DEVICE_TYPE_WIFI) if wifi_device_path.is_none() => wifi_device_path = Some(path),
            Some(DEVICE_TYPE_ETHERNET) if wired_device_path.is_none() => wired_device_path = Some(path),
            Some(DEVICE_TYPE_WIREGUARD) if wireguard_device_path.is_none() => wireguard_device_path = Some(path),
            _ => {}
        }
    }

    if wifi_device_path.is_none() && wired_device_path.is_none() {
        eprintln!("jbar: no wifi or wired device found; wifi applet disabled");
        return None;
    }

    Some(NmClient {
        connection,
        wifi_device_path,
        wired_device_path,
        wireguard_device_path,
    })
}

fn call(
    connection: &gio::DBusConnection,
    path: &str,
    iface: &str,
    method: &str,
    args: Option<&glib::Variant>,
) -> Result<glib::Variant, glib::Error> {
    connection.call_sync(
        Some(NM_BUS_NAME),
        path,
        iface,
        method,
        args,
        None,
        gio::DBusCallFlags::NONE,
        3000,
        gio::Cancellable::NONE,
    )
}

fn get_all_props(connection: &gio::DBusConnection, path: &str, iface: &str) -> HashMap<String, glib::Variant> {
    let args = glib::Variant::tuple_from_iter([iface.to_variant()]);
    call(connection, path, PROPERTIES_IFACE, "GetAll", Some(&args))
        .ok()
        .and_then(|reply| reply.child_value(0).get::<HashMap<String, glib::Variant>>())
        .unwrap_or_default()
}

fn set_prop(
    connection: &gio::DBusConnection,
    path: &str,
    iface: &str,
    prop: &str,
    value: glib::Variant,
) -> Result<(), glib::Error> {
    let args = glib::Variant::tuple_from_iter([iface.to_variant(), prop.to_variant(), value.to_variant()]);
    call(connection, path, PROPERTIES_IFACE, "Set", Some(&args))?;
    Ok(())
}

fn ssid_to_string(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

fn ap_info_from_props(path: &str, props: &HashMap<String, glib::Variant>) -> ApInfo {
    let ssid = props
        .get("Ssid")
        .and_then(|v| v.get::<Vec<u8>>())
        .map(|b| ssid_to_string(&b))
        .unwrap_or_default();
    let strength = props.get("Strength").and_then(|v| v.get::<u8>()).unwrap_or(0);
    let flags = props.get("Flags").and_then(|v| v.get::<u32>()).unwrap_or(0);
    let wpa_flags = props.get("WpaFlags").and_then(|v| v.get::<u32>()).unwrap_or(0);
    let rsn_flags = props.get("RsnFlags").and_then(|v| v.get::<u32>()).unwrap_or(0);
    let key_mgmt = wpa_flags | rsn_flags;
    let sae = key_mgmt & AP_SEC_KEY_MGMT_SAE != 0;
    let secured = flags & AP_FLAG_PRIVACY != 0 || key_mgmt & (AP_SEC_KEY_MGMT_PSK | AP_SEC_KEY_MGMT_SAE) != 0;

    ApInfo {
        path: path.to_string(),
        ssid,
        strength,
        secured,
        sae,
    }
}

impl NmClient {
    pub fn snapshot(&self) -> NmSnapshot {
        let nm_props = get_all_props(&self.connection, NM_OBJECT_PATH, NM_IFACE);
        let wifi_enabled = nm_props.get("WirelessEnabled").and_then(|v| v.get::<bool>()).unwrap_or(false);
        let wwan_enabled = nm_props.get("WwanEnabled").and_then(|v| v.get::<bool>()).unwrap_or(true);
        let airplane_mode = !wifi_enabled && !wwan_enabled;

        let mut connected_ap = None;
        let mut access_points = Vec::new();

        if let Some(wifi_path) = &self.wifi_device_path {
            let wireless_props = get_all_props(&self.connection, wifi_path, WIRELESS_IFACE);
            let active_ap_path = wireless_props
                .get("ActiveAccessPoint")
                .and_then(|v| v.get::<ObjectPath>())
                .map(|p| p.to_string())
                .filter(|p| p != "/");

            if let Ok(reply) = call(&self.connection, wifi_path, WIRELESS_IFACE, "GetAccessPoints", None) {
                let ap_paths: Vec<String> = reply
                    .child_value(0)
                    .get::<Vec<ObjectPath>>()
                    .unwrap_or_default()
                    .iter()
                    .map(|p| p.to_string())
                    .collect();

                for ap_path in ap_paths {
                    let props = get_all_props(&self.connection, &ap_path, AP_IFACE);
                    let info = ap_info_from_props(&ap_path, &props);
                    if info.ssid.is_empty() {
                        continue;
                    }
                    if Some(&ap_path) == active_ap_path.as_ref() {
                        connected_ap = Some(info.clone());
                    }
                    access_points.push(info);
                }
            }
            access_points.sort_by(|a, b| b.strength.cmp(&a.strength));
        }

        let mut has_wired = false;
        let mut wired_connected = false;
        if let Some(wired_path) = &self.wired_device_path {
            has_wired = true;
            let props = get_all_props(&self.connection, wired_path, DEVICE_IFACE);
            let state = props.get("State").and_then(|v| v.get::<u32>()).unwrap_or(0);
            wired_connected = state == DEVICE_STATE_ACTIVATED;
        }

        let mut vpn = None;
        let active_paths: Vec<String> = nm_props
            .get("ActiveConnections")
            .and_then(|v| v.get::<Vec<ObjectPath>>())
            .unwrap_or_default()
            .iter()
            .map(|p| p.to_string())
            .collect();
        for path in active_paths {
            let props = get_all_props(&self.connection, &path, ACTIVE_CONNECTION_IFACE);
            let is_vpn = props.get("Vpn").and_then(|v| v.get::<bool>()).unwrap_or(false);
            if is_vpn {
                let name = props.get("Id").and_then(|v| v.get::<String>()).unwrap_or_default();
                vpn = Some(VpnInfo {
                    name,
                    active_connection_path: path,
                });
                break;
            }
        }

        if vpn.is_none() {
            if let Some(wg_path) = &self.wireguard_device_path {
                let props = get_all_props(&self.connection, wg_path, DEVICE_IFACE);
                let state = props.get("State").and_then(|v| v.get::<u32>()).unwrap_or(0);
                if state == DEVICE_STATE_ACTIVATED {
                    let active_connection_path = props
                        .get("ActiveConnection")
                        .and_then(|v| v.get::<ObjectPath>())
                        .map(|p| p.to_string())
                        .filter(|p| p != "/");
                    if let Some(active_connection_path) = active_connection_path {
                        let active_props =
                            get_all_props(&self.connection, &active_connection_path, ACTIVE_CONNECTION_IFACE);
                        let name = active_props
                            .get("Id")
                            .and_then(|v| v.get::<String>())
                            .or_else(|| props.get("Interface").and_then(|v| v.get::<String>()))
                            .unwrap_or_default();
                        vpn = Some(VpnInfo {
                            name,
                            active_connection_path,
                        });
                    }
                }
            }
        }

        NmSnapshot {
            wifi_enabled,
            airplane_mode,
            connected_ap,
            vpn,
            access_points,
            has_wired,
            wired_connected,
        }
    }

    pub fn subscribe(&self, on_change: impl Fn() + 'static) {
        let on_change = MainThreadOnly(on_change);
        self.connection.signal_subscribe(
            Some(NM_BUS_NAME),
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

    pub fn disconnect_vpn(&self, active_connection_path: &str) {
        let Ok(path) = ObjectPath::try_from(active_connection_path) else {
            return;
        };
        let args = glib::Variant::tuple_from_iter([path.to_variant()]);
        if let Err(err) = call(&self.connection, NM_OBJECT_PATH, NM_IFACE, "DeactivateConnection", Some(&args)) {
            eprintln!("jbar: failed to deactivate VPN: {err}");
        }
    }

    pub fn set_wifi_enabled(&self, enabled: bool) {
        let _ = set_prop(&self.connection, NM_OBJECT_PATH, NM_IFACE, "WirelessEnabled", enabled.to_variant());
    }

    pub fn set_airplane_mode(&self, enabled: bool) {
        let _ = set_prop(&self.connection, NM_OBJECT_PATH, NM_IFACE, "WirelessEnabled", (!enabled).to_variant());
        let _ = set_prop(&self.connection, NM_OBJECT_PATH, NM_IFACE, "WwanEnabled", (!enabled).to_variant());
    }

    pub fn rescan(&self) {
        if let Some(wifi_path) = &self.wifi_device_path {
            let empty_options: HashMap<&str, glib::Variant> = HashMap::new();
            let args = glib::Variant::tuple_from_iter([empty_options.to_variant()]);
            let _ = call(&self.connection, wifi_path, WIRELESS_IFACE, "RequestScan", Some(&args));
        }
    }

    pub fn disconnect(&self) {
        if let Some(wifi_path) = &self.wifi_device_path {
            let _ = call(&self.connection, wifi_path, DEVICE_IFACE, "Disconnect", None);
        }
    }

    pub fn try_connect_saved(&self, ap: &ApInfo) -> Option<()> {
        let saved_path = self.find_saved_connection(&ap.ssid)?;
        let Some(wifi_path) = &self.wifi_device_path else {
            return None;
        };
        let Ok(device_path) = ObjectPath::try_from(wifi_path.as_str()) else {
            return None;
        };
        let Ok(ap_path) = ObjectPath::try_from(ap.path.as_str()) else {
            return None;
        };
        let Ok(saved_object_path) = ObjectPath::try_from(saved_path.as_str()) else {
            return None;
        };

        let args = glib::Variant::tuple_from_iter([
            saved_object_path.to_variant(),
            device_path.to_variant(),
            ap_path.to_variant(),
        ]);
        if let Err(err) = call(&self.connection, NM_OBJECT_PATH, NM_IFACE, "ActivateConnection", Some(&args)) {
            eprintln!("jbar: failed to activate saved connection for {}: {err}", ap.ssid);
        }
        Some(())
    }

    fn find_saved_connection(&self, ssid: &str) -> Option<String> {
        let reply = call(&self.connection, SETTINGS_OBJECT_PATH, SETTINGS_IFACE, "ListConnections", None).ok()?;
        let paths: Vec<ObjectPath> = reply.child_value(0).get()?;

        for path in paths {
            let path = path.to_string();
            let Ok(reply) = call(&self.connection, &path, CONNECTION_IFACE, "GetSettings", None) else {
                continue;
            };
            let Some(settings) = reply
                .child_value(0)
                .get::<HashMap<String, HashMap<String, glib::Variant>>>()
            else {
                continue;
            };
            let profile_ssid = settings
                .get("802-11-wireless")
                .and_then(|w| w.get("ssid"))
                .and_then(|v| v.get::<Vec<u8>>())
                .map(|b| ssid_to_string(&b));
            if profile_ssid.as_deref() == Some(ssid) {
                return Some(path);
            }
        }
        None
    }

    pub fn connect_open(&self, ap: &ApInfo) {
        if self.try_connect_saved(ap).is_some() {
            return;
        }
        self.activate(ap, None);
    }

    pub fn connect_secured(&self, ap: &ApInfo, password: &str) {
        self.activate(ap, Some(password));
    }

    fn activate(&self, ap: &ApInfo, password: Option<&str>) {
        let Some(wifi_path) = &self.wifi_device_path else {
            return;
        };

        let mut wireless: HashMap<&str, glib::Variant> = HashMap::new();
        wireless.insert("ssid", ap.ssid.as_bytes().to_vec().to_variant());
        wireless.insert("mode", "infrastructure".to_variant());

        let mut connection_section: HashMap<&str, glib::Variant> = HashMap::new();
        connection_section.insert("type", "802-11-wireless".to_variant());
        connection_section.insert("id", ap.ssid.as_str().to_variant());

        let mut settings: HashMap<&str, HashMap<&str, glib::Variant>> = HashMap::new();
        settings.insert("connection", connection_section);
        settings.insert("802-11-wireless", wireless);

        if let Some(password) = password {
            let key_mgmt = if ap.sae { "sae" } else { "wpa-psk" };
            let mut security: HashMap<&str, glib::Variant> = HashMap::new();
            security.insert("key-mgmt", key_mgmt.to_variant());
            security.insert("psk", password.to_variant());
            settings.insert("802-11-wireless-security", security);
        }

        let Ok(device_path) = ObjectPath::try_from(wifi_path.as_str()) else {
            return;
        };
        let Ok(ap_path) = ObjectPath::try_from(ap.path.as_str()) else {
            return;
        };

        let args = glib::Variant::tuple_from_iter([
            settings.to_variant(),
            device_path.to_variant(),
            ap_path.to_variant(),
        ]);

        if let Err(err) = call(&self.connection, NM_OBJECT_PATH, NM_IFACE, "AddAndActivateConnection", Some(&args)) {
            eprintln!("jbar: failed to activate wifi connection {}: {err}", ap.ssid);
        }
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
