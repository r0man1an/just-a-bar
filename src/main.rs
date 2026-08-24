mod app;
mod audio;
mod battery;
mod battery_applet;
mod bluetooth;
mod bluetooth_applet;
mod brightness;
mod caffeine;
mod clock;
mod config;
mod configapp;
mod darkmode;
mod desktop;
mod geometry;
mod keyboard_applet;
mod keyboard_backlight;
mod keyboard_layout;
mod layer;
mod mako_config;
mod networkmanager;
mod niri_config;
mod notification_applet;
mod notifications;
mod overlay;
mod places_applet;
mod power;
mod power_applet;
mod power_profiles;
mod screen_applet;
mod sound_applet;
mod style;
mod theme;
mod toplevel;
mod wifi_applet;
mod workspaces;

use gtk4::glib;
use gtk4::prelude::*;

fn main() -> glib::ExitCode {

    if std::env::var_os("GDK_BACKEND").is_none() {
        unsafe { std::env::set_var("GDK_BACKEND", "wayland") };
    }
    if std::env::var_os("GSK_RENDERER").is_none() {
        unsafe { std::env::set_var("GSK_RENDERER", "cairo") };
    }

    if std::env::args().any(|arg| arg == "--config") {
        let application = gtk4::Application::new(
            Some("com.justabar.Bar.Config"),
            gtk4::gio::ApplicationFlags::empty(),
        );
        application.connect_activate(configapp::build_ui);
        return application.run_with_args::<&str>(&[]);
    }

    let application = gtk4::Application::new(
        Some("com.justabar.Bar"),
        gtk4::gio::ApplicationFlags::empty(),
    );
    application.connect_activate(app::build_ui);
    application.run()
}
