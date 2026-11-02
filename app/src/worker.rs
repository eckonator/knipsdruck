// SPDX-License-Identifier: MIT OR Apache-2.0
//! Arbeit im Hintergrund (rayon): Fotos laden und erkennen, Vorschauen rechnen, Ausgabe vorbereiten.
//! Ergebnisse kommen über einen Kanal zurück; die Oberfläche bleibt dabei bedienbar.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};

use eframe::egui;
use image::RgbImage;
use kd_core::page::{Layout, OutPage};
use kd_core::{DocSettings, PageSettings};

use crate::document::Photo;

pub enum Msg {
    Loaded {
        name: String,
        result: Result<(Arc<Photo>, Option<PageSettings>), String>,
    },
    Preview {
        id: u64,
        key: String,
        image: Arc<RgbImage>,
    },
    Progress {
        done: usize,
        total: usize,
    },
    Exported(Result<Exported, String>),
}

/// Wohin eine Ausgabe geht.
#[derive(Clone, Debug)]
pub enum Target {
    Save(PathBuf),
    Mail(PathBuf),
    Print(PathBuf),
}

impl Target {
    pub fn path(&self) -> &PathBuf {
        match self {
            Target::Save(p) | Target::Mail(p) | Target::Print(p) => p,
        }
    }
}

pub struct ExportJob {
    pub pages: Vec<(Arc<RgbImage>, PageSettings)>,
    pub doc: DocSettings,
    pub dpi: f32,
    pub jpeg: u8,
    pub ocr: bool,
    pub title: String,
    pub target: Target,
}

pub struct Exported {
    pub target: Target,
    /// Für das Drucken unter Windows (Bilder statt PDF)
    pub pages: Vec<OutPage>,
    /// Die Texterkennung war gewünscht, aber auf diesem Computer nicht verfügbar
    pub ocr_missing: bool,
}

pub struct Worker {
    tx: Sender<Msg>,
    rx: Receiver<Msg>,
    ctx: egui::Context,
    next_id: AtomicU64,
}

impl Worker {
    pub fn new(ctx: egui::Context) -> Self {
        let (tx, rx) = mpsc::channel();
        Worker {
            tx,
            rx,
            ctx,
            next_id: AtomicU64::new(1),
        }
    }

    pub fn messages(&self) -> Vec<Msg> {
        self.rx.try_iter().collect()
    }

    fn spawn(&self, job: impl FnOnce(&dyn Fn(Msg)) + Send + 'static) {
        let tx = self.tx.clone();
        let ctx = self.ctx.clone();
        rayon::spawn(move || {
            let send = |m: Msg| {
                let _ = tx.send(m);
                ctx.request_repaint();
            };
            job(&send);
        });
    }

    /// Foto dekodieren und das Blatt suchen. `preset`: Einstellungen aus „Zuletzt verwendet“.
    pub fn load(&self, name: String, bytes: Vec<u8>, preset: Option<(PageSettings, kd_core::Detection)>) {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        self.spawn(move |send| {
            let result = kd_core::decode(&bytes).map_err(|e| e.to_string()).map(|image| {
                let (auto, settings) = match preset {
                    Some((ps, det)) => (det, Some(ps)),
                    None => (kd_core::detect(&image), None),
                };
                let photo = Photo {
                    id,
                    name: name.clone(),
                    image: Arc::new(image),
                    original: Arc::new(bytes),
                    auto,
                };
                (Arc::new(photo), settings)
            });
            send(Msg::Loaded { name, result });
        });
    }

    /// Vorschau einer Seite mit langer Seite `long_px` (Rand und Farbe wie beim Druck).
    pub fn preview(
        &self,
        id: u64,
        key: String,
        photo: Arc<RgbImage>,
        ps: PageSettings,
        doc: DocSettings,
        long_px: f32,
    ) {
        self.spawn(move |send| {
            let image = render_preview(&photo, &ps, &doc, long_px);
            send(Msg::Preview {
                id,
                key,
                image: Arc::new(image),
            });
        });
    }

    pub fn export(&self, job: ExportJob) {
        self.spawn(move |send| send(Msg::Exported(run_export(job, send))));
    }
}

