// SPDX-License-Identifier: MIT OR Apache-2.0
//! Windows: Systemdruckdialog `PrintDlgExW`, die Seitenbilder gehen per GDI in exakter physischer Größe
//! an den Drucker (keine Anpassung an die Seite). E-Mail über Simple MAPI, Ordner über den Explorer.

use std::ffi::{OsStr, c_void};
use std::mem::{offset_of, size_of};
use std::os::windows::ffi::OsStrExt;
use std::os::windows::process::CommandExt;
use std::path::Path;
use std::process::Command;
use std::ptr::{null, null_mut};

use windows_sys::Win32::Foundation::{GlobalFree, HGLOBAL, HWND};
use windows_sys::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, DEVMODEW, DIB_RGB_COLORS, DM_COLLATE, DM_COPIES, DM_FORMNAME, DM_ORIENTATION,
    DM_PAPERLENGTH, DM_PAPERSIZE, DM_PAPERWIDTH, DMCOLLATE_FALSE, DMORIENT_LANDSCAPE, DMORIENT_PORTRAIT, DMPAPER_A3,
    DMPAPER_A4, DMPAPER_A5, DMPAPER_A6, DMPAPER_LEGAL, DMPAPER_LETTER, DMPAPER_USER, DeleteDC, GDI_ERROR,
    GetDeviceCaps, HALFTONE, HDC, LOGPIXELSX, LOGPIXELSY, PHYSICALHEIGHT, PHYSICALOFFSETX, PHYSICALOFFSETY,
    PHYSICALWIDTH, ResetDCW, SRCCOPY, SetBrushOrgEx, SetStretchBltMode, StretchDIBits,
};
use windows_sys::Win32::Storage::Xps::{AbortDoc, DOCINFOW, EndDoc, EndPage, StartDocW, StartPage};
use windows_sys::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoUninitialize};
use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};
use windows_sys::Win32::System::Mapi::{MAPI_DIALOG, MAPI_LOGON_UI, MAPI_USER_ABORT, MapiFileDescW, MapiMessageW};
use windows_sys::Win32::System::Memory::{GlobalLock, GlobalUnlock};
use windows_sys::Win32::UI::Controls::Dialogs::{
    PD_COLLATE, PD_NOCURRENTPAGE, PD_NOSELECTION, PD_PAGENUMS, PD_RESULT_PRINT, PD_RETURNDC, PRINTDLGEXW,
    PRINTPAGERANGE, PrintDlgExW, START_PAGE_GENERAL,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetActiveWindow;
use windows_sys::Win32::UI::WindowsAndMessaging::{GetDesktopWindow, GetForegroundWindow};

use crate::winjob::{dib_stripe, print_sequence, scaled, stripes};
use crate::{Job, Outcome, Page, Printer};

/// Kein Konsolenfenster für gestartete Programme.
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Bildzeilen pro `StretchDIBits`-Aufruf; hält den Speicherbedarf im Druckertreiber klein.
const STRIPE_ROWS: u32 = 512;

/// So viele Seitenbereiche darf man im Dialog angeben.
const MAX_PAGE_RANGES: usize = 32;

/// Unterschied zwischen gewünschtem und tatsächlichem Papier, ab dem wir warnen.
const PAPER_MISMATCH_MM: f32 = 2.0;

pub fn printers() -> Vec<Printer> {
    Vec::new()
}

pub fn print_with_lp(_pdf: &Path, _printer: &str, _copies: u32, _media: &str) -> Result<(), String> {
    Err("CUPS gibt es auf Windows nicht".into())
}

pub fn print(job: &Job) -> Result<Outcome, String> {
    let _com = ComApartment::enter();
    let Some(dialog) = PrintDialog::show(job.pages.len())? else {
        return Ok(Outcome::Cancelled);
    };
    let sequence = print_sequence(job.pages.len(), &dialog.ranges, dialog.copies, dialog.collate);
    if sequence.is_empty() {
        return Err("Die gewählten Seiten gibt es nicht".into());
    }
    let mut devmode = dialog.devmode();
    print_document(dialog.dc, devmode.as_mut(), job, &sequence)?;
    Ok(Outcome::Sent)
}

/// Ergebnis des Druckdialogs. Gibt beim Verwerfen Gerätekontext und Dialogspeicher frei.
struct PrintDialog {
    dc: HDC,
    devmode: HGLOBAL,
    devnames: HGLOBAL,
    /// Seitenbereiche ab 1; leer heißt alle.
    ranges: Vec<(u32, u32)>,
    copies: u32,
    collate: bool,
}

impl PrintDialog {
    /// Zeigt den Dialog. `None`, wenn abgebrochen.
    fn show(page_count: usize) -> Result<Option<Self>, String> {
        let mut ranges = [PRINTPAGERANGE::default(); MAX_PAGE_RANGES];
        // SAFETY: PRINTDLGEXW ist ein C-Struct, für das lauter Nullen gültig sind.
        let mut pd: PRINTDLGEXW = unsafe { std::mem::zeroed() };
        pd.lStructSize = size_of::<PRINTDLGEXW>() as u32;
        pd.hwndOwner = owner_window();
        pd.Flags = PD_RETURNDC | PD_NOSELECTION | PD_NOCURRENTPAGE;
        pd.nMinPage = 1;
        pd.nMaxPage = page_count as u32;
        pd.nMaxPageRanges = MAX_PAGE_RANGES as u32;
        pd.lpPageRanges = ranges.as_mut_ptr();
        pd.nCopies = 1;
        pd.nStartPage = START_PAGE_GENERAL;
        // SAFETY: `pd` ist vollständig initialisiert, `ranges` lebt bis nach dem Aufruf.
        let hr = unsafe { PrintDlgExW(&mut pd) };
        let mut dialog = PrintDialog {
            dc: pd.hDC,
            devmode: pd.hDevMode,
            devnames: pd.hDevNames,
            ranges: Vec::new(),
            copies: pd.nCopies.max(1),
            collate: pd.Flags & PD_COLLATE != 0,
        };
        if hr < 0 {
            return Err(format!("PrintDlgExW fehlgeschlagen: 0x{hr:08X}"));
        }
        if pd.dwResultAction != PD_RESULT_PRINT {
            return Ok(None);
        }
        if dialog.dc.is_null() {
            return Err("Druckdialog lieferte keinen Gerätekontext".into());
        }
        if pd.Flags & PD_PAGENUMS != 0 {
            let used = (pd.nPageRanges as usize).min(MAX_PAGE_RANGES);
            dialog.ranges = ranges[..used].iter().map(|r| (r.nFromPage, r.nToPage)).collect();
        }
        Ok(Some(dialog))
    }

    /// Kopie der Druckereinstellungen aus dem Dialog, falls vorhanden.
    fn devmode(&self) -> Option<DevMode> {
        if self.devmode.is_null() {
            return None;
        }
        // SAFETY: `hDevMode` ist ein beweglicher Speicherblock mit einem DEVMODEW samt Treiberdaten.
        unsafe {
            let ptr = GlobalLock(self.devmode) as *const DEVMODEW;
            if ptr.is_null() {
                return None;
            }
            let copy = DevMode::copy_from(ptr);
            GlobalUnlock(self.devmode);
            Some(copy)
        }
    }
}

impl Drop for PrintDialog {
    fn drop(&mut self) {
        // SAFETY: Die Handles stammen aus PrintDlgExW und gehören uns.
        unsafe {
            if !self.dc.is_null() {
                DeleteDC(self.dc);
            }
            if !self.devmode.is_null() {
                GlobalFree(self.devmode);
            }
            if !self.devnames.is_null() {
                GlobalFree(self.devnames);
            }
        }
    }
}

/// Eigene Kopie eines DEVMODEW inklusive der privaten Treiberdaten dahinter, 8-Byte-ausgerichtet.
struct DevMode {
    buf: Vec<u64>,
}

impl DevMode {
    /// # Safety
    /// `ptr` muss auf ein gültiges DEVMODEW mit `dmSize + dmDriverExtra` lesbaren Bytes zeigen.
    unsafe fn copy_from(ptr: *const DEVMODEW) -> Self {
        // SAFETY: laut Vertrag lesbar.
        let len = unsafe { usize::from((*ptr).dmSize) + usize::from((*ptr).dmDriverExtra) };
        let mut buf = vec![0u64; len.max(size_of::<DEVMODEW>()).div_ceil(8)];
        // SAFETY: Quelle hat `len` Bytes, Ziel mindestens so viele.
        unsafe { std::ptr::copy_nonoverlapping(ptr as *const u8, buf.as_mut_ptr() as *mut u8, len) };
        DevMode { buf }
    }

    fn as_ptr(&self) -> *const DEVMODEW {
        self.buf.as_ptr() as *const DEVMODEW
    }

    /// Papierformat und Ausrichtung für eine Seite; Kopien macht der Druckauftrag selbst.
    fn set_page(&mut self, width_mm: f32, height_mm: f32) {
        // SAFETY: Der Puffer ist mindestens so groß wie DEVMODEW und passend ausgerichtet.
        let dm = unsafe { &mut *(self.buf.as_mut_ptr() as *mut DEVMODEW) };
        let (short, long) = (width_mm.min(height_mm), width_mm.max(height_mm));
        let orientation = if width_mm > height_mm {
            DMORIENT_LANDSCAPE
        } else {
            DMORIENT_PORTRAIT
        };
        let paper = crate::media::known_paper(width_mm, height_mm).and_then(|p| dmpaper(p.name));
        // SAFETY: Bei Druckern gilt der Druckerzweig der Union.
        let fields = unsafe { &mut dm.Anonymous1.Anonymous1 };
        fields.dmOrientation = orientation as i16;
        fields.dmCopies = 1;
        dm.dmFields |= DM_ORIENTATION | DM_PAPERSIZE | DM_COPIES;
        // Ein gesetzter Formularname hätte bei manchen Treibern Vorrang vor der Papiergröße.
        dm.dmFields &= !DM_FORMNAME;
        match paper {
            Some(code) => {
                fields.dmPaperSize = code as i16;
                dm.dmFields &= !(DM_PAPERWIDTH | DM_PAPERLENGTH);
            }
            None => {
                // Eigene Größe in 0,1 mm, hochkant angegeben.
                fields.dmPaperSize = DMPAPER_USER as i16;
                fields.dmPaperWidth = (short * 10.0).round() as i16;
                fields.dmPaperLength = (long * 10.0).round() as i16;
                dm.dmFields |= DM_PAPERWIDTH | DM_PAPERLENGTH;
            }
        }
        // Ältere, kürzere DEVMODEs haben kein dmCollate; dort lägen schon die Treiberdaten.
        if usize::from(dm.dmSize) >= offset_of!(DEVMODEW, dmCollate) + size_of::<i16>() {
            dm.dmCollate = DMCOLLATE_FALSE;
            dm.dmFields |= DM_COLLATE;
        }
    }
}

/// Windows-Papiercode für die bekannten Formate. ISO-B5 hat keinen eigenen (DMPAPER_B5 ist JIS-B5).
fn dmpaper(name: &str) -> Option<u32> {
    match name {
        "A3" => Some(DMPAPER_A3),
        "A4" => Some(DMPAPER_A4),
        "A5" => Some(DMPAPER_A5),
        "A6" => Some(DMPAPER_A6),
        "Letter" => Some(DMPAPER_LETTER),
        "Legal" => Some(DMPAPER_LEGAL),
        _ => None,
    }
}

/// Druckt die Seiten in der gegebenen Reihenfolge als ein Dokument.
fn print_document(dc: HDC, mut devmode: Option<&mut DevMode>, job: &Job, sequence: &[usize]) -> Result<(), String> {
    let name = wide(job.name);
    let info = DOCINFOW {
        cbSize: size_of::<DOCINFOW>() as i32,
        lpszDocName: name.as_ptr(),
        lpszOutput: null(),
        lpszDatatype: null(),
        fwType: 0,
    };
    // SAFETY: `dc` ist ein Drucker-DC, `info` und `name` leben bis nach dem Aufruf.
    if unsafe { StartDocW(dc, &info) } <= 0 {
        return Err("StartDocW fehlgeschlagen".into());
    }
    for &index in sequence {
        let page = &job.pages[index];
        if let Some(dm) = devmode.as_deref_mut() {
            dm.set_page(page.width_mm, page.height_mm);
            // SAFETY: zwischen zwei Seiten erlaubt; liefert bei Erfolg denselben DC zurück.
            if unsafe { ResetDCW(dc, dm.as_ptr()) }.is_null() {
                log::warn!("ResetDCW fehlgeschlagen, Seite {} mit bisherigem Papier", index + 1);
            }
        }
        if let Err(e) = print_page(dc, page) {
            // SAFETY: Dokument ist gestartet.
            unsafe { AbortDoc(dc) };
            return Err(format!("Seite {}: {e}", index + 1));
        }
    }
    // SAFETY: Dokument ist gestartet.
    if unsafe { EndDoc(dc) } <= 0 {
        return Err("EndDoc fehlgeschlagen".into());
    }
    Ok(())
}

/// Eine Seite: Bild in physischer Größe, mittig auf dem tatsächlichen Papier.
fn print_page(dc: HDC, page: &Page) -> Result<(), String> {
    // SAFETY: `dc` ist ein Drucker-DC mit gestartetem Dokument.
    if unsafe { StartPage(dc) } <= 0 {
        return Err("StartPage fehlgeschlagen".into());
    }
    let caps = |index: u32| unsafe { GetDeviceCaps(dc, index as i32) };
    let (dpi_x, dpi_y) = (caps(LOGPIXELSX), caps(LOGPIXELSY));
    if dpi_x <= 0 || dpi_y <= 0 {
        return Err("Drucker meldet keine Auflösung".into());
    }
    let dest_w = mm_to_px(page.width_mm, dpi_x);
    let dest_h = mm_to_px(page.height_mm, dpi_y);
    let (paper_w, paper_h) = (caps(PHYSICALWIDTH), caps(PHYSICALHEIGHT));
    warn_on_paper_mismatch(
        page,
        paper_w as f32 / dpi_x as f32 * 25.4,
        paper_h as f32 / dpi_y as f32 * 25.4,
    );
    // Ursprung des DC ist die Ecke des bedruckbaren Bereichs, nicht des Papiers.
    let x = (paper_w - dest_w) / 2 - caps(PHYSICALOFFSETX);
    let y = (paper_h - dest_h) / 2 - caps(PHYSICALOFFSETY);
    draw_image(dc, page.image, x, y, dest_w, dest_h)?;
    // SAFETY: Seite ist gestartet.
    if unsafe { EndPage(dc) } <= 0 {
        return Err("EndPage fehlgeschlagen".into());
    }
    Ok(())
}

fn mm_to_px(mm: f32, dpi: i32) -> i32 {
    (f64::from(mm) / 25.4 * f64::from(dpi)).round() as i32
}

fn warn_on_paper_mismatch(page: &Page, paper_w_mm: f32, paper_h_mm: f32) {
    if (paper_w_mm - page.width_mm).abs() > PAPER_MISMATCH_MM || (paper_h_mm - page.height_mm).abs() > PAPER_MISMATCH_MM
    {
        log::warn!(
            "Drucker nutzt Papier {paper_w_mm:.0}×{paper_h_mm:.0} mm statt {:.0}×{:.0} mm",
            page.width_mm,
            page.height_mm
        );
    }
}

/// Zeichnet das Bild in Streifen auf das Rechteck `(x, y, w, h)` in Gerätepixeln.
fn draw_image(dc: HDC, image: &image::RgbImage, x: i32, y: i32, w: i32, h: i32) -> Result<(), String> {
    // SAFETY: gültiger DC.
    unsafe {
        SetStretchBltMode(dc, HALFTONE);
        SetBrushOrgEx(dc, 0, 0, null_mut());
    }
    for (y0, y1) in stripes(image.height(), STRIPE_ROWS) {
        let bits = dib_stripe(image, y0, y1);
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: image.width() as i32,
                // Positiv: Zeilen von unten nach oben, das verstehen alle Treiber.
                biHeight: (y1 - y0) as i32,
                biPlanes: 1,
                biBitCount: 24,
                biCompression: BI_RGB,
                biSizeImage: bits.len() as u32,
                ..Default::default()
            },
            bmiColors: Default::default(),
        };
        let (dy0, dy1) = (scaled(y0, image.height(), h), scaled(y1, image.height(), h));
        // SAFETY: `bits` passt zu `info` (Breite, Höhe, 24 Bit, aufgefüllte Zeilen).
        let lines = unsafe {
            StretchDIBits(
                dc,
                x,
                y + dy0,
                w,
                dy1 - dy0,
                0,
                0,
                image.width() as i32,
                (y1 - y0) as i32,
                bits.as_ptr() as *const c_void,
                &info,
                DIB_RGB_COLORS,
                SRCCOPY,
            )
        };
        if lines == 0 || lines == GDI_ERROR {
            return Err(format!("StretchDIBits fehlgeschlagen (Zeilen {y0}–{y1})"));
        }
    }
    Ok(())
}

