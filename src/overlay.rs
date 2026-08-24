use gtk4::prelude::*;
use gtk4::{glib, Orientation};
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use crate::style;

const POPOVER_TOP_GAP_PX: i32 = 4;
const EDGE_GAP_PX: i32 = 8;

pub fn build_overlay_window(
    app: &gtk4::Application,
    namespace: &str,
    horizontal_edge: Edge,
    child: &impl IsA<gtk4::Widget>,
) -> gtk4::ApplicationWindow {
    let window = gtk4::ApplicationWindow::builder()
        .application(app)
        .decorated(false)
        .resizable(false)
        .build();
    window.set_child(Some(child));
    window.init_layer_shell();
    window.set_layer(Layer::Overlay);
    window.set_namespace(Some(namespace));
    window.set_keyboard_mode(KeyboardMode::OnDemand);
    window.set_exclusive_zone(0);
    window.set_anchor(Edge::Top, true);
    window.set_anchor(horizontal_edge, true);
    window.set_margin(Edge::Top, POPOVER_TOP_GAP_PX);
    window.set_margin(horizontal_edge, style::BAR_PADDING_PX);
    window
}

pub fn position_near_icon(
    popover_window: &gtk4::ApplicationWindow,
    icon_button: &impl IsA<gtk4::Widget>,
    content: &gtk4::Widget,
    horizontal_edge: Edge,
) {
    let (icon_x, _) = icon_button
        .translate_coordinates(content, 0.0, 0.0)
        .unwrap_or((0.0, 0.0));
    let icon_center = icon_x + icon_button.width() as f64 / 2.0;
    let (_, natural_width, _, _) = popover_window.measure(Orientation::Horizontal, -1);
    let screen_width = content.width() as f64;
    let popover_width = natural_width as f64;
    let left = icon_center - popover_width / 2.0;
    let max_left = (screen_width - popover_width - EDGE_GAP_PX as f64).max(EDGE_GAP_PX as f64);
    let left = left.clamp(EDGE_GAP_PX as f64, max_left);

    let margin = match horizontal_edge {
        Edge::Left => left,
        Edge::Right => screen_width - popover_width - left,
        _ => return,
    };
    popover_window.set_margin(horizontal_edge, margin.round().max(0.0) as i32);
}

pub fn dismiss_on_focus_loss(window: &gtk4::ApplicationWindow) {
    window.connect_notify_local(Some("is-active"), |window, _| {
        if !window.is_active() {
            window.set_visible(false);
        }
    });
}

pub fn dismiss_on_escape(window: &gtk4::ApplicationWindow) {
    let key_controller = gtk4::EventControllerKey::new();
    window.add_controller(key_controller.clone());
    key_controller.connect_key_pressed({
        let window = window.clone();
        move |_, keyval, _, _| {
            if keyval == gtk4::gdk::Key::Escape {
                window.set_visible(false);
                glib::Propagation::Stop
            } else {
                glib::Propagation::Proceed
            }
        }
    });
}
