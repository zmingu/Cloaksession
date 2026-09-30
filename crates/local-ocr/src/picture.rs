use crate::{OcrError, Result};
use image::{
    codecs::{jpeg::JpegDecoder, png::PngDecoder, webp::WebPDecoder},
    DynamicImage, ImageDecoder, ImageFormat, Limits,
};
use sha2::{Digest, Sha256};
use std::io::Cursor;

pub const MAX_IMAGE_BYTES: usize = 10 * 1024 * 1024;
pub const MAX_IMAGE_DIMENSION: u32 = 10_000;
pub const MAX_IMAGE_PIXELS: u64 = 16_000_000;
const MAX_DECODE_BYTES: u64 = 128 * 1024 * 1024;

/// Metadata comes from a successfully decoded image, never URL/extension claims.
/// width/height describe orientation-corrected pixels. encoded_* are stored dimensions.
/// sha256/key identify the ORIGINAL supplied bytes, not the rotated pixel buffer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImageInfo {
    pub mime: &'static str,
    pub ext: &'static str,
    pub width: u32,
    pub height: u32,
    pub encoded_width: u32,
    pub encoded_height: u32,
    pub byte_len: usize,
    pub sha256: String,
    pub key: String,
}

pub(crate) struct DecodedImage {
    pub info: ImageInfo,
    pub pixels: DynamicImage,
}

pub(crate) fn check_bytes(bytes: &[u8]) -> Result<()> {
    if bytes.is_empty() {
        return Err(OcrError::InvalidImage);
    }
    if bytes.len() > MAX_IMAGE_BYTES {
        return Err(OcrError::ImageTooLarge);
    }
    Ok(())
}

fn limits() -> Limits {
    let mut value = Limits::default();
    value.max_image_width = Some(MAX_IMAGE_DIMENSION);
    value.max_image_height = Some(MAX_IMAGE_DIMENSION);
    value.max_alloc = Some(MAX_DECODE_BYTES);
    value
}

fn decode_error(error: image::ImageError) -> OcrError {
    match error {
        image::ImageError::Limits(_) => OcrError::DimensionsExceeded,
        _ => OcrError::InvalidImage,
    }
}

/// Synchronous CPU work: use inspect_image_bounded from async IPC. This function
/// only reads the supplied slice and must never run on the serialized DB thread.
/// Accepts static PNG/JPEG/WebP; animation/SVG/GIF/unknown formats are rejected.
pub fn inspect_image(bytes: &[u8]) -> Result<ImageInfo> {
    let DecodedImage { info, pixels } = decode(bytes)?;
    drop(pixels);
    Ok(info)
}

pub(crate) fn decode(bytes: &[u8]) -> Result<DecodedImage> {
    check_bytes(bytes)?;
    let format = image::guess_format(bytes).map_err(|_| OcrError::InvalidImage)?;
    let reader = Cursor::new(bytes);
    match format {
        ImageFormat::Png => {
            let decoder = PngDecoder::with_limits(reader, limits()).map_err(decode_error)?;
            if decoder.is_apng().map_err(decode_error)? {
                return Err(OcrError::AnimatedImage);
            }
            finish_decode(decoder, bytes, "image/png", "png")
        }
        ImageFormat::Jpeg => {
            // The underlying JPEG decoder is tolerant; also require an EOI marker
            // so a truncated stream is not treated as a complete attachment.
            if !bytes.ends_with(&[0xff, 0xd9]) {
                return Err(OcrError::InvalidImage);
            }
            finish_decode(
                JpegDecoder::new(reader).map_err(decode_error)?,
                bytes,
                "image/jpeg",
                "jpg",
            )
        }
        ImageFormat::WebP => {
            validate_webp_container(bytes)?;
            let decoder = WebPDecoder::new(reader).map_err(decode_error)?;
            if decoder.has_animation() {
                return Err(OcrError::AnimatedImage);
            }
            finish_decode(decoder, bytes, "image/webp", "webp")
        }
        _ => Err(OcrError::UnsupportedImage),
    }
}

// image's WebP wrapper does not forward a metadata allocation limit. Validate
// every top-level RIFF extent before it may allocate based on an EXIF/VP8 length.
// This is only a resource preflight; successful full pixel decode is still required.
fn validate_webp_container(bytes: &[u8]) -> Result<()> {
    let size = bytes.get(4..8).ok_or(OcrError::InvalidImage)?;
    let declared = u32::from_le_bytes(size.try_into().map_err(|_| OcrError::InvalidImage)?);
    if u64::from(declared) + 8 != bytes.len() as u64 {
        return Err(OcrError::InvalidImage);
    }
    let mut offset = 12usize;
    while offset < bytes.len() {
        let header = bytes
            .get(offset..offset + 8)
            .ok_or(OcrError::InvalidImage)?;
        let length = u32::from_le_bytes(
            header[4..8]
                .try_into()
                .map_err(|_| OcrError::InvalidImage)?,
        ) as usize;
        let next = offset
            .checked_add(8)
            .and_then(|v| v.checked_add(length))
            .and_then(|v| v.checked_add(length & 1))
            .ok_or(OcrError::InvalidImage)?;
        if next > bytes.len() {
            return Err(OcrError::InvalidImage);
        }
        if matches!(&header[..4], b"ANIM" | b"ANMF") {
            return Err(OcrError::AnimatedImage);
        }
        offset = next;
    }
    if offset != bytes.len() {
        return Err(OcrError::InvalidImage);
    }
    Ok(())
}

fn finish_decode(
    mut decoder: impl ImageDecoder,
    bytes: &[u8],
    mime: &'static str,
    ext: &'static str,
) -> Result<DecodedImage> {
    decoder.set_limits(limits()).map_err(decode_error)?;
    let (encoded_width, encoded_height) = decoder.dimensions();
    if encoded_width == 0 || encoded_height == 0 {
        return Err(OcrError::InvalidImage);
    }
    if u64::from(encoded_width) * u64::from(encoded_height) > MAX_IMAGE_PIXELS
        || decoder.total_bytes() > MAX_DECODE_BYTES
    {
        return Err(OcrError::DimensionsExceeded);
    }
    let orientation = decoder.orientation().map_err(decode_error)?;
    let mut pixels = DynamicImage::from_decoder(decoder).map_err(decode_error)?;
    pixels.apply_orientation(orientation);
    let sha256 = format!("{:x}", Sha256::digest(bytes));
    let info = ImageInfo {
        mime,
        ext,
        width: pixels.width(),
        height: pixels.height(),
        encoded_width,
        encoded_height,
        byte_len: bytes.len(),
        key: format!("{sha256}.{ext}"),
        sha256,
    };
    Ok(DecodedImage { info, pixels })
}
