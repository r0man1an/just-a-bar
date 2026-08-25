use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;
use gtk4::{glib, Orientation};
use gtk4_layer_shell::Edge;

use crate::clipboard;
use crate::overlay::{build_overlay_window, dismiss_on_escape, position_near_icon};
use crate::style;

const MAX_HISTORY: usize = 100;
const PREVIEW_MAX_CHARS: usize = 40;

struct ClipEntry {
    text: String,
    pinned: bool,
}

struct AppletState {
    icon_button: gtk4::Button,
    content: gtk4::Widget,
    popover_window: gtk4::ApplicationWindow,
    list_box: gtk4::Box,
    empty_label: gtk4::Label,
    clear_button: gtk4::Button,
    popover_just_dismissed: bool,
    private: bool,
    entries: Vec<ClipEntry>,
}

pub fn build(app: &gtk4::Application, content: &gtk4::Widget) -> Option<gtk4::Widget> {
    let icon_image = gtk4::Image::from_icon_name("edit-paste-symbolic");
    let icon_button = gtk4::Button::new();
    icon_button.add_css_class("bar-applet-icon");
    icon_button.set_child(Some(&icon_image));

    let popover_box = gtk4::Box::new(Orientation::Vertical, 10);
    popover_box.add_css_class("bar-applet-popover");
    popover_box.set_size_request(style::APPLET_POPOVER_WIDTH_PX, -1);

    let header_row = gtk4::Box::new(Orientation::Horizontal, 8);
    let header_label = gtk4::Label::new(Some("Private mode"));
    header_label.set_hexpand(true);
    header_label.set_halign(gtk4::Align::Start);
    let private_switch = gtk4::Switch::new();
    header_row.append(&header_label);
    header_row.append(&private_switch);
    popover_box.append(&header_row);
    popover_box.append(&gtk4::Separator::new(Orientation::Horizontal));

    let empty_label = gtk4::Label::new(Some("No clipboard history"));
    empty_label.add_css_class("bar-wifi-subtitle");
    popover_box.append(&empty_label);

    let list_box = gtk4::Box::new(Orientation::Vertical, 6);
    let scroller = gtk4::ScrolledWindow::new();
    scroller.set_policy(gtk4::PolicyType::Never, gtk4::PolicyType::Automatic);
    scroller.set_propagate_natural_height(true);
    scroller.set_max_content_height(320);
    scroller.set_child(Some(&list_box));
    popover_box.append(&scroller);

    let clear_button = gtk4::Button::with_label("Clear");
    clear_button.add_css_class("bar-wifi-row");
    popover_box.append(&clear_button);

    let popover_window =
        build_overlay_window(app, "justabar-clipboard-popover", Edge::Right, &popover_box);
    popover_window.present();
    popover_window.set_visible(false);
    dismiss_on_escape(&popover_window);

    let state = Rc::new(RefCell::new(AppletState {
        icon_button: icon_button.clone(),
        content: content.clone(),
        popover_window: popover_window.clone(),
        list_box,
        empty_label,
        clear_button: clear_button.clone(),
        popover_just_dismissed: false,
        private: false,
        entries: Vec::new(),
    }));

    refresh_list(&state);

    let (events_tx, events_rx) = async_channel::unbounded::<String>();
    clipboard::spawn(events_tx);

    glib::spawn_future_local({
        let state = state.clone();
        async move {
            while let Ok(text) = events_rx.recv().await {
                if state.borrow().private {
                    continue;
                }
                record(&state, text);
                if state.borrow().popover_window.is_visible() {
                    refresh_list(&state);
                }
            }
        }
    });

    private_switch.connect_state_set({
        let state = state.clone();
        move |_, active| {
            state.borrow_mut().private = active;
            glib::Propagation::Proceed
        }
    });

    clear_button.connect_clicked({
        let state = state.clone();
        move |_| {
            state.borrow_mut().entries.retain(|entry| entry.pinned);
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
            refresh_list(&state);
            let guard = state.borrow();
            position_near_icon(
                &guard.popover_window,
                &guard.icon_button,
                &guard.content,
                Edge::Right,
            );
            guard.popover_window.present();
            guard.popover_window.set_visible(true);
        }
    });

    Some(icon_button.upcast())
}

