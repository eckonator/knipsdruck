// SPDX-License-Identifier: MIT OR Apache-2.0
//! Dialoge über dem Hauptfenster: große Vorschau, PDF/E-Mail, Drucken unter Linux, Lizenztexte.
//! Ecken-Editor, Einstellungen und Handy-Empfang haben eigene Module.

use std::path::PathBuf;

use eframe::egui::{self, Color32, RichText, Sense, Stroke, vec2};
use kd_core::page::Layout;

use crate::app::{self, Action, App, Preview};
use crate::i18n::{tr, trf};
use crate::icons::Icon;
use crate::settings::PdfQuality;
use crate::theme;
use crate::widgets::{self, Button};
use crate::worker::Target;

pub enum Dialog {
    None,
    Settings(crate::settings_ui::SettingsState),
    Corners(Box<crate::corners::CornerEditor>),
    Preview(PreviewState),
    Phone(Box<crate::phone_ui::PhoneUi>),
    Output(OutputDialog),
    LinuxPrint(LinuxPrint),
}

pub fn show(app: &mut App, ctx: &egui::Context) {
    let dialog = std::mem::replace(&mut app.dialog, Dialog::None);
    let next = match dialog {
        Dialog::None => Dialog::None,
        Dialog::Settings(mut s) => {
            if crate::settings_ui::show(app, ctx, &mut s) {
                Dialog::Settings(s)
            } else {
                Dialog::None
            }
        }
        Dialog::Corners(mut e) => {
            if e.show(app, ctx) {
                Dialog::Corners(e)
            } else {
                Dialog::None
            }
        }
        Dialog::Preview(mut p) => {
            if preview(app, ctx, &mut p) {
                Dialog::Preview(p)
            } else {
                Dialog::None
            }
        }
        Dialog::Phone(mut p) => {
            if p.show(app, ctx) {
                Dialog::Phone(p)
            } else {
                Dialog::None
            }
        }
        Dialog::Output(mut o) => {
            if output(app, ctx, &mut o) {
                Dialog::Output(o)
            } else {
                Dialog::None
            }
        }
        Dialog::LinuxPrint(mut l) => {
            if linux_print(app, ctx, &mut l) {
                Dialog::LinuxPrint(l)
            } else {
                Dialog::None
            }
        }
    };
    // Ein Dialog kann einen anderen öffnen (Vorschau → Ecken); dann gilt der neue
    if matches!(app.dialog, Dialog::None) {
        app.dialog = next;
    }
    license_window(app, ctx);
}

/// Kopf eines Dialogs mit Titel, Untertitel und Schließen-Knopf. Liefert true, wenn geschlossen werden soll.
pub fn head(ui: &mut egui::Ui, ctx: &egui::Context, title: &str, subtitle: Option<&str>) -> bool {
    let mut close = false;
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.label(
                RichText::new(title)
                    .size(20.0)
                    .family(crate::fonts::semibold())
                    .color(theme::ink(ctx)),
            );
            if let Some(s) = subtitle {
                ui.label(RichText::new(s).color(theme::muted(ctx)));
            }
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
            close = widgets::icon_button(ui, Icon::Close, tr("Schließen")).clicked();
        });
    });
    ui.add_space(6.0);
    ui.separator();
    close || ctx.input(|i| i.key_pressed(egui::Key::Escape))
}

pub fn modal_frame(ctx: &egui::Context) -> egui::Frame {
    egui::Frame::new()
        .fill(theme::panel(ctx))
        .corner_radius(18)
        .inner_margin(egui::Margin::same(22))
        .shadow(egui::epaint::Shadow {
            offset: [0, 20],
            blur: 60,
            spread: 0,
            color: Color32::from_black_alpha(90),
        })
}

// ----------------------------------------------------------------------
// Große Vorschau
// ----------------------------------------------------------------------

pub struct PreviewState {
    pub id: u64,
    pub original: bool,
    /// Zuletzt fertig gerechnete große Vorschau
    pub large: Option<Preview>,
    /// Schlüssel der zuletzt angeforderten großen Vorschau
    requested: Option<String>,
    photo_tex: Option<(u64, egui::TextureHandle)>,
}

impl PreviewState {
    pub fn new(id: u64) -> Self {
        PreviewState {
            id,
            original: false,
            large: None,
            requested: None,
            photo_tex: None,
        }
    }
}

const LARGE_PX: f32 = 1700.0;

