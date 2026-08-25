use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use gtk4::prelude::*;
use gtk4::{glib, Orientation};

use crate::battery_applet;
use crate::bluetooth_applet;
use crate::clipboard_applet;
use crate::clock;
use crate::config::{Config, PanelItem, ThemePreference};
use crate::desktop::DesktopEntryStore;
use crate::geometry::BarGeometry;
use crate::keyboard_applet;
use crate::keyboard_layout;
use crate::layer;
use crate::notification_applet;
use crate::places_applet;
use crate::power_applet;
use crate::screen_applet;
use crate::sound_applet;
use crate::style;
use crate::theme::{self, ColorScheme};
use crate::toplevel::{self, ToplevelEvent, ToplevelId, ToplevelInfo};
use crate::wifi_applet;
use crate::workspaces::{self, WorkspaceEvent, WorkspaceInfo};
use gtk4_layer_shell::LayerShell;

fn apply_prefer_dark(scheme: ColorScheme) {
    if let Some(settings) = gtk4::Settings::default() {
        settings.set_gtk_application_prefer_dark_theme(scheme == ColorScheme::Dark);
    }
}

pub fn build_ui(app: &gtk4::Application) {
    if !gtk4_layer_shell::is_supported() {
        eprintln!(
            "jbar: the compositor doesn't support wlr-layer-shell; \
             this bar only works on wlroots-based Wayland compositors (niri, wayfire, labwc, ...)"
        );
        std::process::exit(1);
    }

    let config = Config::load();

    let window = gtk4::ApplicationWindow::builder()
        .application(app)
        .decorated(false)
        .build();

    let bar_height = config.bar_height as i32;
    layer::init(&window, bar_height);

    let display = gtk4::prelude::WidgetExt::display(&window);
    if let Some(name) = &config.monitor {
        if let Some(monitor) = find_monitor(&display, name) {
            window.set_monitor(Some(&monitor));
        }
    }
    let geometry = BarGeometry::compute(config.bar_height);

    let content = gtk4::CenterBox::new();
    content.add_css_class("bar-background");
    content.set_size_request(-1, bar_height);

    let start_box = gtk4::Box::new(Orientation::Horizontal, geometry.section_spacing);
    start_box.set_valign(gtk4::Align::Center);
    let center_box = gtk4::Box::new(Orientation::Horizontal, geometry.section_spacing);
    center_box.set_valign(gtk4::Align::Center);
    let end_box = gtk4::Box::new(Orientation::Horizontal, geometry.section_spacing);
    end_box.set_valign(gtk4::Align::Center);

    content.set_start_widget(Some(&start_box));
    content.set_center_widget(Some(&center_box));
    content.set_end_widget(Some(&end_box));

    let mut app_label: Option<gtk4::Label> = None;
    let mut workspace_box: Option<gtk4::Box> = None;
    let mut clock_label: Option<gtk4::Label> = None;

    for (section_box, items) in [
        (&start_box, &config.left),
        (&center_box, &config.center),
        (&end_box, &config.right),
    ] {
        for item in items {
            match item {
                PanelItem::Workspaces => {
                    let wb = gtk4::Box::new(Orientation::Horizontal, geometry.workspace_gap);
                    wb.set_valign(gtk4::Align::Center);
                    section_box.append(&wb);
                    workspace_box = Some(wb);
                }
                PanelItem::WindowTitle => {
                    let lbl = gtk4::Label::new(None);
                    lbl.add_css_class("bar-app-label");
                    lbl.set_halign(gtk4::Align::Start);
                    lbl.set_valign(gtk4::Align::Center);
                    section_box.append(&lbl);
                    app_label = Some(lbl);
                }
                PanelItem::Clock => {
                    let lbl = gtk4::Label::new(None);
                    lbl.add_css_class("bar-clock-label");
                    lbl.set_valign(gtk4::Align::Center);
                    section_box.append(&lbl);
                    clock_label = Some(lbl);
                }
                PanelItem::Places => {
                    if let Some(widget) = places_applet::build(
                        app,
                        bar_height,
                        content.upcast_ref(),
                        config.places_display_mode,
                    ) {
                        section_box.append(&widget);
                    }
                }
                PanelItem::Sound => {
                    if let Some(widget) = sound_applet::build(app, bar_height, content.upcast_ref()) {
                        section_box.append(&widget);
                    }
                }
                PanelItem::Wifi => {
                    if let Some(widget) = wifi_applet::build(app, bar_height, content.upcast_ref()) {
                        section_box.append(&widget);
                    }
                }
                PanelItem::Bluetooth => {
                    if let Some(widget) =
                        bluetooth_applet::build(app, bar_height, content.upcast_ref())
                    {
                        section_box.append(&widget);
                    }
                }
                PanelItem::Battery => {
                    if let Some(widget) = battery_applet::build(
                        app,
                        bar_height,
                        content.upcast_ref(),
                        config.battery_show_percentage,
                    ) {
                        section_box.append(&widget);
                    }
                }
                PanelItem::Screen => {
                    if let Some(widget) = screen_applet::build(app, bar_height, content.upcast_ref())
                    {
                        section_box.append(&widget);
                    }
                }
                PanelItem::Keyboard => {
                    let layout_cmd_tx =
                        keyboard_layout::spawn(config.xkb_layouts.clone(), config.xkb_group_toggle);
                    if let Some(widget) = keyboard_applet::build(
                        app,
                        bar_height,
                        content.upcast_ref(),
                        config.xkb_layouts.clone(),
                        layout_cmd_tx,
                    ) {
                        section_box.append(&widget);
                    }
                }
                PanelItem::Clipboard => {
                    if let Some(widget) = clipboard_applet::build(app, content.upcast_ref()) {
                        section_box.append(&widget);
                    }
                }
                PanelItem::Notifications => {
                    if let Some(widget) = notification_applet::build(app, content.upcast_ref()) {
                        section_box.append(&widget);
                    }
                }
                PanelItem::Power => {
                    if let Some(widget) = power_applet::build(app, bar_height, content.upcast_ref())
                    {
                        section_box.append(&widget);
                    }
                }
            }
        }
    }

    window.set_child(Some(&content));

    let css_provider = gtk4::CssProvider::new();
    gtk4::style_context_add_provider_for_display(
        &display,
        &css_provider,
        gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );

    let (scheme, theme_proxy) = match config.theme {
        ThemePreference::System => theme::init(),
        ThemePreference::Light => (ColorScheme::Light, None),
        ThemePreference::Dark => (ColorScheme::Dark, None),
    };
    css_provider.load_from_data(&style::generate_css(scheme, &geometry, config.opacity));
    apply_prefer_dark(scheme);

    if let Some(proxy) = &theme_proxy {
        let css_provider = css_provider.clone();
        let opacity = config.opacity;
        theme::subscribe(proxy, move |scheme| {
            css_provider.load_from_data(&style::generate_css(scheme, &geometry, opacity));
            apply_prefer_dark(scheme);
        });
    }

    if let Some(app_label) = app_label {
        let desktop = DesktopEntryStore::scan();
        let windows: Rc<RefCell<HashMap<ToplevelId, ToplevelInfo>>> =
            Rc::new(RefCell::new(HashMap::new()));

        let (events_tx, events_rx) = async_channel::unbounded();
        let _cmd_tx = toplevel::spawn(events_tx);

        glib::spawn_future_local({
            let windows = windows.clone();
            async move {
                while let Ok(event) = events_rx.recv().await {
                    {
                        let mut guard = windows.borrow_mut();
                        match event {
                            ToplevelEvent::Updated(info) => {
                                guard.insert(info.id, info);
                            }
                            ToplevelEvent::Closed(id) => {
                                guard.remove(&id);
                            }
                        }
                    }

                    let guard = windows.borrow();
                    let name = guard
                        .values()
                        .find(|w| w.activated)
                        .map(|w| {
                            desktop
                                .find_by_app_id(&w.app_id)
                                .map(|e| e.name.clone())
                                .unwrap_or_else(|| w.app_id.clone())
                        })
                        .unwrap_or_default();
                    app_label.set_text(&name);
                }
            }
        });
    }

    if let Some(workspace_box) = workspace_box {
        let workspace_state: Rc<RefCell<Vec<WorkspaceInfo>>> = Rc::new(RefCell::new(Vec::new()));

        let (ws_events_tx, ws_events_rx) = async_channel::unbounded();
        let ws_cmd_tx = workspaces::spawn(ws_events_tx);

        glib::spawn_future_local({
            let workspace_state = workspace_state.clone();
            let workspace_box = workspace_box.clone();
            async move {
                while let Ok(WorkspaceEvent::Snapshot(list)) = ws_events_rx.recv().await {
                    while let Some(child) = workspace_box.first_child() {
                        workspace_box.remove(&child);
                    }
                    for w in &list {
                        let badge = gtk4::Label::new(Some(&w.number.to_string()));
                        badge.add_css_class("bar-workspace-badge");
                        if w.active {
                            badge.add_css_class("active");
                        }
                        badge.set_halign(gtk4::Align::Center);
                        badge.set_valign(gtk4::Align::Center);
                        workspace_box.append(&badge);
                    }
                    *workspace_state.borrow_mut() = list;
                }
            }
        });

        let scroll_switches_workspace = config.scroll_switches_workspace;
        let scroll_controller =
            gtk4::EventControllerScroll::new(gtk4::EventControllerScrollFlags::VERTICAL);
        scroll_controller.connect_scroll(move |_, _dx, dy| {
            if !scroll_switches_workspace {
                return glib::Propagation::Proceed;
            }
            let list = workspace_state.borrow();
            if let Some(active_pos) = list.iter().position(|w| w.active) {
                let target = if dy > 0.0 {
                    active_pos + 1
                } else if dy < 0.0 {
                    active_pos.wrapping_sub(1)
                } else {
                    return glib::Propagation::Proceed;
                };
                if let Some(w) = list.get(target) {
                    let _ = ws_cmd_tx.send(workspaces::Command::Activate(w.id));
                }
            }
            glib::Propagation::Proceed
        });
        content.add_controller(scroll_controller);
    }

    if let Some(clock_label) = clock_label {
        let clock_mode = config.clock_mode;
        let clock_format = config.clock_format;
        clock_label.set_text(&clock::format_now(clock_mode, clock_format));
        glib::timeout_add_local(std::time::Duration::from_secs(1), move || {
            clock_label.set_text(&clock::format_now(clock_mode, clock_format));
            glib::ControlFlow::Continue
        });
    }

    unsafe { window.set_data("theme-proxy", theme_proxy) };

    window.present();
}

fn find_monitor(display: &gtk4::gdk::Display, connector: &str) -> Option<gtk4::gdk::Monitor> {
    let monitors = display.monitors();
    for i in 0..monitors.n_items() {
        let obj = monitors.item(i)?;
        let monitor = obj.downcast::<gtk4::gdk::Monitor>().ok()?;
        if monitor.connector().as_deref() == Some(connector) {
            return Some(monitor);
        }
    }
    None
}
