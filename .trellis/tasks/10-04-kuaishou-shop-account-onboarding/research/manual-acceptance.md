# 真机验收清单 — 快手小店账号建号向导

面向：在**真实 Windows 桌面 + 真实快手小店账号 + 真实代理**上人工执行验收的人。
对应任务：`.trellis/tasks/10-04-kuaishou-shop-account-onboarding`（PRD AC1–AC9）。
覆盖：AC1–AC9；重点 AC3（隐藏窗口 + 二维码）、AC4、AC7、AC9。

> 说明：本清单只描述**要验证的可观测行为**与**核对方法**，不修改任何代码。
> 凡未在本机实测确认之处，均以「需实测确认」标注；未做任何平台行为臆测。

---

## 0. 验证者准备

### 0.1 必备条件

| 项 | 要求 | 说明 |
|---|---|---|
| 真实快手小店账号 | ≥1 个可扫码登录的店主号 | 需另一台手机上的快手 App 扫码 |
| 代理 | ≥1 个 HTTP/SOCKS5 代理，**出口国家已知且非中国大陆**（如 SG/JP/US） | 用于 AC7；无代理则 AC7 只能验「不崩溃」 |
| 桌面 | 单显示器或多显示器均可，但需能肉眼看到整个桌面与任务栏 | 验证「窗口是否可见」 |
| 构建 | 已构建/开发运行的 JiegeGo 应用（`crates/tauri-app`，`tauri dev` 或已安装包） | 见 §0.3 |
| 数据库工具 | `sqlite3`（本机已在 PATH） | 核对指纹落库 |
| 命令行 | PowerShell 7、`curl.exe`（Windows 自带）、`tasklist`/`taskkill` | 核对进程与 CDP |

> 若用**自动化环境（无真实桌面/无真实快手号）**：§1、§2、§4.3/§4.4、§5 的多数项**无法验证**，只有 §6 列出的离线项可通过。

### 0.2 关键路径（本机实测）

- 应用数据根目录：`%LOCALAPPDATA%\com.cloaksession.browser\`
  （Tauri identifier = `com.cloaksession.browser`，见 `crates/tauri-app/tauri.conf.json`）
- 环境数据库：`%LOCALAPPDATA%\com.cloaksession.browser\profiles.db`
- 每个环境的数据目录：`…\profiles\<profileId>\`；Chromix 引擎目录：`…\profiles\<profileId>\engines\chromix\`
- 应用设置：`…\com.cloaksession.browser\settings.json`（本机当前 `browserEngine = "chromix"`）
- 环境启动的 CDP 端口：从 9222 起逐个自增（`browser-launcher/src/driver.rs` `CDP_PORT_BASE=9222`）；Chromix 由 SDK 侧分配并回传（`browser-launcher/src/chromix.rs`）

### 0.3 启动应用（真机）

```powershell
cd F:\Cloaksession\crates\tauri-app
.\ui\node_modules\.bin\tauri.cmd dev
```

（首次需先在 `crates/tauri-app/ui` 执行 `npm install --legacy-peer-deps`。）

### 0.4 常用证据命令（本清单反复引用）

```powershell
# E1. 最近创建的环境 + 指纹三要素 + 代理国家（JSON 落库核对）
sqlite3 "$env:LOCALAPPDATA\com.cloaksession.browser\profiles.db" `
  "select id,name,proxy_country,json_extract(fingerprint,'$.locale'),json_extract(fingerprint,'$.timezone'),json_extract(fingerprint,'$.country') from profiles order by created_at desc limit 5;"

# E2. 浏览器进程与其主窗口标题/句柄（判断窗口是否存在、是否可见）
Get-Process chrome -ErrorAction SilentlyContinue | Select-Object Id,MainWindowTitle,MainWindowHandle | Format-Table -AutoSize

# E3. 主窗口坐标（判断是否被移到屏幕外）——粘贴整段运行
Add-Type @"
using System;using System.Runtime.InteropServices;
public class W { [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  public struct RECT { public int Left,Top,Right,Bottom; } }
"@
Get-Process chrome -ErrorAction SilentlyContinue | Where-Object { $_.MainWindowHandle -ne 0 } | ForEach-Object {
  $r = New-Object W+RECT; [void][W]::GetWindowRect($_.MainWindowHandle,[ref]$r)
  "{0} pid={1} rect=({2},{3})-({4},{5})" -f $_.MainWindowTitle,$_.Id,$r.Left,$r.Top,$r.Right,$r.Bottom }

# E4. 该环境的 CDP 端点（列出目标页/版本）；<port> 见 E2 的进程或 /json/version 探测
curl.exe -s http://127.0.0.1:<port>/json/version
curl.exe -s http://127.0.0.1:<port>/json/list

# E5. 残留进程排查
tasklist /FI "IMAGENAME eq chrome.exe"
Get-Process node -ErrorAction SilentlyContinue | Select-Object Id,Path
```

