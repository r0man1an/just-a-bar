use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::rc::Rc;

use gtk4::prelude::*;
use gtk4::{glib, Orientation};

use crate::config::{
    ClockFormat, ClockMode, Config, PanelItem, PlacesDisplayMode, ThemePreference, XkbGroupToggle,
};
use crate::theme::{self, ColorScheme};

const BAR_HEIGHT_OPTIONS: [u32; 5] = [25, 33, 40, 50, 60];
const OPACITY_OPTIONS: [f64; 8] = [1.0, 0.95, 0.9, 0.85, 0.8, 0.7, 0.6, 0.5];
const MAX_XKB_LAYOUTS: usize = 5;

fn apply_theme_preference(preference: ThemePreference) {
    let prefer_dark = match preference {
        ThemePreference::Light => false,
        ThemePreference::Dark => true,
        ThemePreference::System => theme::init().0 == ColorScheme::Dark,
    };
    if let Some(settings) = gtk4::Settings::default() {
        settings.set_gtk_application_prefer_dark_theme(prefer_dark);
    }
}

fn monitor_options(selected: &Option<String>) -> (Vec<String>, Vec<Option<String>>) {
    let mut labels = vec!["Automatic".to_string()];
    let mut values: Vec<Option<String>> = vec![None];
    if let Some(display) = gtk4::gdk::Display::default() {
        let monitors = display.monitors();
        for i in 0..monitors.n_items() {
            let Some(obj) = monitors.item(i) else { continue };
            let Ok(monitor) = obj.downcast::<gtk4::gdk::Monitor>() else { continue };
            let Some(connector) = monitor.connector() else { continue };
            let name = connector.to_string();
            labels.push(name.clone());
            values.push(Some(name));
        }
    }
    if let Some(name) = selected {
        if !values.iter().any(|value| value.as_deref() == Some(name.as_str())) {
            labels.push(format!("{name} (disconnected)"));
            values.push(Some(name.clone()));
        }
    }
    (labels, values)
}

fn labeled_row(text: &str, control: &impl IsA<gtk4::Widget>) -> gtk4::Box {
    let row = gtk4::Box::new(Orientation::Horizontal, 12);
    let label = gtk4::Label::new(Some(text));
    label.set_hexpand(true);
    label.set_halign(gtk4::Align::Start);
    row.append(&label);
    row.append(control);
    row
}

fn rebuild_layouts_list(
    list_box: &gtk4::Box,
    layouts: &Rc<RefCell<Vec<String>>>,
    add_layout_button: &gtk4::Button,
    layout_entry: &gtk4::Entry,
) {
    while let Some(child) = list_box.first_child() {
        list_box.remove(&child);
    }
    for (index, code) in layouts.borrow().iter().enumerate() {
        let row = gtk4::Box::new(Orientation::Horizontal, 8);
        let label = gtk4::Label::new(Some(code));
        label.set_hexpand(true);
        label.set_halign(gtk4::Align::Start);
        let remove_button = gtk4::Button::with_label("Remove");
        row.append(&label);
        row.append(&remove_button);
        list_box.append(&row);

        remove_button.connect_clicked({
            let layouts = layouts.clone();
            let list_box = list_box.clone();
            let add_layout_button = add_layout_button.clone();
            let layout_entry = layout_entry.clone();
            move |_| {
                layouts.borrow_mut().remove(index);
                rebuild_layouts_list(&list_box, &layouts, &add_layout_button, &layout_entry);
            }
        });
    }

    let at_max = layouts.borrow().len() >= MAX_XKB_LAYOUTS;
    add_layout_button.set_sensitive(!at_max);
    layout_entry.set_sensitive(!at_max);
    layout_entry.set_placeholder_text(Some(if at_max {
        "Maximum of 5 layouts reached"
    } else {
        "e.g. us, de, us(dvorak)"
    }));
}

