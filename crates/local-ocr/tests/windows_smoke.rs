#![cfg(windows)]

use image::{DynamicImage, ImageFormat, Rgb, RgbImage};
use local_ocr::{check_availability, recognize, OcrError, OCR_LANGUAGE};
use std::{
    io::Cursor,
    time::{Duration, Instant},
};

fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(30)
}

/// Opt-in because Windows installations may lack the language resource. This
/// test MUST NOT silently pass or skip when explicitly requested and unavailable.
/// Its fixture is generated offline, bears a fake non-issued region and contains
/// no actual identity document, portrait or private account data.
#[tokio::test]
#[ignore = "requires installed Windows zh-Hans-CN OCR; synthetic local fixture only"]
async fn real_winrt_chinese_smoke() {
    check_availability(deadline())
        .await
        .expect("installed Chinese OCR must be available");
    let bytes = include_bytes!("fixtures/synthetic-zh.png").to_vec();
    let output = recognize(bytes, deadline())
        .await
        .expect("actual WinRT recognition must succeed");
    assert_eq!(output.language, OCR_LANGUAGE);
    assert!(!output.lines.is_empty());
    assert!(
        output.lines.iter().any(|line| line
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect::<String>()
            .contains("测试")),
        "synthetic Chinese text must be recognized"
    );
    assert!(
        output.candidates.names.iter().any(|name| name == "测试"),
        "synthetic name candidate must be recognized"
    );
    let prefix = "99010120000101001"; // deliberately fictitious region, valid date
    let weights = [7, 9, 10, 5, 8, 4, 2, 1, 6, 3, 7, 9, 10, 5, 8, 4, 2];
    let sum: usize = prefix
        .bytes()
        .zip(weights)
        .map(|(n, w)| usize::from(n - b'0') * w)
        .sum();
    let expected = format!("{prefix}{}", b"10X98765432"[sum % 11] as char);
    assert!(
        output.candidates.id_numbers.contains(&expected),
        "synthetic checksum-valid number must be recognized"
    );
    // Evidence without logging text, candidates, pixels or content-derived hashes.
    println!(
        "WinRT zh-Hans-CN: {} lines; name and checksum-valid synthetic number recognized",
        output.lines.len()
    );

    // Repeated engine creation on both fixed workers must not tear down the MTA.
    for _ in 0..3 {
        check_availability(deadline()).await.unwrap();
        let (first, second) = tokio::join!(
            recognize(
                include_bytes!("fixtures/synthetic-zh.png").to_vec(),
                deadline()
            ),
            recognize(
                include_bytes!("fixtures/synthetic-zh.png").to_vec(),
                deadline()
            ),
        );
        for result in [first, second] {
            let repeated = result.expect("repeated concurrent native OCR must succeed");
            assert!(repeated.candidates.names.iter().any(|name| name == "测试"));
            assert!(repeated.candidates.id_numbers.contains(&expected));
        }
    }

    let blank = DynamicImage::ImageRgb8(RgbImage::from_pixel(800, 400, Rgb([255, 255, 255])));
    let mut buffer = Cursor::new(Vec::new());
    blank.write_to(&mut buffer, ImageFormat::Png).unwrap();
    assert!(matches!(
        recognize(buffer.into_inner(), deadline()).await,
        Err(OcrError::EmptyRecognition)
    ));
    assert!(matches!(
        recognize(b"invalid image".to_vec(), deadline()).await,
        Err(OcrError::InvalidImage)
    ));
    assert!(matches!(
        recognize(
            include_bytes!("fixtures/synthetic-zh.png").to_vec(),
            Instant::now()
        )
        .await,
        Err(OcrError::DeadlineExceeded)
    ));
}
