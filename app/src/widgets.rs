// SPDX-License-Identifier: MIT OR Apache-2.0
//! Eigene Bedienelemente im Stil der App: Hauptknopf, Rahmenknopf, Icon-Knopf, Umschalter, Auswahl.

use eframe::egui::{self, Align2, Color32, CornerRadius, FontId, Response, Sense, Stroke, StrokeKind, Ui, Vec2, vec2};

use crate::icons::{self, Icon};
use crate::theme;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Akzentfarbe, nur einer je Bildschirm
    Primary,
    /// Rahmen auf Fläche
    Secondary,
    /// Nur Text, beim Darüberfahren hinterlegt
    Quiet,
}

pub struct Button<'a> {
    text: &'a str,
    icon: Option<Icon>,
    kind: Kind,
    large: bool,
    small: bool,
    enabled: bool,
    min_width: f32,
}

impl<'a> Button<'a> {
    pub fn new(text: &'a str) -> Self {
        Button {
            text,
            icon: None,
            kind: Kind::Secondary,
            large: false,
            small: false,
            enabled: true,
            min_width: 0.0,
        }
    }

    pub fn icon(mut self, icon: Icon) -> Self {
        self.icon = Some(icon);
        self
    }

    pub fn primary(mut self) -> Self {
        self.kind = Kind::Primary;
        self
    }

    pub fn quiet(mut self) -> Self {
        self.kind = Kind::Quiet;
        self
    }

    pub fn large(mut self) -> Self {
        self.large = true;
        self
    }

