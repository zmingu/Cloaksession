# C：隔离真实 Chromium 身份 fixture 验证

## 范围与结论

2026-09-30（Windows / PowerShell）。只新增 `crates/cdp-driver/tests/identity_browser.rs` 与本文；不改 Cargo.toml/lock、生产代码或其他代理测试，不执行 Git，不访问用户浏览器/账号/数据，不运行旧 `integration` ignored 测试。总 TODO 由主代理管理，本代理没有新增/改写 TODO。

**已实际执行并通过**真实 Rust `BrowserSession` / chromiumoxide / `TaskPage` 集成测试。生产 `extract.js` 由 `include_str!` 编译嵌入；生产 `extract.rs` 按现有测试的 `#[path] mod reader` 方式复用（无反向 crate 依赖）。额外整合了 A 的生产 `AvatarPage::open/content/revalidate`。没有 Node/Playwright 业务自动化；Node 仅只读查询安装的 Chromium 路径。

**重要反例：拒绝将 `Page.getResourceContent` 宣称为无请求缓存读取。** 即便 1×1 PNG 已加载且 `getResourceTree` 含此资源，Chrome 153 的 `getResourceContent` 仍再次请求主文档。Fetch 计数增加，Network 计数却不增加。首轮严格“零新增 Fetch”断言实际失败，后经拆分证明不是 canvas 导致，已同步 A；A 采用只读已加载 img 的 canvas 降级。最终测试明确断言并记录该反例，而不是删掉计数假装缓存无请求。

## 隔离与清理

- 显式 `RUN_IDENTITY_BROWSER_FIXTURE=1` 与 `IDENTITY_TEST_CHROMIUM=<exe>`，真实浏览器用例默认 `#[ignore]`；缺 opt-in 明确失败，不空跑伪通过。
- 新建唯一 `cloaksession-identity-fixture-{pid}-{timestamp}` 临时 user-data-dir；Chromium headless、无启动页、无扩展/同步/后台更新。
- `--remote-debugging-address=127.0.0.1 --remote-debugging-port=0`；只读取**自己的目录**中的 DevToolsActivePort，绝不扫描/使用既有 CDP endpoint。
- 两个空白页创建后、首次 fixture 导航前，先安装 `Fetch.enable` 的全 URL Request-stage 拦截。精确 fixture URL 用内存 HTML/PNG fulfill，其余 fail；没有 `continueRequest`、公网 HTTP 客户端或真实平台/CDN请求。
- 纵深阻断：`--host-resolver-rules=MAP * ~NOTFOUND`、禁 QUIC；浏览器代理指向测试独占的动态 loopback `TcpListener` 黑洞，持有至 Child 清理结束，无 accept/forward 代码，且禁隐式 loopback 代理绕过。不依赖固定端口“应该无服务”的假设。不修改 hosts/证书信任。
- 测试仅 setup 或模拟独立页面行为时导航 fixture；生产检测/头像 API 的区间断言不新增请求。URL 看似 HTTPS 平台，内容实际上全部本地 CDP fulfill。
- `OwnedBrowser` RAII 在成功或 Rust panic 时对本次 Child `kill/wait`，只删除自己创建的目录；事件任务 Drop 时 abort。不调用会关闭既有 session 的旧测试，不使用全进程名 kill。
- 无截图、无用户内容持久化。最终明确验证临时目录删除，并只读检查该测试前缀目录与 Chrome 命令行，均无残留；首轮失败后的残留检查也为空。

## 覆盖项

1. 固定两个真实 target 并行 `TaskPage::evaluate(EXTRACTOR)`：A=`12345678`/`Fixture Alpha`，B=`87654321`/`Fixture Beta`；头像来自账号区。正文商品 ID=`9999999999`、商品 id class=`8888888888` 均不误取。
2. 实际生产 `reader::detect` 对两个不同 ID 返回 Conflict；active 仍 A，A visibilityState 不变，始终仅两页，检测前后 Fetch 计数不变。
3. 无 ID 与非法 ID：短 ID、ASCII 字母混入、全角数字、33 位数字、商品文本前缀均拒绝；空 ID 不返回 nickname/avatar；双方无 ID 时生产聚合返回 NoId。
4. 假冒相似域 `s.kwaixiaodian.com.evil.invalid`：生产 JS 返回 null ID，Rust `decode` 返回错误。
5. 两张有效 1×1 PNG 真正加载解码（complete + naturalWidth==1），一张普通跨域 img、一张已有 crossorigin=anonymous 的 img。
6. typed `Page.getResourceTree` 找到图片；typed `Page.getResourceContent` 返回精确 fixture base64（92 字符），但触发主文档额外 Fetch；不存在资源返回 -32000 且不新增 Fetch。
7. 原生 canvas 实验：普通跨域 img 为 SecurityError，CORS img 输出 PNG；该区间 Network/Fetch 均严格不增。
8. A 的**实际生产 AvatarPage**：普通跨域头像 content=None、revalidate 成功；把已加载 CORS img 移入 fixture 账号区（不改 src/不加载新图）后 content 为 PNG；改账号 ID 后 revalidate 失败，整个 API 区间 Network/Fetch 均不增。
9. A 的 documentEpoch：在保留旧 AvatarPage 时由测试模拟同 URL/同账号重新载入，旧观察 revalidate 失败；revalidate 本身无新增 Fetch。
10. 不匹配 `https://blocked.invalid/probe` 被 fail，fetch Promise 返回 blocked；每页 paused == fulfilled + failed，拦截任务未提前退出。

