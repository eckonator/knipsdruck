// SPDX-License-Identifier: MIT OR Apache-2.0
//! Gespeicherte Einstellungen als JSON im Konfigurationsordner des Systems
//! (macOS: ~/Library/Application Support/Knipsdruck/einstellungen.json).
//! Fehlende Felder fallen auf Standardwerte zurück.

use std::path::{Path, PathBuf};

use kd_core::DocSettings;
use kd_core::enhance::ColorMode;
use kd_core::page::{Layout, Margin, Orientation, Paper};
use serde::{Deserialize, Serialize};

pub const APP_DIR: &str = "Knipsdruck";

/// Bei Entwicklerläufen (devtools) wird nichts gespeichert.
static READ_ONLY: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn set_read_only() {
    READ_ONLY.store(true, std::sync::atomic::Ordering::Relaxed);
}

pub fn read_only() -> bool {
    READ_ONLY.load(std::sync::atomic::Ordering::Relaxed)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum LangSetting {
    #[default]
    System,
    De,
    En,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ThemeSetting {
    #[default]
    System,
    Light,
    Dark,
}

/// Was „Schwarzweiß“ bedeutet.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum BwStyle {
    /// Wie eine Kopie (Graustufen)
    #[default]
    Gray,
    /// Nur Schwarz und Weiß
    Pure,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum PdfQuality {
    /// 300 dpi
    #[default]
    Print,
    /// 150 dpi, stärker komprimiert
    Email,
}

impl PdfQuality {
    pub fn dpi(self) -> f32 {
        match self {
            PdfQuality::Print => 300.0,
            PdfQuality::Email => 150.0,
        }
    }

    pub fn jpeg(self) -> u8 {
        match self {
            PdfQuality::Print => 88,
            PdfQuality::Email => 62,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub language: LangSetting,
    pub theme: ThemeSetting,
    pub paper: Paper,
    pub orientation: Orientation,
    pub margin: Margin,
    /// Farbe (true) oder Schwarzweiß
    pub color: bool,
    pub bw_style: BwStyle,
    pub pdf_quality: PdfQuality,
    /// Beim Speichern und Versenden Texterkennung ausführen (durchsuchbares PDF)
    pub searchable_pdf: bool,
    /// „Zuletzt verwendet“ führen
    pub recent: bool,
    /// Hinweis zu „Zuletzt verwendet“ schon gezeigt
    pub recent_hint_shown: bool,
    /// Beim Start bei GitHub nach einer neuen Version fragen; `None` = noch nicht gefragt
    pub update_check: Option<bool>,
    /// Port des letzten Handy-Empfangs (eine Firewall-Freigabe gilt dann weiter)
    pub phone_port: Option<u16>,
    /// Automatisch gesetzt, wenn wgpu nicht startete
    pub opengl_fallback: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            language: LangSetting::System,
            theme: ThemeSetting::System,
            paper: default_paper(),
            orientation: Orientation::Auto,
            margin: Margin::Normal,
            color: true,
            bw_style: BwStyle::Gray,
            pdf_quality: PdfQuality::Print,
            searchable_pdf: true,
            recent: true,
            recent_hint_shown: false,
            update_check: None,
            phone_port: None,
            opengl_fallback: false,
        }
    }
}

/// US Letter in den USA, Kanada, Mexiko und auf den Philippinen, sonst A4.
fn default_paper() -> Paper {
    let region = sys_locale::get_locale().unwrap_or_default().to_uppercase();
    let letter = ["-US", "-CA", "-MX", "-PH", "_US", "_CA", "_MX", "_PH"];
    if letter.iter().any(|r| region.ends_with(r)) {
        Paper::Letter
    } else {
        Paper::A4
    }
}

impl Settings {
    pub fn mode(&self) -> ColorMode {
        match (self.color, self.bw_style) {
            (true, _) => ColorMode::Color,
            (false, BwStyle::Gray) => ColorMode::Gray,
            (false, BwStyle::Pure) => ColorMode::BlackWhite,
        }
    }

    pub fn doc(&self, layout: Layout) -> DocSettings {
        DocSettings {
            paper: self.paper,
            orientation: self.orientation,
            margin: self.margin,
            mode: self.mode(),
            layout,
        }
    }

    pub fn load() -> Self {
        let Some(path) = settings_path() else {
            return Settings::default();
        };
        match std::fs::read_to_string(&path) {
            Ok(s) => serde_json::from_str(&s).unwrap_or_else(|e| {
                log::warn!("Einstellungen nicht lesbar ({e}), nehme Standardwerte");
                Settings::default()
            }),
            Err(_) => Settings::default(),
        }
    }

    pub fn save(&self) {
        if read_only() {
            return;
        }
        let Some(path) = settings_path() else { return };
        match serde_json::to_string_pretty(self) {
            Ok(json) => {
                if let Err(e) = write_atomic(&path, json.as_bytes()) {
                    log::error!("Einstellungen nicht gespeichert: {e}");
                }
            }
            Err(e) => log::error!("Einstellungen nicht gespeichert: {e}"),
        }
    }
}

pub fn config_dir() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join(APP_DIR))
}

fn settings_path() -> Option<PathBuf> {
    config_dir().map(|d| d.join("einstellungen.json"))
}

/// Erst in eine Zwischendatei schreiben, dann umbenennen: Ein Absturz hinterlässt nie eine halbe Datei.
pub fn write_atomic(path: &Path, contents: &[u8]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, contents)?;
    std::fs::rename(tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_fields_fall_back_to_defaults() {
        let s: Settings = serde_json::from_str(r#"{"color": false, "bw_style": "Pure"}"#).unwrap();
        assert_eq!(s.mode(), ColorMode::BlackWhite);
        assert!(s.recent);
        let custom = Settings {
            paper: Paper::Custom {
                width_mm: 150.0,
                height_mm: 100.0,
            },
            ..Settings::default()
        };
        let back: Settings = serde_json::from_str(&serde_json::to_string(&custom).unwrap()).unwrap();
        assert_eq!(back.paper, custom.paper);
    }
}
