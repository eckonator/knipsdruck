// SPDX-License-Identifier: MIT OR Apache-2.0
//! Hauptfenster: Start, Seitenübersicht, Kopf- und Fußzeile, Ablauf von Laden bis Ausgabe.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use eframe::egui::{
    self, Color32, CornerRadius, Key, KeyboardShortcut, Modifiers, RichText, Sense, Stroke, StrokeKind, vec2,
};
use image::RgbImage;
use kd_core::page::Layout;
use kd_core::{PageSettings, Source};

use crate::dialogs::{self, Dialog};
use crate::document::{Document, Page, Photo};
use crate::i18n::{tr, trf};
use crate::icons::{self, Icon};
use crate::recent;
use crate::self_update::Progress;
use crate::settings::{PdfQuality, Settings, ThemeSetting};
use crate::theme;
use crate::widgets::{self, Button};
use crate::worker::{ExportJob, Exported, Msg, Target, Worker};

const CARD_W: f32 = 228.0;
const CARD_GAP: f32 = 28.0;
/// Lange Seite der Kartenvorschau in Pixeln (scharf auf Retina-Bildschirmen)
const CARD_PX: f32 = 560.0;

/// Fertig gerechnete Vorschau einer Seite.
pub struct Preview {
    pub key: String,
    pub tex: egui::TextureHandle,
    pub image: Arc<RgbImage>,
}

/// Was ausgegeben wird.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Print,
    Pdf,
    Mail,
}

pub struct Toast {
    pub text: String,
    pub until: Instant,
    /// Datei, die „Im Ordner zeigen“ anzeigt
    pub reveal: Option<PathBuf>,
}

struct ExportState {
    action: Action,
    done: usize,
    total: usize,
    /// Seiten-Ids der Ausgabe (für „Zuletzt verwendet“)
    scope: Option<u64>,
}

pub struct App {
    pub settings: Settings,
    pub doc: Document,
    pub layout: Layout,
    pub worker: Worker,
    pub previews: HashMap<u64, Preview>,
    pending: HashSet<(u64, String)>,
    /// Kleine Fotos je Seite (während der Aufbereitung und im Ecken-Editor als Platzhalter)
    pub thumbs: HashMap<u64, egui::TextureHandle>,
    /// Fotos in Arbeit, in der Reihenfolge ihres Eintreffens
    incoming: Vec<String>,
    arrived: HashMap<String, (Arc<Photo>, Option<PageSettings>)>,
    pub dialog: Dialog,
    pub license: Option<(&'static str, &'static str)>,
    export: Option<ExportState>,
    pub toasts: Vec<Toast>,
    pub recent: Vec<recent::Entry>,
    recent_tex: HashMap<u64, egui::TextureHandle>,
    update_rx: Option<std::sync::mpsc::Receiver<Result<Option<crate::update::NewVersion>, ()>>>,
    manual_update_check: bool,
    new_version: Option<crate::update::NewVersion>,
    update_progress: Option<Progress>,
    self_update: Option<std::sync::mpsc::Receiver<Progress>>,
    ask_update: bool,
    /// Druckauftrag, der im nächsten Bild auf dem Haupt-Thread startet (macOS verlangt das)
    print_next: Option<Exported>,
    dev: Option<crate::devtools::DevTools>,
}

impl App {
    pub fn new(cc: &eframe::CreationContext, settings: Settings, dev: crate::devtools::DevTools) -> Self {
        crate::fonts::install(&cc.egui_ctx);
        theme::apply(&cc.egui_ctx);
        cc.egui_ctx.options_mut(|o| o.zoom_with_keyboard = false);
        let mut app = App {
            worker: Worker::new(cc.egui_ctx.clone()),
            settings,
            doc: Document::default(),
            layout: Layout::Pages,
            previews: HashMap::new(),
            pending: HashSet::new(),
            thumbs: HashMap::new(),
            incoming: Vec::new(),
            arrived: HashMap::new(),
            dialog: Dialog::None,
            license: None,
            export: None,
            toasts: Vec::new(),
            recent: Vec::new(),
            recent_tex: HashMap::new(),
            update_rx: None,
            manual_update_check: false,
            new_version: None,
            update_progress: None,
            self_update: None,
            ask_update: false,
            print_next: None,
            dev: None,
        };
        let dev_files = dev.files.clone();
        let dev_active = dev.active();
        app.dev = Some(dev);
        app.reload_recent();
        if app.settings.searchable_pdf && !dev_active {
            kd_ocr::warm_up();
        }
        match app.settings.update_check {
            _ if dev_active => {}
            None => app.ask_update = true,
            Some(true) => app.update_rx = Some(crate::update::check_in_background(cc.egui_ctx.clone())),
            Some(false) => {}
        }
        if std::env::args().any(|a| a == crate::self_update::AFTER_UPDATE_FLAG) {
            app.toast(trf(
                "Knipsdruck wurde auf Version {} aktualisiert.",
                &[&env!("CARGO_PKG_VERSION")],
            ));
        }
        // Fotos als Argumente (Öffnen mit …)
        let mut files: Vec<PathBuf> = std::env::args_os()
            .skip(1)
            .map(PathBuf::from)
            .filter(|p| p.is_file())
            .collect();
        files.extend(dev_files);
        app.add_paths(files);
        if dev_active {
            app.reload_recent();
        }
        app
    }

    pub fn save_settings(&self) {
        self.settings.save();
    }

    pub fn toast(&mut self, text: impl Into<String>) {
        self.toasts.push(Toast {
            text: text.into(),
            until: Instant::now() + Duration::from_millis(3200),
            reveal: None,
        });
    }

    pub fn doc_settings(&self) -> kd_core::DocSettings {
        self.settings.doc(self.layout)
    }

    // ------------------------------------------------------------------
    // Fotos hereinholen
    // ------------------------------------------------------------------

    pub fn add_paths(&mut self, paths: Vec<PathBuf>) {
        let files: Vec<(String, Vec<u8>)> = paths
            .into_iter()
            .filter_map(|p| {
                let name = p.file_name()?.to_string_lossy().into_owned();
                std::fs::read(&p).ok().map(|b| (name, b))
            })
            .collect();
        self.add_files(files);
    }

    pub fn add_files(&mut self, files: Vec<(String, Vec<u8>)>) {
        if files.is_empty() {
            return;
        }
        self.doc.snapshot();
        for (name, bytes) in files {
            self.add_one(name, bytes, None);
        }
    }

    /// Ohne eigenen Rückgängig-Schritt (Handy-Sitzung, „Zuletzt verwendet“).
    pub fn add_one(&mut self, name: String, bytes: Vec<u8>, preset: Option<(PageSettings, kd_core::Detection)>) {
        let key = format!(
            "{}#{}",
            name,
            (self.incoming.len() + self.doc.pages.len()) as u64 + rand_suffix()
        );
        self.incoming.push(key.clone());
        self.worker.load(key, bytes, preset);
    }

    fn open_files_dialog(&mut self) {
        let picked = rfd::FileDialog::new()
            .set_title(tr("Fotos auswählen"))
            .add_filter(
                tr("Fotos"),
                &[
                    "jpg", "jpeg", "png", "webp", "heic", "heif", "tif", "tiff", "bmp", "gif",
                ],
            )
            .pick_files();
        if let Some(paths) = picked {
            self.add_paths(paths);
        }
    }

    fn paste(&mut self) {
        let Ok(mut cb) = arboard::Clipboard::new() else { return };
        // Dateien aus dem Finder/Explorer kommen als Pfade, Bildschirmfotos als Bilddaten
        if let Ok(text) = cb.get_text() {
            let paths: Vec<PathBuf> = text
                .lines()
                .map(|l| PathBuf::from(l.trim().trim_start_matches("file://")))
                .filter(|p| p.is_file())
                .collect();
            if !paths.is_empty() {
                self.add_paths(paths);
                return;
            }
        }
        if let Ok(img) = cb.get_image()
            && let Some(rgba) = image::RgbaImage::from_raw(img.width as u32, img.height as u32, img.bytes.into_owned())
        {
            let mut png = Vec::new();
            let rgb = image::DynamicImage::ImageRgba8(rgba).into_rgb8();
            if rgb
                .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
                .is_ok()
            {
                self.add_files(vec![(tr("Eingefügtes Bild.png").to_string(), png)]);
                return;
            }
        }
        self.toast(tr("In der Zwischenablage ist kein Foto."));
    }

