// SPDX-License-Identifier: MIT OR Apache-2.0
//! Hilfen für Entwicklung und Bildschirmfotos (README), über Umgebungsvariablen:
//!
//! - `KD_FOTOS=a.jpg|b.jpg` lädt diese Fotos beim Start
//! - `KD_DIALOG=einstellungen|handy|ecken|vorschau|pdf|ausweis` öffnet das nach dem Laden
//! - `KD_SCREENSHOT=/pfad/bild.png` speichert ein Bild des Fensters, sobald alles gerechnet ist, und beendet die App
//! - `KD_WARTEN=8` wartet so viele Sekunden mehr bis zum Bildschirmfoto
//! - `KD_SIZE=1440x900`, `KD_LANG=de|en`, `KD_THEME=hell|dunkel`
//! - Bei diesen Schaltern wird nichts gespeichert (Einstellungen, „Zuletzt verwendet“).

use std::path::PathBuf;

use eframe::egui::{self, Event, ViewportCommand};

use crate::app::App;

#[derive(Default)]
pub struct DevTools {
    screenshot: Option<PathBuf>,
    pub files: Vec<PathBuf>,
    dialog: Option<String>,
    pub lang: Option<String>,
    pub theme: Option<String>,
    wait: f32,
    frame: u64,
    ready_since: Option<u64>,
    started: Option<std::time::Instant>,
    requested: bool,
    opened: bool,
}

impl DevTools {
    pub fn from_env() -> Self {
        let var = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
        DevTools {
            screenshot: var("KD_SCREENSHOT").map(PathBuf::from),
            files: var("KD_FOTOS")
                .map(|f| f.split('|').map(PathBuf::from).collect())
                .unwrap_or_default(),
            dialog: var("KD_DIALOG"),
            lang: var("KD_LANG"),
            theme: var("KD_THEME"),
            wait: var("KD_WARTEN").and_then(|w| w.parse().ok()).unwrap_or(0.0),
            ..Default::default()
        }
    }

    /// Wird etwas gesteuert? Dann bleibt alles Gespeicherte unangetastet.
    pub fn active(&self) -> bool {
        self.screenshot.is_some() || !self.files.is_empty() || self.dialog.is_some()
    }

    pub fn window_size() -> Option<[f32; 2]> {
        let s = std::env::var("KD_SIZE").ok()?;
        let (w, h) = s.split_once('x')?;
        Some([w.parse().ok()?, h.parse().ok()?])
    }

    pub fn on_frame(&mut self, app: &mut App, ctx: &egui::Context) {
        if !self.active() {
            return;
        }
        self.frame += 1;
        if !self.opened {
            ctx.request_repaint();
        }
        let started = *self.started.get_or_insert_with(std::time::Instant::now);
        let ready = !app.busy() && self.frame > 10;
        if ready && !self.opened {
            self.opened = true;
            if let Some(d) = self.dialog.clone() {
                open_dialog(app, ctx, &d);
            }
        }
        if ready && self.opened {
            self.ready_since.get_or_insert(self.frame);
        }
        let Some(path) = self.screenshot.clone() else { return };
        ctx.request_repaint();
        let settled = self.ready_since.is_some_and(|f| self.frame > f + 40);
        if !self.requested && settled && started.elapsed().as_secs_f32() > 2.0 + self.wait {
            ctx.send_viewport_cmd(ViewportCommand::Screenshot(Default::default()));
            self.requested = true;
        }
        let shot = ctx.input(|i| {
            i.events.iter().find_map(|e| match e {
                Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });
        if let Some(image) = shot {
            let [w, h] = image.size;
            let rgba: Vec<u8> = image.pixels.iter().flat_map(|c| c.to_array()).collect();
            match image::RgbaImage::from_raw(w as u32, h as u32, rgba).map(|i| i.save(&path)) {
                Some(Ok(())) => log::info!("Bildschirmfoto gespeichert: {}", path.display()),
                other => log::error!("Bildschirmfoto nicht gespeichert: {other:?}"),
            }
            ctx.send_viewport_cmd(ViewportCommand::Close);
        }
    }
}

fn open_dialog(app: &mut App, ctx: &egui::Context, which: &str) {
    use crate::dialogs::Dialog;
    let first = app.doc.pages.first().map(|p| p.id());
    match which {
        "einstellungen" => app.dialog = Dialog::Settings(Default::default()),
        "handy" => app.open_phone(ctx),
        "ecken" => {
            if let Some(id) = app
                .doc
                .pages
                .iter()
                .find(|p| p.needs_check())
                .or(app.doc.pages.first())
                .map(|p| p.id())
            {
                app.dialog = Dialog::Corners(Box::new(crate::corners::CornerEditor::new(app, ctx, id)));
            }
        }
        "vorschau" => {
            if let Some(id) = first {
                app.dialog = Dialog::Preview(crate::dialogs::PreviewState::new(id));
            }
        }
        "pdf" => app.start_output(crate::app::Action::Pdf, None),
        "mail" => app.start_output(crate::app::Action::Mail, None),
        "ausweis" => app.layout = kd_core::page::Layout::IdCard,
        other => log::warn!("KD_DIALOG={other} unbekannt"),
    }
}
