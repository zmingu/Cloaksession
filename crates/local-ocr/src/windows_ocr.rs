use crate::{
    check_deadline, extract_candidates, picture, OcrCandidates, OcrError, OcrOutput, Result,
    OCR_LANGUAGE,
};
use std::{
    thread,
    time::{Duration, Instant},
};
use windows::{
    core::HSTRING,
    Globalization::Language,
    Graphics::Imaging::{BitmapAlphaMode, BitmapPixelFormat, SoftwareBitmap},
    Media::Ocr::OcrEngine,
    Storage::Streams::DataWriter,
    Win32::System::WinRT::{RoInitialize, RoUninitialize, RO_INIT_MULTITHREADED},
};
use windows_future::AsyncStatus;

// This guard never crosses a thread boundary. All WinRT objects are created and
// destroyed before its destructor, on the admitted blocking worker's MTA thread.
struct Apartment;
impl Apartment {
    fn enter() -> Result<Self> {
        // SAFETY: called on the current blocking worker; every successful call
        // (including S_FALSE) is balanced by exactly one RoUninitialize here.
        unsafe { RoInitialize(RO_INIT_MULTITHREADED) }.map_err(|_| OcrError::RuntimeUnavailable)?;
        Ok(Self)
    }
}
impl Drop for Apartment {
    fn drop(&mut self) {
        // SAFETY: this guard is scoped synchronously to the initializing thread.
        unsafe { RoUninitialize() };
    }
}

// Initialized once on each of the fixed process-lifetime worker threads. Keeping
// the MTA alive across requests is essential: per-request RoUninitialize followed
// by engine recreation caused a native access violation in the real smoke test.
fn ensure_apartment() -> Result<()> {
    thread_local! { static APARTMENT: Result<Apartment> = Apartment::enter(); }
    APARTMENT.with(|value| value.as_ref().map(|_| ()).map_err(Clone::clone))
}

fn create_engine(language_tag: &str) -> Result<OcrEngine> {
    let language = Language::CreateLanguage(&HSTRING::from(language_tag))
        .map_err(|_| OcrError::RuntimeUnavailable)?;
    if !OcrEngine::IsLanguageSupported(&language).map_err(|_| OcrError::RuntimeUnavailable)? {
        return Err(OcrError::LanguageUnavailable);
    }
    OcrEngine::TryCreateFromLanguage(&language).map_err(|_| OcrError::LanguageUnavailable)
}

pub(crate) fn check_availability() -> Result<()> {
    ensure_apartment()?;
    create_engine(OCR_LANGUAGE)?;
    Ok(())
}

/// One OCR pass over a BGRA8 buffer. Empty text stays an explicit error so the
/// caller can distinguish "nothing recognized" from "text without candidates".
fn recognize_lines(
    engine: &OcrEngine,
    bgra: &[u8],
    width: u32,
    height: u32,
    deadline: Instant,
) -> Result<Vec<String>> {
    let bitmap = {
        let writer = DataWriter::new().map_err(|_| OcrError::RecognitionFailed)?;
        writer
            .WriteBytes(bgra)
            .map_err(|_| OcrError::RecognitionFailed)?;
        let buffer = writer
            .DetachBuffer()
            .map_err(|_| OcrError::RecognitionFailed)?;
        SoftwareBitmap::CreateCopyWithAlphaFromBuffer(
            &buffer,
            BitmapPixelFormat::Bgra8,
            width as i32,
            height as i32,
            BitmapAlphaMode::Ignore,
        )
        .map_err(|_| OcrError::RecognitionFailed)?
    };
    let operation = engine
        .RecognizeAsync(&bitmap)
        .map_err(|_| OcrError::RecognitionFailed)?;
    let mut expired = false;
    loop {
        let status = operation
            .Status()
            .map_err(|_| OcrError::RecognitionFailed)?;
        if status != AsyncStatus::Started {
            if expired {
                return Err(OcrError::DeadlineExceeded);
            }
            if status != AsyncStatus::Completed {
                return Err(OcrError::RecognitionFailed);
            }
            break;
        }
        if !expired && check_deadline(deadline).is_err() {
            expired = true;
            let _ = operation.Cancel();
            // Caller already has a bounded async timeout. Keep the worker permit
            // and bitmap alive until the OS acknowledges terminal status. If an
            // OS request hangs, at most MAX_CONCURRENT_JOBS workers remain occupied.
        }
        thread::sleep(Duration::from_millis(10));
    }
    check_deadline(deadline)?;
    let result = operation
        .GetResults()
        .map_err(|_| OcrError::RecognitionFailed)?;
    let recognized = result.Lines().map_err(|_| OcrError::RecognitionFailed)?;
    let mut lines = Vec::new();
    for line in recognized {
        let text = line
            .Text()
            .map_err(|_| OcrError::RecognitionFailed)?
            .to_string();
        if !text.trim().is_empty() {
            lines.push(text);
        }
    }
    if lines.is_empty() {
        return Err(OcrError::EmptyRecognition);
    }
    Ok(lines)
}