    fn poll_worker(&mut self, ctx: &egui::Context) {
        for msg in self.worker.messages() {
            match msg {
                Msg::Loaded { name, result } => match result {
                    Ok(photo) => {
                        self.arrived.insert(name, photo);
                    }
                    Err(e) => {
                        self.incoming.retain(|n| *n != name);
                        let shown = name.split('#').next().unwrap_or(&name).to_string();
                        log::warn!("{shown}: {e}");
                        let text = if e.contains("HEIC") {
                            trf(
                                "„{}“ ist im HEIC-Format, das auf diesem System nicht gelesen werden kann.",
                                &[&shown],
                            )
                        } else {
                            trf("„{}“ ist kein lesbares Foto.", &[&shown])
                        };
                        self.toast(text);
                    }
                },
                Msg::Preview { id, key, image } => {
                    self.pending.remove(&(id, key.clone()));
                    let tex = ctx.load_texture(
                        format!("vorschau-{id}"),
                        color_image(&image),
                        egui::TextureOptions::LINEAR,
                    );
                    if key.ends_with("|gross") {
                        if let Dialog::Preview(p) = &mut self.dialog {
                            p.large = Some(Preview { key, tex, image });
                        }
                    } else {
                        self.previews.insert(id, Preview { key, tex, image });
                    }
                }
                Msg::Progress { done, total } => {
                    if let Some(e) = &mut self.export {
                        e.done = done;
                        e.total = total;
                    }
                }
                Msg::Exported(result) => self.finish_export(result),
            }
        }
        // Fertige Fotos in der Reihenfolge übernehmen, in der sie kamen
        while let Some(first) = self.incoming.first() {
            let Some((photo, preset)) = self.arrived.remove(first) else {
                break;
            };
            self.incoming.remove(0);
            let thumb = image::imageops::thumbnail(
                &*photo.image,
                400,
                400 * photo.image.height() / photo.image.width().max(1),
            );
            self.thumbs.insert(
                photo.id,
                ctx.load_texture(
                    format!("foto-{}", photo.id),
                    color_image(&thumb),
                    egui::TextureOptions::LINEAR,
                ),
            );
            let settings = preset.unwrap_or(PageSettings {
                quad: photo.auto.quad,
                source: photo.auto.source,
                rotation: 0,
            });
            self.doc.pages.push(Page { photo, settings });
        }
    }

    /// Fehlende Vorschauen in Auftrag geben.
    fn request_previews(&mut self) {
        let doc = self.doc_settings();
        for p in &self.doc.pages {
            let key = format!("{:?}|{:?}|{CARD_PX}", p.settings, doc);
            if self.previews.get(&p.id()).is_some_and(|v| v.key == key) || self.pending.contains(&(p.id(), key.clone()))
            {
                continue;
            }
            self.pending.insert((p.id(), key.clone()));
            self.worker
                .preview(p.id(), key, p.photo.image.clone(), p.settings, doc, CARD_PX);
        }
        let ids: HashSet<u64> = self.doc.pages.iter().map(Page::id).collect();
        self.previews.retain(|id, _| ids.contains(id));
    }

    /// Ist die Vorschau dieser Seite aktuell?
    pub fn preview_ready(&self, p: &Page) -> bool {
        let key = format!("{:?}|{:?}|{CARD_PX}", p.settings, self.doc_settings());
        self.previews.get(&p.id()).is_some_and(|v| v.key == key)
    }

    pub fn busy(&self) -> bool {
        !self.incoming.is_empty() || self.doc.pages.iter().any(|p| !self.preview_ready(p))
    }

    // ------------------------------------------------------------------
    // Seiten bearbeiten
    // ------------------------------------------------------------------

    pub fn rotate(&mut self, id: u64) {
        self.doc.snapshot();
        if let Some(p) = self.doc.page_mut(id) {
            p.settings.rotation = (p.settings.rotation + 90) % 360;
        }
    }

    pub fn remove(&mut self, id: u64) {
        self.doc.snapshot();
        self.doc.pages.retain(|p| p.id() != id);
        // Leeres Dokument verhält sich wie ein neues
        if self.doc.pages.is_empty() {
            self.layout = Layout::Pages;
        }
        self.toast(tr("Seite entfernt. ⌘Z holt sie zurück.").replace('⌘', cmd_key()));
    }

    pub fn set_quad(&mut self, id: u64, quad: kd_core::Quad, source: Source) {
        self.doc.snapshot();
        if let Some(p) = self.doc.page_mut(id) {
            p.settings.quad = quad;
            p.settings.source = source;
        }
    }

    fn undo(&mut self) {
        if self.doc.undo() {
            self.toast(tr("Rückgängig gemacht"));
        }
    }

    fn redo(&mut self) {
        if self.doc.redo() {
            self.toast(tr("Wiederhergestellt"));
        }
    }

    fn new_document(&mut self) {
        self.doc.snapshot();
        self.doc.pages.clear();
        self.doc.recent_id = None;
        self.layout = Layout::Pages;
        self.reload_recent();
        self.toast(tr("Neues Dokument. ⌘Z holt das vorige zurück.").replace('⌘', cmd_key()));
    }

    // ------------------------------------------------------------------
    // Ausgabe
    // ------------------------------------------------------------------

    /// Seiten einer Ausgabe: alle oder eine (bei der Ausweiskopie das ganze Blatt mit dieser Karte).
    pub fn output_pages(&self, scope: Option<u64>) -> Vec<Page> {
        let Some(id) = scope else { return self.doc.pages.clone() };
        let Some(i) = self.doc.index_of(id) else {
            return Vec::new();
        };
        match self.layout {
            Layout::Pages => vec![self.doc.pages[i].clone()],
            Layout::IdCard => {
                let start = i - i % 2;
                self.doc.pages[start..(start + 2).min(self.doc.pages.len())].to_vec()
            }
        }
    }

    pub fn start_output(&mut self, action: Action, scope: Option<u64>) {
        if self.doc.pages.is_empty() || self.export.is_some() {
            return;
        }
        match action {
            Action::Print => {
                let path = std::env::temp_dir().join(format!("knipsdruck-druck-{}.pdf", std::process::id()));
                self.run_export(Action::Print, scope, PdfQuality::Print, Target::Print(path));
            }
            Action::Pdf | Action::Mail => self.dialog = Dialog::Output(dialogs::OutputDialog::new(self, action, scope)),
        }
    }

    /// Dateiname-Vorschlag: Knipsdruck_2026-11-02_4-Seiten.pdf
    pub fn pdf_name(&self, scope: Option<u64>, quality: PdfQuality) -> String {
        let date = today();
        let what = match (scope, self.layout) {
            (Some(id), Layout::Pages) => format!("{}-{}", tr("Seite"), self.doc.index_of(id).unwrap_or(0) + 1),
            (Some(id), Layout::IdCard) => format!("{}-{}", tr("Blatt"), self.doc.index_of(id).unwrap_or(0) / 2 + 1),
            (None, Layout::Pages) => format!("{}-{}", self.doc.pages.len(), tr("Seiten")),
            (None, Layout::IdCard) => format!("{}-{}", self.doc.pages.len().div_ceil(2), tr("Seiten")),
        };
        let id = if self.layout == Layout::IdCard {
            format!("_{}", tr("Ausweis"))
        } else {
            String::new()
        };
        let small = if quality == PdfQuality::Email {
            format!("_{}", tr("klein"))
        } else {
            String::new()
        };
        format!("Knipsdruck_{date}_{what}{id}{small}.pdf")
    }

