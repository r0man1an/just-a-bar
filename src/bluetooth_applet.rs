use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;
use gtk4::{glib, Orientation};
use gtk4_layer_shell::Edge;

use crate::bluetooth::{self, BtClient, BtSnapshot, DeviceInfo};
use crate::overlay::{build_overlay_window, dismiss_on_escape, position_near_icon};
use crate::style;

struct AppletState {
    bt: BtClient,
    snapshot: BtSnapshot,
    icon_image: gtk4::Image,
    icon_button: gtk4::Button,
    content: gtk4::Widget,
    popover_box: gtk4::Box,
    popover_window: gtk4::ApplicationWindow,
    popover_just_dismissed: bool,
}

pub fn build(app: &gtk4::Application, bar_height: i32, content: &gtk4::Widget) -> Option<gtk4::Widget> {
    let bt = bluetooth::init()?;

    let icon_image = gtk4::Image::new();
    let icon_button = gtk4::Button::new();
    icon_button.add_css_class("bar-applet-icon");
    icon_button.set_child(Some(&icon_image));

    let popover_box = gtk4::Box::new(Orientation::Vertical, 10);
    popover_box.add_css_class("bar-applet-popover");
    popover_box.set_size_request(style::APPLET_POPOVER_WIDTH_PX, -1);

    let popover_window = build_overlay_window(app, "justabar-bluetooth-popover", Edge::Right, &popover_box);
    popover_window.present();
    popover_window.set_visible(false);

    dismiss_on_escape(&popover_window);

    let initial_snapshot = bt.snapshot();
    let state = Rc::new(RefCell::new(AppletState {
        bt,
        snapshot: initial_snapshot,
        icon_image,
        icon_button: icon_button.clone(),
        content: content.clone(),
        popover_box,
        popover_window: popover_window.clone(),
        popover_just_dismissed: false,
    }));

    update_icon(&state);

    popover_window.connect_notify_local(Some("is-active"), {
        let state = state.clone();
        move |window, _| {
            if !window.is_active() {
                window.set_visible(false);
                state.borrow_mut().popover_just_dismissed = true;
            }
        }
    });

    icon_button.connect_clicked({
        let state = state.clone();
        move |_| {
            let just_dismissed = {
                let mut guard = state.borrow_mut();
                std::mem::take(&mut guard.popover_just_dismissed)
            };
            if just_dismissed {
                return;
            }
            let already_open = state.borrow().popover_window.is_visible();
            if already_open {
                let popover_window = state.borrow().popover_window.clone();
                popover_window.set_visible(false);
                return;
            }
            refresh(&state);
            let guard = state.borrow();
            position_near_icon(&guard.popover_window, &guard.icon_button, &guard.content, Edge::Right);
            guard.popover_window.present();
            guard.popover_window.set_visible(true);
        }
    });

    {
        let state_for_subscribe = state.clone();
        state.borrow().bt.subscribe(move || {
            refresh(&state_for_subscribe);
        });
    }

    Some(icon_button.upcast())
}

fn refresh(state: &Rc<RefCell<AppletState>>) {
    let new_snapshot = state.borrow().bt.snapshot();
    state.borrow_mut().snapshot = new_snapshot;
    update_icon(state);
    rebuild_popover(state);
}

fn update_icon(state: &Rc<RefCell<AppletState>>) {
    let guard = state.borrow();
    guard.icon_image.set_icon_name(Some(icon_name_for(&guard.snapshot)));
}

fn icon_name_for(snapshot: &BtSnapshot) -> &'static str {
    if !snapshot.powered {
        "bluetooth-disabled-symbolic"
    } else if snapshot.devices.iter().any(|d| d.connected) {
        "bluetooth-active-symbolic"
    } else {
        "bluetooth-symbolic"
    }
}

fn rebuild_popover(state: &Rc<RefCell<AppletState>>) {
    let (popover_box, snapshot) = {
        let guard = state.borrow();
        (guard.popover_box.clone(), guard.snapshot.clone())
    };

    while let Some(child) = popover_box.first_child() {
        popover_box.remove(&child);
    }

    let power_row = gtk4::Box::new(Orientation::Horizontal, 8);
    power_row.add_css_class("bar-wifi-switch-row");
    let power_label = gtk4::Label::new(Some("Bluetooth"));
    power_label.set_hexpand(true);
    power_label.set_halign(gtk4::Align::Start);
    let power_switch = gtk4::Switch::new();
    power_switch.set_active(snapshot.powered);
    power_row.append(&power_label);
    power_row.append(&power_switch);
    popover_box.append(&power_row);

    power_switch.connect_state_set({
        let state = state.clone();
        move |_, active| {
            state.borrow().bt.set_powered(active);
            refresh(&state);
            glib::Propagation::Proceed
        }
    });

    if !snapshot.powered {
        return;
    }

    popover_box.append(&gtk4::Separator::new(Orientation::Horizontal));

    let scan_label = if snapshot.discovering { "Stop scanning" } else { "Scan for devices" };
    let scan_button = gtk4::Button::with_label(scan_label);
    popover_box.append(&scan_button);
    scan_button.connect_clicked({
        let state = state.clone();
        let discovering = snapshot.discovering;
        move |_| {
            state.borrow().bt.set_discovering(!discovering);
            refresh(&state);
        }
    });

    if snapshot.devices.is_empty() {
        let empty_text = if snapshot.discovering { "Searching..." } else { "No devices found" };
        let empty_label = gtk4::Label::new(Some(empty_text));
        empty_label.add_css_class("bar-wifi-subtitle");
        popover_box.append(&empty_label);
        return;
    }

    for device in &snapshot.devices {
        let row = device_row(device);
        if device.connected {
            row.add_css_class("connected");
        }
        popover_box.append(&row);

        let device = device.clone();
        row.connect_clicked({
            let state = state.clone();
            move |_| {
                let on_done = {
                    let state = state.clone();
                    move |result: Result<(), glib::Error>| {
                        if let Err(err) = result {
                            eprintln!("jbar: bluetooth action failed: {err}");
                        }
                        refresh(&state);
                    }
                };
                let guard = state.borrow();
                if device.connected {
                    guard.bt.disconnect(&device.path, on_done);
                } else if device.paired {
                    guard.bt.connect(&device.path, on_done);
                } else {
                    guard.bt.pair_and_connect(&device.path, on_done);
                }
            }
        });
    }
}

fn device_row(device: &DeviceInfo) -> gtk4::Button {
    let inner = gtk4::Box::new(Orientation::Vertical, 2);

    let header = gtk4::Box::new(Orientation::Horizontal, 8);
    header.append(&gtk4::Image::from_icon_name("bluetooth-symbolic"));
    let label = gtk4::Label::new(Some(&device.name));
    label.set_hexpand(true);
    label.set_halign(gtk4::Align::Start);
    header.append(&label);
    inner.append(&header);

    let status = if device.connected {
        Some("Connected")
    } else if device.paired {
        Some("Paired")
    } else {
        None
    };
    if let Some(status) = status {
        let status_label = gtk4::Label::new(Some(status));
        status_label.add_css_class("bar-wifi-subtitle");
        status_label.set_halign(gtk4::Align::Start);
        inner.append(&status_label);
    }

    let button = gtk4::Button::new();
    button.set_child(Some(&inner));
    button.add_css_class("bar-wifi-row");
    button
}