fn preview(app: &mut App, ctx: &egui::Context, st: &mut PreviewState) -> bool {
    let Some(idx) = app.doc.index_of(st.id) else {
        return false;
    };
    let page = app.doc.pages[idx].clone();
    let doc = app.doc_settings();
    let key = format!("{:?}|{:?}|{LARGE_PX}|gross", page.settings, doc);
    if st.requested.as_deref() != Some(&key) {
        app.worker.preview(
            page.id(),
            key.clone(),
            page.photo.image.clone(),
            page.settings,
            doc,
            LARGE_PX,
        );
        st.requested = Some(key.clone());
    }
    if st.photo_tex.as_ref().is_none_or(|(id, _)| *id != page.id()) {
        let img = &page.photo.image;
        let t = image::imageops::thumbnail(&**img, 1600, 1600 * img.height() / img.width().max(1));
        st.photo_tex = Some((
            page.id(),
            ctx.load_texture("original", app::color_image(&t), egui::TextureOptions::LINEAR),
        ));
    }
    let mut keep = true;
    let mut go: Option<usize> = None;
    let screen = ctx.content_rect();
    let size = vec2((screen.width() - 80.0).min(1200.0), (screen.height() - 80.0).min(980.0));
    egui::Modal::new(egui::Id::new("vorschau"))
        .frame(modal_frame(ctx))
        .show(ctx, |ui| {
            ui.set_width(size.x);
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(trf("Seite {} von {}", &[&(idx + 1), &app.doc.pages.len()]))
                        .size(20.0)
                        .family(crate::fonts::semibold())
                        .color(theme::ink(ctx)),
                );
                ui.add_space(16.0);
                let options: [(bool, &str, widgets::Deco); 2] =
                    [(false, tr("Ergebnis"), None), (true, tr("Originalfoto"), None)];
                if let Some(v) = widgets::segmented(ui, st.original, &options, false) {
                    st.original = v;
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if widgets::icon_button(ui, Icon::Close, tr("Schließen")).clicked() {
                        keep = false;
                    }
                });
            });
            ui.add_space(10.0);
            let (rect, _) = ui.allocate_exact_size(vec2(size.x, size.y - 150.0), Sense::hover());
            ui.painter().rect_filled(rect, 12, theme::desk(ctx));
            let inner = rect.shrink(20.0);
            if st.original {
                if let Some((_, t)) = &st.photo_tex {
                    app::paint_texture(ui.painter(), t, inner, 1.0);
                }
            } else if let Some(tex) = st
                .large
                .as_ref()
                .filter(|l| l.key == key)
                .map(|l| &l.tex)
                .or_else(|| app.previews.get(&page.id()).map(|p| &p.tex))
            {
                let s = tex.size_vec2();
                let scale = (inner.width() / s.x).min(inner.height() / s.y);
                let r = egui::Rect::from_center_size(inner.center(), s * scale);
                ui.painter()
                    .add(theme::shadow(ctx).as_shape(r, egui::CornerRadius::same(2)));
                app::paint_texture(ui.painter(), tex, r, 1.0);
            }
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if Button::new(tr("Vorherige"))
                    .icon(Icon::Left)
                    .enabled(idx > 0)
                    .show(ui)
                    .clicked()
                {
                    go = Some(idx - 1);
                }
                if Button::new(tr("Nächste"))
                    .icon(Icon::Right)
                    .enabled(idx + 1 < app.doc.pages.len())
                    .show(ui)
                    .clicked()
                {
                    go = Some(idx + 1);
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let more = Button::new("")
                        .icon(Icon::More)
                        .show(ui)
                        .on_hover_text(tr("Weitere Aktionen"));
                    egui::Popup::menu(&more).width(280.0).show(|ui| {
                        let layout = app.layout;
                        let (print, pdf, mail) = match layout {
                            Layout::Pages => (
                                tr("Nur diese Seite drucken"),
                                tr("Nur diese Seite als PDF speichern"),
                                tr("Nur diese Seite per E-Mail senden"),
                            ),
                            Layout::IdCard => (
                                tr("Nur dieses Blatt drucken"),
                                tr("Nur dieses Blatt als PDF speichern"),
                                tr("Nur dieses Blatt per E-Mail senden"),
                            ),
                        };
                        for (icon, text, action) in [
                            (Icon::Print, print, Action::Print),
                            (Icon::Pdf, pdf, Action::Pdf),
                            (Icon::Mail, mail, Action::Mail),
                        ] {
                            if app::menu_item(ui, ctx, icon, text, false).clicked() {
                                ui.close();
                                keep = false;
                                app.start_output(action, Some(page.id()));
                            }
                        }
                        ui.separator();
                        if app::menu_item(ui, ctx, Icon::Trash, tr("Seite entfernen"), true).clicked() {
                            ui.close();
                            keep = false;
                            app.remove(page.id());
                        }
                    });
                    if Button::new(tr("Ecken anpassen"))
                        .icon(Icon::Corners)
                        .show(ui)
                        .on_hover_text("E")
                        .clicked()
                    {
                        app.dialog = Dialog::Corners(Box::new(crate::corners::CornerEditor::new(app, ctx, page.id())));
                        keep = false;
                    }
                    if Button::new(tr("Drehen"))
                        .icon(Icon::Rotate)
                        .show(ui)
                        .on_hover_text("R")
                        .clicked()
                    {
                        app.rotate(page.id());
                    }
                });
            });
        });
    let (left, right, rot, corners, esc, del) = ctx.input(|i| {
        (
            i.key_pressed(egui::Key::ArrowLeft),
            i.key_pressed(egui::Key::ArrowRight),
            i.key_pressed(egui::Key::R),
            i.key_pressed(egui::Key::E),
            i.key_pressed(egui::Key::Escape),
            i.key_pressed(egui::Key::Backspace) || i.key_pressed(egui::Key::Delete),
        )
    });
    if left && idx > 0 {
        go = Some(idx - 1);
    }
    if right && idx + 1 < app.doc.pages.len() {
        go = Some(idx + 1);
    }
    if rot {
        app.rotate(page.id());
    }
    if corners && keep {
        app.dialog = Dialog::Corners(Box::new(crate::corners::CornerEditor::new(app, ctx, page.id())));
        keep = false;
    }
    if del {
        app.remove(page.id());
        keep = false;
    }
    if esc {
        keep = false;
    }
    if let Some(i) = go {
        st.id = app.doc.pages[i].id();
        st.large = None;
        st.requested = None;
    }
    keep
}