    pub fn run_export(&mut self, action: Action, scope: Option<u64>, quality: PdfQuality, target: Target) {
        let pages = self.output_pages(scope);
        if pages.is_empty() {
            return;
        }
        let job = ExportJob {
            pages: pages.iter().map(|p| (p.photo.image.clone(), p.settings)).collect(),
            doc: self.doc_settings(),
            dpi: quality.dpi(),
            jpeg: quality.jpeg(),
            ocr: self.settings.searchable_pdf && action != Action::Print,
            title: target
                .path()
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
            target,
        };
        self.export = Some(ExportState {
            action,
            done: 0,
            total: pages.len(),
            scope,
        });
        self.worker.export(job);
    }

    fn finish_export(&mut self, result: Result<Exported, String>) {
        let Some(state) = self.export.take() else { return };
        let done = match result {
            Ok(done) => done,
            Err(e) => {
                log::error!("Ausgabe fehlgeschlagen: {e}");
                self.toast(trf("Das hat nicht geklappt: {}", &[&e]));
                return;
            }
        };
        self.remember();
        if done.ocr_missing {
            self.toast(tr(
                "Auf diesem Computer ist keine Texterkennung verfügbar; das PDF ist nicht durchsuchbar.",
            ));
        }
        match (&done.target, state.action) {
            (Target::Save(path), _) => {
                let name = path.file_name().unwrap_or_default().to_string_lossy().into_owned();
                self.toasts.push(Toast {
                    text: trf("Gespeichert: {}", &[&name]),
                    until: Instant::now() + Duration::from_secs(6),
                    reveal: Some(path.clone()),
                });
            }
            (Target::Mail(path), _) => {
                let subject = trf("Dokument ({})", &[&pages_text(self.output_count(state.scope))]);
                if let Err(e) = kd_print::compose_email(path, &subject) {
                    log::info!("Kein E-Mail-Programm ({e}), zeige die Datei");
                    let _ = kd_print::reveal(path);
                    self.toasts.push(Toast {
                        text: tr("Kein E-Mail-Programm gefunden. Die Datei ist im Ordner markiert – ziehe sie in deine E-Mail.").into(),
                        until: Instant::now() + Duration::from_secs(8),
                        reveal: None,
                    });
                }
            }
            (Target::Print(_), _) => self.print_next = Some(done),
        }
    }

    fn output_count(&self, scope: Option<u64>) -> usize {
        let n = self.output_pages(scope).len();
        if self.layout == Layout::IdCard {
            n.div_ceil(2)
        } else {
            n
        }
    }

    fn run_print(&mut self, done: Exported) {
        let pdf = done.target.path().clone();
        if kd_print::system_dialog_available() {
            let pages: Vec<kd_print::Page> = done
                .pages
                .iter()
                .map(|p| kd_print::Page {
                    image: &p.image,
                    width_mm: p.width_mm,
                    height_mm: p.height_mm,
                })
                .collect();
            let job = kd_print::Job {
                name: "Knipsdruck",
                pages: &pages,
                pdf: &pdf,
            };
            match kd_print::print(&job) {
                Ok(kd_print::Outcome::Sent) => self.toast(tr("An den Drucker geschickt.")),
                Ok(kd_print::Outcome::Cancelled) => {}
                Err(e) => {
                    log::error!("Drucken: {e}");
                    self.toast(trf("Drucken hat nicht geklappt: {}", &[&e]));
                }
            }
        } else {
            let first = done
                .pages
                .first()
                .map(|p| (p.width_mm, p.height_mm))
                .unwrap_or((210.0, 297.0));
            self.dialog = Dialog::LinuxPrint(dialogs::LinuxPrint::new(pdf, kd_print::cups_media(first.0, first.1)));
        }
    }

    /// Unter „Zuletzt verwendet“ merken (nicht bei Ausweiskopien).
    fn remember(&mut self) {
        if !self.settings.recent
            || self.layout == Layout::IdCard
            || self.doc.pages.is_empty()
            || crate::settings::read_only()
        {
            return;
        }
        let first = &self.doc.pages[0];
        let preview = match self.previews.get(&first.id()) {
            Some(p) => p.image.clone(),
            None => Arc::new(crate::worker::render_preview(
                &first.photo.image,
                &first.settings,
                &self.doc_settings(),
                400.0,
            )),
        };
        match recent::save(self.doc.recent_id, &self.doc.pages, self.settings.color, &preview) {
            Ok(id) => {
                self.doc.recent_id = Some(id);
                self.recent_tex.remove(&id);
            }
            Err(e) => log::warn!("Zuletzt verwendet: {e}"),
        }
        if !self.settings.recent_hint_shown {
            self.settings.recent_hint_shown = true;
            self.save_settings();
            self.toasts.push(Toast {
                text: tr("Knipsdruck merkt sich deine letzten Dokumente 30 Tage lang, nur auf diesem Computer. Abschalten in den Einstellungen.").into(),
                until: Instant::now() + Duration::from_secs(7),
                reveal: None,
            });
        }
    }

    pub fn reload_recent(&mut self) {
        self.recent = if self.settings.recent {
            recent::list()
        } else {
            Vec::new()
        };
    }

    fn open_recent(&mut self, e: recent::Entry) {
        self.doc.snapshot();
        self.doc.pages.clear();
        self.layout = Layout::Pages;
        self.settings.color = e.color;
        for p in &e.pages {
            match recent::photo_bytes(&e, p) {
                Some(bytes) => self.add_one(p.name.clone(), bytes, Some((p.settings, p.detection()))),
                None => log::warn!("Foto {} fehlt", p.file),
            }
        }
        self.doc.recent_id = Some(e.id);
    }

    pub fn forget_recent(&mut self) {
        recent::clear();
        self.recent.clear();
        self.recent_tex.clear();
        self.doc.recent_id = None;
    }

    // ------------------------------------------------------------------
    // Oberfläche
    // ------------------------------------------------------------------

    fn shortcuts(&mut self, ctx: &egui::Context) {
        let cmd = |k| KeyboardShortcut::new(Modifiers::COMMAND, k);
        let cmd_shift = |k| KeyboardShortcut::new(Modifiers::COMMAND | Modifiers::SHIFT, k);
        let typing = ctx.memory(|m| m.focused().is_some());
        let modal = !matches!(self.dialog, Dialog::None);
        let (mut open, mut paste, mut print, mut pdf, mut undo, mut redo, mut phone, mut settings) =
            (false, false, false, false, false, false, false, false);
        ctx.input_mut(|i| {
            open = i.consume_shortcut(&cmd(Key::O));
            print = i.consume_shortcut(&cmd(Key::P));
            pdf = i.consume_shortcut(&cmd(Key::S));
            redo = i.consume_shortcut(&cmd_shift(Key::Z)) || i.consume_shortcut(&cmd(Key::Y));
            undo = !typing && i.consume_shortcut(&cmd(Key::Z));
            phone = i.consume_shortcut(&cmd_shift(Key::H));
            settings = i.consume_shortcut(&cmd(Key::Comma));
            paste = !typing && i.events.iter().any(|e| {
                matches!(e, egui::Event::Paste(_))
                    || matches!(e, egui::Event::Key { key: Key::V, pressed: true, modifiers, .. } if modifiers.command)
            });
        });
        if modal {
            return;
        }
        if open {
            self.open_files_dialog();
        }
        if paste {
            self.paste();
        }
        if print && !self.busy() {
            self.start_output(Action::Print, None);
        }
        if pdf && !self.busy() {
            self.start_output(Action::Pdf, None);
        }
        if undo {
            self.undo();
        }
        if redo {
            self.redo();
        }
        if phone {
            self.open_phone(ctx);
        }
        if settings {
            self.dialog = Dialog::Settings(Default::default());
        }
    }

