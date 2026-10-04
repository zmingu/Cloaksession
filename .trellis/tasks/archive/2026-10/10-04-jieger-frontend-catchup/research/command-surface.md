# 15 模块命令面（前端封装用）

来源：`crates/tauri-app/src/commands/*.rs` + driver 结构体 + `ui/src/lib/ipc.ts` / `ui/src/lib/huiboLive.ts`。
约定：invoke 通道名 = Rust 函数 snake_case 名；参数为 camelCase；请求/响应结构体 `#[serde(rename_all="camelCase")]`。

总计 84 个 `#[tauri::command]`；已有 TS 封装 7 个（bind_creator 3 + huibo_live 4）；未覆盖 77 个。

## 已有 TS 封装

- `ui/src/lib/ipc.ts` — `bindCreator` 对象覆盖全部 3 个 `bind_creator_*` 命令；类型 `AuthorizeListResult`、`BindCreatorResult`、`AuthorizeItem` 从 `../types` 导入。
- `ui/src/lib/huiboLive.ts` — `huiboLive` 对象覆盖全部 4 个 huibo 命令；`HuiboVideo`、`ShopLiveState` 接口在文件内定义。

## 1. commands/kuaishou_auth.rs — 3，无 TS

| 命令 | 参数 | 返回 |
|---|---|---|
| `kuaishou_connect` | `profileId: string`, `targetId: string` | `boolean` |
| `kuaishou_login` | `profileId: string`, `targetId: string` | `void` |
| `ensure_kuaishou_auth` | `profileId: string`, `targetId: string` | `EnsureAuthResult{ok, scanned, error?}`（`crates/cdp-driver/src/platforms/kuaishou.rs:409`） |

事件：`kuaishou-auth-phase` `{profileId, phase}`。

## 2. commands/mate_login.rs — 3，无 TS

| 命令 | 参数 | 返回 |
|---|---|---|
| `mate_login_start` | `accountId: string` | `MateLoginState` |
| `mate_login_cancel` | `accountId: string` | `MateLoginState` |
| `mate_login_state` | `accountId: string` | `MateLoginState` |

`MateLoginState`（`driver/mate_login.rs:106`）：`accountId`, `stage: MateLoginStage`, `qrImageDataUrl?`, `qrLoginToken?`, `qrLoginSignature?`, `expireAt?: i64`, `errorMessage?`, `user?: {userId, userName, avatarUrl?}`, `startedAt?: i64`, `finishedAt?: i64`。`MateLoginStage` kebab-case：`idle,starting,awaiting-scan,awaiting-confirm,receiving,success,expired,cancelled,error`。事件 `mate-login-state-changed`。

## 3. commands/live_launch.rs — 7，无 TS

| 命令 | 参数 | 返回 |
|---|---|---|
| `live_launch_status` | `profileId: string` | `StreamingState` |
| `live_launch_prerequisites` | 无（用 `AppHandle` resource_dir） | `PrerequisitesReport` |
| `live_launch_credentials` | `profileId: string`, `controlUrl?: string` | `StreamCredentials` |
| `live_launch_heartbeat_start` | `profileId: string`, `controlUrl?: string` | `StreamingState` |
| `live_launch_heartbeat_stop` | `profileId: string` | `StreamingState` |
| `live_launch_stream_start` | `profileId: string`, `videoPath: string`, `controlUrl?: string` | `StreamingState` |
| `live_launch_stream_stop` | `profileId: string` | `StreamingState` |

结构体（`driver/live_launch.rs:153,220,284`）：`StreamCredentials{rtmpServer, streamKey, liveStreamId, placeholder}`，`StreamingState{profileId, status: StreamingStatus, mode?: StreamMode, target?, pid?, stderrTail, exitCode?, error?, placeholderCredentials, startedAt?}`，`PrerequisitesReport{available, ffmpegPath?, searched, error?}`。事件 `live-launch-state-changed`。

## 4. commands/live_room_monitor.rs — 3，无 TS

