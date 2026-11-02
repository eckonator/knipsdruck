// SPDX-License-Identifier: MIT OR Apache-2.0
//! Aufbereitung des entzerrten Blatts: Licht ausgleichen, Rand säubern, schärfen, Farbmodus anwenden.

use image::RgbImage;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::geom::Rgbf;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ColorMode {
    #[default]
    Color,
    /// Wie eine Kopie: Graustufen mit kräftiger Schrift
    Gray,
    /// Nur Schwarz und Weiß (spart Toner)
    BlackWhite,
}

/// Licht ausgleichen: Hintergrund schätzen (verkleinern → Maximumfilter entfernt die Schrift → weichzeichnen)
/// und das Bild durch ihn teilen. Danach ist Papier überall weiß, auch bei Schatten und Farbstich.
pub fn normalize(img: &Rgbf) -> Rgbf {
    let (w, h) = (img.width, img.height);
    // Blockgröße mit der Auflösung mitwachsen lassen: ~2 mm auf A4, egal wie groß das Bild ist
    let k = (w.max(h) / 137).max(2);
    let (bw, bh) = (w.div_ceil(k), h.div_ceil(k));
    let mut small = vec![0.0f32; bw * bh * 3];
    let mut count = vec![0.0f32; bw * bh];
    for y in 0..h {
        let by = y / k;
        for x in 0..w {
            let b = by * bw + x / k;
            let i = (y * w + x) * 3;
            for c in 0..3 {
                small[b * 3 + c] += img.data[i + c];
            }
            count[b] += 1.0;
        }
    }
    for (b, n) in count.iter().enumerate() {
        for c in 0..3 {
            small[b * 3 + c] /= n;
        }
    }
    let bg = blur(&max_filter(&small, bw, bh, 3), bw, bh, 2, 3);
    let mut out = img.clone();
    out.data.par_chunks_mut(w * 3).enumerate().for_each(|(y, row)| {
        let gy = ((y as f32 + 0.5) / k as f32 - 0.5).clamp(0.0, bh as f32 - 1.001);
        let (iy, fy) = (gy as usize, gy.fract());
        let iy1 = (iy + 1).min(bh - 1);
        for x in 0..w {
            let gx = ((x as f32 + 0.5) / k as f32 - 0.5).clamp(0.0, bw as f32 - 1.001);
            let (ix, fx) = (gx as usize, gx.fract());
            let ix1 = (ix + 1).min(bw - 1);
            for c in 0..3 {
                let p = |yy: usize, xx: usize| bg[(yy * bw + xx) * 3 + c];
                let top = p(iy, ix) * (1.0 - fx) + p(iy, ix1) * fx;
                let bot = p(iy1, ix) * (1.0 - fx) + p(iy1, ix1) * fx;
                let b = (top * (1.0 - fy) + bot * fy).max(1.0 / 255.0);
                let v = &mut row[x * 3 + c];
                *v = (*v / b).min(1.0);
            }
        }
    });
    out
}

/// Maximumfilter mit Radius `r` (getrennt nach Zeilen und Spalten), je Kanal.
fn max_filter(src: &[f32], w: usize, h: usize, r: usize) -> Vec<f32> {
    let mut tmp = vec![0.0f32; src.len()];
    for y in 0..h {
        for x in 0..w {
            for c in 0..3 {
                let mut m = 0.0f32;
                for xx in x.saturating_sub(r)..=(x + r).min(w - 1) {
                    m = m.max(src[(y * w + xx) * 3 + c]);
                }
                tmp[(y * w + x) * 3 + c] = m;
            }
        }
    }
    let mut out = vec![0.0f32; src.len()];
    for y in 0..h {
        for x in 0..w {
            for c in 0..3 {
                let mut m = 0.0f32;
                for yy in y.saturating_sub(r)..=(y + r).min(h - 1) {
                    m = m.max(tmp[(yy * w + x) * 3 + c]);
                }
                out[(y * w + x) * 3 + c] = m;
            }
        }
    }
    out
}

