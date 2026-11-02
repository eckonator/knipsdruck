// SPDX-License-Identifier: MIT OR Apache-2.0
//! Tesseract als externes Programm (`tesseract … tsv`). Unter Linux die Standard-Engine;
//! muss der Nutzer selbst installieren (z. B. `apt install tesseract-ocr tesseract-ocr-deu`).

use std::process::Command;

use image::RgbImage;

use crate::{Engine, Word, encode_png};

pub struct Tesseract;

impl Tesseract {
    fn languages() -> String {
        let out = Command::new("tesseract").arg("--list-langs").output();
        let list = out
            .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
            .unwrap_or_default();
        let has = |l: &str| list.lines().any(|x| x.trim() == l);
        match (has("deu"), has("eng")) {
            (true, true) => "deu+eng".into(),
            (true, false) => "deu".into(),
            _ => "eng".into(),
        }
    }
}

impl Engine for Tesseract {
    fn name(&self) -> &'static str {
        "Tesseract"
    }

    fn available(&self) -> bool {
        Command::new("tesseract")
            .arg("--version")
            .output()
            .is_ok_and(|o| o.status.success())
    }

    fn recognize(&self, image: &RgbImage) -> Result<Vec<Word>, String> {
        let png = encode_png(image)?;
        let file = std::env::temp_dir().join(format!(
            "knipsdruck-ocr-{}-{:?}.png",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::write(&file, png).map_err(|e| format!("tesseract: {e}"))?;
        let out = Command::new("tesseract")
            .arg(&file)
            .arg("stdout")
            .args(["-l", &Self::languages(), "--psm", "3", "tsv"])
            .output();
        let _ = std::fs::remove_file(&file);
        let out = out.map_err(|e| format!("tesseract: {e}"))?;
        if !out.status.success() {
            return Err(format!("tesseract: {}", String::from_utf8_lossy(&out.stderr).trim()));
        }
        Ok(parse_tsv(&String::from_utf8_lossy(&out.stdout)))
    }
}

/// Wertet die TSV-Ausgabe aus: Ebene 5 sind Wörter mit Position und Sicherheit (0–100, −1 = kein Wort).
fn parse_tsv(tsv: &str) -> Vec<Word> {
    let mut words = Vec::new();
    for line in tsv.lines().skip(1) {
        let cols: Vec<&str> = line.split('\t').collect();
        if cols.len() < 12 || cols[0] != "5" {
            continue;
        }
        let text = cols[11].trim();
        let conf: f32 = cols[10].parse().unwrap_or(-1.0);
        if text.is_empty() || conf < 0.0 {
            continue;
        }
        let num = |i: usize| cols[i].parse::<f32>().unwrap_or(0.0);
        words.push(Word {
            text: text.to_string(),
            x: num(6),
            y: num(7),
            w: num(8),
            h: num(9),
        });
    }
    words
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_words() {
        let tsv = "level\tpage_num\tblock_num\tpar_num\tline_num\tword_num\tleft\ttop\twidth\theight\tconf\ttext\n\
                   1\t1\t0\t0\t0\t0\t0\t0\t2480\t3508\t-1\t\n\
                   5\t1\t1\t1\t1\t1\t300\t400\t200\t40\t96.5\tRechnung\n\
                   5\t1\t1\t1\t1\t2\t520\t400\t180\t40\t88\tNr.\n";
        let words = parse_tsv(tsv);
        assert_eq!(words.len(), 2);
        assert_eq!(words[0].text, "Rechnung");
        assert_eq!(words[1].x, 520.0);
    }
}
