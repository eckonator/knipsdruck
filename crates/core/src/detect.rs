// SPDX-License-Identifier: MIT OR Apache-2.0
//! Blatt im Foto finden.
//!
//! Papier ist hell und kaum farbig. Auf einer verkleinerten Kopie wird daraus eine Maske (Otsu-Schwelle),
//! Schrift und Löcher werden geschlossen, die größte, möglichst mittige Fläche gewählt. Aus ihrer konvexen Hülle
//! kommt das größte einbeschriebene Viereck; dessen Seiten werden als Geraden an den Rand der Fläche angepasst,
//! damit abgerundete oder verdeckte Ecken (Daumen) nicht stören. Die Sicherheit misst, wie gut Fläche und
//! Viereck zusammenpassen, wie klar sich das Blatt abhebt und wie rechtwinklig es aussieht.

use image::{GrayImage, RgbImage};

use crate::{Point, Quad, Source};

/// Lange Seite der Arbeitskopie.
const WORK: u32 = 640;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Detection {
    pub quad: Quad,
    /// 0–100; unter 60 zeigt die App „Bitte prüfen“
    pub confidence: u8,
    pub source: Source,
}

/// Zwischenergebnisse für `knipsdruck-cli --debug-dir`.
pub struct Debug {
    pub mask: GrayImage,
    pub scale: f32,
}

pub fn detect(photo: &RgbImage) -> Detection {
    detect_with_debug(photo).0
}

pub fn detect_with_debug(photo: &RgbImage) -> (Detection, Debug) {
    let (w, h) = photo.dimensions();
    let scale = (WORK as f32 / w.max(h) as f32).min(1.0);
    let (sw, sh) = (
        ((w as f32 * scale).round() as u32).max(8),
        ((h as f32 * scale).round() as u32).max(8),
    );
    let small = image::imageops::resize(photo, sw, sh, image::imageops::FilterType::Triangle);
    let (sw, sh) = (sw as usize, sh as usize);
    let score = box_blur(&paper_score(&small), sw, sh, 2);

    let mut best = candidate(&score, sw, sh, None, otsu(&score).0);
    // Liegt das Blatt auf etwas ebenfalls Hellem (grauer Ordnerdeckel, Blattstapel), gehört das mit zur Fläche.
    // Zerfällt die Fläche klar in hell und dunkler, die hellere innere Fläche nehmen, wenn sie nicht schlechter ist.
    for _ in 0..2 {
        let Some(outer) = &best else { break };
        let inside: Vec<f32> = score
            .iter()
            .zip(&outer.comp)
            .filter(|(_, c)| **c)
            .map(|(s, _)| *s)
            .collect();
        let (t, low, high, low_share) = otsu(&inside);
        if high - low < 0.12 || !(0.06..0.7).contains(&low_share) {
            break;
        }
        match candidate(&score, sw, sh, Some(&outer.comp), t) {
            Some(inner)
                if inner.area >= sw * sh / 7
                    && quad_area(&inner.corners) < 0.92 * quad_area(&outer.corners)
                    && inner.confidence > outer.confidence =>
            {
                best = Some(inner);
            }
            _ => break,
        }
    }

    let debug_mask = GrayImage::from_fn(sw as u32, sh as u32, |x, y| {
        let i = y as usize * sw + x as usize;
        image::Luma([if best.as_ref().is_some_and(|c| c.comp[i]) {
            255
        } else {
            (score[i] * 120.0) as u8
        }])
    });
    let debug = Debug {
        mask: debug_mask,
        scale,
    };
    let Some(best) = best else {
        let whole = Detection {
            quad: Quad::whole(w, h),
            confidence: 0,
            source: Source::WholePhoto,
        };
        return (whole, debug);
    };
    let quad = Quad(
        best.corners
            .map(|[x, y]| [(x / scale).clamp(0.0, w as f32), (y / scale).clamp(0.0, h as f32)]),
    );
    (
        Detection {
            quad,
            confidence: best.confidence,
            source: Source::Auto,
        },
        debug,
    )
}

/// Ein möglicher Fund: Fläche, Viereck darin und dessen Bewertung.
struct Candidate {
    corners: [Point; 4],
    comp: Vec<bool>,
    area: usize,
    confidence: u8,
}

