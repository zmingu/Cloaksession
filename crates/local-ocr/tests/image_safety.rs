use image::{DynamicImage, ImageFormat, Rgb, RgbImage};
use local_ocr::{inspect_image, OcrError, MAX_IMAGE_BYTES, MAX_IMAGE_DIMENSION};
use std::io::Cursor;

fn encoded(width: u32, height: u32, format: ImageFormat) -> Vec<u8> {
    let image = DynamicImage::ImageRgb8(RgbImage::from_pixel(width, height, Rgb([255, 255, 255])));
    let mut output = Cursor::new(Vec::new());
    image.write_to(&mut output, format).unwrap();
    output.into_inner()
}

#[test]
fn real_decode_and_content_addressed_key() {
    for (format, mime, ext) in [
        (ImageFormat::Png, "image/png", "png"),
        (ImageFormat::Jpeg, "image/jpeg", "jpg"),
        (ImageFormat::WebP, "image/webp", "webp"),
    ] {
        let bytes = encoded(32, 16, format);
        let info = inspect_image(&bytes).unwrap();
        assert_eq!((info.width, info.height), (32, 16));
        assert_eq!((info.mime, info.ext), (mime, ext));
        assert_eq!(info.byte_len, bytes.len());
        assert_eq!(info.key, format!("{}.{ext}", info.sha256));
        assert_eq!(info.sha256.len(), 64);
        assert!(info
            .sha256
            .bytes()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
        assert_eq!(inspect_image(&bytes).unwrap(), info);
    }
}

#[test]
fn rejects_empty_corrupt_unsupported_and_header_only() {
    for bytes in [
        &b""[..],
        &b"not an image"[..],
        &b"\xff\xd8\xff\xd9"[..],
        &b"\x89PNG\r\n\x1a\n"[..],
    ] {
        assert_eq!(inspect_image(bytes), Err(OcrError::InvalidImage));
    }
    assert_eq!(
        inspect_image(b"GIF89a................"),
        Err(OcrError::UnsupportedImage)
    );
    let bytes = encoded(32, 16, ImageFormat::Png);
    assert_eq!(inspect_image(&bytes[..40]), Err(OcrError::InvalidImage));
    let jpeg = encoded(32, 16, ImageFormat::Jpeg);
    assert_eq!(
        inspect_image(&jpeg[..jpeg.len() - 2]),
        Err(OcrError::InvalidImage)
    );
}

#[test]
fn rejects_webp_declared_chunks_outside_the_actual_buffer() {
    let mut bytes = encoded(32, 16, ImageFormat::WebP);
    bytes[16..20].copy_from_slice(&u32::MAX.to_le_bytes());
    assert_eq!(inspect_image(&bytes), Err(OcrError::InvalidImage));
    let mut bytes = encoded(32, 16, ImageFormat::WebP);
    bytes[4..8].copy_from_slice(&u32::MAX.to_le_bytes());
    assert_eq!(inspect_image(&bytes), Err(OcrError::InvalidImage));
}

#[test]
fn rejects_encoded_size_and_dimensions_before_pixel_decode() {
    assert_eq!(
        inspect_image(&vec![0; MAX_IMAGE_BYTES + 1]),
        Err(OcrError::ImageTooLarge)
    );
    let bytes = encoded(MAX_IMAGE_DIMENSION + 1, 1, ImageFormat::Png);
    assert_eq!(inspect_image(&bytes), Err(OcrError::DimensionsExceeded));
    let bytes = encoded(4001, 4000, ImageFormat::Png);
    assert_eq!(inspect_image(&bytes), Err(OcrError::DimensionsExceeded));
}

#[test]
fn applies_jpeg_exif_orientation_before_reporting_dimensions() {
    let bytes = encoded(32, 16, ImageFormat::Jpeg);
    // EXIF little-endian TIFF with one orientation SHORT = 6 (rotate 90 CW).
    let exif = b"Exif\0\0II\x2a\0\x08\0\0\0\x01\0\x12\x01\x03\0\x01\0\0\0\x06\0\0\0\0\0\0\0";
    let mut oriented = Vec::from(&bytes[..2]);
    oriented.extend([0xff, 0xe1]);
    oriented.extend(((exif.len() + 2) as u16).to_be_bytes());
    oriented.extend(exif);
    oriented.extend(&bytes[2..]);
    let info = inspect_image(&oriented).unwrap();
    assert_eq!((info.encoded_width, info.encoded_height), (32, 16));
    assert_eq!((info.width, info.height), (16, 32));
    assert_ne!(info.key, inspect_image(&bytes).unwrap().key);
}

// Build only synthetic PNG metadata chunks; never load external test images.
fn png_chunk(kind: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let mut output = Vec::from((data.len() as u32).to_be_bytes());
    output.extend(kind);
    output.extend(data);
    let mut crc = !0u32;
    for &byte in &output[4..] {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = if crc & 1 == 1 {
                (crc >> 1) ^ 0xedb88320
            } else {
                crc >> 1
            };
        }
    }
    output.extend((!crc).to_be_bytes());
    output
}

#[test]
fn rejects_animation_instead_of_silently_ocring_a_thumbnail() {
    let bytes = encoded(32, 16, ImageFormat::Png);
    let mut animation = bytes[..33].to_vec(); // after IHDR
    animation.extend(png_chunk(b"acTL", &[0, 0, 0, 2, 0, 0, 0, 0]));
    animation.extend(&bytes[33..]);
    assert_eq!(inspect_image(&animation), Err(OcrError::AnimatedImage));
}

#[test]
fn applies_png_exif_orientation() {
    let bytes = encoded(32, 16, ImageFormat::Png);
    let exif = b"II\x2a\0\x08\0\0\0\x01\0\x12\x01\x03\0\x01\0\0\0\x08\0\0\0\0\0\0\0";
    let mut oriented = bytes[..33].to_vec();
    oriented.extend(png_chunk(b"eXIf", exif));
    oriented.extend(&bytes[33..]);
    let info = inspect_image(&oriented).unwrap();
    assert_eq!((info.width, info.height), (16, 32));
}

#[tokio::test]
async fn deadline_and_bounded_inspection_are_explicit() {
    let bytes = encoded(32, 16, ImageFormat::Png);
    assert_eq!(
        local_ocr::inspect_image_bounded(bytes.clone(), std::time::Instant::now()).await,
        Err(OcrError::DeadlineExceeded)
    );
    let info = local_ocr::inspect_image_bounded(
        bytes,
        std::time::Instant::now() + std::time::Duration::from_secs(5),
    )
    .await
    .unwrap();
    assert_eq!(info.width, 32);
}

#[cfg(not(windows))]
#[tokio::test]
async fn non_windows_is_explicitly_unsupported() {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    assert!(matches!(
        local_ocr::recognize(vec![], deadline).await,
        Err(OcrError::Unsupported)
    ));
    assert_eq!(
        local_ocr::check_availability(deadline).await,
        Err(OcrError::Unsupported)
    );
}
