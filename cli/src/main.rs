// SPDX-License-Identifier: MIT OR Apache-2.0
//! Knipsdruck auf der Kommandozeile.
//!
//! ```text
//! knipsdruck-cli pdf [Optionen] -o ausgabe.pdf foto1.jpg foto2.jpg …
//! knipsdruck-cli ecken foto1.jpg …        (erkannte Ecken als TSV: Datei, Sicherheit, x0 y0 … x3 y3)
//! ```

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use kd_core::enhance::ColorMode;
use kd_core::page::{Layout, Margin, Orientation, Paper};
use kd_core::{DocSettings, PageSettings};

const HELP: &str = "\
Aufruf:
  knipsdruck-cli pdf [Optionen] -o ausgabe.pdf FOTO…
  knipsdruck-cli ecken FOTO…

Optionen für pdf:
  --papier A3|A4|A5|A6|B5|Letter|Legal|BREITExHÖHE   (z. B. 150x100, in mm; Standard A4)
  --hoch | --quer          Ausrichtung erzwingen (Standard: wie das Blatt liegt)
  --rand ohne|schmal|normal
  --grau | --sw            Graustufen wie eine Kopie bzw. nur Schwarz und Weiß
  --ausweis                Ausweiskopie: je zwei Karten in Originalgröße auf ein Blatt
  --dpi N                  Auflösung (Standard 300)
  --qualitaet N            JPEG-Qualität 1–100 (Standard 88)
  --debug-dir ORDNER       Maske der Erkennung und Zwischenschritte ablegen
";

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("pdf") => pdf(&args[1..]),
        Some("ecken") => corners(&args[1..]),
        _ => {
            print!("{HELP}");
            Ok(())
        }
    }
}

fn corners(files: &[String]) -> Result<()> {
    for f in files {
        let photo = kd_core::load_path(Path::new(f)).map_err(|e| anyhow::anyhow!("{f}: {e}"))?;
        let d = kd_core::detect(&photo);
        let pts: Vec<String> = d.quad.0.iter().map(|p| format!("{:.0} {:.0}", p[0], p[1])).collect();
        println!("{f}\t{}\t{}", d.confidence, pts.join("\t"));
    }
    Ok(())
}

fn parse_paper(s: &str) -> Result<Paper> {
    Ok(match s.to_ascii_lowercase().as_str() {
        "a3" => Paper::A3,
        "a4" => Paper::A4,
        "a5" => Paper::A5,
        "a6" => Paper::A6,
        "b5" => Paper::B5,
        "letter" => Paper::Letter,
        "legal" => Paper::Legal,
        other => {
            let (w, h) = other.split_once('x').context("Papierformat unbekannt")?;
            Paper::Custom {
                width_mm: w.parse().context("Breite")?,
                height_mm: h.parse().context("Höhe")?,
            }
        }
    })
}

fn pdf(args: &[String]) -> Result<()> {
    let mut doc = DocSettings::default();
    let (mut out, mut debug_dir, mut files) = (None, None, Vec::new());
    let (mut dpi, mut quality) = (300.0f32, 88u8);
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let mut value = || it.next().with_context(|| format!("{a} braucht einen Wert"));
        match a.as_str() {
            "-o" => out = Some(PathBuf::from(value()?)),
            "--papier" => doc.paper = parse_paper(value()?)?,
            "--hoch" => doc.orientation = Orientation::Portrait,
            "--quer" => doc.orientation = Orientation::Landscape,
            "--rand" => {
                doc.margin = match value()?.as_str() {
                    "ohne" => Margin::None,
                    "schmal" => Margin::Narrow,
                    _ => Margin::Normal,
                }
            }
            "--grau" => doc.mode = ColorMode::Gray,
            "--sw" => doc.mode = ColorMode::BlackWhite,
            "--ausweis" => doc.layout = Layout::IdCard,
            "--dpi" => dpi = value()?.parse().context("--dpi")?,
            "--qualitaet" => quality = value()?.parse().context("--qualitaet")?,
            "--debug-dir" => debug_dir = Some(PathBuf::from(value()?)),
            f if f.starts_with('-') => bail!("unbekannte Option {f}\n\n{HELP}"),
            f => files.push(PathBuf::from(f)),
        }
    }
    let out = out.context("-o ausgabe.pdf fehlt")?;
    if files.is_empty() {
        bail!("keine Fotos angegeben");
    }
    let mut photos = Vec::new();
    for f in &files {
        let photo = kd_core::load_path(f).map_err(|e| anyhow::anyhow!("{}: {e}", f.display()))?;
        let (det, dbg) = kd_core::detect::detect_with_debug(&photo);
        eprintln!("{}: Sicherheit {}", f.display(), det.confidence);
        if let Some(dir) = &debug_dir {
            std::fs::create_dir_all(dir)?;
            let stem = f.file_stem().unwrap_or_default().to_string_lossy();
            dbg.mask.save(dir.join(format!("{stem}-maske.png")))?;
            outline(&photo, &det.quad).save(dir.join(format!("{stem}-ecken.png")))?;
        }
        let ps = PageSettings {
            quad: det.quad,
            source: det.source,
            rotation: 0,
        };
        photos.push((photo, ps));
    }
    let refs: Vec<_> = photos.iter().map(|(p, s)| (p, *s)).collect();
    let pages = kd_core::render_document(&refs, &doc, dpi, |_| {});
    if let Some(dir) = &debug_dir {
        for (i, p) in pages.iter().enumerate() {
            p.image.save(dir.join(format!("seite-{}.png", i + 1)))?;
        }
    }
    let pdf_pages: Vec<kd_pdf::Page> = pages
        .iter()
        .map(|p| kd_pdf::Page {
            image: &p.image,
            width_mm: p.width_mm,
            height_mm: p.height_mm,
            words: &[],
        })
        .collect();
    let opts = kd_pdf::Options {
        title: out.file_stem().unwrap_or_default().to_string_lossy().into_owned(),
        quality,
        gray: doc.mode != ColorMode::Color,
    };
    kd_pdf::write_file(&out, &pdf_pages, &opts).with_context(|| format!("{} schreiben", out.display()))?;
    eprintln!("{} Seiten → {}", pages.len(), out.display());
    Ok(())
}

/// Foto mit eingezeichnetem Viereck (Kontrolle der Erkennung).
fn outline(photo: &image::RgbImage, quad: &kd_core::Quad) -> image::RgbImage {
    let mut img = photo.clone();
    let (w, h) = (img.width() as i64, img.height() as i64);
    let thick = (w.max(h) / 300).max(2);
    for i in 0..4 {
        let (a, b) = (quad.0[i], quad.0[(i + 1) % 4]);
        let steps = (kd_core::geom::dist(a, b) as i64).max(1);
        for s in 0..=steps {
            let t = s as f32 / steps as f32;
            let (x, y) = ((a[0] + (b[0] - a[0]) * t) as i64, (a[1] + (b[1] - a[1]) * t) as i64);
            for dy in -thick..=thick {
                for dx in -thick..=thick {
                    let (px, py) = (x + dx, y + dy);
                    if px >= 0 && py >= 0 && px < w && py < h {
                        img.put_pixel(px as u32, py as u32, image::Rgb([255, 40, 160]));
                    }
                }
            }
        }
    }
    img
}