// ----------------------------------------------------------------------
// PDF speichern / per E-Mail senden
// ----------------------------------------------------------------------

pub struct OutputDialog {
    action: Action,
    scope: Option<u64>,
    quality: PdfQuality,
    /// geschätzte Größen (Druck, E-Mail) in Byte
    sizes: (f64, f64),
}

impl OutputDialog {
    pub fn new(app: &App, action: Action, scope: Option<u64>) -> Self {
        let quality = if action == Action::Mail {
            PdfQuality::Email
        } else {
            app.settings.pdf_quality
        };
        OutputDialog {
            action,
            scope,
            quality,
            sizes: estimate(app, scope),
        }
    }
}

/// Größe vorab schätzen: Kartenvorschauen als JPEG kodieren und auf die Zielauflösung hochrechnen.
fn estimate(app: &App, scope: Option<u64>) -> (f64, f64) {
    let pages = app.output_pages(scope);
    let mut per_px = (0.0f64, 0.0f64);
    let mut n = 0.0;
    for p in pages.iter().take(3) {
        let Some(pv) = app.previews.get(&p.id()) else { continue };
        let px = (pv.image.width() * pv.image.height()) as f64;
        let enc = |q: u8| {
            let mut out = Vec::new();
            let _ = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, q).encode_image(&*pv.image);
            out.len() as f64
        };
        per_px.0 += enc(PdfQuality::Print.jpeg()) / px;
        per_px.1 += enc(PdfQuality::Email.jpeg()) / px;
        n += 1.0;
    }
    if n == 0.0 {
        return (0.0, 0.0);
    }
    let doc = app.doc_settings();
    let area_mm2: f64 = match app.layout {
        Layout::Pages => pages
            .iter()
            .map(|p| {
                let (w, h) = kd_core::page::page_mm(&p.settings, &doc);
                (w * h) as f64
            })
            .sum(),
        Layout::IdCard => pages.len() as f64 * 85.6 * 54.0 + pages.len().div_ceil(2) as f64 * 2000.0,
    };
    let px = |dpi: f64| area_mm2 * (dpi / 25.4).powi(2);
    let gray = if app.settings.color { 1.0 } else { 0.7 };
    // Höhere Auflösung komprimiert besser je Pixel
    (
        per_px.0 / n * px(300.0) * 0.45 * gray,
        per_px.1 / n * px(150.0) * 0.6 * gray,
    )
}

