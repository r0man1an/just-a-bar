use std::fs;
use std::path::PathBuf;
use std::process::Command;

const DND_SECTION: &str = "[mode=do-not-disturb]";
const DND_BLOCK: &str = "[mode=do-not-disturb]\ninvisible=1\n";

fn config_path() -> Option<PathBuf> {
    Some(dirs::config_dir()?.join("mako").join("config"))
}

pub fn ensure_dnd_mode() {
    let Some(path) = config_path() else { return };

    let raw = fs::read_to_string(&path).unwrap_or_default();
    if raw.lines().any(|line| line.trim() == DND_SECTION) {
        return;
    }

    if let Some(parent) = path.parent() {
        if let Err(err) = fs::create_dir_all(parent) {
            eprintln!("jbar: failed to create {parent:?} for mako's config: {err}");
            return;
        }
    }

    if path.exists() {
        let backup_path = path.with_extension("jbar-bak");
        if !backup_path.exists() {
            let _ = fs::copy(&path, &backup_path);
        }
    }

    let mut new_text = raw;
    if !new_text.is_empty() && !new_text.ends_with('\n') {
        new_text.push('\n');
    }
    new_text.push_str(DND_BLOCK);

    let tmp_path = path.with_extension("jbar-tmp");
    if let Err(err) = fs::write(&tmp_path, &new_text) {
        eprintln!("jbar: failed to write mako config at {tmp_path:?}: {err}");
        return;
    }
    if let Err(err) = fs::rename(&tmp_path, &path) {
        eprintln!("jbar: failed to replace mako config at {path:?}: {err}");
        return;
    }

    let _ = Command::new("makoctl").arg("reload").status();
}