/// Papierfläche über der Schwelle `t` (nur innerhalb von `within`, wenn angegeben) und ihr Viereck.
fn candidate(score: &[f32], w: usize, h: usize, within: Option<&[bool]>, t: f32) -> Option<Candidate> {
    let mut mask: Vec<bool> = score
        .iter()
        .enumerate()
        .map(|(i, &s)| s > t && within.is_none_or(|m| m[i]))
        .collect();
    mask = erode(&dilate(&mask, w, h, 2), w, h, 2);
    fill_holes(&mut mask, w, h);
    let comp = best_component(&mask, w, h)?;
    let boundary = boundary_points(&comp, w, h);
    let hull = convex_hull(&boundary);
    if hull.len() < 4 {
        return None;
    }
    let corners = order_corners(refine_with_lines(max_area_quad(&hull), &boundary, w, h));
    let confidence = rate(&corners, &comp, score, w, h);
    let area = comp.iter().filter(|c| **c).count();
    Some(Candidate {
        corners,
        comp,
        area,
        confidence,
    })
}

/// Wie sehr ein Pixel nach Papier aussieht: hell und wenig gesättigt.
fn paper_score(img: &RgbImage) -> Vec<f32> {
    img.pixels()
        .map(|p| {
            let [r, g, b] = p.0.map(|v| v as f32 / 255.0);
            let max = r.max(g).max(b);
            let min = r.min(g).min(b);
            let sat = if max > 0.0 { (max - min) / max } else { 0.0 };
            let luma = 0.299 * r + 0.587 * g + 0.114 * b;
            luma * (1.0 - 0.9 * sat)
        })
        .collect()
}

fn box_blur(src: &[f32], w: usize, h: usize, r: usize) -> Vec<f32> {
    let mut tmp = vec![0.0; src.len()];
    for y in 0..h {
        for x in 0..w {
            let (lo, hi) = (x.saturating_sub(r), (x + r).min(w - 1));
            tmp[y * w + x] = (lo..=hi).map(|xx| src[y * w + xx]).sum::<f32>() / (hi - lo + 1) as f32;
        }
    }
    let mut out = vec![0.0; src.len()];
    for y in 0..h {
        let (lo, hi) = (y.saturating_sub(r), (y + r).min(h - 1));
        for x in 0..w {
            out[y * w + x] = (lo..=hi).map(|yy| tmp[yy * w + x]).sum::<f32>() / (hi - lo + 1) as f32;
        }
    }
    out
}

/// Schwelle nach Otsu (Werte 0..1, 256 Klassen), dazu die Mittelwerte beider Klassen und der Anteil der dunkleren.
fn otsu(v: &[f32]) -> (f32, f32, f32, f32) {
    let mut hist = [0usize; 256];
    for &s in v {
        hist[((s.clamp(0.0, 1.0)) * 255.0) as usize] += 1;
    }
    let total = v.len() as f64;
    let sum: f64 = hist.iter().enumerate().map(|(i, &n)| i as f64 * n as f64).sum();
    let (mut sum_b, mut w_b, mut best, mut t) = (0.0, 0.0, 0.0, 128usize);
    let (mut low, mut high, mut share) = (0.0, 0.0, 0.0);
    for (i, &n) in hist.iter().enumerate() {
        w_b += n as f64;
        if w_b == 0.0 {
            continue;
        }
        let w_f = total - w_b;
        if w_f == 0.0 {
            break;
        }
        sum_b += i as f64 * n as f64;
        let (m_b, m_f) = (sum_b / w_b, (sum - sum_b) / w_f);
        let between = w_b * w_f * (m_b - m_f).powi(2);
        if between > best {
            best = between;
            t = i;
            (low, high, share) = (m_b / 255.0, m_f / 255.0, w_b / total);
        }
    }
    (t as f32 / 255.0, low as f32, high as f32, share as f32)
}

fn dilate(m: &[bool], w: usize, h: usize, r: usize) -> Vec<bool> {
    morph(m, w, h, r, true)
}

fn erode(m: &[bool], w: usize, h: usize, r: usize) -> Vec<bool> {
    morph(m, w, h, r, false)
}

