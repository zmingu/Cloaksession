/**
 * TypeScript mirrors of the Rust serde types consumed by the Tauri
 * commands registered in P4.3. All Rust structs use
 * `#[serde(rename_all = "camelCase")]`, so the TS field names are the
 * camelCase versions of the Rust field names.
 *
 * These types are intentionally conservative: every field that the
 * command signatures return is modeled. Fields that are not yet needed
 * by the UI are still included when they are part of the Rust struct,
 * to keep the mirrors faithful. Where a Rust type leaves a field
 * optional (`Option<T>`), the TS field is `T | null` (serde serializes
 * `None` as `null`).
 *
 * Source of truth:
 *   crates/multizen-core/src/profile.rs
 *   crates/multizen-core/src/settings.rs
 *   crates/mcp-server/src/activity.rs
 *   crates/tauri-app/src/driver.rs
 *   crates/tauri-app/src/commands/system.rs
 */

// ---------------------------------------------------------------------------
// Profile domain (crates/multizen-core/src/profile.rs)
// ---------------------------------------------------------------------------

export type ProfileId = string;

/** `ProxyConfig.proxy_type` ("http" | "socks5"). Kept as string for forward compat. */
export type ProxyType = string;

export interface ProxyConfig {
  /** Rust field `proxy_type` renamed via `#[serde(rename = "type")]`. */
  type: ProxyType;
  host: string;
  port: number;
  username?: string | null;
  password?: string | null;
}

/**
 * `DeviceFamily` enum uses `#[serde(rename_all = "kebab-case")]` plus
 * per-variant `#[serde(rename = "...")]`, so the serialized form is the
 * kebab-case string (e.g. "macbook-pro-14-m3"). We model it as a string
 * union for type-safety; the UI can use the `fingerprint_devices`
 * command to enumerate valid values at runtime.
 */
export type DeviceFamily =
  | "macbook-pro-14-m3"
  | "macbook-pro-14-m3-pro"
  | "macbook-pro-16-m3-pro"
  | "macbook-air-13-m3"
  | "macbook-air-15-m3"
  | "imac-24-m3"
  | "mac-mini-m2"
  | "windows-laptop-intel"
  | "windows-laptop-intel-uhd"
  | "windows-laptop-amd"
  | "windows-laptop-nvidia"
  | "windows-laptop-nvidia-4050"
  | "windows-desktop-nvidia"
  | "windows-desktop-nvidia-4080"
  | "windows-desktop-amd"
  | "windows-desktop-intel"
  | "linux-desktop-intel"
  | "linux-desktop-amd"
  | "linux-desktop-nvidia"
  | (string & {}); // allow unknown families without breaking narrowing

export interface ClientHints {
  secChUa: string;
  secChUaPlatform: string;
  secChUaPlatformVersion: string;
  secChUaArch: string;
  secChUaBitness: string;
  secChUaMobile: string;
  secChUaModel: string;
  secChUaFullVersionList: string;
}

export interface ScreenSize {
  width: number;
  height: number;
}

export interface WebGLConfig {
  vendor: string;
  renderer: string;
}

export interface FingerprintConfig {
  device: DeviceFamily;
  userAgent: string;
  platform: string;
  clientHints: ClientHints;
  locale: string;
  languages: string[];
  acceptLanguage: string;
  timezone: string;
  country: string;
  screen: ScreenSize;
  availScreen?: ScreenSize | null;
  dpr: number;
  webgl: WebGLConfig;
  hardwareConcurrency: number;
  deviceMemory: number;
  fontsDir?: string | null;
  storageQuota?: number | null;
  seed?: string | null;
}

export interface ExtensionConfig {
  id: string;
  name: string;
  version: string;
  enabled: boolean;
  scope: string;
  dir: string;
  source: string;
}

export interface Profile {
  id: ProfileId;
  name: string;
  notes?: string | null;
  tags: string[];
  proxy?: ProxyConfig | null;
  fingerprint: FingerprintConfig;
  chromixOptions?: Record<string, unknown>;
  extensions?: ExtensionConfig[] | null;
  icon?: string | null;
  startUrl?: string | null;
  searchProvider?: string | null;
  dataDir: string;
  createdAt: string;
  updatedAt: string;
  lastOpenedAt?: string | null;
  proxyCountry?: string | null;
  group: string | null;
}

