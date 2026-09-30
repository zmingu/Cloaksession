# local-ocr

独立 Rust 本地图片与 Windows 中文 OCR 能力；不依赖 multizen-core，不打开文件、不接数据库、不执行 PowerShell、不请求网络或安装资源。

## 集成

```rust,no_run
use local_ocr::{inspect_image_bounded, recognize};
use std::time::{Duration, Instant};

async fn inspect_then_recognize(bytes: Vec<u8>) -> local_ocr::Result<local_ocr::OcrOutput> {
    let deadline = Instant::now() + Duration::from_secs(20);
    // 图片接收阶段已经按 MAX_IMAGE_BYTES 限额，不要先任意分配再检查。
    let info = inspect_image_bounded(bytes.clone(), deadline).await?;
    // info.key 仅由实际原始 bytes 的 SHA-256 + 检测到的扩展名生成。
    // 应用自行在受控目录原子落位并提交DB引用；这里没有文件路径能力。
    let _ = info;
    recognize(bytes, deadline).await
}
```

仅需 OCR 时直接 `recognize`，无需重复 inspect。提供：

- `inspect_image(&[u8]) -> Result<ImageInfo>`：同步 CPU 解码；只供离线/调用者有界工作线程，不能放进 DB/launcher 循环。
- `inspect_image_bounded(Vec<u8>, Instant)`：异步，内部固定工作线程。
- `check_availability(Instant)`：实际创建 `zh-Hans-CN` 引擎，缺语言 `LanguageUnavailable`，无运行时 `RuntimeUnavailable`。
- `recognize(Vec<u8>, Instant)`：异步，真实 WinRT OCR。非 Windows 明确 `Unsupported`。
- `extract_candidates(&[String])`：无 IO 的保守姓名/18位号码候选提取，仅去空白和末位 x 大写化，不做 O/0 等纠错、不代表身份校验。

异步接口需要启用 time 的 Tokio/Tauri runtime。内部进程级固定 2 个 `local-ocr-*` OS 线程；线程内 MTA 保持至线程退出，避免反复销毁最后一个 MTA 后再次创建 Windows OCR 的原生崩溃。线程不会在空闲时缩容，也不按账号创建；不是每请求新线程。全进程在执行/排队合计最多 2 个任务，满额立即 `Busy`；调用者应有界退避，不积累图片队列。

传入绝对 deadline 贯穿队列、解码、WinRT 调用及返回；调用者得到有界超时。CPU 解码不能强制抢占，进行中的 OS 识别在 deadline 触发 `Cancel()`，工作线程继续保留配额与位图直至 OS 进入终态。若 OS 卡死，最多占住 2 个线程/配额，不自动扩容、不杀进程。调用 future 被丢弃后，排队任务可跳过，已在执行的任务仍按原 deadline 结束；取消不承诺立即终止原生计算。

## 图片与附件契约

只接受静态 PNG/JPEG/WebP。每图最多 10 MiB，单边最多 10,000，像素积最多 16,000,000；解码器 `max_alloc` 设为 128 MiB（依赖库明确这是尽力限制，不是整个进程 RSS 上限），输出像素缓冲另检查。禁用 image 默认格式集；拒绝 APNG/动画 WebP、GIF/SVG/其他格式。进行实际像素解码，不以魔数或 MIME/URL 声明当成功。

应用 EXIF 方向，包括旋转/镜像；不猜无 EXIF 的侧转照片、不静默缩放。OCR 额外比较系统 `MaxImageDimension`；透明像素合成白底并转 BGRA8/Ignore alpha `SoftwareBitmap`，真实 `RecognizeAsync` 后读取各行文本。没有云端、PowerShell 业务或第三方模型回退。

`ImageInfo`：`mime/ext: &'static str`，`width/height: u32` 为方向校正后的尺寸，`encoded_width/encoded_height: u32` 为原编码尺寸，`byte_len: usize`，`sha256/key: String`。哈希始终对应调用者的原始字节，不是旋转后的位图。key 形式只有 `64位小写sha256.png|jpg|webp`，不接收用户文件名/路径。

主代理适配 core 的 `SubjectAttachment`：`mime -> mime_type`，`byte_len as u64`，其余同名字段；原 core 存储上限较宽，运行时接收请使用本库更严格 `MAX_IMAGE_*`。本库不写缓存，调用方必须验证受控目录、防路径越界/符号链接、原子落位，并在成功后提交 DB 引用。检查/哈希不是图片清洗或 EXIF 敏感元数据剥离，也不是图片真实性证明。

`OcrOutput` 含 `image/language/lines/candidates`，`OcrCandidates` 含 `names/id_numbers`。均是独立 Rust 数据对象，不直接 serde/IPC；Tauri 按最小用途映射。OCR 行文本、姓名、号码不写日志；`Debug` 只显示行/候选计数，不输出其内容。完整号码结构、校验码、日期、可见片段比对、人工确认/revision 仍由 core/业务适配层处理，OCR 返回成功不能置“已核对”。

## 验证

仓库根目录：

```powershell
cargo check -p local-ocr --locked
cargo test -p local-ocr --locked
cargo test -p local-ocr --test windows_smoke --locked -- --ignored --nocapture
cargo clippy -p local-ocr --all-targets --locked -- -D warnings
```

`windows_smoke` 是需本机中文资源的显式测试，不把缺资源变成静默跳过。它只使用随库合成图，真实运行 WinRT 提取中文/姓名/虚构校验号码，重复及并发识别，并验证空白图、坏图和过期 deadline。fixture 标有“合成测试资料 非真实证件”，号码用虚构未分配地区代码、有效日期和自动计算校验码，不能代表真实身份证。

重生成固定 fixture（可选、开发专用）：`& crates/local-ocr/tools/generate-fixture.ps1`。使用系统已安装中文字体及 GDI+，不读真实证件、不下载字体/图片/模型。fixture 保留为离线 PNG，不需要目标机器生成字体。生成脚本不属于产品运行路径。
