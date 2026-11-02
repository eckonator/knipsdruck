// SPDX-License-Identifier: MIT OR Apache-2.0
//! Erkennt Bilddateien an ihren ersten Bytes („Magic Bytes“), unabhängig von Dateiname und Content-Type.

/// Angenommene Bildformate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageKind {
    Jpeg,
    Png,
    Webp,
    Heic,
    Gif,
    Tiff,
    Bmp,
}

impl ImageKind {
    /// Übliche Dateiendung, falls das Handy keinen Namen mitschickt.
    pub fn extension(self) -> &'static str {
        match self {
            ImageKind::Jpeg => "jpg",
            ImageKind::Png => "png",
            ImageKind::Webp => "webp",
            ImageKind::Heic => "heic",
            ImageKind::Gif => "gif",
            ImageKind::Tiff => "tif",
            ImageKind::Bmp => "bmp",
        }
    }
}

/// HEIF-Marken (iPhone: `heic`, manche Android-Geräte: `mif1`).
const HEIF_BRANDS: &[&[u8; 4]] = &[b"heic", b"heix", b"hevc", b"mif1", b"msf1"];

/// Bestimmt das Format, `None` bei allem, was kein bekanntes Bild ist.
pub fn detect(data: &[u8]) -> Option<ImageKind> {
    let starts = |sig: &[u8]| data.starts_with(sig);
    if starts(&[0xFF, 0xD8, 0xFF]) {
        Some(ImageKind::Jpeg)
    } else if starts(b"\x89PNG\r\n\x1a\n") {
        Some(ImageKind::Png)
    } else if data.len() >= 12 && starts(b"RIFF") && &data[8..12] == b"WEBP" {
        Some(ImageKind::Webp)
    } else if is_heif(data) {
        Some(ImageKind::Heic)
    } else if starts(b"GIF87a") || starts(b"GIF89a") {
        Some(ImageKind::Gif)
    } else if starts(b"II*\0") || starts(b"MM\0*") {
        Some(ImageKind::Tiff)
    } else if data.len() >= 26 && starts(b"BM") {
        Some(ImageKind::Bmp)
    } else {
        None
    }
}

/// ISO-BMFF mit `ftyp`-Box ab Byte 4; Hauptmarke oder eine der kompatiblen Marken muss HEIF sein.
fn is_heif(data: &[u8]) -> bool {
    if data.len() < 12 || &data[4..8] != b"ftyp" {
        return false;
    }
    let box_len = u32::from_be_bytes([data[0], data[1], data[2], data[3]]) as usize;
    let end = box_len.clamp(12, data.len());
    let major = data[8..12].as_chunks::<4>().0;
    // Byte 12..16 ist die Nebenversion, danach folgen die kompatiblen Marken
    let (compatible, _) = data.get(16..end).unwrap_or_default().as_chunks::<4>();
    major.iter().chain(compatible).any(|brand| HEIF_BRANDS.contains(&brand))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ftyp(major: &[u8; 4], compatible: &[&[u8; 4]]) -> Vec<u8> {
        let len = 16 + 4 * compatible.len();
        let mut v = (len as u32).to_be_bytes().to_vec();
        v.extend_from_slice(b"ftyp");
        v.extend_from_slice(major);
        v.extend_from_slice(&[0, 0, 0, 0]);
        for c in compatible {
            v.extend_from_slice(*c);
        }
        v.extend_from_slice(&[0; 8]);
        v
    }

    #[test]
    fn known_formats() {
        assert_eq!(detect(&[0xFF, 0xD8, 0xFF, 0xE0, 0, 0x10]), Some(ImageKind::Jpeg));
        assert_eq!(detect(b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR"), Some(ImageKind::Png));
        assert_eq!(detect(b"RIFF\x24\0\0\0WEBPVP8 "), Some(ImageKind::Webp));
        assert_eq!(detect(b"GIF89a\x01\0\x01\0"), Some(ImageKind::Gif));
        assert_eq!(detect(b"GIF87a\x01\0\x01\0"), Some(ImageKind::Gif));
        assert_eq!(detect(b"II*\0\x08\0\0\0"), Some(ImageKind::Tiff));
        assert_eq!(detect(b"MM\0*\0\0\0\x08"), Some(ImageKind::Tiff));
        let mut bmp = b"BM".to_vec();
        bmp.resize(54, 0);
        assert_eq!(detect(&bmp), Some(ImageKind::Bmp));
    }

    #[test]
    fn heif_brands() {
        for brand in HEIF_BRANDS {
            assert_eq!(detect(&ftyp(brand, &[])), Some(ImageKind::Heic));
        }
        assert_eq!(detect(&ftyp(b"isom", &[b"mp41", b"mif1"])), Some(ImageKind::Heic));
        assert_eq!(detect(&ftyp(b"isom", &[b"mp41", b"avc1"])), None, "MP4-Video");
        assert_eq!(detect(&ftyp(b"qt  ", &[])), None);
    }

    #[test]
    fn rejects_other_data() {
        assert_eq!(detect(b""), None);
        assert_eq!(detect(b"%PDF-1.7"), None);
        assert_eq!(detect(b"<html>"), None);
        assert_eq!(detect(b"RIFF\x24\0\0\0WAVEfmt "), None);
        assert_eq!(detect(b"BM"), None);
        assert_eq!(detect(&[0xFF, 0xD8]), None);
        assert_eq!(detect(b"PK\x03\x04"), None);
    }
}