export interface ProfileSummary {
  id: ProfileId;
  name: string;
  tags: string[];
  lastOpenedAt?: string | null;
  isRunning: boolean;
  icon?: string | null;
  proxy?: ProxyConfig | null;
  timezone?: string | null;
  proxyCountry?: string | null;
  device?: DeviceFamily | null;
  group: string | null;
}

export interface ProfileGroup {
  name: string | null;
  count: number;
}

export interface PartialFingerprintInput {
  userAgent?: string;
  locale?: string;
  timezone?: string;
  country?: string;
}

export interface CreateProfileInput {
  name: string;
  notes?: string;
  tags?: string[];
  icon?: string;
  startUrl?: string;
  searchProvider?: string;
  group?: string | null;
  proxy?: ProxyConfig;
  /** Full UI fingerprint, or the legacy partial MCP-compatible patch. */
  fingerprint?: FingerprintConfig | PartialFingerprintInput;
  chromixOptions?: Record<string, unknown>;
  extensions?: ExtensionConfig[];
}

export interface UpdateProfileInput {
  name?: string;
  notes?: string;
  tags?: string[];
  icon?: string | null;
  startUrl?: string | null;
  searchProvider?: string | null;
  group?: string | null;
  proxy?: ProxyConfig | null;
  /** Whole-replace — the UI always holds a complete FingerprintConfig. */
  fingerprint?: FingerprintConfig;
  chromixOptions?: Record<string, unknown>;
  extensions?: ExtensionConfig[];
}

export interface LaunchedProfile {
  id: ProfileId;
  cdpEndpoint: string;
  pid: number;
  startedAt: string;
}

// ---------------------------------------------------------------------------
// Settings (crates/multizen-core/src/settings.rs)
// ---------------------------------------------------------------------------

export type BrowserEngine = "cft" | "cloakbrowser" | "chromix";

/** App UI language wire values (`multizen_core::AppLanguage`). No system mode. */
export type AppLanguage = "zh-CN" | "en";

export interface ChromixSettings {
  nodePath: string;
  options: Record<string, unknown>;
  environment: Record<string, string>;
}

export interface AppSettings {
  theme: string;
  language: AppLanguage;
  mcpHttpEnabled: boolean;
  mcpHttpPort: number;
  browserEngine: BrowserEngine;
  browserBinaryPath?: string | null;
  chromix: ChromixSettings;
  skipBrowserDownload: boolean;
  autoUpdate: boolean;
  usageReporting: boolean;
}

// ---------------------------------------------------------------------------
// Activity (crates/mcp-server/src/activity.rs)
// ---------------------------------------------------------------------------

export interface ActivityEvent {
  id: string;
  timestamp: string;
  tool: string;
  profileId?: string | null;
  args: unknown;
  status: string;
  summary?: string | null;
  durationMs?: number | null;
}

// ---------------------------------------------------------------------------
// Push event payloads (crates/tauri-app/src/driver.rs)
//
// NOTE: the on-wire Rust payloads are flat `{ profileId, status, error }`
// (chromium) and `{ profileId, running }` (running-changed). The migrated
// renderer, however, was written against the legacy Electron preload which
// used discriminated-union shapes (`kind` tag). For P4.7 we keep the
// renderer's expected shapes so it compiles; the Tauri runtime payload is
// structurally different, so these listeners effectively degrade until
// P4.8 reconciles them. This is the documented "functional degradation
// acceptable" path for the scope-excluded chromium feature.
// ---------------------------------------------------------------------------

export type RunningStateChange =
  | { kind: "launched"; profileId: ProfileId }
  | { kind: "closing"; profileId: ProfileId }
  | { kind: "closed"; profileId: ProfileId; reason: "user-close" | "external-exit" };

/**
 * Chromium runtime status — discriminated union by `kind`, matching the
 * legacy renderer contract. The Rust `chromium:status` event currently
 * emits a flat object; until P4.8 the UI treats unknown payloads as
 * `kind: "ready"` (modal hidden). Keep this as a union so the renderer's
 * exhaustive `switch (status.kind)` compiles.
 */
export type ChromiumStatus =
  | { kind: "ready" }
  | { kind: "dev-system" }
  | { kind: "missing" }
  | { kind: "fetching-manifest" }
  | { kind: "downloading"; version: string; bytesReceived: number; bytesTotal: number }
  | { kind: "verifying" }
  | { kind: "extracting"; version: string }
  | { kind: "error"; message: string };

