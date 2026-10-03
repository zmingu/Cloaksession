# jieger 业务功能整体迁移到 Cloaksession

## Goal

将 jieger (`F:\jieger`, Electron+React+Playwright 快手直播中控) 的快手业务自动化模块**功能等价重写**到 Cloaksession (Rust+chromiumoxide)。本任务为**父任务**，持有迁移地图、依赖链、跨子任务验收标准，本身不直接实现——实际工作在各子任务中完成。

**范围边界（架构记忆已确认）：**
- **只迁功能，不迁旧账号/配置/话术/历史记录/登录态/数据库。** 旧数据一律不搬。
- jieger `Account.profileId` 是平台用户 ID，不是 Cloaksession 浏览器环境 Profile ID——不要套用。
- aiChat 不迁：Cloaksession 已有 MCP server，AI 能力走 MCP 路线，不复刻 jieger 的 OpenAI 兼容客户端。
- liveStats（直播统计）/ negative-whitelist（测负向）暂缓，不在本次 15 个子任务内。
- 前端：在现有单页 App（`crates/tauri-app/ui/src/App.tsx` 的 section 机制）内加业务 section，不引入 jieger 的多页面路由结构。

## 迁移地图（15 子任务，按依赖链 5 波）

### 波 0 — 平台基础层（所有直播/互动/商品 task 的前置依赖）

| 子任务 | jieger 源 | 规模 | 说明 |
|---|---|---|---|
| `ks-platform-primitives` | `platforms/kuaishou/connection.ts` + `utils/platformConfig.ts` | ~260 行 | 三原语 `kuaishouConnect`/`kuaishouLogin`/`ensureKuaishouAuth` + 三套平台配置（小店中控/小号观众/金牛）。改写为 Rust+chromiumoxide，复用已有 `cdp-driver` TaskPage/BrowserSession。**所有其他子任务的前置。** |

### 波 1 — 直播中控核心

| 子任务 | jieger 源 | 规模 | 依赖 |
|---|---|---|---|
| `live-launch` | `tasks/liveLaunch` + `services/kuaishou/liveMate` | 416 行 | ks-platform-primitives |
| `mate-login` | `tasks/mateLogin` + `platforms/kuaishou/liveMate` | 387 行 | ks-platform-primitives（与 live-launch 协作） |
| `live-room-monitor` | `tasks/liveRoomMonitor` | 777 行 | ks-platform-primitives |

### 波 2 — 弹幕与互动基础

| 子任务 | jieger 源 | 规模 | 依赖 |
|---|---|---|---|
| `comment-listener` | `tasks/commentListener` + `platforms/kuaishou/commentParser` | 114 行 | ks-platform-primitives |
| `sub-account` | `tasks/subAccount`（5 文件） | 1753 行（最大） | ks-platform-primitives, comment-listener |
| `auto-message` | `tasks/autoMessage` | 252 行 | sub-account |
| `auto-reply` | `tasks/autoReply` | 247 行 | comment-listener, sub-account |

### 波 3 — 互动自动化

| 子任务 | jieger 源 | 规模 | 依赖 |
|---|---|---|---|
| `auto-popup` | `tasks/autoPopUp` | 405 行 | sub-account, shop-product-script |
| `scene-play` | `tasks/scenePlay` | 365 行 | sub-account |

### 波 4 — 商品/金牛

| 子任务 | jieger 源 | 规模 | 依赖 |
|---|---|---|---|
| `shop-helper` | `tasks/shopHelper` + `shopHelperActions` | 132 行 | ks-platform-primitives |
| `shop-product-script` | `tasks/shopProductScript` | 264 行 | （独立，可早做） |
| `jinniu-promote` | `tasks/jinniuPromote` | 325 行 | ks-platform-primitives |
| `bind-creator` | `tasks/jinniu/bindCreator` | — | ks-platform-primitives, jinniu-promote |
| `huibo-live` | `tasks/huiboLive` | 51 行（最小） | ks-platform-primitives, live-room-monitor |

## 依赖链（图示）

```
波0  ks-platform-primitives ─────────────────────────────────┬──────────────────────────────┐
                                                               │                              │
波1  live-launch ── mate-login        live-room-monitor        │                              │
          └────────────┘                  │                     │                              │
                                         │                     │                              │
波2  comment-listener ──── sub-account   │        shop-product-script（独立）              │
          │                  │   │       │                     │                              │
          │                  │   └── auto-message              │                              │
          │                  │       │                         │                              │
          └── auto-reply ◀───┘       │                         │                              │
                                      │                         │                              │
波3  auto-popup ◀── shop-product-script  scene-play ◀──────────┘                              │
                                                                                                │
波4  shop-helper    jinniu-promote ── bind-creator    huibo-live ◀─ live-room-monitor ◀────────┘
```

**并行机会：** 波 0 落地后，波 1 的三个、波 2 的 comment-listener/shop-product-script、波 4 的 shop-helper/jinniu-promote 可并行规划与实现。`shop-product-script` 无前置依赖，可与波 0 同步。

## Requirements

