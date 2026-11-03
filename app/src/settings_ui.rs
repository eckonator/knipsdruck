// SPDX-License-Identifier: MIT OR Apache-2.0
//! Einstellungen: links die Bereiche mit Icon, rechts deren Optionen in Karten (jede Wahl in einem Satz erklärt).

use eframe::egui::{self, Align2, RichText, Sense, Stroke, pos2, vec2};
use kd_core::page::{Margin, Orientation, Paper};

use crate::app::App;
use crate::dialogs::{head, modal_frame};
use crate::i18n::{self, tr, trf};
use crate::icons::{self, Icon};
use crate::settings::{BwStyle, LangSetting, ThemeSetting};
use crate::theme;
use crate::widgets::{self, Button};

/// Bereiche in der Leiste links.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum Page {
    /// Sprache, Darstellung, Verlauf, Updates.
    #[default]
    General,
    /// Papierformat, Ausrichtung, Rand.
    Paper,
    /// Schwarzweiß-Art, durchsuchbares PDF.
    Output,
    About,
}

impl Page {
    const ALL: [Page; 4] = [Page::General, Page::Paper, Page::Output, Page::About];

    fn name(self) -> &'static str {
        match self {
            Page::General => tr("Allgemein"),
            Page::Paper => tr("Papier"),
            Page::Output => tr("Druck & PDF"),
            Page::About => tr("Über"),
        }
    }

    fn icon(self) -> Icon {
        match self {
            Page::General => Icon::Gear,
            Page::Paper => Icon::Sheet,
            Page::Output => Icon::Print,
            Page::About => Icon::Info,
        }
    }

    /// Bereich zu einem Namen der Entwicklerhilfe `KD_DIALOG=einstellungen-…`.
    pub fn from_name(name: &str) -> Option<Page> {
        match name {
            "allgemein" => Some(Page::General),
            "papier" => Some(Page::Paper),
            "druck" | "pdf" => Some(Page::Output),
            "ueber" => Some(Page::About),
            _ => None,
        }
    }
}

/// Breite der Leiste links, Abstand bis zur Seite (mit Trennlinie) und Breite der Seite.
const NAV_W: f32 = 168.0;
const NAV_GAP: f32 = 21.0;
const PAGE_W: f32 = 500.0;
/// Höhe eines Eintrags in der Leiste.
const NAV_ROW_H: f32 = 40.0;
/// Höhe des Seitenbereichs: fest, damit das Fenster beim Durchklicken nicht springt.
const PAGE_H: f32 = 540.0;

pub struct SettingsState {
    page: Page,
    /// Eigenes Maß in Bearbeitung (Breite, Höhe in mm)
    custom: (f32, f32),
}

impl Default for SettingsState {
    fn default() -> Self {
        SettingsState {
            page: Page::default(),
            custom: (210.0, 297.0),
        }
    }
}

impl SettingsState {
    /// Öffnet die Einstellungen gleich auf einem Bereich.
    pub fn at(page: Page) -> Self {
        SettingsState {
            page,
            ..Default::default()
        }
    }
}

/// Zeigt die Einstellungen; false = geschlossen.
pub fn show(app: &mut App, ctx: &egui::Context, st: &mut SettingsState) -> bool {
    if let Paper::Custom { width_mm, height_mm } = app.settings.paper {
        st.custom = (width_mm, height_mm);
    }
    let before = app.settings.clone();
    let mut keep = true;
    let screen = ctx.content_rect();
    egui::Modal::new(egui::Id::new("einstellungen"))
        .frame(modal_frame(ctx))
        .show(ctx, |ui| {
            let width = NAV_W + NAV_GAP + PAGE_W;
            ui.set_width(width);
            if head(ui, ctx, tr("Einstellungen"), None) {
                keep = false;
            }
            ui.add_space(4.0);
            let h = (screen.height() - 170.0).clamp(300.0, PAGE_H);
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                ui.allocate_ui_with_layout(vec2(NAV_W, h), egui::Layout::top_down(egui::Align::LEFT), |ui| {
                    ui.spacing_mut().item_spacing.y = 4.0;
                    for page in Page::ALL {
                        if nav_entry(ui, ctx, page, st.page == page).clicked() {
                            st.page = page;
                        }
                    }
                });
                let (gap, _) = ui.allocate_exact_size(vec2(NAV_GAP, h), Sense::hover());
                ui.painter()
                    .vline(gap.left() + 10.0, gap.y_range(), Stroke::new(1.0, theme::line(ctx)));
                ui.allocate_ui_with_layout(vec2(PAGE_W, h), egui::Layout::top_down(egui::Align::LEFT), |ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    egui::ScrollArea::vertical()
                        .id_salt(("einstellungen", st.page))
                        .max_height(h)
                        .min_scrolled_height(h)
                        .show(ui, |ui| {
                            // Rand rechts für die Scrollleiste, damit die Karten nicht darunter liegen
                            ui.set_width(PAGE_W - 14.0);
                            page_content(app, ui, ctx, st);
                        });
                });
            });
        });
    if app.settings != before {
        if app.settings.language != before.language {
            i18n::apply_setting(app.settings.language);
        }
        app.save_settings();
    }
    keep
}