| 命令 | 参数 | 返回 |
|---|---|---|
| `start_live_room_monitor` | `profileId: string`, `config: MonitorConfig` | `LiveRoomMonitorState` |
| `stop_live_room_monitor` | 无 `profile_id` | `LiveRoomMonitorState` |
| `get_live_room_monitor_state` | 同步函数，无参数 | `LiveRoomMonitorState` |

`MonitorConfig`（`driver/live_room_monitor.rs:184`）：`liveRoomUrl`, `sceneId?: i64`, `groupId?`, `productScriptId?: i64`, `productScriptAccountId?`, `autoExitSubAccounts: bool`。`LiveRoomMonitorState`（同文件 197 行）：`enabled`, `profileId?`, `liveRoomUrl?`, `sceneId?`, `groupId?`, `productScriptId?`, `productScriptAccountId?`, `autoExitSubAccounts`, `status`, `liveStatus`, `triggeredForCurrentLive`, `enteringRooms`, `exitingRooms`, `lastCheckedAt?`, `nextCheckAt?`, `lastTriggeredAt?`, `lastEnterAllResult?`, `lastExitAllResult?`, `lastProductScriptResult?`, `error?`。事件 `live-room-monitor-state-changed`。

## 5. commands/comment_listener.rs — 7，无 TS

| 命令 | 参数 | 返回 |
|---|---|---|
| `comment_listener_start` | `profileId: string`, `targetId?: string` | `ListenerStatus` |
| `comment_listener_stop` | `profileId: string` | `boolean` |
| `comment_listener_status` | `profileId: string` | `ListenerStatus | null` |
| `comment_listener_status_all` | 无 | `ListenerStatus[]` |
| `comment_events_recent` | `profileId: string`, `limit?: number` | `CommentEvent[]` |
| `comment_events_history` | `profileId: string`, `limit?: number` | `LiveEventRow[]` |
| `comment_events_next` | `timeoutMs?: number` | `CommentEvent | null` |

`CommentEvent`（`driver/comment_listener.rs:115`）：`id`, `type: CommentEventType`, `sendAt: f64`, `text`, `userId?`, `nickname?`, `accountId`, `time`。`ListenerStatus`（同文件 394 行）：`accountId`, `running`, `mode: string`, `targetId`, `seenCount`, `recentCount`, `startedAt`, `lastEventAt?`, `lastError?`。`LiveEventRow`（`profile-manager/src/live_events.rs:18`）：`msgId`, `type: string`, `userId?`, `nickname?`, `content`, `time`, `accountId`。

## 6. commands/sub_account.rs — 8，无 TS

| 命令 | 参数 | 返回 |
|---|---|---|
| `save_sub_account` | `input: SaveBusinessAccountInput` | `BusinessAccount` |
| `list_sub_accounts` | 无 | `BusinessAccount[]` |
| `unbind_sub_account` | `id: string` | `void` |
| `sub_account_login` | `accountId: string` | `SubAccountLoginResult` |
| `batch_login_sub_accounts` | `accountIds: string[]` | `SubAccountLoginResult[]` |
| `sub_account_enter_live_room` | `accountId: string`, `liveUrl: string` | `void` |
| `sub_account_send_danmaku` | `accountId: string`, `content: string` | `SubAccountInteraction` |
| `sub_account_interactions` | `accountId: string` | `SubAccountInteraction[]` |

`SaveBusinessAccountInput`（`multizen-core/src/business.rs:70`）：`id?`, `profileId`, `kind: BusinessAccountKind`, `displayName`, `platformUserId?`。`BusinessAccount`（同文件 51 行）：`id`, `kind`, `displayName`, `platformUserId?`, `profileId?`, `createdAt`, `updatedAt`。`SubAccountLoginResult`（`driver/sub_account.rs:422`）：`accountId`, `ok`, `error?`。`SubAccountInteraction`（`profile-manager/src/scenes.rs:120`）：`id`, `accountId`, `sceneId?`, `action`, `message?`, `liveRoomUrl?`, `ok`, `error?`, `durationMs?`, `createdAt`。

## 7. commands/shop_product_script.rs — 9，无 TS

