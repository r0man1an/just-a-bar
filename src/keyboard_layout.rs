use std::os::fd::AsFd;
use std::thread;
use std::time::Instant;

use calloop::channel::{channel, Channel, Sender as CalloopSender};
use calloop_wayland_source::WaylandSource;
use niri_ipc::socket::{Socket as NiriSocket, SOCKET_PATH_ENV as NIRI_SOCKET_PATH_ENV};
use niri_ipc::{Action as NiriAction, LayoutSwitchTarget, Request as NiriRequest};
use wayland_client::globals::{registry_queue_init, GlobalListContents};
use wayland_client::protocol::wl_registry;
use wayland_client::protocol::wl_seat::{self, WlSeat};
use wayland_client::{Connection, Dispatch, QueueHandle};
use wayland_protocols_misc::zwp_virtual_keyboard_v1::client::zwp_virtual_keyboard_manager_v1::{
    self, ZwpVirtualKeyboardManagerV1,
};
use wayland_protocols_misc::zwp_virtual_keyboard_v1::client::zwp_virtual_keyboard_v1::{self, ZwpVirtualKeyboardV1};

use crate::config::XkbGroupToggle;
use crate::niri_config;

const KEYMAP_FORMAT_XKB_V1: u32 = 1;
const KEY_STATE_PRESSED: u32 = 1;
const KEY_STATE_RELEASED: u32 = 0;

// Linux evdev keycodes (linux/input-event-codes.h) - these are the raw codes wl_keyboard/
// virtual-keyboard use, NOT the xkbcommon keycodes (which are evdev + 8).
const KEY_LEFTCTRL: u32 = 29;
const KEY_LEFTSHIFT: u32 = 42;
const KEY_CAPSLOCK: u32 = 58;
const KEY_LEFTALT: u32 = 56;
const KEY_SPACE: u32 = 57;
const KEY_LEFTMETA: u32 = 125;

fn toggle_keys(toggle: XkbGroupToggle) -> &'static [u32] {
    match toggle {
        XkbGroupToggle::AltShift => &[KEY_LEFTALT, KEY_LEFTSHIFT],
        XkbGroupToggle::SuperSpace => &[KEY_LEFTMETA, KEY_SPACE],
        XkbGroupToggle::CtrlShift => &[KEY_LEFTCTRL, KEY_LEFTSHIFT],
        XkbGroupToggle::CapsLock => &[KEY_CAPSLOCK],
    }
}

#[derive(Debug, Clone, Copy)]
pub enum Command {
    SwitchToGroup(u32),
}

// spawns a background thread and returns a channel to request a layout switch; prefers niri's
// IPC (reliable, exact) and only falls back to the virtual-keyboard trick (best-effort) when
// NIRI_SOCKET isn't set or the connection fails
pub fn spawn(layouts: Vec<String>, toggle: XkbGroupToggle) -> Option<CalloopSender<Command>> {
    let on_niri = std::env::var_os(NIRI_SOCKET_PATH_ENV).is_some();
    if on_niri {
        // niri only switches among XKB groups it already knows about, so keep its config's
        // layout list in sync with ours instead of requiring the user to hand-edit it
        niri_config::sync_layouts(&layouts);
    }

    if layouts.is_empty() {
        return None;
    }

    if on_niri {
        match spawn_niri() {
            Some(tx) => return Some(tx),
            None => eprintln!(
                "jbar: NIRI_SOCKET is set but connecting failed; falling back to the virtual-keyboard layout switcher"
            ),
        }
    }

    spawn_virtual_keyboard(layouts, toggle)
}

fn spawn_niri() -> Option<CalloopSender<Command>> {
    let socket = NiriSocket::connect().ok()?;

    let (cmd_tx, cmd_rx) = channel::<Command>();

    thread::Builder::new()
        .name("jbar-xkb-layout".into())
        .spawn(move || {
            if let Err(err) = run_niri(socket, cmd_rx) {
                eprintln!("jbar: niri keyboard layout switcher stopped ({err})");
            }
        })
        .expect("failed to spawn jbar-xkb-layout thread");

    Some(cmd_tx)
}

fn run_niri(mut socket: NiriSocket, cmd_rx: Channel<Command>) -> Result<(), Box<dyn std::error::Error>> {
    let mut event_loop: calloop::EventLoop<()> = calloop::EventLoop::try_new()?;
    let handle = event_loop.handle();

    handle
        .insert_source(cmd_rx, move |event, _, _| {
            let calloop::channel::Event::Msg(Command::SwitchToGroup(target)) = event else {
                return;
            };
            let Ok(target) = u8::try_from(target) else {
                return;
            };
            let action = NiriAction::SwitchLayout {
                layout: LayoutSwitchTarget::Index(target),
            };
            let _ = socket.send(NiriRequest::Action(action));
        })
        .map_err(|e| format!("failed to register command source: {e}"))?;

    loop {
        event_loop.dispatch(None, &mut ())?;
    }
}

fn spawn_virtual_keyboard(layouts: Vec<String>, toggle: XkbGroupToggle) -> Option<CalloopSender<Command>> {
    let (cmd_tx, cmd_rx) = channel::<Command>();

    thread::Builder::new()
        .name("jbar-xkb-layout".into())
        .spawn(move || {
            if let Err(err) = run_virtual_keyboard(layouts, toggle, cmd_rx) {
                eprintln!("jbar: keyboard layout switcher unavailable ({err})");
            }
        })
        .expect("failed to spawn jbar-xkb-layout thread");

    Some(cmd_tx)
}

