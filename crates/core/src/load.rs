// SPDX-License-Identifier: MIT OR Apache-2.0
//! Fotos laden: JPEG, PNG, WebP, TIFF, BMP, GIF über `image`, Lage aus den EXIF-Daten angewendet.
//! HEIC (iPhone) kann `image` nicht; auf macOS wandelt es `sips` um, sonst gibt es eine verständliche Meldung.

use std::io::Cursor;
use std::path::Path;

use image::{DynamicImage, ImageDecoder, ImageReader, RgbImage};

/// Größte Kantenlänge, die wir verarbeiten; größere Fotos werden verkleinert (mehr bringt beim Drucken nichts).
pub const MAX_SIDE: u32 = 6000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoadError {
    /// Kein Bild oder ein Format, das wir nicht lesen
    Unsupported,
    /// HEIC/HEIF ohne Umwandlung auf diesem System
    Heic,
    Broken(String),
}

impl std::fmt::Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoadError::Unsupported => write!(f, "kein lesbares Bild"),
            LoadError::Heic => write!(f, "HEIC wird auf diesem System nicht unterstützt"),
            LoadError::Broken(e) => write!(f, "Bild beschädigt: {e}"),
        }
    }
}

pub fn is_heic(bytes: &[u8]) -> bool {
    bytes.len() >= 12
        && &bytes[4..8] == b"ftyp"
        && matches!(
            &bytes[8..12],
            b"heic" | b"heix" | b"hevc" | b"heim" | b"heis" | b"mif1" | b"msf1"
        )
}

/// Liest ein Bild aus dem Speicher und dreht es so, wie die Kamera es gemeint hat.
pub fn decode(bytes: &[u8]) -> Result<RgbImage, LoadError> {
    if is_heic(bytes) {
        return heic_to_jpeg(bytes).and_then(|jpeg| decode(&jpeg));
    }
    let reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|_| LoadError::Unsupported)?;
    if reader.format().is_none() {
        return Err(LoadError::Unsupported);
    }
    let mut decoder = reader.into_decoder().map_err(|e| LoadError::Broken(e.to_string()))?;
    let orientation = decoder.orientation().ok();
    let mut img = DynamicImage::from_decoder(decoder).map_err(|e| LoadError::Broken(e.to_string()))?;
    if let Some(o) = orientation {
        img.apply_orientation(o);
    }
    let img = if img.width().max(img.height()) > MAX_SIDE {
        img.resize(MAX_SIDE, MAX_SIDE, image::imageops::FilterType::Triangle)
    } else {
        img
    };
    Ok(img.into_rgb8())
}

pub fn load_path(path: &Path) -> Result<RgbImage, LoadError> {
    let bytes = std::fs::read(path).map_err(|e| LoadError::Broken(e.to_string()))?;
    decode(&bytes)
}

#[cfg(target_os = "macos")]
fn heic_to_jpeg(bytes: &[u8]) -> Result<Vec<u8>, LoadError> {
    let dir = std::env::temp_dir().join(format!("knipsdruck-heic-{}", std::process::id()));
    std::fs::create_dir_all(&dir).map_err(|e| LoadError::Broken(e.to_string()))?;
    let n = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let (src, dst) = (dir.join(format!("{n}.heic")), dir.join(format!("{n}.jpg")));
    std::fs::write(&src, bytes).map_err(|e| LoadError::Broken(e.to_string()))?;
    let ok = std::process::Command::new("sips")
        .args(["-s", "format", "jpeg", "-s", "formatOptions", "95"])
        .arg(&src)
        .arg("--out")
        .arg(&dst)
        .output()
        .is_ok_and(|o| o.status.success());
    let result = if ok {
        std::fs::read(&dst).map_err(|_| LoadError::Heic)
    } else {
        Err(LoadError::Heic)
    };
    let _ = std::fs::remove_file(&src);
    let _ = std::fs::remove_file(&dst);
    result
}

#[cfg(not(target_os = "macos"))]
fn heic_to_jpeg(_bytes: &[u8]) -> Result<Vec<u8>, LoadError> {
    Err(LoadError::Heic)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_heic_by_brand() {
        let mut b = vec![0, 0, 0, 24];
        b.extend_from_slice(b"ftypheic");
        b.extend_from_slice(&[0; 8]);
        assert!(is_heic(&b));
        b[8..12].copy_from_slice(b"avif");
        assert!(!is_heic(&b));
    }

    #[test]
    fn decodes_png_and_rejects_garbage() {
        let img = RgbImage::from_pixel(3, 2, image::Rgb([10, 20, 30]));
        let mut png = Vec::new();
        DynamicImage::ImageRgb8(img.clone())
            .write_to(&mut Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
        assert_eq!(decode(&png).unwrap(), img);
        assert_eq!(decode(b"kein Bild"), Err(LoadError::Unsupported));
    }
}
