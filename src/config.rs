use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemePreference {
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ClockMode {
    Time,
    DateTime,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ClockFormat {
    Hour12,
    Hour24,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PlacesDisplayMode {
    Icon,
    Text,
    IconAndText,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum XkbGroupToggle {
    AltShift,
    SuperSpace,
    CtrlShift,
    CapsLock,
}

impl XkbGroupToggle {
    pub fn xkb_option(self) -> &'static str {
        match self {
            Self::AltShift => "grp:alt_shift_toggle",
            Self::SuperSpace => "grp:win_space_toggle",
            Self::CtrlShift => "grp:ctrl_shift_toggle",
            Self::CapsLock => "grp:caps_toggle",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PanelItem {
    Workspaces,
    WindowTitle,
    Clock,
    Places,
    Sound,
    Wifi,
    Bluetooth,
    Battery,
    Screen,
    Keyboard,
    Notifications,
    Power,
}

impl PanelItem {
    pub const ALL: [PanelItem; 12] = [
        PanelItem::Workspaces,
        PanelItem::WindowTitle,
        PanelItem::Clock,
        PanelItem::Places,
        PanelItem::Sound,
        PanelItem::Wifi,
        PanelItem::Bluetooth,
        PanelItem::Battery,
        PanelItem::Screen,
        PanelItem::Keyboard,
        PanelItem::Notifications,
        PanelItem::Power,
    ];

    pub fn label(self) -> &'static str {
        match self {
            PanelItem::Workspaces => "Workspaces",
            PanelItem::WindowTitle => "Window title",
            PanelItem::Clock => "Clock",
            PanelItem::Places => "Places",
            PanelItem::Sound => "Sound",
            PanelItem::Wifi => "WiFi",
            PanelItem::Bluetooth => "Bluetooth",
            PanelItem::Battery => "Battery",
            PanelItem::Screen => "Screen",
            PanelItem::Keyboard => "Keyboard",
            PanelItem::Notifications => "Notifications",
            PanelItem::Power => "Power",
        }
    }

    pub fn has_settings(self) -> bool {
        matches!(
            self,
            PanelItem::Clock | PanelItem::Battery | PanelItem::Places | PanelItem::Keyboard
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub theme: ThemePreference,
    pub monitor: Option<String>,
    pub bar_height: u32,
    pub opacity: f64,
    #[serde(default)]
    pub left: Vec<PanelItem>,
    #[serde(default)]
    pub center: Vec<PanelItem>,
    #[serde(default)]
    pub right: Vec<PanelItem>,
    #[serde(default)]
    pub layout_configured: bool,
    pub clock_mode: ClockMode,
    pub clock_format: ClockFormat,
    pub battery_show_percentage: bool,
    pub places_display_mode: PlacesDisplayMode,
    pub scroll_switches_workspace: bool,
    pub xkb_layouts: Vec<String>,
    pub xkb_group_toggle: XkbGroupToggle,
    #[serde(skip_serializing)]
    pub wifi_applet_enabled: bool,
    #[serde(skip_serializing)]
    pub bluetooth_applet_enabled: bool,
    #[serde(skip_serializing)]
    pub sound_applet_enabled: bool,
    #[serde(skip_serializing)]
    pub battery_applet_enabled: bool,
    #[serde(skip_serializing)]
    pub screen_applet_enabled: bool,
    #[serde(skip_serializing)]
    pub power_applet_enabled: bool,
    #[serde(skip_serializing)]
    pub places_applet_enabled: bool,
    #[serde(skip_serializing)]
    pub keyboard_applet_enabled: bool,
    #[serde(skip_serializing)]
    pub notification_applet_enabled: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            theme: ThemePreference::System,
            monitor: None,
            bar_height: 33,
            opacity: 0.9,
            left: vec![PanelItem::Workspaces, PanelItem::WindowTitle, PanelItem::Places],
            center: vec![PanelItem::Clock],
            right: vec![
                PanelItem::Sound,
                PanelItem::Wifi,
                PanelItem::Bluetooth,
                PanelItem::Battery,
                PanelItem::Screen,
                PanelItem::Keyboard,
                PanelItem::Notifications,
                PanelItem::Power,
            ],
            layout_configured: true,
            clock_mode: ClockMode::Time,
            clock_format: ClockFormat::Hour24,
            battery_show_percentage: false,
            places_display_mode: PlacesDisplayMode::IconAndText,
            scroll_switches_workspace: true,
            xkb_layouts: Vec::new(),
            xkb_group_toggle: XkbGroupToggle::AltShift,
            wifi_applet_enabled: true,
            bluetooth_applet_enabled: true,
            sound_applet_enabled: true,
            battery_applet_enabled: true,
            screen_applet_enabled: true,
            power_applet_enabled: true,
            places_applet_enabled: true,
            keyboard_applet_enabled: true,
            notification_applet_enabled: true,
        }
    }
}

impl Config {
    pub fn config_path() -> PathBuf {
        let base = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
        base.join("jbar").join("config.toml")
    }

    pub fn load() -> Self {
        let path = Self::config_path();
        match fs::read_to_string(&path) {
            Ok(raw) => match toml::from_str::<Config>(&raw) {
                Ok(mut cfg) => {
                    cfg.migrate_layout();
                    cfg
                }
                Err(err) => {
                    eprintln!("jbar: failed to parse {path:?}: {err}; using defaults");
                    Config::default()
                }
            },
            Err(_) => {
                let cfg = Config::default();
                cfg.write_default(&path);
                cfg
            }
        }
    }

    fn migrate_layout(&mut self) {
        if self.layout_configured {
            return;
        }
        let mut left = vec![PanelItem::Workspaces, PanelItem::WindowTitle];
        if self.places_applet_enabled {
            left.push(PanelItem::Places);
        }
        let center = vec![PanelItem::Clock];
        let mut right = Vec::new();
        if self.sound_applet_enabled {
            right.push(PanelItem::Sound);
        }
        if self.wifi_applet_enabled {
            right.push(PanelItem::Wifi);
        }
        if self.bluetooth_applet_enabled {
            right.push(PanelItem::Bluetooth);
        }
        if self.battery_applet_enabled {
            right.push(PanelItem::Battery);
        }
        if self.screen_applet_enabled {
            right.push(PanelItem::Screen);
        }
        if self.keyboard_applet_enabled {
            right.push(PanelItem::Keyboard);
        }
        if self.notification_applet_enabled {
            right.push(PanelItem::Notifications);
        }
        if self.power_applet_enabled {
            right.push(PanelItem::Power);
        }
        self.left = left;
        self.center = center;
        self.right = right;
        self.layout_configured = true;
    }

    pub fn save(&self) {
        self.write_default(&Self::config_path());
    }

    fn write_default(&self, path: &PathBuf) {
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Ok(toml_str) = toml::to_string_pretty(self) {
            let _ = fs::write(path, toml_str);
        }
    }
}
