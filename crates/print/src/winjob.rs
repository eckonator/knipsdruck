// SPDX-License-Identifier: MIT OR Apache-2.0
//! Plattformunabhängige Teile des Windows-Drucks, damit sie sich überall testen lassen.

use image::RgbImage;

/// Seitenindizes (ab 0) in Druckreihenfolge. `ranges` sind die Bereiche aus dem Dialog (ab 1,
/// einschließlich); leer heißt alle Seiten. Mit `collate` kommt jede Kopie als ganzer Satz,
/// sonst jede Seite `copies`-mal hintereinander.
pub(crate) fn print_sequence(page_count: usize, ranges: &[(u32, u32)], copies: u32, collate: bool) -> Vec<usize> {
    let selected: Vec<usize> = if ranges.is_empty() {
        (0..page_count).collect()
    } else {
        ranges
            .iter()
            .flat_map(|&(from, to)| from.min(to)..=from.max(to))
            .filter(|&p| p >= 1 && (p as usize) <= page_count)
            .map(|p| p as usize - 1)
            .collect()
    };
    let copies = copies.max(1) as usize;
    if collate {
        (0..copies).flat_map(|_| selected.iter().copied()).collect()
    } else {
        selected.iter().flat_map(|&p| std::iter::repeat_n(p, copies)).collect()
    }
}

/// Bytes pro Zeile einer 24-Bit-DIB: auf 4 Bytes aufgefüllt.
pub(crate) fn dib_stride(width: u32) -> usize {
    (width as usize * 3).div_ceil(4) * 4
}

/// Die Zeilen `y0..y1` als 24-Bit-DIB von unten nach oben (BGR, Zeilen auf 4 Bytes aufgefüllt).
pub(crate) fn dib_stripe(image: &RgbImage, y0: u32, y1: u32) -> Vec<u8> {
    let stride = dib_stride(image.width());
    let mut bits = vec![0u8; stride * (y1 - y0) as usize];
    for (row, y) in (y0..y1).rev().enumerate() {
        let line = &mut bits[row * stride..];
        for x in 0..image.width() {
            let [r, g, b] = image.get_pixel(x, y).0;
            let i = x as usize * 3;
            line[i..i + 3].copy_from_slice(&[b, g, r]);
        }
    }
    bits
}

/// Abschnitte `(y0, y1)` des Bildes mit höchstens `rows` Zeilen.
pub(crate) fn stripes(height: u32, rows: u32) -> impl Iterator<Item = (u32, u32)> {
    (0..height)
        .step_by(rows.max(1) as usize)
        .map(move |y0| (y0, (y0 + rows.max(1)).min(height)))
}

/// Gerätezeile für Bildzeile `y`, wenn `height` Bildzeilen auf `dest` Gerätezeilen kommen. Gerundet,
/// damit aneinanderstoßende Streifen weder Lücken noch Überlappungen haben.
pub(crate) fn scaled(y: u32, height: u32, dest: i32) -> i32 {
    (f64::from(y) * f64::from(dest) / f64::from(height.max(1))).round() as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reihenfolge() {
        assert_eq!(print_sequence(3, &[], 1, true), vec![0, 1, 2]);
        assert_eq!(print_sequence(2, &[], 2, true), vec![0, 1, 0, 1]);
        assert_eq!(print_sequence(2, &[], 2, false), vec![0, 0, 1, 1]);
        assert_eq!(print_sequence(5, &[(2, 3), (5, 9)], 1, true), vec![1, 2, 4]);
        assert_eq!(print_sequence(5, &[(3, 2)], 0, true), vec![1, 2]);
    }

    #[test]
    fn dib_zeilen() {
        let mut img = RgbImage::new(2, 2);
        img.put_pixel(0, 0, image::Rgb([1, 2, 3]));
        img.put_pixel(1, 0, image::Rgb([4, 5, 6]));
        img.put_pixel(0, 1, image::Rgb([7, 8, 9]));
        assert_eq!(dib_stride(2), 8);
        assert_eq!(dib_stride(4), 12);
        let bits = dib_stripe(&img, 0, 2);
        // Unterste Bildzeile zuerst, BGR, 2 Bytes Füllung.
        assert_eq!(bits, vec![9, 8, 7, 0, 0, 0, 0, 0, 3, 2, 1, 6, 5, 4, 0, 0]);
        assert_eq!(dib_stripe(&img, 1, 2), vec![9, 8, 7, 0, 0, 0, 0, 0]);
    }

    #[test]
    fn streifen() {
        assert_eq!(
            stripes(1100, 512).collect::<Vec<_>>(),
            vec![(0, 512), (512, 1024), (1024, 1100)]
        );
        assert_eq!(scaled(512, 1100, 2200), 1024);
        assert_eq!(scaled(1100, 1100, 2201), 2201);
    }
}
