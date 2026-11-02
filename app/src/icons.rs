// SPDX-License-Identifier: MIT OR Apache-2.0
//! Linien-Icons, mit dem Painter gezeichnet (24er-Raster, 2 px Strich), damit sie in jeder Größe scharf sind.

use std::f32::consts::{FRAC_PI_2, PI, TAU};

use eframe::egui::{Color32, Painter, Pos2, Rect, Shape, Stroke, vec2};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Icon {
    Rotate,
    Corners,
    More,
    Print,
    Pdf,
    Mail,
    Phone,
    Gear,
    Undo,
    NewDoc,
    Plus,
    Close,
    Warn,
    Trash,
    Left,
    Right,
    Photo,
    Card,
    Bulb,
}

/// Zeichenhilfe: Koordinaten im 24er-Raster auf das Zielrechteck abbilden.
struct Pen<'a> {
    p: &'a Painter,
    o: Pos2,
    s: f32,
    stroke: Stroke,
}

impl Pen<'_> {
    fn pt(&self, x: f32, y: f32) -> Pos2 {
        self.o + vec2(x, y) * self.s
    }

    fn line(&self, pts: &[(f32, f32)]) {
        let pts: Vec<Pos2> = pts.iter().map(|&(x, y)| self.pt(x, y)).collect();
        self.p.add(Shape::line(pts, self.stroke));
    }

    fn closed(&self, pts: &[(f32, f32)]) {
        let pts: Vec<Pos2> = pts.iter().map(|&(x, y)| self.pt(x, y)).collect();
        self.p.add(Shape::closed_line(pts, self.stroke));
    }

    fn rect(&self, x: f32, y: f32, w: f32, h: f32, r: f32) {
        let rect = Rect::from_min_size(self.pt(x, y), vec2(w, h) * self.s);
        self.p
            .rect_stroke(rect, r * self.s, self.stroke, eframe::egui::StrokeKind::Middle);
    }

    fn circle(&self, x: f32, y: f32, r: f32) {
        self.p.circle_stroke(self.pt(x, y), r * self.s, self.stroke);
    }

    fn dot(&self, x: f32, y: f32, r: f32) {
        self.p.circle_filled(self.pt(x, y), r * self.s, self.stroke.color);
    }

    /// Kreisbogen um (cx, cy) von `a0` nach `a1` (Bogenmaß, im Uhrzeigersinn bei y nach unten).
    fn arc(&self, cx: f32, cy: f32, r: f32, a0: f32, a1: f32) {
        let n = 24;
        let pts: Vec<Pos2> = (0..=n)
            .map(|i| {
                let a = a0 + (a1 - a0) * i as f32 / n as f32;
                self.pt(cx + r * a.cos(), cy + r * a.sin())
            })
            .collect();
        self.p.add(Shape::line(pts, self.stroke));
    }
}

