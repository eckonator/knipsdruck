// SPDX-License-Identifier: MIT OR Apache-2.0
//! macOS: Drucken über PDFKit mit dem Standarddruckdialog, E-Mail über `NSSharingService`, Finder.

use std::path::Path;

use objc2::rc::Retained;
use objc2::{AnyThread, MainThreadMarker};
use objc2_app_kit::{
    NSPaperOrientation, NSPrintInfo, NSPrintingPaginationMode, NSSharingService, NSSharingServiceNameComposeEmail,
    NSWorkspace,
};
use objc2_foundation::{NSArray, NSCopying, NSSize, NSString, NSURL};
use objc2_pdf_kit::{PDFDocument, PDFPrintScalingMode};

use crate::{Job, Outcome};

pub use crate::cups::{print_with_lp, printers};

/// Typografische Punkte pro Millimeter.
const POINTS_PER_MM: f64 = 72.0 / 25.4;

pub fn print(job: &Job) -> Result<Outcome, String> {
    let mtm = main_thread()?;
    let url = file_url(job.pdf)?;
    let document = unsafe { PDFDocument::initWithURL(PDFDocument::alloc(), &url) }
        .ok_or_else(|| format!("PDF {} nicht lesbar", job.pdf.display()))?;
    let info = print_info(&job.pages[0]);
    // 100 % Größe; autoRotate dreht Querseiten auf das hochkant eingelegte Papier.
    let operation = unsafe {
        document.printOperationForPrintInfo_scalingMode_autoRotate(
            Some(&info),
            PDFPrintScalingMode::PageScaleNone,
            true,
            mtm,
        )
    }
    .ok_or("PDFKit liefert keinen Druckvorgang")?;
    operation.setJobTitle(Some(&NSString::from_str(job.name)));
    operation.setShowsPrintPanel(true);
    operation.setShowsProgressPanel(true);
    if operation.runOperation() {
        Ok(Outcome::Sent)
    } else {
        Ok(Outcome::Cancelled)
    }
}

/// Kopie der geteilten Druckeinstellungen (behält den zuletzt gewählten Drucker) ohne Ränder, mit dem
/// Papier der ersten Seite.
fn print_info(first: &crate::Page) -> Retained<NSPrintInfo> {
    let info = NSPrintInfo::sharedPrintInfo().copy();
    info.setLeftMargin(0.0);
    info.setRightMargin(0.0);
    info.setTopMargin(0.0);
    info.setBottomMargin(0.0);
    info.setHorizontallyCentered(true);
    info.setVerticallyCentered(true);
    info.setHorizontalPagination(NSPrintingPaginationMode::Automatic);
    info.setVerticalPagination(NSPrintingPaginationMode::Automatic);
    let (w, h) = (
        f64::from(first.width_mm) * POINTS_PER_MM,
        f64::from(first.height_mm) * POINTS_PER_MM,
    );
    // Papier immer hochkant angeben, die Lage steckt in der Ausrichtung.
    info.setPaperSize(NSSize::new(w.min(h), w.max(h)));
    info.setOrientation(if w > h {
        NSPaperOrientation::Landscape
    } else {
        NSPaperOrientation::Portrait
    });
    info
}

pub fn compose_email(attachment: &Path, subject: &str) -> Result<(), String> {
    main_thread()?;
    let service = NSSharingService::sharingServiceNamed(unsafe { NSSharingServiceNameComposeEmail })
        .ok_or("Kein E-Mail-Dienst vorhanden")?;
    let url = file_url(attachment)?;
    let items = NSArray::from_retained_slice(&[Retained::into_super(Retained::into_super(url))]);
    if !unsafe { service.canPerformWithItems(Some(&items)) } {
        return Err("Kein E-Mail-Programm eingerichtet".into());
    }
    service.setSubject(Some(&NSString::from_str(subject)));
    unsafe { service.performWithItems(&items) };
    Ok(())
}

pub fn reveal(path: &Path) -> Result<(), String> {
    let url = file_url(path)?;
    NSWorkspace::sharedWorkspace().activateFileViewerSelectingURLs(&NSArray::from_retained_slice(&[url]));
    Ok(())
}

fn main_thread() -> Result<MainThreadMarker, String> {
    MainThreadMarker::new().ok_or_else(|| "Muss auf dem Haupt-Thread aufgerufen werden".to_string())
}

fn file_url(path: &Path) -> Result<Retained<NSURL>, String> {
    NSURL::from_file_path(path).ok_or_else(|| format!("Kein Datei-URL für {}", path.display()))
}
