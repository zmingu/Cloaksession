# 并行收尾A：已加载头像 fallback

2026-09-30，任务继续 `in_progress`，交主代理复审。无 Git、真实用户浏览器、平台/CDN请求；本代理仅执行本地 wire/Rust 测试。独立真实引擎 fixture 由并行 C 管理。

## 关键结论：拒用 getResourceContent

不能把 `Page.getResourceContent` 当成纯缓存读取。C 的专用 Chrome153、本地 Fetch 全拦截 fixture 中：

- `Page.getResourceTree` 无新增请求；读取已经加载的跨域 PNG 时，`getResourceContent` 返回 PNG base64，**却额外请求了主文档** `https://s.kwaixiaodian.com/a`：Fetch 3→4、Network 3→3。
- 不存在的图片 URL 返回 `-32000 No resource with given URL found`，Fetch 4→4；MISS 无新增不能抵消 HIT 的额外请求证据。
- 已加载 img 的 canvas 实验无新增 Fetch/Network；no-CORS 污染抛 SecurityError，有 CORS 可输出 PNG。

因此按用户允许的降级方案，只使用已经加载的 `img` 像素；**未新增 cdp-driver 资源读取 API，未使用 getResourceContent，也未改默认 reqwest 直连策略**。原 TaskPage::evaluate 的受控 future、锁、&mut、timeout/cancel 足够承担此窄用途。

## 实现与 API

- `crates/tauri-app/src/driver/identity/extract.rs`
  - `detect_with_avatar_target(session,cancel) -> Result<(Detection,Option<String>),String>`：将同次观察选中头像的 target 元数据单独返回。`PageIdentity` 和 `extract.js` 四字段 wire DTO 不变，现有测试 `detect` 包装保留。
  - `avatar_probe_expression(identity,url,pixels) -> String`：独立 probe 组合生产 EXTRACTOR；准确主 frame、账号DOM、ID、精确 currentSrc、complete/natural dimensions 读取；同步绘制前后复核，无 img src/crossOrigin 修改、无 new Image/fetch/页面截图。
  - `AvatarPage::open(session,target,identity,url,cancel)` / `content()` / `revalidate()`：保持原target TaskPage租约跨本地IO/明确代理等待；复核ID/URL/加载状态、页面URL与performance.timeOrigin。无原始Page或Deref暴露；每次操作500ms锁等待/2s读取。
  - 头像 URL 校验由此模块统一拥有，avatar.rs 复用：HTTPS443、yximgs.com及子域、无userinfo；file/data/其他域/显式.svg/.svgz拒绝。
- `crates/tauri-app/src/driver/identity/avatar.rs`
  - `AvatarCache::store_base64(content)`：先检查编码长度/四字节分组及padding推算<=2MiB，再STANDARD严格解码和既有magic/hash/路径约束；不是任意文件写入API。
- `crates/tauri-app/src/driver/identity/mod.rs`
  - 有效本地成功缓存仍优先复用；新头像先读浏览器已加载图片，失败只尝试原明确代理或跳过；5s预算内复核后才关联当前ID。
  - before/after当前slot与取消检查，既有保存时UUID/Arc、业务状态、截止期、Profile存在性保护不变。账号/图片/文档/会话变化时不引用新头像，ID读取不因普通头像失败而失败。
  - 提示区分浏览器已加载像素、明确代理下载、已验证本地复用。三个Tauri IPC、avatarKey和本地data URI合同不变。
- `crates/cdp-driver/tests/cached_resource.rs`：复用tests/common，新增6项wire测试；同时编译生产reader的3项纯测试。验证固定target/租约、取消（含in-flight）/期限、同次target元信息、不可信URL与payload大小、读取后ID/URL/文档/加载状态变化丢弃、像素不可读为None。
- `crates/cdp-driver/tests/kuaishou_identity.rs`：仅对按路径导入的生产reader加测试局部dead_code说明（该binary不使用新增头像helper，专属cached_resource负责）；无测试行为修改。
- 规范：`cdp-driver/backend/task-control.md`、`tauri-app/backend/kuaishou-identity.md` 已记录拒用CDP资源读取的原因与canvas能力边界。已有implement.md追加执行说明，没有建新任务。

## 验证记录

- `cargo test -p cdp-driver --locked --test cached_resource`：9通过/0失败（6 wire+3 reader），后续in-flight取消增强已单独补入，完整复跑见下。
- `cargo test -p tauri-app --lib --locked`：最新 **34通过/0失败/0忽略**，包含并行B的4项生命周期测试；A自身新增2项base64/cache测试。`avatar-tauri-test.log`。
- `cargo check -p tauri-app --locked`：通过。`avatar-check.log`。
- `cargo test -p cdp-driver --locked`：最终 **56通过/0失败/2忽略**，含in-flight取消增强；无编译警告。`avatar-cdp-test.log`。中途曾遇并行C正在编辑fixture的 `OwnedBrowser.network_sink` 初始化中间态编译错误，已先沟通、未覆盖其文件；C完成后22:06复跑成功。
- C独立 `identity_browser.rs` 最终专用Chrome153.0.8010.12实测：1通过/0失败（3项纯测试filtered）。生产AvatarPage无CORS为None、有CORS为PNG、读取后改ID/同URL重载epoch变化拒绝，Fetch/Network均无新增；黑洞代理、临时profile与进程清理由C负责，详见同目录 `results-browser-fixture.md`。A未运行ignored或连接浏览器。

## 能力与未验收范围

- **不保证常见 no-CORS 平台头像可用**：跨域污染会跳过，无明确代理时仍可能只显示ID。这是拒绝额外网络读取后的有意限制，不伪称完整浏览器缓存支持。
- 只绘制已加载图片到未插入DOM的canvas，不包含遮罩、浮层或整页。每维<=2048，输出是重新编码的PNG，可能不同于原文件字节/动图；hash针对输出字节。
- 没有读取原资源响应，因此不验证原文件Content-Type；拒绝显式SVG地址，保存/返回的payload始终须通过栅格magic。未知扩展图片经canvas只输出PNG，不返回SVG文本。
- 真实平台选择器、原生Tauri联调、真实代理/CDN流量仍未验收。页面自身脚本行为不是TaskPage的沙箱承诺；本地fixture证据不能声称所有浏览器版本的绝对网络行为。
- 迟到/取消时可能留下未引用的内容hash缓存文件；不会关联到另一账号或经avatar IPC任意读取，沿用原DB引用保护。
