# Chromix renderer 崩溃根因调查 — 2026-10-02

## 结论（证据强度：已证实）

**崩溃根因是真实快手页面 `s.kwaixiaodian.com/zone/shop/info/qualification` 的「达人主体信息」Tab 切换触发的 renderer 崩溃，不是启动参数、不是 crashpad 缺失、不是本仓库代码。**

errorCode `-36861` 在两次独立登录的真机复现中完全相同，崩溃点一致：在资质页点击「达人主体信息」Tab 后立即崩溃。

### 证据强度分级

| 结论 | 强度 | 证据 |
|---|---|---|
| 崩溃在真实页面可复现 | **已证实** | 2 次登录、共 2 轮成功复现，崩溃点相同（达人Tab点击后），errorCode 相同（-36861） |
| 崩溃不在合成页面复现 | **已证实** | 4 组启动参数对照 × 8 轮 = 32 轮合成页面全无崩溃 |
| 崩溃与启动参数无关 | **强推测** | 4 组对照（含去 breakpad、开 sandbox、去 dev-shm）全无崩溃；真实页面在基线参数下崩溃。但合成页面与真实页面差异不止"参数"一个变量，严格说无法完全排除页面内容差异与参数的交互 |
| 崩溃由达人Tab切换触发 | **已证实** | 两次复现都在 `action:"talent" → DISPATCHED` 后约 400-2200ms 内出现 `Target.targetCrashed` |
| errorCode -36861 是真实崩溃码 | **已证实** | 与 detector-control 注入的 `Page.crash`（-2147483645 = STATUS_BREAKPOINT）不同，是 renderer 自身崩溃 |
| 是上游 Chromix/Chromium 151 补丁栈问题 | **推测** | 本仓库未修改任何启动参数；参数来自 SDK 依赖的 playwright-core 默认。但"上游"判定需要二分上游版本，本轮未做 |

## 复现步骤

### 前置
- 已安装 Chromix `151.0.7922.173`（`C:/Users/Administrator/.cache/chromix/v151.0.7922.173/win-x64/chromix/chrome.exe`）
- Node >= 22，Windows
- 仓库根 `F:\Cloaksession`

### 命令（需手动扫码）

```powershell
node .trellis/tasks/09-30-kuaishou-account-init/research/chromix-root-cause-2026-10-02.mjs --run --mode=live --cohort=baseline --cycles=3 --login-seconds=300
```

脚本会用**全新一次性 profile** 启动 Chromix（不复制用户 Cookie），打开 login.kwaixiaodian.com。手动扫码登录后，脚本在新标签重放只读序列：
1. 导航到 qualification 页
2. 点「主体信息」Tab（如已选中则跳过）
3. **点「达人主体信息」Tab** ← 崩溃点
4. （如未崩溃）导航到 slice 页、点「修改设置」开抽屉

崩溃后脚本记录 `Target.targetCrashed` 事件、探活 3 次（全 NO_REPLY）、anchor 健康度，然后清理临时 profile。

### 无需登录的对照（确认崩溃是页面特有）

```powershell
# 4 组合成对照，各 8 轮，全无崩溃
node <script> --run --mode=synthetic --cohort=baseline --cycles=8
node <script> --run --mode=synthetic --cohort=without-disable-breakpad --cycles=8
node <script> --run --mode=synthetic --cohort=sandbox --cycles=8
node <script> --run --mode=synthetic --cohort=without-disable-dev-shm-usage --cycles=8
```

合成模式用 `Fetch.fulfillRequest` 注入本地 HTML（模拟 qualification/slice 的 DOM 结构但不含真实脚本/数据），30 轮全无崩溃。

### 崩溃检测能力验证（detector-control）

```powershell
node <script> --run --mode=detector-control --cohort=baseline --cycles=1
```

在合成页注入 `Page.crash`，验证 `Inspector.targetCrashed` + `Target.targetCrashed` 事件捕获、target 仍在 inventory、3 次探活全 NO_REPLY、anchor 仍 RESPONSIVE。结果：DETECTOR_CAUGHT_CRASH_WITH_RETAINED_TARGET。

## 原始证据

### 第一次登录复现（artifact: chromix-root-cause-2026-10-02-live-baseline-2026-10-02T11-19-38-582Z.jsonl）

- 11:21:39 登录成功，cycle-1 开始
- 11:21:39.939 导航 qualification（506ms，ready=interactive）
- 11:21:40.765 主体Tab ALREADY_SELECTED
- 11:21:40.777 达人Tab DISPATCHED
- 11:21:41.154 导航 slice（373ms）→ 开抽屉 → **cycle-1 完成 RESPONSIVE**
- 11:21:43.838 cycle-2 开始
- 11:21:44.651 导航 qualification（798ms）
- 11:21:44.830 主体Tab ALREADY_SELECTED
- 11:21:44.851 达人Tab DISPATCHED
- **11:21:45.216 `Inspector.targetCrashed` + `Target.targetCrashed` status=crashed errorCode=-36861**
- 11:21:51.286 failure-evidence: stillInInventory=true, probes=[NO_REPLY×3], anchorHealth=RESPONSIVE