/** Legacy alias kept for any code referencing `ChromiumStatusV1`. */
export type ChromiumStatusEvent = ChromiumStatus;

// ---------------------------------------------------------------------------
// System info (crates/tauri-app/src/commands/system.rs)
// ---------------------------------------------------------------------------

export interface SystemInfo {
  mcpHttpUrl: string;
  mcpAuthToken?: string | null;
  appVersion: string;
  platform: string;
}

// ---------------------------------------------------------------------------
// Renderer-only types (migrated from the legacy renderer/src/types.ts).
// These describe catalog shapes and push events that the renderer expects
// but that the Tauri backend does not yet faithfully emit. They are kept
// here so the migrated components compile; runtime values may be stale or
// stubbed until P4.8 / P5.
// ---------------------------------------------------------------------------

/** `extensions:installed` push payload ("Add to Cloaksession" companion event). */
export type ExtensionInstalledEvent =
  | { ok: true; profileId: string; extension: ExtensionConfig }
  | { ok: false; profileId: string; error: string };

export interface DeviceCatalogEntry {
  family: DeviceFamily;
  label: string;
  screens: ReadonlyArray<{ width: number; height: number; label: string }>;
}

export interface LocaleCatalogEntry {
  id: string;
  label: string;
  locale: string;
  country: string;
  timezones: ReadonlyArray<string>;
}

export interface FingerprintReconcilePatch {
  device?: DeviceFamily;
  localeId?: string;
  screen?: { width: number; height: number };
  timezone?: string;
  hardwareConcurrency?: number;
  deviceMemory?: number;
  country?: string;
}

export interface ProxyGeoResult {
  country: string;
  countryName: string;
  timezone: string;
  city: string;
  ip: string;
}

/**
 * Update-checker status (renderer contract). The Tauri build has no
 * updater wired yet (scope-excluded); the `update` IPC namespace is a
 * stub, so any `UpdateStatus` the UI receives will be the stub's
 * `{ kind: "idle" }` default.
 */
export type UpdateStatus =
  | { kind: "idle" }
  | { kind: "checking" }
  | { kind: "available"; version: string; releaseNotes?: string }
  | { kind: "downloading"; version: string; received: number; total: number; percent: number }
  | { kind: "ready"; version: string }
  | { kind: "no-update" }
  | { kind: "up-to-date" }
  | { kind: "error"; message: string };

// ---------------------------------------------------------------------------
// Bind creator (jinniu authorize) — crates/tauri-app/src/driver/bind_creator.rs
// ---------------------------------------------------------------------------

/** One creator-authorize row (jieger `AuthorizeItem`, camelCase over IPC). */
export interface AuthorizeItem {
  userId: string;
  userName: string;
  status: string;
  authorizeTime: string;
}

/** List result (jieger `AuthorizeListResult`). */
export interface AuthorizeListResult {
  ok: boolean;
  data: AuthorizeItem[];
  error?: string;
}

/** Bind result (jieger `BindCreatorResult`). */
export interface BindCreatorResult {
  ok: boolean;
  error?: string;
}

// ---------------------------------------------------------------------------
// D-group shop (product scripts / shop helper / auto popup)
//
// Frontend mirrors of:
//   crates/profile-manager/src/shop_product_script.rs
//   crates/tauri-app/src/driver/shop_helper.rs
//   crates/tauri-app/src/driver/auto_popup.rs
// All Rust structs use `#[serde(rename_all = "camelCase")]` except
// `ScriptLineAction` (kebab-case), `KnowledgeField` (kebab-case) and
// `PopupEventKind` (kebab-case). `None` serializes as `null`.
// ---------------------------------------------------------------------------

/** 话术行动作：`ScriptLineAction` (kebab-case over IPC). */
export type ScriptLineAction = "on-shelf" | "off-shelf" | "explain" | "cancel-explain";

/** 商品话术脚本（库条目）。Rust `ShopProductScript`. */
export interface ShopProductScript {
  id: string;
  name: string;
  description: string | null;
  createdAt: string;
  updatedAt: string;
}

/** 脚本明细：脚本 + 按 `sortOrder` 排序的话术行。 */
export interface ShopProductScriptDetail {
  script: ShopProductScript;
  lines: ShopProductScriptLine[];
}

