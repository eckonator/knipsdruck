// SPDX-License-Identifier: MIT OR Apache-2.0
//! Seiten setzen: Papierformat, Ausrichtung, Rand, Einpassen statt Strecken, Ausweiskopie.
//!
//! Ein entzerrtes Blatt füllt die Seite, wenn sein Seitenverhältnis (nach Einrasten auf DIN A) zum
//! bedruckbaren Bereich passt; sonst wird es mittig eingepasst. Verzerrt wird nie.

use image::RgbImage;
use serde::{Deserialize, Serialize};

use crate::enhance::{self, ColorMode};
use crate::geom::{self, Rgbf};
use crate::{PageSettings, Quad};

/// Ausweis, Bankkarte, Führerschein (ID-1).
pub const CARD_MM: (f32, f32) = (85.6, 54.0);
/// Abstand der ersten Karte vom oberen Blattrand und zwischen den Karten.
const CARD_TOP_MM: f32 = 25.0;
const CARD_GAP_MM: f32 = 15.0;

#[derive(Clone, Copy, Debug, PartialEq, Default, Serialize, Deserialize)]
pub enum Paper {
    A3,
    #[default]
    A4,
    A5,
    A6,
    B5,
    Letter,
    Legal,
    Custom {
        width_mm: f32,
        height_mm: f32,
    },
}

impl Paper {
    pub const FIXED: [Paper; 7] = [
        Paper::A3,
        Paper::A4,
        Paper::A5,
        Paper::A6,
        Paper::B5,
        Paper::Letter,
        Paper::Legal,
    ];

    /// Kurze und lange Kante in mm.
    pub fn mm(self) -> (f32, f32) {
        match self {
            Paper::A3 => (297.0, 420.0),
            Paper::A4 => (210.0, 297.0),
            Paper::A5 => (148.0, 210.0),
            Paper::A6 => (105.0, 148.0),
            Paper::B5 => (176.0, 250.0),
            Paper::Letter => (215.9, 279.4),
            Paper::Legal => (215.9, 355.6),
            Paper::Custom { width_mm, height_mm } => (width_mm.min(height_mm), width_mm.max(height_mm)),
        }
    }

    pub fn name(self) -> String {
        match self {
            Paper::A3 => "A3".into(),
            Paper::A4 => "A4".into(),
            Paper::A5 => "A5".into(),
            Paper::A6 => "A6".into(),
            Paper::B5 => "B5".into(),
            Paper::Letter => "US Letter".into(),
            Paper::Legal => "US Legal".into(),
            Paper::Custom { width_mm, height_mm } => format!("{width_mm:.0} × {height_mm:.0} mm"),
        }
    }

