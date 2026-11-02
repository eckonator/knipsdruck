// SPDX-License-Identifier: MIT OR Apache-2.0
//! Geometrie: Projektion vom Einheitsquadrat auf das Blatt-Viereck und Entzerren per inverser Abbildung.

use image::RgbImage;
use rayon::prelude::*;

use crate::{Point, Quad};

pub fn dist(a: Point, b: Point) -> f32 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt()
}

/// Bild mit drei Kanälen als Gleitkommazahlen 0..1, zeilenweise RGBRGB…
#[derive(Clone, Debug)]
pub struct Rgbf {
    pub width: usize,
    pub height: usize,
    pub data: Vec<f32>,
}

impl Rgbf {
    pub fn filled(width: usize, height: usize, value: f32) -> Rgbf {
        Rgbf {
            width,
            height,
            data: vec![value; width * height * 3],
        }
    }

    /// Ein Viertel-, halbe oder Dreivierteldrehung im Uhrzeigersinn.
    pub fn rotated(&self, degrees: u16) -> Rgbf {
        let (w, h) = (self.width, self.height);
        let quarter = !degrees.is_multiple_of(180);
        let (nw, nh) = if quarter { (h, w) } else { (w, h) };
        let mut out = Rgbf::filled(nw, nh, 1.0);
        out.data.par_chunks_mut(nw * 3).enumerate().for_each(|(y, row)| {
            for x in 0..nw {
                let (sx, sy) = match degrees % 360 {
                    90 => (y, h - 1 - x),
                    180 => (w - 1 - x, h - 1 - y),
                    270 => (w - 1 - y, x),
                    _ => (x, y),
                };
                let s = (sy * w + sx) * 3;
                row[x * 3..x * 3 + 3].copy_from_slice(&self.data[s..s + 3]);
            }
        });
        out
    }

    /// Kopiert `src` an die Stelle (x, y) dieses Bilds.
    pub fn paste(&mut self, src: &Rgbf, x: usize, y: usize) {
        for row in 0..src.height.min(self.height.saturating_sub(y)) {
            let n = src.width.min(self.width.saturating_sub(x)) * 3;
            let d = ((y + row) * self.width + x) * 3;
            let s = row * src.width * 3;
            self.data[d..d + n].copy_from_slice(&src.data[s..s + n]);
        }
    }
}

/// Projektive Abbildung vom Einheitsquadrat (u, v ∈ 0..1) auf ein Viereck (Heckbert).
#[derive(Clone, Copy, Debug)]
pub struct Homography {
    a: f64,
    b: f64,
    c: f64,
    d: f64,
    e: f64,
    f: f64,
    g: f64,
    h: f64,
}

impl Homography {
    pub fn square_to_quad(q: &Quad) -> Homography {
        let [[x0, y0], [x1, y1], [x2, y2], [x3, y3]] = q.0.map(|p| p.map(f64::from));
        let (dx1, dx2, dx3) = (x1 - x2, x3 - x2, x0 - x1 + x2 - x3);
        let (dy1, dy2, dy3) = (y1 - y2, y3 - y2, y0 - y1 + y2 - y3);
        let (mut g, mut h) = (0.0, 0.0);
        if dx3.abs() > 1e-9 || dy3.abs() > 1e-9 {
            let det = dx1 * dy2 - dx2 * dy1;
            if det.abs() > 1e-12 {
                g = (dx3 * dy2 - dx2 * dy3) / det;
                h = (dx1 * dy3 - dx3 * dy1) / det;
            }
        }
        Homography {
            a: x1 - x0 + g * x1,
            b: x3 - x0 + h * x3,
            c: x0,
            d: y1 - y0 + g * y1,
            e: y3 - y0 + h * y3,
            f: y0,
            g,
            h,
        }
    }

    pub fn map(&self, u: f64, v: f64) -> (f64, f64) {
        let z = self.g * u + self.h * v + 1.0;
        (
            (self.a * u + self.b * v + self.c) / z,
            (self.d * u + self.e * v + self.f) / z,
        )
    }
}

/// Catmull-Rom-Gewichte für die vier Nachbarn bei Abstand `t` (0..1) zum zweiten.
fn cubic_weights(t: f32) -> [f32; 4] {
    let t2 = t * t;
    let t3 = t2 * t;
    [
        -0.5 * t3 + t2 - 0.5 * t,
        1.5 * t3 - 2.5 * t2 + 1.0,
        -1.5 * t3 + 2.0 * t2 + 0.5 * t,
        0.5 * t3 - 0.5 * t2,
    ]
}

