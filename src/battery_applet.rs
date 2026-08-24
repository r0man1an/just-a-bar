use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use gtk4::prelude::*;
use gtk4::{glib, Orientation};
use gtk4_layer_shell::Edge;

use crate::battery::{self, BatteryStatus, ChargeState};
use crate::overlay::{build_overlay_window, dismiss_on_escape, position_near_icon};
use crate::power_profiles::{self, PowerProfilesClient};
use crate::style;

struct AppletState {
    battery_path: PathBuf,
    icon_image: gtk4::Image,
    percentage_label: Option<gtk4::Label>,
    popover_window: gtk4::ApplicationWindow,
    status_label: gtk4::Label,
    popover_just_dismissed: bool,
    ppd: Option<PowerProfilesClient>,
    profile_buttons: Vec<(String, gtk4::Button)>,
    icon_button: gtk4::Button,
    content: gtk4::Widget,
}

pub fn build(
    app: &gtk4::Application,
    bar_height: i32,
    content: &gtk4::Widget,
    show_percentage: bool,
) -> Option<gtk4::Widget> {
    let battery_path = battery::find_battery()?;
    battery::read(&battery_path)?;

    let icon_image = gtk4::Image::new();
    let icon_button = gtk4::Button::new();
    icon_button.add_css_class("bar-applet-icon");

    let percentage_label = if show_percentage {
        let icon_row = gtk4::Box::new(Orientation::Horizontal, 4);
        let percentage_label = gtk4::Label::new(None);
        icon_row.append(&icon_image);
        icon_row.append(&percentage_label);
        icon_button.set_child(Some(&icon_row));
        Some(percentage_label)
    } else {
        icon_button.set_child(Some(&icon_image));
        None
    };

    let popover_box = gtk4::Box::new(Orientation::Vertical, 10);
    popover_box.add_css_class("bar-applet-popover");
    popover_box.set_size_request(style::APPLET_POPOVER_WIDTH_PX, -1);

    let title = gtk4::Label::new(Some("Battery"));
    title.set_halign(gtk4::Align::Start);
    let status_label = gtk4::Label::new(None);
    status_label.set_halign(gtk4::Align::Start);
    status_label.add_css_class("bar-wifi-subtitle");

    popover_box.append(&title);
    popover_box.append(&status_label);

    let ppd = power_profiles::init();
    let ppd_for_subscribe = ppd.clone();
    let profiles = ppd.as_ref().map(|p| p.available_profiles()).unwrap_or_default();

    let mut profile_buttons = Vec::new();
    if !profiles.is_empty() {
        let profile_row = gtk4::Box::new(Orientation::Horizontal, 6);
        for id in &profiles {
            let button = gtk4::Button::with_label(&profile_title(id));
            button.add_css_class("bar-wifi-row");
            button.set_hexpand(true);
            profile_row.append(&button);
            profile_buttons.push((id.clone(), button));
        }
        popover_box.append(&profile_row);
    }

    let popover_window = build_overlay_window(app, "justabar-battery-popover", Edge::Right, &popover_box);
    popover_window.present();
    popover_window.set_visible(false);

    dismiss_on_escape(&popover_window);

    let state = Rc::new(RefCell::new(AppletState {
        battery_path,
        icon_image,
        percentage_label,
        popover_window: popover_window.clone(),
        status_label,
        popover_just_dismissed: false,
        ppd,
        profile_buttons,
        icon_button: icon_button.clone(),
        content: content.clone(),
    }));

    refresh(&state);
    refresh_profile(&state);

    for (id, button) in &state.borrow().profile_buttons {
        let id = id.clone();
        button.connect_clicked({
            let state = state.clone();
            move |_| {
                if let Some(ppd) = &state.borrow().ppd {
                    ppd.set_active_profile(&id);
                }
                refresh_profile(&state);
            }
        });
    }

    if let Some(ppd) = ppd_for_subscribe {
        let state_for_subscribe = state.clone();
        ppd.subscribe(move || {
            refresh_profile(&state_for_subscribe);
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
            refresh(&state);
            refresh_profile(&state);
            let guard = state.borrow();
            position_near_icon(&guard.popover_window, &guard.icon_button, &guard.content, Edge::Right);
            guard.popover_window.present();
            guard.popover_window.set_visible(true);
        }
    });

    glib::timeout_add_local(std::time::Duration::from_secs(2), {
        let state = state.clone();
        move || {
            refresh(&state);
            glib::ControlFlow::Continue
        }
    });

    Some(icon_button.upcast())
}

fn refresh(state: &Rc<RefCell<AppletState>>) {
    let guard = state.borrow();
    let Some(status) = battery::read(&guard.battery_path) else {
        return;
    };
    guard.icon_image.set_icon_name(Some(&icon_name_for(status)));
    guard.status_label.set_text(&status_text(status));
    if let Some(percentage_label) = &guard.percentage_label {
        percentage_label.set_text(&format!("{}%", status.percentage));
    }
}

fn refresh_profile(state: &Rc<RefCell<AppletState>>) {
    let guard = state.borrow();
    let Some(ppd) = &guard.ppd else {
        return;
    };
    let Some(active) = ppd.active_profile() else {
        return;
    };
    for (id, button) in &guard.profile_buttons {
        if *id == active {
            button.add_css_class("connected");
        } else {
            button.remove_css_class("connected");
        }
    }
}

fn profile_title(id: &str) -> String {
    let mut title = id.replace('-', " ");
    if let Some(first) = title.get_mut(0..1) {
        first.make_ascii_uppercase();
    }
    title
}

fn icon_name_for(status: BatteryStatus) -> String {
    let level = match status.percentage {
        0..=19 => "caution",
        20..=39 => "low",
        40..=79 => "good",
        _ => "full",
    };
    if status.state == ChargeState::Charging {
        format!("battery-{level}-charging-symbolic")
    } else {
        format!("battery-{level}-symbolic")
    }
}

fn status_text(status: BatteryStatus) -> String {
    let state = match status.state {
        ChargeState::Charging => "Charging",
        ChargeState::Discharging => "Discharging",
        ChargeState::Full => "Full",
        ChargeState::Other => "Not charging",
    };
    format!("{}% - {state}", status.percentage)
}