/** 话术行：动作 + 商品 + 视频时间点 + 提前量 + 话术内容。 */
export interface ShopProductScriptLine {
  id: string;
  scriptId: string;
  sortOrder: number;
  action: ScriptLineAction;
  goodsId: string;
  goodsName: string | null;
  videoTimeSec: number;
  leadSec: number;
  content: string;
  createdAt: string;
  updatedAt: string;
}

/** 创建脚本入参。`description` 缺省 = 无描述。 */
export interface CreateShopProductScriptInput {
  name: string;
  description?: string | null;
}

/**
 * 更新脚本入参（三态 `description`）：
 * - `undefined`（字段缺省）= 保持原值；
 * - `null`（`Some(None)`）= 清空描述；
 * - `string`（`Some(Some(v))`）= 设为新值（空字符串按后端规则归一为 `None`）。
 */
export interface UpdateShopProductScriptInput {
  name?: string;
  description?: string | null;
}

/** 新增话术行入参：`sortOrder` 缺省时追加到末尾。 */
export interface AddShopProductScriptLineInput {
  scriptId: string;
  action: ScriptLineAction;
  goodsId: string;
  goodsName?: string | null;
  videoTimeSec: number;
  leadSec?: number;
  content?: string;
  sortOrder?: number | null;
}

/**
 * 更新话术行入参：缺省字段保持原值。
 * `goodsName` 三态：`undefined` = 保持；`null`（`Some(None)`）= 清空；
 * `string` = 设为新值（空字符串归一为 `None`）。
 */
export interface UpdateShopProductScriptLineInput {
  action?: ScriptLineAction;
  goodsId?: string;
  goodsName?: string | null;
  videoTimeSec?: number;
  leadSec?: number;
  content?: string;
}

/** 商品 Tab：小黄车内商品 / 待上车商品。 */
export type HelperGoodTab = "inCart" | "toAdd";

/** 商品状态（Rust `HelperGoodStatus`, camelCase）。 */
export type HelperGoodStatus = "available" | "onShelf" | "offShelf" | "unknown";

/** 跟播助手商品信息。Rust `HelperGoodInfo`. */
export interface HelperGoodInfo {
  goodsId: string;
  goodsName: string;
  rawText: string;
  /** 可用动作（`上车`/`下车`子集）。 */
  availableActions: string[];
  status: HelperGoodStatus;
  /** 读取来源 Tab（`inCart`/`toAdd`）。 */
  sourceTab: string;
}

/** 上车/下车写动作结果（含操作后重读的商品列表）。 */
export interface HelperGoodActionResult {
  ok: boolean;
  goodsId: string;
  /** `on`（上车）/`off`（下车）。 */
  action: string;
  detail: string;
  goods: HelperGoodInfo[];
}

/** `shop-helper:goods-changed` 推送载荷（camelCase）。 */
export interface ShopHelperGoodsChanged {
  profileId: string;
  goodsId: string;
  action: string;
  ok: boolean;
}

/** 增强商品队列项。Rust `AutoPopUpGoodsItem`. */
export interface AutoPopUpGoodsItem {
  id: string;
  repeatCount?: number | null;
  interval?: [number, number] | null;
}

/** 失败重试配置。Rust `AutoPopUpRetryConfig`. */
export interface AutoPopUpRetryConfig {
  maxRetries?: number | null;
  retryDelayMs?: number | null;
}

/** 自动弹品配置。`goodsItems` 优先，缺席时回退到 `goodsIds`。 */
export interface AutoPopUpConfig {
  goodsIds?: string[] | null;
  interval: [number, number];
  perGoodsInterval?: Record<string, [number, number]> | null;
  goodsItems?: AutoPopUpGoodsItem[] | null;
  random?: boolean;
  retry?: AutoPopUpRetryConfig | null;
}

/** 配置热更新补丁：`Some` 字段覆盖运行中配置，缺省保持不变。 */
export interface AutoPopUpConfigPatch {
  goodsIds?: string[] | null;
  interval?: [number, number] | null;
  perGoodsInterval?: Record<string, [number, number]> | null;
  goodsItems?: AutoPopUpGoodsItem[] | null;
  random?: boolean | null;
  retry?: AutoPopUpRetryConfig | null;
}

/** 商品行。空字符串后端归一为 `None` → 前端 `null`。 */
export interface PopupGoodsInfo {
  serial: string;
  title: string | null;
  price: string | null;
}