    pub fn open_phone(&mut self, ctx: &egui::Context) {
        match crate::phone_ui::PhoneUi::start(ctx, &mut self.settings) {
            Ok(p) => {
                self.save_settings();
                self.dialog = Dialog::Phone(Box::new(p));
            }
            Err(e) => {
                log::error!("Handy-Empfang: {e}");
                self.toast(trf("Der Empfang vom Handy ließ sich nicht starten: {}", &[&e]));
            }
        }
    }

    fn dropped_files(&mut self, ctx: &egui::Context) -> bool {
        let (dropped, hovering) = ctx.input(|i| (i.raw.dropped_files.clone(), !i.raw.hovered_files.is_empty()));
        let mut files = Vec::new();
        for f in dropped {
            let name = f.path().file_name().unwrap_or_default().to_string_lossy().into_owned();
            match f.bytes() {
                Ok(bytes) => files.push((name, bytes)),
                Err(e) => log::warn!("{name}: {e}"),
            }
        }
        self.add_files(files);
        hovering
    }

    fn header(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.horizontal_centered(|ui| {
            ui.add_space(8.0);
            let (r, _) = ui.allocate_exact_size(vec2(32.0, 32.0), Sense::hover());
            icons::logo(ui.painter(), r, theme::accent(ctx), theme::ink(ctx), theme::panel(ctx));
            ui.label(
                RichText::new("Knipsdruck")
                    .size(19.0)
                    .family(crate::fonts::semibold())
                    .color(theme::ink(ctx)),
            );
            let has_doc = !self.doc.pages.is_empty() || !self.incoming.is_empty();
            let right_w = 280.0;
            let seg_w = 260.0;
            let space = (ui.available_width() - right_w - seg_w) / 2.0;
            if has_doc && space > 0.0 {
                ui.add_space(space);
                let farbe = tr("Farbe");
                let sw = tr("Schwarzweiß");
                let options: [(bool, &str, widgets::Deco); 2] = [
                    (true, farbe, Some(&widgets::color_dot)),
                    (false, sw, Some(&widgets::bw_dot)),
                ];
                if let Some(v) = widgets::segmented(ui, self.settings.color, &options, true) {
                    self.settings.color = v;
                    self.save_settings();
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(8.0);
                if widgets::icon_button(
                    ui,
                    Icon::Gear,
                    &format!("{} ({}{})", tr("Einstellungen"), cmd_key(), ","),
                )
                .clicked()
                {
                    self.dialog = Dialog::Settings(Default::default());
                }
                if has_doc {
                    if self.doc.can_undo()
                        && widgets::icon_button(ui, Icon::Undo, &format!("{} ({}Z)", tr("Rückgängig"), cmd_key()))
                            .clicked()
                    {
                        self.undo();
                    }
                    if widgets::icon_button(ui, Icon::NewDoc, tr("Neues Dokument")).clicked() {
                        self.new_document();
                    }
                    let phone = Button::new(tr("Vom Handy")).icon(Icon::Phone).show(ui);
                    if phone
                        .on_hover_text(format!("{} ({}⇧H)", tr("Vom Handy holen"), cmd_key()))
                        .clicked()
                    {
                        self.open_phone(ctx);
                    }
                } else if self.doc.can_undo()
                    && widgets::icon_button(ui, Icon::Undo, &format!("{} ({}Z)", tr("Rückgängig"), cmd_key())).clicked()
                {
                    self.undo();
                }
            });
        });
    }