/// Quadratisches Strukturelement, getrennt nach Zeilen und Spalten.
fn morph(m: &[bool], w: usize, h: usize, r: usize, grow: bool) -> Vec<bool> {
    // Wachsen: irgendein Nachbar gesetzt; Schrumpfen: alle Nachbarn gesetzt
    let pick = |mut it: std::iter::Map<std::ops::RangeInclusive<usize>, &dyn Fn(usize) -> bool>| {
        if grow { it.any(|b| b) } else { it.all(|b| b) }
    };
    let mut tmp = vec![false; m.len()];
    for y in 0..h {
        for x in 0..w {
            let (lo, hi) = (x.saturating_sub(r), (x + r).min(w - 1));
            let at = |xx: usize| m[y * w + xx];
            tmp[y * w + x] = pick((lo..=hi).map(&at as &dyn Fn(usize) -> bool));
        }
    }
    let mut out = vec![false; m.len()];
    for y in 0..h {
        let (lo, hi) = (y.saturating_sub(r), (y + r).min(h - 1));
        for x in 0..w {
            let at = |yy: usize| tmp[yy * w + x];
            out[y * w + x] = pick((lo..=hi).map(&at as &dyn Fn(usize) -> bool));
        }
    }
    out
}

/// Löcher (Schrift, Bilder auf dem Blatt) füllen: alles Nicht-Papier, das den Bildrand nicht erreicht.
fn fill_holes(m: &mut [bool], w: usize, h: usize) {
    let mut outside = vec![false; m.len()];
    let mut stack: Vec<usize> = (0..w).flat_map(|x| [x, (h - 1) * w + x]).collect();
    stack.extend((0..h).flat_map(|y| [y * w, y * w + w - 1]));
    while let Some(i) = stack.pop() {
        if m[i] || outside[i] {
            continue;
        }
        outside[i] = true;
        let (x, y) = (i % w, i / w);
        if x > 0 {
            stack.push(i - 1);
        }
        if x + 1 < w {
            stack.push(i + 1);
        }
        if y > 0 {
            stack.push(i - w);
        }
        if y + 1 < h {
            stack.push(i + w);
        }
    }
    for (v, o) in m.iter_mut().zip(&outside) {
        if !*o {
            *v = true;
        }
    }
}

/// Die Papierfläche: groß und nahe der Bildmitte. Mindestens 4 % des Bilds.
fn best_component(m: &[bool], w: usize, h: usize) -> Option<Vec<bool>> {
    let mut label = vec![0u32; m.len()];
    let (cx, cy, diag) = (w as f32 / 2.0, h as f32 / 2.0, ((w * w + h * h) as f32).sqrt());
    let mut best: Option<(f32, u32)> = None;
    let mut next = 0u32;
    let mut stack = Vec::new();
    for start in 0..m.len() {
        if !m[start] || label[start] != 0 {
            continue;
        }
        next += 1;
        let (mut area, mut sx, mut sy) = (0usize, 0.0f64, 0.0f64);
        stack.push(start);
        label[start] = next;
        while let Some(i) = stack.pop() {
            area += 1;
            let (x, y) = (i % w, i / w);
            sx += x as f64;
            sy += y as f64;
            let mut visit = |j: usize| {
                if m[j] && label[j] == 0 {
                    label[j] = next;
                    stack.push(j);
                }
            };
            if x > 0 {
                visit(i - 1);
            }
            if x + 1 < w {
                visit(i + 1);
            }
            if y > 0 {
                visit(i - w);
            }
            if y + 1 < h {
                visit(i + w);
            }
        }
        if (area as f32) < 0.04 * m.len() as f32 {
            continue;
        }
        let (mx, my) = ((sx / area as f64) as f32, (sy / area as f64) as f32);
        let off = ((mx - cx).powi(2) + (my - cy).powi(2)).sqrt() / diag;
        let value = area as f32 * (1.0 - off);
        if best.is_none_or(|(v, _)| value > v) {
            best = Some((value, next));
        }
    }
    let (_, id) = best?;
    Some(label.iter().map(|&l| l == id).collect())
}