/// COM im Single-Threaded-Apartment, wie es der Druckdialog braucht; nur abmelden, was wir angemeldet haben.
struct ComApartment {
    initialized: bool,
}

impl ComApartment {
    fn enter() -> Self {
        // SAFETY: einfacher Aufruf; RPC_E_CHANGED_MODE (schon anders initialisiert) ist harmlos.
        let hr = unsafe { CoInitializeEx(null(), COINIT_APARTMENTTHREADED as u32) };
        ComApartment { initialized: hr >= 0 }
    }
}

impl Drop for ComApartment {
    fn drop(&mut self) {
        if self.initialized {
            // SAFETY: passend zu einem erfolgreichen CoInitializeEx.
            unsafe { CoUninitialize() };
        }
    }
}

/// Fenster, dem die Dialoge gehören. `PrintDlgExW` lehnt `NULL` ab.
fn owner_window() -> HWND {
    // SAFETY: einfache Abfragen ohne Vorbedingungen.
    unsafe {
        [GetActiveWindow(), GetForegroundWindow()]
            .into_iter()
            .find(|w| !w.is_null())
            .unwrap_or_else(|| GetDesktopWindow())
    }
}

/// Signatur von `MAPISendMailW`.
type MapiSendMailW = unsafe extern "system" fn(usize, usize, *const MapiMessageW, u32, u32) -> u32;