    fn footer(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.horizontal_centered(|ui| {
            ui.add_space(12.0);
            let n = self.doc.pages.len();
            let busy = self.busy();
            let info = if busy {
                format!(
                    "{} · {}",
                    pages_text(n.max(self.incoming.len())),
                    tr("wird aufbereitet …")
                )
            } else {
                let mode = if self.settings.color {
                    tr("Farbe")
                } else {
                    tr("Schwarzweiß")
                };
                match self.layout {
                    Layout::Pages => format!("{} · {} · {mode}", pages_text(n), self.paper_text()),
                    Layout::IdCard => {
                        let sheets = n.div_ceil(2);
                        format!(
                            "{} · {} · {mode}",
                            trf("{} Fotos · Ausweiskopie auf {} Blatt", &[&n, &sheets]),
                            self.settings.paper.name()
                        )
                    }
                }
            };
            let link =
                ui.add(egui::Label::new(RichText::new(info).color(theme::muted(ctx)).size(15.0)).sense(Sense::click()));
            if link.on_hover_text(tr("Papierformat ändern")).clicked() {
                self.dialog = Dialog::Settings(crate::settings_ui::SettingsState::at(crate::settings_ui::Page::Paper));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(12.0);
                let ready = n > 0 && !busy && self.export.is_none();
                if Button::new(tr("Drucken"))
                    .icon(Icon::Print)
                    .primary()
                    .large()
                    .enabled(ready)
                    .show(ui)
                    .clicked()
                {
                    self.start_output(Action::Print, None);
                }
                if Button::new(tr("Als PDF speichern"))
                    .icon(Icon::Pdf)
                    .large()
                    .enabled(ready)
                    .show(ui)
                    .clicked()
                {
                    self.start_output(Action::Pdf, None);
                }
                if Button::new(tr("Per E-Mail"))
                    .icon(Icon::Mail)
                    .large()
                    .enabled(ready)
                    .show(ui)
                    .clicked()
                {
                    self.start_output(Action::Mail, None);
                }
            });
        });
    }

    pub fn paper_text(&self) -> String {
        use kd_core::page::Orientation;
        let o = match self.settings.orientation {
            _ if self.settings.paper.is_square() => "",
            Orientation::Auto => "",
            Orientation::Portrait => tr(" hoch"),
            Orientation::Landscape => tr(" quer"),
        };
        format!("{}{o}", self.settings.paper.name())
    }

    fn start_screen(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, hovering: bool) {
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.vertical_centered(|ui| {
                let top = ((ui.available_height() - 640.0) / 2.0).max(24.0);
                ui.add_space(top);
                let width = 620.0f32.min(ui.available_width() - 32.0);
                let frame = egui::Frame::new()
                    .fill(if hovering { theme::accent_soft(ctx) } else { theme::panel(ctx) })
                    .stroke(Stroke::new(2.0, if hovering { theme::accent(ctx) } else { theme::line(ctx) }))
                    .corner_radius(22)
                    .inner_margin(egui::Margin::symmetric(32, 44));
                frame.show(ui, |ui| {
                    ui.set_width(width - 64.0);
                    ui.vertical_centered(|ui| {
                        let (r, _) = ui.allocate_exact_size(vec2(96.0, 96.0), Sense::hover());
                        start_illustration(ui.painter(), r, ctx);
                        ui.add_space(12.0);
                        ui.label(RichText::new(tr("Fotos hierher ziehen")).size(26.0).family(crate::fonts::semibold()).color(theme::ink(ctx)));
                        ui.label(RichText::new(tr("Knipsdruck macht daraus saubere Seiten zum Ausdrucken.")).color(theme::muted(ctx)));
                        ui.add_space(22.0);
                        if Button::new(tr("Fotos auswählen …")).icon(Icon::Photo).primary().large().min_width(270.0).show(ui).clicked() {
                            self.open_files_dialog();
                        }
                        ui.add_space(4.0);
                        if Button::new(tr("Vom Handy holen")).icon(Icon::Phone).large().min_width(270.0).show(ui).clicked() {
                            self.open_phone(ctx);
                        }
                        ui.add_space(6.0);
                        ui.label(RichText::new(trf("oder {}V zum Einfügen", &[&cmd_key()])).small().color(theme::muted(ctx)));
                        ui.add_space(6.0);
                        if widgets::link(ui, tr("Ausweis oder Karte kopieren")).clicked() {
                            self.layout = Layout::IdCard;
                            self.open_files_dialog();
                        }
                        ui.add_space(18.0);
                        ui.horizontal(|ui| {
                            let tip = tr("Tipp: Blatt flach hinlegen und möglichst von oben fotografieren. Alle vier Ecken sollten im Bild sein.");
                            let w = 460.0f32.min(ui.available_width());
                            ui.add_space(((ui.available_width() - w) / 2.0).max(0.0));
                            let (r, _) = ui.allocate_exact_size(vec2(22.0, 22.0), Sense::hover());
                            icons::paint(ui.painter(), r, Icon::Bulb, theme::accent(ctx));
                            ui.add_sized(
                                [w - 30.0, 0.0],
                                egui::Label::new(RichText::new(tip).size(14.0).color(theme::muted(ctx))).wrap(),
                            );
                        });
                    });
                });
                self.recent_row(ui, ctx);
            });
        });
    }

    fn recent_row(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        if self.recent.is_empty() {
            return;
        }
        ui.add_space(34.0);
        ui.label(
            RichText::new(tr("ZULETZT VERWENDET"))
                .size(13.0)
                .family(crate::fonts::semibold())
                .color(theme::muted(ctx)),
        );
        ui.add_space(10.0);
        let item_w = 132.0;
        let n = self
            .recent
            .len()
            .min(((ui.available_width() + 18.0) / (item_w + 18.0)).floor().max(1.0) as usize);
        let row_w = n as f32 * (item_w + 18.0) - 18.0;
        let mut open = None;
        let mut delete = None;
        for chunk in self.recent.chunks(n) {
            ui.horizontal(|ui| {
                ui.add_space(((ui.available_width() - row_w) / 2.0).max(0.0));
                for e in chunk {
                    let tex = self.recent_tex.entry(e.id).or_insert_with(|| {
                        let img = recent::preview_path(e.id)
                            .and_then(|p| image::open(p).ok())
                            .map(|i| i.into_rgb8())
                            .unwrap_or_else(|| RgbImage::from_pixel(10, 13, image::Rgb([255; 3])));
                        ctx.load_texture(
                            format!("zuletzt-{}", e.id),
                            color_image(&img),
                            egui::TextureOptions::LINEAR,
                        )
                    });
                    let (rect, resp) = ui.allocate_exact_size(vec2(item_w, item_w * 1.25 + 46.0), Sense::click());
                    let img_rect = egui::Rect::from_min_size(rect.min, vec2(item_w, item_w * 1.25));
                    let lift = if resp.hovered() { -2.0 } else { 0.0 };
                    let img_rect = img_rect.translate(vec2(0.0, lift));
                    ui.painter()
                        .add(theme::shadow(ctx).as_shape(img_rect, CornerRadius::same(6)));
                    ui.painter().rect_filled(img_rect, 6, Color32::WHITE);
                    let size = tex.size_vec2();
                    let uv_h = ((img_rect.height() / img_rect.width()) * size.x / size.y).min(1.0);
                    let mut mesh = egui::Mesh::with_texture(tex.id());
                    mesh.add_rect_with_uv(
                        img_rect.shrink(1.0),
                        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, uv_h)),
                        Color32::WHITE,
                    );
                    ui.painter().add(mesh);
                    let text_y = img_rect.bottom() + 8.0 - lift;
                    ui.painter().text(
                        egui::pos2(rect.left(), text_y),
                        egui::Align2::LEFT_TOP,
                        pages_text(e.pages.len()),
                        egui::FontId::proportional(14.0),
                        theme::ink(ctx),
                    );
                    ui.painter().text(
                        egui::pos2(rect.left(), text_y + 19.0),
                        egui::Align2::LEFT_TOP,
                        when_text(e.saved),
                        egui::FontId::proportional(12.5),
                        theme::muted(ctx),
                    );
                    let del_rect =
                        egui::Rect::from_center_size(img_rect.right_top() + vec2(-4.0, 4.0), vec2(26.0, 26.0));
                    let del = ui.interact(del_rect, ui.id().with(("zuletzt-weg", e.id)), Sense::click());
                    if resp.hovered() || del.hovered() {
                        ui.painter().circle(
                            del_rect.center(),
                            13.0,
                            theme::panel(ctx),
                            Stroke::new(
                                1.0,
                                if del.hovered() {
                                    theme::danger(ctx)
                                } else {
                                    theme::line(ctx)
                                },
                            ),
                        );
                        icons::paint(
                            ui.painter(),
                            del_rect.shrink(7.0),
                            Icon::Close,
                            if del.hovered() {
                                theme::danger(ctx)
                            } else {
                                theme::muted(ctx)
                            },
                        );
                    }
                    if del.on_hover_text(tr("Aus der Liste entfernen")).clicked() {
                        delete = Some(e.id);
                    } else if resp
                        .on_hover_text(tr("Öffnen"))
                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                        .clicked()
                    {
                        open = Some(e.clone());
                    }
                    ui.add_space(18.0);
                }
            });
            ui.add_space(12.0);
        }
        if let Some(id) = delete {
            recent::remove(id);
            self.recent_tex.remove(&id);
            self.reload_recent();
        }
        if let Some(e) = open {
            self.open_recent(e);
        }
    }

    fn document_view(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.add_space(24.0);
            ui.vertical_centered(|ui| {
                let options: [(Layout, &str, widgets::Deco); 2] = [
                    (Layout::Pages, tr("Jedes Foto eine Seite"), None),
                    (Layout::IdCard, tr("Ausweiskopie"), None),
                ];
                if let Some(l) = widgets::segmented(ui, self.layout, &options, false) {
                    self.layout = l;
                }
            });
            if self.layout == Layout::IdCard {
                ui.add_space(14.0);
                self.id_hint(ui, ctx);
            }
            ui.add_space(26.0);
            self.grid(ui, ctx);
            ui.add_space(22.0);
            ui.vertical_centered(|ui| {
                ui.label(
                    RichText::new(tr(
                        "Zum Umsortieren eine Seite mit der Maus an eine andere Stelle ziehen.",
                    ))
                    .size(14.0)
                    .color(theme::muted(ctx)),
                );
            });
            ui.add_space(30.0);
        });
    }

    fn id_hint(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.vertical_centered(|ui| {
            egui::Frame::new()
                .fill(theme::accent_soft(ctx))
                .corner_radius(12)
                .inner_margin(egui::Margin::symmetric(18, 14))
                .show(ui, |ui| {
                    ui.set_max_width(820.0f32.min(ui.available_width() - 40.0));
                    ui.horizontal(|ui| {
                        let (r, _) = ui.allocate_exact_size(vec2(24.0, 24.0), Sense::hover());
                        icons::paint(ui.painter(), r, Icon::Card, theme::accent(ctx));
                        ui.add(egui::Label::new(
                            RichText::new(tr("Vorder- und Rückseite kommen in Originalgröße (85,6 × 54 mm) untereinander auf ein Blatt. Foto 1 + 2 → Blatt 1, Foto 3 + 4 → Blatt 2 …"))
                                .color(theme::ink(ctx)),
                        ).wrap());
                    });
                });
        });
    }

    fn grid(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let avail = ui.available_width() - 32.0;
        let cols = (((avail + CARD_GAP) / (CARD_W + CARD_GAP)).floor() as usize).max(1);
        let total = self.doc.pages.len() + self.incoming.len() + 1;
        let row_w = cols.min(total) as f32 * (CARD_W + CARD_GAP) - CARD_GAP;
        let mut actions: Vec<CardAction> = Vec::new();
        let ids: Vec<u64> = self.doc.pages.iter().map(Page::id).collect();
        let mut index = 0usize;
        while index < total {
            let end = (index + cols).min(total);
            ui.horizontal_top(|ui| {
                ui.add_space(((ui.available_width() - row_w) / 2.0).max(16.0));
                for slot in index..end {
                    if slot < ids.len() {
                        self.card(ui, ctx, slot, &mut actions);
                    } else if slot < ids.len() + self.incoming.len() {
                        loading_card(ui, ctx);
                    } else if add_card(ui, ctx).clicked() {
                        actions.push(CardAction::Add);
                    }
                    ui.add_space(CARD_GAP);
                }
            });
            ui.add_space(CARD_GAP);
            index = end;
        }
        for a in actions {
            match a {
                CardAction::Rotate(id) => self.rotate(id),
                CardAction::Corners(id) => {
                    self.dialog = Dialog::Corners(Box::new(crate::corners::CornerEditor::new(self, ctx, id)))
                }
                CardAction::Open(id) => self.dialog = Dialog::Preview(dialogs::PreviewState::new(id)),
                CardAction::Remove(id) => self.remove(id),
                CardAction::Output(action, id) => self.start_output(action, Some(id)),
                CardAction::Move(id, to) => {
                    self.doc.snapshot();
                    self.doc.move_page(id, to);
                }
                CardAction::Add => self.open_files_dialog(),
            }
        }
    }

    fn sheet_aspect(&self, p: &Page) -> f32 {
        let doc = self.doc_settings();
        let (w, h) = kd_core::page::page_mm(&p.settings, &doc);
        w / h
    }

    fn card(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, slot: usize, actions: &mut Vec<CardAction>) {
        let page = self.doc.pages[slot].clone();
        let id = page.id();
        let aspect = self.sheet_aspect(&page);
        let sheet_h = (CARD_W / aspect).min(CARD_W * 1.45);
        let sheet_w = sheet_h * aspect;
        ui.vertical(|ui| {
            ui.set_width(CARD_W);
            let drag_id = egui::Id::new(("seite", id));
            let (sheet_rect, sheet_resp) = ui.allocate_exact_size(vec2(CARD_W, CARD_W * 1.45), Sense::click());
            let sheet = egui::Rect::from_center_size(
                egui::pos2(sheet_rect.center().x, sheet_rect.bottom() - sheet_h / 2.0),
                vec2(sheet_w, sheet_h),
            );
            let lift = if sheet_resp.hovered() { -2.0 } else { 0.0 };
            let sheet = sheet.translate(vec2(0.0, lift));
            let radius = if self.layout == Layout::IdCard { 10 } else { 3 };
            let ready = self.preview_ready(&page);
            ui.painter()
                .add(theme::shadow(ctx).as_shape(sheet, CornerRadius::same(radius)));
            ui.painter().rect_filled(sheet, radius, Color32::WHITE);
            if let Some(pv) = self.previews.get(&id) {
                paint_texture(ui.painter(), &pv.tex, sheet, if ready { 1.0 } else { 0.55 });
            } else if let Some(t) = self.thumbs.get(&id) {
                paint_texture(ui.painter(), t, sheet, 0.45);
            }
            if !ready {
                spinner(ui, sheet.center(), ctx);
            }
            // Seitennummer
            let num_rect = egui::Rect::from_min_size(sheet.min + vec2(8.0, 8.0), vec2(28.0, 28.0));
            ui.painter().circle_filled(
                num_rect.center(),
                14.0,
                Color32::from_rgba_unmultiplied(26, 28, 34, 200),
            );
            widgets::centered_text(ui, num_rect, &(slot + 1).to_string(), 14.0, Color32::WHITE);
            if ready && page.needs_check() {
                let text = if page.settings.source == Source::WholePhoto {
                    tr("Bitte Ecken prüfen")
                } else {
                    tr("Bitte prüfen")
                };
                let g =
                    ui.painter()
                        .layout_no_wrap(text.to_string(), egui::FontId::proportional(13.5), theme::warn(ctx));
                let badge = egui::Rect::from_center_size(
                    egui::pos2(sheet.center().x, sheet.bottom() - 24.0),
                    vec2(g.size().x + 40.0, 28.0),
                );
                let b = ui.interact(badge, ui.id().with(("pruefen", id)), Sense::click());
                ui.painter()
                    .add(theme::shadow(ctx).as_shape(badge, CornerRadius::same(14)));
                ui.painter().rect_filled(badge, 14, theme::warn_bg(ctx));
                icons::paint(
                    ui.painter(),
                    egui::Rect::from_center_size(badge.left_center() + vec2(18.0, 0.0), vec2(16.0, 16.0)),
                    Icon::Warn,
                    theme::warn(ctx),
                );
                ui.painter().galley(
                    egui::pos2(badge.left() + 30.0, badge.center().y - g.size().y / 2.0),
                    g,
                    theme::warn(ctx),
                );
                if b.on_hover_text(tr("Ist das ganze Blatt erfasst? Klicken zum Anpassen."))
                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                    .clicked()
                {
                    actions.push(CardAction::Corners(id));
                }
            }
            // Ziehen zum Umsortieren
            let drag = ui.interact(sheet, drag_id, Sense::click_and_drag());
            if drag.drag_started() {
                egui::DragAndDrop::set_payload(ctx, id);
            }
            if drag.dragged() {
                ui.painter()
                    .rect_filled(sheet, radius, theme::desk(ctx).gamma_multiply(0.6));
            }
            if let Some(from) = drag.dnd_hover_payload::<u64>()
                && *from != id
            {
                let x = sheet_rect.left() - CARD_GAP / 2.0;
                ui.painter().line_segment(
                    [egui::pos2(x, sheet.top()), egui::pos2(x, sheet.bottom())],
                    Stroke::new(4.0, theme::accent(ctx)),
                );
            }
            if let Some(from) = drag.dnd_release_payload::<u64>()
                && *from != id
            {
                actions.push(CardAction::Move(*from, slot));
            }
            if drag.clicked() || sheet_resp.clicked() {
                actions.push(CardAction::Open(id));
            }
            drag.on_hover_cursor(egui::CursorIcon::Grab);
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                if Button::new(tr("Drehen"))
                    .icon(Icon::Rotate)
                    .small()
                    .show(ui)
                    .on_hover_text(tr("Drehen (R)"))
                    .clicked()
                {
                    actions.push(CardAction::Rotate(id));
                }
                if Button::new(tr("Ecken"))
                    .icon(Icon::Corners)
                    .small()
                    .show(ui)
                    .on_hover_text(tr("Ecken anpassen (E)"))
                    .clicked()
                {
                    actions.push(CardAction::Corners(id));
                }
                let more = Button::new("")
                    .icon(Icon::More)
                    .small()
                    .show(ui)
                    .on_hover_text(tr("Weitere Aktionen"));
                egui::Popup::menu(&more).width(280.0).show(|ui| {
                    page_menu(ui, ctx, self.layout, id, actions);
                });
            });
        });
    }

    fn update_bars(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        if let Some(rx) = &self.update_rx
            && let Ok(result) = rx.try_recv()
        {
            self.update_rx = None;
            let manual = std::mem::take(&mut self.manual_update_check);
            match result {
                Ok(Some(v)) => self.new_version = Some(v),
                Ok(None) if manual => self.toast(trf(
                    "Knipsdruck {} ist die neueste Version.",
                    &[&env!("CARGO_PKG_VERSION")],
                )),
                Err(()) if manual => self.toast(tr("GitHub ist nicht erreichbar, bitte später noch einmal versuchen.")),
                _ => {}
            }
        }
        if let Some(rx) = &self.self_update {
            while let Ok(p) = rx.try_recv() {
                if p == Progress::Restart {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
                self.update_progress = Some(p);
            }
            if matches!(self.update_progress, Some(Progress::Restart | Progress::Failed(_))) {
                self.self_update = None;
            }
        }
        if self.ask_update {
            egui::Panel::top("update-frage")
                .frame(egui::Frame::new().fill(theme::panel(ctx)).inner_margin(egui::Margin::symmetric(16, 10)))
                .show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new(tr("Beim Start nach neuen Versionen suchen?")).family(crate::fonts::semibold()));
                        ui.label(
                            RichText::new(tr("Dabei fragt Knipsdruck nur bei GitHub nach der neuesten Version – ohne Daten über deine Fotos."))
                                .color(theme::muted(ctx)),
                        );
                        if Button::new(tr("Ja, prüfen")).primary().show(ui).clicked() {
                            self.settings.update_check = Some(true);
                            self.ask_update = false;
                            self.save_settings();
                            self.update_rx = Some(crate::update::check_in_background(ctx.clone()));
                        }
                        if Button::new(tr("Nein")).quiet().show(ui).clicked() {
                            self.settings.update_check = Some(false);
                            self.ask_update = false;
                            self.save_settings();
                        }
                    });
                });
        }
        let Some(v) = self.new_version.clone() else { return };
        let mut close = false;
        let mut install = None;
        egui::Panel::top("update-hinweis")
            .frame(
                egui::Frame::new()
                    .fill(theme::accent_soft(ctx))
                    .inner_margin(egui::Margin::symmetric(16, 10)),
            )
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| match &self.update_progress {
                    Some(Progress::Downloading) => {
                        ui.spinner();
                        ui.label(
                            RichText::new(trf("Lade Knipsdruck {} …", &[&v.version])).family(crate::fonts::semibold()),
                        );
                    }
                    Some(Progress::Installing | Progress::Restart) => {
                        ui.spinner();
                        ui.label(
                            RichText::new(trf(
                                "Installiere Knipsdruck {} – die App startet gleich neu …",
                                &[&v.version],
                            ))
                            .family(crate::fonts::semibold()),
                        );
                    }
                    progress => {
                        ui.label(
                            RichText::new(trf(
                                "Knipsdruck {} ist verfügbar (installiert: {}).",
                                &[&v.version, &env!("CARGO_PKG_VERSION")],
                            ))
                            .family(crate::fonts::semibold()),
                        );
                        let failed = matches!(progress, Some(Progress::Failed(_)));
                        if let Some(asset) = &v.asset
                            && !failed
                            && Button::new(tr("Jetzt aktualisieren")).primary().show(ui).clicked()
                        {
                            install = Some(asset.clone());
                        }
                        if Button::new(tr("Download-Seite öffnen")).show(ui).clicked() {
                            ctx.open_url(egui::OpenUrl::new_tab(&v.url));
                        }
                        if Button::new(tr("Später")).quiet().show(ui).clicked() {
                            close = true;
                        }
                        if let Some(Progress::Failed(e)) = progress {
                            ui.label(RichText::new(trf("Update fehlgeschlagen: {}", &[e])).color(theme::danger(ctx)));
                        }
                    }
                });
            });
        if let Some(url) = install {
            self.settings.save();
            self.update_progress = Some(Progress::Downloading);
            self.self_update = Some(crate::self_update::start(url, ctx.clone()));
        }
        if close {
            self.new_version = None;
            self.update_progress = None;
        }
    }

    pub fn check_updates_now(&mut self, ctx: &egui::Context) {
        if self.update_rx.is_none() && self.self_update.is_none() {
            self.manual_update_check = true;
            self.update_rx = Some(crate::update::check_in_background(ctx.clone()));
        }
    }

    fn toasts_ui(&mut self, ctx: &egui::Context) {
        let now = Instant::now();
        self.toasts.retain(|t| t.until > now);
        let Some(t) = self.toasts.last() else { return };
        let text = t.text.clone();
        let reveal = t.reveal.clone();
        let mut do_reveal = false;
        egui::Area::new(egui::Id::new("meldung"))
            .anchor(egui::Align2::CENTER_BOTTOM, vec2(0.0, -96.0))
            .order(egui::Order::Tooltip)
            .interactable(reveal.is_some())
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(Color32::from_rgb(0x1a, 0x1c, 0x22))
                    .corner_radius(12)
                    .inner_margin(egui::Margin::symmetric(20, 12))
                    .shadow(theme::shadow(ctx))
                    .show(ui, |ui| {
                        ui.set_max_width(640.0);
                        ui.horizontal(|ui| {
                            ui.add(egui::Label::new(RichText::new(&text).color(Color32::WHITE).size(15.0)).wrap());
                            if reveal.is_some()
                                && ui
                                    .add(
                                        egui::Label::new(
                                            RichText::new(tr("Im Ordner zeigen"))
                                                .color(Color32::from_rgb(0x9d, 0xbd, 0xff))
                                                .underline(),
                                        )
                                        .sense(Sense::click()),
                                    )
                                    .clicked()
                            {
                                do_reveal = true;
                            }
                        });
                    });
            });
        if do_reveal && let Some(p) = reveal {
            let _ = kd_print::reveal(&p);
        }
        ctx.request_repaint_after(Duration::from_millis(250));
    }

    fn export_overlay(&self, ctx: &egui::Context) {
        let Some(e) = &self.export else { return };
        egui::Modal::new(egui::Id::new("ausgabe")).show(ctx, |ui| {
            ui.set_width(360.0);
            ui.vertical_centered(|ui| {
                ui.add_space(10.0);
                ui.spinner();
                ui.add_space(8.0);
                let what = match e.action {
                    Action::Print => tr("Druck wird vorbereitet …"),
                    Action::Pdf => tr("PDF wird erstellt …"),
                    Action::Mail => tr("E-Mail wird vorbereitet …"),
                };
                ui.label(RichText::new(what).family(crate::fonts::semibold()).size(16.0));
                ui.label(
                    RichText::new(trf("{} von {} Seiten", &[&e.done.min(e.total), &e.total])).color(theme::muted(ctx)),
                );
                ui.add_space(6.0);
                ui.add(egui::ProgressBar::new(e.done as f32 / e.total.max(1) as f32).desired_width(300.0));
                ui.add_space(10.0);
            });
        });
        ctx.request_repaint_after(Duration::from_millis(100));
    }
}