fn page_content(app: &mut App, ui: &mut egui::Ui, ctx: &egui::Context, st: &mut SettingsState) {
    ui.label(
        RichText::new(st.page.name())
            .family(crate::fonts::semibold())
            .size(18.0)
            .color(theme::ink(ctx)),
    );
    ui.add_space(2.0);
    match st.page {
        Page::General => {
            language(app, ui, ctx);
            appearance(app, ui, ctx);
            recent(app, ui, ctx);
            updates(app, ui, ctx);
        }
        Page::Paper => {
            paper(app, ui, ctx, st);
            orientation(app, ui, ctx);
            margin(app, ui, ctx);
        }
        Page::Output => {
            bw(app, ui, ctx);
            searchable(app, ui, ctx);
        }
        Page::About => about(app, ui, ctx),
    }
    if st.page != Page::About {
        ui.label(
            RichText::new(tr("Einstellungen werden automatisch gespeichert."))
                .size(12.5)
                .color(theme::muted(ctx)),
        );
    }
}

/// Eintrag in der Leiste links: Icon und Name, gewählt mit Akzentfläche.
fn nav_entry(ui: &mut egui::Ui, ctx: &egui::Context, page: Page, on: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(NAV_W, NAV_ROW_H), Sense::click());
    let p = ui.painter();
    if on {
        p.rect_filled(rect, 10.0, theme::accent_soft(ctx));
    } else if resp.hovered() {
        p.rect_filled(rect, 10.0, theme::desk(ctx));
    }
    let (icon_color, text_color, font) = if on {
        (
            theme::accent(ctx),
            theme::ink(ctx),
            egui::FontId::new(15.0, crate::fonts::semibold()),
        )
    } else {
        (theme::muted(ctx), theme::ink(ctx), egui::FontId::proportional(15.0))
    };
    let icon = egui::Rect::from_center_size(pos2(rect.left() + 22.0, rect.center().y), vec2(20.0, 20.0));
    icons::paint(p, icon, page.icon(), icon_color);
    p.text(
        pos2(rect.left() + 44.0, rect.center().y),
        Align2::LEFT_CENTER,
        page.name(),
        font,
        text_color,
    );
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Karte mit Überschrift für eine Einstellung.
fn card(ui: &mut egui::Ui, ctx: &egui::Context, title: &str, add: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::new()
        .fill(theme::panel(ctx).lerp_to_gamma(theme::desk(ctx), 0.45))
        .stroke(Stroke::new(1.0, theme::line(ctx)))
        .corner_radius(12)
        .inner_margin(egui::Margin::symmetric(16, 14))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(
                RichText::new(title)
                    .family(crate::fonts::semibold())
                    .size(15.5)
                    .color(theme::ink(ctx)),
            );
            ui.add_space(2.0);
            add(ui);
        });
    ui.add_space(10.0);
}

fn hint(ui: &mut egui::Ui, ctx: &egui::Context, text: &str) {
    ui.add(egui::Label::new(RichText::new(text).size(13.0).color(theme::muted(ctx))).wrap());
}

