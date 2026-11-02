// SPDX-License-Identifier: MIT OR Apache-2.0
//! Farben und Grundoptik: eine Akzentfarbe, ruhige Flächen, hell und dunkel gleichwertig.

use eframe::egui::{self, Color32, Context, CornerRadius, FontFamily, FontId, Stroke, TextStyle};

fn dark(ctx: &Context) -> bool {
    ctx.global_style().visuals.dark_mode
}

macro_rules! pair {
    ($name:ident, $light:expr, $dark:expr) => {
        pub fn $name(ctx: &Context) -> Color32 {
            if dark(ctx) { $dark } else { $light }
        }
    };
}

pair!(
    accent,
    Color32::from_rgb(0x1f, 0x5b, 0xd6),
    Color32::from_rgb(0x5b, 0x8f, 0xf9)
);
pair!(accent_ink, Color32::WHITE, Color32::from_rgb(0x0d, 0x14, 0x24));
pair!(
    accent_soft,
    Color32::from_rgb(0xe3, 0xeb, 0xfb),
    Color32::from_rgb(0x23, 0x31, 0x4f)
);
// Hintergrund hinter den Seiten („Schreibtisch“)
pair!(
    desk,
    Color32::from_rgb(0xe7, 0xe7, 0xec),
    Color32::from_rgb(0x18, 0x18, 0x1b)
);
// Flächen von Leisten, Karten und Dialogen
pair!(panel, Color32::WHITE, Color32::from_rgb(0x24, 0x24, 0x2a));
pair!(
    ink,
    Color32::from_rgb(0x1a, 0x1c, 0x22),
    Color32::from_rgb(0xec, 0xee, 0xf2)
);
pair!(
    muted,
    Color32::from_rgb(0x5a, 0x61, 0x6d),
    Color32::from_rgb(0xa8, 0xad, 0xb8)
);
pair!(
    line,
    Color32::from_rgb(0xd4, 0xd5, 0xdc),
    Color32::from_rgb(0x3a, 0x3a, 0x42)
);
pair!(
    warn,
    Color32::from_rgb(0x8a, 0x58, 0x00),
    Color32::from_rgb(0xe3, 0xa7, 0x2f)
);
pair!(
    warn_bg,
    Color32::from_rgb(0xff, 0xf3, 0xd6),
    Color32::from_rgb(0x3a, 0x2f, 0x17)
);
pair!(
    danger,
    Color32::from_rgb(0xa8, 0x24, 0x24),
    Color32::from_rgb(0xef, 0x6b, 0x6b)
);
pair!(
    ok,
    Color32::from_rgb(0x12, 0x73, 0x3b),
    Color32::from_rgb(0x3e, 0xc2, 0x7a)
);

/// Schatten unter Seiten und Karten.
pub fn shadow(ctx: &Context) -> egui::epaint::Shadow {
    egui::epaint::Shadow {
        offset: [0, 4],
        blur: 16,
        spread: 0,
        color: Color32::from_black_alpha(if dark(ctx) { 110 } else { 34 }),
    }
}

pub fn apply(ctx: &Context) {
    ctx.all_styles_mut(|style| {
        style.text_styles = [
            (TextStyle::Small, FontId::new(12.5, FontFamily::Proportional)),
            (TextStyle::Body, FontId::new(15.0, FontFamily::Proportional)),
            (TextStyle::Button, FontId::new(15.0, FontFamily::Proportional)),
            (TextStyle::Heading, FontId::new(22.0, FontFamily::Proportional)),
            (TextStyle::Monospace, FontId::new(13.5, FontFamily::Monospace)),
        ]
        .into();
        style.spacing.item_spacing = egui::vec2(8.0, 8.0);
        style.spacing.button_padding = egui::vec2(12.0, 7.0);
        style.spacing.interact_size.y = 34.0;
        let v = &mut style.visuals;
        let d = v.dark_mode;
        let pick = |l: Color32, dk: Color32| if d { dk } else { l };
        let ink = pick(Color32::from_rgb(0x1a, 0x1c, 0x22), Color32::from_rgb(0xec, 0xee, 0xf2));
        let line = pick(Color32::from_rgb(0xd4, 0xd5, 0xdc), Color32::from_rgb(0x3a, 0x3a, 0x42));
        let panel = pick(Color32::WHITE, Color32::from_rgb(0x24, 0x24, 0x2a));
        let desk = pick(Color32::from_rgb(0xe7, 0xe7, 0xec), Color32::from_rgb(0x18, 0x18, 0x1b));
        let accent = pick(Color32::from_rgb(0x1f, 0x5b, 0xd6), Color32::from_rgb(0x5b, 0x8f, 0xf9));
        v.panel_fill = desk;
        v.window_fill = panel;
        v.extreme_bg_color = panel;
        v.faint_bg_color = desk;
        v.window_stroke = Stroke::new(1.0, line);
        v.window_corner_radius = CornerRadius::same(16);
        v.menu_corner_radius = CornerRadius::same(12);
        v.weak_text_color = Some(pick(
            Color32::from_rgb(0x5a, 0x61, 0x6d),
            Color32::from_rgb(0xa8, 0xad, 0xb8),
        ));
        v.hyperlink_color = accent;
        v.selection.bg_fill = accent.gamma_multiply(0.35);
        v.selection.stroke = Stroke::new(1.5, accent);
        let r = CornerRadius::same(10);
        for w in [
            &mut v.widgets.noninteractive,
            &mut v.widgets.inactive,
            &mut v.widgets.hovered,
            &mut v.widgets.active,
            &mut v.widgets.open,
        ] {
            w.corner_radius = r;
            w.fg_stroke.color = ink;
        }
        v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, line);
        v.widgets.inactive.bg_fill = panel;
        v.widgets.inactive.weak_bg_fill = panel;
        v.widgets.inactive.bg_stroke = Stroke::new(1.0, line);
        v.widgets.hovered.weak_bg_fill = desk;
        v.widgets.hovered.bg_fill = desk;
        v.widgets.hovered.bg_stroke = Stroke::new(
            1.0,
            pick(Color32::from_rgb(0x9a, 0x9f, 0xab), Color32::from_rgb(0x60, 0x60, 0x6a)),
        );
        v.widgets.active.weak_bg_fill = desk;
        v.widgets.active.bg_stroke = Stroke::new(1.0, accent);
    });
}
