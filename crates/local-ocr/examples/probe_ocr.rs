use std::time::{Duration, Instant};

fn main() {
    let path = std::env::args().nth(1).expect("usage: probe-ocr <image-path>");
    let bytes = std::fs::read(&path).unwrap_or_else(|e| { eprintln!("read failed: {e}"); std::process::exit(1); });
    eprintln!("Image size: {} bytes", bytes.len());

    let runtime = tokio::runtime::Builder::new_current_thread().enable_time().build().expect("tokio runtime");
    runtime.block_on(async {
        let deadline = Instant::now() + Duration::from_secs(30);
        match local_ocr::inspect_image_bounded(bytes.clone(), deadline).await {
            Ok(info) => eprintln!("Image info: {}x{} mime={} key={}", info.width, info.height, info.mime, info.key),
            Err(e) => { eprintln!("inspect failed: {e}"); std::process::exit(1); }
        }
        match local_ocr::recognize(bytes, deadline).await {
            Ok(output) => {
                eprintln!("OCR language: {}", output.language);
                eprintln!("Lines: {}", output.lines.len());
                for (i, line) in output.lines.iter().enumerate() {
                    eprintln!("  line {i}: {line}");
                }
                eprintln!("Names: {:?}", output.candidates.names);
                eprintln!("ID numbers: {:?}", output.candidates.id_numbers);
            }
            Err(e) => { eprintln!("OCR failed: {e}"); std::process::exit(1); }
        }
    });
}