> 建议：每步操作同时**录屏**，并对关键画面（桌面、Alt+Tab、向导二维码、身份显示、列表行）单独截图存档。

---

## 1. 隐藏窗口（AC3 前半：不出现可见浏览器窗口）

### HW-1 启动后桌面/任务栏无浏览器窗口

- **目的**：AC3 —— 创建后浏览器窗口不得出现在用户眼前。
- **前置**：应用已运行，处于「快手 › 小店」页；已准备代理（可选）与名称。
- **操作步骤**：
  1. 点「添加账号」→ 填名称 →（可选）勾选代理并填 host/port → 点「创建并登录」。
  2. 向导进入「正在创建环境…」→「正在打开快手小店登录页…」后，**立刻离开键盘鼠标，观察整个桌面与任务栏 ≥10 秒**。
  3. 运行 E2 与 E3，记录 chrome 进程的窗口标题、句柄与坐标。
  4. 依次触发：`Alt+Tab`（按住 Alt 连续 Tab）、`Win+Tab`（任务视图）、`Win+D`（显示桌面）、点击任务栏。
- **期望结果**：
  - 桌面上**看不到**任何新的浏览器窗口；任务栏**没有**新增浏览器任务按钮（或虽有进程但无可见窗口）。
  - E2：存在 chrome 进程，但**无可见主窗口**，或 `MainWindowHandle = 0`。
  - E3：若存在窗口句柄，其坐标应为 `-32000,-32000` 附近（屏幕外）；**不应**落在任何显示器可视范围内。
  - Alt+Tab / 任务视图：见 HW-2 判定（此项本身**允许**出现，见下）。
- **失败判定 / 降级处理**：
  - 若窗口**肉眼可见且停留在屏幕内** → AC3 未达成。按 design §3.2/§6 的**可接受降级**：窗口可见但功能不阻塞，记录为「降级为可见有头」，**不阻断**其余验收；同时记录引擎（CloakBrowser / Chromix）与窗口坐标，作为缺陷证据。
  - 若窗口被移出屏幕但**坐标非 -32000**（被 WM 归一化）→ 记录实际坐标，仍算达成（以「肉眼不可见」为准）。
- **证据**：桌面/任务栏截图、E2+E3 输出、录屏。标注 `browserEngine`（settings.json）。

### HW-2 Alt+Tab / 任务视图是否出现（判定可接受性）

- **目的**：明确「屏幕外窗口」在 Windows 切换器中的可见性，决定降级是否可接受。
- **前置**：同上，向导处于「等待扫码」态。
- **操作步骤**：Alt+Tab 观察缩略图列表；Win+Tab 打开任务视图；检查任务栏预览。
- **期望结果（需实测确认，不得臆测）**：
  - **理想**：屏幕外窗口不出现在 Alt+Tab / 任务视图，或仅显示为空白/极小缩略图。
  - **可接受**：窗口出现在 Alt+Tab（因 Chromium 仍是有头窗口），但**正常视线下不可见**、不影响操作 —— 记录为「可接受」。
