# mate-login — 直播伴侣扫码登录迁移

## Goal
迁移 jieger 的直播伴侣 HTTP QR 登录 4 步状态机：start → scanResult(long poll) → acceptResult(long poll) → receive。

## jieger 源文件
- `electron/main/tasks/mateLogin/index.ts`（387 行）— 4 步状态机 + QR 渲染 + user 信息提取
- `electron/main/ipc/handlers/mateLoginHandler.ts`（31 行）— IPC
- `electron/main/services/kuaishou/liveMate.ts` — 推流 token 相关

## 功能点
1. **MateLoginStage 状态机**：`idle|starting|awaiting-scan|awaiting-confirm|receiving|success|expired|cancelled|error`。
2. **4 步 QR 登录**：
   - `start`：POST `qr.kuaishou.com/rest/q/user/login/qrcode/start` 取 QR token + signature + imageDataUrl。
   - `scanResult`：long poll `id.kwaixiaodian.com/rest/c/infra/ks/qr/scanResult`（等用户扫码）。
   - `acceptResult`：long poll `acceptResult`（等用户手机确认）。
   - `receive`：`qr.kuaishou.com/rest/q/user/login/qrcode/receive` 取 userId/userName/avatarUrl。
3. **MateLoginState 广播**：stage + qrImageDataUrl + expireAt + user 信息，经 IPC 推渲染端展示 QR。
4. **超时与重试**：`POLL_TIMEOUT_MS=70s`、`POLL_RETRY_DELAY_MS=1.5s`、`POLL_MAX_RETRIES=5`、`ONE_SHOT_TIMEOUT_MS=20s`。
5. **取消**：AbortController 等价物（tokio 任务取消）。
6. **SID**：`kuaishou.shop.b`。

## 依赖
- 前置：`ks-platform-primitives`。
- 协作：`live-launch`（登录态供推流用）。

## 验收标准
- [ ] 4 步状态机在 Rust 等价实现，HTTP long poll + 重试 + 超时。
- [ ] QR 图片数据经 Tauri event 推前端展示。
- [ ] 支持 cancel（tokio::select! 中断 long poll）。
- [ ] 离线门禁通过（HTTP mock 测试）；真号扫码验收单独确认。

## Out of Scope
- 不迁移旧登录态。
- 不做直播伴侣客户端本身的安装/配置。

## Notes
- 这是 HTTP API 直连，不走浏览器页面；但仍需与浏览器登录态协作（live-launch）。
- UA 头：`Mozilla/4.0 (compatible; MSIE 9.0; Windows NT 6.1)` 与 `kuaishou 5.105.2.3505` 两个，需在 Rust HTTP client 复刻。
- 关联 spec：`.trellis/spec/tauri-app/backend/ipc.md`。
