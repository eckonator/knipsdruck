// SPDX-License-Identifier: MIT OR Apache-2.0
//! Entwicklerwerkzeuge.
//!
//! `cargo xtask demo-fotos` erzeugt aus den Beispieldokumenten in `xtask/demo/` (frei erfundene Inhalte)
//! „Handyfotos“: schräg auf einem Tisch, mit Schatten und ungleichmäßigem Licht. Sie dienen für Bildschirmfotos
//! in der README und zum Ausprobieren – die echten Fotos von Testpersonen gehören nie ins Repo.
//! Die Dokumente rendert Google Chrome (Pfad in `CHROME`, Standard: macOS-Programmordner).

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use image::{Rgb, RgbImage};

/// Größe der Fotos wie bei einem Handy (Hochformat).
const PHOTO: (u32, u32) = (1536, 2048);

struct Scene {
    doc: &'static str,
    /// Ecken des Blatts im Foto: oben links, oben rechts, unten rechts, unten links
    corners: [[f64; 2]; 4],
    table: Table,
    /// Licht: Mittelpunkt (relativ) und Abfall zum Rand
    light: ([f64; 2], f64),
}

#[derive(Clone, Copy)]
enum Table {
    Wood,
    Fabric,
    Stone,
}

const SCENES: &[Scene] = &[
    Scene {
        doc: "elternbrief",
        corners: [[214.0, 262.0], [1328.0, 196.0], [1418.0, 1790.0], [126.0, 1842.0]],
        table: Table::Wood,
        light: ([0.25, 0.2], 0.42),
    },
    Scene {
        doc: "arbeitsblatt",
        corners: [[168.0, 318.0], [1290.0, 250.0], [1386.0, 1868.0], [104.0, 1760.0]],
        table: Table::Fabric,
        light: ([0.7, 0.25], 0.5),
    },
    Scene {
        doc: "rezept",
        corners: [[260.0, 210.0], [1380.0, 300.0], [1300.0, 1880.0], [170.0, 1780.0]],
        table: Table::Stone,
        light: ([0.4, 0.75], 0.45),
    },
];

fn main() -> Result<()> {
    match std::env::args().nth(1).as_deref() {
        Some("demo-fotos") => demo_photos(),
        _ => {
            eprintln!("Aufruf: cargo xtask demo-fotos");
            Ok(())
        }
    }
}

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf()
}

fn demo_photos() -> Result<()> {
    let src = root().join("xtask/demo");
    let out = root().join("demo");
    std::fs::create_dir_all(&out)?;
    let chrome = std::env::var("CHROME")
        .unwrap_or_else(|_| "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome".into());
    for (i, scene) in SCENES.iter().enumerate() {
        let png = out.join(format!("{}.png", scene.doc));
        render_html(&chrome, &src.join(format!("{}.html", scene.doc)), &png)?;
        let doc = image::open(&png)
            .with_context(|| format!("{} lesen", png.display()))?
            .into_rgb8();
        let photo = compose(&doc, scene, i as u64 + 1);
        let jpg = out.join(format!("Foto_{}_{}.jpg", i + 1, scene.doc));
        let mut f = std::io::BufWriter::new(std::fs::File::create(&jpg)?);
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut f, 82).encode_image(&photo)?;
        std::fs::remove_file(&png)?;
        println!("{}", jpg.display());
    }
    Ok(())
}

fn render_html(chrome: &str, html: &Path, png: &Path) -> Result<()> {
    let status = Command::new(chrome)
        .args([
            "--headless=new",
            "--disable-gpu",
            "--hide-scrollbars",
            "--window-size=1240,1754",
        ])
        .arg("--force-device-scale-factor=1")
        .arg(format!("--screenshot={}", png.display()))
        .arg(format!("file://{}", html.display()))
        .output()
        .context("Google Chrome starten (Pfad in CHROME setzen)")?;
    if !png.exists() {
        bail!(
            "Chrome hat {} nicht gerendert: {}",
            html.display(),
            String::from_utf8_lossy(&status.stderr)
        );
    }
    Ok(())
}

