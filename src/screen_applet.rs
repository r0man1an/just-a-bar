use std::cell::RefCell;
use std::os::fd::OwnedFd;
use std::rc::Rc;

use gtk4::prelude::*;
use gtk4::{glib, Orientation};
use gtk4_layer_shell::Edge;

use crate::brightness::{self, BrightnessClient};
use crate::caffeine::{self, CaffeineClient};
use crate::darkmode;
use crate::overlay::{build_overlay_window, dismiss_on_escape, position_near_icon};
use crate::style;
use crate::theme::ColorScheme;

const DURATIONS: [(&str, Option<u64>); 4] = [
    ("15 min", Some(15 * 60)),
    ("30 min", Some(30 * 60)),
    ("1 h", Some(60 * 60)),
    ("∞", None),
];

struct AppletState {
    popover_window: gtk4::ApplicationWindow,
    popover_just_dismissed: bool,
    icon_button: gtk4::Button,
    content: gtk4::Widget,

    brightness: Option<BrightnessClient>,
    brightness_scale: Option<gtk4::Scale>,
    updating_brightness_from_code: bool,

    dark_mode_switch: Option<gtk4::Switch>,
    updating_dark_mode_from_code: bool,
    dark_mode_known: Option<bool>,

    caffeine: Option<CaffeineClient>,
    caffeine_switch: gtk4::Switch,
    duration_row: gtk4::Box,
    duration_buttons: Vec<(Option<u64>, gtk4::Button)>,
    active_lock: Option<OwnedFd>,
    active_timer: Option<glib::SourceId>,
}

pub fn build(app: &gtk4::Application, bar_height: i32, content: &gtk4::Widget) -> Option<gtk4::Widget> {
    let brightness = brightness::init();
    let caffeine = caffeine::init();
    let dark_mode_available = darkmode::is_available();
    if brightness.is_none() && caffeine.is_none() && !dark_mode_available {
        eprintln!("jbar: no backlight, no logind, and no gsettings dark mode; screen applet disabled");
        return None;
    }

    let icon_image = gtk4::Image::from_icon_name("display-brightness-symbolic");
    let icon_button = gtk4::Button::new();
    icon_button.add_css_class("bar-applet-icon");
    icon_button.set_child(Some(&icon_image));

    let popover_box = gtk4::Box::new(Orientation::Vertical, 10);
    popover_box.add_css_class("bar-applet-popover");
    popover_box.set_size_request(style::APPLET_POPOVER_WIDTH_PX, -1);

    let brightness_scale = if brightness.is_some() {
        let label = gtk4::Label::new(Some("Brightness"));
        label.set_halign(gtk4::Align::Start);
        let scale = gtk4::Scale::with_range(Orientation::Horizontal, 0.0, 100.0, 1.0);
        scale.set_hexpand(true);
        popover_box.append(&label);
        popover_box.append(&scale);
        Some(scale)
    } else {
        None
    };

    let dark_mode_switch = if dark_mode_available {
        let row = gtk4::Box::new(Orientation::Horizontal, 8);
        let label = gtk4::Label::new(Some("Dark mode"));
        label.set_hexpand(true);
        label.set_halign(gtk4::Align::Start);
        let switch = gtk4::Switch::new();
        row.append(&label);
        row.append(&switch);
        popover_box.append(&row);
        Some(switch)
    } else {
        None
    };

    if caffeine.is_some() && (brightness_scale.is_some() || dark_mode_available) {
        popover_box.append(&gtk4::Separator::new(Orientation::Horizontal));
    }

    let caffeine_header = gtk4::Box::new(Orientation::Horizontal, 8);
    let caffeine_label = gtk4::Label::new(Some("Caffeine"));
    caffeine_label.set_hexpand(true);
    caffeine_label.set_halign(gtk4::Align::Start);
    let caffeine_switch = gtk4::Switch::new();
    caffeine_header.append(&caffeine_label);
    caffeine_header.append(&caffeine_switch);

    let duration_row = gtk4::Box::new(Orientation::Horizontal, 6);
    let mut duration_buttons = Vec::new();
    for (label, seconds) in DURATIONS {
        let button = gtk4::Button::with_label(label);
        button.add_css_class("bar-wifi-row");
        button.set_hexpand(true);
        duration_row.append(&button);
        duration_buttons.push((seconds, button));
    }
    duration_row.set_visible(false);

    if caffeine.is_some() {
        popover_box.append(&caffeine_header);
        popover_box.append(&duration_row);
    }

    let popover_window = build_overlay_window(app, "justabar-screen-popover", Edge::Right, &popover_box);
    popover_window.present();
    popover_window.set_visible(false);

    dismiss_on_escape(&popover_window);

    let state = Rc::new(RefCell::new(AppletState {
        popover_window: popover_window.clone(),
        popover_just_dismissed: false,
        icon_button: icon_button.clone(),
        content: content.clone(),
        brightness,
        brightness_scale: brightness_scale.clone(),
        updating_brightness_from_code: false,
        dark_mode_switch: dark_mode_switch.clone(),
        updating_dark_mode_from_code: false,
        dark_mode_known: None,
        caffeine,
        caffeine_switch: caffeine_switch.clone(),
        duration_row: duration_row.clone(),
        duration_buttons: duration_buttons.clone(),
        active_lock: None,
        active_timer: None,
    }));

    sync_brightness_from_system(&state);
    sync_dark_mode_from_system(&state);

    if let Some(switch) = &dark_mode_switch {
        switch.connect_state_set({
            let state = state.clone();
            move |_, active| {
                if state.borrow().updating_dark_mode_from_code {
                    return glib::Propagation::Proceed;
                }
                darkmode::set_dark(active);
                state.borrow_mut().dark_mode_known = Some(active);
                glib::Propagation::Proceed
            }
        });
    }

    if let Some(scale) = &brightness_scale {
        scale.connect_value_changed({
            let state = state.clone();
            move |scale| {
                if state.borrow().updating_brightness_from_code {
                    return;
                }
                if let Some(brightness) = &state.borrow().brightness {
                    brightness.set(scale.value() / 100.0);
                }
            }
        });
    }

    caffeine_switch.connect_state_set({
        let state = state.clone();
        move |_, active| {
            if active {
                ensure_lock_active(&state);
                apply_duration(&state, None);
                state.borrow().duration_row.set_visible(true);
            } else {
                cancel_timer(&state);
                clear_duration_highlight(&state);
                let lock = state.borrow_mut().active_lock.take();
                drop(lock);
                state.borrow().duration_row.set_visible(false);
            }
            glib::Propagation::Proceed
        }
    });

    for (seconds, button) in &duration_buttons {
        let seconds = *seconds;
        button.connect_clicked({
            let state = state.clone();
            move |_| {
                let switch = state.borrow().caffeine_switch.clone();
                if !switch.is_active() {
                    switch.set_active(true);
                }
                ensure_lock_active(&state);
                apply_duration(&state, seconds);
            }
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
            sync_brightness_from_system(&state);
            sync_dark_mode_from_system(&state);
            let guard = state.borrow();
            position_near_icon(&guard.popover_window, &guard.icon_button, &guard.content, Edge::Right);
            guard.popover_window.present();
            guard.popover_window.set_visible(true);
        }
    });

    Some(icon_button.upcast())
}

