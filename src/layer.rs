use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};

pub fn init(window: &gtk4::ApplicationWindow, height_px: i32) {
    window.init_layer_shell();
    window.set_layer(Layer::Top);
    window.set_namespace(Some("justabar"));
    window.set_keyboard_mode(KeyboardMode::None);

    for edge in [Edge::Top, Edge::Left, Edge::Right] {
        window.set_anchor(edge, true);
        window.set_margin(edge, 0);
    }

    window.set_exclusive_zone(height_px);
}
