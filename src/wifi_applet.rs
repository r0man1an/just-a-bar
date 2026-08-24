use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;
use gtk4::{glib, Orientation};
use gtk4_layer_shell::Edge;

use crate::networkmanager::{self, ApInfo, NmClient, NmSnapshot};
use crate::overlay::{build_overlay_window, dismiss_on_escape, dismiss_on_focus_loss, position_near_icon};
use crate::style;

struct AppletState {
    nm: NmClient,
    snapshot: NmSnapshot,
    show_all: bool,
    pending_ap: Option<ApInfo>,
    icon_image: gtk4::Image,
    icon_button: gtk4::Button,
    content: gtk4::Widget,
    popover_box: gtk4::Box,
    popover_window: gtk4::ApplicationWindow,
    password_window: gtk4::ApplicationWindow,
    password_ssid_label: gtk4::Label,
    password_entry: gtk4::PasswordEntry,
    password_error_label: gtk4::Label,
    popover_just_dismissed: bool,
}

pub fn build(app: &gtk4::Application, bar_height: i32, content: &gtk4::Widget) -> Option<gtk4::Widget> {
    let nm = networkmanager::init()?;

    let icon_image = gtk4::Image::new();
    let icon_button = gtk4::Button::new();
    icon_button.add_css_class("bar-applet-icon");
    icon_button.set_child(Some(&icon_image));

    let popover_box = gtk4::Box::new(Orientation::Vertical, 10);
    popover_box.add_css_class("bar-applet-popover");
    popover_box.set_size_request(style::APPLET_POPOVER_WIDTH_PX, -1);

    let popover_window = build_overlay_window(app, "justabar-wifi-popover", Edge::Right, &popover_box);
    popover_window.present();
    popover_window.set_visible(false);

    let password_box = gtk4::Box::new(Orientation::Vertical, 10);
    password_box.add_css_class("bar-applet-popover");
    password_box.set_size_request(style::APPLET_POPOVER_WIDTH_PX, -1);

    let password_ssid_label = gtk4::Label::new(None);
    password_ssid_label.set_halign(gtk4::Align::Start);
    let password_entry = gtk4::PasswordEntry::new();
    password_entry.set_show_peek_icon(true);
    let password_error_label = gtk4::Label::new(None);
    password_error_label.add_css_class("bar-wifi-error");
    password_error_label.set_visible(false);
    let password_button_row = gtk4::Box::new(Orientation::Horizontal, 8);
    password_button_row.set_halign(gtk4::Align::End);
    let password_cancel = gtk4::Button::with_label("Cancel");
    let password_connect = gtk4::Button::with_label("Connect");
    password_connect.add_css_class("suggested-action");
    password_button_row.append(&password_cancel);
    password_button_row.append(&password_connect);

    password_box.append(&password_ssid_label);
    password_box.append(&password_entry);
    password_box.append(&password_error_label);
    password_box.append(&password_button_row);

    let password_window = build_overlay_window(app, "justabar-wifi-password", Edge::Right, &password_box);
    password_window.present();
    password_window.set_visible(false);

    dismiss_on_focus_loss(&password_window);
    dismiss_on_escape(&popover_window);
    dismiss_on_escape(&password_window);

    let initial_snapshot = nm.snapshot();
    let state = Rc::new(RefCell::new(AppletState {
        nm,
        snapshot: initial_snapshot,
        show_all: false,
        pending_ap: None,
        icon_image,
        icon_button: icon_button.clone(),
        content: content.clone(),
        popover_box,
        popover_window: popover_window.clone(),
        password_window: password_window.clone(),
        password_ssid_label,
        password_entry: password_entry.clone(),
        password_error_label,
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

            state.borrow_mut().show_all = false;
            refresh(&state);
            let guard = state.borrow();
            position_near_icon(&guard.popover_window, &guard.icon_button, &guard.content, Edge::Right);
            guard.popover_window.present();
            guard.popover_window.set_visible(true);
        }
    });

    password_cancel.connect_clicked({
        let state = state.clone();
        move |_| {
            state.borrow().password_window.set_visible(false);
        }
    });

    password_connect.connect_clicked({
        let state = state.clone();
        move |_| {
            let (ap, password) = {
                let guard = state.borrow();
                (guard.pending_ap.clone(), guard.password_entry.text().to_string())
            };
            if let Some(ap) = ap {
                if password.is_empty() {
                    state.borrow().password_error_label.set_text("Enter a password");
                    state.borrow().password_error_label.set_visible(true);
                    return;
                }
                state.borrow().nm.connect_secured(&ap, &password);
            }
            state.borrow().password_window.set_visible(false);
            refresh(&state);
        }
    });

    password_entry.connect_activate({
        let password_connect = password_connect.clone();
        move |_| password_connect.emit_clicked()
    });

    {
        let state_for_subscribe = state.clone();
        state.borrow().nm.subscribe(move || {
            refresh(&state_for_subscribe);
        });
    }

    Some(icon_button.upcast())
}

