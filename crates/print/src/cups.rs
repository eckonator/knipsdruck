// SPDX-License-Identifier: MIT OR Apache-2.0
//! Drucken über CUPS mit den Kommandozeilenwerkzeugen `lpstat` und `lp`.

use std::path::Path;
use std::process::Command;

use crate::Printer;

/// Alle eingerichteten Drucker; der Standarddrucker steht vorne.
pub fn printers() -> Vec<Printer> {
    let Some(names) = lpstat(&["-e"]) else {
        return Vec::new();
    };
    let default = lpstat(&["-d"]).and_then(|out| parse_default(&out));
    let descriptions = lpstat(&["-l", "-p"])
        .map(|out| parse_descriptions(&out))
        .unwrap_or_default();
    let mut printers: Vec<Printer> = names
        .lines()
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(|name| Printer {
            name: name.to_string(),
            description: descriptions
                .iter()
                .find(|(n, _)| n == name)
                .map(|(_, d)| d.clone())
                .unwrap_or_default(),
            is_default: default.as_deref() == Some(name),
        })
        .collect();
    printers.sort_by_key(|p| !p.is_default);
    printers
}

/// Druckt das PDF ohne Skalierung auf dem gewünschten Papier.
pub fn print_with_lp(pdf: &Path, printer: &str, copies: u32, media: &str) -> Result<(), String> {
    let output = english(&mut Command::new("lp"))
        .arg("-d")
        .arg(printer)
        .arg("-n")
        .arg(copies.max(1).to_string())
        .arg("-o")
        .arg(format!("media={media}"))
        .arg("-o")
        .arg("fit-to-page=false")
        .arg("--")
        .arg(pdf)
        .output()
        .map_err(|e| format!("lp nicht startbar: {e}"))?;
    if output.status.success() {
        log::info!(
            "Druckauftrag an {printer}: {}",
            String::from_utf8_lossy(&output.stdout).trim()
        );
        Ok(())
    } else {
        Err(format!(
            "lp fehlgeschlagen ({}): {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ))
    }
}

/// Führt `lpstat` mit englischer Ausgabe aus; `None`, wenn es fehlt oder scheitert.
fn lpstat(args: &[&str]) -> Option<String> {
    let output = english(&mut Command::new("lpstat")).args(args).output().ok()?;
    if !output.status.success() {
        log::debug!("lpstat {args:?}: {}", String::from_utf8_lossy(&output.stderr).trim());
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Englische Ausgabe erzwingen. Das CUPS von macOS richtet sich sonst nach der Systemsprache und
/// beachtet `LANG` nur, wenn `SOFTWARE` gesetzt ist (wie bei CUPS-Filtern).
fn english(cmd: &mut Command) -> &mut Command {
    cmd.env("LC_ALL", "C").env("LANG", "C").env("SOFTWARE", "CUPS")
}

/// Name aus `system default destination: NAME`. Zur Sicherheit auch übersetzt: der Text nach dem
/// letzten Doppelpunkt der ersten Zeile.
fn parse_default(out: &str) -> Option<String> {
    let line = out.lines().next()?.trim();
    let name = line
        .strip_prefix("system default destination:")
        .or_else(|| line.rsplit_once(": ").map(|(_, n)| n))?;
    Some(name.trim().to_string()).filter(|name| !name.is_empty())
}

/// Paare (Drucker, Beschreibung) aus `lpstat -l -p`: Kopfzeile `printer NAME ...`, darunter eingerückt
/// `Description: ...`.
fn parse_descriptions(out: &str) -> Vec<(String, String)> {
    let mut result = Vec::new();
    let mut current: Option<String> = None;
    for line in out.lines() {
        if let Some(rest) = line.strip_prefix("printer ") {
            current = rest.split_whitespace().next().map(str::to_string);
        } else if let Some(desc) = line.trim().strip_prefix("Description:")
            && let Some(name) = current.take()
        {
            result.push((name, desc.trim().to_string()));
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standarddrucker() {
        assert_eq!(
            parse_default("system default destination: HP_LaserJet\n").as_deref(),
            Some("HP_LaserJet")
        );
        assert_eq!(
            parse_default("System-Standardzielort: Canon\n").as_deref(),
            Some("Canon")
        );
        assert_eq!(parse_default("no system default destination\n"), None);
    }

    #[test]
    fn beschreibungen() {
        let out = "printer HP_LaserJet is idle.  enabled since Mon Nov  2 10:00:00 2026\n\
                   \tForm mounts:\n\
                   \tDescription: HP LaserJet im Flur\n\
                   \tLocation: Flur\n\
                   printer PDF disabled since Mon Nov  2 10:00:00 2026 -\n\
                   \tDescription: \n";
        assert_eq!(
            parse_descriptions(out),
            vec![
                ("HP_LaserJet".into(), "HP LaserJet im Flur".into()),
                ("PDF".into(), String::new())
            ]
        );
    }
}