pub fn render_preview(photo: &RgbImage, ps: &PageSettings, doc: &DocSettings, long_px: f32) -> RgbImage {
    match doc.layout {
        Layout::Pages => {
            let (w, h) = kd_core::page::page_mm(ps, doc);
            kd_core::render_page(photo, ps, doc, long_px * 25.4 / w.max(h)).image
        }
        Layout::IdCard => kd_core::render_card(photo, ps, doc.mode, long_px * 25.4 / kd_core::page::CARD_MM.0).image,
    }
}

fn run_export(job: ExportJob, send: &dyn Fn(Msg)) -> Result<Exported, String> {
    let total = job.pages.len();
    send(Msg::Progress { done: 0, total });
    let refs: Vec<(&RgbImage, PageSettings)> = job.pages.iter().map(|(p, s)| (p.as_ref(), *s)).collect();
    let pages = kd_core::render_document(&refs, &job.doc, job.dpi, |done| {
        send(Msg::Progress {
            done: done.min(total),
            total,
        })
    });
    let engine = if job.ocr { kd_ocr::engine() } else { None };
    let ocr_missing = job.ocr && engine.is_none();
    let words: Vec<Vec<kd_pdf::Word>> = pages
        .iter()
        .map(|p| {
            let Some(engine) = &engine else { return Vec::new() };
            match engine.recognize(&p.image) {
                Ok(words) => words
                    .into_iter()
                    .map(|w| kd_pdf::Word {
                        text: w.text,
                        x: w.x,
                        y: w.y,
                        w: w.w,
                        h: w.h,
                    })
                    .collect(),
                Err(e) => {
                    log::warn!("Texterkennung fehlgeschlagen: {e}");
                    Vec::new()
                }
            }
        })
        .collect();
    let pdf_pages: Vec<kd_pdf::Page> = pages
        .iter()
        .zip(&words)
        .map(|(p, w)| kd_pdf::Page {
            image: &p.image,
            width_mm: p.width_mm,
            height_mm: p.height_mm,
            words: w,
        })
        .collect();
    let opts = kd_pdf::Options {
        title: job.title,
        quality: job.jpeg,
        gray: job.doc.mode != kd_core::enhance::ColorMode::Color,
    };
    let path = job.target.path();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    kd_pdf::write_file(path, &pdf_pages, &opts).map_err(|e| e.to_string())?;
    let keep = matches!(job.target, Target::Print(_));
    Ok(Exported {
        target: job.target,
        pages: if keep { pages } else { Vec::new() },
        ocr_missing,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Foto mit Text → Seite → Texterkennung → PDF mit unsichtbarer Textebene (nur wo eine Engine da ist).
    #[test]
    fn export_writes_searchable_pdf() {
        let photo = image::open(concat!(env!("CARGO_MANIFEST_DIR"), "/../crates/ocr/tests/text.png"))
            .unwrap()
            .into_rgb8();
        let det = kd_core::detect(&photo);
        let ps = PageSettings {
            quad: kd_core::Quad::whole(photo.width(), photo.height()),
            source: det.source,
            rotation: 0,
        };
        let dir = std::env::temp_dir().join(format!("kd-export-test-{}", std::process::id()));
        let path = dir.join("test.pdf");
        let job = ExportJob {
            pages: vec![(Arc::new(photo), ps)],
            doc: DocSettings::default(),
            dpi: 150.0,
            jpeg: 80,
            ocr: true,
            title: "Test".into(),
            target: Target::Save(path.clone()),
        };
        let done = run_export(job, &|_| {}).unwrap();
        let pdf = std::fs::read(&path).unwrap();
        assert!(pdf.starts_with(b"%PDF-1.4"));
        if done.ocr_missing {
            eprintln!("keine Texterkennung auf diesem System, Textebene nicht geprüft");
        } else {
            let text = String::from_utf8_lossy(&pdf);
            assert!(text.contains("Fertigungsverfahren"), "Textebene fehlt");
        }
        std::fs::remove_dir_all(dir).unwrap();
    }
}