fn paper(app: &mut App, ui: &mut egui::Ui, ctx: &egui::Context, st: &mut SettingsState) {
    card(ui, ctx, tr("Papierformat"), |ui| {
        ui.horizontal_wrapped(|ui| {
            for p in Paper::FIXED {
                if widgets::choice(ui, app.settings.paper == p, &p.name()).clicked() {
                    app.settings.paper = p;
                }
            }
            let custom = matches!(app.settings.paper, Paper::Custom { .. });
            if widgets::choice(ui, custom, tr("Eigenes Maß …")).clicked() && !custom {
                app.settings.paper = Paper::Custom {
                    width_mm: st.custom.0,
                    height_mm: st.custom.1,
                };
            }
        });
        if let Paper::Custom { .. } = app.settings.paper {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label(tr("Breite"));
                let w = ui.add(
                    egui::DragValue::new(&mut st.custom.0)
                        .range(30.0..=600.0)
                        .speed(1.0)
                        .suffix(" mm"),
                );
                ui.label("×");
                ui.label(tr("Höhe"));
                let h = ui.add(
                    egui::DragValue::new(&mut st.custom.1)
                        .range(30.0..=600.0)
                        .speed(1.0)
                        .suffix(" mm"),
                );
                if w.changed() || h.changed() {
                    st.custom = (st.custom.0.round(), st.custom.1.round());
                    app.settings.paper = Paper::Custom {
                        width_mm: st.custom.0,
                        height_mm: st.custom.1,
                    };
                    // Breite > Höhe heißt Querformat; ein quadratisches Maß lässt die Wahl offen
                    if app.settings.orientation != Orientation::Auto && st.custom.0 != st.custom.1 {
                        app.settings.orientation = if st.custom.0 > st.custom.1 {
                            Orientation::Landscape
                        } else {
                            Orientation::Portrait
                        };
                    }
                }
            });
        }
        hint(ui, ctx, tr("Standard: automatisch nach Region."));
    });
}

fn orientation(app: &mut App, ui: &mut egui::Ui, ctx: &egui::Context) {
    card(ui, ctx, tr("Ausrichtung"), |ui| {
        ui.horizontal_wrapped(|ui| {
            for (o, label) in [
                (Orientation::Auto, tr("Automatisch")),
                (Orientation::Portrait, tr("Hochformat")),
                (Orientation::Landscape, tr("Querformat")),
            ] {
                if widgets::choice(ui, app.settings.orientation == o, label).clicked() {
                    app.settings.orientation = o;
                    // Beim eigenen Maß bestimmt die Ausrichtung, welche Kante oben liegt
                    if let Paper::Custom { width_mm, height_mm } = app.settings.paper
                        && o != Orientation::Auto
                    {
                        let (s, l) = (width_mm.min(height_mm), width_mm.max(height_mm));
                        let (w, h) = if o == Orientation::Landscape { (l, s) } else { (s, l) };
                        app.settings.paper = Paper::Custom {
                            width_mm: w,
                            height_mm: h,
                        };
                    }
                }
            }
        });
        hint(
            ui,
            ctx,
            tr("„Automatisch“ dreht jede Seite so, wie das Blatt auf dem Foto liegt."),
        );
    });
}

fn margin(app: &mut App, ui: &mut egui::Ui, ctx: &egui::Context) {
    card(ui, ctx, tr("Rand"), |ui| {
        ui.horizontal_wrapped(|ui| {
            for (m, label) in [
                (Margin::None, tr("Ohne")),
                (Margin::Narrow, tr("Schmal")),
                (Margin::Normal, tr("Normal")),
            ] {
                if widgets::choice(ui, app.settings.margin == m, label).clicked() {
                    app.settings.margin = m;
                }
            }
        });
        hint(ui, ctx, tr("Viele Drucker drucken nicht bis ganz an den Rand."));
    });
}

fn bw(app: &mut App, ui: &mut egui::Ui, ctx: &egui::Context) {
    card(ui, ctx, tr("Schwarzweiß-Art"), |ui| {
        ui.horizontal_wrapped(|ui| {
            for (b, label) in [
                (BwStyle::Gray, tr("Wie eine Kopie (Graustufen)")),
                (BwStyle::Pure, tr("Nur Schwarz und Weiß")),
            ] {
                if widgets::choice(ui, app.settings.bw_style == b, label).clicked() {
                    app.settings.bw_style = b;
                }
            }
        });
        hint(
            ui,
            ctx,
            tr("„Nur Schwarz und Weiß“ spart Toner, Bilder und Textmarker gehen dabei verloren."),
        );
    });
}