struct AppletsUi {
    layout: RefCell<[Vec<PanelItem>; 3]>,
    selected: RefCell<Option<(usize, usize)>>,
    updating: Cell<bool>,
    columns: [gtk4::ListBox; 3],
    up: gtk4::Button,
    down: gtk4::Button,
    left: gtk4::Button,
    right: gtk4::Button,
    remove: gtk4::Button,
    add: gtk4::Button,
    settings: gtk4::Button,
    add_popover: gtk4::Popover,
    settings_popover: gtk4::Popover,
    clock_mode: Cell<ClockMode>,
    clock_format: Cell<ClockFormat>,
    battery_show_percentage: Cell<bool>,
    places_display_mode: Cell<PlacesDisplayMode>,
    xkb_layouts: Rc<RefCell<Vec<String>>>,
    xkb_group_toggle: Cell<XkbGroupToggle>,
}

impl AppletsUi {
    fn pool(&self) -> Vec<PanelItem> {
        let layout = self.layout.borrow();
        PanelItem::ALL
            .into_iter()
            .filter(|item| !layout.iter().any(|col| col.contains(item)))
            .collect()
    }

    fn selected_item(&self) -> Option<PanelItem> {
        let (c, i) = (*self.selected.borrow())?;
        self.layout.borrow().get(c)?.get(i).copied()
    }

    fn refresh(self: &Rc<Self>) {
        self.updating.set(true);
        for c in 0..3 {
            let list = &self.columns[c];
            while let Some(child) = list.first_child() {
                list.remove(&child);
            }
            for item in self.layout.borrow()[c].iter() {
                let row = gtk4::ListBoxRow::new();
                let label = gtk4::Label::new(Some(item.label()));
                label.set_halign(gtk4::Align::Start);
                label.set_margin_top(4);
                label.set_margin_bottom(4);
                label.set_margin_start(8);
                label.set_margin_end(8);
                row.set_child(Some(&label));
                list.append(&row);
            }
        }

        for c in 0..3 {
            self.columns[c].unselect_all();
        }
        let sel = *self.selected.borrow();
        if let Some((c, i)) = sel {
            if let Some(row) = self.columns[c].row_at_index(i as i32) {
                self.columns[c].select_row(Some(&row));
            } else {
                *self.selected.borrow_mut() = None;
            }
        }
        self.updating.set(false);
        self.update_controls();
    }

    fn update_controls(&self) {
        let sel = *self.selected.borrow();
        let has_sel = sel.is_some();
        let (c, i) = sel.unwrap_or((0, 0));
        let col_len = self.layout.borrow().get(c).map(|v| v.len()).unwrap_or(0);
        self.up.set_sensitive(has_sel && i > 0);
        self.down.set_sensitive(has_sel && i + 1 < col_len);
        self.left.set_sensitive(has_sel && c > 0);
        self.right.set_sensitive(has_sel && c < 2);
        self.remove.set_sensitive(has_sel);
        self.add.set_sensitive(!self.pool().is_empty());
        let can_settings = self.selected_item().map(|it| it.has_settings()).unwrap_or(false);
        self.settings.set_sensitive(can_settings);
    }

    fn move_within(self: &Rc<Self>, delta: isize) {
        let sel = *self.selected.borrow();
        if let Some((c, i)) = sel {
            let mut layout = self.layout.borrow_mut();
            let target = i as isize + delta;
            if target < 0 || target as usize >= layout[c].len() {
                return;
            }
            layout[c].swap(i, target as usize);
            drop(layout);
            *self.selected.borrow_mut() = Some((c, target as usize));
        }
        self.refresh();
    }

    fn move_section(self: &Rc<Self>, delta: isize) {
        let sel = *self.selected.borrow();
        if let Some((c, i)) = sel {
            let target = c as isize + delta;
            if !(0..=2).contains(&target) {
                return;
            }
            let target = target as usize;
            let mut layout = self.layout.borrow_mut();
            let item = layout[c].remove(i);
            layout[target].push(item);
            let new_index = layout[target].len() - 1;
            drop(layout);
            *self.selected.borrow_mut() = Some((target, new_index));
        }
        self.refresh();
    }

    fn remove_selected(self: &Rc<Self>) {
        let sel = *self.selected.borrow();
        if let Some((c, i)) = sel {
            let mut layout = self.layout.borrow_mut();
            if i < layout[c].len() {
                layout[c].remove(i);
            }
            drop(layout);
            *self.selected.borrow_mut() = None;
        }
        self.refresh();
    }

