// SPDX-License-Identifier: MIT OR Apache-2.0
//! Der Name des Computers, wie ihn Menschen kennen („Büro-PC“), für die Anzeige auf der Handyseite.

/// Höchstlänge des angezeigten Namens in Zeichen.
const MAX_CHARS: usize = 60;

/// Hübscher Computername: macOS `scutil --get ComputerName`, Windows `%COMPUTERNAME%`, Linux `/etc/hostname`;
/// Ersatz „Computer“.
pub fn computer_name() -> String {
    system_name()
        .map(|n| tidy(&n))
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| "Computer".into())
}

/// Entfernt Steuerzeichen und Leerraum, kürzt auf eine anzeigbare Länge.
fn tidy(raw: &str) -> String {
    let name: String = raw.chars().filter(|c| !c.is_control()).take(MAX_CHARS).collect();
    name.trim().to_string()
}

#[cfg(target_os = "macos")]
fn system_name() -> Option<String> {
    let out = std::process::Command::new("scutil")
        .args(["--get", "ComputerName"])
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

#[cfg(windows)]
fn system_name() -> Option<String> {
    std::env::var("COMPUTERNAME").ok()
}

#[cfg(not(any(target_os = "macos", windows)))]
fn system_name() -> Option<String> {
    let raw = std::fs::read_to_string("/etc/hostname")
        .ok()
        .or_else(|| std::env::var("HOSTNAME").ok())?;
    // „buero-pc.fritz.box“ → „buero-pc“
    raw.split('.').next().map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tidies() {
        assert_eq!(tidy("Büro-PC\n"), "Büro-PC");
        assert_eq!(tidy(&"a".repeat(100)).len(), MAX_CHARS);
        assert!(!computer_name().is_empty());
    }
}