/// Randpixel der Fläche (Mitte des Pixels); am Bildrand liegende Pixel zählen als Rand.
fn boundary_points(c: &[bool], w: usize, h: usize) -> Vec<Point> {
    let mut pts = Vec::new();
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            if !c[i] {
                continue;
            }
            let edge = x == 0 || y == 0 || x + 1 == w || y + 1 == h || !c[i - 1] || !c[i + 1] || !c[i - w] || !c[i + w];
            if edge {
                pts.push([x as f32 + 0.5, y as f32 + 0.5]);
            }
        }
    }
    pts
}

fn cross(o: Point, a: Point, b: Point) -> f32 {
    (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0])
}

/// Konvexe Hülle (monotone Kette), Punkte gegen den Uhrzeigersinn im mathematischen Sinn.
fn convex_hull(points: &[Point]) -> Vec<Point> {
    let mut p = points.to_vec();
    p.sort_by(|a, b| a[0].total_cmp(&b[0]).then(a[1].total_cmp(&b[1])));
    p.dedup();
    if p.len() < 3 {
        return p;
    }
    let mut hull: Vec<Point> = Vec::with_capacity(2 * p.len());
    for &pt in p.iter().chain(p.iter().rev().skip(1)) {
        while hull.len() >= 2 && cross(hull[hull.len() - 2], hull[hull.len() - 1], pt) <= 0.0 {
            hull.pop();
        }
        hull.push(pt);
    }
    hull.pop();
    hull
}

fn quad_area(q: &[Point; 4]) -> f32 {
    let mut a = 0.0;
    for i in 0..4 {
        let (p, n) = (q[i], q[(i + 1) % 4]);
        a += p[0] * n[1] - n[0] * p[1];
    }
    a.abs() / 2.0
}

/// Größtes Viereck mit Ecken auf der Hülle: Start an den vier diagonalen Extrempunkten, dann jede Ecke
/// zwischen ihren Nachbarn verschieben, solange die Fläche wächst.
fn max_area_quad(hull: &[Point]) -> [Point; 4] {
    let n = hull.len();
    let argmax = |f: &dyn Fn(Point) -> f32| (0..n).max_by(|&a, &b| f(hull[a]).total_cmp(&f(hull[b]))).unwrap_or(0);
    let mut idx = [
        argmax(&|p| -p[0] - p[1]),
        argmax(&|p| p[0] - p[1]),
        argmax(&|p| p[0] + p[1]),
        argmax(&|p| -p[0] + p[1]),
    ];
    // in Reihenfolge der Hülle bringen, sonst überkreuzt sich das Viereck
    idx.sort_unstable();
    let corners = |idx: &[usize; 4]| idx.map(|i| hull[i]);
    for _ in 0..12 {
        let mut improved = false;
        for k in 0..4 {
            let (prev, next) = (idx[(k + 3) % 4], idx[(k + 1) % 4]);
            let mut best = (quad_area(&corners(&idx)), idx[k]);
            let mut j = (prev + 1) % n;
            while j != next {
                let mut trial = idx;
                trial[k] = j;
                let a = quad_area(&corners(&trial));
                if a > best.0 + 1e-3 {
                    best = (a, j);
                }
                j = (j + 1) % n;
            }
            if best.1 != idx[k] {
                idx[k] = best.1;
                improved = true;
            }
        }
        if !improved {
            break;
        }
    }
    corners(&idx)
}

/// Gerade durch Punkte (Hauptachse), als (Punkt, Richtung).
fn fit_line(pts: &[Point]) -> Option<(Point, Point)> {
    if pts.len() < 6 {
        return None;
    }
    let n = pts.len() as f32;
    let (mx, my) = (
        pts.iter().map(|p| p[0]).sum::<f32>() / n,
        pts.iter().map(|p| p[1]).sum::<f32>() / n,
    );
    let (mut sxx, mut sxy, mut syy) = (0.0, 0.0, 0.0);
    for p in pts {
        let (dx, dy) = (p[0] - mx, p[1] - my);
        sxx += dx * dx;
        sxy += dx * dy;
        syy += dy * dy;
    }
    let angle = 0.5 * (2.0 * sxy).atan2(sxx - syy);
    Some(([mx, my], [angle.cos(), angle.sin()]))
}

