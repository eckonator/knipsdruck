// SPDX-License-Identifier: MIT OR Apache-2.0
//! Das Dokument: Seiten aus Fotos, nicht-destruktiv (Foto bleibt, nur die Einstellungen ändern sich),
//! mit Rückgängig und Wiederholen.

use std::sync::Arc;

use image::RgbImage;
use kd_core::{Detection, PageSettings};

/// Ein geladenes Foto mit dem, was die Erkennung gefunden hat.
pub struct Photo {
    pub id: u64,
    pub name: String,
    pub image: Arc<RgbImage>,
    /// Die Originaldatei (für „Zuletzt verwendet“)
    pub original: Arc<Vec<u8>>,
    pub auto: Detection,
}

#[derive(Clone)]
pub struct Page {
    pub photo: Arc<Photo>,
    pub settings: PageSettings,
}

impl Page {
    pub fn id(&self) -> u64 {
        self.photo.id
    }

    /// Erkennung unsicher und Ecken nicht von Hand gesetzt
    pub fn needs_check(&self) -> bool {
        self.settings.source == kd_core::Source::Auto && self.photo.auto.confidence < 60
            || self.settings.source == kd_core::Source::WholePhoto && self.photo.auto.confidence == 0
    }
}

#[derive(Default)]
pub struct Document {
    pub pages: Vec<Page>,
    undo: Vec<Vec<Page>>,
    redo: Vec<Vec<Page>>,
    /// Eintrag unter „Zuletzt verwendet“, wenn das Dokument schon einmal ausgegeben wurde
    pub recent_id: Option<u64>,
}

const UNDO_DEPTH: usize = 50;

impl Document {
    /// Vor jeder Änderung aufrufen.
    pub fn snapshot(&mut self) {
        self.undo.push(self.pages.clone());
        if self.undo.len() > UNDO_DEPTH {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn undo(&mut self) -> bool {
        let Some(prev) = self.undo.pop() else { return false };
        self.redo.push(std::mem::replace(&mut self.pages, prev));
        true
    }

    pub fn redo(&mut self) -> bool {
        let Some(next) = self.redo.pop() else { return false };
        self.undo.push(std::mem::replace(&mut self.pages, next));
        true
    }

    pub fn index_of(&self, id: u64) -> Option<usize> {
        self.pages.iter().position(|p| p.id() == id)
    }

    pub fn page_mut(&mut self, id: u64) -> Option<&mut Page> {
        self.pages.iter_mut().find(|p| p.id() == id)
    }

    /// Verschiebt die Seite `id` vor die Seite an Position `to` (oder ans Ende).
    pub fn move_page(&mut self, id: u64, to: usize) {
        let Some(from) = self.index_of(id) else { return };
        let page = self.pages.remove(from);
        let to = if to > from { to - 1 } else { to };
        self.pages.insert(to.min(self.pages.len()), page);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kd_core::{Quad, Source};

    fn page(id: u64) -> Page {
        let image = Arc::new(RgbImage::new(4, 4));
        let auto = Detection {
            quad: Quad::whole(4, 4),
            confidence: 90,
            source: Source::Auto,
        };
        Page {
            photo: Arc::new(Photo {
                id,
                name: format!("{id}"),
                image,
                original: Arc::new(Vec::new()),
                auto,
            }),
            settings: PageSettings {
                quad: auto.quad,
                source: Source::Auto,
                rotation: 0,
            },
        }
    }

    #[test]
    fn undo_and_redo_restore_pages() {
        let mut d = Document::default();
        d.snapshot();
        d.pages = vec![page(1), page(2), page(3)];
        d.snapshot();
        d.move_page(3, 0);
        assert_eq!(d.pages.iter().map(Page::id).collect::<Vec<_>>(), [3, 1, 2]);
        d.snapshot();
        d.page_mut(1).unwrap().settings.rotation = 90;
        assert!(d.undo());
        assert_eq!(d.pages[1].settings.rotation, 0);
        assert!(d.undo());
        assert_eq!(d.pages.iter().map(Page::id).collect::<Vec<_>>(), [1, 2, 3]);
        assert!(d.redo());
        assert_eq!(d.pages[0].id(), 3);
    }

    #[test]
    fn move_to_end_and_back() {
        let mut d = Document {
            pages: vec![page(1), page(2), page(3)],
            ..Default::default()
        };
        d.move_page(1, 3);
        assert_eq!(d.pages.iter().map(Page::id).collect::<Vec<_>>(), [2, 3, 1]);
        d.move_page(1, 0);
        assert_eq!(d.pages.iter().map(Page::id).collect::<Vec<_>>(), [1, 2, 3]);
    }
}
