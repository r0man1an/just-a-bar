use std::collections::HashMap;
use std::sync::Arc;
use std::thread;

use async_channel::Sender as AsyncSender;
use calloop::channel::{channel, Channel, Sender as CalloopSender};
use calloop_wayland_source::WaylandSource;
use wayland_client::backend::ObjectData;
use wayland_client::globals::{registry_queue_init, GlobalListContents};
use wayland_client::protocol::wl_registry;
use wayland_client::{Connection, Dispatch, Proxy, QueueHandle};
use wayland_protocols::ext::workspace::v1::client::ext_workspace_group_handle_v1::{
    self, ExtWorkspaceGroupHandleV1,
};
use wayland_protocols::ext::workspace::v1::client::ext_workspace_handle_v1::{
    self, ExtWorkspaceHandleV1,
};
use wayland_protocols::ext::workspace::v1::client::ext_workspace_manager_v1::{
    self, ExtWorkspaceManagerV1,
};

pub type WorkspaceId = u32;

#[derive(Debug, Clone)]
pub struct WorkspaceInfo {
    pub id: WorkspaceId,
    pub number: u32,
    pub active: bool,
}

#[derive(Debug, Clone)]
pub enum WorkspaceEvent {
    Snapshot(Vec<WorkspaceInfo>),
}

#[derive(Debug, Clone, Copy)]
pub enum Command {
    Activate(WorkspaceId),
}

#[derive(Default)]
struct PendingWorkspace {
    name: String,
    active: bool,
    coordinates: Vec<i32>,
}

struct AppState {
    manager: Option<ExtWorkspaceManagerV1>,
    order: Vec<WorkspaceId>,
    pending: HashMap<WorkspaceId, PendingWorkspace>,
    handles: HashMap<WorkspaceId, ExtWorkspaceHandleV1>,
    events_tx: AsyncSender<WorkspaceEvent>,
}

fn handle_id(handle: &ExtWorkspaceHandleV1) -> WorkspaceId {
    handle.id().protocol_id()
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

impl Dispatch<ExtWorkspaceGroupHandleV1, ()> for AppState {
    fn event(
        _state: &mut Self,
        _proxy: &ExtWorkspaceGroupHandleV1,
        _event: ext_workspace_group_handle_v1::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ExtWorkspaceManagerV1, ()> for AppState {
    fn event(
        state: &mut Self,
        _manager: &ExtWorkspaceManagerV1,
        event: ext_workspace_manager_v1::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        match event {
            ext_workspace_manager_v1::Event::Workspace { workspace } => {
                let id = handle_id(&workspace);
                state.order.push(id);
                state.pending.insert(id, PendingWorkspace::default());
                state.handles.insert(id, workspace);
            }
            ext_workspace_manager_v1::Event::Done => {
                let mut ordered: Vec<&WorkspaceId> = state.order.iter().collect();
                ordered.sort_by(|a, b| {
                    let ca = state.pending.get(*a).map(|p| &p.coordinates);
                    let cb = state.pending.get(*b).map(|p| &p.coordinates);
                    ca.cmp(&cb)
                });
                let snapshot: Vec<WorkspaceInfo> = ordered
                    .into_iter()
                    .enumerate()
                    .filter_map(|(i, id)| {
                        state.pending.get(id).map(|p| WorkspaceInfo {
                            id: *id,
                            number: p.name.parse().unwrap_or((i + 1) as u32),
                            active: p.active,
                        })
                    })
                    .collect();
                let _ = state.events_tx.try_send(WorkspaceEvent::Snapshot(snapshot));
            }
            _ => {}
        }
    }

    fn event_created_child(opcode: u16, qhandle: &QueueHandle<Self>) -> Arc<dyn ObjectData> {
        match opcode {
            0 => qhandle.make_data::<ExtWorkspaceGroupHandleV1, ()>(()),
            1 => qhandle.make_data::<ExtWorkspaceHandleV1, ()>(()),
            _ => unreachable!("ext_workspace_manager_v1 has no other object-creating events"),
        }
    }
}

impl Dispatch<ExtWorkspaceHandleV1, ()> for AppState {
    fn event(
        state: &mut Self,
        handle: &ExtWorkspaceHandleV1,
        event: ext_workspace_handle_v1::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        let id = handle_id(handle);
        match event {
            ext_workspace_handle_v1::Event::Name { name } => {
                state.pending.entry(id).or_default().name = name;
            }
            ext_workspace_handle_v1::Event::Coordinates { coordinates } => {
                let parsed = coordinates
                    .chunks_exact(4)
                    .map(|c| i32::from_ne_bytes(c.try_into().unwrap()))
                    .collect();
                state.pending.entry(id).or_default().coordinates = parsed;
            }
            ext_workspace_handle_v1::Event::State { state: raw } => {
                let bits: u32 = raw.into();
                state.pending.entry(id).or_default().active = bits & 1 != 0;
            }
            ext_workspace_handle_v1::Event::Removed => {
                state.pending.remove(&id);
                state.order.retain(|&x| x != id);
                if let Some(h) = state.handles.remove(&id) {
                    h.destroy();
                }
            }
            _ => {}
        }
    }
}

pub fn spawn(events_tx: AsyncSender<WorkspaceEvent>) -> CalloopSender<Command> {
    let (cmd_tx, cmd_rx) = channel::<Command>();

    thread::Builder::new()
        .name("ext-workspace".into())
        .spawn(move || {
            if let Err(err) = run(events_tx, cmd_rx) {
                eprintln!(
                    "jbar: ext-workspace-v1 unavailable ({err}); the workspace number won't update"
                );
            }
        })
        .expect("failed to spawn ext-workspace thread");

    cmd_tx
}

fn run(
    events_tx: AsyncSender<WorkspaceEvent>,
    cmd_rx: Channel<Command>,
) -> Result<(), Box<dyn std::error::Error>> {
    let conn = Connection::connect_to_env()?;
    let (globals, queue) = registry_queue_init::<AppState>(&conn)?;
    let qh = queue.handle();

    let manager: ExtWorkspaceManagerV1 = globals
        .bind(&qh, 1..=1, ())
        .map_err(|e| format!("compositor does not implement ext-workspace-v1: {e}"))?;

    let mut state = AppState {
        manager: Some(manager),
        order: Vec::new(),
        pending: HashMap::new(),
        handles: HashMap::new(),
        events_tx,
    };

    let mut event_loop: calloop::EventLoop<AppState> = calloop::EventLoop::try_new()?;
    let handle = event_loop.handle();

    WaylandSource::new(conn, queue)
        .insert(handle.clone())
        .map_err(|e| format!("failed to register wayland event source: {e}"))?;

    handle
        .insert_source(cmd_rx, move |event, _, state: &mut AppState| {
            let calloop::channel::Event::Msg(cmd) = event else {
                return;
            };
            match cmd {
                Command::Activate(id) => {
                    if let Some(handle) = state.handles.get(&id) {
                        handle.activate();
                        if let Some(manager) = &state.manager {
                            manager.commit();
                        }
                    }
                }
            }
        })
        .map_err(|e| format!("failed to register command source: {e}"))?;

    loop {
        event_loop.dispatch(None, &mut state)?;
    }
}
