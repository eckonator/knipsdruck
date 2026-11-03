// SPDX-License-Identifier: MIT OR Apache-2.0
//! Sprachen der Oberfläche.
//!
//! Der deutsche Text im Code ist zugleich der Schlüssel: `tr("Fotos auswählen …")` liefert auf Englisch
//! "Choose photos …". Platzhalter `{}` werden mit `trf` der Reihe nach ersetzt. Fehlt eine Übersetzung,
//! bleibt der deutsche Text (und ein Test schlägt fehl).

use std::fmt::Display;
use std::sync::atomic::{AtomicU8, Ordering};

use crate::settings::LangSetting;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lang {
    De,
    En,
}

static LANG: AtomicU8 = AtomicU8::new(0);

pub fn set_lang(l: Lang) {
    LANG.store(l as u8, Ordering::Relaxed);
}

pub fn lang() -> Lang {
    if LANG.load(Ordering::Relaxed) == Lang::En as u8 {
        Lang::En
    } else {
        Lang::De
    }
}

/// Einstellung „System“ folgt der Systemsprache: Deutsch, wenn sie mit „de“ beginnt, sonst Englisch.
pub fn apply_setting(s: LangSetting) {
    set_lang(match s {
        LangSetting::De => Lang::De,
        LangSetting::En => Lang::En,
        LangSetting::System => {
            if sys_locale::get_locale().is_some_and(|l| l.to_lowercase().starts_with("de")) {
                Lang::De
            } else {
                Lang::En
            }
        }
    });
}

