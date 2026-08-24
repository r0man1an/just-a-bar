use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;
use gtk4::{glib, Orientation};
use gtk4_layer_shell::Edge;

use crate::audio;
use crate::overlay::{build_overlay_window, dismiss_on_escape, position_near_icon};
use crate::style;

struct AppletState {
    icon_image: gtk4::Image,
    popover_window: gtk4::ApplicationWindow,
    icon_button: gtk4::Button,
    content: gtk4::Widget,
    volume_scale: gtk4::Scale,
    percent_label: gtk4::Label,
    mute_icon: gtk4::Image,
    updating_from_code: bool,

    source_volume_scale: Option<gtk4::Scale>,
    source_percent_label: Option<gtk4::Label>,
    source_mute_icon: Option<gtk4::Image>,
    source_updating_from_code: bool,

    apps_section: gtk4::Box,
    apps_box: gtk4::Box,

    popover_just_dismissed: bool,
}

pub fn build(app: &gtk4::Application, bar_height: i32, content: &gtk4::Widget) -> Option<gtk4::Widget> {
    if !audio::is_available() {
        eprintln!("jbar: wpctl unavailable; sound applet disabled");
        return None;
    }

    let icon_image = gtk4::Image::new();
    let icon_button = gtk4::Button::new();
    icon_button.add_css_class("bar-applet-icon");
    icon_button.set_child(Some(&icon_image));

    let popover_box = gtk4::Box::new(Orientation::Vertical, 10);
    popover_box.add_css_class("bar-applet-popover");
    popover_box.set_size_request(style::APPLET_POPOVER_WIDTH_PX, -1);

    let (volume_scale, percent_label, mute_icon, mute_button) = build_volume_row(&popover_box, "Volume");

    let source_widgets = if audio::is_source_available() {
        popover_box.append(&gtk4::Separator::new(Orientation::Horizontal));
        Some(build_volume_row(&popover_box, "Microphone"))
    } else {
        None
    };

    let apps_section = gtk4::Box::new(Orientation::Vertical, 8);
    apps_section.append(&gtk4::Separator::new(Orientation::Horizontal));
    let apps_label = gtk4::Label::new(Some("Apps"));
    apps_label.set_halign(gtk4::Align::Start);
    apps_section.append(&apps_label);
    let apps_box = gtk4::Box::new(Orientation::Vertical, 10);
    apps_section.append(&apps_box);
    apps_section.set_visible(false);
    popover_box.append(&apps_section);

    let popover_window = build_overlay_window(app, "justabar-sound-popover", Edge::Right, &popover_box);
    popover_window.present();
    popover_window.set_visible(false);

    dismiss_on_escape(&popover_window);

    let state = Rc::new(RefCell::new(AppletState {
        icon_image,
        popover_window: popover_window.clone(),
        icon_button: icon_button.clone(),
        content: content.clone(),
        volume_scale: volume_scale.clone(),
        percent_label,
        mute_icon,
        updating_from_code: false,
        source_volume_scale: source_widgets.as_ref().map(|(scale, ..)| scale.clone()),
        source_percent_label: source_widgets.as_ref().map(|(_, label, ..)| label.clone()),
        source_mute_icon: source_widgets.as_ref().map(|(_, _, icon, _)| icon.clone()),
        source_updating_from_code: false,
        apps_section,
        apps_box,
        popover_just_dismissed: false,
    }));

    refresh_display(&state);
    rebuild_apps(&state);

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

    mute_button.connect_clicked({
        let state = state.clone();
        move |_| {
            if let Some((_, muted)) = audio::get_volume() {
                audio::set_mute(!muted);
            }
            refresh_display(&state);
        }
    });

    volume_scale.connect_value_changed({
        let state = state.clone();
        move |scale| {
            if state.borrow().updating_from_code {
                return;
            }
            audio::set_volume(scale.value() / 100.0);
            refresh_display(&state);
        }
    });

    if let Some((scale, _, _, mute_button)) = source_widgets {
        mute_button.connect_clicked({
            let state = state.clone();
            move |_| {
                if let Some((_, muted)) = audio::get_source_volume() {
                    audio::set_source_mute(!muted);
                }
                refresh_display(&state);
            }
        });

        scale.connect_value_changed({
            let state = state.clone();
            move |scale| {
                if state.borrow().source_updating_from_code {
                    return;
                }
                audio::set_source_volume(scale.value() / 100.0);
                refresh_display(&state);
            }
        });
    }

    glib::timeout_add_local(std::time::Duration::from_secs(2), {
        let state = state.clone();
        move || {
            refresh_display(&state);
            if state.borrow().popover_window.is_visible() {
                sync_from_system(&state);
            }
            glib::ControlFlow::Continue
        }
    });

    Some(icon_button.upcast())
}

