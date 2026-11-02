// SPDX-License-Identifier: MIT OR Apache-2.0
//! Texterkennung (OCR) für durchsuchbare PDFs. Je System die eingebaute Engine:
//! Apple Vision (macOS), Windows.Media.Ocr (Windows), sonst Tesseract als externes Programm.

mod tesseract;
#[cfg(target_os = "macos")]
mod vision;
#[cfg(windows)]
mod winocr;

use image::RgbImage;

/// Ein erkanntes Wort mit Kasten in Bildpixeln (Ursprung oben links).
#[derive(Debug, Clone, PartialEq)]
pub struct Word {
    pub text: String,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

pub trait Engine: Send + Sync {
    /// Anzeigename, z. B. „Apple Vision“
    fn name(&self) -> &'static str;
    /// Ob die Engine auf diesem Rechner nutzbar ist (Sprachpaket, Programm installiert …)
    fn available(&self) -> bool;
    /// Erkennt die Wörter eines Seitenbilds; Kästen in Pixeln des übergebenen Bilds.
    fn recognize(&self, image: &RgbImage) -> Result<Vec<Word>, String>;
}

/// Systemeigene Engine (Apple Vision auf macOS, Windows.Media.Ocr auf Windows), sonst Tesseract, falls installiert.
pub fn engine() -> Option<Box<dyn Engine>> {
    let tesseract: Box<dyn Engine> = Box::new(tesseract::Tesseract);
    native().into_iter().chain([tesseract]).find(|e| e.available())
}

/// Die im Betriebssystem eingebaute Engine, falls es eine gibt.
fn native() -> Option<Box<dyn Engine>> {
    #[cfg(target_os = "macos")]
    {
        Some(Box::new(vision::Vision))
    }
    #[cfg(windows)]
    {
        Some(Box::new(winocr::WindowsOcr))
    }
    #[cfg(not(any(target_os = "macos", windows)))]
    {
        None
    }
}

/// Für die Einstellungen: Name der verfügbaren Engine oder None („Auf diesem Computer ist keine Texterkennung verfügbar“).
pub fn available_engine_name() -> Option<&'static str> {
    engine().map(|e| e.name())
}

/// Lädt das Modell der systemeigenen Engine im Hintergrund vor. Apple Vision braucht beim ersten Aufruf
/// nach dem Systemstart bis zu einer halben Minute, danach Sekundenbruchteile.
pub fn warm_up() {
    std::thread::Builder::new()
        .name("kd-ocr-warm-up".into())
        .spawn(|| {
            if let Some(engine) = native().filter(|e| e.available()) {
                let _ = engine.recognize(&RgbImage::from_pixel(64, 32, image::Rgb([255; 3])));
            }
        })
        .ok();
}

/// PNG für Engines, die ein Bild als Datei bzw. Datenblock erwarten.
pub(crate) fn encode_png(image: &RgbImage) -> Result<Vec<u8>, String> {
    use image::ImageEncoder;
    use image::codecs::png::{CompressionType, FilterType, PngEncoder};
    let mut out = Vec::new();
    PngEncoder::new_with_quality(&mut out, CompressionType::Fast, FilterType::Adaptive)
        .write_image(
            image.as_raw(),
            image.width(),
            image.height(),
            image::ExtendedColorType::Rgb8,
        )
        .map_err(|e| format!("PNG: {e}"))?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn png_roundtrip() {
        let img = RgbImage::from_pixel(7, 3, image::Rgb([10, 20, 30]));
        let png = encode_png(&img).unwrap();
        let back = image::load_from_memory(&png).unwrap().to_rgb8();
        assert_eq!(back, img);
    }
}