| 命令 | 参数 | 返回 |
|---|---|---|
| `shop_product_scripts_list` | 无 | `ShopProductScript[]` |
| `shop_product_script_get` | `id: string` | `ShopProductScriptDetail | null` |
| `shop_product_script_create` | `input: CreateShopProductScriptInput` | `ShopProductScript` |
| `shop_product_script_update` | `id: string`, `patch: UpdateShopProductScriptInput` | `ShopProductScript` |
| `shop_product_script_delete` | `id: string` | `void` |
| `shop_product_script_add_line` | `input: AddShopProductScriptLineInput` | `ShopProductScriptLine` |
| `shop_product_script_update_line` | `id: string`, `patch: UpdateShopProductScriptLineInput` | `ShopProductScriptLine` |
| `shop_product_script_delete_line` | `id: string` | `void` |
| `shop_product_script_reorder_lines` | `scriptId: string`, `orderedIds: string[]` | `ShopProductScriptLine[]` |

结构体（`profile-manager/src/shop_product_script.rs:38,49,57,74,83,93,111`）：`ShopProductScript{id, name, description?, createdAt, updatedAt}`，`ShopProductScriptDetail{script, lines}`，`ShopProductScriptLine{id, scriptId, sortOrder, action, goodsId, goodsName?, videoTimeSec: f64, leadSec: f64, content, createdAt, updatedAt}`，`CreateShopProductScriptInput{name, description?}`，`UpdateShopProductScriptInput{name?, description?: Option<Option<string>>}`，`AddShopProductScriptLineInput{scriptId, action, goodsId, goodsName?, videoTimeSec, leadSec?, content?, sortOrder?}`，`UpdateShopProductScriptLineInput{action?, goodsId?, goodsName?: Option<Option<string>>, videoTimeSec?, leadSec?, content?}`。

## 8. commands/auto_message.rs — 3，无 TS

| 命令 | 参数 | 返回 |
|---|---|---|
| `auto_message_start` | `lines: MessageLine[]`, `insertRandomSpace?: boolean`, `nickname?: string`, `anchor?: string`, `startAt?: u64` | `AutoMessageStarted` |
| `auto_message_stop` | `runId: string` | `void` |
| `auto_message_status` | `runId: string` | `AutoMessageState` |

`MessageLine`（`driver/auto_message.rs:93`）：`offsetSec: f64`, `message`, `accountId`。`AutoMessageStarted`（同文件 137 行）：`runId`, `startedAt: u64`, `scheduledCount`。`AutoMessageState`（同文件 127 行）：`startedAt: u64`, `totalCount`, `sentCount`, `schedule: ScheduledLine{offsetSec, triggerAt, message, accountId}[]`。

注意：`insertRandomSpace`（随机插空格规避检测）**前端不得提供该选项**，按既有安全约束不实现。

## 9. commands/auto_reply.rs — 3，无 TS

| 命令 | 参数 | 返回 |
|---|---|---|
| `auto_reply_preview` | `question: string`, `goods: GoodsKnowledge[]` | `ReplyResult`（直接返回，非 `Result`） |
| `auto_reply_history` | `accountId: string`, `limit?: number` | `AutoReplyRecord[]` |
| `auto_reply_record` | `accountId: string`, `content: string`, `reply: string`, `source: string`, `goodsId?: string` | `AutoReplyRecord` |

`GoodsKnowledge`（`driver/auto_reply.rs:64`）：`goodsId`, `title`, `price?`, `promotion?`, `status?`, `highlights`, `tokens`, `qa: {question, answer}[]`。`ReplyResult`（同文件 115 行）：`ok`, `reply?`, `source: ReplySource`, `goodsId?`, `intent?`, `error?`, `knowledgeHit`。`AutoReplyRecord`（`profile-manager/src/auto_reply.rs:27`）：`id`, `accountId`, `content`, `reply`, `source`, `goodsId?`, `createdAt`。

## 10. commands/scene_play.rs — 11，无 TS