    /// Kompakt (Knöpfe unter den Seitenkarten)
    pub fn small(mut self) -> Self {
        self.small = true;
        self
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    pub fn min_width(mut self, w: f32) -> Self {
        self.min_width = w;
        self
    }

    pub fn show(self, ui: &mut Ui) -> Response {
        let ctx = ui.ctx().clone();
        let (font, pad, height, icon_size, radius) = if self.large {
            (FontId::new(17.0, crate::fonts::semibold()), 26.0, 52.0, 22.0, 14)
        } else if self.small {
            (FontId::proportional(14.0), 10.0, 36.0, 17.0, 10)
        } else {
            (FontId::new(15.0, crate::fonts::semibold()), 14.0, 38.0, 18.0, 10)
        };
        let galley = ui
            .painter()
            .layout_no_wrap(self.text.to_string(), font, Color32::PLACEHOLDER);
        let icon_w = if self.icon.is_some() { icon_size + 8.0 } else { 0.0 };
        let text_w = if self.text.is_empty() { -8.0 } else { galley.size().x };
        let width = (pad * 2.0 + icon_w + text_w).max(self.min_width).max(height);
        let sense = if self.enabled { Sense::click() } else { Sense::hover() };
        let (rect, resp) = ui.allocate_exact_size(vec2(width, height), sense);
        let hovered = resp.hovered() && self.enabled;
        let (fill, stroke, fg) = match self.kind {
            Kind::Primary => {
                let fill = if hovered {
                    theme::accent(&ctx).gamma_multiply(0.9)
                } else {
                    theme::accent(&ctx)
                };
                (fill, Stroke::NONE, theme::accent_ink(&ctx))
            }
            Kind::Secondary => (
                if hovered { theme::desk(&ctx) } else { theme::panel(&ctx) },
                Stroke::new(1.0, if hovered { theme::muted(&ctx) } else { theme::line(&ctx) }),
                theme::ink(&ctx),
            ),
            Kind::Quiet => (
                if hovered {
                    theme::desk(&ctx)
                } else {
                    Color32::TRANSPARENT
                },
                Stroke::NONE,
                theme::ink(&ctx),
            ),
        };
        let fg = if self.enabled { fg } else { fg.gamma_multiply(0.45) };
        let fill = if self.enabled { fill } else { fill.gamma_multiply(0.6) };
        ui.painter()
            .rect(rect, CornerRadius::same(radius), fill, stroke, StrokeKind::Inside);
        let content_w = icon_w + text_w.max(0.0);
        let mut x = rect.center().x - content_w / 2.0;
        if let Some(icon) = self.icon {
            let r =
                egui::Rect::from_center_size(egui::pos2(x + icon_size / 2.0, rect.center().y), Vec2::splat(icon_size));
            icons::paint(ui.painter(), r, icon, fg);
            x += icon_w;
        }
        if !self.text.is_empty() {
            ui.painter()
                .galley(egui::pos2(x, rect.center().y - galley.size().y / 2.0), galley, fg);
        }
        if resp.has_focus() {
            ui.painter().rect_stroke(
                rect.expand(2.0),
                radius + 2,
                Stroke::new(2.0, theme::accent(&ctx)),
                StrokeKind::Outside,
            );
        }
        if self.enabled {
            resp.on_hover_cursor(egui::CursorIcon::PointingHand)
        } else {
            resp
        }
    }
}

/// Verzierung links vom Text in einem Umschalter (Farbpunkt).
pub type Deco<'a> = Option<&'a dyn Fn(&egui::Painter, egui::Rect)>;

/// Runder Icon-Knopf (Kopfzeile, Dialog schließen).
pub fn icon_button(ui: &mut Ui, icon: Icon, tooltip: &str) -> Response {
    let ctx = ui.ctx().clone();
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(40.0), Sense::click());
    if resp.hovered() {
        ui.painter().circle_filled(rect.center(), 20.0, theme::desk(&ctx));
    }
    let color = if resp.hovered() {
        theme::ink(&ctx)
    } else {
        theme::muted(&ctx)
    };
    icons::paint(ui.painter(), rect.shrink(9.0), icon, color);
    resp.on_hover_text(tooltip)
        .on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Umschalter mit zwei oder mehr Teilen (Farbe | Schwarzweiß). Liefert den gewählten Wert, wenn er sich ändert.
pub fn segmented<T: Copy + PartialEq>(ui: &mut Ui, value: T, options: &[(T, &str, Deco)], pill: bool) -> Option<T> {
    let ctx = ui.ctx().clone();
    let font = FontId::new(15.0, crate::fonts::semibold());
    let widths: Vec<f32> = options
        .iter()
        .map(|(_, t, d)| {
            let g = ui
                .painter()
                .layout_no_wrap(t.to_string(), font.clone(), Color32::PLACEHOLDER);
            g.size().x + 36.0 + if d.is_some() { 24.0 } else { 0.0 }
        })
        .collect();
    let h = 42.0;
    let total: f32 = widths.iter().sum::<f32>() + 8.0;
    let (outer, _) = ui.allocate_exact_size(vec2(total, h), Sense::hover());
    let r_outer = if pill {
        CornerRadius::same(21)
    } else {
        CornerRadius::same(10)
    };
    let r_inner = if pill {
        CornerRadius::same(17)
    } else {
        CornerRadius::same(7)
    };
    ui.painter().rect(
        outer,
        r_outer,
        theme::desk(&ctx),
        Stroke::new(1.0, theme::line(&ctx)),
        StrokeKind::Inside,
    );
    let mut x = outer.left() + 4.0;
    let mut chosen = None;
    for ((v, text, deco), w) in options.iter().zip(&widths) {
        let r = egui::Rect::from_min_size(egui::pos2(x, outer.top() + 4.0), vec2(*w, h - 8.0));
        let resp = ui.interact(r, ui.id().with(("seg", x as i32)), Sense::click());
        let on = *v == value;
        if on {
            ui.painter()
                .rect_filled(r.translate(vec2(0.0, 1.0)), r_inner, Color32::from_black_alpha(18));
            ui.painter().rect_filled(r, r_inner, theme::panel(&ctx));
        }
        let color = if on || resp.hovered() {
            theme::ink(&ctx)
        } else {
            theme::muted(&ctx)
        };
        let g = ui
            .painter()
            .layout_no_wrap(text.to_string(), FontId::new(15.0, crate::fonts::semibold()), color);
        let deco_w = if deco.is_some() { 24.0 } else { 0.0 };
        let start = r.center().x - (g.size().x + deco_w) / 2.0;
        if let Some(d) = deco {
            d(
                ui.painter(),
                egui::Rect::from_center_size(egui::pos2(start + 8.0, r.center().y), Vec2::splat(16.0)),
            );
        }
        ui.painter()
            .galley(egui::pos2(start + deco_w, r.center().y - g.size().y / 2.0), g, color);
        if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() && !on {
            chosen = Some(*v);
        }
        x += w;
    }
    chosen
}

/// Auswahl-Knopf in den Einstellungen (gewählt: Akzentrahmen und -fläche).
pub fn choice(ui: &mut Ui, selected: bool, text: &str) -> Response {
    let ctx = ui.ctx().clone();
    let font = if selected {
        FontId::new(15.0, crate::fonts::semibold())
    } else {
        FontId::proportional(15.0)
    };
    let g = ui
        .painter()
        .layout_no_wrap(text.to_string(), font, Color32::PLACEHOLDER);
    let (rect, resp) = ui.allocate_exact_size(vec2(g.size().x + 28.0, 36.0), Sense::click());
    let (fill, stroke) = if selected {
        (theme::accent_soft(&ctx), Stroke::new(1.5, theme::accent(&ctx)))
    } else if resp.hovered() {
        (theme::desk(&ctx), Stroke::new(1.0, theme::muted(&ctx)))
    } else {
        (theme::panel(&ctx), Stroke::new(1.0, theme::line(&ctx)))
    };
    ui.painter().rect(rect, 10, fill, stroke, StrokeKind::Inside);
    ui.painter().galley(rect.center() - g.size() / 2.0, g, theme::ink(&ctx));
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Unterstrichener Link-Text in Akzentfarbe.
pub fn link(ui: &mut Ui, text: &str) -> Response {
    let ctx = ui.ctx().clone();
    let resp = ui.add(
        egui::Label::new(
            egui::RichText::new(text)
                .color(theme::accent(&ctx))
                .underline()
                .family(crate::fonts::semibold()),
        )
        .sense(Sense::click()),
    );
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Text mittig in ein Rechteck.
pub fn centered_text(ui: &Ui, rect: egui::Rect, text: &str, size: f32, color: Color32) {
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        text,
        FontId::proportional(size),
        color,
    );
}

/// Farbpunkt für „Farbe“ im Umschalter.
pub fn color_dot(p: &egui::Painter, r: egui::Rect) {
    let colors = [
        Color32::from_rgb(0xe9, 0x4b, 0x4b),
        Color32::from_rgb(0xf2, 0xc9, 0x4c),
        Color32::from_rgb(0x3f, 0xb3, 0x6b),
        Color32::from_rgb(0x3d, 0x8b, 0xf0),
        Color32::from_rgb(0xa0, 0x5b, 0xe0),
    ];
    let c = r.center();
    let rad = r.width() / 2.0;
    for (i, col) in colors.iter().enumerate() {
        let a0 = i as f32 / 5.0 * std::f32::consts::TAU - std::f32::consts::FRAC_PI_2;
        let a1 = (i + 1) as f32 / 5.0 * std::f32::consts::TAU - std::f32::consts::FRAC_PI_2;
        let pts: Vec<egui::Pos2> = std::iter::once(c)
            .chain((0..=8).map(|k| {
                let a = a0 + (a1 - a0) * k as f32 / 8.0;
                c + vec2(a.cos(), a.sin()) * rad
            }))
            .collect();
        p.add(egui::Shape::convex_polygon(pts, *col, Stroke::NONE));
    }
}

/// Halb schwarz, halb hell für „Schwarzweiß“.
pub fn bw_dot(p: &egui::Painter, r: egui::Rect) {
    let c = r.center();
    let rad = r.width() / 2.0;
    p.circle_filled(c, rad, Color32::from_gray(0xdd));
    let pts: Vec<egui::Pos2> = (0..=16)
        .map(|k| {
            let a = std::f32::consts::FRAC_PI_2 + std::f32::consts::PI * k as f32 / 16.0;
            c + vec2(a.cos(), a.sin()) * rad
        })
        .collect();
    p.add(egui::Shape::convex_polygon(pts, Color32::from_gray(0x11), Stroke::NONE));
    p.circle_stroke(c, rad, Stroke::new(1.0, Color32::from_gray(0x88)));
}