fn intersect(a: (Point, Point), b: (Point, Point)) -> Option<Point> {
    let (p, r) = a;
    let (q, s) = b;
    let denom = r[0] * s[1] - r[1] * s[0];
    if denom.abs() < 1e-4 {
        return None;
    }
    let t = ((q[0] - p[0]) * s[1] - (q[1] - p[1]) * s[0]) / denom;
    Some([p[0] + t * r[0], p[1] + t * r[1]])
}

/// Jede Seite als Gerade an die Randpunkte in ihrer Nähe anpassen (mittlere 80 % der Seite), Ecken = Schnittpunkte.
fn refine_with_lines(q: [Point; 4], boundary: &[Point], w: usize, h: usize) -> [Point; 4] {
    let diag = ((w * w + h * h) as f32).sqrt();
    let near = 0.025 * diag;
    let lines: Vec<Option<(Point, Point)>> = (0..4)
        .map(|i| {
            let (a, b) = (q[i], q[(i + 1) % 4]);
            let len = crate::geom::dist(a, b);
            if len < 1.0 {
                return None;
            }
            let d = [(b[0] - a[0]) / len, (b[1] - a[1]) / len];
            let pts: Vec<Point> = boundary
                .iter()
                .copied()
                .filter(|p| {
                    let (vx, vy) = (p[0] - a[0], p[1] - a[1]);
                    let t = vx * d[0] + vy * d[1];
                    let off = (vx * d[1] - vy * d[0]).abs();
                    off < near && t > 0.1 * len && t < 0.9 * len
                })
                .collect();
            fit_line(&pts)
        })
        .collect();
    let mut out = q;
    for i in 0..4 {
        let (Some(before), Some(after)) = (lines[(i + 3) % 4], lines[i]) else {
            continue;
        };
        if let Some(p) = intersect(before, after)
            && crate::geom::dist(p, q[i]) < 0.04 * diag
        {
            out[i] = p;
        }
    }
    out
}

/// Reihenfolge oben links, oben rechts, unten rechts, unten links (Bildkoordinaten, y nach unten).
fn order_corners(q: [Point; 4]) -> [Point; 4] {
    let (cx, cy) = (
        q.iter().map(|p| p[0]).sum::<f32>() / 4.0,
        q.iter().map(|p| p[1]).sum::<f32>() / 4.0,
    );
    let mut v = q.to_vec();
    // im Uhrzeigersinn auf dem Bildschirm = aufsteigender Winkel bei y nach unten
    v.sort_by(|a, b| (a[1] - cy).atan2(a[0] - cx).total_cmp(&(b[1] - cy).atan2(b[0] - cx)));
    let start = (0..4)
        .min_by(|&a, &b| (v[a][0] + v[a][1]).total_cmp(&(v[b][0] + v[b][1])))
        .unwrap_or(0);
    [0, 1, 2, 3].map(|k| v[(start + k) % 4])
}

fn inside(q: &[Point; 4], p: Point) -> bool {
    (0..4).all(|i| cross(q[i], q[(i + 1) % 4], p) >= 0.0)
}