    fn add_item(self: &Rc<Self>, item: PanelItem) {
        let target = self.selected.borrow().map(|(c, _)| c).unwrap_or(0);
        let mut layout = self.layout.borrow_mut();
        layout[target].push(item);
        let new_index = layout[target].len() - 1;
        drop(layout);
        *self.selected.borrow_mut() = Some((target, new_index));
        self.refresh();
    }

    fn open_add_popover(self: &Rc<Self>) {
        let list = gtk4::Box::new(Orientation::Vertical, 4);
        let pool = self.pool();
        if pool.is_empty() {
            let label = gtk4::Label::new(Some("All applets are already placed"));
            label.set_margin_top(4);
            label.set_margin_bottom(4);
            list.append(&label);
        } else {
            for item in pool {
                let button = gtk4::Button::with_label(item.label());
                button.set_has_frame(false);
                button.connect_clicked({
                    let ui = self.clone();
                    move |_| {
                        ui.add_popover.popdown();
                        ui.add_item(item);
                    }
                });
                list.append(&button);
            }
        }
        self.add_popover.set_child(Some(&list));
        self.add_popover.popup();
    }

    fn open_settings_popover(self: &Rc<Self>) {
        let Some(item) = self.selected_item() else {
            return;
        };
        if !item.has_settings() {
            return;
        }

        let content = gtk4::Box::new(Orientation::Vertical, 10);
        content.set_margin_top(8);
        content.set_margin_bottom(8);
        content.set_margin_start(8);
        content.set_margin_end(8);

        match item {
            PanelItem::Clock => {
                let mode_dropdown = gtk4::DropDown::from_strings(&["Time", "Date and time"]);
                mode_dropdown.set_selected(match self.clock_mode.get() {
                    ClockMode::Time => 0,
                    ClockMode::DateTime => 1,
                });
                mode_dropdown.connect_selected_notify({
                    let ui = self.clone();
                    move |dropdown| {
                        ui.clock_mode.set(match dropdown.selected() {
                            1 => ClockMode::DateTime,
                            _ => ClockMode::Time,
                        });
                    }
                });

                let format_dropdown = gtk4::DropDown::from_strings(&["24-hour", "12-hour"]);
                format_dropdown.set_selected(match self.clock_format.get() {
                    ClockFormat::Hour24 => 0,
                    ClockFormat::Hour12 => 1,
                });
                format_dropdown.connect_selected_notify({
                    let ui = self.clone();
                    move |dropdown| {
                        ui.clock_format.set(match dropdown.selected() {
                            1 => ClockFormat::Hour12,
                            _ => ClockFormat::Hour24,
                        });
                    }
                });

                content.append(&labeled_row("Clock", &mode_dropdown));
                content.append(&labeled_row("Time format", &format_dropdown));
            }
            PanelItem::Battery => {
                let percentage_switch = gtk4::Switch::new();
                percentage_switch.set_active(self.battery_show_percentage.get());
                percentage_switch.set_halign(gtk4::Align::End);
                percentage_switch.connect_active_notify({
                    let ui = self.clone();
                    move |switch| ui.battery_show_percentage.set(switch.is_active())
                });
                content.append(&labeled_row("Show percentage", &percentage_switch));
            }
            PanelItem::Places => {
                let display_dropdown =
                    gtk4::DropDown::from_strings(&["Icon", "Text", "Icon and text"]);
                display_dropdown.set_selected(match self.places_display_mode.get() {
                    PlacesDisplayMode::Icon => 0,
                    PlacesDisplayMode::Text => 1,
                    PlacesDisplayMode::IconAndText => 2,
                });
                display_dropdown.connect_selected_notify({
                    let ui = self.clone();
                    move |dropdown| {
                        ui.places_display_mode.set(match dropdown.selected() {
                            0 => PlacesDisplayMode::Icon,
                            1 => PlacesDisplayMode::Text,
                            _ => PlacesDisplayMode::IconAndText,
                        });
                    }
                });
                content.append(&labeled_row("Display", &display_dropdown));
            }
            PanelItem::Keyboard => {
                let layouts_list_box = gtk4::Box::new(Orientation::Vertical, 6);
                let layout_entry = gtk4::Entry::new();
                layout_entry.set_hexpand(true);
                let add_layout_button = gtk4::Button::with_label("Add");

                rebuild_layouts_list(
                    &layouts_list_box,
                    &self.xkb_layouts,
                    &add_layout_button,
                    &layout_entry,
                );

                let add_layout_row = gtk4::Box::new(Orientation::Horizontal, 8);
                add_layout_row.append(&layout_entry);
                add_layout_row.append(&add_layout_button);

                add_layout_button.connect_clicked({
                    let xkb_layouts = self.xkb_layouts.clone();
                    let layouts_list_box = layouts_list_box.clone();
                    let layout_entry = layout_entry.clone();
                    let add_layout_button = add_layout_button.clone();
                    move |_| {
                        if xkb_layouts.borrow().len() >= MAX_XKB_LAYOUTS {
                            return;
                        }
                        let code = layout_entry.text().trim().to_string();
                        if code.is_empty() {
                            return;
                        }
                        xkb_layouts.borrow_mut().push(code);
                        layout_entry.set_text("");
                        rebuild_layouts_list(
                            &layouts_list_box,
                            &xkb_layouts,
                            &add_layout_button,
                            &layout_entry,
                        );
                    }
                });
                layout_entry.connect_activate({
                    let add_layout_button = add_layout_button.clone();
                    move |_| add_layout_button.emit_clicked()
                });

                let toggle_dropdown = gtk4::DropDown::from_strings(&[
                    "Alt+Shift",
                    "Super+Space",
                    "Ctrl+Shift",
                    "Caps Lock",
                ]);
                toggle_dropdown.set_selected(match self.xkb_group_toggle.get() {
                    XkbGroupToggle::AltShift => 0,
                    XkbGroupToggle::SuperSpace => 1,
                    XkbGroupToggle::CtrlShift => 2,
                    XkbGroupToggle::CapsLock => 3,
                });
                toggle_dropdown.connect_selected_notify({
                    let ui = self.clone();
                    move |dropdown| {
                        ui.xkb_group_toggle.set(match dropdown.selected() {
                            1 => XkbGroupToggle::SuperSpace,
                            2 => XkbGroupToggle::CtrlShift,
                            3 => XkbGroupToggle::CapsLock,
                            _ => XkbGroupToggle::AltShift,
                        });
                    }
                });

                let layouts_label = gtk4::Label::new(Some("XKB layouts"));
                layouts_label.set_halign(gtk4::Align::Start);

                content.append(&layouts_label);
                content.append(&layouts_list_box);
                content.append(&add_layout_row);
                content.append(&gtk4::Separator::new(Orientation::Horizontal));
                content.append(&labeled_row("Layout switch key", &toggle_dropdown));
            }
            _ => {}
        }

        self.settings_popover.set_child(Some(&content));
        self.settings_popover.popup();
    }
}