## 可重放命令

仓库：`F:\Cloaksession`。

只读定位（在 `crates\tauri-app\ui`）：

```powershell
node -e "console.log(require('playwright').chromium.executablePath())"
```

本轮安装路径：`C:\Users\Administrator\AppData\Local\ms-playwright\chromium-1243\chrome-win64\chrome.exe`。
CDP `/json/version` 实测：`Chrome/153.0.8010.12`。

```powershell
$env:IDENTITY_TEST_CHROMIUM='C:\Users\Administrator\AppData\Local\ms-playwright\chromium-1243\chrome-win64\chrome.exe'
$env:RUN_IDENTITY_BROWSER_FIXTURE='1'
cargo test -p cdp-driver --locked --test identity_browser -- --ignored --nocapture
# 普通命令只跑按路径引入的纯单元测试，不启动浏览器：
cargo test -p cdp-driver --locked --test identity_browser
cargo check -p cdp-driver --tests --locked
Remove-Item Env:IDENTITY_TEST_CHROMIUM, Env:RUN_IDENTITY_BROWSER_FIXTURE
```

仅格式化本文件（防止 rustfmt 跟随 #[path] 修改其他代理的生产文件）：

```powershell
rustfmt --edition 2021 --config skip_children=true crates/cdp-driver/tests/identity_browser.rs
```

## 最终真实执行证据

22:04 最终独占网络黑洞版本：命令 exit 0，真实用例 **1 passed / 0 failed / 3 filtered out**，耗时 1.65s；普通用例 **3 passed / 0 failed / 1 ignored**；`cargo check -p cdp-driver --tests --locked` 通过，无 warning。3 个纯测试来自按路径引入的生产 reader 测试模块，不是 3 个浏览器用例。

```text
browser="Chrome/153.0.8010.12"
owned_pid=14040
temp=C:\Users\ADMINI~1\AppData\Local\Temp\cloaksession-identity-fixture-37900-1790777053663118000
Fetch.requestPaused url=https://s.kwaixiaodian.com/a
Fetch.requestPaused url=https://fixture.yximgs.com/cors.png
Fetch.requestPaused url=https://fixture.yximgs.com/avatar.png
Fetch.requestPaused url=https://s.kwaixiaodian.com/b
Fetch.requestPaused url=https://fixture.yximgs.com/cors.png
Fetch.requestPaused url=https://fixture.yximgs.com/avatar.png
Fetch.requestPaused url=https://s.kwaixiaodian.com/a
getResourceContent HIT: Fetch before=3 after=4; Network before=3 after=3
getResourceContent MISS: error=Some(Chrome(Error { code: -32000, message: "No resource with given URL found" })) Fetch before=4 after=4
cache resource: base64=true bytes(base64)=92 network_before=3 network_after=3 canvas_no_cors=SecurityError canvas_cors=PNG
production AvatarPage: no-CORS=None; loaded CORS=PNG; changed ID rejected; Fetch/Network unchanged
Fetch.requestPaused url=https://s.kwaixiaodian.com/no-id
Fetch.requestPaused url=https://s.kwaixiaodian.com/invalid
Fetch.requestPaused url=https://s.kwaixiaodian.com.evil.invalid/wrong
Fetch.requestPaused url=https://fixture.yximgs.com/cors.png
Fetch.requestPaused url=https://fixture.yximgs.com/avatar.png
Fetch.requestPaused url=https://s.kwaixiaodian.com/a
production AvatarPage: same-URL reload rejected via documentEpoch; revalidation issued no request
Fetch.requestPaused url=https://blocked.invalid/probe
target=a paused=6 fulfilled=5 failed=1 network=7
target=b paused=8 fulfilled=8 failed=0 network=12
cleanup: own child killed/reaped; temporary profile removed; no screenshots or user data
test identity_in_disposable_chromium ... ok
LEFTOVER_TEST_PROFILES:
LEFTOVER_TEST_PROCESSES:
```

## 限制与交接

- 不是当前真实小店 DOM/账号/服务端认证验收，不证明真实站点保留这些选择器。没有运行原生 Tauri 完整联调或真实 CDN/代理下载。
- Chromium 使用 Chromix 枚举绕开 CloakBrowser 已知域策略风险，但本轮可执行文件是 Playwright 安装的普通 Chromium；不宣称验证了 Chromix SDK/CloakBrowser 指纹或引擎兼容性。
- `getResourceContent` 的额外请求断言是当前 Chrome153 fixture 的负面回归证据；未来引擎行为变化可能使此断言失败，应重新审查证据，不直接改成零请求成功。MISS 无请求也不抵消 HIT 反例。
- canvas 的无 CORS 失败是预期保守降级。没有改 crossorigin、重新赋 src、另建 Image、绕过跨域限制或切到截图。
- C 无生产修改；A 新 API 已直接整合并实跑通过。主代理负责最终 workspace 联合验收和 TODO，C 不关闭任务。