/** 已入库的商品知识（标题 / 价格，弹窗扫描用；与 C 组回复预览的同名类型区分）。 */
export interface PopupGoodsKnowledge {
  title?: string | null;
  price?: string | null;
}

/** 知识差异字段（kebab-case）。 */
export type KnowledgeField = "title" | "price";

export interface PopupScanDiff {
  goodsId: string;
  field: KnowledgeField;
  current: string | null;
  candidate: string | null;
}

export interface PopupScanReport {
  scannedCount: number;
  diffs: PopupScanDiff[];
  candidates: Record<string, PopupGoodsKnowledge>;
}

export interface ShortcutFailure {
  accelerator: string;
  error: string;
}

export interface ShortcutRegisterResult {
  ok: boolean;
  registered: string[];
  failed: ShortcutFailure[];
}

/** 运行状态快照（命令返回 + `auto-popup:state` 广播载荷）。 */
export interface AutoPopUpStatus {
  profileId: string;
  running: boolean;
  queueLen: number;
  lastGoodsId: string | null;
  lastError: string | null;
  updatedAt: string;
}

/** 广播事件种类（kebab-case）。 */
export type PopupEventKind =
  | "started"
  | "stopped"
  | "explained"
  | "explain-failed"
  | "config-updated"
  | "shortcut-triggered";

/** 广播事件（`auto-popup:event` 载荷）。 */
export interface AutoPopUpEvent {
  profileId: string;
  kind: PopupEventKind;
  goodsId?: string | null;
  reason?: string | null;
}

// C-group: auto-message / auto-reply / scene-play
// (crates/tauri-app/src/driver/auto_message.rs, auto_reply.rs, scene_play.rs
// + profile-manager scenes / auto_reply records).
//
// NOTE: the auto-message structs (`MessageLine`, `ScheduledLine`,
// `AutoMessageState`, `AutoMessageStarted`, `AutoMessageStopped`) have no
// `#[serde(rename_all)]`, so their wire keys stay snake_case (unlike the
// camelCase structs elsewhere in this file). Scene / reply structs use
// camelCase, `TriggerMode` is kebab-case, `SceneLineAction` lowercase.
// Character-spacing injection is intentionally NOT modeled here:
// the frontend never enables it.
// ---------------------------------------------------------------------------

/** One timeline entry: send `message` at `offset_sec` after run start. */
export interface AutoMessageLine {
  offset_sec: number;
  message: string;
  account_id: string;
}

/** Options for `auto_message_start` (random-space injection excluded). */
export interface AutoMessageStartOptions {
  nickname?: string | null;
  anchor?: string | null;
  startAt?: number | null;
}

/** A line with its absolute fire time resolved. */
export interface AutoMessageScheduledLine {
  offset_sec: number;
  trigger_at: number;
  message: string;
  account_id: string;
}

/** Snapshot emitted on start and after every dispatch. */
export interface AutoMessageState {
  started_at: number;
  total_count: number;
  sent_count: number;
  schedule: AutoMessageScheduledLine[];
}

/** Return value of `auto_message_start`. */
export interface AutoMessageStarted {
  run_id: string;
  started_at: number;
  scheduled_count: number;
}

/** Payload of the `auto-message:stopped` push event. */
export interface AutoMessageStopped {
  run_id: string;
  reason: string;
}

/** One goods' reply knowledge (caller-supplied snapshot for preview). */
export interface GoodsKnowledge {
  goodsId: string;
  title: string;
  price?: string | null;
  promotion?: string | null;
  status?: string | null;
  highlights?: string[];
  tokens?: string[];
  qa?: GoodsQa[];
}

export interface GoodsQa {
  question: string;
  answer: string;
}

/** Where a reply came from (`Ai` is hook-only, never auto-sent). */
export type ReplySource = "knowledge" | "ai" | "template";

/** Resolution outcome for one comment. */
export interface ReplyResult {
  ok: boolean;
  reply: string | null;
  source: ReplySource;
  goodsId: string | null;
  intent: string | null;
  error: string | null;
  knowledgeHit: boolean;
}

/** One persisted auto-reply resolution row. */
export interface AutoReplyRecord {
  id: number;
  accountId: string;
  content: string;
  reply: string;
  source: string;
  goodsId: string | null;
  createdAt: string;
}

/** Scene trigger mode (wire: kebab-case). */
export type SceneTriggerMode = "relative-time" | "local-time";

/** Per-line action (wire: lowercase). */
export type SceneLineAction = "danmaku" | "like" | "follow";