| 命令 | 参数 | 返回 |
|---|---|---|
| `scene_create` | `name: string`, `triggerMode?: TriggerMode`, `groupId?: string` | `Scene` |
| `scene_get` | `id: i64` | `Scene | null` |
| `scene_list` | 无 | `Scene[]` |
| `scene_update` | `id: i64`, `name?: string`, `triggerMode?: TriggerMode`, `groupId?: Option<Option<string>>` | `Scene` |
| `scene_delete` | `id: i64` | `void` |
| `scene_add_line` | `sceneId: i64`, `message: string`, `timeOffsetSec: i64`, `actionType?: SceneLineAction` | `SceneLine` |
| `scene_update_line` | `id: i64`, `message?: string`, `timeOffsetSec?: i64`, `actionType?: SceneLineAction` | `SceneLine` |
| `scene_delete_line` | `id: i64` | `void` |
| `scene_reorder_lines` | `sceneId: i64`, `lineIds: i64[]` | `SceneLine[]` |
| `scene_play` | `sceneId: i64`, `options?: PlaySceneOptions` | `PlayStarted` |
| `scene_stop` | `sceneId: i64` | `boolean` |

`Scene` / `SceneLine`（`profile-manager/src/scenes.rs:97,108`）：`Scene{id, name, triggerMode, groupId?, lines, createdAt, updatedAt}`，`SceneLine{id, sceneId, ord, message, timeOffsetSec, actionType}`。`PlaySceneOptions`（`driver/scene_play.rs:194`）：`startAtMs?`, `allowDynamicPool`, `groupId?`。`PlayStarted`（同文件 209 行）：`sceneId`, `scheduledCount`, `schedule: {lineId, ord, accountId, profileId, accountName, message, actionType, triggerAtMs}[]`。

注意：`scene_update` 的 `groupId: Option<Option<string>>` 需要三态（不改 / 置空 / 设值），前端传参时留意。

## 11. commands/auto_popup.rs — 10，无 TS

| 命令 | 参数 | 返回 |
|---|---|---|
| `auto_popup_start` | `profileId: string`, `config: AutoPopUpConfig` | `AutoPopUpStatus` |
| `auto_popup_stop` | `profileId: string`, `reason?: string` | `AutoPopUpStatus` |
| `auto_popup_status` | `profileId: string` | `AutoPopUpStatus` |
| `auto_popup_update_config` | `profileId: string`, `patch: AutoPopUpConfigPatch` | `AutoPopUpStatus` |
| `auto_popup_goods` | `profileId: string` | `GoodsInfo[]` |
| `auto_popup_scan` | `profileId: string` | `ScanReport` |
| `auto_popup_explain_once` | `profileId: string`, `goodsId: string` | `void` |
| `auto_popup_register_shortcuts` | `profileId: string`, `bindings: Record<string,string>` | `ShortcutRegisterResult` |
| `auto_popup_unregister_shortcuts` | `profileId: string` | `void` |
| `auto_popup_trigger_shortcut` | `profileId: string`, `accelerator: string` | `string`（绑定的 goodsId） |

结构体（`driver/auto_popup.rs`）：`AutoPopUpConfig{goodsIds?, interval: [f64;2], perGoodsInterval?, goodsItems?: {id, repeatCount?, interval?}[], random, retry?}`（131 行），`AutoPopUpConfigPatch` 同形全可选（148 行），`GoodsInfo{serial, title?, price?}`（361 行），`ScanReport{scannedCount, diffs, candidates}`（645 行），`ShortcutRegisterResult{ok, registered, failed}`（715 行），`AutoPopUpStatus{profileId, running, queueLen, lastGoodsId?, lastError?, updatedAt}`（790 行）。事件 `auto-popup:state`、`auto-popup:event`。

## 12. commands/shop_helper.rs — 4，无 TS

| 命令 | 参数 | 返回 |
|---|---|---|
| `shop_helper_read_goods` | `profileId: string`, `targetId: string`, `tab: string` | `HelperGoodInfo[]` |
| `shop_helper_switch_tab` | `profileId: string`, `targetId: string`, `tab: string` | `HelperGoodInfo[]` |
| `shop_helper_add_to_cart` | `profileId: string`, `targetId: string`, `goodsId: string` | `HelperGoodActionResult` |
| `shop_helper_remove_from_cart` | `profileId: string`, `targetId: string`, `goodsId: string` | `HelperGoodActionResult` |