pub fn compose_email(attachment: &Path, subject: &str) -> Result<(), String> {
    let send = mapi_send_mail()?;
    let mut path = wide(attachment);
    let mut file_name = wide(attachment.file_name().unwrap_or(attachment.as_os_str()));
    let mut subject = wide(subject);
    let mut note = wide("");
    let mut file = MapiFileDescW {
        nPosition: u32::MAX,
        lpszPathName: path.as_mut_ptr(),
        lpszFileName: file_name.as_mut_ptr(),
        ..Default::default()
    };
    let message = MapiMessageW {
        lpszSubject: subject.as_mut_ptr(),
        lpszNoteText: note.as_mut_ptr(),
        nFileCount: 1,
        lpFiles: &mut file,
        ..Default::default()
    };
    // SAFETY: Alle Zeiger in `message` zeigen auf nullterminierte Puffer, die bis nach dem Aufruf leben.
    let code = unsafe { send(0, owner_window() as usize, &message, MAPI_LOGON_UI | MAPI_DIALOG, 0) };
    match code {
        0 => Ok(()),
        // Fenster war offen, die Person hat es selbst geschlossen.
        MAPI_USER_ABORT => Ok(()),
        _ => Err(format!("MAPISendMailW fehlgeschlagen (Code {code})")),
    }
}