/// Sicherheit 0–100 aus Deckung, Kontrast und Winkeln.
fn rate(q: &[Point; 4], comp: &[bool], score: &[f32], w: usize, h: usize) -> u8 {
    let area = quad_area(q);
    let image_area = (w * h) as f32;
    if area < 0.04 * image_area {
        return 0;
    }
    let (mut in_comp, mut in_n, mut s_in, mut s_out, mut out_n) = (0usize, 0usize, 0.0f32, 0.0f32, 0usize);
    for y in (0..h).step_by(2) {
        for x in (0..w).step_by(2) {
            let i = y * w + x;
            if inside(q, [x as f32 + 0.5, y as f32 + 0.5]) {
                in_n += 1;
                in_comp += comp[i] as usize;
                s_in += score[i];
            } else {
                out_n += 1;
                s_out += score[i];
            }
        }
    }
    let coverage = in_comp as f32 / in_n.max(1) as f32;
    let cov = ((coverage - 0.80) / 0.17).clamp(0.0, 1.0);
    // füllt das Blatt (fast) das ganze Foto, gibt es keinen Hintergrund zum Vergleichen: wie ein Scan behandeln
    let contrast = if out_n < in_n / 20 {
        0.7
    } else {
        let d = s_in / in_n.max(1) as f32 - s_out / out_n.max(1) as f32;
        ((d - 0.08) / 0.22).clamp(0.0, 1.0)
    };
    let mut worst = 0.0f32;
    for i in 0..4 {
        let (p, c, n) = (q[(i + 3) % 4], q[i], q[(i + 1) % 4]);
        let (a, b) = ([p[0] - c[0], p[1] - c[1]], [n[0] - c[0], n[1] - c[1]]);
        let cos = (a[0] * b[0] + a[1] * b[1]) / ((a[0].hypot(a[1])) * (b[0].hypot(b[1]))).max(1e-6);
        worst = worst.max((cos.clamp(-1.0, 1.0).acos().to_degrees() - 90.0).abs());
    }
    let angle = (1.0 - (worst - 20.0) / 30.0).clamp(0.0, 1.0);
    (100.0 * (0.4 * cov + 0.3 * contrast + 0.3 * angle)).round() as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helles, schräg liegendes Blatt auf dunklem Holz, mit Schrift.
    fn synthetic(quad: [Point; 4], w: u32, h: u32) -> RgbImage {
        RgbImage::from_fn(w, h, |x, y| {
            let p = [x as f32 + 0.5, y as f32 + 0.5];
            if inside(&quad, p) {
                if y % 23 < 3 && x % 9 < 6 {
                    image::Rgb([40, 40, 45])
                } else {
                    let shade = 230 - (x / 30) as u8;
                    image::Rgb([shade, shade, shade - 6])
                }
            } else {
                image::Rgb([120 + (x % 7) as u8, 80, 50])
            }
        })
    }

    #[test]
    fn finds_a_tilted_sheet() {
        let truth = [[180.0, 90.0], [760.0, 140.0], [700.0, 980.0], [110.0, 930.0]];
        let det = detect(&synthetic(truth, 900, 1100));
        assert_eq!(det.source, Source::Auto);
        for (found, want) in det.quad.0.iter().zip(truth) {
            assert!(crate::geom::dist(*found, want) < 8.0, "{found:?} statt {want:?}");
        }
        assert!(det.confidence >= 80, "Sicherheit {}", det.confidence);
    }

    /// Lampenlicht oben links: das Blatt wird zur Ecke unten rechts deutlich dunkler, darf aber nicht zerfallen.
    #[test]
    fn uneven_light_keeps_the_whole_sheet() {
        let truth = [[140.0, 120.0], [820.0, 90.0], [870.0, 1040.0], [90.0, 1080.0]];
        let base = synthetic(truth, 960, 1200);
        let img = RgbImage::from_fn(960, 1200, |x, y| {
            let r = ((x as f32).powi(2) + (y as f32).powi(2)).sqrt() / 1536.0;
            let light = (1.0 - 0.9 * r * r).clamp(0.55, 1.0);
            image::Rgb(base.get_pixel(x, y).0.map(|v| (v as f32 * light) as u8))
        });
        let (det, dbg) = detect_with_debug(&img);
        if std::env::var_os("KD_TEST_DEBUG").is_some() {
            img.save("/tmp/kd-licht.png").unwrap();
            dbg.mask.save("/tmp/kd-licht-maske.png").unwrap();
            eprintln!("{det:?}");
        }
        for (found, want) in det.quad.0.iter().zip(truth) {
            assert!(crate::geom::dist(*found, want) < 12.0, "{found:?} statt {want:?}");
        }
    }

    #[test]
    fn empty_photo_uses_whole_photo() {
        let img = RgbImage::from_pixel(300, 200, image::Rgb([90, 60, 40]));
        let det = detect(&img);
        assert_eq!(det.source, Source::WholePhoto);
        assert_eq!(det.confidence, 0);
    }

    #[test]
    fn orders_corners_clockwise_from_top_left() {
        let q = order_corners([[10.0, 90.0], [90.0, 10.0], [10.0, 10.0], [90.0, 90.0]]);
        assert_eq!(q, [[10.0, 10.0], [90.0, 10.0], [90.0, 90.0], [10.0, 90.0]]);
    }
}