enum CardAction {
    Rotate(u64),
    Corners(u64),
    Open(u64),
    Remove(u64),
    Output(Action, u64),
    Move(u64, usize),
    Add,
}

fn page_menu(ui: &mut egui::Ui, ctx: &egui::Context, layout: Layout, id: u64, actions: &mut Vec<CardAction>) {
    ui.spacing_mut().button_padding = vec2(10.0, 8.0);
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
        if menu_item(ui, ctx, icon, text, false).clicked() {
            actions.push(CardAction::Output(action, id));
            ui.close();
        }
    }
    ui.separator();
    if menu_item(ui, ctx, Icon::Trash, tr("Seite entfernen"), true).clicked() {
        actions.push(CardAction::Remove(id));
        ui.close();
    }
}

pub fn menu_item(ui: &mut egui::Ui, ctx: &egui::Context, icon: Icon, text: &str, danger: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width().max(260.0), 38.0), Sense::click());
    if resp.hovered() {
        ui.painter().rect_filled(rect, 8, theme::desk(ctx));
    }
    let color = if danger { theme::danger(ctx) } else { theme::ink(ctx) };
    icons::paint(
        ui.painter(),
        egui::Rect::from_center_size(rect.left_center() + vec2(20.0, 0.0), vec2(18.0, 18.0)),
        icon,
        color,
    );
    ui.painter().text(
        rect.left_center() + vec2(40.0, 0.0),
        egui::Align2::LEFT_CENTER,
        text,
        egui::FontId::proportional(14.5),
        color,
    );
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

