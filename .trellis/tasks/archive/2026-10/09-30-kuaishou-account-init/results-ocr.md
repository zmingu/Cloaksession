# OCR 独立能力实施结果

日期：2026-10-01。范围：仅本子任务独立 `crates/local-ocr`、根 Cargo 注册/锁文件及本文；未修改主任务状态或规划审批记录。用户已批准本轮实现，本报告不代表 UI/账号初始化闭环完成。

## 已实现接口与安全边界

新增独立 Rust crate `local-ocr`，不依赖 multizen-core、profile-manager、tauri-app、cdp-driver；不引入循环依赖。仅接收图片内存字节，没有任意文件路径、网络、DB、安装语言或模型的能力。

- `inspect_image(&[u8]) -> Result<ImageInfo>`：真实像素解码后返回元信息，不只看头部；同步 CPU 方法，不在 launcher/DB 线程调用。
- `inspect_image_bounded(Vec<u8>, std::time::Instant) -> async Result<ImageInfo>`：内置有界工作线程和绝对 deadline，推荐 Tauri async 调用。
- `check_availability(Instant) -> async Result<()>`：只读检查/创建系统 `zh-Hans-CN` 引擎，缺资源明确失败，不自动安装/云端回退。
- `recognize(Vec<u8>, Instant) -> async Result<OcrOutput>`：真实 Rust WinRT `Windows.Media.Ocr`，非 Windows 明确 `Unsupported`。
- `extract_candidates(&[String]) -> OcrCandidates`：姓名标签和18位号码保守候选，仅移除空白、末位小写 x 规范化，不做 O/0 等自动纠错。core 继续负责校验码/日期/可见片段/人工确认。

`ImageInfo`：`mime/ext: &'static str`，方向校正后 `width/height: u32`、原编码 `encoded_width/encoded_height: u32`，`byte_len: usize`、`sha256/key: String`。key 是原始输入 SHA-256 小写64位 + `.png/.jpg/.webp`。core `SubjectAttachment` 适配：`mime -> mime_type`、`byte_len as u64`，其余同名；无需本库依赖 core。

`OcrOutput`：`image/language/lines/candidates`。`OcrCandidates`：`names/id_numbers`。不直接导出 serde/IPC 模型，集成代理按必要字段适配；`Debug` 隐去所有行文字/姓名/号码，错误仅为固定类别/固定文字，不输出底层异常携带的输入数据。

图片限额：10 MiB、单边10,000、像素积16,000,000；解码内存 `max_alloc=128 MiB` 并检查输出字节。依赖库说明 max_alloc 是尽力限额，不是整个进程RSS硬上限。core 的存储上限较宽，采集层应使用本库更严格的 `MAX_IMAGE_*`。格式静态 PNG/JPEG/WebP；动画 APNG/WebP、GIF/SVG/其他格式拒绝。WebP 在实际解码之前校验 RIFF 总长度、所有顶层 chunk/padding 范围，避免恶意声明 EXIF/像素 chunk 长度触发超出输入大小的分配。JPEG 要求 EOI，解码错误明确返回。

处理链路：bytes → 有界真实解码 → EXIF 方向（含旋转/镜像）→ 白底透明合成/BGRA8 → 内存 DataWriter/IBuffer → SoftwareBitmap → `RecognizeAsync` → 逐行文字 → 保守候选。额外读取 OS `MaxImageDimension` 防止引擎维度更小；无 EXIF 的歪斜/侧转照片不猜测方向、不静默降采样。哈希对应原始附件字节，方向校正仅用于解码/OCR，不改写附件原图。

## 线程、截止时间与实际修复

内部进程级固定 2 个命名 OS 工作线程 `local-ocr-*`，有界队列 + 全局2份许可，执行与排队合计最多2个任务；满额立即 `Busy`。不占 DB/launcher 线程，也不让 Tokio runtime 关闭等待卡住的 spawn_blocking OCR。Tauri async 侧须有启用 time 的 Tokio runtime，传任务整体绝对 deadline，且在接收图片时先限流/限字节。

初版每调用 `RoInitialize/RoUninitialize` 曾在“可用性探测后再次创建引擎”处出现 `STATUS_ACCESS_VIOLATION`；实际识别测试暴露，未作为完成。修复为固定线程内 thread-local MTA 保持线程生命周期，绝不在每个请求后销毁最后一个MTA。最终测试连续与并发重复创建/识别成功。