fn mb(bytes: f64) -> String {
    if bytes <= 0.0 {
        return String::new();
    }
    let v = bytes / 1e6;
    let s = if v < 0.1 {
        "< 0,1".to_string()
    } else {
        format!("{v:.1}")
    };
    let s = if crate::i18n::lang() == crate::i18n::Lang::En {
        s.replace(',', ".")
    } else {
        s.replace('.', ",")
    };
    trf("ca. {} MB", &[&s])
}

fn output(app: &mut App, ctx: &egui::Context, o: &mut OutputDialog) -> bool {
    let mut keep = true;
    let mut go = false;
    let title = if o.action == Action::Mail {
        tr("Per E-Mail senden")
    } else {
        tr("Als PDF speichern")
    };
    let count = {
        let n = app.output_pages(o.scope).len();
        if app.layout == Layout::IdCard { n.div_ceil(2) } else { n }
    };
    let scope_text = match (o.scope, app.layout) {
        (Some(id), Layout::Pages) => trf("Nur Seite {}", &[&(app.doc.index_of(id).unwrap_or(0) + 1)]),
        (Some(id), Layout::IdCard) => trf("Nur Blatt {}", &[&(app.doc.index_of(id).unwrap_or(0) / 2 + 1)]),
        (None, Layout::Pages) => trf("Alle {} Seiten", &[&count]),
        (None, Layout::IdCard) => trf("Alle {} Blätter", &[&count]),
    };
    egui::Modal::new(egui::Id::new("ausgabe-dialog"))
        .frame(modal_frame(ctx))
        .show(ctx, |ui| {
            ui.set_width(440.0);
            if head(ui, ctx, title, None) {
                keep = false;
            }
            ui.add_space(10.0);
            ui.label(
                RichText::new(scope_text)
                    .family(crate::fonts::semibold())
                    .color(theme::ink(ctx)),
            );
            ui.add_space(6.0);
            for (q, name, detail, size) in [
                (
                    PdfQuality::Print,
                    tr("Zum Drucken"),
                    tr("Beste Qualität (300 dpi)"),
                    o.sizes.0,
                ),
                (
                    PdfQuality::Email,
                    tr("Für E-Mail"),
                    tr("Kleinere Datei (150 dpi), gut lesbar"),
                    o.sizes.1,
                ),
            ] {
                if quality_option(ui, ctx, o.quality == q, name, detail, &mb(size)).clicked() {
                    o.quality = q;
                }
                ui.add_space(4.0);
            }
            let note = if o.sizes.1 > 2e6 {
                tr("Auch klein über 2 MB – bei manchen Postfächern die Seiten auf zwei E-Mails verteilen.")
            } else {
                tr("Die Größen sind geschätzt.")
            };
            ui.label(RichText::new(note).small().color(theme::muted(ctx)));
            if app.settings.searchable_pdf {
                ui.label(
                    RichText::new(tr("Mit Texterkennung: Das PDF lässt sich durchsuchen."))
                        .small()
                        .color(theme::muted(ctx)),
                );
            }
            ui.add_space(14.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let label = if o.action == Action::Mail {
                    tr("E-Mail vorbereiten")
                } else {
                    tr("Speichern …")
                };
                if Button::new(label).primary().show(ui).clicked() || ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    go = true;
                }
                if Button::new(tr("Abbrechen")).quiet().show(ui).clicked() {
                    keep = false;
                }
            });
        });
    if go {
        let name = app.pdf_name(o.scope, o.quality);
        match o.action {
            Action::Pdf => {
                app.settings.pdf_quality = o.quality;
                app.save_settings();
                let dir = dirs::document_dir().or_else(dirs::home_dir).unwrap_or_default();
                if let Some(path) = rfd::FileDialog::new()
                    .set_title(tr("Als PDF speichern"))
                    .set_directory(dir)
                    .set_file_name(&name)
                    .add_filter("PDF", &["pdf"])
                    .save_file()
                {
                    let path = if path.extension().is_none() {
                        path.with_extension("pdf")
                    } else {
                        path
                    };
                    app.run_export(Action::Pdf, o.scope, o.quality, Target::Save(path));
                }
            }
            Action::Mail => {
                let dir = dirs::document_dir()
                    .or_else(dirs::home_dir)
                    .unwrap_or_else(std::env::temp_dir)
                    .join("Knipsdruck");
                app.run_export(Action::Mail, o.scope, o.quality, Target::Mail(dir.join(name)));
            }
            Action::Print => {}
        }
        keep = false;
    }
    keep
}