fn refresh(state: &Rc<RefCell<AppletState>>) {
    let new_snapshot = state.borrow().nm.snapshot();
    state.borrow_mut().snapshot = new_snapshot;
    update_icon(state);
    rebuild_popover(state);
}

fn update_icon(state: &Rc<RefCell<AppletState>>) {
    let guard = state.borrow();
    guard.icon_image.set_icon_name(Some(icon_name_for(&guard.snapshot)));
}

fn icon_name_for(snapshot: &NmSnapshot) -> &'static str {
    if snapshot.airplane_mode {
        "airplane-mode-symbolic"
    } else if let Some(ap) = &snapshot.connected_ap {
        signal_icon_name(ap.strength)
    } else if snapshot.wired_connected {
        "network-wired-symbolic"
    } else if snapshot.wifi_enabled {
        "network-wireless-signal-none-symbolic"
    } else {
        "network-wireless-disabled-symbolic"
    }
}

fn signal_icon_name(strength: u8) -> &'static str {
    match strength {
        80..=100 => "network-wireless-signal-excellent-symbolic",
        55..=79 => "network-wireless-signal-good-symbolic",
        30..=54 => "network-wireless-signal-ok-symbolic",
        1..=29 => "network-wireless-signal-weak-symbolic",
        _ => "network-wireless-signal-none-symbolic",
    }
}

