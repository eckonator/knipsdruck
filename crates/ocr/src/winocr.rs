// SPDX-License-Identifier: MIT OR Apache-2.0
//! Windows.Media.Ocr: in Windows 10/11 eingebaut. Nimmt die Sprachen des Benutzerprofils,
//! sonst Deutsch oder Englisch, falls das Sprachpaket installiert ist.

use image::RgbImage;
use image::imageops::FilterType;
use windows::Globalization::Language;
use windows::Graphics::Imaging::{BitmapAlphaMode, BitmapPixelFormat, SoftwareBitmap};
use windows::Media::Ocr::OcrEngine;
use windows::Storage::Streams::DataWriter;
use windows::core::HSTRING;

use crate::{Engine, Word};

pub struct WindowsOcr;

fn engine() -> Option<OcrEngine> {
    OcrEngine::TryCreateFromUserProfileLanguages().ok().or_else(|| {
        ["de-DE", "en-US"].iter().find_map(|tag| {
            let lang = Language::CreateLanguage(&HSTRING::from(*tag)).ok()?;
            OcrEngine::TryCreateFromLanguage(&lang).ok()
        })
    })
}

impl Engine for WindowsOcr {
    fn name(&self) -> &'static str {
        "Windows OCR"
    }

    fn available(&self) -> bool {
        engine().is_some()
    }

    fn recognize(&self, image: &RgbImage) -> Result<Vec<Word>, String> {
        let err = |e: windows::core::Error| format!("Windows OCR: {}", e.message());
        let engine = engine().ok_or("Windows OCR: keine Sprache verfügbar")?;

        // Größere Bilder lehnt die Engine ab: verkleinern und die Kästen danach zurückrechnen
        let max = OcrEngine::MaxImageDimension().map_err(err)?.max(1);
        let longest = image.width().max(image.height());
        let scaled;
        let (src, factor) = if longest > max {
            let f = max as f32 / longest as f32;
            let nw = ((image.width() as f32 * f) as u32).clamp(1, max);
            let nh = ((image.height() as f32 * f) as u32).clamp(1, max);
            scaled = image::imageops::resize(image, nw, nh, FilterType::Triangle);
            (&scaled, image.width() as f32 / nw as f32)
        } else {
            (image, 1.0)
        };

        // Windows erwartet BGRA
        let mut bgra = Vec::with_capacity(src.as_raw().len() / 3 * 4);
        for px in src.pixels() {
            let [r, g, b] = px.0;
            bgra.extend_from_slice(&[b, g, r, 255]);
        }
        let writer = DataWriter::new().map_err(err)?;
        writer.WriteBytes(&bgra).map_err(err)?;
        let buffer = writer.DetachBuffer().map_err(err)?;
        let bitmap = SoftwareBitmap::CreateCopyWithAlphaFromBuffer(
            &buffer,
            BitmapPixelFormat::Bgra8,
            src.width() as i32,
            src.height() as i32,
            BitmapAlphaMode::Premultiplied,
        )
        .map_err(err)?;
        let result = engine.RecognizeAsync(&bitmap).map_err(err)?.join().map_err(err)?;
        let mut words = Vec::new();
        for line in result.Lines().map_err(err)? {
            for word in line.Words().map_err(err)? {
                let r = word.BoundingRect().map_err(err)?;
                words.push(Word {
                    text: word.Text().map_err(err)?.to_string(),
                    x: r.X * factor,
                    y: r.Y * factor,
                    w: r.Width * factor,
                    h: r.Height * factor,
                });
            }
        }
        Ok(words)
    }
}