/// Homographie aus vier Punktpaaren (DLT mit h33 = 1): bildet `from[i]` auf `to[i]` ab.
fn homography(from: &[[f64; 2]; 4], to: &[[f64; 2]; 4]) -> [f64; 9] {
    let mut a = [[0.0f64; 9]; 8];
    for i in 0..4 {
        let ([x, y], [u, v]) = (from[i], to[i]);
        a[2 * i] = [x, y, 1.0, 0.0, 0.0, 0.0, -u * x, -u * y, u];
        a[2 * i + 1] = [0.0, 0.0, 0.0, x, y, 1.0, -v * x, -v * y, v];
    }
    // Gauß mit Pivotsuche
    for c in 0..8 {
        let p = (c..8).max_by(|&i, &j| a[i][c].abs().total_cmp(&a[j][c].abs())).unwrap();
        a.swap(c, p);
        let pivot = a[c];
        for (r, row) in a.iter_mut().enumerate() {
            if r != c {
                let f = row[c] / pivot[c];
                for (x, p) in row[c..].iter_mut().zip(&pivot[c..]) {
                    *x -= f * p;
                }
            }
        }
    }
    let mut h = [1.0; 9];
    for (i, row) in a.iter().enumerate() {
        h[i] = row[8] / row[i];
    }
    h
}

fn apply(h: &[f64; 9], x: f64, y: f64) -> (f64, f64) {
    let w = h[6] * x + h[7] * y + h[8];
    ((h[0] * x + h[1] * y + h[2]) / w, (h[3] * x + h[4] * y + h[5]) / w)
}

/// Gleichmäßiges Rauschen 0..1 aus einer Ganzzahl (für Holzmaserung und Bildrauschen).
fn hash(mut n: u64) -> f64 {
    n = (n ^ (n >> 33)).wrapping_mul(0xff51_afd7_ed55_8ccd);
    n = (n ^ (n >> 33)).wrapping_mul(0xc4ce_b9fe_1a85_ec53);
    ((n ^ (n >> 33)) % 10_000) as f64 / 10_000.0
}

/// Weiches Rauschen (Wertrauschen, bilinear) im Raster `cell`.
fn smooth_noise(x: f64, y: f64, cell: f64, seed: u64) -> f64 {
    let (gx, gy) = (x / cell, y / cell);
    let (ix, iy) = (gx.floor() as i64, gy.floor() as i64);
    let (fx, fy) = (gx - ix as f64, gy - iy as f64);
    let at = |i: i64, j: i64| hash(seed ^ ((i as u64).wrapping_mul(73_856_093) ^ (j as u64).wrapping_mul(19_349_663)));
    let s = |t: f64| t * t * (3.0 - 2.0 * t);
    let top = at(ix, iy) + (at(ix + 1, iy) - at(ix, iy)) * s(fx);
    let bot = at(ix, iy + 1) + (at(ix + 1, iy + 1) - at(ix, iy + 1)) * s(fx);
    top + (bot - top) * s(fy)
}

fn table(kind: Table, x: f64, y: f64) -> [f64; 3] {
    match kind {
        Table::Wood => {
            let grain = (x * 0.018 + 6.0 * smooth_noise(x, y, 140.0, 7)).sin() * 0.5 + 0.5;
            let fine = smooth_noise(x, y * 0.15, 6.0, 3);
            let v = 0.75 + 0.18 * grain + 0.07 * fine;
            [128.0 * v, 86.0 * v, 54.0 * v]
        }
        Table::Fabric => {
            let weave = ((x * 0.9).sin() * (y * 0.9).sin()) * 0.5 + 0.5;
            let stripe = if ((x + y) / 90.0).floor() as i64 % 2 == 0 {
                1.0
            } else {
                0.86
            };
            let v = (0.82 + 0.1 * weave) * stripe;
            [112.0 * v, 104.0 * v, 98.0 * v]
        }
        Table::Stone => {
            let n = 0.6 * smooth_noise(x, y, 60.0, 11) + 0.4 * smooth_noise(x, y, 9.0, 5);
            let v = 0.72 + 0.25 * n;
            [96.0 * v, 98.0 * v, 104.0 * v]
        }
    }
}