/// A sideways certificate photo arrives portrait-oriented while the engine reads
/// horizontal lines, so both quarter turns are attempted; the orientation with
/// complete, unambiguous candidates wins, and the as-is pass always runs first.
fn candidate_rank(candidates: &OcrCandidates) -> (bool, usize) {
    (
        candidates.names.len() == 1 && candidates.id_numbers.len() == 1,
        candidates.names.len() + candidates.id_numbers.len(),
    )
}

fn pass(
    engine: &OcrEngine,
    bitmap: &image::RgbaImage,
    info: &crate::picture::ImageInfo,
    deadline: Instant,
) -> Result<(bool, usize, OcrOutput)> {
    check_deadline(deadline)?;
    let lines = recognize_lines(
        engine,
        bitmap.as_raw(),
        bitmap.width(),
        bitmap.height(),
        deadline,
    )?;
    let candidates = extract_candidates(&lines);
    let (complete, score) = candidate_rank(&candidates);
    Ok((
        complete,
        score,
        OcrOutput {
            image: info.clone(),
            language: OCR_LANGUAGE,
            lines,
            candidates,
        },
    ))
}

fn record(
    best: &mut Option<(bool, usize, OcrOutput)>,
    first_error: &mut Option<OcrError>,
    result: Result<(bool, usize, OcrOutput)>,
) {
    match result {
        Ok((complete, score, output)) => {
            if best.as_ref().map_or(true, |(won_complete, won_score, _)| {
                complete > *won_complete || (complete == *won_complete && score > *won_score)
            }) {
                *best = Some((complete, score, output));
            }
        }
        Err(error) => {
            first_error.get_or_insert(error);
        }
    }
}

fn complete_pass(best: &Option<(bool, usize, OcrOutput)>) -> bool {
    best.as_ref().is_some_and(|(complete, _, _)| *complete)
}

pub(crate) fn recognize(bytes: &[u8], deadline: Instant) -> Result<OcrOutput> {
    check_deadline(deadline)?;
    let decoded = picture::decode(bytes)?;
    check_deadline(deadline)?;
    ensure_apartment()?;
    let engine = create_engine(OCR_LANGUAGE)?;
    let max = OcrEngine::MaxImageDimension().map_err(|_| OcrError::RuntimeUnavailable)?;
    if decoded.info.width > max || decoded.info.height > max {
        return Err(OcrError::DimensionsExceeded);
    }

    // Flatten transparency onto white before BGRA conversion. The raw buffer is
    // entirely memory-local and SoftwareBitmap makes an owned copy of it.
    let mut rgba = decoded.pixels.into_rgba8();
    for pixel in rgba.pixels_mut() {
        let a = u16::from(pixel[3]);
        for channel in &mut pixel.0[..3] {
            *channel = ((u16::from(*channel) * a + 255 * (255 - a) + 127) / 255) as u8;
        }
        pixel.0.swap(0, 2);
        pixel[3] = 255;
    }
    check_deadline(deadline)?;

    let portrait = decoded.info.height > decoded.info.width;
    let mut best: Option<(bool, usize, OcrOutput)> = None;
    let mut first_error: Option<OcrError> = None;
    record(
        &mut best,
        &mut first_error,
        pass(&engine, &rgba, &decoded.info, deadline),
    );
    if portrait && !complete_pass(&best) {
        let clockwise = image::imageops::rotate90(&rgba);
        record(
            &mut best,
            &mut first_error,
            pass(&engine, &clockwise, &decoded.info, deadline),
        );
        if !complete_pass(&best) {
            let counter_clockwise = image::imageops::rotate270(&rgba);
            record(
                &mut best,
                &mut first_error,
                pass(&engine, &counter_clockwise, &decoded.info, deadline),
            );
        }
    }
    match best {
        Some((_, _, output)) => Ok(output),
        None => Err(first_error.unwrap_or(OcrError::EmptyRecognition)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn missing_language_is_explicit() {
        let _apartment = Apartment::enter().expect("WinRT must initialize");
        assert!(matches!(
            create_engine("zxx"),
            Err(OcrError::LanguageUnavailable)
        ));
    }
}