fn build_column(title: &str, list: &gtk4::ListBox) -> gtk4::Box {
    let column = gtk4::Box::new(Orientation::Vertical, 6);
    column.set_hexpand(true);

    let header = gtk4::Label::new(Some(title));
    header.set_halign(gtk4::Align::Start);
    header.add_css_class("heading");

    let scroller = gtk4::ScrolledWindow::new();
    scroller.set_policy(gtk4::PolicyType::Never, gtk4::PolicyType::Automatic);
    scroller.set_min_content_height(220);
    scroller.set_vexpand(true);
    scroller.set_child(Some(list));

    column.append(&header);
    column.append(&scroller);
    column
}

fn icon_button(icon: &str, tooltip: &str) -> gtk4::Button {
    let button = gtk4::Button::from_icon_name(icon);
    button.set_tooltip_text(Some(tooltip));
    button
}

pub fn build_ui(app: &gtk4::Application) {
    let config = Config::load();
    let default_bar_height = Config::default().bar_height;
    let default_opacity = Config::default().opacity;

    let window = gtk4::ApplicationWindow::builder()
        .application(app)
        .title("JustABar Settings")
        .default_width(520)
        .default_height(480)
        .resizable(false)
        .build();

    let root = gtk4::Box::new(Orientation::Vertical, 12);
    root.set_margin_top(20);
    root.set_margin_bottom(20);
    root.set_margin_start(20);
    root.set_margin_end(20);

    let theme_dropdown = gtk4::DropDown::from_strings(&["System", "Light", "Dark"]);
    theme_dropdown.set_selected(match config.theme {
        ThemePreference::System => 0,
        ThemePreference::Light => 1,
        ThemePreference::Dark => 2,
    });
    theme_dropdown.connect_selected_notify(|dropdown| {
        let preference = match dropdown.selected() {
            1 => ThemePreference::Light,
            2 => ThemePreference::Dark,
            _ => ThemePreference::System,
        };
        apply_theme_preference(preference);
    });

    let (monitor_labels, monitor_values) = monitor_options(&config.monitor);
    let monitor_label_refs: Vec<&str> = monitor_labels.iter().map(String::as_str).collect();
    let monitor_dropdown = gtk4::DropDown::from_strings(&monitor_label_refs);
    let monitor_index = monitor_values
        .iter()
        .position(|value| value.as_deref() == config.monitor.as_deref())
        .unwrap_or(0);
    monitor_dropdown.set_selected(monitor_index as u32);

    let height_labels: Vec<String> = BAR_HEIGHT_OPTIONS
        .iter()
        .map(|height| match *height == default_bar_height {
            true => format!("{height}px (default)"),
            false => format!("{height}px"),
        })
        .collect();
    let height_label_refs: Vec<&str> = height_labels.iter().map(String::as_str).collect();
    let height_dropdown = gtk4::DropDown::from_strings(&height_label_refs);
    let height_index = BAR_HEIGHT_OPTIONS
        .iter()
        .position(|height| *height == config.bar_height)
        .unwrap_or(0);
    height_dropdown.set_selected(height_index as u32);

    let opacity_labels: Vec<String> = OPACITY_OPTIONS
        .iter()
        .map(|value| match (*value - default_opacity).abs() < f64::EPSILON {
            true => format!("{:.0}% (default)", value * 100.0),
            false => format!("{:.0}%", value * 100.0),
        })
        .collect();
    let opacity_label_refs: Vec<&str> = opacity_labels.iter().map(String::as_str).collect();
    let opacity_dropdown = gtk4::DropDown::from_strings(&opacity_label_refs);
    let opacity_index = OPACITY_OPTIONS
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| {
            (**a - config.opacity)
                .abs()
                .total_cmp(&(**b - config.opacity).abs())
        })
        .map(|(index, _)| index)
        .unwrap_or(0);
    opacity_dropdown.set_selected(opacity_index as u32);

    let scroll_workspace_switch = gtk4::Switch::new();
    scroll_workspace_switch.set_active(config.scroll_switches_workspace);
    scroll_workspace_switch.set_halign(gtk4::Align::End);

    let panel_page = gtk4::Box::new(Orientation::Vertical, 14);
    panel_page.set_margin_top(16);
    panel_page.append(&labeled_row("Theme", &theme_dropdown));
    panel_page.append(&labeled_row("Monitor", &monitor_dropdown));
    panel_page.append(&labeled_row("Bar height", &height_dropdown));
    panel_page.append(&labeled_row("Bar opacity", &opacity_dropdown));
    panel_page.append(&labeled_row("Scroll bar to switch workspace", &scroll_workspace_switch));

    let mut seen: HashSet<PanelItem> = HashSet::new();
    let mut initial_layout: [Vec<PanelItem>; 3] = [Vec::new(), Vec::new(), Vec::new()];
    for (index, source) in [
        config.left.clone(),
        config.center.clone(),
        config.right.clone(),
    ]
    .into_iter()
    .enumerate()
    {
        for item in source {
            if seen.insert(item) {
                initial_layout[index].push(item);
            }
        }
    }

    let ui = Rc::new(AppletsUi {
        layout: RefCell::new(initial_layout),
        selected: RefCell::new(None),
        updating: Cell::new(false),
        columns: [
            gtk4::ListBox::new(),
            gtk4::ListBox::new(),
            gtk4::ListBox::new(),
        ],
        up: icon_button("go-up-symbolic", "Move up"),
        down: icon_button("go-down-symbolic", "Move down"),
        left: icon_button("go-previous-symbolic", "Move to the section on the left"),
        right: icon_button("go-next-symbolic", "Move to the section on the right"),
        remove: icon_button("list-remove-symbolic", "Remove from the panel"),
        add: icon_button("list-add-symbolic", "Add an applet"),
        settings: gtk4::Button::with_label("Applet settings"),
        add_popover: gtk4::Popover::new(),
        settings_popover: gtk4::Popover::new(),
        clock_mode: Cell::new(config.clock_mode),
        clock_format: Cell::new(config.clock_format),
        battery_show_percentage: Cell::new(config.battery_show_percentage),
        places_display_mode: Cell::new(config.places_display_mode),
        xkb_layouts: Rc::new(RefCell::new(config.xkb_layouts.clone())),
        xkb_group_toggle: Cell::new(config.xkb_group_toggle),
    });

    for column in &ui.columns {
        column.set_selection_mode(gtk4::SelectionMode::Single);
    }

    ui.add_popover.set_parent(&ui.add);
    ui.settings_popover.set_parent(&ui.settings);

    for c in 0..3 {
        let ui_handler = ui.clone();
        ui.columns[c].connect_row_selected(move |_, row| {
            if ui_handler.updating.get() {
                return;
            }
            if let Some(row) = row {
                ui_handler.set_selection_from_click(c, row.index() as usize);
            }
        });
    }

    ui.up.connect_clicked({
        let ui = ui.clone();
        move |_| ui.move_within(-1)
    });
    ui.down.connect_clicked({
        let ui = ui.clone();
        move |_| ui.move_within(1)
    });
    ui.left.connect_clicked({
        let ui = ui.clone();
        move |_| ui.move_section(-1)
    });
    ui.right.connect_clicked({
        let ui = ui.clone();
        move |_| ui.move_section(1)
    });
    ui.remove.connect_clicked({
        let ui = ui.clone();
        move |_| ui.remove_selected()
    });
    ui.add.connect_clicked({
        let ui = ui.clone();
        move |_| ui.open_add_popover()
    });
    ui.settings.connect_clicked({
        let ui = ui.clone();
        move |_| ui.open_settings_popover()
    });

    let columns_box = gtk4::Box::new(Orientation::Horizontal, 12);
    columns_box.set_homogeneous(true);
    columns_box.set_vexpand(true);
    columns_box.append(&build_column("Left", &ui.columns[0]));
    columns_box.append(&build_column("Middle", &ui.columns[1]));
    columns_box.append(&build_column("Right", &ui.columns[2]));

    let toolbar = gtk4::Box::new(Orientation::Horizontal, 6);
    toolbar.set_halign(gtk4::Align::Center);
    toolbar.set_margin_top(6);
    toolbar.append(&ui.up);
    toolbar.append(&ui.down);
    toolbar.append(&gtk4::Separator::new(Orientation::Vertical));
    toolbar.append(&ui.left);
    toolbar.append(&ui.right);
    toolbar.append(&gtk4::Separator::new(Orientation::Vertical));
    toolbar.append(&ui.remove);
    toolbar.append(&ui.add);
    toolbar.append(&gtk4::Separator::new(Orientation::Vertical));
    toolbar.append(&ui.settings);

    let applets_page = gtk4::Box::new(Orientation::Vertical, 12);
    applets_page.set_margin_top(16);
    applets_page.append(&columns_box);
    applets_page.append(&toolbar);

    ui.refresh();

    let stack = gtk4::Stack::new();
    stack.set_vexpand(true);
    stack.add_titled(&panel_page, Some("panel"), "Panel");
    stack.add_titled(&applets_page, Some("applets"), "Applets");

    let stack_switcher = gtk4::StackSwitcher::new();
    stack_switcher.set_stack(Some(&stack));
    stack_switcher.set_halign(gtk4::Align::Center);

    root.append(&stack_switcher);
    root.append(&stack);

    let button_row = gtk4::Box::new(Orientation::Horizontal, 8);
    button_row.set_halign(gtk4::Align::End);
    let cancel_button = gtk4::Button::with_label("Cancel");
    let save_button = gtk4::Button::with_label("Save");
    save_button.add_css_class("suggested-action");
    button_row.append(&cancel_button);
    button_row.append(&save_button);
    root.append(&button_row);

    window.set_child(Some(&root));

    cancel_button.connect_clicked({
        let window = window.clone();
        move |_| window.close()
    });

    save_button.connect_clicked({
        let window = window.clone();
        let ui = ui.clone();
        let theme_dropdown = theme_dropdown.clone();
        let monitor_dropdown = monitor_dropdown.clone();
        let monitor_values = monitor_values.clone();
        let height_dropdown = height_dropdown.clone();
        let opacity_dropdown = opacity_dropdown.clone();
        let scroll_workspace_switch = scroll_workspace_switch.clone();
        move |_| {
            let mut config = Config::load();
            config.theme = match theme_dropdown.selected() {
                1 => ThemePreference::Light,
                2 => ThemePreference::Dark,
                _ => ThemePreference::System,
            };
            config.monitor = monitor_values
                .get(monitor_dropdown.selected() as usize)
                .cloned()
                .unwrap_or(None);
            config.bar_height = BAR_HEIGHT_OPTIONS
                .get(height_dropdown.selected() as usize)
                .copied()
                .unwrap_or(default_bar_height);
            config.opacity = OPACITY_OPTIONS
                .get(opacity_dropdown.selected() as usize)
                .copied()
                .unwrap_or(default_opacity);
            config.scroll_switches_workspace = scroll_workspace_switch.is_active();

            {
                let layout = ui.layout.borrow();
                config.left = layout[0].clone();
                config.center = layout[1].clone();
                config.right = layout[2].clone();
            }
            config.layout_configured = true;
            config.clock_mode = ui.clock_mode.get();
            config.clock_format = ui.clock_format.get();
            config.battery_show_percentage = ui.battery_show_percentage.get();
            config.places_display_mode = ui.places_display_mode.get();
            config.xkb_layouts = ui.xkb_layouts.borrow().clone();
            config.xkb_group_toggle = ui.xkb_group_toggle.get();

            config.save();
            restart_bar();
            window.close();
        }
    });

    window.present();

    glib::idle_add_local_once(move || apply_theme_preference(config.theme));
}