调用者得到 deadline 超时；未开始的过期/已取消任务不启动，已开始的 CPU 解码不能抢占。原生 OCR 到期 `Cancel()`，仍保留位图及许可直到 OS 终态，防止超时释放许可后无限创建替代任务。若OS卡死，最多占满2线程并令后续请求Busy，不扩容、不停止用户进程。调用 future 被丢弃不表示即时终止原生计算，仍受原 deadline 管理。线程是进程生命周期资源，空闲不缩容。

## 测试与实际证据

只使用合成资料：`tests/fixtures/synthetic-zh.png`（1600×600、23924字节），开发脚本 `tools/generate-fixture.ps1` 使用已安装 Microsoft YaHei/GDI+ 离线生成。图片标明“合成测试资料 非真实证件”，姓名为测试字样，号码使用虚构未分配地区、有效日期和计算的合法校验码，无真人照片/真实账号/真实证件。该 PowerShell **仅测试图生成器**，不是产品OCR业务。

执行命令（仓库根）：

```powershell
& crates/local-ocr/tools/generate-fixture.ps1
cargo check -p local-ocr --locked
cargo fetch --target x86_64-pc-windows-msvc
cargo check -p local-ocr --offline
cargo test -p local-ocr --locked
cargo test -p local-ocr --test windows_smoke --locked -- --ignored --nocapture
cargo clippy -p local-ocr --all-targets --locked -- -D warnings
cargo check --workspace --locked
```

初次普通 `cargo check -p local-ocr --locked` 按预期失败：新增图片依赖需要更新 Cargo.lock。随后 Cargo 获取公开crate并重锁，`--offline` 完成编译，再用 `--locked` 测试与工作区检查通过。未下载OCR模型、图片或字体。

最终验证：

- 普通测试：5个单元测试 + 8个图片安全测试通过。覆盖无效/损坏/空图、仅头部、编码字节上限、维度/像素数上限、WebP恶意声明长度、APNG拒绝、PNG/JPEG EXIF方向、3种格式真实解码、hash/key、候选去空白/去重/保守拒绝、Debug脱敏、缺语言错误和超时后许可继续占用/Busy/完成后恢复。
- 显式真实 `windows_smoke`：1项通过，实际输出 **3行文字，姓名候选与虚构合法校验号码候选均成功**。随后3轮每轮2并发的真实识别均成功；空白图返回 `EmptyRecognition`，坏图返回 `InvalidImage`，过期deadline返回 `DeadlineExceeded`。日志只记录行数与断言成功，不输出证件文字。
- `cargo clippy -p local-ocr --all-targets --locked -- -D warnings` 通过。
- `cargo check --workspace --locked` 通过。
- 本机只安装 `x86_64-pc-windows-msvc` 目标；非Windows有明确 `Unsupported` 分支及条件测试，但本轮未执行跨平台编译，不能宣称已验证。

真实 smoke 默认 ignored 是为了无中文资源机器的常规CI；本轮已明确执行 `--ignored`，并且测试在显式运行时资源不可用会失败，绝不静默跳过。

## 依赖与待集成内容

公开依赖：`image` 声明0.25.8兼容范围/锁定0.25.10（仅PNG/JPEG/WebP，禁用默认格式集）、`sha2 0.10`、`thiserror 1`、`tokio 1`；Windows条件依赖复用 `windows 0.61.3`、`windows-future 0.2.1`。锁文件新增公开图像链路8包：image、image-webp、byteorder-lite、moxcms、pxfm、quick-error、zune-core、zune-jpeg。

未做且不宣称完成：Tauri命令/UI绑定、证件保护下载/缓存读取链路、附件原子写入与DB引用提交、真实证件准确率/打包后目标机验收、人工确认闭环、任务调度/切片写操作。本库只提供正确可集成的独立能力。应用级缓存仍须受控目录/key、拒绝路径越界/符号链接、原子落位/失败清理，不能把本库hash验证当文件权限边界。完整集成说明见 `crates/local-ocr/README.md`。

没有Git操作、真实证件读取/上传、用户DB写入、下载模型/图片、停止用户进程，未更改其他代理所有的现有业务crate或主任务状态。