- **失败判定 / 降级**：若在 Alt+Tab/任务视图中出现**清晰可辨的浏览器窗口缩略图**且用户会误以为窗口在前台 → 记为**体验缺陷**（不阻断功能），升级为「隐藏方案需改进」。
- **证据**：Alt+Tab 与任务视图截图（含时间戳）。

### HW-3 验证 hidden 参数确实生效（两种引擎）

- **目的**：确认 `profiles_launch { hidden: true }` 的行为与引擎无关地「移出屏幕」。
- **前置**：能查看启动参数或进程命令行。
- **操作步骤**：
  1. 记录当前 `settings.json` 的 `browserEngine`。
  2. 分别以 `cloakbrowser` / `chromix` 启动应用各做一次 HW-1。
  3. 用 E4 拿到 CDP 端口后，用 `curl http://127.0.0.1:<port>/json/version` 确认进程存活（间接证明「有头但不可见」而非崩溃）。
- **期望结果**：
  - CloakBrowser：启动参数含 `--window-position=-32000,-32000`，**不含** `--headless`（对照 `browser-launcher/src/args.rs:157`）。
  - Chromix：SDK `launchOptions.args` 被注入 `--window-position=-32000,-32000`（`browser-launcher/src/driver.rs` `launch_with_chromix`）。
  - 两种引擎均**不出现** `--headless`（避免被风控识别为无头）。
- **失败判定**：出现 `--headless` 或窗口在屏幕内 → 记录引擎与现象；按 design §3.2 兜底说明（`--headless=new` 需实测风控影响，**不得**默认接受）。
- **证据**：进程命令行（`Get-CimInstance Win32_Process -Filter "name='chrome.exe'" | Select CommandLine`）、settings.json、CDP `/json/version`。

---

## 2. 二维码（AC3 后半：向导内可扫）

### QR-1 屏幕外窗口仍能稳定截到二维码

- **目的**：AC3 —— 窗口在屏幕外时，向导内二维码可持续刷新且可被手机识别。
- **前置**：HW-1 通过（窗口已隐藏）；向导处于「扫码登录快手小店」态。
- **操作步骤**：
  1. 观察向导二维码区域（`data-testid="wizard-qr"`）是否出现图片。
  2. 持续观察 ≥15 秒（轮询间隔 2000ms，见 `KuaishouAccountWizard.tsx` `QR_POLL_MS`），确认图片会刷新（二维码过期后应换新）。
  3. 用**另一台手机的快手 App** 对准向导内二维码扫码。
  4. 点一次「重新获取二维码」按钮，确认清空后能重新截到。
- **期望结果**：
  - 二维码图片**可见、清晰、可被手机识别**；每 ~2s 刷新一次。
  - 扫码后手机端提示登录成功。
  - 截图内容来自**屏幕外窗口**（即 CDP 截图不依赖窗口前台可见）。
- **失败判定 / 降级处理**：
  - 若二维码区长期显示「正在获取二维码…」→ 记录 `kuaishou_login_qr` 是否报错（应用日志/控制台）。可能原因：会话未就绪（命令在无会话时返回 `None`，属正常，应继续轮询）、CDP 截图失败。
  - 若图片模糊/过小导致扫不动 → 见 QR-2（整图回退）与下方「已知实现事实」。
- **证据**：向导二维码截图（含手机扫码成功截图）、应用日志。

### QR-2 裁剪区域命中 / 整图回退

- **目的**：验证 design §3.3 的「裁剪二维码 → 裁不到回退整图」。
- **前置**：同 QR-1。
- **操作步骤**：
  1. 观察向导内图片的**内容范围**：是「页面中央一块方形二维码」还是「整页截图」？
  2. 用 E4 的 CDP 端点对同一页面执行一次截图（或观察 `<img>` 长宽比），与向导内图片比对。
