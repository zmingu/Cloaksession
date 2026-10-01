# 2026-10-01 公开头像浏览器GET修复

## 最小边界与授权

用户授权仅修复头像缓存。无主体/切片/UI/IPC/DB schema/登录数据改动，无Git、用户进程关闭、页面导航、active切换、页面截图、宿主直连或平台写操作。实际用户Chrome151及原CDP页面target已只读重新确认；未在用户页面运行新GET，未查询/写入其DB。

行为差：已加载跨域img的canvas污染，且AvatarRoute::Skip阻止宿主HTTP下载时，现在可通过固定TaskPage执行一次浏览器自身网络公开头像GET。保留显式代理下载原路径，无浏览器失败后二次下载。成功内容复用现有base64/2MiB/raster magic/hash/路径缓存和DB提交世代校验。

## 方法选择

未采用Page.getResourceContent（前史额外请求主文档），未采用Network.loadNetworkResource（Chromium实现跟随重定向且OnDataReceived先累积全量内容，IO.read不是上游下载限流）。no-cors + redirect:error本地Chrome151不发出请求并失败。

实际采用Rust chromiumoxide：固定main frame创建无universal access的隔离world，原生fetch GET mode:cors/credentials:omit/redirect:error/referrerPolicy:no-referrer；只在CDP中读取响应，原fetch最终被取消，不向网页暴露跨域数据。仅精确转义URL且resourceType=Fetch的响应阶段拦截，Network.requestWillBeSent的frame+GET+URL+专属sourceURL标记关联networkId。拒绝非200/Location/重定向/Content-Length超限，Fetch.takeResponseBodyAsStream + IO.read每次至多32KiB，累计最多2MiB再编码返回，之后同账号/URL/documentEpoch/加载状态前后复核。

取消后Rust工作任务继续有界清理自己的AbortController/IO句柄/Fetch请求/Fetch域；独立session/target弱锁持有到清理结束，防止下个头像任务的Fetch配置被旧任务清理覆盖。不关闭标签或浏览器。CDP连接异常时清理是best effort；未确认Fetch.disable不返回成功。不保证撤回已发送CDP或已开始的GET。

## 代码范围

- crates/cdp-driver/src/avatar_resource.rs：窄公开头像GET、域/流大小策略、清理。
- cdp-driver src/lib.rs/bound_page.rs/task_page.rs/session.rs：导出、固定TaskPage受控调用和清理锁。
- cdp-driver/Cargo.toml及Cargo.lock：复用本地已锁定base64 0.22，无下载。
- tauri-app src/driver/identity/extract.rs：复用域/大小策略及前后复核；mod.rs：仅Skip头像路径接入，文字明确可能请求公开头像。
- cdp-driver/tests/identity_browser.rs：已有隔离fixture扩展，读取已安装Chrome151 binary，两个独立CDP连接分别拥有fixture fulfill与生产读取。用户环境不是fixture。

## 最终验证（2026-10-01）

- `cargo check --workspace --locked`：通过，无警告。
- `cargo test -p cdp-driver -p tauri-app --locked`：97通过、0失败、3忽略（两个专用fixture和旧导航/关浏览器integration；含compile-fail doctest）。
- 设置 `RUN_IDENTITY_BROWSER_FIXTURE=1`、`IDENTITY_TEST_CHROMIUM=C:\Users\Administrator\.cache\chromix\v151.0.7922.173\win-x64\chromix\chrome.exe`，运行 `cargo test -p cdp-driver --locked --test identity_browser -- --ignored --nocapture`：两个独立临时浏览器fixture均通过，0失败。未执行旧integration测试。
- 实测引擎Chrome/151.0.7922.173。原canvas-only回归仍证明getResourceContent HIT引起Fetch3→4、Network3→3；新GET测试证明响应流无CORS成功及所有异常拒绝。超大检查另外断言在2秒内拒绝，不能靠3秒总超时假通过。
- 编译期间只等待Cargo文件锁，没有终止任何进程。无下载、Git、用户DB写入、UI改动或用户浏览器重启。测试只回收自己的Child及临时user-data-dir。
- 开发中失败的原型和代理fixture已修正，不冒称从未失败：no-cors+redirect:error不可用；fixture全DNS阻断最初也拦了本地代理解析，最终只排除127.0.0.1，未开放任何外部DNS。最终命令全通过。

真实fixture覆盖：无CORS头像成功、精确不可信URL不发出请求、302无目标请求、声明超大与实际超大流拒绝、换号/同URL重载丢弃、取消/超时拒绝、清理后再次读取成功、无新target。正向代理证据：生产GET到从未缓存的fixture头像URL，只继续至本地独占非转发代理；捕获CONNECT fixture.yximgs.com:443。除127.0.0.1（本机代理）外DNS全阻断，测试不转发外网请求。

## 必須独立复核/未验收

- 真实用户头像CDN、真实代理及原生Tauri重启后的avatarKey/UI尚未验收。本次不擅自重启应用。
- 用户真实页面CSP/服务工作者/引擎差异可能导致浏览器GET失败；保持头像失败不影响身份detected。
- Fetch状态属于该CDP session：协作TaskPage头像操作有额外清理锁，但不承诺与同session任意raw Fetch配置并发兼容（现有生产未启用其它Fetch业务）。其它CDP连接各自独立。
- HTTP流大小检查无法撤回浏览器网络栈已经预读的字节；上限约束进入应用缓存的数据，IO逐块读取和取消用于尽早终止。
- UI未改，不运行UI构建。未宣称真实平台业务验收。
