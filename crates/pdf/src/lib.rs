// SPDX-License-Identifier: MIT OR Apache-2.0
//! PDF aus fertigen Seitenbildern: je Seite ein JPEG (DCTDecode) in exakter Papiergröße.
//! Bewusst klein gehalten – mehr als Bilder auf Seiten braucht Knipsdruck nicht. Auf Wunsch liegt über dem Bild
//! unsichtbarer Text aus der Texterkennung (Darstellungsmodus 3), damit sich das PDF durchsuchen lässt.

use std::io::{self, Write};
use std::path::Path;

use image::RgbImage;
use image::codecs::jpeg::JpegEncoder;

pub struct Page<'a> {
    pub image: &'a RgbImage,
    pub width_mm: f32,
    pub height_mm: f32,
    /// Erkannte Wörter in Bildpixeln (leer = keine Textebene)
    pub words: &'a [Word],
}

/// Ein Wort der Textebene, Kasten in Pixeln des Seitenbilds (Ursprung oben links).
#[derive(Debug, Clone, PartialEq)]
pub struct Word {
    pub text: String,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

#[derive(Clone, Debug)]
pub struct Options {
    pub title: String,
    /// JPEG-Qualität 1–100
    pub quality: u8,
    /// Seiten als Graustufen speichern (kleiner, für Schwarzweiß)
    pub gray: bool,
}

const PT_PER_MM: f32 = 72.0 / 25.4;

/// Schreibt das PDF erst in eine Zwischendatei und benennt sie dann um.
pub fn write_file(path: &Path, pages: &[Page], opts: &Options) -> io::Result<()> {
    let bytes = to_bytes(pages, opts)?;
    let tmp = path.with_extension("pdf.tmp");
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(tmp, path)
}

pub fn to_bytes(pages: &[Page], opts: &Options) -> io::Result<Vec<u8>> {
    let mut w = Writer::default();
    w.out.extend_from_slice(b"%PDF-1.4\n%\xE2\xE3\xCF\xD3\n");
    // Objektnummern: 1 Katalog, 2 Seitenbaum, 3 Info, 4 Schrift der Textebene, danach je Seite: Seite, Inhalt, Bild
    let page_ids: Vec<usize> = (0..pages.len()).map(|i| 5 + i * 3).collect();
    w.object(1, b"<< /Type /Catalog /Pages 2 0 R >>")?;
    let kids: Vec<String> = page_ids.iter().map(|id| format!("{id} 0 R")).collect();
    w.object(
        2,
        format!("<< /Type /Pages /Kids [{}] /Count {} >>", kids.join(" "), pages.len()).as_bytes(),
    )?;
    w.object(
        3,
        format!(
            "<< /Title {} /Producer {} /Creator (Knipsdruck) /CreationDate (D:{}) >>",
            text_string(&opts.title),
            text_string(&format!("Knipsdruck {}", env!("CARGO_PKG_VERSION"))),
            pdf_date()
        )
        .as_bytes(),
    )?;
    w.object(
        4,
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>",
    )?;
    for (page, &id) in pages.iter().zip(&page_ids) {
        let (wpt, hpt) = (page.width_mm * PT_PER_MM, page.height_mm * PT_PER_MM);
        let (jpeg, gray) = encode(page.image, opts)?;
        w.object(
            id,
            format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {wpt:.3} {hpt:.3}] \
                 /Resources << /XObject << /Im0 {} 0 R >> /Font << /F0 4 0 R >> >> /Contents {} 0 R >>",
                id + 2,
                id + 1
            )
            .as_bytes(),
        )?;
        let mut content = format!("q {wpt:.3} 0 0 {hpt:.3} 0 0 cm /Im0 Do Q\n");
        text_layer(&mut content, page, wpt / page.image.width().max(1) as f32, hpt);
        w.stream(id + 1, "", content.as_bytes())?;
        let dict = format!(
            "/Type /XObject /Subtype /Image /Width {} /Height {} /ColorSpace {} /BitsPerComponent 8 /Filter /DCTDecode",
            page.image.width(),
            page.image.height(),
            if gray { "/DeviceGray" } else { "/DeviceRGB" }
        );
        w.stream(id + 2, &dict, &jpeg)?;
    }
    w.finish(4 + pages.len() * 3)
}

/// Unsichtbarer Text an den Positionen der Wörter; `scale` = Punkt je Bildpixel, `page_h` in Punkt.
fn text_layer(content: &mut String, page: &Page, scale: f32, page_h: f32) {
    for w in page.words {
        let text = win_ansi(&w.text);
        if text.is_empty() || w.w <= 0.0 || w.h <= 0.0 {
            continue;
        }
        let (bw, bh) = (w.w * scale, w.h * scale);
        let size = (bh * 0.85).max(1.0);
        // Breite anpassen: Helvetica ist im Mittel etwa eine halbe Schriftgröße breit je Zeichen
        let natural = 0.52 * size * text.len() as f32;
        let stretch = (100.0 * bw / natural).clamp(10.0, 1000.0);
        let x = w.x * scale;
        let y = page_h - (w.y + w.h) * scale + 0.2 * bh;
        content.push_str(&format!(
            "BT 3 Tr /F0 {size:.2} Tf {stretch:.1} Tz 1 0 0 1 {x:.2} {y:.2} Tm ({}) Tj ET\n",
            escape_literal(&text)
        ));
    }
}