impl AppletsUi {
    fn set_selection_from_click(self: &Rc<Self>, column: usize, index: usize) {
        *self.selected.borrow_mut() = Some((column, index));
        self.updating.set(true);
        for c in 0..3 {
            if c != column {
                self.columns[c].unselect_all();
            }
        }
        self.updating.set(false);
        self.update_controls();
    }
}

fn restart_bar() {
    let Ok(current_exe) = std::env::current_exe() else {
        return;
    };
    let my_pid = std::process::id();

    if let Ok(entries) = std::fs::read_dir("/proc") {
        for entry in entries.flatten() {
            let Some(pid) = entry.file_name().to_str().and_then(|s| s.parse::<u32>().ok()) else {
                continue;
            };
            if pid == my_pid {
                continue;
            }
            let Ok(exe) = std::fs::read_link(entry.path().join("exe")) else {
                continue;
            };
            if exe != current_exe {
                continue;
            }
            let Ok(cmdline) = std::fs::read(entry.path().join("cmdline")) else {
                continue;
            };
            let is_config_window = cmdline.split(|&b| b == 0).any(|arg| arg == b"--config");
            if is_config_window {
                continue;
            }
            unsafe {
                libc::kill(pid as i32, libc::SIGTERM);
            }
        }
    }

    std::thread::sleep(std::time::Duration::from_millis(150));
    let _ = std::process::Command::new(current_exe).spawn();
}
