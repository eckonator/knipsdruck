// SPDX-License-Identifier: MIT OR Apache-2.0
//! Erkennt eine kleine Textzeile (1200 × 400 px, „Fertigungsverfahren Übung 2026“, Helvetica 64 px).
//! Ohne Engine auf dem Rechner wird der Test übersprungen.

#[test]
fn finds_word_with_box() {
    let Some(engine) = kd_ocr::engine() else {
        eprintln!("keine Texterkennung verfügbar, Test übersprungen");
        return;
    };
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/text.png");
    let img = image::open(path).unwrap().to_rgb8();
    let words = engine.recognize(&img).unwrap();
    let word = words
        .iter()
        .find(|w| w.text.trim_matches(|c: char| !c.is_alphanumeric()) == "Fertigungsverfahren")
        .unwrap_or_else(|| panic!("{}: Wort nicht gefunden in {words:?}", engine.name()));
    // Lage im Bild: etwa x 60–630, y 90–150
    assert!((40.0..90.0).contains(&word.x), "{word:?}");
    assert!((60.0..110.0).contains(&word.y), "{word:?}");
    assert!((500.0..620.0).contains(&word.w), "{word:?}");
    assert!((35.0..90.0).contains(&word.h), "{word:?}");
    assert!(words.iter().any(|w| w.text.contains("Übung")), "{words:?}");
}
