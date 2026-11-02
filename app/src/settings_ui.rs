// SPDX-License-Identifier: MIT OR Apache-2.0
//! Einstellungen (bewusst kurz, jede Wahl in einem Satz erklärt) und „Über“.

use eframe::egui::{self, RichText, Sense, vec2};
use kd_core::page::{Margin, Orientation, Paper};

use crate::app::App;
use crate::dialogs::{head, modal_frame};
use crate::i18n::{self, tr, trf};
use crate::icons;
use crate::settings::{BwStyle, LangSetting, ThemeSetting};
use crate::theme;
use crate::widgets::{self, Button};

pub struct SettingsState {
    /// Eigenes Maß in Bearbeitung (Breite, Höhe in mm)
    custom: (f32, f32),
}

impl Default for SettingsState {
    fn default() -> Self {
        SettingsState { custom: (210.0, 297.0) }
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
            ui.set_width(560.0);
            if head(ui, ctx, tr("Einstellungen"), None) {
                keep = false;
            }
            let h = (screen.height() - 170.0).max(300.0);
            egui::ScrollArea::vertical()
                .max_height(h)
                .min_scrolled_height(h)
                .show(ui, |ui| {
                    ui.set_width(540.0);
                    paper(app, ui, ctx, st);
                    orientation(app, ui, ctx);
                    margin(app, ui, ctx);
                    bw(app, ui, ctx);
                    searchable(app, ui, ctx);
                    recent(app, ui, ctx);
                    language(app, ui, ctx);
                    appearance(app, ui, ctx);
                    updates(app, ui, ctx);
                    about(app, ui, ctx);
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

fn section(ui: &mut egui::Ui, ctx: &egui::Context, title: &str) {
    ui.add_space(14.0);
    ui.label(
        RichText::new(title)
            .family(crate::fonts::semibold())
            .size(15.5)
            .color(theme::ink(ctx)),
    );
    ui.add_space(4.0);
}

fn hint(ui: &mut egui::Ui, ctx: &egui::Context, text: &str) {
    ui.add(egui::Label::new(RichText::new(text).size(13.0).color(theme::muted(ctx))).wrap());
}

fn end(ui: &mut egui::Ui) {
    ui.add_space(12.0);
    ui.separator();
}

fn paper(app: &mut App, ui: &mut egui::Ui, ctx: &egui::Context, st: &mut SettingsState) {
    section(ui, ctx, tr("Papierformat"));
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
    end(ui);
}

fn orientation(app: &mut App, ui: &mut egui::Ui, ctx: &egui::Context) {
    section(ui, ctx, tr("Ausrichtung"));
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
    end(ui);
}

fn margin(app: &mut App, ui: &mut egui::Ui, ctx: &egui::Context) {
    section(ui, ctx, tr("Rand"));
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
    end(ui);
}

fn bw(app: &mut App, ui: &mut egui::Ui, ctx: &egui::Context) {
    section(ui, ctx, tr("Schwarzweiß-Art"));
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
    end(ui);
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
    section(ui, ctx, tr("Durchsuchbares PDF"));
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
    end(ui);
}

fn recent(app: &mut App, ui: &mut egui::Ui, ctx: &egui::Context) {
    section(ui, ctx, tr("Zuletzt verwendet"));
    let was = app.settings.recent;
    ui.horizontal_wrapped(|ui| {
        on_off(ui, &mut app.settings.recent);
        if Button::new(tr("Verlauf jetzt löschen")).quiet().show(ui).clicked() {
            app.forget_recent();
            app.toast(tr("Verlauf gelöscht"));
        }
    });
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
    end(ui);
}

fn language(app: &mut App, ui: &mut egui::Ui, ctx: &egui::Context) {
    section(ui, ctx, tr("Sprache"));
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
    end(ui);
}

fn appearance(app: &mut App, ui: &mut egui::Ui, ctx: &egui::Context) {
    section(ui, ctx, tr("Darstellung"));
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
    end(ui);
}

fn updates(app: &mut App, ui: &mut egui::Ui, ctx: &egui::Context) {
    section(ui, ctx, tr("Updates"));
    let mut check = app.settings.update_check == Some(true);
    ui.horizontal_wrapped(|ui| {
        if ui
            .checkbox(&mut check, tr("Beim Start nach neuen Versionen suchen"))
            .changed()
        {
            app.settings.update_check = Some(check);
        }
        if Button::new(tr("Jetzt nach Updates suchen")).quiet().show(ui).clicked() {
            app.check_updates_now(ctx);
            app.dialog = crate::dialogs::Dialog::None;
        }
    });
    hint(
        ui,
        ctx,
        tr("Dabei fragt Knipsdruck nur bei GitHub nach der neuesten Version – ohne Daten über deine Fotos."),
    );
    end(ui);
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
    section(ui, ctx, tr("Über"));
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