fn loading_card(ui: &mut egui::Ui, ctx: &egui::Context) {
    let (rect, _) = ui.allocate_exact_size(vec2(CARD_W, CARD_W * 1.45 + 48.0), Sense::hover());
    let sheet = egui::Rect::from_min_size(rect.min + vec2(0.0, 0.0), vec2(CARD_W, CARD_W * 1.414));
    ui.painter()
        .rect_filled(sheet, 3, theme::panel(ctx).gamma_multiply(0.6));
    spinner(ui, sheet.center(), ctx);
}

fn add_card(ui: &mut egui::Ui, ctx: &egui::Context) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(CARD_W, CARD_W * 1.414), Sense::click());
    let color = if resp.hovered() {
        theme::accent(ctx)
    } else {
        theme::line(ctx)
    };
    let r = rect.shrink(1.5);
    let corners = [r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom()];
    for i in 0..4 {
        ui.painter().extend(egui::Shape::dashed_line(
            &[corners[i], corners[(i + 1) % 4]],
            Stroke::new(2.5, color),
            9.0,
            6.0,
        ));
    }
    let text_color = if resp.hovered() {
        theme::accent(ctx)
    } else {
        theme::muted(ctx)
    };
    icons::paint(
        ui.painter(),
        egui::Rect::from_center_size(rect.center() - vec2(0.0, 16.0), vec2(40.0, 40.0)),
        Icon::Plus,
        text_color,
    );
    ui.painter().text(
        rect.center() + vec2(0.0, 22.0),
        egui::Align2::CENTER_CENTER,
        tr("Weitere Fotos"),
        egui::FontId::proportional(15.0),
        text_color,
    );
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

