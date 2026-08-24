use std::collections::HashMap;

use gio::glib;
use gio::prelude::*;
use glib::variant::ObjectPath;

const BLUEZ_BUS_NAME: &str = "org.bluez";
const OBJECT_MANAGER_IFACE: &str = "org.freedesktop.DBus.ObjectManager";
const ADAPTER_IFACE: &str = "org.bluez.Adapter1";
const DEVICE_IFACE: &str = "org.bluez.Device1";
const AGENT_MANAGER_PATH: &str = "/org/bluez";
const AGENT_MANAGER_IFACE: &str = "org.bluez.AgentManager1";
const PROPERTIES_IFACE: &str = "org.freedesktop.DBus.Properties";
const AGENT_PATH: &str = "/com/justabar/BluetoothAgent";

const AGENT_XML: &str = r#"<node>
  <interface name="org.bluez.Agent1">
    <method name="Release"/>
    <method name="RequestPinCode">
      <arg type="o" name="device" direction="in"/>
      <arg type="s" name="pincode" direction="out"/>
    </method>
    <method name="DisplayPinCode">
      <arg type="o" name="device" direction="in"/>
      <arg type="s" name="pincode" direction="in"/>
    </method>
    <method name="RequestPasskey">
      <arg type="o" name="device" direction="in"/>
      <arg type="u" name="passkey" direction="out"/>
    </method>
    <method name="DisplayPasskey">
      <arg type="o" name="device" direction="in"/>
      <arg type="u" name="passkey" direction="in"/>
      <arg type="q" name="entered" direction="in"/>
    </method>
    <method name="RequestConfirmation">
      <arg type="o" name="device" direction="in"/>
      <arg type="u" name="passkey" direction="in"/>
    </method>
    <method name="RequestAuthorization">
      <arg type="o" name="device" direction="in"/>
    </method>
    <method name="AuthorizeService">
      <arg type="o" name="device" direction="in"/>
      <arg type="s" name="uuid" direction="in"/>
    </method>
    <method name="Cancel"/>
  </interface>
</node>"#;

type ManagedObjects = HashMap<ObjectPath, HashMap<String, HashMap<String, glib::Variant>>>;

#[derive(Debug, Clone)]
pub struct DeviceInfo {
    pub path: String,
    pub name: String,
    pub paired: bool,
    pub connected: bool,
}

#[derive(Debug, Clone, Default)]
pub struct BtSnapshot {
    pub powered: bool,
    pub discovering: bool,
    pub devices: Vec<DeviceInfo>,
}

pub struct BtClient {
    connection: gio::DBusConnection,
    adapter_path: String,
    _agent_registration: Option<gio::RegistrationId>,
}

pub fn init() -> Option<BtClient> {
    let connection = match gio::functions::bus_get_sync(gio::BusType::System, gio::Cancellable::NONE) {
        Ok(c) => c,
        Err(err) => {
            eprintln!("jbar: no system D-Bus connection ({err}); bluetooth applet disabled");
            return None;
        }
    };

    let Some(objects) = get_managed_objects(&connection) else {
        eprintln!("jbar: bluez unavailable; bluetooth applet disabled");
        return None;
    };
    let Some(adapter_path) = objects
        .iter()
        .find(|(_, ifaces)| ifaces.contains_key(ADAPTER_IFACE))
        .map(|(path, _)| path.to_string())
    else {
        eprintln!("jbar: no bluetooth adapter found; bluetooth applet disabled");
        return None;
    };

    let agent_registration = register_agent(&connection);
    if agent_registration.is_some() {
        request_default_agent(&connection);
    }

    Some(BtClient {
        connection,
        adapter_path,
        _agent_registration: agent_registration,
    })
}

fn register_agent(connection: &gio::DBusConnection) -> Option<gio::RegistrationId> {
    let node_info = gio::DBusNodeInfo::for_xml(AGENT_XML).ok()?;
    let interface_info = node_info.interfaces().first()?.clone();

    let registration = connection
        .register_object(AGENT_PATH, &interface_info)
        .method_call(|_connection, _sender, _object_path, _interface, method, _params, invocation| {
            let reply = match method {
                "RequestPinCode" => Some("0000".to_variant()),
                "RequestPasskey" => Some(0u32.to_variant()),
                _ => None,
            };
            invocation.return_result(Ok(reply));
        })
        .build();

    match registration {
        Ok(id) => Some(id),
        Err(err) => {
            eprintln!("jbar: failed to register bluetooth pairing agent ({err}); pairing new devices may fail");
            None
        }
    }
}

fn request_default_agent(connection: &gio::DBusConnection) {
    let Ok(agent_path) = ObjectPath::try_from(AGENT_PATH) else {
        return;
    };
    let register_args = glib::Variant::tuple_from_iter([agent_path.to_variant(), "NoInputNoOutput".to_variant()]);
    if call(connection, AGENT_MANAGER_PATH, AGENT_MANAGER_IFACE, "RegisterAgent", Some(&register_args)).is_err() {
        return;
    }
    let default_args = glib::Variant::tuple_from_iter([agent_path.to_variant()]);
    let _ = call(connection, AGENT_MANAGER_PATH, AGENT_MANAGER_IFACE, "RequestDefaultAgent", Some(&default_args));
}