struct AppState {
    virtual_keyboard: ZwpVirtualKeyboardV1,
    toggle_keys: &'static [u32],
    layout_count: u32,
    current_group: u32,
    start: Instant,
}

impl AppState {
    fn now_ms(&self) -> u32 {
        self.start.elapsed().as_millis() as u32
    }

    fn send_toggle_once(&mut self) {
        let time = self.now_ms();
        for &key in self.toggle_keys {
            self.virtual_keyboard.key(time, key, KEY_STATE_PRESSED);
        }
        for &key in self.toggle_keys.iter().rev() {
            let time = self.now_ms();
            self.virtual_keyboard.key(time, key, KEY_STATE_RELEASED);
        }
    }
}

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for AppState {
    fn event(
        _state: &mut Self,
        _proxy: &wl_registry::WlRegistry,
        _event: wl_registry::Event,
        _data: &GlobalListContents,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<WlSeat, ()> for AppState {
    fn event(
        _state: &mut Self,
        _proxy: &WlSeat,
        _event: wl_seat::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ZwpVirtualKeyboardManagerV1, ()> for AppState {
    fn event(
        _state: &mut Self,
        _proxy: &ZwpVirtualKeyboardManagerV1,
        _event: zwp_virtual_keyboard_manager_v1::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        // this interface has no events
    }
}

impl Dispatch<ZwpVirtualKeyboardV1, ()> for AppState {
    fn event(
        _state: &mut Self,
        _proxy: &ZwpVirtualKeyboardV1,
        _event: zwp_virtual_keyboard_v1::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        // this interface has no events
    }
}

fn compile_keymap(layouts: &[String], toggle: XkbGroupToggle) -> Result<String, Box<dyn std::error::Error>> {
    let joined = layouts.join(",");
    let context = xkbcommon::xkb::Context::new(xkbcommon::xkb::CONTEXT_NO_FLAGS);
    let keymap = xkbcommon::xkb::Keymap::new_from_names(
        &context,
        "",
        "pc105",
        &joined,
        "",
        Some(toggle.xkb_option().to_string()),
        xkbcommon::xkb::KEYMAP_COMPILE_NO_FLAGS,
    )
    .ok_or("xkbcommon failed to compile a keymap for the configured layouts")?;
    Ok(keymap.get_as_string(xkbcommon::xkb::KEYMAP_FORMAT_TEXT_V1))
}

fn upload_keymap(virtual_keyboard: &ZwpVirtualKeyboardV1, keymap_str: &str) -> std::io::Result<()> {
    use std::io::Write;

    let name = std::ffi::CString::new("jbar-xkb-keymap").unwrap();
    let raw_fd = unsafe { libc::memfd_create(name.as_ptr(), 0) };
    if raw_fd < 0 {
        return Err(std::io::Error::last_os_error());
    }
    let fd = unsafe { <std::os::fd::OwnedFd as std::os::fd::FromRawFd>::from_raw_fd(raw_fd) };
    let mut file = std::fs::File::from(fd.try_clone()?);
    file.write_all(keymap_str.as_bytes())?;
    file.write_all(&[0u8])?; // protocol requires a NUL-terminated buffer
    let size = (keymap_str.len() + 1) as u32;

    virtual_keyboard.keymap(KEYMAP_FORMAT_XKB_V1, fd.as_fd(), size);
    Ok(())
}

fn run_virtual_keyboard(
    layouts: Vec<String>,
    toggle: XkbGroupToggle,
    cmd_rx: Channel<Command>,
) -> Result<(), Box<dyn std::error::Error>> {
    let conn = Connection::connect_to_env()?;
    let (globals, queue) = registry_queue_init::<AppState>(&conn)?;
    let qh = queue.handle();

    let manager: ZwpVirtualKeyboardManagerV1 = globals
        .bind(&qh, 1..=1, ())
        .map_err(|e| format!("compositor does not implement virtual-keyboard-unstable-v1: {e}"))?;
    let seat: WlSeat = globals.bind(&qh, 1..=9, ()).map_err(|e| format!("no wl_seat: {e}"))?;

    let virtual_keyboard = manager.create_virtual_keyboard(&seat, &qh, ());

    let keymap_str = compile_keymap(&layouts, toggle)?;
    upload_keymap(&virtual_keyboard, &keymap_str)?;

    let mut state = AppState {
        virtual_keyboard,
        toggle_keys: toggle_keys(toggle),
        layout_count: layouts.len() as u32,
        current_group: 0,
        start: Instant::now(),
    };

    let mut event_loop: calloop::EventLoop<AppState> = calloop::EventLoop::try_new()?;
    let handle = event_loop.handle();

    WaylandSource::new(conn, queue)
        .insert(handle.clone())
        .map_err(|e| format!("failed to register wayland event source: {e}"))?;

    handle
        .insert_source(cmd_rx, move |event, _, state: &mut AppState| {
            let calloop::channel::Event::Msg(Command::SwitchToGroup(target)) = event else {
                return;
            };
            if state.layout_count == 0 || target >= state.layout_count {
                return;
            }
            let presses = (target + state.layout_count - state.current_group) % state.layout_count;
            for _ in 0..presses {
                state.send_toggle_once();
            }
            state.current_group = target;
        })
        .map_err(|e| format!("failed to register command source: {e}"))?;

    loop {
        event_loop.dispatch(None, &mut state)?;
    }
}