fn on_off(ui: &mut egui::Ui, value: &mut bool) {
    ui.horizontal_wrapped(|ui| {
        if widgets::choice(ui, *value, tr("An")).clicked() {
            *value = true;
        }
        if widgets::choice(ui, !*value, tr("Aus")).clicked() {
            *value = false;
        }
    });
}

fn searchable(app: &mut App, ui: &mut egui::Ui, ctx: &egui::Context) {
    card(ui, ctx, tr("Durchsuchbares PDF"), |ui| {
        let was_on = app.settings.searchable_pdf;
        on_off(ui, &mut app.settings.searchable_pdf);
        if app.settings.searchable_pdf && !was_on {
            kd_ocr::warm_up();
        }
        match kd_ocr::available_engine_name() {
            Some(engine) => hint(
                ui,
                ctx,
                &trf(
                    "Beim Speichern und Versenden wird der Text erkannt ({}), damit sich das PDF durchsuchen und kopieren lässt. Dauert pro Seite etwas länger; alles bleibt auf diesem Computer.",
                    &[&engine],
                ),
            ),
            None => hint(
                ui,
                ctx,
                tr(
                    "Auf diesem Computer ist keine Texterkennung verfügbar. Unter Linux hilft das Paket „tesseract-ocr“ (mit „tesseract-ocr-deu“).",
                ),
            ),
        }
    });
}

fn recent(app: &mut App, ui: &mut egui::Ui, ctx: &egui::Context) {
    card(ui, ctx, tr("Zuletzt verwendet"), |ui| {
        let was = app.settings.recent;
        on_off(ui, &mut app.settings.recent);
        if was && !app.settings.recent {
            app.forget_recent();
            app.toast(tr("Verlauf gelöscht"));
        }
        if !was && app.settings.recent {
            app.reload_recent();
        }
        hint(
            ui,
            ctx,
            tr(
                "Die letzten 10 Dokumente, 30 Tage lang, nur auf diesem Computer. Ausweiskopien werden nie gespeichert. „Aus“ löscht den Verlauf.",
            ),
        );
        ui.add_space(4.0);
        if Button::new(tr("Verlauf jetzt löschen"))
            .icon(Icon::Trash)
            .small()
            .enabled(app.settings.recent)
            .show(ui)
            .clicked()
        {
            app.forget_recent();
            app.toast(tr("Verlauf gelöscht"));
        }
    });
}

fn language(app: &mut App, ui: &mut egui::Ui, ctx: &egui::Context) {
    card(ui, ctx, tr("Sprache"), |ui| {
        ui.horizontal_wrapped(|ui| {
            for (l, label) in [
                (LangSetting::System, tr("Wie das System")),
                (LangSetting::De, "Deutsch"),
                (LangSetting::En, "English"),
            ] {
                if widgets::choice(ui, app.settings.language == l, label).clicked() {
                    app.settings.language = l;
                }
            }
        });
    });
}

fn appearance(app: &mut App, ui: &mut egui::Ui, ctx: &egui::Context) {
    card(ui, ctx, tr("Darstellung"), |ui| {
        ui.horizontal_wrapped(|ui| {
            for (t, label) in [
                (ThemeSetting::System, tr("Wie das System")),
                (ThemeSetting::Light, tr("Hell")),
                (ThemeSetting::Dark, tr("Dunkel")),
            ] {
                if widgets::choice(ui, app.settings.theme == t, label).clicked() {
                    app.settings.theme = t;
                }
            }
        });
    });
}

