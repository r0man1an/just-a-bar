use std::cell::RefCell;
use std::rc::Rc;

use calloop::channel::Sender as CalloopSender;
use gtk4::prelude::*;
use gtk4::{glib, Orientation};
use gtk4_layer_shell::{Edge, LayerShell};

use crate::keyboard_backlight::{self, KeyboardBacklightClient};
use crate::keyboard_layout;
use crate::overlay::{build_overlay_window, dismiss_on_escape, position_near_icon};
use crate::style;

#[derive(Clone, Copy, PartialEq)]
enum Level {
    Low,
    High,
}

impl Level {
    fn value(self, max: u32) -> u32 {
        match self {
            Level::Low => (max / 2).max(1).min(max),
            Level::High => max,
        }
    }
}

struct AppletState {
    popover_window: gtk4::ApplicationWindow,
    popover_just_dismissed: bool,
    icon_button: gtk4::Button,
    content: gtk4::Widget,
    backlight: KeyboardBacklightClient,
    backlight_switch: gtk4::Switch,
    level_row: gtk4::Box,
    low_button: gtk4::Button,
    high_button: gtk4::Button,
    updating_from_code: bool,
    layout_cmd_tx: Option<CalloopSender<keyboard_layout::Command>>,
    layout_buttons: Vec<gtk4::Button>,
    current_layout: usize,
}

pub fn build(
    app: &gtk4::Application,
    bar_height: i32,
    content: &gtk4::Widget,
    xkb_layouts: Vec<String>,
    layout_cmd_tx: Option<CalloopSender<keyboard_layout::Command>>,
) -> Option<gtk4::Widget> {
    let backlight = keyboard_backlight::init()?;

    let icon_image = gtk4::Image::from_icon_name("input-keyboard-symbolic");
    let icon_button = gtk4::Button::new();
    icon_button.add_css_class("bar-applet-icon");
    icon_button.set_child(Some(&icon_image));

    let popover_box = gtk4::Box::new(Orientation::Vertical, 10);
    popover_box.add_css_class("bar-applet-popover");
    popover_box.set_size_request(style::APPLET_POPOVER_WIDTH_PX, -1);

    let header_row = gtk4::Box::new(Orientation::Horizontal, 8);
    let label = gtk4::Label::new(Some("Keyboard Backlight"));
    label.set_hexpand(true);
    label.set_halign(gtk4::Align::Start);
    let backlight_switch = gtk4::Switch::new();
    header_row.append(&label);
    header_row.append(&backlight_switch);
    popover_box.append(&header_row);

    let level_row = gtk4::Box::new(Orientation::Horizontal, 6);
    let low_button = gtk4::Button::with_label("Low");
    let high_button = gtk4::Button::with_label("High");
    low_button.add_css_class("bar-wifi-row");
    high_button.add_css_class("bar-wifi-row");
    low_button.set_hexpand(true);
    high_button.set_hexpand(true);
    level_row.append(&low_button);
    level_row.append(&high_button);
    popover_box.append(&level_row);

    let mut layout_buttons = Vec::new();
    if !xkb_layouts.is_empty() {
        popover_box.append(&gtk4::Separator::new(Orientation::Horizontal));
        let layouts_label = gtk4::Label::new(Some("Layout"));
        layouts_label.set_halign(gtk4::Align::Start);
        popover_box.append(&layouts_label);

        let layouts_row = gtk4::Box::new(Orientation::Horizontal, 6);
        for code in &xkb_layouts {
            let button = gtk4::Button::with_label(code);
            button.add_css_class("bar-wifi-row");
            button.set_hexpand(true);
            layouts_row.append(&button);
            layout_buttons.push(button);
        }
        popover_box.append(&layouts_row);
        if let Some(first) = layout_buttons.first() {
            first.add_css_class("connected");
        }
    }

    let popover_window =
        build_overlay_window(app, "justabar-keyboard-popover", Edge::Right, &popover_box);
    popover_window.present();
    popover_window.set_visible(false);
    dismiss_on_escape(&popover_window);

    let state = Rc::new(RefCell::new(AppletState {
        popover_window: popover_window.clone(),
        popover_just_dismissed: false,
        icon_button: icon_button.clone(),
        content: content.clone(),
        backlight,
        backlight_switch: backlight_switch.clone(),
        level_row: level_row.clone(),
        low_button: low_button.clone(),
        high_button: high_button.clone(),
        updating_from_code: false,
        layout_cmd_tx,
        layout_buttons: layout_buttons.clone(),
        current_layout: 0,
    }));

    sync_from_system(&state);

    for (index, button) in layout_buttons.iter().enumerate() {
        button.connect_clicked({
            let state = state.clone();
            move |_| switch_layout(&state, index)
        });
    }

    backlight_switch.connect_state_set({
        let state = state.clone();
        move |switch, active| {
            if state.borrow().updating_from_code {
                return glib::Propagation::Proceed;
            }
            if active {
                apply_level(&state, Level::High);
            } else {
                state.borrow().backlight.set(0);
                highlight_level(&state, None);
            }
            state.borrow().level_row.set_visible(active);
            switch.set_state(active);
            glib::Propagation::Stop
        }
    });

    low_button.connect_clicked({
        let state = state.clone();
        move |_| {
            let switch = state.borrow().backlight_switch.clone();
            if !switch.is_active() {
                switch.set_active(true);
            }
            apply_level(&state, Level::Low);
        }
    });

    high_button.connect_clicked({
        let state = state.clone();
        move |_| {
            let switch = state.borrow().backlight_switch.clone();
            if !switch.is_active() {
                switch.set_active(true);
            }
            apply_level(&state, Level::High);
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
            sync_from_system(&state);
            let guard = state.borrow();
            position_near_icon(&guard.popover_window, &guard.icon_button, &guard.content, Edge::Right);
            guard.popover_window.present();
            guard.popover_window.set_visible(true);
        }
    });

    Some(icon_button.upcast())
}