- **期望结果（重要 — 与当前实现对照）**：
  - **实测事实**：`kuaishou_login_qr` 当前**直接返回整页截图**（`crates/tauri-app/src/commands/kuaishou_auth.rs` 只调 `session.screenshot()`，代码中**未实现**任何裁剪/`clip`/区域常量；前端 `<img>` 仅用 CSS 限高 `max-h-[360px]`）。
  - 因此**当前行为恒为「整图回退」**：只要整页截图里二维码清晰可扫，即视为**通过**；design §3.3 的「裁剪命中」在当前版本**不适用**。
- **失败判定 / 降级处理**：
  - 若整页截图缩放后二维码像素过低、手机扫不动 → **AC3 未达成**（记为缺陷：需要实现裁剪或提高截图分辨率）。
  - 若整页截图被 `max-h-[360px]` 压得过小 → 记录为可用性缺陷。
- **证据**：向导图片截图 + 页面截图对比；注明「未裁剪，走整图回退」。

### QR-3 二维码过期 / 网络异常时不崩溃

- **目的**：AC9 相关 —— 轮询期间的瞬时失败不得中止等待。
- **前置**：向导「等待扫码」态；代理可能不稳定。
- **操作步骤**：
  1. 让二维码自然过期（不扫码）≥1 分钟，观察是否自动换新。
  2. （可选）临时断开代理，观察向导是否仍显示提示而不崩溃。
- **期望结果**：截图/检测的**瞬时失败被吞掉**，继续轮询；界面不白屏、不报致命错误。
- **失败判定**：向导卡死、崩溃或停止刷新 → 记缺陷。
- **证据**：录屏、应用日志。

---

## 3. 指纹一致性（AC7）

> **本项必须真机执行**：`fingerprint_reconcile`/`locale_for_country` 可离线测，但「代理出口地理 ↔ 指纹一致」必须用真实代理实测。

### FP-1 代理出口地理探测正确

- **目的**：确认向导以代理出口国家/时区对齐指纹（`fingerprint.rs` `fingerprint_locale_for_country` + `reconcile`）。
- **前置**：已知代理出口国家（如 SG）。
- **操作步骤**：
  1. 向导内填代理 → 点「创建并登录」。
  2. 观察创建耗时（会先做 geo 探测，ipapi.co，超时 12s）。
  3. 运行 E1，读该环境的 `proxy_country`、`locale`、`timezone`、`country`。
- **期望结果**：
  - `proxy_country` ≈ 代理实际出口国家（小写，如 `sg`）。
  - `country` = 出口国家（大写，如 `SG`，由 `reconcile` 的 country 覆盖保证与出口一致）。
  - `locale` 由 `locale_for_country(出口国)` 决定：命中目录则精确匹配（如 `ja-JP`）；否则文化邻近回退（如 SG→`en-GB`）。
  - `timezone` = 探测到的出口时区（如 `Asia/Singapore`）。
- **失败判定 / 降级**：
  - 探测失败（代理不可达）→ 向导不 reconcile，指纹保持默认（见 FP-3）。记录为「代理不可用」而非缺陷。
  - `country` 与出口国家不符 → **AC7 未达成**。
- **证据**：E1 输出、代理服务商面板/第三方 IP 查询截图。

### FP-2 浏览器运行时真实生效（关键 — 需实测确认）

- **目的**：AC7 的真正含义是**浏览器内可观测的 locale/timezone**与代理一致，而非仅数据库字段。
- **前置**：环境已在向导中隐藏启动且会话存活；能拿到 CDP 端口（E4）。
- **操作步骤**（任选其一）：
  - **A. 应用内置 MCP**（本机 `mcpHttpEnabled=true`，端口 7777，token 见 `…\com.cloaksession.browser\mcp-token`）：对目标环境调用 `evaluate_js`，读取：
    ```js
    ({ lang: navigator.language, langs: navigator.languages,
       tz: Intl.DateTimeFormat().resolvedOptions().timeZone,
       offset: new Date().getTimezoneOffset() })
    ```
  - **B. 直接 CDP**：对 E4 的页面执行 `Runtime.evaluate`（等价表达式）。