fn record(state: &Rc<RefCell<AppletState>>, text: String) {
    let mut guard = state.borrow_mut();
    if let Some(pos) = guard.entries.iter().position(|entry| entry.text == text) {
        let entry = guard.entries.remove(pos);
        guard.entries.insert(0, entry);
        return;
    }
    guard.entries.insert(0, ClipEntry { text, pinned: false });
    let unpinned = guard.entries.iter().filter(|entry| !entry.pinned).count();
    if unpinned > MAX_HISTORY {
        if let Some(pos) = guard.entries.iter().rposition(|entry| !entry.pinned) {
            guard.entries.remove(pos);
        }
    }
}

fn ordered_entries(state: &Rc<RefCell<AppletState>>) -> Vec<(String, bool)> {
    let guard = state.borrow();
    let mut ordered: Vec<(String, bool)> = Vec::new();
    for entry in guard.entries.iter().filter(|entry| entry.pinned) {
        ordered.push((entry.text.clone(), true));
    }
    for entry in guard.entries.iter().filter(|entry| !entry.pinned) {
        ordered.push((entry.text.clone(), false));
    }
    ordered
}

fn refresh_list(state: &Rc<RefCell<AppletState>>) {
    let (list_box, empty_label, clear_button) = {
        let guard = state.borrow();
        (
            guard.list_box.clone(),
            guard.empty_label.clone(),
            guard.clear_button.clone(),
        )
    };

    while let Some(child) = list_box.first_child() {
        list_box.remove(&child);
    }

    let entries = ordered_entries(state);
    empty_label.set_visible(entries.is_empty());
    clear_button.set_visible(entries.iter().any(|(_, pinned)| !pinned));

    for (text, pinned) in entries {
        let row = clip_row(&text, pinned);
        list_box.append(&row.container);

        row.copy_button.connect_clicked({
            let state = state.clone();
            let text = text.clone();
            move |_| {
                clipboard::copy(&text);
                state.borrow().popover_window.set_visible(false);
            }
        });

        row.pin_button.connect_clicked({
            let state = state.clone();
            let text = text.clone();
            move |_| {
                {
                    let mut guard = state.borrow_mut();
                    if let Some(entry) = guard.entries.iter_mut().find(|entry| entry.text == text) {
                        entry.pinned = !entry.pinned;
                    }
                }
                refresh_list(&state);
            }
        });

        row.delete_button.connect_clicked({
            let state = state.clone();
            let text = text.clone();
            move |_| {
                state.borrow_mut().entries.retain(|entry| entry.text != text);
                refresh_list(&state);
            }
        });
    }
}

struct ClipRow {
    container: gtk4::Box,
    copy_button: gtk4::Button,
    pin_button: gtk4::Button,
    delete_button: gtk4::Button,
}

fn clip_row(text: &str, pinned: bool) -> ClipRow {
    let container = gtk4::Box::new(Orientation::Horizontal, 8);
    container.add_css_class("bar-wifi-row");

    let copy_button = gtk4::Button::new();
    copy_button.set_hexpand(true);
    copy_button.set_has_frame(false);
    let label = gtk4::Label::new(Some(&preview(text)));
    label.set_halign(gtk4::Align::Start);
    label.set_wrap(false);
    label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    copy_button.set_child(Some(&label));
    copy_button.set_tooltip_text(Some(text));
    container.append(&copy_button);

    let pin_button = gtk4::Button::new();
    pin_button.add_css_class("bar-applet-icon");
    let pin_icon = if pinned {
        "starred-symbolic"
    } else {
        "non-starred-symbolic"
    };
    pin_button.set_child(Some(&gtk4::Image::from_icon_name(pin_icon)));
    pin_button.set_tooltip_text(Some(if pinned { "Unpin" } else { "Pin" }));
    container.append(&pin_button);

    let delete_button = gtk4::Button::new();
    delete_button.add_css_class("bar-applet-icon");
    delete_button.set_child(Some(&gtk4::Image::from_icon_name("window-close-symbolic")));
    delete_button.set_tooltip_text(Some("Delete"));
    container.append(&delete_button);

    ClipRow {
        container,
        copy_button,
        pin_button,
        delete_button,
    }
}

fn preview(text: &str) -> String {
    let first_line = text.lines().next().unwrap_or("").trim();
    let more_lines = text.lines().nth(1).is_some();
    if first_line.chars().count() > PREVIEW_MAX_CHARS {
        let short: String = first_line.chars().take(PREVIEW_MAX_CHARS).collect();
        format!("{} ...", short.trim_end())
    } else if more_lines {
        format!("{first_line} ...")
    } else {
        first_line.to_string()
    }
}