fn rebuild_popover(state: &Rc<RefCell<AppletState>>) {
    let (popover_box, snapshot, show_all) = {
        let guard = state.borrow();
        (guard.popover_box.clone(), guard.snapshot.clone(), guard.show_all)
    };

    while let Some(child) = popover_box.first_child() {
        popover_box.remove(&child);
    }

    let airplane_row = gtk4::Box::new(Orientation::Horizontal, 8);
    airplane_row.add_css_class("bar-wifi-switch-row");
    let airplane_label = gtk4::Label::new(Some("Airplane mode"));
    airplane_label.set_hexpand(true);
    airplane_label.set_halign(gtk4::Align::Start);
    let airplane_switch = gtk4::Switch::new();
    airplane_switch.set_active(snapshot.airplane_mode);
    airplane_row.append(&airplane_label);
    airplane_row.append(&airplane_switch);
    popover_box.append(&airplane_row);

    let wifi_row = gtk4::Box::new(Orientation::Horizontal, 8);
    wifi_row.add_css_class("bar-wifi-switch-row");
    let wifi_label = gtk4::Label::new(Some("Wi-Fi"));
    wifi_label.set_hexpand(true);
    wifi_label.set_halign(gtk4::Align::Start);
    let wifi_switch = gtk4::Switch::new();
    wifi_switch.set_active(snapshot.wifi_enabled);
    wifi_switch.set_sensitive(!snapshot.airplane_mode);
    wifi_row.append(&wifi_label);
    wifi_row.append(&wifi_switch);
    popover_box.append(&wifi_row);

    airplane_switch.connect_state_set({
        let state = state.clone();
        move |_, active| {
            state.borrow().nm.set_airplane_mode(active);
            refresh(&state);
            glib::Propagation::Proceed
        }
    });
    wifi_switch.connect_state_set({
        let state = state.clone();
        move |_, active| {
            state.borrow().nm.set_wifi_enabled(active);
            refresh(&state);
            glib::Propagation::Proceed
        }
    });

    if snapshot.has_wired {
        let wired_row = gtk4::Box::new(Orientation::Horizontal, 8);
        wired_row.add_css_class("bar-wifi-row");
        let text = if snapshot.wired_connected {
            "Ethernet: Connected"
        } else {
            "Ethernet: Not connected"
        };
        let label = gtk4::Label::new(Some(text));
        label.set_halign(gtk4::Align::Start);
        wired_row.append(&label);
        popover_box.append(&wired_row);
    }

    if let Some(vpn) = &snapshot.vpn {
        let vpn_row = gtk4::Box::new(Orientation::Vertical, 2);
        vpn_row.add_css_class("bar-wifi-switch-row");

        let vpn_header = gtk4::Box::new(Orientation::Horizontal, 8);
        let vpn_label = gtk4::Label::new(Some("VPN"));
        vpn_label.set_hexpand(true);
        vpn_label.set_halign(gtk4::Align::Start);
        let vpn_switch = gtk4::Switch::new();
        vpn_switch.set_active(true);
        vpn_header.append(&vpn_label);
        vpn_header.append(&vpn_switch);

        let vpn_name_label = gtk4::Label::new(Some(&vpn.name));
        vpn_name_label.add_css_class("bar-wifi-subtitle");
        vpn_name_label.set_halign(gtk4::Align::Start);

        vpn_row.append(&vpn_header);
        vpn_row.append(&vpn_name_label);
        popover_box.append(&vpn_row);

        vpn_switch.connect_state_set({
            let state = state.clone();
            let path = vpn.active_connection_path.clone();
            move |_, active| {
                if !active {
                    state.borrow().nm.disconnect_vpn(&path);
                    refresh(&state);
                }
                glib::Propagation::Proceed
            }
        });
    }

    if snapshot.airplane_mode || !snapshot.wifi_enabled {
        return;
    }

    popover_box.append(&gtk4::Separator::new(Orientation::Horizontal));

    if let (Some(connected), false) = (&snapshot.connected_ap, show_all) {
        let row = network_row(connected);
        row.add_css_class("connected");
        popover_box.append(&row);

        let button_row = gtk4::Box::new(Orientation::Horizontal, 10);
        let show_all_button = gtk4::Button::with_label("Show all networks");
        let disconnect_button = gtk4::Button::with_label("Disconnect");
        show_all_button.set_hexpand(true);
        disconnect_button.set_hexpand(true);
        button_row.append(&show_all_button);
        button_row.append(&disconnect_button);
        popover_box.append(&button_row);

        show_all_button.connect_clicked({
            let state = state.clone();
            move |_| {
                state.borrow_mut().show_all = true;
                rebuild_popover(&state);
            }
        });
        disconnect_button.connect_clicked({
            let state = state.clone();
            move |_| {
                state.borrow().nm.disconnect();
                state.borrow_mut().show_all = false;
                refresh(&state);
            }
        });
    } else {
        for ap in &snapshot.access_points {
            let row = network_row(ap);
            if Some(ap.path.as_str()) == snapshot.connected_ap.as_ref().map(|a| a.path.as_str()) {
                row.add_css_class("connected");
            }
            popover_box.append(&row);

            let ap = ap.clone();
            row.connect_clicked({
                let state = state.clone();
                move |_| {
                    let already_saved = state.borrow().nm.try_connect_saved(&ap).is_some();
                    if already_saved {
                        state.borrow_mut().show_all = false;
                        refresh(&state);
                    } else if ap.secured {
                        open_password_popover(&state, ap.clone());
                    } else {
                        state.borrow().nm.connect_open(&ap);
                        state.borrow_mut().show_all = false;
                        refresh(&state);
                    }
                }
            });
        }
    }
}

fn network_row(ap: &ApInfo) -> gtk4::Button {
    let inner = gtk4::Box::new(Orientation::Horizontal, 8);
    inner.append(&gtk4::Image::from_icon_name(signal_icon_name(ap.strength)));
    let label = gtk4::Label::new(Some(&ap.ssid));
    label.set_hexpand(true);
    label.set_halign(gtk4::Align::Start);
    inner.append(&label);
    if ap.secured {
        inner.append(&gtk4::Image::from_icon_name("network-wireless-encrypted-symbolic"));
    }

    let button = gtk4::Button::new();
    button.set_child(Some(&inner));
    button.add_css_class("bar-wifi-row");
    button
}

fn open_password_popover(state: &Rc<RefCell<AppletState>>, ap: ApInfo) {
    let (popover_window, password_window, icon_button, content) = {
        let mut guard = state.borrow_mut();
        guard.password_ssid_label.set_text(&ap.ssid);
        guard.password_entry.set_text("");
        guard.password_error_label.set_visible(false);
        guard.pending_ap = Some(ap);
        (
            guard.popover_window.clone(),
            guard.password_window.clone(),
            guard.icon_button.clone(),
            guard.content.clone(),
        )
    };
    popover_window.set_visible(false);
    position_near_icon(&password_window, &icon_button, &content, Edge::Right);
    password_window.present();
    password_window.set_visible(true);
}