- **期望结果**：
  - `navigator.language` / `navigator.languages` 与 `fingerprint.locale` 一致。
  - `Intl.DateTimeFormat().resolvedOptions().timeZone` 与 `fingerprint.timezone` 一致。
- **失败判定 / 降级**：
  - 二者与 E1 落库值**不一致** → AC7 **未达成**（记录引擎与 `browserEngine`）。
  - **已知高风险（需实测确认，不得臆测）**：当前 `settings.json` 为 `browserEngine = "chromix"`。在 Chromix 路径下：
    - `build_spawn_args` 提前返回（`args.rs:132`），**不注入** `--fingerprint-locale/-timezone`；
    - `cdp-driver::bootstrap` 对 Chromix 直接 `return Ok(())`（`bootstrap.rs:15`）；
    - 向导创建环境时**未写入** `chromix_options`（`KuaishouAccountWizard.tsx` 的 `profilesApi.create` 仅传 `name/proxy/fingerprint`）。
    → 因此 **Chromix 下浏览器实际 locale/timezone 很可能取系统默认值，与指纹字段不一致**。必须实测确认；若确认不一致，AC7 在 Chromix 引擎下不成立，需在 CloakBrowser/CFT 引擎复测或另立缺陷。
- **证据**：`evaluate_js` / CDP 返回、E1 输出、引擎标注。

### FP-3 无代理时的行为

- **目的**：AC7 的边界（无代理不应误对齐、不应报错）。
- **前置**：向导不勾选代理。
- **操作步骤**：创建环境 → 运行 E1。
- **期望结果**：`proxy_country` 为空；指纹为默认值（`en-US` / `America/New_York` / `US`）；向导不因缺代理报错。
- **失败判定**：无代理却写入某国家，或创建失败 → 记缺陷。
- **证据**：E1 输出。

### FP-4 「指纹随机生成」是否成立（重要 — 疑似缺陷，需确认）

- **目的**：核对 PRD R2「指纹自动随机生成」。
- **前置**：连续创建 2 个不同环境（可均不带代理）。
- **操作步骤**：运行 E1，比对两条记录的 `locale/timezone/country`；必要时比对完整 `fingerprint` 字段。
- **期望结果（实测事实，与 PRD 存在偏差）**：
  - `fingerprint_generate` → `profile_manager::fingerprint::default_fingerprint(&seed)`，而向导传 `seed=""`；该函数**无随机分支**，返回**固定**指纹（`en-US`/`America/New_York`/`US`/`WindowsDesktopIntel`…）。
  - 因此**除代理 reconcile 外，指纹不随环境变化**。
- **失败判定 / 降级**：若确认「无代理时两个环境指纹完全相同」→ 与 PRD R2「随机生成」不符，记为**需求偏差/缺陷**（不阻断 AC7 的代理对齐路径，但影响多环境差异化）。
- **证据**：E1 两条记录对比、源码 `crates/profile-manager/src/fingerprint.rs`。

---

## 4. 完整主流程（AC1/AC2/AC4/AC5/AC6/AC8）

### FL-1 AC1：向导入口与字段

- **目的**：AC1 —— 「添加账号」打开**小店建号向导**，而非通用「新建浏览器环境」弹窗。
- **前置**：应用处于「快手 › 小店」。
- **操作步骤**：点「添加账号」。
- **期望结果**：弹出标题为「添加小店账号」的弹窗，字段为**账号名称 + 代理（可选）**；**不出现**通用建环境弹窗（无标签/备注/启动页/扩展/指纹等字段）。
- **失败判定**：弹出通用 NewProfileSheet → AC1 未达成。
- **证据**：弹窗截图。