### R1 — 范围
- R1.1 15 个子任务覆盖 4 大组：直播中控核心（3）、自动化互动（5）、小号互动体系（1）、商品/金牛（5）、平台基础层（1）。
- R1.2 不迁 aiChat；不走 OpenAI 兼容客户端；AI 能力经 MCP。
- R1.3 不迁 liveStats / negative-whitelist（后续任务）。
- R1.4 不搬旧数据（账号/配置/话术/历史/登录态/DB）。
- R1.5 磁力金牛独立 Profile，与快手小店登录态隔离（架构记忆硬约束，已由 business_accounts scope=jinniu/kuaishou 落地）。
- R1.6 不混淆两个商品流程：CPS「加货架」=`cps.kwaixiaodian.com`（已实现，开播前准备）；跟播助手「上车/小黄车」=`zs.kwaixiaodian.com/page/helper`（本任务 shop-helper）。

### R2 — 技术路线
- R2.1 业务自动化统一走 Rust + chromiumoxide，不新增 Node 业务 worker（架构记忆）。
- R2.2 浏览器侧复用已实现的 `cdp-driver::TaskPage`（cooperative lease + cancel + timeout + selector state）与 `BrowserSession`，不另起封装。
- R2.3 账户/Profile 持久化复用 `profile-manager::business_accounts`（scope=jinniu|kuaishou），不新增账号表。
- R2.4 弹幕识别：`tools/danmaku-pipeline/` 是独立 Rust crate，上游只产 JSON（`send_at` 为发送权威），下游发送端不在本仓库。`comment-listener` 子任务需对齐 pipeline 契约（`CONTRACT.md` + `schema/danmaku-script.schema.json`），只做监听/发送端。
- R2.5 IPC：新业务命令挂到 `crates/tauri-app/src/commands/`，遵循现有 IPC 验证规范（`.trellis/spec/tauri-app/backend/ipc.md` 的 command/prerequisite 表）。
- R2.6 前端：在 `crates/tauri-app/ui/src/App.tsx` 现有 section 机制（profiles/mcp/settings）内加业务 section，共用 TopBar + 左栏 + ActivityDrawer；不引入 react-router 多页面。

### R3 — 每个子任务的统一要求
- R3.1 子任务必须有自己的 `prd.md`（含 jieger 源文件清单、功能点、验收标准）+ 复杂者补 `design.md`/`implement.md`。
- R3.2 子任务 PRD 必须列出依赖的前置子任务（写在 prd/implement，不靠树位置隐式表达）。
- R3.3 涉及真实账号写动作（开播、切片、商品上下架、推广操作）的子任务，离线门禁 + 主代理复审通过后，还需真实浏览器集成验收单独确认——不能用旧授权代替。
- R3.4 每个 task 改写时对照 jieger 源文件逐功能点核对，不漏功能；但选择器/URL/选择器调试可在真号上重新校准（jieger 的选择器可能已过期）。
- R3.5 磁力金牛相关 task（jinniu-promote/bind-creator）注意 `homeType=new`（旧版）强制策略（架构记忆：快手命名是反的，new=旧版，super=新版）。

## Acceptance Criteria

### 跨子任务（父任务验收）
- [ ] AC1 15 个子任务全部 archive（status=completed）。
- [ ] AC2 4 大组各自可端到端走通：直播中控（开播→监控→伴侣登录）、小号互动（批量登录→进直播间→发弹幕→自动回复/弹窗/剧本）、商品/金牛（上车/话术/推广/达人授权/跟播）。
- [ ] AC3 平台基础层（ks-platform-primitives）被所有依赖它的子任务实际使用，无重复实现 connect/login 原语。
- [ ] AC4 业务命令全部注册到 Tauri IPC 且通过 ipc.md 的验证表；浏览器无关测试 ≠ 端到端验收。
- [ ] AC5 前端业务 section 在单页 App 内可用，不引入多页面路由。
- [ ] AC6 无旧数据迁移；无 Node 业务 worker；aiChat 未复刻。
- [ ] AC7 磁力金牛与快手小店 Profile 隔离未被破坏。
- [ ] AC8 CPS 加货架与跟播助手上车两个流程未混淆。

### 单子任务（每个子任务自己的 AC，此处只列统一模板）
- [ ] jieger 源文件清单中的功能点逐个有对应 Rust 实现。
- [ ] 离线门禁（`cargo test -p <crate> --locked` + `cargo check --workspace --locked`）通过。
- [ ] 复杂 task 的 `design.md`/`implement.md` 经审查。
- [ ] 真实账号写动作有单独验收确认记录。

## Out of Scope

- aiChat（走 MCP）、liveStats、negative-whitelist。
- 旧账号/配置/话术/历史记录/登录态迁移。
- Node/Playwright 业务 worker（已废弃路线）。
- 小店/直播/直播伴侣之间的 Cookie 共享或免登录（已延期，不得作为迁移前提）。
- 主体资料/切片权限/CPS 货架三块（Cloaksession 已实现，非 jieger 移植）。

## Notes

- 本父任务不直接 `task.py start`；逐个 start 子任务进入实现。
- 子任务实现顺序按波次：波 0 →（波1 并行 + shop-product-script）→ 波 2 → 波 3 → 波 4。但每个子任务独立可验证，不强制全局串行。
- 关联记忆：`cloaksession-architecture`、`kuaishou-account-init-rules`、`snow-danmaku-reference`、`windows-rust-pitfalls`。
- 关键 spec：`.trellis/spec/cdp-driver/backend/{index,sessions,task-control}.md`、`.trellis/spec/profile-manager/backend/business-accounts.md`、`.trellis/spec/tauri-app/backend/{ipc,kuaishou-initialization}.md`。