/// Abstand eines Punkts zum Viereck (0 innen), für den weichen Schatten.
fn outside_distance(q: &[[f64; 2]; 4], x: f64, y: f64) -> f64 {
    let mut d = 0.0f64;
    for i in 0..4 {
        let (a, b) = (q[i], q[(i + 1) % 4]);
        let (ex, ey) = (b[0] - a[0], b[1] - a[1]);
        let len = (ex * ex + ey * ey).sqrt();
        // Abstand zur Kante, positiv außen (Ecken im Uhrzeigersinn, y nach unten)
        let side = ((x - a[0]) * ey - (y - a[1]) * ex) / len;
        d = d.max(-side);
    }
    d.max(0.0)
}

fn compose(doc: &RgbImage, s: &Scene, seed: u64) -> RgbImage {
    let (w, h) = (PHOTO.0 as f64, PHOTO.1 as f64);
    let (dw, dh) = (doc.width() as f64, doc.height() as f64);
    let to_doc = homography(&s.corners, &[[0.0, 0.0], [dw, 0.0], [dw, dh], [0.0, dh]]);
    // Schatten leicht nach rechts unten versetzt
    let shadow_q = s.corners.map(|[x, y]| [x + 18.0, y + 26.0]);
    RgbImage::from_fn(PHOTO.0, PHOTO.1, |px, py| {
        let (x, y) = (px as f64 + 0.5, py as f64 + 0.5);
        let (u, v) = apply(&to_doc, x, y);
        let mut c = if u >= 0.0 && v >= 0.0 && u < dw - 1.0 && v < dh - 1.0 {
            bilinear(doc, u, v)
        } else {
            let mut t = table(s.table, x, y);
            let shade = 1.0 - 0.45 * (-outside_distance(&shadow_q, x, y) / 30.0).exp();
            t.iter_mut().for_each(|c| *c *= shade);
            t
        };
        // Licht: hell um einen Punkt, zum Rand dunkler; Papier unter Lampenlicht leicht warm
        let (lx, ly) = (s.light.0[0] * w, s.light.0[1] * h);
        let r = ((x - lx).powi(2) + (y - ly).powi(2)).sqrt() / w.hypot(h);
        let light = (1.0 - s.light.1 * r * r * 2.2).clamp(0.45, 1.0) * 0.94;
        let tint = [1.0, 0.97, 0.9];
        let noise = (hash(seed * 1_000_003 + (py as u64) * 4096 + px as u64) - 0.5) * 7.0;
        for k in 0..3 {
            c[k] = c[k] * light * tint[k] + noise;
        }
        Rgb(c.map(|v| v.clamp(0.0, 255.0) as u8))
    })
}

fn bilinear(img: &RgbImage, x: f64, y: f64) -> [f64; 3] {
    let (ix, iy) = (x as u32, y as u32);
    let (fx, fy) = (x - ix as f64, y - iy as f64);
    let p = |dx: u32, dy: u32| img.get_pixel(ix + dx, iy + dy).0.map(f64::from);
    let (a, b, c, d) = (p(0, 0), p(1, 0), p(0, 1), p(1, 1));
    [0, 1, 2].map(|k| {
        let top = a[k] + (b[k] - a[k]) * fx;
        let bot = c[k] + (d[k] - c[k]) * fx;
        top + (bot - top) * fy
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn homography_maps_the_four_points() {
        let from = [[10.0, 20.0], [300.0, 5.0], [320.0, 400.0], [0.0, 380.0]];
        let to = [[0.0, 0.0], [100.0, 0.0], [100.0, 141.0], [0.0, 141.0]];
        let h = homography(&from, &to);
        for (f, t) in from.iter().zip(to) {
            let (u, v) = apply(&h, f[0], f[1]);
            assert!((u - t[0]).abs() < 1e-6 && (v - t[1]).abs() < 1e-6);
        }
    }
}
