// SPDX-License-Identifier: MIT OR Apache-2.0
//! Übergabe der fertigen Seiten an das Betriebssystem: drucken, per E-Mail verschicken, im Ordner zeigen.
//!
//! macOS: Systemdruckdialog über PDFKit (gedruckt wird das PDF), E-Mail über `NSSharingService`.
//! Windows: Systemdruckdialog `PrintDlgExW`, die Seitenbilder gehen per GDI in exakter Größe an den
//! Drucker; E-Mail über Simple MAPI.
//! Linux: Kein Systemdialog. Die App zeigt einen eigenen Dialog und druckt über CUPS (`lp`);
//! E-Mail über `xdg-email`.

use std::path::Path;

mod media;

#[cfg(unix)]
mod cups;
#[cfg(all(unix, not(target_os = "macos")))]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(windows)]
mod windows;
#[cfg(any(windows, test))]
mod winjob;

#[cfg(all(unix, not(target_os = "macos")))]
use linux as sys;
#[cfg(target_os = "macos")]
use macos as sys;
#[cfg(windows)]
use windows as sys;

pub use media::cups_media;

/// Eine fertige Seite: Bild in Druckauflösung inkl. Rand, plus ihre physische Größe.
pub struct Page<'a> {
    pub image: &'a image::RgbImage,
    pub width_mm: f32,
    pub height_mm: f32,
}

/// Ein Druckauftrag.
pub struct Job<'a> {
    /// Dokumentname für die Druckerwarteschlange.
    pub name: &'a str,
    /// In Reihenfolge; Seiten können hoch und quer gemischt sein.
    pub pages: &'a [Page<'a>],
    /// Dieselben Seiten bereits als PDF geschrieben (macOS/Linux drucken das PDF).
    pub pdf: &'a Path,
}

/// Ergebnis des Druckdialogs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Sent,
    Cancelled,
}

/// Ein CUPS-Drucker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Printer {
    /// Name der Warteschlange, so wie `lp -d` ihn erwartet.
    pub name: String,
    /// Lesbare Beschreibung; leer, wenn CUPS keine kennt.
    pub description: String,
    pub is_default: bool,
}

/// Zeigt den Druckdialog des Systems und druckt. Muss auf dem Haupt-Thread aufgerufen werden (macOS),
/// blockiert bis der Dialog zu ist.
pub fn print(job: &Job) -> Result<Outcome, String> {
    if job.pages.is_empty() {
        return Err("Keine Seiten zum Drucken".into());
    }
    sys::print(job)
}

/// Ob [`print`] einen Systemdialog zeigen kann. Sonst braucht es [`printers`] und [`print_with_lp`].
pub fn system_dialog_available() -> bool {
    cfg!(any(target_os = "macos", windows))
}

/// Die CUPS-Drucker (Linux, auf macOS als Rückfall). Leer, wenn CUPS fehlt oder nichts eingerichtet ist.
pub fn printers() -> Vec<Printer> {
    sys::printers()
}

/// Druckt das PDF über CUPS in Originalgröße. `media` ist ein CUPS-Name, z. B. aus [`cups_media`].
pub fn print_with_lp(pdf: &Path, printer: &str, copies: u32, media: &str) -> Result<(), String> {
    sys::print_with_lp(pdf, printer, copies, media)
}

/// E-Mail mit Anhang im E-Mail-Programm öffnen. `Err(..)` heißt: kein passendes Programm, die App zeigt
/// dann die Datei im Ordner. Auf macOS nur auf dem Haupt-Thread.
pub fn compose_email(attachment: &Path, subject: &str) -> Result<(), String> {
    sys::compose_email(&absolute(attachment)?, subject)
}

/// Datei im Finder/Explorer/Dateimanager markiert zeigen.
pub fn reveal(path: &Path) -> Result<(), String> {
    sys::reveal(&absolute(path)?)
}

/// Absoluter Pfad ohne `\\?\`-Präfix (das `canonicalize` auf Windows liefert und das MAPI nicht mag).
fn absolute(path: &Path) -> Result<std::path::PathBuf, String> {
    std::path::absolute(path).map_err(|e| format!("Pfad {} nicht auflösbar: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Manuell: `cargo test -p kd-print -- --ignored --test-threads=1` öffnet Mail und Finder.
    #[test]
    #[ignore]
    fn mail_und_ordner_oeffnen() {
        let file = std::env::temp_dir().join("kd-print-test.txt");
        std::fs::write(&file, "Knipsdruck-Test").unwrap();
        eprintln!("compose_email: {:?}", compose_email(&file, "Knipsdruck-Test"));
        eprintln!("reveal: {:?}", reveal(&file));
        eprintln!("printers: {:?}", printers());
    }

    #[test]
    fn leerer_auftrag_ist_fehler() {
        let job = Job {
            name: "leer",
            pages: &[],
            pdf: Path::new("/gibt/es/nicht.pdf"),
        };
        assert!(print(&job).is_err());
    }
}
