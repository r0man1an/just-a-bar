use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::rc::Rc;

use gtk4::prelude::*;
use gtk4::{glib, Orientation};
use gtk4_layer_shell::{Edge, LayerShell};

use crate::config::PlacesDisplayMode;
use crate::overlay::{build_overlay_window, dismiss_on_escape};

struct AppletState {
    popover_window: gtk4::ApplicationWindow,
    popover_just_dismissed: bool,
    icon_button: gtk4::Button,
    content: gtk4::Widget,
}

pub fn build(
    app: &gtk4::Application,
    bar_height: i32,
    content: &gtk4::Widget,
    display_mode: PlacesDisplayMode,
) -> Option<gtk4::Widget> {
    let places = user_places();
    if places.is_empty() {
        return None;
    }

    let button_content = gtk4::Box::new(Orientation::Horizontal, 6);
    if matches!(display_mode, PlacesDisplayMode::Icon | PlacesDisplayMode::IconAndText) {
        button_content.append(&gtk4::Image::from_icon_name("folder-symbolic"));
    }
    if matches!(display_mode, PlacesDisplayMode::Text | PlacesDisplayMode::IconAndText) {
        button_content.append(&gtk4::Label::new(Some("Places")));
    }

    let icon_button = gtk4::Button::new();
    icon_button.add_css_class("bar-applet-icon");
    icon_button.set_child(Some(&button_content));

    let popover_box = gtk4::Box::new(Orientation::Vertical, 4);
    popover_box.add_css_class("bar-applet-popover");

    let popover_window = build_overlay_window(app, "justabar-places-popover", Edge::Left, &popover_box);
    popover_window.present();
    popover_window.set_visible(false);
    dismiss_on_escape(&popover_window);

    let state = Rc::new(RefCell::new(AppletState {
        popover_window: popover_window.clone(),
        popover_just_dismissed: false,
        icon_button: icon_button.clone(),
        content: content.clone(),
    }));

    for (label, path) in places {
        let row = place_row(&label);
        popover_box.append(&row);

        let state = state.clone();
        row.connect_clicked(move |_| {

            let popover_window = state.borrow().popover_window.clone();
            popover_window.set_visible(false);
            open_folder(&path);
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
            position_popover(&state);
            {
                let popover_window = state.borrow().popover_window.clone();
                popover_window.present();
                popover_window.set_visible(true);
            }

            glib::timeout_add_local_once(std::time::Duration::from_millis(150), {
                let state = state.clone();
                move || {
                    if state.borrow().popover_window.is_visible() {
                        position_popover(&state);
                    }
                }
            });
        }
    });

    Some(icon_button.upcast())
}

fn position_popover(state: &Rc<RefCell<AppletState>>) {
    let guard = state.borrow();
    let (x, _) = guard
        .icon_button
        .translate_coordinates(&guard.content, 0.0, 0.0)
        .unwrap_or((0.0, 0.0));
    guard.popover_window.set_margin(Edge::Left, x.round() as i32);
}

fn place_row(label: &str) -> gtk4::Button {
    let text = gtk4::Label::new(Some(label));
    text.set_halign(gtk4::Align::Start);
    text.set_hexpand(true);

    let button = gtk4::Button::new();
    button.set_child(Some(&text));
    button.add_css_class("bar-wifi-row");
    button
}

fn open_folder(path: &Path) {
    if let Err(err) = Command::new("xdg-open").arg(path).spawn() {
        eprintln!("jbar: failed to launch xdg-open for {path:?} ({err})");
    }
}

fn user_places() -> Vec<(String, PathBuf)> {
    let candidates: [(&str, Option<PathBuf>); 8] = [
        ("Home", dirs::home_dir()),
        ("Desktop", dirs::desktop_dir()),
        ("Documents", dirs::document_dir()),
        ("Downloads", dirs::download_dir()),
        ("Music", dirs::audio_dir()),
        ("Pictures", dirs::picture_dir()),
        ("Videos", dirs::video_dir()),
        ("Public", dirs::public_dir()),
    ];

    candidates
        .into_iter()
        .filter_map(|(label, path)| {
            let path = path?;
            path.is_dir().then_some((label.to_string(), path))
        })
        .collect()
}