/// Mehrfacher Kastenfilter (nähert einen Gauß an), Radius `r`, `passes` Durchgänge.
fn blur(src: &[f32], w: usize, h: usize, r: usize, passes: usize) -> Vec<f32> {
    let mut a = src.to_vec();
    let mut t = vec![0.0f32; src.len()];
    for _ in 0..passes {
        for y in 0..h {
            for x in 0..w {
                let (lo, hi) = (x.saturating_sub(r), (x + r).min(w - 1));
                for c in 0..3 {
                    let s: f32 = (lo..=hi).map(|xx| a[(y * w + xx) * 3 + c]).sum();
                    t[(y * w + x) * 3 + c] = s / (hi - lo + 1) as f32;
                }
            }
        }
        for y in 0..h {
            let (lo, hi) = (y.saturating_sub(r), (y + r).min(h - 1));
            for x in 0..w {
                for c in 0..3 {
                    let s: f32 = (lo..=hi).map(|yy| t[(yy * w + x) * 3 + c]).sum();
                    a[(y * w + x) * 3 + c] = s / (hi - lo + 1) as f32;
                }
            }
        }
    }
    a
}

/// Rand säubern: dunkle oder farbige Flächen, die den Blattrand berühren (Ordnerringe, Tisch, Nachbarblatt),
/// werden weiß – aber nur innerhalb eines schmalen Streifens am Rand, damit Inhalt stehen bleibt.
pub fn clean_border(img: &mut Rgbf) {
    let (w, h) = (img.width, img.height);
    let short = w.min(h) as f32;
    let band = (0.07 * short).round() as usize;
    let seed = ((0.012 * short).round() as usize).max(2);
    let in_band = |x: usize, y: usize| x < band || y < band || x + band >= w || y + band >= h;
    let bad = |d: &[f32], i: usize| {
        let (r, g, b) = (d[i * 3], d[i * 3 + 1], d[i * 3 + 2]);
        let l = (r + g + b) / 3.0;
        let chroma = r.max(g).max(b) - r.min(g).min(b);
        l < 0.75 || chroma > 0.25
    };
    // 1 = Saumstreifen (immer weiß), 2 = vom Rand her erreichte dunkle Fläche
    let mut kill = vec![0u8; w * h];
    let mut stack = Vec::new();
    for y in 0..h {
        for x in 0..w {
            if x < seed || y < seed || x + seed >= w || y + seed >= h {
                let i = y * w + x;
                kill[i] = 1;
                if bad(&img.data, i) {
                    stack.push(i);
                }
            }
        }
    }
    while let Some(i) = stack.pop() {
        let (x, y) = (i % w, i / w);
        for (dx, dy) in [
            (-1i64, 0i64),
            (1, 0),
            (0, -1),
            (0, 1),
            (-1, -1),
            (1, 1),
            (-1, 1),
            (1, -1),
        ] {
            let (xx, yy) = (x as i64 + dx, y as i64 + dy);
            if xx < 0 || yy < 0 || xx >= w as i64 || yy >= h as i64 {
                continue;
            }
            let (xx, yy) = (xx as usize, yy as usize);
            let j = yy * w + xx;
            if kill[j] == 2 || !in_band(xx, yy) {
                continue;
            }
            if bad(&img.data, j) {
                kill[j] = 2;
                stack.push(j);
            }
        }
    }
    for (i, k) in kill.iter().enumerate() {
        if *k != 0 {
            img.data[i * 3..i * 3 + 3].fill(1.0);
        }
    }
}

/// Unschärfemaske mit 3 × 3-Gauß: hebt Schriftkanten an. `amount` 0.4–1.0.
pub fn sharpen(img: &mut Rgbf, amount: f32) {
    let (w, h) = (img.width, img.height);
    let src = img.data.clone();
    let at = |x: usize, y: usize, c: usize| src[(y * w + x) * 3 + c];
    img.data.par_chunks_mut(w * 3).enumerate().for_each(|(y, row)| {
        let (y0, y1) = (y.saturating_sub(1), (y + 1).min(h - 1));
        for x in 0..w {
            let (x0, x1) = (x.saturating_sub(1), (x + 1).min(w - 1));
            for c in 0..3 {
                let blur = (at(x0, y0, c)
                    + at(x1, y0, c)
                    + at(x0, y1, c)
                    + at(x1, y1, c)
                    + 2.0 * (at(x, y0, c) + at(x, y1, c) + at(x0, y, c) + at(x1, y, c))
                    + 4.0 * at(x, y, c))
                    / 16.0;
                let v = at(x, y, c);
                row[x * 3 + c] = (v + amount * (v - blur)).clamp(0.0, 1.0);
            }
        }
    });
}

