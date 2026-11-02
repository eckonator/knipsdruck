// SPDX-License-Identifier: MIT OR Apache-2.0
//! Kleine Texthelfer: Prozent-Dekodierung, HTML-Maskierung und Bereinigung von Dateinamen.

use crate::magic::ImageKind;

/// Höchstlänge eines übernommenen Dateinamens in Zeichen.
const MAX_NAME_CHARS: usize = 120;

fn hex_value(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

/// Dekodiert `%XX`-Folgen (wie von `encodeURIComponent`) zu UTF-8. Ungültige Folgen bleiben stehen,
/// ungültiges UTF-8 wird ersetzt. `+` bleibt `+` (Leerzeichen kommen als `%20`).
pub fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let decoded = match bytes.get(i + 1..i + 3) {
            Some([h, l]) if bytes[i] == b'%' => hex_value(*h).zip(hex_value(*l)).map(|(h, l)| (h << 4) | l),
            _ => None,
        };
        match decoded {
            Some(b) => {
                out.push(b);
                i += 3;
            }
            None => {
                out.push(bytes[i]);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Maskiert Text für HTML-Inhalt und Attributwerte.
pub fn html_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

/// Macht aus dem mitgeschickten Namen einen harmlosen Dateinamen: nur der letzte Pfadteil, ohne
/// Steuerzeichen, gekürzt. Fehlt er, entsteht „Foto <n>.<endung>“.
pub fn clean_file_name(raw: &str, kind: ImageKind, number: usize) -> String {
    let last = raw.rsplit(['/', '\\']).next().unwrap_or_default();
    let name: String = last.chars().filter(|c| !c.is_control()).take(MAX_NAME_CHARS).collect();
    let name = name.trim().trim_start_matches('.').trim();
    if name.is_empty() {
        format!("Foto {number}.{}", kind.extension())
    } else {
        name.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_percent() {
        assert_eq!(percent_decode("Seite%201.png"), "Seite 1.png");
        assert_eq!(percent_decode("%C3%9Cbung.jpg"), "Übung.jpg");
        assert_eq!(percent_decode("%c3%bc"), "ü");
        assert_eq!(percent_decode("a+b"), "a+b");
        assert_eq!(percent_decode("100%"), "100%");
        assert_eq!(percent_decode("%zz%4"), "%zz%4");
        assert_eq!(percent_decode("%FF"), "\u{FFFD}");
        assert_eq!(percent_decode(""), "");
        assert_eq!(percent_decode("%F0%9F%93%B7"), "📷");
    }

    #[test]
    fn escapes_html() {
        assert_eq!(html_escape("Büro-PC"), "Büro-PC");
        assert_eq!(
            html_escape("<b>\"A&B\"</b>'"),
            "&lt;b&gt;&quot;A&amp;B&quot;&lt;/b&gt;&#39;"
        );
    }

    #[test]
    fn cleans_names() {
        assert_eq!(clean_file_name("IMG_0001.HEIC", ImageKind::Heic, 1), "IMG_0001.HEIC");
        assert_eq!(clean_file_name("../../etc/passwd", ImageKind::Png, 1), "passwd");
        assert_eq!(clean_file_name("C:\\temp\\a.jpg", ImageKind::Jpeg, 1), "a.jpg");
        assert_eq!(clean_file_name("", ImageKind::Jpeg, 3), "Foto 3.jpg");
        assert_eq!(clean_file_name(" \n ", ImageKind::Png, 2), "Foto 2.png");
        assert_eq!(clean_file_name("..", ImageKind::Png, 2), "Foto 2.png");
        assert_eq!(
            clean_file_name(&"x".repeat(500), ImageKind::Png, 1).chars().count(),
            MAX_NAME_CHARS
        );
    }
}