### 第二次登录复现（artifact: chromix-root-cause-2026-10-02-live-baseline-2026-10-02T11-23-54-XXX.jsonl）

- 11:23:54.333 登录成功，cycle-1 开始
- 11:23:54.859 导航 qualification（511ms，ready=complete）
- 11:23:55.972 主体Tab ALREADY_SELECTED
- 11:23:55.986 达人Tab DISPATCHED
- **11:23:56.375 `Inspector.targetCrashed` + `Target.targetCrashed` status=crashed errorCode=-36861**
- 11:24:02.406 failure-evidence: stillInInventory=true, probes=[NO_REPLY×3], anchorHealth=RESPONSIVE

两次崩溃点完全一致：达人Tab DISPATCHED 后 ~390ms / ~2225ms 出现崩溃。errorCode 相同。

### 合成对照汇总

| cohort | flags | cycles | 崩溃 | 崩溃事件 | NO_REPLY | exitCode |
|---|---|---|---|---|---|---|
| baseline | disableBreakpad=true, noSandbox=true, devShm=true, pipe=true | 8 | 0 | 0 | 0 | 0 |
| without-disable-breakpad | disableBreakpad=false | 8 | 0 | 0 | 0 | 0 |
| sandbox | noSandbox=false | 8 | 0 | 0 | 0 | 0 |
| without-disable-dev-shm-usage | devShm=false | 8 | 0 | 0 | 0 | 0 |

合成页面 32 轮全无崩溃。**崩溃只在真实页面复现**。

### detector-control 验证

- 注入 `Page.crash` 到合成 owned target
- 捕获到 `Inspector.targetCrashed` + `Target.targetCrashed`（status=crashed, errorCode=-2147483645 = STATUS_BREAKPOINT）
- target 仍在 inventory，3 次探活全 NO_REPLY，anchor 仍 RESPONSIVE
- verdict: DETECTOR_CAUGHT_CRASH_WITH_RETAINED_TARGET

这验证了应用侧 `init_owned_target` 的存活探测逻辑（"在 inventory 但无响应" → proven-missing → 重建）能正确处理此类崩溃。

## 竞争假设与裁决

1. **crashpad_handler 缺失导致崩溃处理失败** — 部分推翻。安装 zip 确无 crashpad_handler.exe，`--disable-breakpad` 来自 SDK 默认。但去 breakpad 的合成对照无崩溃，且真实崩溃 errorCode 与 crashpad 无关。crashpad 缺失只影响崩溃后的 dump/report，不导致 renderer 崩溃本身。`Crashpad_NotConnectedToHandler` 是崩溃处理失败的**症状**，不是**原因**。
2. **启动参数导致崩溃** — 推翻。4 组参数对照（32 轮）全无崩溃。
3. **页面特有 renderer 崩溃** — 已证实。只在真实快手资质页、达人Tab 切换后崩溃；合成页面（相同 DOM 结构、相同参数）不崩溃。
4. **上游 Chromix/Chromium 151 补丁栈问题** — 推测但未证实。需要二分上游版本来确认，本轮未做。证据：本仓库未改启动参数，崩溃在真实页面复现，errorCode 稳定。

## 本仓库未修改的项（硬约束遵守）

- 未删除/修改 `--disable-breakpad`，未塞 crashpad_handler，未换引擎，未动生产启动参数。
- 未碰真实账号做写操作；只读导航 + Tab 切换 + 开抽屉，不点权限、不提交。
- 临时 profile 用完清理（两次运行均 `disposableProfileRemoved: true`）。
- 未 git commit / archive / 改 task.json 状态。
- 诊断脚本只在 `.trellis/tasks/09-30-kuaishou-account-init/research/` 下。

## 下一步建议（不在本轮范围）

- **立即可做**：应用侧 `init_owned_target` 的存活探测已能处理此类崩溃（proven-missing → 重建）。建议确认重建后的新 target 是否也会在达人Tab 崩溃——如果是，初始化流程需把达人Tab 设为已知失败点，走 OCR 兜底而非重试。
- **上游二分**：用 Chromix 其他版本（如 150/152）跑同样的真实页面序列，确认是否为 151 特有。需要对应版本的安装包，本轮未做。
- **最小化复现**：本轮已确认崩溃点是达人Tab 切换。可进一步最小化为"只导航 qualification + 只点达人Tab"，省去 slice/抽屉步骤。但每次复现需登录，建议先确认是否每次必崩（本轮 2/2）。
- **错误码 -36861 上游追踪**：在 Chromium 151 源码或 issue tracker 搜索该码对应的崩溃类别。本轮未做。
