// SPDX-License-Identifier: MIT OR Apache-2.0
//! Apple Vision (`VNRecognizeTextRequest`, Stufe „accurate“, Deutsch und Englisch). In macOS eingebaut.

use image::RgbImage;
use objc2::AnyThread;
use objc2::rc::autoreleasepool;
use objc2_foundation::{NSArray, NSData, NSDictionary, NSRange, NSString};
use objc2_vision::{VNImageRequestHandler, VNRecognizeTextRequest, VNRequest, VNRequestTextRecognitionLevel};

use crate::{Engine, Word, encode_png};

pub struct Vision;

impl Engine for Vision {
    fn name(&self) -> &'static str {
        "Apple Vision"
    }

    fn available(&self) -> bool {
        true
    }

    fn recognize(&self, image: &RgbImage) -> Result<Vec<Word>, String> {
        let png = encode_png(image)?;
        let (w, h) = (image.width() as f64, image.height() as f64);
        autoreleasepool(|_| {
            let data = NSData::with_bytes(&png);
            let handler = VNImageRequestHandler::initWithData_options(
                VNImageRequestHandler::alloc(),
                &data,
                &NSDictionary::new(),
            );
            let request = VNRecognizeTextRequest::new();
            request.setRecognitionLevel(VNRequestTextRecognitionLevel::Accurate);
            request.setUsesLanguageCorrection(true);
            let langs = NSArray::from_retained_slice(&[NSString::from_str("de-DE"), NSString::from_str("en-US")]);
            request.setRecognitionLanguages(&langs);
            let as_request: &VNRequest = &request;
            handler
                .performRequests_error(&NSArray::from_slice(&[as_request]))
                .map_err(|e| format!("Apple Vision: {}", e.localizedDescription()))?;

            let mut words = Vec::new();
            for obs in request.results().iter().flat_map(|r| r.iter()) {
                let Some(text) = obs.topCandidates(1).firstObject() else {
                    continue;
                };
                let line = text.string().to_string();
                // Wörter mit eigenem Rahmen (Bereich in UTF-16-Einheiten, wie NSString zählt)
                let mut offset = 0usize;
                for part in line.split(' ') {
                    let len = part.encode_utf16().count();
                    if len > 0 {
                        let range = NSRange::new(offset, len);
                        // SAFETY: Bereich liegt innerhalb der Zeichenkette dieses Kandidaten
                        if let Ok(b) = unsafe { text.boundingBoxForRange_error(range) } {
                            // SAFETY: einfache Eigenschaft einer gültigen Beobachtung
                            let bb = unsafe { b.boundingBox() };
                            // normiert, Ursprung unten links → Pixel, Ursprung oben links
                            words.push(Word {
                                text: part.to_string(),
                                x: (bb.origin.x * w) as f32,
                                y: ((1.0 - bb.origin.y - bb.size.height) * h) as f32,
                                w: (bb.size.width * w) as f32,
                                h: (bb.size.height * h) as f32,
                            });
                        }
                    }
                    offset += len + 1;
                }
            }
            Ok(words)
        })
    }
}