`HelperGoodInfo`（`driver/shop_helper.rs:260`）：`goodsId`, `goodsName`, `rawText`, `availableActions`, `status`, `sourceTab`。`HelperGoodActionResult`（同文件 274 行）：`ok`, `goodsId`, `action`, `detail`, `goods`。事件 `shop-helper:goods-changed` `{profileId, goodsId, action, ok}`。

## 13. commands/jinniu_promote.rs — 6，无 TS

| 命令 | 参数 | 返回 |
|---|---|---|
| `jinniu_promote_open_store_create` | `profileId: string` | `StoreCreateTab` |
| `jinniu_promote_live_users` | `profileId: string` | `JinniuLiveUsers` |
| `jinniu_promote_select_live_user` | `profileId: string`, `uid: string` | `JinniuLiveUser` |
| `jinniu_promote_apply_phase1` | `profileId: string`, `config: StoreCreatePhase1Config` | `void` |
| `jinniu_promote_apply_phase2` | `profileId: string` | `string[]` |
| `jinniu_promote_submit` | `profileId: string` | `void` |

结构体（`driver/jinniu_promote.rs:144,154,162,172`）：`JinniuLiveUser{uid, displayName, fullText, isSelected}`，`JinniuLiveUsers{accountId, users}`，`StoreCreateTab{accountId, url, targetId}`，`StoreCreatePhase1Config{enableNetRoi?, dailyBudget?, roiCoefficient?, promoteType?, roiTargetMode?, creativeMode?}`。

## 14. commands/bind_creator.rs — 3，TS 已全覆盖（`ipc.ts` 的 `bindCreator`）

| 命令 | 参数 | 返回 | TS 封装 |
|---|---|---|---|
| `bind_creator_get_authorize_list` | `jinniuId: string` | `AuthorizeListResult` | `bindCreator.list(jinniuId)` |
| `bind_creator_sync_authorize_list` | `profileId: string`, `jinniuId: string`, `accountId?: string` | `AuthorizeListResult` | `bindCreator.sync(profileId, jinniuId, accountId?)` |
| `bind_creator_start_authorize` | `profileId: string`, `jinniuId: string`, `kuaishouId: string`, `skipConfirm?: boolean`, `accountId?: string` | `BindCreatorResult` | `bindCreator.authorize(...)` |

`AuthorizeListResult`（`driver/bind_creator.rs:128`）：`ok`, `data: AuthorizeItem{userId, userName, status, authorizeTime}[]`, `error?`。`BindCreatorResult`（同文件 156 行）：`ok`, `error?`。

## 15. commands/huibo_live.rs — 4，TS 已全覆盖（`huiboLive.ts` 的 `huiboLive`）

| 命令 | 参数 | 返回 | TS 封装 |
|---|---|---|---|
| `get_huibo_video_list` | `profileId: string` | `HuiboVideo[]` | `huiboLive.videoList(profileId)` |
| `start_huibo_live` | `profileId: string`, `replayId: string` | `ShopLiveState` | `huiboLive.startLive(profileId, replayId)` |
| `get_shop_live_state` | `profileId: string` | `ShopLiveState` | `huiboLive.liveState(profileId)` |
| `cancel_huibo_task` | `profileId: string` | `boolean` | `huiboLive.cancel(profileId)` |

`HuiboVideo`（`driver/huibo_live.rs:75`）：`id`, `name`, `uploadTime`, `segments: u32`, `size`, `duration: string`, `status`, `usageCount: u32`, `expiryTime`, `goodsCount: u32`。`ShopLiveState`（同文件 112 行）：`profileId`, `status: ShopLiveStatus`, `liveRoomUrl?`, `updatedAt: i64`, `error?`。

## 特殊注意

- `stop_live_room_monitor` 无 `profile_id` 参数；`get_live_room_monitor_state` 是同步函数。
- `auto_reply_preview` 直接返回 `ReplyResult`，不是 `Result`。
- `auto_popup_register_shortcuts` / `unregister_shortcuts` 不经过 `AppState` driver；`trigger_shortcut` 返回绑定的 goodsId 字符串。
- `scene_update` 的 `groupId` 三态需处理。
- `businessAccounts.ts`、`kuaishouIdentity.ts` 已确认不含这 15 模块的封装。
