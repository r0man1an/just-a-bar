use std::fs;
use std::path::PathBuf;

use kdl::{KdlDocument, KdlEntry, KdlNode};

const LAYOUT_NODE: &str = "layout";

fn config_path() -> Option<PathBuf> {
    Some(dirs::config_dir()?.join("niri").join("config.kdl"))
}

pub fn sync_layouts(layouts: &[String]) {
    let Some(path) = config_path() else { return };

    let Ok(raw) = fs::read_to_string(&path) else {
        eprintln!("jbar: niri config not found at {path:?}; not touching keyboard layouts there");
        return;
    };

    let Ok(mut doc) = KdlDocument::parse_v1(&raw) else {
        eprintln!("jbar: niri config at {path:?} failed to parse; leaving it untouched");
        return;
    };

    let input_children = ensure_node(&mut doc, "input").ensure_children();
    let keyboard_children = ensure_node(input_children, "keyboard").ensure_children();
    let xkb_children = ensure_node(keyboard_children, "xkb").ensure_children();

    if layouts.is_empty() {
        xkb_children
            .nodes_mut()
            .retain(|n| n.name().value() != LAYOUT_NODE);
    } else {

        let joined = layouts.join(",");
        let escaped = joined.replace('\\', "\\\\").replace('"', "\\\"");
        let entry = KdlEntry::parse_v1(&format!("\"{escaped}\"")).expect("well-formed literal entry");

        if let Some(existing) = xkb_children.get_mut(LAYOUT_NODE) {
            existing.entries_mut().clear();
            existing.push(entry);
        } else {
            let node_text = format!("\n            {LAYOUT_NODE} \"{escaped}\"\n");
            let node: KdlNode = node_text.parse().expect("well-formed literal node");
            xkb_children.nodes_mut().push(node);
        }
    }

    let new_text = doc.to_string();
    if new_text == raw {
        return;
    }

    if KdlDocument::parse_v1(&new_text).is_err() {
        eprintln!("jbar: refusing to write niri config at {path:?}: rewritten file failed to re-parse");
        return;
    }

    let backup_path = path.with_extension("kdl.jbar-bak");
    if !backup_path.exists() {
        let _ = fs::copy(&path, &backup_path);
    }

    let tmp_path = path.with_extension("kdl.jbar-tmp");
    if let Err(err) = fs::write(&tmp_path, &new_text) {
        eprintln!("jbar: failed to write niri config at {tmp_path:?}: {err}");
        return;
    }
    if let Err(err) = fs::rename(&tmp_path, &path) {
        eprintln!("jbar: failed to replace niri config at {path:?}: {err}");
    }
}

fn ensure_node<'a>(doc: &'a mut KdlDocument, name: &str) -> &'a mut KdlNode {
    if doc.get(name).is_none() {
        doc.nodes_mut().push(KdlNode::new(name));
    }
    doc.get_mut(name).expect("just inserted above")
}
