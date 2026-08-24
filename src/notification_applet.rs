use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;
use gtk4::{glib, Orientation};
use gtk4_layer_shell::Edge;

use crate::mako_config;
use crate::notifications::{self, Notification};
use crate::overlay::{build_overlay_window, dismiss_on_escape, position_near_icon};
use crate::style;

struct AppletState {
    icon_image: gtk4::Image,
    icon_button: gtk4::Button,
    content: gtk4::Widget,
    popover_window: gtk4::ApplicationWindow,
    list_box: gtk4::Box,
    empty_label: gtk4::Label,
    dnd_switch: gtk4::Switch,
    updating_dnd_from_code: bool,
    clear_all_button: gtk4::Button,
    popover_just_dismissed: bool,
}

pub fn build(app: &gtk4::Application, content: &gtk4::Widget) -> Option<gtk4::Widget> {
    mako_config::ensure_dnd_mode();

    let icon_image = gtk4::Image::from_icon_name("notification-symbolic");
    let icon_button = gtk4::Button::new();
    icon_button.add_css_class("bar-applet-icon");
    icon_button.set_child(Some(&icon_image));

    let popover_box = gtk4::Box::new(Orientation::Vertical, 10);
    popover_box.add_css_class("bar-applet-popover");
    popover_box.set_size_request(style::APPLET_POPOVER_WIDTH_PX, -1);

    let header_row = gtk4::Box::new(Orientation::Horizontal, 8);
    let header_label = gtk4::Label::new(Some("Do Not Disturb"));
    header_label.set_hexpand(true);
    header_label.set_halign(gtk4::Align::Start);
    let dnd_switch = gtk4::Switch::new();
    header_row.append(&header_label);
    header_row.append(&dnd_switch);
    popover_box.append(&header_row);
    popover_box.append(&gtk4::Separator::new(Orientation::Horizontal));

    let empty_label = gtk4::Label::new(Some("No notifications"));
    empty_label.add_css_class("bar-wifi-subtitle");
    popover_box.append(&empty_label);

    let list_box = gtk4::Box::new(Orientation::Vertical, 6);
    let scroller = gtk4::ScrolledWindow::new();
    scroller.set_policy(gtk4::PolicyType::Never, gtk4::PolicyType::Automatic);
    scroller.set_propagate_natural_height(true);
    scroller.set_max_content_height(320);
    scroller.set_child(Some(&list_box));
    popover_box.append(&scroller);

    let clear_all_button = gtk4::Button::with_label("Clear all");
    clear_all_button.add_css_class("bar-wifi-row");
    popover_box.append(&clear_all_button);

    let popover_window = build_overlay_window(app, "justabar-notification-popover", Edge::Right, &popover_box);
    popover_window.present();
    popover_window.set_visible(false);
    dismiss_on_escape(&popover_window);

    let state = Rc::new(RefCell::new(AppletState {
        icon_image,
        icon_button: icon_button.clone(),
        content: content.clone(),
        popover_window: popover_window.clone(),
        list_box,
        empty_label,
        dnd_switch: dnd_switch.clone(),
        updating_dnd_from_code: false,
        clear_all_button: clear_all_button.clone(),
        popover_just_dismissed: false,
    }));

    sync_dnd_from_system(&state);
    refresh_list(&state);

    dnd_switch.connect_state_set({
        let state = state.clone();
        move |_, active| {
            if state.borrow().updating_dnd_from_code {
                return glib::Propagation::Proceed;
            }
            notifications::set_dnd(active);
            update_icon(&state, active);
            glib::Propagation::Proceed
        }
    });

    clear_all_button.connect_clicked({
        let state = state.clone();
        move |_| {
            let current = notifications::list();
            notifications::dismiss_all(&current);
            refresh_list(&state);
        }
    });

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
            sync_dnd_from_system(&state);
            refresh_list(&state);
            let guard = state.borrow();
            position_near_icon(&guard.popover_window, &guard.icon_button, &guard.content, Edge::Right);
            guard.popover_window.present();
            guard.popover_window.set_visible(true);
        }
    });

    glib::timeout_add_local(std::time::Duration::from_secs(2), {
        let state = state.clone();
        move || {
            if state.borrow().popover_window.is_visible() {
                sync_dnd_from_system(&state);
                refresh_list(&state);
            }
            glib::ControlFlow::Continue
        }
    });

    Some(icon_button.upcast())
}

fn update_icon(state: &Rc<RefCell<AppletState>>, dnd: bool) {
    let icon_name = if dnd {
        "notifications-disabled-symbolic"
    } else {
        "notification-symbolic"
    };
    state.borrow().icon_image.set_icon_name(Some(icon_name));
}

fn sync_dnd_from_system(state: &Rc<RefCell<AppletState>>) {
    let dnd = notifications::dnd_active();
    state.borrow_mut().updating_dnd_from_code = true;
    let switch = state.borrow().dnd_switch.clone();
    switch.set_active(dnd);
    state.borrow_mut().updating_dnd_from_code = false;
    update_icon(state, dnd);
}

fn refresh_list(state: &Rc<RefCell<AppletState>>) {
    let items = notifications::list();

    let (list_box, empty_label, clear_all_button) = {
        let guard = state.borrow();
        (guard.list_box.clone(), guard.empty_label.clone(), guard.clear_all_button.clone())
    };

    while let Some(child) = list_box.first_child() {
        list_box.remove(&child);
    }

    empty_label.set_visible(items.is_empty());
    clear_all_button.set_visible(!items.is_empty());

    for item in items {
        let (row, dismiss_button) = notification_row(&item);
        list_box.append(&row);

        dismiss_button.connect_clicked({
            let state = state.clone();
            move |_| {
                notifications::dismiss(&item);
                refresh_list(&state);
            }
        });
    }
}

fn notification_row(item: &Notification) -> (gtk4::Box, gtk4::Button) {
    let row = gtk4::Box::new(Orientation::Horizontal, 8);
    row.add_css_class("bar-wifi-row");

    let text_box = gtk4::Box::new(Orientation::Vertical, 2);
    text_box.set_hexpand(true);

    let app_name = if item.app_name.is_empty() {
        "Notification"
    } else {
        &item.app_name
    };
    let app_label = gtk4::Label::new(Some(app_name));
    app_label.add_css_class("bar-wifi-subtitle");
    app_label.set_halign(gtk4::Align::Start);
    text_box.append(&app_label);

    let summary_text = if item.summary.is_empty() { &item.body } else { &item.summary };
    let summary_label = gtk4::Label::new(Some(summary_text));
    summary_label.set_halign(gtk4::Align::Start);
    summary_label.set_wrap(true);
    summary_label.set_lines(2);
    summary_label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    text_box.append(&summary_label);

    row.append(&text_box);

    let dismiss_button = gtk4::Button::new();
    dismiss_button.add_css_class("bar-applet-icon");
    dismiss_button.set_child(Some(&gtk4::Image::from_icon_name("window-close-symbolic")));
    row.append(&dismiss_button);

    (row, dismiss_button)
}
