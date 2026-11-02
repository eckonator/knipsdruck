// SPDX-License-Identifier: MIT OR Apache-2.0
//! Papierformate und ihre CUPS-Namen.

/// Ein bekanntes Papierformat, Maße hochkant in Millimetern.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Paper {
    pub name: &'static str,
    pub width_mm: f32,
    pub height_mm: f32,
}

/// Die Formate, die Drucker unter festem Namen kennen.
pub(crate) const PAPERS: [Paper; 7] = [
    Paper {
        name: "A3",
        width_mm: 297.0,
        height_mm: 420.0,
    },
    Paper {
        name: "A4",
        width_mm: 210.0,
        height_mm: 297.0,
    },
    Paper {
        name: "A5",
        width_mm: 148.0,
        height_mm: 210.0,
    },
    Paper {
        name: "A6",
        width_mm: 105.0,
        height_mm: 148.0,
    },
    Paper {
        name: "B5",
        width_mm: 176.0,
        height_mm: 250.0,
    },
    Paper {
        name: "Letter",
        width_mm: 215.9,
        height_mm: 279.4,
    },
    Paper {
        name: "Legal",
        width_mm: 215.9,
        height_mm: 355.6,
    },
];

/// Toleranz beim Wiedererkennen eines Formats.
const TOLERANCE_MM: f32 = 1.0;

/// Das bekannte Format dieser Größe, egal ob hoch oder quer.
pub(crate) fn known_paper(width_mm: f32, height_mm: f32) -> Option<&'static Paper> {
    let (short, long) = if width_mm <= height_mm {
        (width_mm, height_mm)
    } else {
        (height_mm, width_mm)
    };
    PAPERS
        .iter()
        .find(|p| (p.width_mm - short).abs() <= TOLERANCE_MM && (p.height_mm - long).abs() <= TOLERANCE_MM)
}

/// CUPS-Medienname für eine Seitengröße: `A4`, `Letter`, … oder `Custom.{b}x{h}mm`.
pub fn cups_media(width_mm: f32, height_mm: f32) -> String {
    match known_paper(width_mm, height_mm) {
        Some(paper) => paper.name.to_string(),
        None => format!("Custom.{}x{}mm", width_mm.round() as i64, height_mm.round() as i64),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bekannte_formate() {
        assert_eq!(cups_media(210.0, 297.0), "A4");
        assert_eq!(cups_media(297.0, 210.0), "A4");
        assert_eq!(cups_media(209.3, 297.8), "A4");
        assert_eq!(cups_media(297.0, 420.0), "A3");
        assert_eq!(cups_media(148.0, 210.0), "A5");
        assert_eq!(cups_media(148.0, 105.0), "A6");
        assert_eq!(cups_media(176.0, 250.0), "B5");
        assert_eq!(cups_media(215.9, 279.4), "Letter");
        assert_eq!(cups_media(279.4, 215.9), "Letter");
        assert_eq!(cups_media(216.0, 356.0), "Legal");
    }

    #[test]
    fn eigene_formate() {
        assert_eq!(cups_media(150.0, 100.0), "Custom.150x100mm");
        assert_eq!(cups_media(211.5, 297.0), "Custom.212x297mm");
        assert_eq!(cups_media(85.6, 53.98), "Custom.86x54mm");
    }

    #[test]
    fn format_ohne_ausrichtung() {
        let paper = known_paper(297.0, 210.0).unwrap();
        assert_eq!((paper.width_mm, paper.height_mm), (210.0, 297.0));
        assert!(known_paper(100.0, 100.0).is_none());
    }
}
