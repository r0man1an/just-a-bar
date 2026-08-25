use std::io::{Read, Write};
use std::process::{Command, Stdio};
use std::thread;

use async_channel::Sender as AsyncSender;

pub fn spawn(events_tx: AsyncSender<String>) {
    thread::Builder::new()
        .name("clipboard-watch".to_string())
        .spawn(move || watch(events_tx))
        .ok();
}

fn watch(events_tx: AsyncSender<String>) {
    let mut child = match Command::new("wl-paste")
        .arg("--type")
        .arg("text")
        .arg("--watch")
        .arg("sh")
        .arg("-c")
        .arg("cat; printf '\\0'")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(child) => child,
        Err(_) => return,
    };

    let Some(mut stdout) = child.stdout.take() else {
        return;
    };

    let mut buffer: Vec<u8> = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        match stdout.read(&mut chunk) {
            Ok(0) => break,
            Ok(count) => {
                for &byte in &chunk[..count] {
                    if byte == 0 {
                        emit(&events_tx, &mut buffer);
                    } else {
                        buffer.push(byte);
                    }
                }
            }
            Err(_) => break,
        }
    }

    let _ = child.wait();
}

fn emit(events_tx: &AsyncSender<String>, buffer: &mut Vec<u8>) {
    let bytes = std::mem::take(buffer);
    if bytes.is_empty() {
        return;
    }
    if let Ok(text) = String::from_utf8(bytes) {
        let _ = events_tx.try_send(text);
    }
}

pub fn copy(text: &str) {
    let text = text.to_string();
    thread::spawn(move || {
        let child = Command::new("wl-copy")
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
        if let Ok(mut child) = child {
            if let Some(mut stdin) = child.stdin.take() {
                let _ = stdin.write_all(text.as_bytes());
            }
            let _ = child.wait();
        }
    });
}