### FL-2 AC2：名称必填 + 代理校验

- **目的**：AC2。
- **前置**：向导「form」态。
- **操作步骤**：
  1. 不填名称直接点「创建并登录」。
  2. 填名称，勾选代理，**只填类型不填 host**，点创建。
  3. 填写合法 host 后创建。
- **期望结果**：
  - 步骤 1 → 提示「请填写账号名称」，**不进入下一步**。
  - 步骤 2 → 提示「请填写代理地址」，不创建。
  - 步骤 3 → 正常创建。
  - 代理 host 支持粘贴 `host:port:user:pass` 自动拆分（可选核对）。
- **失败判定**：空名称也能创建、或非法代理被静默接受 → 记缺陷。
- **证据**：各态截图。

### FL-3 AC4：扫码成功后显示身份

- **目的**：AC4。
- **前置**：向导「等待扫码」态；真实快手号可扫。
- **操作步骤**：用快手 App 扫码并在手机上确认登录；观察向导。
- **期望结果**：向导**自动**切换到成功态，显示「已识别账号」+ **头像 + 昵称 + 快手ID**，并出现「完成」按钮；无需手动点刷新。
- **失败判定**：
  - 扫码后长时间不切换 → 检查身份检测（`kuaishou_identity_detect`）；注意向导自身每 2s 轮询检测。
  - 只显示 ID 无头像 → 记录（头像取自小店顶栏，可能因页面未就绪而缺）。
- **证据**：成功态截图（含 ID/昵称/头像）。

### FL-4 AC5：完成后小店列表出现该账号

- **目的**：AC5。
- **前置**：FL-3 成功。
- **操作步骤**：点「完成」→ 向导关闭 → 回到小店列表。
- **期望结果**：列表出现该账号，**头像 / 快手ID / 检测状态**正确（状态为「本轮已识别」）。
- **失败判定 / 说明**：
  - 列表行由 `profiles_list` + 身份 provider（每 15s 轮询）驱动；向导 `onCreated` 在**创建时**（扫码前）已触发一次刷新，因此**快手ID/头像可能延迟至多 ~15s** 才出现。可等待或切换页面刷新后再判定；若 >30s 仍不出现 → 记缺陷。
- **证据**：列表截图（含状态列）。

### FL-5 AC6：创建后取消 → 环境保留且状态未识别

- **目的**：AC6。
- **前置**：新建一个环境（不扫码）。
- **操作步骤**：在「等待扫码」态点「取消」（或按 Esc / 点遮罩）。
- **期望结果**：
  - 向导关闭；向导内应提示「账号已创建，可稍后在小店列表继续登录。」。
  - 小店列表**仍能看到该账号**（环境保留）。
  - 状态列为**未识别**（当前文案为「未检测」，见 `kuaishou.shop.status.unknown`）。
- **失败判定**：环境被删除、或列表不出现 → AC6 未达成。
- **证据**：取消前/后截图、E1 显示该环境仍在。

### FL-6 AC8：重复快手ID警告但不阻断

- **目的**：AC8。
- **前置**：**已存在**一个环境 A，其身份已被识别为某快手ID（在 provider 中可见）。再新建环境 B，扫**同一**快手号。
- **操作步骤**：按 FL-3 让 B 读到与 A 相同的快手ID；观察向导。
- **期望结果**：向导显示警告文案「该快手ID已在另一个环境使用：<id>」，**但「完成」按钮仍可用**，可正常完成。
- **失败判定 / 说明**：
  - 无警告 → 确认 A 的身份是否已进入 provider（`kuaishou_identity_list`）；provider 未收录则不会触发查重（需先让 A 被检测到）。
  - 警告中的 `<id>` 当前是**另一个环境的 profileId**（源码 `duplicateOf` 返回 `otherId`），不是环境名 —— 记录为文案可用性观察（不阻断）。
  - 若警告**阻断**完成 → AC8 未达成。