pub fn paint(painter: &Painter, rect: Rect, icon: Icon, color: Color32) {
    let size = rect.width().min(rect.height());
    let s = size / 24.0;
    let pen = Pen {
        p: painter,
        o: rect.center() - vec2(12.0, 12.0) * s,
        s,
        stroke: Stroke::new((2.0 * s).max(1.2), color),
    };
    match icon {
        Icon::Rotate => {
            pen.arc(12.0, 12.0, 8.0, -PI * 0.35, PI * 1.55);
            pen.line(&[(17.0, 3.5), (17.6, 8.6), (12.6, 8.2)]);
        }
        Icon::Corners => {
            pen.line(&[(4.0, 9.0), (4.0, 4.0), (9.0, 4.0)]);
            pen.line(&[(15.0, 4.0), (20.0, 4.0), (20.0, 9.0)]);
            pen.line(&[(20.0, 15.0), (20.0, 20.0), (15.0, 20.0)]);
            pen.line(&[(9.0, 20.0), (4.0, 20.0), (4.0, 15.0)]);
        }
        Icon::More => {
            for x in [5.0, 12.0, 19.0] {
                pen.dot(x, 12.0, 1.9);
            }
        }
        Icon::Print => {
            pen.line(&[(6.0, 9.0), (6.0, 3.0), (18.0, 3.0), (18.0, 9.0)]);
            pen.rect(3.0, 9.0, 18.0, 8.0, 2.0);
            pen.closed(&[(6.0, 14.0), (18.0, 14.0), (18.0, 21.0), (6.0, 21.0)]);
        }
        Icon::Pdf => {
            pen.closed(&[
                (14.0, 3.0),
                (6.0, 3.0),
                (4.5, 4.5),
                (4.5, 19.5),
                (6.0, 21.0),
                (18.0, 21.0),
                (19.5, 19.5),
                (19.5, 8.5),
            ]);
            pen.line(&[(14.0, 3.0), (14.0, 8.5), (19.5, 8.5)]);
            pen.line(&[(12.0, 11.5), (12.0, 17.5)]);
            pen.line(&[(9.0, 14.5), (12.0, 17.5), (15.0, 14.5)]);
        }
        Icon::Mail => {
            pen.rect(3.0, 5.0, 18.0, 14.0, 2.0);
            pen.line(&[(3.5, 7.0), (12.0, 13.0), (20.5, 7.0)]);
        }
        Icon::Phone => {
            pen.rect(6.5, 2.0, 11.0, 20.0, 2.5);
            pen.line(&[(11.0, 18.0), (13.0, 18.0)]);
        }
        Icon::Gear => {
            pen.circle(12.0, 12.0, 3.0);
            let teeth: Vec<(f32, f32)> = (0..16)
                .map(|i| {
                    let a = i as f32 / 16.0 * TAU;
                    let r = if (i / 2) % 2 == 0 { 9.0 } else { 7.0 };
                    let a = a + if i % 2 == 0 { -0.12 } else { 0.12 };
                    (12.0 + r * a.cos(), 12.0 + r * a.sin())
                })
                .collect();
            pen.closed(&teeth);
        }
        Icon::Undo => {
            pen.line(&[(9.0, 14.0), (4.0, 9.0), (9.0, 4.0)]);
            pen.line(&[(4.0, 9.0), (15.0, 9.0)]);
            pen.arc(15.0, 14.0, 5.0, -FRAC_PI_2, FRAC_PI_2);
            pen.line(&[(15.0, 19.0), (12.0, 19.0)]);
        }
        Icon::NewDoc => {
            pen.closed(&[
                (14.0, 3.0),
                (6.0, 3.0),
                (4.5, 4.5),
                (4.5, 19.5),
                (6.0, 21.0),
                (18.0, 21.0),
                (19.5, 19.5),
                (19.5, 8.5),
            ]);
            pen.line(&[(14.0, 3.0), (14.0, 8.5), (19.5, 8.5)]);
            pen.line(&[(12.0, 12.0), (12.0, 18.0)]);
            pen.line(&[(9.0, 15.0), (15.0, 15.0)]);
        }
        Icon::Plus => {
            pen.line(&[(12.0, 5.0), (12.0, 19.0)]);
            pen.line(&[(5.0, 12.0), (19.0, 12.0)]);
        }
        Icon::Close => {
            pen.line(&[(6.0, 6.0), (18.0, 18.0)]);
            pen.line(&[(18.0, 6.0), (6.0, 18.0)]);
        }
        Icon::Warn => {
            pen.closed(&[(12.0, 3.5), (21.5, 20.0), (2.5, 20.0)]);
            pen.line(&[(12.0, 9.5), (12.0, 14.0)]);
            pen.dot(12.0, 17.0, 1.2);
        }
        Icon::Trash => {
            pen.line(&[(3.0, 6.0), (21.0, 6.0)]);
            pen.line(&[(8.0, 6.0), (8.0, 4.0), (16.0, 4.0), (16.0, 6.0)]);
            pen.line(&[(5.5, 6.0), (6.5, 20.0), (17.5, 20.0), (18.5, 6.0)]);
        }
        Icon::Left => pen.line(&[(15.0, 5.0), (8.0, 12.0), (15.0, 19.0)]),
        Icon::Right => pen.line(&[(9.0, 5.0), (16.0, 12.0), (9.0, 19.0)]),
        Icon::Photo => {
            pen.rect(3.0, 5.0, 18.0, 14.0, 2.0);
            pen.circle(8.5, 10.0, 1.5);
            pen.line(&[(21.0, 16.0), (16.0, 11.0), (7.0, 19.0)]);
        }
        Icon::Card => {
            pen.rect(2.5, 5.5, 19.0, 13.0, 2.0);
            pen.circle(8.0, 11.0, 2.0);
            pen.line(&[(5.0, 16.0), (11.0, 16.0)]);
            pen.line(&[(14.0, 10.0), (19.0, 10.0)]);
            pen.line(&[(14.0, 13.5), (18.0, 13.5)]);
        }
        Icon::Bulb => {
            pen.line(&[(9.0, 18.0), (15.0, 18.0)]);
            pen.line(&[(10.0, 21.5), (14.0, 21.5)]);
            pen.arc(12.0, 9.0, 6.0, PI * 0.8, PI * 2.2);
            pen.line(&[(7.6, 13.2), (9.0, 15.0), (9.0, 18.0)]);
            pen.line(&[(16.4, 13.2), (15.0, 15.0), (15.0, 18.0)]);
        }
    }
}

/// Kleines Logo der App (schräges Foto hinter geradem Blatt) für Kopfzeile und „Über“.
pub fn logo(painter: &Painter, rect: Rect, accent: Color32, ink: Color32, paper: Color32) {
    let s = rect.width().min(rect.height()) / 40.0;
    let o = rect.center() - vec2(20.0, 20.0) * s;
    let pt = |x: f32, y: f32| o + vec2(x, y) * s;
    // gestricheltes, gekipptes Foto
    let tilt: Vec<Pos2> = [(4.0, 11.0), (20.0, 8.0), (22.5, 31.0), (6.5, 34.0)]
        .iter()
        .map(|&(x, y)| pt(x, y))
        .collect();
    for i in 0..4 {
        let (a, b) = (tilt[i], tilt[(i + 1) % 4]);
        painter.extend(Shape::dashed_line(
            &[a, b],
            Stroke::new(2.0 * s, ink.gamma_multiply(0.55)),
            3.0 * s,
            2.5 * s,
        ));
    }
    let sheet = Rect::from_min_size(pt(18.0, 6.0), vec2(18.0, 25.0) * s);
    painter.rect_filled(sheet, 2.0 * s, accent);
    for (y, w) in [(13.0, 10.0), (17.0, 10.0), (21.0, 7.0)] {
        painter.line_segment([pt(22.0, y), pt(22.0 + w, y)], Stroke::new(2.0 * s, paper));
    }
    painter.line_segment([pt(13.0, 35.0), pt(29.0, 35.0)], Stroke::new(2.4 * s, ink));
}
