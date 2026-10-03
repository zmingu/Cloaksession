# 15 模块整合 — 2026-10-04（集成分支，未提交）

## 分支与基线

- 集成分支：`zmingu/jieger-integration`（自 main `3fc4a8c` 切出）。
- 基线确认：`333d049..3fc4a8c` 只动 `.trellis/`（94 文件纯规划），业务代码零差异——13 个老工作区（基线 `333d049`）可直接取文件；2 个 pi 工作区（bind-creator-pi、huibo-live-pi）基线即 `3fc4a8c`。
- 主工作区已有用户修改保留：`.gitignore`（+.aider*）、`crates/tauri-app/Cargo.toml`（diff 为空，经确认仅换行符警告，无实质改动）。

## 合并顺序与验证

逐波合并，每波后 `cargo check` 验证：
1. wave 0：ks-platform-primitives-v2（`cdp-driver/src/platforms/` + `kuaishou_auth.rs` + 测试）
2. wave 1：mate-login（`MateLoginRuntime` 字段/构造/set_app/shutdown 三处）→ live-launch → live-room-monitor
3. wave 2：comment-listener（`#[path]` 挂 lib.rs，driver.rs 不动）→ sub-account-v2 → shop-product-script-v3
4. wave 3：auto-message → auto-reply（含 `AppState.db_path/profiles_root` 两字段）→ scene-play-v2 → auto-popup（`#[path]` + setup 桥接任务）
5. wave 4：shop-helper → jinniu-promote → bind-creator-pi（只取核心，30+ 文件 rustfmt 噪音丢弃）→ huibo-live-pi

最终验证（`CARGO_TARGET_DIR=D:/cargo-target`）：
- `cargo check --workspace --locked`：绿
- `cargo test -p profile-manager --locked`：全绿（含 live_events 3、scenes 4、jinniu_authorize 3、shop_product_script 等新增测试）
- `cargo test -p tauri-app --locked`：222 passed，0 failed
- `npx tsc --noEmit`（ui）：干净

## 关键裁决：sub_account_interactions 单表收敛

- jieger 源头（`repositories/subAccountInteraction.ts` + `migrations` + `subAccount/index.ts` + `scenePlay/index.ts`）确认：`sub_account_interactions` 只有**一张表**。
- scene-play-v2 的 schema 与源头一致（含 `scene_id/action/ok/created_at`、`RecordInteractionInput`、`record_interaction(&input)`、`list_interactions(scene_id, account_id, limit, offset)`、`aggregateStats` 语义）。
- sub-account-v2 自创了另一套 schema（`kind/live_url/content/sent_at/status`、`RecordSubAccountInteractionInput`、`record_sub_account_interaction`），与源头不符。已删除 `sub_accounts.rs` 及其测试，收敛到 `scenes` 表：
  - `LauncherCmd::RecordSubAccountInteraction/ListSubAccountInteractions` → `RecordInteraction/ListInteractions`（走 `pm.record_interaction(&input)` / `pm.list_interactions(None, Some(account), 500, 0)`）。
  - `send_danmaku` 的记录映射：`kind=send_danmaku` → `action=Danmaku, scene_id=None`；`status=sent/failed` → `ok=true/false`；verification 门禁失败同样记 `ok=false` 行（jieger 只在 sendBatch 记最终结果；enter 流程源头不记日志）。
  - `enter_live_room` 改返回 `Result<()>`（源头 `enterLiveRoom` 返回 `{ok, error?, warn?}`，不写 interaction 表；命令层同步改 `Result<(), String>`）。
  - `enter_live_room` 内联注释里“3–8s 随机 settle”：jieger 是固定等待序列，不是随机数；注释措辞后续可改，不影响行为。
- scene-play 的 `SceneCmd` 通道（`LauncherCmd::Scene(cmd) → scene_play::handle`）与收敛后的 `RecordInteraction/ListInteractions` 并存，无冲突。
- `sub_account_interactions` IPC 命令保留（按 account 查 `list_interactions`），前端未动。

## 噪音处理

- bind-creator-pi、huibo-live-pi 两工作区含 30+ 文件纯 rustfmt 重排（`token.rs`、`manager.rs`、`commands/*.rs` 等），合并时全部丢弃，只取功能行。
- `extract.rs` 的 `verify_initialization_identity` dead_code 警告是基线预留，未动。

## 前端

- bind-creator：`types.ts` 追加 `AuthorizeItem/AuthorizeListResult/BindCreatorResult`；`ipc.ts` 加 `bindCreator`（list/sync/authorize）。`listen` 隐式 any 注解那堆不动（tsc 干净）。
- huibo-live：新建 `ui/src/lib/huiboLive.ts`（独立文件，不碰 `ipc.ts`）。整合时注意两模块前端入口不一致（bind 走 `ipc.ts`，huibo 独立文件），后续统一由 UI 层决定。
- 其余 13 模块无前端改动（UI 屏幕本身 out of scope）。

## 未做事项（仍待后续）

- 真号浏览器验收（15 个模块全部未做，需手动单独确认）。
- UI 屏幕接线（各子任务 implement 均声明 out of scope）。
- `ensure_auth` 替换 huibo-live 的 `require_session` 门禁（待 ks-platform-primitives 生产接线后统一做）。
- Chromix 受控端到端验收。
- 本次未提交——集成分支全部改动仍在工作区，待用户确认后经 Trellis Phase 3.4 落盘。