- **证据**：警告截图、完成后的列表截图。

---

## 5. 资源清理（AC9）

### CL-1 登录成功后轮询停止

- **目的**：AC9。
- **前置**：向导「等待扫码」态。
- **操作步骤**：扫码成功进入「完成」态后，观察是否仍每 2s 变化；可用 DevTools（`Ctrl+Shift+I`）观察网络/IPC 调用频率。
- **期望结果**：进入 done 后**不再**发起 `kuaishou_login_qr` / `kuaishou_identity_detect` 轮询（`useEffect` 依赖 `step`，`step!=="waiting"` 时清理 `setTimeout`）。
- **失败判定**：done 态仍持续截图 → 记缺陷。
- **证据**：DevTools 截图/录屏、应用日志。

### CL-2 关闭向导后隐藏窗口被关闭、无残留

- **目的**：AC9 + PRD「待定项（倾向关闭）」。
- **前置**：任一「等待扫码」态（无论是否扫码成功）。
- **操作步骤**：
  1. 点「完成」或「取消」关闭向导。
  2. 运行 E5 与 E2，确认浏览器进程与窗口是否消失。
  3. 观察应用 CPU/网络是否恢复空闲（无持续截图）。
- **期望结果**：
  - 向导关闭时调用 `profiles_close`，隐藏窗口/进程被关闭；E5 无对应 chrome 进程残留。
  - 无持续轮询定时器（`Modal` 在 `!open` 时返回 `null`，子组件卸载触发清理）。
- **失败判定 / 说明**：
  - `close()` 是**异步**的：先 `await profilesApi.close(id)` 再 `onClose()`；关闭瞬间可能有短暂延迟。若进程**长期残留** → 记缺陷。
  - 注意**伴随轮询器**（companion poller，600ms 一次）在环境运行时存在，环境关闭后应自行退出（`driver.is_running` 为假即退出）。
- **证据**：关闭前后 `tasklist` 对比、录屏。

### CL-3 向导未创建环境时取消无副作用

- **目的**：PRD R7 前半（未创建前取消＝无副作用）。
- **前置**：向导「form」态（尚未点创建）。
- **操作步骤**：点「取消」。
- **期望结果**：向导关闭；**不产生**任何新环境（E1 记录数不变）、不启动任何浏览器。
- **失败判定**：出现多余环境/进程 → 记缺陷。
- **证据**：取消前后 E1 对比。

### CL-4 建号失败时环境不被误删

- **目的**：稳健性（PRD R7 语义）。
- **前置**：可用一个**不可达代理**制造创建后启动失败。
- **操作步骤**：填不可达代理 → 创建 → 观察。
- **期望结果**：若环境已创建但启动失败，向导停留在可继续扫码的状态（源码 `submit` 的 catch 在 `createdId` 存在时回到 `waiting`），**环境保留**。
- **失败判定**：环境被删除或向导直接崩溃 → 记缺陷。
- **证据**：错误提示截图、E1。

---

## 6. 自动化环境已可覆盖（无需真机）

以下由离线测试覆盖，真机验收时**只需确认测试通过**即可，不必人工重复：

| 项 | 覆盖位置 | 说明 |
|---|---|---|
| hidden 只加 `--window-position`、不加 `--headless`、其余参数不变 | `crates/browser-launcher/tests/args.rs::hidden_launch_moves_the_window_off_screen_without_headless` | `cargo test -p browser-launcher --test args` |
| Chromix 参数不继承 Cloak 指纹/代理策略 | 同文件 `chromix_args_do_not_inherit_...` | 同上 |
| 向导 UI 契约：名称必填、创建后显示二维码、识别后显示完成、关闭时调 `profiles_close`、`profiles_launch` 带 `hidden:true` | `crates/tauri-app/ui/tests/kuaishou-shop-wizard.spec.ts` | `npm --prefix crates/tauri-app/ui test`（Playwright，全为本地 fake，不启动浏览器） |
| i18n 键集一致 | `crates/tauri-app/ui/src/i18n/dictionaries.test.mjs` | 随 UI 测试运行 |
| 指纹 locale/timezone/country 落库 | `crates/profile-manager/tests/manager.rs` | `cargo test -p profile-manager` |

