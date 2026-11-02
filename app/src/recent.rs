// SPDX-License-Identifier: MIT OR Apache-2.0
//! „Zuletzt verwendet“: die letzten Dokumente zum erneuten Drucken oder Ändern.
//!
//! Gespeichert werden die Originalfotos und die Einstellungen je Seite (nicht die fertigen Seiten), damit alles
//! änderbar bleibt. Höchstens 10 Dokumente, jedes höchstens 30 Tage. Ausweiskopien nie (entscheidet die App).
//! Ablage: `<Konfig-Ordner>/zuletzt/<id>/` mit `dokument.json`, den Fotos und `vorschau.jpg`. Nur lokal.

use std::path::PathBuf;

use image::RgbImage;
use kd_core::{Detection, PageSettings, Quad, Source};
use serde::{Deserialize, Serialize};

use crate::document::Page;

pub const MAX_ENTRIES: usize = 10;
pub const MAX_AGE_SECS: u64 = 30 * 24 * 3600;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Entry {
    pub id: u64,
    /// Unix-Zeit in Sekunden
    pub saved: u64,
    pub color: bool,
    pub pages: Vec<EntryPage>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EntryPage {
    pub file: String,
    pub name: String,
    pub settings: PageSettings,
    pub auto_quad: Quad,
    pub auto_confidence: u8,
}

impl EntryPage {
    pub fn detection(&self) -> Detection {
        Detection {
            quad: self.auto_quad,
            confidence: self.auto_confidence,
            source: if self.auto_confidence == 0 {
                Source::WholePhoto
            } else {
                Source::Auto
            },
        }
    }
}

pub fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn root() -> Option<PathBuf> {
    crate::settings::config_dir().map(|d| d.join("zuletzt"))
}

pub fn entry_dir(id: u64) -> Option<PathBuf> {
    root().map(|r| r.join(id.to_string()))
}

pub fn preview_path(id: u64) -> Option<PathBuf> {
    entry_dir(id).map(|d| d.join("vorschau.jpg"))
}

/// Alle Einträge, neueste zuerst; zu alte und überzählige werden dabei gelöscht.
pub fn list() -> Vec<Entry> {
    let Some(root) = root() else { return Vec::new() };
    let Ok(dirs) = std::fs::read_dir(&root) else {
        return Vec::new();
    };
    let mut entries: Vec<Entry> = dirs
        .filter_map(Result::ok)
        .filter_map(|d| std::fs::read_to_string(d.path().join("dokument.json")).ok())
        .filter_map(|s| serde_json::from_str::<Entry>(&s).ok())
        .collect();
    entries.sort_by_key(|e| std::cmp::Reverse(e.saved));
    let now = now();
    let (keep, drop): (Vec<_>, Vec<_>) = entries
        .into_iter()
        .enumerate()
        .partition(|(i, e)| *i < MAX_ENTRIES && now.saturating_sub(e.saved) <= MAX_AGE_SECS);
    for (_, e) in drop {
        remove(e.id);
    }
    keep.into_iter().map(|(_, e)| e).collect()
}

/// Ein Dokument merken bzw. seinen Eintrag aktualisieren. Liefert die Id.
pub fn save(id: Option<u64>, pages: &[Page], color: bool, preview: &RgbImage) -> std::io::Result<u64> {
    let id = id.unwrap_or_else(|| now() * 1000 + u64::from(std::process::id() % 1000));
    let dir = entry_dir(id).ok_or_else(|| std::io::Error::other("kein Konfig-Ordner"))?;
    // alte Fotos dieses Eintrags entfernen (Seiten können weggefallen sein)
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir)?;
    let mut entry_pages = Vec::new();
    for (i, p) in pages.iter().enumerate() {
        let file = format!("foto-{:02}.{}", i + 1, extension(&p.photo.original));
        std::fs::write(dir.join(&file), p.photo.original.as_slice())?;
        entry_pages.push(EntryPage {
            file,
            name: p.photo.name.clone(),
            settings: p.settings,
            auto_quad: p.photo.auto.quad,
            auto_confidence: p.photo.auto.confidence,
        });
    }
    let thumb = image::imageops::thumbnail(preview, 264, (264 * preview.height() / preview.width().max(1)).max(1));
    thumb.save(dir.join("vorschau.jpg")).map_err(std::io::Error::other)?;
    let entry = Entry {
        id,
        saved: now(),
        color,
        pages: entry_pages,
    };
    let json = serde_json::to_string_pretty(&entry).map_err(std::io::Error::other)?;
    crate::settings::write_atomic(&dir.join("dokument.json"), json.as_bytes())?;
    // zu alte und überzählige Einträge gleich wegräumen
    let _ = list();
    Ok(id)
}

pub fn photo_bytes(e: &Entry, p: &EntryPage) -> Option<Vec<u8>> {
    std::fs::read(entry_dir(e.id)?.join(&p.file)).ok()
}

pub fn remove(id: u64) {
    if let Some(dir) = entry_dir(id) {
        let _ = std::fs::remove_dir_all(dir);
    }
}

pub fn clear() {
    if let Some(root) = root() {
        let _ = std::fs::remove_dir_all(root);
    }
}

/// Dateiendung nach den ersten Bytes.
fn extension(bytes: &[u8]) -> &'static str {
    match bytes {
        [0xff, 0xd8, ..] => "jpg",
        [0x89, b'P', b'N', b'G', ..] => "png",
        [b'R', b'I', b'F', b'F', _, _, _, _, b'W', b'E', b'B', b'P', ..] => "webp",
        [b'G', b'I', b'F', ..] => "gif",
        [b'B', b'M', ..] => "bmp",
        [b'I', b'I', ..] | [b'M', b'M', ..] => "tif",
        _ if kd_core::load::is_heic(bytes) => "heic",
        _ => "bin",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extensions_by_content() {
        assert_eq!(extension(&[0xff, 0xd8, 0xff]), "jpg");
        assert_eq!(extension(b"\x89PNG\r\n"), "png");
        assert_eq!(extension(b"RIFF1234WEBPVP8"), "webp");
        assert_eq!(extension(b"egal"), "bin");
    }
}