export interface SceneLine {
  id: number;
  sceneId: number;
  ord: number;
  message: string;
  timeOffsetSec: number;
  actionType: SceneLineAction;
}

export interface Scene {
  id: number;
  name: string;
  triggerMode: SceneTriggerMode;
  groupId: string | null;
  lines: SceneLine[];
  createdAt: number;
  updatedAt: number;
}

/**
 * Tri-state `group_id` patch for `scene_update`, mirroring the Rust
 * `Option<Option<String>>`: `keep` omits the field, `clear` sends null,
 * `set` sends the new id.
 */
export type SceneGroupIdPatch =
  | { mode: "keep" }
  | { mode: "clear" }
  | { mode: "set"; groupId: string };

/** A line pinned to an account and an absolute fire time (ms epoch). */
export interface SceneScheduledItem {
  lineId: number;
  ord: number;
  accountId: string;
  profileId: string;
  accountName: string;
  message: string;
  actionType: SceneLineAction;
  triggerAtMs: number;
}

/** Options for `scene_play` (also the `PlaySceneOptions` IPC payload). */
export interface PlaySceneOptions {
  startAtMs?: number | null;
  allowDynamicPool?: boolean;
  groupId?: string | null;
}

/** `scene_play` result. */
export interface PlayStarted {
  sceneId: number;
  scheduledCount: number;
  schedule: SceneScheduledItem[];
}

/** Payload of the `scene:started` push event. */
export interface SceneStartedPayload {
  sceneId: number;
  schedule: SceneScheduledItem[];
  startedAt: number;
}

/** Payload of the `scene:progress` push event. */
export interface SceneProgressPayload {
  sceneId: number;
  sentCount: number;
  totalCount: number;
  lastItem: SceneScheduledItem;
  ok: boolean;
  error: string | null;
}

/** Payload of the `scene:finished` push event. */
export interface SceneFinishedPayload {
  sceneId: number;
  stopped: boolean | null;
  reason: string | null;
}

// E-group: jinniu promote — crates/tauri-app/src/driver/jinniu_promote.rs
// (serde `camelCase`; mirrors the Rust structs 1:1)
// ---------------------------------------------------------------------------

/** One promotable live user (jieger `LiveUserInfo`). */
export interface JinniuLiveUser {
  uid: string;
  displayName: string;
  fullText: string;
  isSelected: boolean;
}

/** `getLiveUsers` result. */
export interface JinniuLiveUsers {
  accountId: string;
  users: JinniuLiveUser[];
}

/** Opened (or reused) storeCreate tab. */
export interface StoreCreateTab {
  accountId: string;
  url: string;
  targetId: string;
}

/**
 * Phase-1 config (jieger `StoreCreatePhase1Config`). All fields optional;
 * the backend fills jieger's defaults.
 */
export interface StoreCreatePhase1Config {
  enableNetRoi?: boolean | null;
  dailyBudget?: string | null;
  roiCoefficient?: string | null;
  promoteType?: string | null;
  roiTargetMode?: string | null;
  creativeMode?: string | null;
}

// ---------------------------------------------------------------------------
// Frontend A group: accounts & live launch (kuaishou_auth / mate_login /
// live_launch / live_room_monitor). Mirrors:
//   crates/cdp-driver/src/platforms/kuaishou.rs (EnsureAuthResult, AuthPhase)
//   crates/tauri-app/src/driver/mate_login.rs
//   crates/tauri-app/src/driver/live_launch.rs
//   crates/tauri-app/src/driver/live_room_monitor.rs
// All structs use `#[serde(rename_all = "camelCase")]` except where noted.
// ---------------------------------------------------------------------------

/** `AuthPhase::as_str()` snake_case wire values for `kuaishou-auth-phase`. */
export type KuaishouAuthPhase =
  | "launching_browser"
  | "verifying_session"
  | "waiting_for_login"
  | "restoring_headless";

export interface KuaishouAuthPhaseEvent {
  profileId: string;
  phase: KuaishouAuthPhase;
}

/** Outcome of `ensure_kuaishou_auth` (cdp-driver kuaishou.rs:409). */
export interface EnsureAuthResult {
  ok: boolean;
  scanned: boolean;
  error?: string;
}

