use crate::geometry::BarGeometry;
use crate::theme::ColorScheme;

pub const BAR_PADDING_PX: i32 = 32;
pub const APPLET_POPOVER_WIDTH_PX: i32 = 300;

struct Palette {
    bg_hex: &'static str,
    fg: &'static str,
    popover_bg: &'static str,
    workspace_active: &'static str,
    applet_hover: &'static str,
    applet_press: &'static str,
    wifi_connected: &'static str,
    subtitle: &'static str,
    error: &'static str,
}

pub fn generate_css(scheme: ColorScheme, geometry: &BarGeometry, opacity: f64) -> String {
    let p = match scheme {
        ColorScheme::Light => Palette {
            bg_hex: "#E6E6E6",
            fg: "#1a1a1a",
            popover_bg: "alpha(#E6E6E6, 0.97)",
            workspace_active: "alpha(#1a1a1a, 0.22)",
            applet_hover: "alpha(#1a1a1a, 0.12)",
            applet_press: "alpha(#1a1a1a, 0.22)",
            wifi_connected: "alpha(#1a1a1a, 0.16)",
            subtitle: "alpha(#1a1a1a, 0.65)",
            error: "#c0392b",
        },
        ColorScheme::Dark => Palette {
            bg_hex: "#1e1e1e",
            fg: "#f5f5f5",
            popover_bg: "alpha(#1e1e1e, 0.97)",
            workspace_active: "alpha(#f5f5f5, 0.22)",
            applet_hover: "alpha(#f5f5f5, 0.12)",
            applet_press: "alpha(#f5f5f5, 0.22)",
            wifi_connected: "alpha(#f5f5f5, 0.16)",
            subtitle: "alpha(#f5f5f5, 0.65)",
            error: "#ff6b6b",
        },
    };

    let bg_fill = format!("alpha({}, {:.3})", p.bg_hex, opacity.clamp(0.0, 1.0));
    let fg = p.fg;
    let popover_bg = p.popover_bg;
    let workspace_active = p.workspace_active;
    let applet_hover = p.applet_hover;
    let applet_press = p.applet_press;
    let wifi_connected = p.wifi_connected;
    let subtitle = p.subtitle;
    let error = p.error;

    let height = geometry.height;
    let font_pt = geometry.font_pt;
    let badge_width = geometry.badge_width;
    let badge_height = geometry.badge_height;
    let badge_radius = geometry.badge_radius;
    let applet_padding = geometry.applet_padding;

    format!(
        "\
window, .background {{
  background-color: transparent;
  box-shadow: none;
  margin: 0;
}}

.bar-background {{
  background-color: {bg_fill};
  min-height: {height:.0}px;
  padding: 0 {BAR_PADDING_PX}px;
}}

.bar-app-label {{
  color: {fg};
  font-size: {font_pt:.2}pt;
  font-weight: bold;
}}

.bar-clock-label {{
  color: {fg};
  font-size: {font_pt:.2}pt;
  font-weight: bold;
}}

.bar-workspace-badge {{
  color: {fg};
  font-size: {font_pt:.2}pt;
  font-weight: bold;
  min-width: {badge_width}px;
  min-height: {badge_height}px;
  border-radius: {badge_radius:.2}px;
}}

.bar-workspace-badge.active {{
  background-color: {workspace_active};
}}

.bar-applet-icon {{
  background: none;
  border: none;
  box-shadow: none;
  padding: {applet_padding:.0}px;
}}

.bar-applet-popover {{
  background-color: {popover_bg};
  color: {fg};
  border-radius: 12px;
  padding: 16px;
}}

.bar-wifi-switch-row {{
  padding: 6px 8px;
}}

.bar-wifi-row {{
  background: none;
  color: {fg};
  border: none;
  box-shadow: none;
  border-radius: 8px;
  padding: 6px 8px;
}}

.bar-wifi-row:hover {{
  background-color: {applet_hover};
}}

.bar-wifi-row:active {{
  background-color: {applet_press};
}}

.bar-wifi-row.connected {{
  background-color: {wifi_connected};
}}

.bar-wifi-error {{
  color: {error};
  font-size: 12px;
}}

.bar-wifi-subtitle {{
  color: {subtitle};
  font-size: 12px;
}}
"
    )
}
