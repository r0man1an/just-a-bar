use std::fs;
use std::path::{Path, PathBuf};

const POWER_SUPPLY_DIR: &str = "/sys/class/power_supply";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChargeState {
    Charging,
    Discharging,
    Full,
    Other,
}

#[derive(Debug, Clone, Copy)]
pub struct BatteryStatus {
    pub percentage: u8,
    pub state: ChargeState,
}

pub fn find_battery() -> Option<PathBuf> {
    let entries = fs::read_dir(POWER_SUPPLY_DIR).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        let kind = fs::read_to_string(path.join("type")).unwrap_or_default();
        if kind.trim() == "Battery" {
            return Some(path);
        }
    }
    None
}

pub fn read(battery_path: &Path) -> Option<BatteryStatus> {
    let capacity: u8 = fs::read_to_string(battery_path.join("capacity"))
        .ok()?
        .trim()
        .parse()
        .ok()?;
    let status = fs::read_to_string(battery_path.join("status")).unwrap_or_default();
    let state = match status.trim() {
        "Charging" => ChargeState::Charging,
        "Discharging" => ChargeState::Discharging,
        "Full" => ChargeState::Full,
        _ => ChargeState::Other,
    };
    Some(BatteryStatus {
        percentage: capacity.min(100),
        state,
    })
}
