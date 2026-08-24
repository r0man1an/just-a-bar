use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

const DND_MODE: &str = "do-not-disturb";

#[derive(Debug, Clone)]
pub struct Notification {
    pub id: u32,
    pub app_name: String,
    pub summary: String,
    pub body: String,
    pub active: bool,
}

pub fn list() -> Vec<Notification> {
    let mut notifications = list_of("list", true);
    notifications.extend(list_of("history", false));

    let visible_ids: HashSet<u32> = notifications.iter().map(|n| n.id).collect();
    let hidden = prune_hidden(&visible_ids);

    notifications.retain(|n| !hidden.contains(&n.id));
    notifications
}

fn list_of(subcommand: &str, active: bool) -> Vec<Notification> {
    let output = match Command::new("makoctl").arg(subcommand).arg("-j").output() {
        Ok(output) => output,
        Err(err) => {
            eprintln!("jbar: failed to run makoctl {subcommand} ({err}); is mako running?");
            return Vec::new();
        }
    };
    if !output.status.success() {
        return Vec::new();
    }
    let Ok(items) = serde_json::from_slice::<serde_json::Value>(&output.stdout) else {
        return Vec::new();
    };
    let Some(items) = items.as_array() else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| {
            let id = item["id"].as_u64()? as u32;
            Some(Notification {
                id,
                app_name: item["app_name"].as_str().unwrap_or("").to_string(),
                summary: item["summary"].as_str().unwrap_or("").to_string(),
                body: item["body"].as_str().unwrap_or("").to_string(),
                active,
            })
        })
        .collect()
}

pub fn dismiss(notification: &Notification) {
    if notification.active {
        let _ = Command::new("makoctl")
            .arg("dismiss")
            .arg("-n")
            .arg(notification.id.to_string())
            .status();
    } else {
        let mut hidden = hidden_ids();
        hidden.insert(notification.id);
        save_hidden(&hidden);
    }
}

pub fn dismiss_all(notifications: &[Notification]) {
    let _ = Command::new("makoctl").arg("dismiss").arg("-a").status();

    let mut hidden = hidden_ids();
    for n in notifications {
        if !n.active {
            hidden.insert(n.id);
        }
    }
    save_hidden(&hidden);
}

pub fn dnd_active() -> bool {
    let output = match Command::new("makoctl").arg("mode").output() {
        Ok(output) => output,
        Err(_) => return false,
    };
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .any(|line| line.trim() == DND_MODE)
}

pub fn set_dnd(enabled: bool) {
    let flag = if enabled { "-a" } else { "-r" };
    let _ = Command::new("makoctl").arg("mode").arg(flag).arg(DND_MODE).status();
}

fn hidden_ids_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("jbar")
        .join("hidden_notifications.txt")
}

fn hidden_ids() -> HashSet<u32> {
    fs::read_to_string(hidden_ids_path())
        .ok()
        .map(|s| s.lines().filter_map(|l| l.trim().parse().ok()).collect())
        .unwrap_or_default()
}

fn save_hidden(hidden: &HashSet<u32>) {
    let path = hidden_ids_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let text: Vec<String> = hidden.iter().map(|id| id.to_string()).collect();
    let _ = fs::write(&path, text.join("\n"));
}

fn prune_hidden(visible_ids: &HashSet<u32>) -> HashSet<u32> {
    let hidden = hidden_ids();
    let pruned: HashSet<u32> = hidden.intersection(visible_ids).copied().collect();
    if pruned.len() != hidden.len() {
        save_hidden(&pruned);
    }
    pruned
}