    pub fn is_square(self) -> bool {
        let (s, l) = self.mm();
        (l - s).abs() < 0.5
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Orientation {
    /// Jede Seite so, wie das Blatt auf dem Foto liegt
    #[default]
    Auto,
    Portrait,
    Landscape,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Margin {
    None,
    Narrow,
    #[default]
    Normal,
}

impl Margin {
    /// Anteil der kurzen Papierkante je Seite.
    pub fn fraction(self) -> f32 {
        match self {
            Margin::None => 0.0,
            Margin::Narrow => 0.025,
            Margin::Normal => 0.05,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Layout {
    /// Jedes Foto eine Seite
    #[default]
    Pages,
    /// Je zwei Karten in Originalgröße auf ein Blatt
    IdCard,
}

/// Was für das ganze Dokument gilt.
#[derive(Clone, Copy, Debug, PartialEq, Default, Serialize, Deserialize)]
pub struct DocSettings {
    pub paper: Paper,
    pub orientation: Orientation,
    pub margin: Margin,
    pub mode: ColorMode,
    pub layout: Layout,
}

/// Fertige Seite in Druckauflösung, mit ihrer Größe auf Papier.
pub struct OutPage {
    pub image: RgbImage,
    pub width_mm: f32,
    pub height_mm: f32,
}

/// Liegt das Blatt (nach der Drehung durch den Nutzer) quer?
pub fn content_landscape(quad: &Quad, rotation: u16) -> bool {
    let (w, h) = quad.size();
    (w > h * 1.05) != !rotation.is_multiple_of(180)
}

/// Liegt die fertige Seite quer?
pub fn sheet_landscape(ps: &PageSettings, doc: &DocSettings) -> bool {
    match (doc.layout, doc.orientation) {
        (Layout::Pages, Orientation::Portrait) => false,
        (Layout::Pages, Orientation::Landscape) => true,
        _ => content_landscape(&ps.quad, ps.rotation),
    }
}

/// Größe der fertigen Seite in mm (Breite, Höhe).
pub fn page_mm(ps: &PageSettings, doc: &DocSettings) -> (f32, f32) {
    if doc.layout == Layout::IdCard {
        return if sheet_landscape(ps, doc) {
            CARD_MM
        } else {
            (CARD_MM.1, CARD_MM.0)
        };
    }
    let (s, l) = doc.paper.mm();
    if sheet_landscape(ps, doc) { (l, s) } else { (s, l) }
}

/// Seitenverhältnis Breite/Höhe des Blatts nach der Drehung, auf DIN A eingerastet, wenn es nahe liegt.
fn content_aspect(ps: &PageSettings) -> f32 {
    let (w, h) = ps.quad.size();
    let mut r = (w / h.max(1.0)).max(0.05);
    let din = if r > 1.0 {
        std::f32::consts::SQRT_2
    } else {
        std::f32::consts::FRAC_1_SQRT_2
    };
    if (r / din).ln().abs() < 0.15 {
        r = din;
    }
    if ps.quarter_turned() { 1.0 / r } else { r }
}

/// Lage des Blatts auf der Seite in Pixeln: (x, y, Breite, Höhe) nach der Drehung.
fn content_box(page_px: (usize, usize), margin_px: usize, aspect: f32) -> (usize, usize, usize, usize) {
    let (pw, ph) = page_px;
    let (aw, ah) = (
        pw.saturating_sub(2 * margin_px).max(1),
        ph.saturating_sub(2 * margin_px).max(1),
    );
    let area = aw as f32 / ah as f32;
    let (cw, ch) = if (aspect / area).ln().abs() < 0.03 {
        (aw, ah)
    } else if aspect > area {
        (aw, ((aw as f32 / aspect).round() as usize).clamp(1, ah))
    } else {
        (((ah as f32 * aspect).round() as usize).clamp(1, aw), ah)
    };
    (margin_px + (aw - cw) / 2, margin_px + (ah - ch) / 2, cw, ch)
}

fn mm_to_px(mm: f32, dpi: f32) -> usize {
    ((mm / 25.4 * dpi).round() as usize).max(1)
}

/// Blatt entzerren und aufbereiten, Ergebnis in Größe `w` × `h` nach der Drehung (normalisiert, noch ohne Farbmodus).
pub fn prepare_content(photo: &RgbImage, ps: &PageSettings, w: usize, h: usize, card: bool) -> Rgbf {
    let (ww, wh) = if ps.quarter_turned() { (h, w) } else { (w, h) };
    let warped = geom::warp(photo, &ps.quad, ww, wh);
    let mut n = enhance::normalize(&warped);
    // Kartenmotive reichen bis an den Rand; dort nichts wegputzen
    if !card {
        enhance::clean_border(&mut n);
    }
    // Wird das Foto stark vergrößert (WhatsApp), kräftiger schärfen
    let (qw, qh) = ps.quad.size();
    let upscale = (ww as f32 / qw.max(1.0)).max(wh as f32 / qh.max(1.0));
    enhance::sharpen(&mut n, if upscale > 1.6 { 0.9 } else { 0.5 });
    if !ps.rotation.is_multiple_of(360) {
        n.rotated(ps.rotation % 360)
    } else {
        n
    }
}

/// Eine Seite im Layout „Jedes Foto eine Seite“ in `dpi`.
pub fn render_page(photo: &RgbImage, ps: &PageSettings, doc: &DocSettings, dpi: f32) -> OutPage {
    let (wmm, hmm) = page_mm(ps, doc);
    let (pw, ph) = (mm_to_px(wmm, dpi), mm_to_px(hmm, dpi));
    let margin = mm_to_px(doc.margin.fraction() * wmm.min(hmm), dpi).min(pw.min(ph) / 4);
    let margin = if doc.margin == Margin::None { 0 } else { margin };
    let (x, y, cw, ch) = content_box((pw, ph), margin, content_aspect(ps));
    let content = prepare_content(photo, ps, cw, ch, false);
    let mut page = Rgbf::filled(pw, ph, 1.0);
    page.paste(&content, x, y);
    OutPage {
        image: enhance::colorize(&page, doc.mode),
        width_mm: wmm,
        height_mm: hmm,
    }
}

/// Eine Karte (Ausweiskopie) in Originalgröße und `dpi`.
pub fn render_card(photo: &RgbImage, ps: &PageSettings, mode: ColorMode, dpi: f32) -> OutPage {
    let landscape = content_landscape(&ps.quad, ps.rotation);
    let (wmm, hmm) = if landscape { CARD_MM } else { (CARD_MM.1, CARD_MM.0) };
    let content = prepare_content(photo, ps, mm_to_px(wmm, dpi), mm_to_px(hmm, dpi), true);
    OutPage {
        image: enhance::colorize(&content, mode),
        width_mm: wmm,
        height_mm: hmm,
    }
}

/// Lage der Karten auf einem Blatt der Ausweiskopie in mm: (x, y) je Karte.
pub fn card_positions(cards: &[(f32, f32)], paper: Paper) -> Vec<(f32, f32)> {
    let (pw, _) = paper.mm();
    let mut y = CARD_TOP_MM;
    cards
        .iter()
        .map(|&(w, h)| {
            let pos = ((pw - w) / 2.0, y);
            y += h + CARD_GAP_MM;
            pos
        })
        .collect()
}

/// Ein Blatt der Ausweiskopie aus bis zu zwei fertigen Karten (hochkant, im Papierformat).
pub fn compose_id_sheet(cards: &[OutPage], paper: Paper, dpi: f32) -> OutPage {
    let (wmm, hmm) = paper.mm();
    let mut sheet = RgbImage::from_pixel(
        mm_to_px(wmm, dpi) as u32,
        mm_to_px(hmm, dpi) as u32,
        image::Rgb([255; 3]),
    );
    let sizes: Vec<(f32, f32)> = cards.iter().map(|c| (c.width_mm, c.height_mm)).collect();
    for (card, (x, y)) in cards.iter().zip(card_positions(&sizes, paper)) {
        image::imageops::overlay(
            &mut sheet,
            &card.image,
            mm_to_px(x, dpi) as i64,
            mm_to_px(y, dpi) as i64,
        );
    }
    OutPage {
        image: sheet,
        width_mm: wmm,
        height_mm: hmm,
    }
}

/// Das ganze Dokument in `dpi`; `on_page` wird nach jeder fertigen Seite aufgerufen (Fortschritt).
pub fn render_document(
    pages: &[(&RgbImage, PageSettings)],
    doc: &DocSettings,
    dpi: f32,
    mut on_page: impl FnMut(usize),
) -> Vec<OutPage> {
    match doc.layout {
        Layout::Pages => pages
            .iter()
            .enumerate()
            .map(|(i, (photo, ps))| {
                let p = render_page(photo, ps, doc, dpi);
                on_page(i + 1);
                p
            })
            .collect(),
        Layout::IdCard => pages
            .chunks(2)
            .enumerate()
            .map(|(i, pair)| {
                let cards: Vec<OutPage> = pair.iter().map(|(p, ps)| render_card(p, ps, doc.mode, dpi)).collect();
                on_page((i + 1) * 2);
                compose_id_sheet(&cards, doc.paper, dpi)
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Source;

    fn settings(w: f32, h: f32, rotation: u16) -> PageSettings {
        PageSettings {
            quad: Quad([[0.0, 0.0], [w, 0.0], [w, h], [0.0, h]]),
            source: Source::Auto,
            rotation,
        }
    }

    #[test]
    fn orientation_follows_the_sheet_unless_forced() {
        let doc = DocSettings::default();
        assert_eq!(page_mm(&settings(100.0, 141.0, 0), &doc), (210.0, 297.0));
        assert_eq!(page_mm(&settings(141.0, 100.0, 0), &doc), (297.0, 210.0));
        assert_eq!(page_mm(&settings(100.0, 141.0, 90), &doc), (297.0, 210.0));
        let quer = DocSettings {
            orientation: Orientation::Landscape,
            ..doc
        };
        assert_eq!(page_mm(&settings(100.0, 141.0, 0), &quer), (297.0, 210.0));
    }

    #[test]
    fn din_sheet_fills_a5_but_is_fitted_on_letter() {
        let area = (1000, 1414);
        let (_, _, cw, ch) = content_box(area, 0, std::f32::consts::FRAC_1_SQRT_2);
        assert_eq!((cw, ch), area);
        let letter = (1000, 1294);
        let (x, y, cw, ch) = content_box(letter, 0, std::f32::consts::FRAC_1_SQRT_2);
        assert_eq!(ch, 1294);
        assert!(cw < 1000 && x > 0 && y == 0, "{x} {cw}");
    }

    #[test]
    fn rendered_page_has_paper_size() {
        let photo = RgbImage::from_pixel(210, 297, image::Rgb([240, 240, 240]));
        let p = render_page(&photo, &settings(210.0, 297.0, 0), &DocSettings::default(), 50.0);
        assert_eq!((p.width_mm, p.height_mm), (210.0, 297.0));
        assert_eq!(p.image.dimensions(), (413, 585));
        // Rand bleibt weiß, Papier wird weiß
        assert_eq!(p.image.get_pixel(2, 2).0, [255, 255, 255]);
        assert_eq!(p.image.get_pixel(200, 300).0, [255, 255, 255]);
    }

    #[test]
    fn id_copy_puts_two_cards_on_one_sheet_at_actual_size() {
        let photo = RgbImage::from_pixel(856, 540, image::Rgb([200, 60, 60]));
        let ps = settings(856.0, 540.0, 0);
        let doc = DocSettings {
            layout: Layout::IdCard,
            ..DocSettings::default()
        };
        let out = render_document(&[(&photo, ps), (&photo, ps), (&photo, ps)], &doc, 100.0, |_| {});
        assert_eq!(out.len(), 2);
        assert_eq!((out[0].width_mm, out[0].height_mm), (210.0, 297.0));
        let pos = card_positions(&[CARD_MM, CARD_MM], Paper::A4);
        assert!((pos[0].0 - 62.2).abs() < 0.01 && pos[1].1 == 25.0 + 54.0 + 15.0);
    }
}