/// Text in WinAnsi (Windows-1252) für die Standardschrift; unbekannte Zeichen werden „?“.
fn win_ansi(s: &str) -> Vec<u8> {
    s.chars()
        .map(|c| match c {
            ' '..='~' => c as u8,
            '\u{a0}'..='\u{ff}' => c as u32 as u8,
            '€' => 0x80,
            '‚' => 0x82,
            '„' => 0x84,
            '…' => 0x85,
            '‘' => 0x91,
            '’' => 0x92,
            '“' => 0x93,
            '”' => 0x94,
            '–' => 0x96,
            '—' => 0x97,
            _ => b'?',
        })
        .collect()
}

/// PDF-Literal: Klammern und Backslash maskiert, Bytes außerhalb von ASCII als Oktal-Escape.
fn escape_literal(bytes: &[u8]) -> String {
    let mut out = String::new();
    for &b in bytes {
        match b {
            b'(' | b')' | b'\\' => {
                out.push('\\');
                out.push(b as char);
            }
            0x20..=0x7e => out.push(b as char),
            _ => out.push_str(&format!("\\{b:03o}")),
        }
    }
    out
}

fn encode(img: &RgbImage, opts: &Options) -> io::Result<(Vec<u8>, bool)> {
    let mut out = Vec::new();
    let mut enc = JpegEncoder::new_with_quality(&mut out, opts.quality.clamp(1, 100));
    let result = if opts.gray {
        let gray = image::DynamicImage::ImageRgb8(img.clone()).into_luma8();
        enc.encode_image(&gray)
    } else {
        enc.encode_image(img)
    };
    result.map_err(io::Error::other)?;
    Ok((out, opts.gray))
}

#[derive(Default)]
struct Writer {
    out: Vec<u8>,
    offsets: Vec<(usize, usize)>,
}

impl Writer {
    fn object(&mut self, id: usize, body: &[u8]) -> io::Result<()> {
        self.offsets.push((id, self.out.len()));
        writeln!(self.out, "{id} 0 obj")?;
        self.out.extend_from_slice(body);
        self.out.extend_from_slice(b"\nendobj\n");
        Ok(())
    }

    fn stream(&mut self, id: usize, dict: &str, data: &[u8]) -> io::Result<()> {
        self.offsets.push((id, self.out.len()));
        write!(self.out, "{id} 0 obj\n<< {dict} /Length {} >>\nstream\n", data.len())?;
        self.out.extend_from_slice(data);
        self.out.extend_from_slice(b"\nendstream\nendobj\n");
        Ok(())
    }

    fn finish(mut self, max_id: usize) -> io::Result<Vec<u8>> {
        self.offsets.sort_unstable();
        let xref = self.out.len();
        write!(self.out, "xref\n0 {}\n0000000000 65535 f \n", max_id + 1)?;
        for (_, off) in &self.offsets {
            writeln!(self.out, "{off:010} 00000 n ")?;
        }
        write!(
            self.out,
            "trailer\n<< /Size {} /Root 1 0 R /Info 3 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            max_id + 1
        )?;
        Ok(self.out)
    }
}

/// Text als UTF-16BE mit BOM (für Umlaute im Titel), hexadezimal.
fn text_string(s: &str) -> String {
    let mut hex = String::from("<FEFF");
    for u in s.encode_utf16() {
        hex.push_str(&format!("{u:04X}"));
    }
    hex.push('>');
    hex
}

/// Aktuelle Zeit als PDF-Datum (UTC).
fn pdf_date() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    let (y, m, d) = civil_from_days(days);
    format!(
        "{y:04}{m:02}{d:02}{:02}{:02}{:02}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

/// Tage seit 1970-01-01 → (Jahr, Monat, Tag), nach Howard Hinnant.
fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_a_valid_looking_pdf() {
        let img = RgbImage::from_pixel(20, 30, image::Rgb([250, 250, 250]));
        let words = [Word {
            text: "Übung (1)".into(),
            x: 2.0,
            y: 3.0,
            w: 10.0,
            h: 2.0,
        }];
        let pages = [
            Page {
                image: &img,
                width_mm: 210.0,
                height_mm: 297.0,
                words: &words,
            },
            Page {
                image: &img,
                width_mm: 297.0,
                height_mm: 210.0,
                words: &[],
            },
        ];
        let opts = Options {
            title: "Prüfung".into(),
            quality: 80,
            gray: false,
        };
        let pdf = to_bytes(&pages, &opts).unwrap();
        let text = String::from_utf8_lossy(&pdf);
        assert!(text.starts_with("%PDF-1.4"));
        assert!(text.contains("/Count 2"));
        assert!(text.contains("/MediaBox [0 0 595.276 841.890]"));
        assert!(text.contains("/MediaBox [0 0 841.890 595.276]"));
        assert!(text.contains("<FEFF0050007200FC00660075006E0067>"));
        assert!(text.contains(r"(\334bung \(1\)) Tj"), "Textebene fehlt");
        // Verweis auf die xref-Tabelle stimmt
        let start: usize = text
            .rsplit("startxref\n")
            .next()
            .unwrap()
            .lines()
            .next()
            .unwrap()
            .parse()
            .unwrap();
        assert!(pdf[start..].starts_with(b"xref"));
    }

    #[test]
    fn dates() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(20_759), (2026, 11, 2));
    }
}