fn quality_option(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    on: bool,
    name: &str,
    detail: &str,
    size: &str,
) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 66.0), Sense::click());
    let (fill, stroke) = if on {
        (theme::accent_soft(ctx), Stroke::new(1.5, theme::accent(ctx)))
    } else if resp.hovered() {
        (theme::desk(ctx), Stroke::new(1.0, theme::muted(ctx)))
    } else {
        (theme::panel(ctx), Stroke::new(1.0, theme::line(ctx)))
    };
    ui.painter().rect(rect, 12, fill, stroke, egui::StrokeKind::Inside);
    let p = ui.painter();
    p.text(
        rect.left_top() + vec2(16.0, 14.0),
        egui::Align2::LEFT_TOP,
        name,
        egui::FontId::proportional(16.0),
        theme::ink(ctx),
    );
    p.text(
        rect.left_top() + vec2(16.0, 37.0),
        egui::Align2::LEFT_TOP,
        detail,
        egui::FontId::proportional(13.5),
        theme::muted(ctx),
    );
    p.text(
        rect.right_center() - vec2(16.0, 0.0),
        egui::Align2::RIGHT_CENTER,
        size,
        egui::FontId::proportional(15.0),
        if on { theme::ink(ctx) } else { theme::muted(ctx) },
    );
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

// ----------------------------------------------------------------------
// Drucken unter Linux (kein Systemdialog)
// ----------------------------------------------------------------------

pub struct LinuxPrint {
    pdf: PathBuf,
    media: String,
    printers: Vec<kd_print::Printer>,
    selected: usize,
    copies: u32,
}

impl LinuxPrint {
    pub fn new(pdf: PathBuf, media: String) -> Self {
        let printers: Vec<kd_print::Printer> = kd_print::printers()
            .into_iter()
            .filter(|p| !p.name.to_lowercase().contains("fax"))
            .collect();
        let selected = printers.iter().position(|p| p.is_default).unwrap_or(0);
        LinuxPrint {
            pdf,
            media,
            printers,
            selected,
            copies: 1,
        }
    }
}

fn linux_print(app: &mut App, ctx: &egui::Context, l: &mut LinuxPrint) -> bool {
    let mut keep = true;
    egui::Modal::new(egui::Id::new("drucken-linux"))
        .frame(modal_frame(ctx))
        .show(ctx, |ui| {
            ui.set_width(440.0);
            if head(ui, ctx, tr("Drucken"), None) {
                keep = false;
            }
            ui.add_space(8.0);
            if l.printers.is_empty() {
                ui.label(tr(
                    "Es ist kein Drucker eingerichtet. Bitte in den Systemeinstellungen einen Drucker hinzufügen.",
                ));
            } else {
                ui.label(RichText::new(tr("Drucker")).family(crate::fonts::semibold()));
                for (i, p) in l.printers.iter().enumerate() {
                    let text = if p.description.is_empty() {
                        p.name.clone()
                    } else {
                        p.description.clone()
                    };
                    if widgets::choice(ui, l.selected == i, &text).clicked() {
                        l.selected = i;
                    }
                }
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new(tr("Kopien")).family(crate::fonts::semibold()));
                    ui.add(egui::DragValue::new(&mut l.copies).range(1..=99));
                });
            }
            ui.add_space(14.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let ok = !l.printers.is_empty();
                if Button::new(tr("Drucken"))
                    .icon(Icon::Print)
                    .primary()
                    .enabled(ok)
                    .show(ui)
                    .clicked()
                {
                    let printer = &l.printers[l.selected].name;
                    match kd_print::print_with_lp(&l.pdf, printer, l.copies, &l.media) {
                        Ok(()) => app.toast(tr("An den Drucker geschickt.")),
                        Err(e) => app.toast(trf("Drucken hat nicht geklappt: {}", &[&e])),
                    }
                    keep = false;
                }
                if Button::new(tr("Abbrechen")).quiet().show(ui).clicked() {
                    keep = false;
                }
            });
        });
    keep
}

// ----------------------------------------------------------------------
// Lizenztexte
// ----------------------------------------------------------------------

fn license_window(app: &mut App, ctx: &egui::Context) {
    let Some((title, text)) = app.license else { return };
    let mut open = true;
    egui::Modal::new(egui::Id::new("lizenz"))
        .frame(modal_frame(ctx))
        .show(ctx, |ui| {
            ui.set_width(640.0);
            if head(ui, ctx, title, None) {
                open = false;
            }
            egui::ScrollArea::vertical().max_height(520.0).show(ui, |ui| {
                ui.label(RichText::new(text).monospace().size(12.5));
            });
        });
    if !open {
        app.license = None;
    }
}