fn sync_brightness_from_system(state: &Rc<RefCell<AppletState>>) {
    let value = {
        let guard = state.borrow();
        let Some(brightness) = &guard.brightness else {
            return;
        };
        brightness.get()
    };
    let Some(value) = value else {
        return;
    };
    state.borrow_mut().updating_brightness_from_code = true;
    let scale = state.borrow().brightness_scale.clone();
    if let Some(scale) = scale {
        scale.set_value((value * 100.0).clamp(0.0, 100.0));
    }
    state.borrow_mut().updating_brightness_from_code = false;
}

fn sync_dark_mode_from_system(state: &Rc<RefCell<AppletState>>) {
    let has_switch = state.borrow().dark_mode_switch.is_some();
    if !has_switch {
        return;
    }

    let known = state.borrow().dark_mode_known;
    let is_dark = match known {
        Some(known) => known,
        None => darkmode::get() == ColorScheme::Dark,
    };

    state.borrow_mut().updating_dark_mode_from_code = true;
    let switch = state.borrow().dark_mode_switch.clone();
    if let Some(switch) = switch {
        switch.set_active(is_dark);
    }
    state.borrow_mut().updating_dark_mode_from_code = false;
}

fn ensure_lock_active(state: &Rc<RefCell<AppletState>>) {
    let (needs_lock, caffeine) = {
        let guard = state.borrow();
        (guard.active_lock.is_none(), guard.caffeine.clone())
    };
    if !needs_lock {
        return;
    }
    let Some(caffeine) = caffeine else {
        return;
    };
    let Some(lock) = caffeine.inhibit() else {
        return;
    };
    state.borrow_mut().active_lock = Some(lock);
}

fn apply_duration(state: &Rc<RefCell<AppletState>>, seconds: Option<u64>) {
    cancel_timer(state);
    highlight_duration(state, seconds);

    if let Some(seconds) = seconds {
        let source_id = glib::timeout_add_local(std::time::Duration::from_secs(seconds), {
            let state = state.clone();
            move || {
                state.borrow_mut().active_timer = None;
                let switch = state.borrow().caffeine_switch.clone();
                switch.set_active(false);
                glib::ControlFlow::Break
            }
        });
        state.borrow_mut().active_timer = Some(source_id);
    }
}

fn cancel_timer(state: &Rc<RefCell<AppletState>>) {
    let timer = state.borrow_mut().active_timer.take();
    if let Some(timer) = timer {
        timer.remove();
    }
}

fn highlight_duration(state: &Rc<RefCell<AppletState>>, active: Option<u64>) {
    let guard = state.borrow();
    for (seconds, button) in &guard.duration_buttons {
        if *seconds == active {
            button.add_css_class("connected");
        } else {
            button.remove_css_class("connected");
        }
    }
}

fn clear_duration_highlight(state: &Rc<RefCell<AppletState>>) {
    let guard = state.borrow();
    for (_, button) in &guard.duration_buttons {
        button.remove_css_class("connected");
    }
}