fn switch_layout(state: &Rc<RefCell<AppletState>>, index: usize) {
    let mut guard = state.borrow_mut();
    if index == guard.current_layout {
        return;
    }
    if let Some(tx) = &guard.layout_cmd_tx {
        let _ = tx.send(keyboard_layout::Command::SwitchToGroup(index as u32));
    }
    for (i, button) in guard.layout_buttons.iter().enumerate() {
        if i == index {
            button.add_css_class("connected");
        } else {
            button.remove_css_class("connected");
        }
    }
    guard.current_layout = index;
}

fn apply_level(state: &Rc<RefCell<AppletState>>, level: Level) {
    let (max, backlight_switch) = {
        let guard = state.borrow();
        (guard.backlight.max(), guard.backlight_switch.clone())
    };
    state.borrow().backlight.set(level.value(max));

    state.borrow_mut().updating_from_code = true;
    backlight_switch.set_active(true);
    state.borrow_mut().updating_from_code = false;

    highlight_level(state, Some(level));
}

fn highlight_level(state: &Rc<RefCell<AppletState>>, active: Option<Level>) {
    let guard = state.borrow();
    guard.low_button.remove_css_class("connected");
    guard.high_button.remove_css_class("connected");
    match active {
        Some(Level::Low) => guard.low_button.add_css_class("connected"),
        Some(Level::High) => guard.high_button.add_css_class("connected"),
        None => {}
    }
}

fn sync_from_system(state: &Rc<RefCell<AppletState>>) {
    let (current, max) = {
        let guard = state.borrow();
        (guard.backlight.get().unwrap_or(0), guard.backlight.max())
    };
    let level = if current == 0 {
        None
    } else if current <= Level::Low.value(max) {
        Some(Level::Low)
    } else {
        Some(Level::High)
    };

    state.borrow_mut().updating_from_code = true;
    let switch = state.borrow().backlight_switch.clone();
    switch.set_active(current > 0);
    state.borrow_mut().updating_from_code = false;

    state.borrow().level_row.set_visible(current > 0);
    highlight_level(state, level);
}
