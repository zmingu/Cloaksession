# ks-platform-primitives — 快手平台连接原语与配置迁移

## Goal
迁移 jieger 的快手平台连接三原语（connect/login/ensureAuth）与三套平台配置到 Rust+chromiumoxide，作为所有直播/互动/商品子任务的共用基础层。复用已实现的 `cdp-driver::TaskPage`/`BrowserSession`，不另起封装。

## jieger 源文件
- `electron/main/platforms/kuaishou/connection.ts`（198 行）— 三原语 `kuaishouConnect`/`kuaishouLogin`/`ensureKuaishouAuth`
- `electron/main/utils/platformConfig.ts`（159 行）— `KUAISHOU_CONFIG`/`KUAISHOU_SUB_ACCOUNT_CONFIG`/`KUAISHOU_JINNIU_CONFIG` 三套配置 + `extractJinniuAccountIdFromUrl`
- `electron/main/platforms/kuaishou/selectors.ts` — 选择器常量
- `electron/main/platforms/kuaishou/config.ts` — 平台辅助配置

## 功能点
1. **kuaishouConnect**：goto 中控页 → race（登录页 URL 命中 = 未登录 vs `inLiveControlSelector` 出现 = 已登录）→ 返回是否已登录。
2. **kuaishouLogin**：确保在登录页 → 等待 `loggedInSelector`（昵称元素）出现（扫码完成硬指标）。
3. **ensureKuaishouAuth**：完整流程——headless 启动 + cookie 复用尝试 → connect 校验 → 失败切 headful 扫码 → 成功后按需切回 headless + 重校验。支持取消信号与阶段回调（`launching_browser`/`verifying_session`/`waiting_for_login`/`restoring_headless`）。
4. **三套平台配置**：
   - 小店中控：`loginUrl`/`liveControlUrl`(zs.kwaixiaodian.com/page/helper)/`storeHomeUrl`/`storeLoginUrl` + verify（URL pattern + DOM 选择器 + cookieKey `sessionid` + localStorageKey `user_info`）。
   - 小号观众：`loginUrl`(kuaishou.com)/`liveRoomUrlTemplate`/评论输入框/发送按钮选择器。
   - 磁力金牛：`loginUrl`/`homeUrl`(niu.e.kuaishou.com/home?homeType=new) + `buildStartupUrl(__accountId__)` + 账号选择弹窗/顶栏 popover 选择器启发式数组 + `extractJinniuAccountIdFromUrl`。
5. **金牛 homeType=new 强制策略**：快手命名是反的（new=旧版，super=新版），所有金牛 URL 强制 `homeType=new`，避免新版 popover 结构变化导致余额/账号信息提取失败。

## 依赖
- 无前置子任务（波 0，所有其他子任务依赖本任务）。
- 复用已有：`cdp-driver::TaskPage`（task-control spec）、`BrowserSession`、`browser-launcher` 的 Chromix 启动。

## 验收标准
- [ ] Rust 模块提供三原语等价 API，复用 TaskPage/BrowserSession，无重复 connect/login 逻辑散落各子任务。
- [ ] 三套平台配置以 Rust 常量/结构体落地，URL/选择器可配置（真号校准时改选择器不改逻辑）。
- [ ] `ensureKuaishouAuth` 支持 cancel + 阶段事件广播（Tauri event）。
- [ ] 离线门禁：`cargo test -p cdp-driver --locked` + `cargo check --workspace --locked` 通过。
- [ ] 真号扫码登录集成验收单独确认（涉及真实登录态）。

## Out of Scope
- 不迁移旧登录态/cookie/storageState。
- 不实现账号选择弹窗的自动子户搜索/选择（jieger 已废弃此路径，用户必须手动选子户）。
- 选择器在真号上重新校准属后续各业务 task 的事，本任务只提供配置载体。

## Notes
- jieger 选择器可能已过期，本任务只迁配置结构，不保证选择器值有效。
- 关联 spec：`.trellis/spec/cdp-driver/backend/{index,task-control,sessions}.md`、`.trellis/spec/browser-launcher/backend/business-isolation.md`。