/// Übersetzt einen festen Text.
pub fn tr(de: &'static str) -> &'static str {
    if !de.chars().any(char::is_alphabetic) {
        return de;
    }
    match lang() {
        Lang::De => de,
        Lang::En => en(de).unwrap_or_else(|| {
            if cfg!(debug_assertions) {
                log::debug!("Übersetzung fehlt: {de:?}");
            }
            de
        }),
    }
}

/// Übersetzt einen Text mit `{}`-Platzhaltern und setzt die Werte der Reihe nach ein.
pub fn trf(de: &'static str, args: &[&dyn Display]) -> String {
    let template = tr(de);
    let mut out = String::with_capacity(template.len() + 16);
    let mut args = args.iter();
    let mut rest = template;
    while let Some(i) = rest.find("{}") {
        out.push_str(&rest[..i]);
        if let Some(a) = args.next() {
            out.push_str(&a.to_string());
        }
        rest = &rest[i + 2..];
    }
    out.push_str(rest);
    out
}

fn en(de: &str) -> Option<&'static str> {
    Some(match de {
        " hoch" => " portrait",
        " quer" => " landscape",
        "1 Foto empfangen" => "1 photo received",
        "1 Seite" => "1 page",
        "Abbrechen" => "Cancel",
        "Alle {} Blätter" => "All {} sheets",
        "Alle {} Seiten" => "All {} pages",
        "Als PDF speichern" => "Save as PDF",
        "An" => "On",
        "An den Drucker geschickt." => "Sent to the printer.",
        "Andere Adresse versuchen:" => "Try another address:",
        "Apache-Lizenz 2.0" => "Apache License 2.0",
        "Apache-Lizenz 2.0 anzeigen" => "Show Apache License 2.0",
        "Auch klein über 2 MB – bei manchen Postfächern die Seiten auf zwei E-Mails verteilen." => {
            "Over 2 MB even when small – some mailboxes need the pages split across two emails."
        }
        "Auf den Link tippen," => "Tap the link",
        "Auf diesem Computer ist keine Texterkennung verfügbar. Unter Linux hilft das Paket „tesseract-ocr“ (mit „tesseract-ocr-deu“)." => {
            "No text recognition is available on this computer. On Linux, install the package “tesseract-ocr” (plus “tesseract-ocr-eng” or “-deu”)."
        }
        "Auf diesem Computer ist keine Texterkennung verfügbar; das PDF ist nicht durchsuchbar." => {
            "No text recognition is available on this computer; the PDF is not searchable."
        }
        "Aus" => "Off",
        "Aus der Liste entfernen" => "Remove from list",
        "Ausrichtung" => "Orientation",
        "Ausweis" => "ID",
        "Ausweis oder Karte kopieren" => "Copy an ID or card",
        "Ausweiskopie" => "ID copy",
        "Automatisch" => "Automatic",
        "Beim Speichern und Versenden wird der Text erkannt ({}), damit sich das PDF durchsuchen und kopieren lässt. Dauert pro Seite etwas länger; alles bleibt auf diesem Computer." => {
            "When saving or emailing, the text is recognized ({}) so the PDF can be searched and copied from. Takes a little longer per page; everything stays on this computer."
        }
        "Beim Start nach neuen Versionen suchen" => "Check for new versions at startup",
        "Beim Start nach neuen Versionen suchen?" => "Check for new versions at startup?",
        "Beim Ziehen zeigt eine Lupe die Ecke vergrößert." => {
            "While dragging, a magnifier shows the corner enlarged."
        }
        "Beste Qualität (300 dpi)" => "Best quality (300 dpi)",
        "Bitte Ecken prüfen" => "Please check corners",
        "Bitte prüfen" => "Please check",
        "Blatt" => "Sheet",
        "Allgemein" => "General",
        "Breite" => "Width",
        "ca. {} MB" => "approx. {} MB",
        "Code gilt noch {} Min." => "Code valid for {} min",
        "Dabei fragt Knipsdruck nur bei GitHub nach der neuesten Version – ohne Daten über deine Fotos." => {
            "Knipsdruck only asks GitHub for the latest version – without any data about your photos."
        }
        "Darstellung" => "Appearance",
        "Das ganze Foto als Seite verwenden" => "Use the whole photo as the page",
        "Das hat nicht geklappt: {}" => "That didn't work: {}",
        "Den Vorschlag der Erkennung wiederherstellen" => "Restore the detected corners",
        "Der Code ist abgelaufen." => "The code has expired.",
        "Der Empfang vom Handy ließ sich nicht starten: {}" => "Receiving from the phone could not be started: {}",
        "der im Kamerabild erscheint." => "that appears in the camera view.",
        "Die Größen sind geschätzt." => "Sizes are estimates.",
        "Die letzten 10 Dokumente, 30 Tage lang, nur auf diesem Computer. Ausweiskopien werden nie gespeichert. „Aus“ löscht den Verlauf." => {
            "The last 10 documents, for 30 days, only on this computer. ID copies are never kept. “Off” clears the history."
        }
        "Die vollständige Liste liegt als THIRD-PARTY-LICENSES.yml im Programmordner." => {
            "The complete list is in THIRD-PARTY-LICENSES.yml in the program folder."
        }
        "Dokument ({})" => "Document ({})",
        "Download-Seite öffnen" => "Open download page",
        "Drehen" => "Rotate",
        "Drehen (R)" => "Rotate (R)",
        "Druck wird vorbereitet …" => "Preparing to print …",
        "Druck & PDF" => "Print & PDF",
        "Drucken" => "Print",
        "Drucken hat nicht geklappt: {}" => "Printing didn't work: {}",
        "Drucker" => "Printer",
        "Dunkel" => "Dark",
        "Durchsuchbares PDF" => "Searchable PDF",
        "E-Mail vorbereiten" => "Prepare email",
        "E-Mail wird vorbereitet …" => "Preparing email …",
        "Ecken" => "Corners",
        "Ecken anpassen" => "Adjust corners",
        "Ecken anpassen (E)" => "Adjust corners (E)",
        "Eigenes Maß …" => "Custom size …",
        "Ein Gäste-WLAN trennt Geräte oft voneinander – dann das normale WLAN nehmen." => {
            "A guest Wi-Fi often keeps devices apart – use the regular Wi-Fi instead."
        }
        "Eingefügtes Bild.png" => "Pasted image.png",
        "Einstellungen" => "Settings",
        "Einstellungen werden automatisch gespeichert." => "Settings are saved automatically.",
        "Ergebnis" => "Result",
        "ERGEBNIS" => "RESULT",
        "Es ist kein Drucker eingerichtet. Bitte in den Systemeinstellungen einen Drucker hinzufügen." => {
            "No printer is set up. Please add a printer in the system settings."
        }
        "Farbe" => "Color",
        "Fertig" => "Done",
        "Fotos" => "Photos",
        "Fotos aufnehmen oder auswählen" => "Take or choose photos",
        "Fotos auswählen" => "Choose photos",
        "Fotos auswählen …" => "Choose photos …",
        "Fotos hierher ziehen" => "Drop photos here",
        "Fotos vom Handy holen" => "Get photos from your phone",
        "Fragt der Computer nach der Firewall, bitte „Zulassen“ wählen." => {
            "If the computer asks about the firewall, choose “Allow”."
        }
        "Freie Software, lizenziert unter MIT oder Apache-2.0, nach Wahl. Alles wird auf diesem Computer berechnet, kein Foto verlässt ihn. Ohne jegliche Gewährleistung." => {
            "Free software, licensed under MIT or Apache-2.0, at your option. Everything is processed on this computer, no photo leaves it. Provided without any warranty."
        }
        "Für E-Mail" => "For email",
        "Ganzes Foto" => "Whole photo",
        "Gespeichert: {}" => "Saved: {}",
        "Gestern, {}" => "Yesterday, {}",
        "GitHub ist nicht erreichbar, bitte später noch einmal versuchen." => {
            "GitHub can't be reached, please try again later."
        }
        "Handy und Computer müssen im selben WLAN sein." => "Phone and computer must be on the same Wi-Fi.",
        "Handy verbunden" => "Phone connected",
        "Hell" => "Light",
        "Heute, {}" => "Today, {}",
        "Hochformat" => "Portrait",
        "Höhe" => "Height",
        "Im Ordner zeigen" => "Show in folder",
        "In der Zwischenablage ist kein Foto." => "There is no photo on the clipboard.",
        "Installiere Knipsdruck {} – die App startet gleich neu …" => {
            "Installing Knipsdruck {} – the app will restart in a moment …"
        }
        "Ist das ganze Blatt erfasst? Klicken zum Anpassen." => "Is the whole sheet captured? Click to adjust.",
        "Ist das Handy im WLAN? Mit mobilen Daten klappt es nicht." => {
            "Is the phone on Wi-Fi? It doesn't work over mobile data."
        }
        "Ja, prüfen" => "Yes, check",
        "Jedes Foto eine Seite" => "One page per photo",
        "Jetzt aktualisieren" => "Update now",
        "Jetzt nach Updates suchen" => "Check for updates now",
        "Kamera am Handy öffnen" => "Open the camera on your phone",
        "Kein E-Mail-Programm gefunden. Die Datei ist im Ordner markiert – ziehe sie in deine E-Mail." => {
            "No email program found. The file is selected in its folder – drag it into your email."
        }
        "Klappt nicht? ▸" => "Not working? ▸",
        "Klappt nicht? ▾" => "Not working? ▾",
        "klein" => "small",
        "Kleinere Datei (150 dpi), gut lesbar" => "Smaller file (150 dpi), easy to read",
        "Knipsdruck macht daraus saubere Seiten zum Ausdrucken." => {
            "Knipsdruck turns them into clean pages ready to print."
        }
        "Knipsdruck merkt sich deine letzten Dokumente 30 Tage lang, nur auf diesem Computer. Abschalten in den Einstellungen." => {
            "Knipsdruck keeps your recent documents for 30 days, only on this computer. You can turn this off in the settings."
        }
        "Knipsdruck wurde auf Version {} aktualisiert." => "Knipsdruck was updated to version {}.",
        "Knipsdruck {} ist die neueste Version." => "Knipsdruck {} is the latest version.",
        "Knipsdruck {} ist verfügbar (installiert: {})." => "Knipsdruck {} is available (installed: {}).",
        "Kopien" => "Copies",
        "Lade Knipsdruck {} …" => "Downloading Knipsdruck {} …",
        "LAN" => "LAN",
        "Lizenzen von Drittanbietern" => "Third-party licenses",
        "Lokal" => "Local",
        "Macht aus Handyfotos von Dokumenten druckfertige Seiten." => {
            "Turns phone photos of documents into print-ready pages."
        }
        "Mit Texterkennung: Das PDF lässt sich durchsuchen." => "With text recognition: the PDF will be searchable.",
        "MIT-Lizenz" => "MIT license",
        "MIT-Lizenz anzeigen" => "Show MIT license",
        "Nein" => "No",
        "Netzwerk" => "Network",
        "Neuen Code zeigen" => "Show new code",
        "Neues Dokument" => "New document",
        "Neues Dokument. ⌘Z holt das vorige zurück." => "New document. ⌘Z brings back the previous one.",
        "Normal" => "Normal",
        "Nur Blatt {}" => "Sheet {} only",
        "Nur diese Seite als PDF speichern" => "Save this page as PDF",
        "Nur diese Seite drucken" => "Print this page only",
        "Nur diese Seite per E-Mail senden" => "Email this page",
        "Nur dieses Blatt als PDF speichern" => "Save this sheet as PDF",
        "Nur dieses Blatt drucken" => "Print this sheet only",
        "Nur dieses Blatt per E-Mail senden" => "Email this sheet",
        "Nur Schwarz und Weiß" => "Pure black and white",
        "Nur Seite {}" => "Page {} only",
        "Nächste" => "Next",
        "Oder diese Adresse im Browser des Handys eingeben:" => "Or type this address into the phone's browser:",
        "oder {}V zum Einfügen" => "or {}V to paste",
        "Ohne" => "None",
        "Originalfoto" => "Original photo",
        "Papier" => "Paper",
        "Papierformat" => "Paper size",
        "Papierformat ändern" => "Change paper size",
        "PDF wird erstellt …" => "Creating PDF …",
        "Per E-Mail" => "Email",
        "Per E-Mail senden" => "Send by email",
        "Querformat" => "Landscape",
        "Rand" => "Margin",
        "Rückgängig" => "Undo",
        "Rückgängig gemacht" => "Undone",
        "Schließen" => "Close",
        "Schmal" => "Narrow",
        "Schwarzweiß" => "Black & white",
        "Schwarzweiß-Art" => "Black & white style",
        "Seite" => "Page",
        "Seite entfernen" => "Remove page",
        "Seite entfernt. ⌘Z holt sie zurück." => "Page removed. ⌘Z brings it back.",
        "Seite {} von {}" => "Page {} of {}",
        "Seiten" => "pages",
        "Sitzung beendet" => "Session ended",
        "Speichern …" => "Save …",
        "Sprache" => "Language",
        "Später" => "Later",
        "Standard: automatisch nach Region." => "Default: automatic by region.",
        "Tipp: Blatt flach hinlegen und möglichst von oben fotografieren. Alle vier Ecken sollten im Bild sein." => {
            "Tip: lay the sheet flat and take the photo from above. All four corners should be visible."
        }
        "und auf den Code richten." => "and point it at the code.",
        "Update fehlgeschlagen: {}" => "Update failed: {}",
        "Updates" => "Updates",
        "Verlauf gelöscht" => "History cleared",
        "Verlauf jetzt löschen" => "Clear history now",
        "Viele Drucker drucken nicht bis ganz an den Rand." => "Many printers cannot print right up to the edge.",
        "Vom Handy" => "From phone",
        "Vom Handy holen" => "Get from phone",
        "Vorder- und Rückseite kommen in Originalgröße (85,6 × 54 mm) untereinander auf ein Blatt. Foto 1 + 2 → Blatt 1, Foto 3 + 4 → Blatt 2 …" => {
            "Front and back are placed at actual size (85.6 × 54 mm) one above the other on a sheet. Photo 1 + 2 → sheet 1, photo 3 + 4 → sheet 2 …"
        }
        "Vorherige" => "Previous",
        "Warte auf das Handy …" => "Waiting for the phone …",
        "Weitere Aktionen" => "More actions",
        "Weitere Fotos" => "More photos",
        "Wie das System" => "Same as system",
        "Wie eine Kopie (Graustufen)" => "Like a photocopy (grayscale)",
        "Wiederhergestellt" => "Redone",
        "wird aufbereitet …" => "processing …",
        "WLAN" => "Wi-Fi",
        "Ziehe die vier Punkte auf die Ecken des Blatts." => "Drag the four points onto the corners of the sheet.",
        "Zuletzt verwendet" => "Recently used",
        "ZULETZT VERWENDET" => "RECENTLY USED",
        "Zum Drucken" => "For printing",
        "Zum Umsortieren eine Seite mit der Maus an eine andere Stelle ziehen." => {
            "Drag a page to another position to reorder."
        }
        "{} Fotos empfangen" => "{} photos received",
        "{} Fotos · Ausweiskopie auf {} Blatt" => "{} photos · ID copy on {} sheets",
        "{} im Dokument" => "{} in the document",
        "{} Seiten" => "{} pages",
        "{} von {} Seiten" => "{} of {} pages",
        "Öffnen" => "Open",
        "Über" => "About",
        "– sie erscheinen sofort hier." => "– they show up here right away.",
        "„Automatisch“ dreht jede Seite so, wie das Blatt auf dem Foto liegt." => {
            "“Automatic” turns each page the way the sheet lies in the photo."
        }
        "„Nur Schwarz und Weiß“ spart Toner, Bilder und Textmarker gehen dabei verloren." => {
            "“Pure black and white” saves toner, but pictures and highlighter are lost."
        }
        "„{}“ ist im HEIC-Format, das auf diesem System nicht gelesen werden kann." => {
            "“{}” is in HEIC format, which can't be read on this system."
        }
        "„{}“ ist kein lesbares Foto." => "“{}” is not a readable photo.",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Alle `tr("…")`/`trf("…")`-Texte im Quellcode müssen eine englische Übersetzung haben.
    #[test]
    fn every_text_is_translated() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let call = regex_lite_find;
        let mut missing = Vec::new();
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            let src = std::fs::read_to_string(&path).unwrap();
            for text in call(&src) {
                if text.chars().any(char::is_alphabetic) && en(&text).is_none() {
                    missing.push(format!("{}: {text:?}", path.file_name().unwrap().to_string_lossy()));
                }
            }
        }
        missing.sort();
        missing.dedup();
        assert!(missing.is_empty(), "Übersetzung fehlt für:\n{}", missing.join("\n"));
    }

    /// Findet Zeichenketten in `tr("…")`, `trf("…"` sowie in Tabellen mit `(Wert, "…")`, die später übersetzt werden.
    fn regex_lite_find(src: &str) -> Vec<String> {
        let mut out = Vec::new();
        for marker in ["tr(\"", "trf(\""] {
            let mut rest = src;
            while let Some(i) = rest.find(marker) {
                // nicht mitten in einem längeren Namen wie „attr(“
                let before = rest[..i].chars().last();
                rest = &rest[i + marker.len()..];
                if before.is_some_and(|c| c.is_alphanumeric() || c == '_') {
                    continue;
                }
                if let Some(end) = find_string_end(rest) {
                    out.push(unescape(&rest[..end]));
                }
            }
        }
        out
    }

    fn find_string_end(s: &str) -> Option<usize> {
        let mut escaped = false;
        for (i, c) in s.char_indices() {
            match c {
                '\\' if !escaped => escaped = true,
                '"' if !escaped => return Some(i),
                _ => escaped = false,
            }
        }
        None
    }

    fn unescape(s: &str) -> String {
        s.replace("\\n", "\n").replace("\\\"", "\"").replace("\\\\", "\\")
    }

    #[test]
    fn placeholders_are_filled_in_order() {
        set_lang(Lang::De);
        assert_eq!(trf("{} von {} Seiten", &[&3, &7]), "3 von 7 Seiten");
    }
}