**运行命令**：

```powershell
cargo test -p browser-launcher --test args
cargo test -p profile-manager
cd crates\tauri-app\ui; npx playwright test tests/kuaishou-shop-wizard.spec.ts
```

> 注意：上述 UI 测试使用**浏览器本地 mock**，**不验证**真实隐藏窗口、真实二维码、真实代理地理——那些**必须真机执行**。

---

## 7. 必须真机人工执行 vs 已可自动化

**必须真机人工执行（自动化不可替代）**：

- §1 全部（隐藏窗口可见性、Alt+Tab/任务视图、窗口坐标）
- §2 全部（屏幕外窗口的二维码截图与可扫性）
- §3 FP-1/FP-2/FP-3/FP-4（真实代理出口地理 ↔ 指纹/运行时一致性）
- §4 全部（真实扫码、真实身份、列表与取消语义、重复警告）
- §5 全部（真实进程/定时器清理）

**已可自动化**：§6 表列各项。

---

## 8. 已知无法验证 / 高风险点（需实测确认，不得臆测）

1. **Chromix 下指纹一致性（FP-2）高风险**：向导不写 `chromix_options`，Chromix 路径不注入 locale/timezone 参数、bootstrap 对 Chromix 为空操作。当前默认引擎为 Chromix，**AC7 极可能不成立**。需实测确认，并在 CloakBrowser/CFT 复测对照。
2. **「指纹随机生成」不成立（FP-4）**：`default_fingerprint` 为固定值，向导 `seed=""`。与 PRD R2 有偏差。
3. **二维码恒为整图截图（QR-2）**：design §3.3 的裁剪未实现；整页缩放后二维码可扫性依赖页面布局，页面改版可能失效（风险 R-b 常驻）。
4. **Alt+Tab / 任务视图可见性（HW-2）**：Windows 对屏幕外有头窗口的处理**未实测**，不可假定其一定不出现。
5. **截图轮询频率与登录超时对齐（风险 R-c）**：向导轮询由 UI 控制，未见与 `KS_LOGIN_TIMEOUT` 的显式对齐；长时间停留可能持续截图。需实测观察资源占用。
6. **无代理/多显示器/缩放（DPR）环境**：窗口坐标 `-32000` 在多显示器或高 DPI 下是否仍安全移出屏幕，**需实测确认**。
7. **重复警告文案**（FL-6）：显示的是 profileId 而非环境名，属可用性观察，非功能阻断。

---

## 9. 验收结论记录表（执行时填写）

| 编号 | 对应 AC | 结果(通过/失败/降级) | 证据链接 | 备注 |
|---|---|---|---|---|
| HW-1 | AC3 |  |  | 引擎： |
| HW-2 | AC3 |  |  |  |
| HW-3 | AC3 |  |  |  |
| QR-1 | AC3 |  |  |  |
| QR-2 | AC3 |  |  | 整图回退 |
| QR-3 | AC9 |  |  |  |
| FP-1 | AC7 |  |  |  |
| FP-2 | AC7 |  |  | 引擎： |
| FP-3 | AC7 |  |  |  |
| FP-4 | R2 |  |  |  |
| FL-1 | AC1 |  |  |  |
| FL-2 | AC2 |  |  |  |
| FL-3 | AC4 |  |  |  |
| FL-4 | AC5 |  |  |  |
| FL-5 | AC6 |  |  |  |
| FL-6 | AC8 |  |  |  |
| CL-1 | AC9 |  |  |  |
| CL-2 | AC9 |  |  |  |
| CL-3 | R7 |  |  |  |
| CL-4 | R7 |  |  |  |