fn build_volume_row(
    container: &gtk4::Box,
    title: &str,
) -> (gtk4::Scale, gtk4::Label, gtk4::Image, gtk4::Button) {
    let label = gtk4::Label::new(Some(title));
    label.set_halign(gtk4::Align::Start);

    let slider_row = gtk4::Box::new(Orientation::Horizontal, 12);

    let mute_icon = gtk4::Image::new();
    let mute_button = gtk4::Button::new();
    mute_button.add_css_class("bar-applet-icon");
    mute_button.set_child(Some(&mute_icon));

    let volume_scale = gtk4::Scale::with_range(Orientation::Horizontal, 0.0, 100.0, 1.0);
    volume_scale.set_hexpand(true);
    volume_scale.set_draw_value(false);

    let percent_label = gtk4::Label::new(None);
    percent_label.set_width_chars(4);

    slider_row.append(&mute_button);
    slider_row.append(&volume_scale);
    slider_row.append(&percent_label);

    container.append(&label);
    container.append(&slider_row);

    (volume_scale, percent_label, mute_icon, mute_button)
}

fn refresh_display(state: &Rc<RefCell<AppletState>>) {
    let Some((volume, muted)) = audio::get_volume() else {
        return;
    };
    let guard = state.borrow();
    guard.icon_image.set_icon_name(Some(volume_icon_name(volume, muted)));
    guard.mute_icon.set_icon_name(Some(mute_icon_name(muted)));
    guard.percent_label.set_text(&format!("{}%", (volume * 100.0).round() as i64));

    if let Some((source_volume, source_muted)) = audio::get_source_volume() {
        if let Some(icon) = &guard.source_mute_icon {
            icon.set_icon_name(Some(mic_icon_name(source_muted)));
        }
        if let Some(label) = &guard.source_percent_label {
            label.set_text(&format!("{}%", (source_volume * 100.0).round() as i64));
        }
    }
}

fn volume_icon_name(volume: f64, muted: bool) -> &'static str {
    if muted || volume <= 0.0 {
        "audio-volume-muted-symbolic"
    } else if volume < 0.34 {
        "audio-volume-low-symbolic"
    } else if volume < 0.67 {
        "audio-volume-medium-symbolic"
    } else {
        "audio-volume-high-symbolic"
    }
}

fn mute_icon_name(muted: bool) -> &'static str {
    if muted {
        "audio-volume-muted-symbolic"
    } else {
        "audio-volume-high-symbolic"
    }
}

fn mic_icon_name(muted: bool) -> &'static str {
    if muted {
        "microphone-sensitivity-muted-symbolic"
    } else {
        "microphone-sensitivity-high-symbolic"
    }
}

fn sync_from_system(state: &Rc<RefCell<AppletState>>) {
    let Some((volume, _muted)) = audio::get_volume() else {
        return;
    };

    state.borrow_mut().updating_from_code = true;
    let scale = state.borrow().volume_scale.clone();
    scale.set_value((volume * 100.0).clamp(0.0, 100.0));
    state.borrow_mut().updating_from_code = false;

    if let Some((source_volume, _)) = audio::get_source_volume() {
        let source_scale = state.borrow().source_volume_scale.clone();
        if let Some(scale) = source_scale {
            state.borrow_mut().source_updating_from_code = true;
            scale.set_value((source_volume * 100.0).clamp(0.0, 100.0));
            state.borrow_mut().source_updating_from_code = false;
        }
    }

    refresh_display(state);
    rebuild_apps(state);
}

fn rebuild_apps(state: &Rc<RefCell<AppletState>>) {
    let (apps_section, apps_box) = {
        let guard = state.borrow();
        (guard.apps_section.clone(), guard.apps_box.clone())
    };

    while let Some(child) = apps_box.first_child() {
        apps_box.remove(&child);
    }

    let streams = audio::list_app_streams();
    apps_section.set_visible(!streams.is_empty());

    for stream in streams {
        let Some((volume, _muted)) = audio::get_stream_volume(stream.id) else {
            continue;
        };

        let row = gtk4::Box::new(Orientation::Vertical, 2);
        let name_label = gtk4::Label::new(Some(&stream.name));
        name_label.set_halign(gtk4::Align::Start);

        let scale = gtk4::Scale::with_range(Orientation::Horizontal, 0.0, 100.0, 1.0);
        scale.set_hexpand(true);
        scale.set_draw_value(false);
        scale.set_value((volume * 100.0).clamp(0.0, 100.0));

        row.append(&name_label);
        row.append(&scale);
        apps_box.append(&row);

        let id = stream.id;
        scale.connect_value_changed(move |scale| {
            audio::set_stream_volume(id, scale.value() / 100.0);
        });
    }
}
