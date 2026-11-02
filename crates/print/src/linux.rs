// SPDX-License-Identifier: MIT OR Apache-2.0
//! Linux und andere Unix-Systeme ohne Systemdruckdialog: E-Mail über `xdg-email`, Ordner über D-Bus.

use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::{Job, Outcome};

pub use crate::cups::{print_with_lp, printers};

/// So lange warten wir, ob `xdg-email` mit einem Fehler endet. Startet es das E-Mail-Programm direkt,
/// kehrt es erst zurück, wenn dieses geschlossen wird.
const MAIL_START_TIMEOUT: Duration = Duration::from_secs(3);

pub fn print(_job: &Job) -> Result<Outcome, String> {
    Err("Dieses System hat keinen Druckdialog".into())
}

pub fn compose_email(attachment: &Path, subject: &str) -> Result<(), String> {
    let mut child = Command::new("xdg-email")
        .arg("--subject")
        .arg(subject)
        .arg("--attach")
        .arg(attachment)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("xdg-email nicht startbar: {e}"))?;
    let started = Instant::now();
    while started.elapsed() < MAIL_START_TIMEOUT {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => return Ok(()),
            Ok(Some(status)) => return Err(format!("xdg-email fehlgeschlagen ({status})")),
            Ok(None) => std::thread::sleep(Duration::from_millis(50)),
            Err(e) => return Err(format!("xdg-email: {e}")),
        }
    }
    // Läuft noch, also offenbar das E-Mail-Programm selbst; im Hintergrund aufräumen.
    std::thread::spawn(move || child.wait());
    Ok(())
}

pub fn reveal(path: &Path) -> Result<(), String> {
    match show_items(path) {
        Ok(()) => Ok(()),
        Err(e) => {
            log::debug!("FileManager1.ShowItems nicht möglich, öffne Ordner: {e}");
            open_parent(path)
        }
    }
}

/// Bittet den Dateimanager über `org.freedesktop.FileManager1`, die Datei zu markieren.
fn show_items(path: &Path) -> Result<(), String> {
    let status = Command::new("dbus-send")
        .args([
            "--session",
            "--print-reply",
            "--dest=org.freedesktop.FileManager1",
            "--type=method_call",
            "/org/freedesktop/FileManager1",
            "org.freedesktop.FileManager1.ShowItems",
        ])
        .arg(format!("array:string:{}", file_uri(path)))
        .arg("string:")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|e| format!("dbus-send nicht startbar: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("dbus-send fehlgeschlagen ({status})"))
    }
}

fn open_parent(path: &Path) -> Result<(), String> {
    let dir = path.parent().unwrap_or(path);
    Command::new("xdg-open")
        .arg(dir)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(drop)
        .map_err(|e| format!("xdg-open nicht startbar: {e}"))
}

/// `file://`-URI mit prozentkodierten Sonderzeichen. Kommas würden `dbus-send` das Array zerteilen,
/// deshalb wird alles außer den unreservierten Zeichen und `/` kodiert.
fn file_uri(path: &Path) -> String {
    use std::fmt::Write;
    use std::os::unix::ffi::OsStrExt;
    let mut uri = String::from("file://");
    for &b in path.as_os_str().as_bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'/' | b'-' | b'_' | b'.' | b'~') {
            uri.push(b as char);
        } else {
            let _ = write!(uri, "%{b:02X}");
        }
    }
    uri
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uri_kodiert_sonderzeichen() {
        assert_eq!(
            file_uri(Path::new("/home/a/Brief, März.pdf")),
            "file:///home/a/Brief%2C%20M%C3%A4rz.pdf"
        );
    }
}