/** `MateLoginStage`, `#[serde(rename_all = "kebab-case")]`. */
export type MateLoginStage =
  | "idle"
  | "starting"
  | "awaiting-scan"
  | "awaiting-confirm"
  | "receiving"
  | "success"
  | "expired"
  | "cancelled"
  | "error";

export interface MateLoginUser {
  userId: string;
  userName: string;
  avatarUrl?: string | null;
}

/** Full login snapshot (driver/mate_login.rs:106). */
export interface MateLoginState {
  accountId: string;
  stage: MateLoginStage;
  qrImageDataUrl?: string | null;
  qrLoginToken?: string | null;
  qrLoginSignature?: string | null;
  expireAt?: number | null;
  errorMessage?: string | null;
  user?: MateLoginUser | null;
  startedAt?: number | null;
  finishedAt?: number | null;
}

/** Terminal stages: polling stops. */
export const MATE_LOGIN_TERMINAL_STAGES: readonly MateLoginStage[] = [
  "success",
  "expired",
  "cancelled",
  "error",
] as const;

/** `StreamingStatus`, `#[serde(rename_all = "camelCase")]`. */
export type StreamingStatus =
  | "idle"
  | "starting"
  | "streaming"
  | "stopping"
  | "stopped"
  | "error";

/** `StreamMode`, `#[serde(rename_all = "camelCase")]`. */
export type StreamMode = "heartbeat" | "realtime";

/** `StreamCredentials` (driver/live_launch.rs:153). streamKey never logged. */
export interface StreamCredentials {
  rtmpServer: string;
  streamKey: string;
  liveStreamId: string;
  placeholder: boolean;
}

/** `StreamingState` (driver/live_launch.rs:220). `target` is redacted. */
export interface StreamingState {
  profileId: string;
  status: StreamingStatus;
  mode?: StreamMode | null;
  target?: string | null;
  pid?: number | null;
  stderrTail: string[];
  exitCode?: number | null;
  error?: string | null;
  placeholderCredentials: boolean;
  startedAt?: string | null;
}

/** Subset pushed via `live-launch-state-changed`. */
export interface LiveLaunchStateChanged {
  profileId: string;
  status: StreamingStatus;
  mode?: StreamMode | null;
  target?: string | null;
  pid?: number | null;
  exitCode?: number | null;
  error?: string | null;
  placeholderCredentials: boolean;
}

/** `PrerequisitesReport` (driver/live_launch.rs:284). */
export interface PrerequisitesReport {
  available: boolean;
  ffmpegPath?: string | null;
  searched: string[];
  error?: string | null;
}

/** `MonitorConfig` (driver/live_room_monitor.rs:184). */
export interface MonitorConfig {
  liveRoomUrl: string;
  sceneId?: number | null;
  groupId?: string | null;
  productScriptId?: number | null;
  productScriptAccountId?: string | null;
  autoExitSubAccounts: boolean;
}

/** `LiveRoomMonitorStatus`, `#[serde(rename_all = "lowercase")]`. */
export type LiveRoomMonitorStatus =
  | "idle"
  | "checking"
  | "offline"
  | "live"
  | "triggering"
  | "triggered"
  | "error";

/** `LiveRoomLiveStatus`, `#[serde(rename_all = "lowercase")]`. */
export type LiveRoomLiveStatus = "unknown" | "offline" | "live";

/** Downstream batch result (enter/exit/product-script share shape). */
export interface TriggerBatchResult {
  attempted: number;
  succeeded: number;
  failed: number;
  message?: string | null;
  error?: string | null;
  at: string;
}

/** `LiveRoomMonitorState` (driver/live_room_monitor.rs:197). */
export interface LiveRoomMonitorState {
  enabled: boolean;
  profileId?: string | null;
  liveRoomUrl?: string | null;
  sceneId?: number | null;
  groupId?: string | null;
  productScriptId?: number | null;
  productScriptAccountId?: string | null;
  autoExitSubAccounts: boolean;
  status: LiveRoomMonitorStatus;
  liveStatus: LiveRoomLiveStatus;
  triggeredForCurrentLive: boolean;
  enteringRooms: boolean;
  exitingRooms: boolean;
  lastCheckedAt?: number | null;
  lastTriggeredAt?: number | null;
  nextCheckAt?: number | null;
  lastEnterAllResult?: TriggerBatchResult | null;
  lastExitAllResult?: TriggerBatchResult | null;
  lastProductScriptResult?: TriggerBatchResult | null;
  error?: string | null;
}
