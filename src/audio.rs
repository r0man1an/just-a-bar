use std::process::Command;

const SINK: &str = "@DEFAULT_AUDIO_SINK@";
const SOURCE: &str = "@DEFAULT_AUDIO_SOURCE@";

pub struct AppStream {
    pub id: u32,
    pub name: String,
}

pub fn is_available() -> bool {
    get_volume().is_some()
}

pub fn is_source_available() -> bool {
    get_source_volume().is_some()
}

pub fn get_volume() -> Option<(f64, bool)> {
    get_volume_of(SINK)
}

pub fn set_volume(volume: f64) {
    set_volume_of(SINK, volume);
}

pub fn set_mute(muted: bool) {
    set_mute_of(SINK, muted);
}

pub fn get_source_volume() -> Option<(f64, bool)> {
    get_volume_of(SOURCE)
}

pub fn set_source_volume(volume: f64) {
    set_volume_of(SOURCE, volume);
}

pub fn set_source_mute(muted: bool) {
    set_mute_of(SOURCE, muted);
}

pub fn get_stream_volume(id: u32) -> Option<(f64, bool)> {
    get_volume_of(&id.to_string())
}

pub fn set_stream_volume(id: u32, volume: f64) {
    set_volume_of(&id.to_string(), volume);
}

pub fn list_app_streams() -> Vec<AppStream> {
    let output = match Command::new("pw-dump").output() {
        Ok(output) => output,
        Err(err) => {
            eprintln!("jbar: failed to run pw-dump ({err}); is it on PATH for jbar's process?");
            return Vec::new();
        }
    };
    if !output.status.success() {
        return Vec::new();
    }
    let Ok(nodes) = serde_json::from_slice::<serde_json::Value>(&output.stdout) else {
        return Vec::new();
    };
    let Some(nodes) = nodes.as_array() else {
        return Vec::new();
    };

    nodes
        .iter()
        .filter_map(|node| {
            let props = &node["info"]["props"];
            if props["media.class"].as_str() != Some("Stream/Output/Audio") {
                return None;
            }

            if node["info"]["state"].as_str() != Some("running") {
                return None;
            }
            let id = node["id"].as_u64()? as u32;
            let name = props["application.name"]
                .as_str()
                .or_else(|| props["node.description"].as_str())
                .or_else(|| props["node.name"].as_str())
                .unwrap_or("Unknown")
                .to_string();
            Some(AppStream { id, name })
        })
        .collect()
}

fn get_volume_of(target: &str) -> Option<(f64, bool)> {
    let output = match Command::new("wpctl").arg("get-volume").arg(target).output() {
        Ok(output) => output,
        Err(err) => {
            eprintln!("jbar: failed to run wpctl ({err}); is it on PATH for jbar's process?");
            return None;
        }
    };
    if !output.status.success() {
        eprintln!(
            "jbar: wpctl get-volume {target} exited with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        );
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let line = text.trim();
    let muted = line.contains("[MUTED]");
    let Some(raw) = line.split_whitespace().nth(1) else {
        eprintln!("jbar: unexpected wpctl output: {line:?}");
        return None;
    };
    match raw.parse::<f64>() {
        Ok(volume) => Some((volume, muted)),
        Err(err) => {
            eprintln!("jbar: could not parse wpctl volume {raw:?} ({err})");
            None
        }
    }
}

fn set_volume_of(target: &str, volume: f64) {
    let volume = volume.clamp(0.0, 1.0);
    let _ = Command::new("wpctl")
        .arg("set-volume")
        .arg(target)
        .arg(format!("{volume:.2}"))
        .status();
}

fn set_mute_of(target: &str, muted: bool) {
    let _ = Command::new("wpctl")
        .arg("set-mute")
        .arg(target)
        .arg(if muted { "1" } else { "0" })
        .status();
}
