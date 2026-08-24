use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;
use gtk4::Orientation;
use gtk4_layer_shell::Edge;

use crate::overlay::{build_overlay_window, dismiss_on_escape, position_near_icon};
use crate::power::{self, PowerClient};

struct AppletState {
    popover_window: gtk4::ApplicationWindow,
    popover_just_dismissed: bool,
    icon_button: gtk4::Button,
    content: gtk4::Widget,
}

pub fn build(app: &gtk4::Application, bar_height: i32, content: &gtk4::Widget) -> Option<gtk4::Widget> {
    let power = power::init()?;

    let icon_image = gtk4::Image::from_icon_name("system-shutdown-symbolic");
    let icon_button = gtk4::Button::new();
    icon_button.add_css_class("bar-applet-icon");
    icon_button.set_child(Some(&icon_image));

    let popover_box = gtk4::Box::new(Orientation::Vertical, 4);
    popover_box.add_css_class("bar-applet-popover");

    let popover_window = build_overlay_window(app, "justabar-power-popover", Edge::Right, &popover_box);
    popover_window.present();
    popover_window.set_visible(false);
    dismiss_on_escape(&popover_window);

    let state = Rc::new(RefCell::new(AppletState {
        popover_window: popover_window.clone(),
        popover_just_dismissed: false,
        icon_button: icon_button.clone(),
        content: content.clone(),
    }));

    let actions: [(&str, fn(&PowerClient)); 4] = [
        ("Suspend", PowerClient::suspend),
        ("Restart", PowerClient::reboot),
        ("Power Off", PowerClient::power_off),
        ("Log Out", PowerClient::log_out),
    ];
    for (label, action) in actions {
        let row = action_row(label);
        popover_box.append(&row);

        let power = power.clone();
        let state = state.clone();
        row.connect_clicked(move |_| {

            let popover_window = state.borrow().popover_window.clone();
            popover_window.set_visible(false);
            action(&power);
        });
    }
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
            let guard = state.borrow();
            position_near_icon(&guard.popover_window, &guard.icon_button, &guard.content, Edge::Right);
            guard.popover_window.present();
            guard.popover_window.set_visible(true);
        }
    });

    Some(icon_button.upcast())
}

fn action_row(label: &str) -> gtk4::Button {
    let text = gtk4::Label::new(Some(label));
    text.set_halign(gtk4::Align::Start);
    text.set_hexpand(true);

    let button = gtk4::Button::new();
    button.set_child(Some(&text));
    button.add_css_class("bar-wifi-row");
    button
}