/// Entzerrt das Viereck `quad` aus `src` auf ein Bild `width` × `height` (bikubisch, Werte 0..1).
/// Wird stark verkleinert, wird vorher gemittelt (sonst flimmert feine Schrift).
pub fn warp(src: &RgbImage, quad: &Quad, width: usize, height: usize) -> Rgbf {
    let (qw, qh) = quad.size();
    let shrink = (qw / width as f32).min(qh / height as f32);
    if shrink > 2.0 {
        let f = (shrink / 1.5).floor() as u32;
        let small = box_shrink(src, f);
        return warp_direct(&small, &quad.scaled(1.0 / f as f32), width, height);
    }
    warp_direct(src, quad, width, height)
}

fn warp_direct(src: &RgbImage, quad: &Quad, width: usize, height: usize) -> Rgbf {
    let hm = Homography::square_to_quad(quad);
    let (sw, sh) = (src.width() as i64, src.height() as i64);
    let raw = src.as_raw();
    let px = |x: i64, y: i64, c: usize| -> f32 {
        let x = x.clamp(0, sw - 1);
        let y = y.clamp(0, sh - 1);
        raw[((y * sw + x) * 3) as usize + c] as f32
    };
    let mut out = Rgbf::filled(width, height, 1.0);
    out.data.par_chunks_mut(width * 3).enumerate().for_each(|(y, row)| {
        let v = (y as f64 + 0.5) / height as f64;
        for x in 0..width {
            let u = (x as f64 + 0.5) / width as f64;
            let (fx, fy) = hm.map(u, v);
            let (fx, fy) = (fx as f32 - 0.5, fy as f32 - 0.5);
            let (ix, iy) = (fx.floor(), fy.floor());
            let wx = cubic_weights(fx - ix);
            let wy = cubic_weights(fy - iy);
            let (ix, iy) = (ix as i64, iy as i64);
            for c in 0..3 {
                let mut acc = 0.0;
                for (j, wyj) in wy.iter().enumerate() {
                    let yy = iy - 1 + j as i64;
                    let mut line = 0.0;
                    for (i, wxi) in wx.iter().enumerate() {
                        line += wxi * px(ix - 1 + i as i64, yy, c);
                    }
                    acc += wyj * line;
                }
                row[x * 3 + c] = (acc / 255.0).clamp(0.0, 1.0);
            }
        }
    });
    out
}

/// Verkleinert um den ganzzahligen Faktor `f` durch Mitteln von f × f Pixeln.
pub fn box_shrink(src: &RgbImage, f: u32) -> RgbImage {
    let (w, h) = ((src.width() / f).max(1), (src.height() / f).max(1));
    let mut out = RgbImage::new(w, h);
    let n = f * f;
    out.as_mut()
        .par_chunks_mut(w as usize * 3)
        .enumerate()
        .for_each(|(y, row)| {
            for x in 0..w {
                let mut acc = [0u32; 3];
                for dy in 0..f {
                    for dx in 0..f {
                        let p = src.get_pixel(x * f + dx, y as u32 * f + dy).0;
                        for c in 0..3 {
                            acc[c] += p[c] as u32;
                        }
                    }
                }
                for c in 0..3 {
                    row[x as usize * 3 + c] = (acc[c] / n) as u8;
                }
            }
        });
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn homography_maps_corners() {
        let q = Quad([[10.0, 20.0], [110.0, 15.0], [120.0, 220.0], [5.0, 200.0]]);
        let hm = Homography::square_to_quad(&q);
        for (uv, p) in [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)].iter().zip(q.0) {
            let (x, y) = hm.map(uv.0, uv.1);
            assert!((x - p[0] as f64).abs() < 1e-6 && (y - p[1] as f64).abs() < 1e-6);
        }
    }

    #[test]
    fn warp_of_whole_image_keeps_colors() {
        let mut img = RgbImage::new(40, 20);
        for (x, _, p) in img.enumerate_pixels_mut() {
            *p = if x < 20 {
                image::Rgb([255, 0, 0])
            } else {
                image::Rgb([0, 0, 255])
            };
        }
        let out = warp(&img, &Quad::whole(40, 20), 40, 20);
        let at = |x: usize, y: usize| &out.data[(y * 40 + x) * 3..(y * 40 + x) * 3 + 3];
        assert!(at(5, 10)[0] > 0.95 && at(5, 10)[2] < 0.05);
        assert!(at(35, 10)[2] > 0.95 && at(35, 10)[0] < 0.05);
    }

    #[test]
    fn rotation_moves_pixels_clockwise() {
        let mut img = Rgbf::filled(3, 2, 0.0);
        img.data[0] = 1.0; // oben links rot
        let r = img.rotated(90);
        assert_eq!((r.width, r.height), (2, 3));
        // nach einer Vierteldrehung im Uhrzeigersinn liegt oben links oben rechts
        assert_eq!(r.data[3], 1.0);
        assert_eq!(img.rotated(360).data, img.data);
    }
}