/// `MAPISendMailW` aus MAPI32.DLL. Die DLL bleibt geladen, manche E-Mail-Programme arbeiten nach dem
/// Aufruf noch darin weiter.
fn mapi_send_mail() -> Result<MapiSendMailW, String> {
    let name = wide("MAPI32.DLL");
    // SAFETY: nullterminierter Name.
    let module = unsafe { LoadLibraryW(name.as_ptr()) };
    if module.is_null() {
        return Err("MAPI32.DLL nicht vorhanden".into());
    }
    // SAFETY: gültiges Modul, nullterminierter Name.
    let proc = unsafe { GetProcAddress(module, c"MAPISendMailW".as_ptr() as *const u8) }
        .ok_or("MAPISendMailW nicht vorhanden (kein MAPI-E-Mail-Programm)")?;
    // SAFETY: dokumentierte Signatur von MAPISendMailW.
    Ok(unsafe { std::mem::transmute::<unsafe extern "system" fn() -> isize, MapiSendMailW>(proc) })
}

pub fn reveal(path: &Path) -> Result<(), String> {
    // Explorer will `/select,"Pfad"` als ein Argument mit genau diesen Anführungszeichen.
    Command::new("explorer")
        .raw_arg(format!("/select,\"{}\"", path.display()))
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map(drop)
        .map_err(|e| format!("Explorer nicht startbar: {e}"))
}

/// Nullterminierter UTF-16-String für die Windows-API.
fn wide(s: impl AsRef<OsStr>) -> Vec<u16> {
    s.as_ref().encode_wide().chain(std::iter::once(0)).collect()
}