fn call(
    connection: &gio::DBusConnection,
    path: &str,
    iface: &str,
    method: &str,
    args: Option<&glib::Variant>,
) -> Result<glib::Variant, glib::Error> {
    connection.call_sync(
        Some(BLUEZ_BUS_NAME),
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

fn set_prop(connection: &gio::DBusConnection, path: &str, iface: &str, prop: &str, value: glib::Variant) {
    let args = glib::Variant::tuple_from_iter([iface.to_variant(), prop.to_variant(), value.to_variant()]);
    let _ = call(connection, path, PROPERTIES_IFACE, "Set", Some(&args));
}

fn get_managed_objects(connection: &gio::DBusConnection) -> Option<ManagedObjects> {
    call(connection, "/", OBJECT_MANAGER_IFACE, "GetManagedObjects", None)
        .ok()
        .and_then(|reply| reply.child_value(0).get::<ManagedObjects>())
}

impl BtClient {
    pub fn snapshot(&self) -> BtSnapshot {
        let Some(objects) = get_managed_objects(&self.connection) else {
            return BtSnapshot::default();
        };

        let mut powered = false;
        let mut discovering = false;
        let mut devices = Vec::new();

        for (path, ifaces) in &objects {
            if path.to_string() == self.adapter_path {
                if let Some(props) = ifaces.get(ADAPTER_IFACE) {
                    powered = props.get("Powered").and_then(|v| v.get::<bool>()).unwrap_or(false);
                    discovering = props.get("Discovering").and_then(|v| v.get::<bool>()).unwrap_or(false);
                }
            }

            let Some(props) = ifaces.get(DEVICE_IFACE) else {
                continue;
            };
            let adapter = props.get("Adapter").and_then(|v| v.get::<ObjectPath>()).map(|p| p.to_string());
            if adapter.as_deref() != Some(self.adapter_path.as_str()) {
                continue;
            }
            let name = props
                .get("Alias")
                .and_then(|v| v.get::<String>())
                .filter(|s| !s.is_empty())
                .or_else(|| props.get("Name").and_then(|v| v.get::<String>()))
                .unwrap_or_default();
            if name.is_empty() {
                continue;
            }
            let paired = props.get("Paired").and_then(|v| v.get::<bool>()).unwrap_or(false);
            let connected = props.get("Connected").and_then(|v| v.get::<bool>()).unwrap_or(false);
            devices.push(DeviceInfo {
                path: path.to_string(),
                name,
                paired,
                connected,
            });
        }

        devices.sort_by(|a, b| b.connected.cmp(&a.connected).then_with(|| b.paired.cmp(&a.paired)).then_with(|| a.name.cmp(&b.name)));

        BtSnapshot {
            powered,
            discovering,
            devices,
        }
    }

    pub fn subscribe(&self, on_change: impl Fn() + 'static) {
        let on_change = MainThreadOnly(on_change);
        self.connection.signal_subscribe(
            Some(BLUEZ_BUS_NAME),
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

    pub fn set_powered(&self, enabled: bool) {
        set_prop(&self.connection, &self.adapter_path, ADAPTER_IFACE, "Powered", enabled.to_variant());
    }

    pub fn set_discovering(&self, enabled: bool) {
        let method = if enabled { "StartDiscovery" } else { "StopDiscovery" };
        let _ = call(&self.connection, &self.adapter_path, ADAPTER_IFACE, method, None);
    }

    pub fn connect(&self, device_path: &str, on_done: impl FnOnce(Result<(), glib::Error>) + 'static) {
        self.connection.call(
            Some(BLUEZ_BUS_NAME),
            device_path,
            DEVICE_IFACE,
            "Connect",
            None,
            None,
            gio::DBusCallFlags::NONE,
            15000,
            gio::Cancellable::NONE,
            move |result| on_done(result.map(|_| ())),
        );
    }

    pub fn disconnect(&self, device_path: &str, on_done: impl FnOnce(Result<(), glib::Error>) + 'static) {
        self.connection.call(
            Some(BLUEZ_BUS_NAME),
            device_path,
            DEVICE_IFACE,
            "Disconnect",
            None,
            None,
            gio::DBusCallFlags::NONE,
            15000,
            gio::Cancellable::NONE,
            move |result| on_done(result.map(|_| ())),
        );
    }

    pub fn pair_and_connect(&self, device_path: &str, on_done: impl FnOnce(Result<(), glib::Error>) + 'static) {
        let connection = self.connection.clone();
        let device_path = device_path.to_string();
        let connect_path = device_path.clone();
        self.connection.call(
            Some(BLUEZ_BUS_NAME),
            &device_path,
            DEVICE_IFACE,
            "Pair",
            None,
            None,
            gio::DBusCallFlags::NONE,
            30000,
            gio::Cancellable::NONE,
            move |result| match result {
                Err(err) => on_done(Err(err)),
                Ok(_) => {
                    connection.call(
                        Some(BLUEZ_BUS_NAME),
                        &connect_path,
                        DEVICE_IFACE,
                        "Connect",
                        None,
                        None,
                        gio::DBusCallFlags::NONE,
                        15000,
                        gio::Cancellable::NONE,
                        move |result| on_done(result.map(|_| ())),
                    );
                }
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
