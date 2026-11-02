// SPDX-License-Identifier: MIT OR Apache-2.0
//! Bildverarbeitung von Knipsdruck: Foto laden, Blatt finden, entzerren, aufbereiten und zu Seiten setzen.
//! Ohne Oberfläche und ohne Plattform-Code; jeder Schritt ist eine reine Funktion.

pub mod detect;
pub mod enhance;
pub mod geom;
pub mod load;
pub mod page;

use serde::{Deserialize, Serialize};

pub use detect::{Detection, detect};
pub use load::{decode, load_path};
pub use page::{DocSettings, OutPage, render_card, render_document, render_page};

/// Punkt im Foto (Pixel, Ursprung oben links).
pub type Point = [f32; 2];

/// Vier Ecken des Blatts im Originalfoto: oben links, oben rechts, unten rechts, unten links.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Quad(pub [Point; 4]);

impl Quad {
    /// Das ganze Foto.
    pub fn whole(width: u32, height: u32) -> Quad {
        let (w, h) = (width as f32, height as f32);
        Quad([[0.0, 0.0], [w, 0.0], [w, h], [0.0, h]])
    }

    /// Mittlere Breite und Höhe (oben/unten bzw. links/rechts gemittelt).
    pub fn size(&self) -> (f32, f32) {
        let [a, b, c, d] = self.0;
        let w = (geom::dist(a, b) + geom::dist(d, c)) / 2.0;
        let h = (geom::dist(a, d) + geom::dist(b, c)) / 2.0;
        (w, h)
    }

    pub fn scaled(&self, f: f32) -> Quad {
        Quad(self.0.map(|[x, y]| [x * f, y * f]))
    }
}

/// Woher die Ecken einer Seite stammen.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Source {
    #[default]
    Auto,
    Manual,
    WholePhoto,
}

/// Alles, was eine einzelne Seite bestimmt (das Dokument steuert Papier, Farbe usw. über `DocSettings`).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct PageSettings {
    pub quad: Quad,
    pub source: Source,
    /// Drehung im Uhrzeigersinn: 0, 90, 180 oder 270
    pub rotation: u16,
}

impl PageSettings {
    pub fn quarter_turned(&self) -> bool {
        !self.rotation.is_multiple_of(180)
    }
}