fn updates(app: &mut App, ui: &mut egui::Ui, ctx: &egui::Context) {
    card(ui, ctx, tr("Beim Start nach neuen Versionen suchen"), |ui| {
        let mut check = app.settings.update_check == Some(true);
        on_off(ui, &mut check);
        if check != (app.settings.update_check == Some(true)) {
            app.settings.update_check = Some(check);
        }
        hint(
            ui,
            ctx,
            tr("Dabei fragt Knipsdruck nur bei GitHub nach der neuesten Version – ohne Daten über deine Fotos."),
        );
        ui.add_space(4.0);
        if Button::new(tr("Jetzt nach Updates suchen"))
            .icon(Icon::Rotate)
            .small()
            .show(ui)
            .clicked()
        {
            app.check_updates_now(ctx);
            app.dialog = crate::dialogs::Dialog::None;
        }
    });
}

/// Wichtigste Bestandteile anderer Projekte und ihre Lizenzen (vollständige Liste: THIRD-PARTY-LICENSES.yml).
const THIRD_PARTY: &[(&str, &str)] = &[
    ("egui / eframe", "MIT OR Apache-2.0"),
    ("image", "MIT OR Apache-2.0"),
    ("rayon", "MIT OR Apache-2.0"),
    ("serde, serde_json", "MIT OR Apache-2.0"),
    ("tiny_http", "MIT OR Apache-2.0"),
    ("qrcode", "MIT OR Apache-2.0"),
    ("if-addrs", "MIT OR BSD-3-Clause"),
    ("rfd, arboard", "MIT OR Apache-2.0"),
    ("objc2 (macOS)", "MIT"),
    ("windows, windows-sys", "MIT OR Apache-2.0"),
    ("Inter", "SIL OFL 1.1"),
    ("DejaVu Sans", "Bitstream Vera"),
];

fn about(app: &mut App, ui: &mut egui::Ui, ctx: &egui::Context) {
    ui.horizontal(|ui| {
        let (r, _) = ui.allocate_exact_size(vec2(52.0, 52.0), Sense::hover());
        icons::logo(ui.painter(), r, theme::accent(ctx), theme::ink(ctx), theme::panel(ctx));
        ui.vertical(|ui| {
            let hash = env!("KD_GIT_HASH");
            let version = if hash.is_empty() {
                format!("Knipsdruck {}", env!("CARGO_PKG_VERSION"))
            } else {
                format!("Knipsdruck {} ({hash})", env!("CARGO_PKG_VERSION"))
            };
            ui.label(RichText::new(version).family(crate::fonts::semibold()).size(16.0));
            ui.label(tr("Macht aus Handyfotos von Dokumenten druckfertige Seiten."));
            ui.horizontal(|ui| {
                ui.label("© 2026 Markus Eckert ·");
                ui.hyperlink_to(
                    "github.com/eckonator/knipsdruck",
                    "https://github.com/eckonator/knipsdruck",
                );
            });
        });
    });
    ui.add_space(6.0);
    hint(
        ui,
        ctx,
        tr(
            "Freie Software, lizenziert unter MIT oder Apache-2.0, nach Wahl. Alles wird auf diesem Computer berechnet, kein Foto verlässt ihn. Ohne jegliche Gewährleistung.",
        ),
    );
    ui.add_space(6.0);
    ui.horizontal_wrapped(|ui| {
        if Button::new(tr("MIT-Lizenz anzeigen")).show(ui).clicked() {
            app.license = Some((tr("MIT-Lizenz"), include_str!("../../LICENSE-MIT")));
        }
        if Button::new(tr("Apache-Lizenz 2.0 anzeigen")).show(ui).clicked() {
            app.license = Some((tr("Apache-Lizenz 2.0"), include_str!("../../LICENSE-APACHE")));
        }
    });
    ui.add_space(8.0);
    egui::CollapsingHeader::new(tr("Lizenzen von Drittanbietern")).show(ui, |ui| {
        egui::Grid::new("fremd")
            .num_columns(2)
            .spacing([16.0, 4.0])
            .show(ui, |ui| {
                for (name, license) in THIRD_PARTY {
                    ui.label(*name);
                    ui.label(RichText::new(*license).color(theme::muted(ctx)));
                    ui.end_row();
                }
            });
        hint(
            ui,
            ctx,
            tr("Die vollständige Liste liegt als THIRD-PARTY-LICENSES.yml im Programmordner."),
        );
    });
    ui.add_space(8.0);
}