/// Tonkurve: Schwarzpunkt `bp`, Weißpunkt `wp`, Gamma `g`.
fn curve(x: f32, bp: f32, wp: f32, g: f32) -> f32 {
    let t = (x - bp) / (wp - bp);
    if t <= 0.0 {
        0.0
    } else if t >= 1.0 {
        1.0
    } else {
        t.powf(g)
    }
}

/// Farbmodus anwenden und in 8 Bit umwandeln.
pub fn colorize(img: &Rgbf, mode: ColorMode) -> RgbImage {
    let (w, h) = (img.width, img.height);
    let mut out = RgbImage::new(w as u32, h as u32);
    out.as_mut()
        .par_chunks_mut(3)
        .zip(img.data.par_chunks(3))
        .for_each(|(o, p)| {
            let (r, g, b) = (p[0], p[1], p[2]);
            let l = 0.299 * r + 0.587 * g + 0.114 * b;
            let rgb = match mode {
                ColorMode::Color => {
                    let [cr, cg, cb] = [r, g, b].map(|v| curve(v, 0.25, 0.9, 1.2));
                    let cl = 0.299 * cr + 0.587 * cg + 0.114 * cb;
                    // Farben etwas satter, damit Textmarker und Stempel wie im Original wirken
                    [cr, cg, cb].map(|v| cl + (v - cl) * 1.3)
                }
                ColorMode::Gray => [curve(l, 0.30, 0.88, 1.4); 3],
                ColorMode::BlackWhite => [if l < 0.62 { 0.0 } else { 1.0 }; 3],
            };
            for c in 0..3 {
                o[c] = (rgb[c].clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
            }
        });
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Papier mit Verlauf (links hell, rechts im Schatten) und dunkler Schrift in der Mitte.
    fn shaded_page() -> Rgbf {
        let (w, h) = (400, 300);
        let mut img = Rgbf::filled(w, h, 0.0);
        for y in 0..h {
            for x in 0..w {
                let paper = 0.9 - 0.4 * x as f32 / w as f32;
                let ink = (190..210).contains(&y) && (100..300).contains(&x) && x % 12 < 4;
                let v = if ink { paper * 0.25 } else { paper };
                img.data[(y * w + x) * 3..(y * w + x) * 3 + 3].fill(v);
            }
        }
        img
    }

    #[test]
    fn normalize_whitens_shadowed_paper_and_keeps_ink() {
        let n = normalize(&shaded_page());
        let at = |x: usize, y: usize| n.data[(y * 400 + x) * 3];
        assert!(at(20, 50) > 0.97, "links {}", at(20, 50));
        assert!(at(380, 50) > 0.97, "rechts im Schatten {}", at(380, 50));
        assert!(at(204, 200) < 0.4, "Schrift {}", at(204, 200));
    }

    #[test]
    fn clean_border_removes_dark_edge_but_not_content() {
        let mut img = Rgbf::filled(200, 200, 1.0);
        for y in 0..200 {
            for x in 0..8 {
                img.data[(y * 200 + x) * 3..(y * 200 + x) * 3 + 3].fill(0.1); // Tischkante links
            }
        }
        for x in 90..110 {
            img.data[(100 * 200 + x) * 3..(100 * 200 + x) * 3 + 3].fill(0.0); // Schrift in der Mitte
        }
        clean_border(&mut img);
        assert_eq!(img.data[(50 * 200 + 3) * 3], 1.0);
        assert_eq!(img.data[(100 * 200 + 100) * 3], 0.0);
    }

    #[test]
    fn colorize_modes() {
        let mut img = Rgbf::filled(2, 1, 1.0);
        img.data[3..6].copy_from_slice(&[0.1, 0.1, 0.1]);
        let c = colorize(&img, ColorMode::Color);
        assert_eq!(c.get_pixel(0, 0).0, [255, 255, 255]);
        assert_eq!(c.get_pixel(1, 0).0, [0, 0, 0]);
        let bw = colorize(&img, ColorMode::BlackWhite);
        assert_eq!(bw.get_pixel(1, 0).0, [0, 0, 0]);
    }
}