fn spinner(ui: &egui::Ui, center: egui::Pos2, ctx: &egui::Context) {
    let t = ctx.input(|i| i.time) as f32;
    let r = 20.0;
    ui.painter()
        .circle_stroke(center, r, Stroke::new(4.0, theme::accent(ctx).gamma_multiply(0.2)));
    let pts: Vec<egui::Pos2> = (0..=20)
        .map(|k| {
            let a = t * 6.0 + k as f32 / 20.0 * 1.6;
            center + vec2(a.cos(), a.sin()) * r
        })
        .collect();
    ui.painter()
        .add(egui::Shape::line(pts, Stroke::new(4.0, theme::accent(ctx))));
    ctx.request_repaint_after(Duration::from_millis(30));
}

/// Bild so in das Rechteck legen, dass es ganz zu sehen ist (Seitenverhältnis bleibt).
pub fn paint_texture(painter: &egui::Painter, tex: &egui::TextureHandle, rect: egui::Rect, alpha: f32) {
    let size = tex.size_vec2();
    let scale = (rect.width() / size.x).min(rect.height() / size.y);
    let r = egui::Rect::from_center_size(rect.center(), size * scale);
    painter.image(
        tex.id(),
        r,
        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
        Color32::WHITE.gamma_multiply(alpha),
    );
}

/// Startbild: schräges Foto wird zu geradem Blatt.
fn start_illustration(p: &egui::Painter, r: egui::Rect, ctx: &egui::Context) {
    let s = r.width() / 96.0;
    let o = r.min;
    let pt = |x: f32, y: f32| o + vec2(x, y) * s;
    let tilt = [pt(14.0, 26.0), pt(52.0, 19.0), pt(60.0, 72.0), pt(21.0, 79.0)];
    p.add(egui::Shape::convex_polygon(
        tilt.to_vec(),
        theme::desk(ctx),
        Stroke::new(2.5 * s, theme::muted(ctx)),
    ));
    let sheet = egui::Rect::from_min_size(pt(40.0, 14.0), vec2(42.0, 58.0) * s);
    p.rect(
        sheet,
        4.0 * s,
        theme::panel(ctx),
        Stroke::new(3.0 * s, theme::accent(ctx)),
        StrokeKind::Inside,
    );
    for (y, w) in [(27.0, 26.0), (35.0, 26.0), (43.0, 20.0)] {
        p.line_segment([pt(48.0, y), pt(48.0 + w, y)], Stroke::new(3.0 * s, theme::accent(ctx)));
    }
    p.line_segment(
        [pt(61.0, 56.0), pt(61.0, 82.0)],
        Stroke::new(3.5 * s, theme::accent(ctx)),
    );
    p.add(egui::Shape::line(
        vec![pt(50.0, 71.0), pt(61.0, 82.0), pt(72.0, 71.0)],
        Stroke::new(3.5 * s, theme::accent(ctx)),
    ));
}

pub fn color_image(img: &RgbImage) -> egui::ColorImage {
    egui::ColorImage::from_rgb([img.width() as usize, img.height() as usize], img.as_raw())
}

pub fn pages_text(n: usize) -> String {
    if n == 1 {
        tr("1 Seite").to_string()
    } else {
        trf("{} Seiten", &[&n])
    }
}

/// „⌘“ auf dem Mac, sonst „Strg+“.
pub fn cmd_key() -> &'static str {
    if cfg!(target_os = "macos") { "⌘" } else { "Strg+" }
}

fn rand_suffix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos() as u64)
        .unwrap_or(0)
}

fn today() -> String {
    let secs = recent::now() as i64 + local_offset_secs();
    let days = secs.div_euclid(86_400);
    let (y, m, d) = civil_from_days(days);
    format!("{y:04}-{m:02}-{d:02}")
}

/// Ortszeit grob über die Umgebung: ohne eigene Zeitzonen-Bibliothek reicht für Datum im Dateinamen UTC+Versatz
/// des Systems, den `date` liefert (fällt auf UTC zurück).
fn local_offset_secs() -> i64 {
    static OFFSET: std::sync::OnceLock<i64> = std::sync::OnceLock::new();
    *OFFSET.get_or_init(query_offset)
}

fn query_offset() -> i64 {
    #[cfg(unix)]
    {
        let out = std::process::Command::new("date").arg("+%z").output().ok();
        if let Some(o) = out {
            let s = String::from_utf8_lossy(&o.stdout);
            let s = s.trim();
            if s.len() == 5 {
                let sign = if s.starts_with('-') { -1 } else { 1 };
                let h: i64 = s[1..3].parse().unwrap_or(0);
                let m: i64 = s[3..5].parse().unwrap_or(0);
                return sign * (h * 3600 + m * 60);
            }
        }
    }
    0
}

fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

/// „Heute, 14:32“, „Gestern, 09:10“ oder „28.10.“
fn when_text(saved: u64) -> String {
    let off = local_offset_secs();
    let t = saved as i64 + off;
    let now = recent::now() as i64 + off;
    let (day, today) = (t.div_euclid(86_400), now.div_euclid(86_400));
    let hm = format!("{:02}:{:02}", t.rem_euclid(86_400) / 3600, t.rem_euclid(3600) / 60);
    match today - day {
        0 => trf("Heute, {}", &[&hm]),
        1 => trf("Gestern, {}", &[&hm]),
        _ => {
            let (_, m, d) = civil_from_days(day);
            if crate::i18n::lang() == crate::i18n::Lang::En {
                format!("{m}/{d}")
            } else {
                format!("{d}.{m}.")
            }
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        ctx.set_theme(match self.settings.theme {
            ThemeSetting::System => egui::ThemePreference::System,
            ThemeSetting::Light => egui::ThemePreference::Light,
            ThemeSetting::Dark => egui::ThemePreference::Dark,
        });
        if let Some(done) = self.print_next.take() {
            self.run_print(done);
        }
        self.poll_worker(&ctx);
        self.request_previews();
        self.shortcuts(&ctx);
        let hovering = self.dropped_files(&ctx);

        egui::Panel::top("kopf")
            .exact_size(64.0)
            .frame(
                egui::Frame::new()
                    .fill(theme::panel(&ctx))
                    .stroke(Stroke::new(1.0, theme::line(&ctx))),
            )
            .show(ui, |ui| self.header(ui, &ctx));
        self.update_bars(ui, &ctx);
        let has_doc = !self.doc.pages.is_empty() || !self.incoming.is_empty();
        if has_doc {
            egui::Panel::bottom("fuss")
                .exact_size(84.0)
                .frame(
                    egui::Frame::new()
                        .fill(theme::panel(&ctx))
                        .stroke(Stroke::new(1.0, theme::line(&ctx))),
                )
                .show(ui, |ui| self.footer(ui, &ctx));
        }
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(theme::desk(&ctx)))
            .show(ui, |ui| {
                if has_doc {
                    self.document_view(ui, &ctx);
                } else {
                    self.start_screen(ui, &ctx, hovering);
                }
            });
        dialogs::show(self, &ctx);
        self.export_overlay(&ctx);
        self.toasts_ui(&ctx);
        if let Some(mut dev) = self.dev.take() {
            dev.on_frame(self, &ctx);
            self.dev = Some(dev);
        }
    }
}
