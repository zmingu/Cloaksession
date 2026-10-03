# implement — sub-account

## 执行清单

### 1. 账号存储层
- [ ] 1.1 business_accounts 新增 kind `kuaishou_sub`（在 kind 枚举加变体）
- [ ] 1.2 小号 CRUD：`save_sub_account`/`list_sub_accounts`/`unbind_sub_account`（复用 business_accounts API，kind 固定）
- [ ] 1.3 分组：复用 Profile group，不新增表
- [ ] 1.4 `sub_account_interactions` 表 + migration（account_id/action/content/result/timestamp）

### 2. 浏览器会话与登录
- [ ] 2.1 `login_account(account_id, cancel)`：调 ks-platform-primitives ensure_auth（KUAISHOU_SUB_ACCOUNT_CONFIG）
- [ ] 2.2 后台登录轮询：`sub_login_poll` 每 3s 检查（KS_SUB_LOGIN_DETECT_MS=30s 初次头像 / KS_VERIFY_MS=20s / KS_LOGIN_MS=3min）
- [ ] 2.3 `browserCloseGrace=150ms` 宽限
- [ ] 2.4 `batch_login(account_ids, cancel)`：并发上限 3，每个登录间隔随机

### 3. 进入直播间
- [ ] 3.1 `enter_live_room(account_id, live_room_url, cancel)`：navigate + waitForLiveRoomReady
- [ ] 3.2 随机延迟 3-8s 防风控（KS_ENTER_ROOM_MIN/MAX_DELAY_MS）
- [ ] 3.3 RATE_LIMIT_RETRIES=5 重试
- [ ] 3.4 KS_ENTER_ROOM_DETECT_DELAY_MS=2.5s 检测延迟

### 4. 发弹幕（liveRoomActions 等价）
- [ ] 4.1 `pick_input(task_page, selector)`：多候选按可见/可用/位置/placeholder 评分
- [ ] 4.2 `pick_send_button(task_page, selector)`：按可见/可用/位置/“发送”文案评分
- [ ] 4.3 `fill_comment_input(input, message)`：兼容 input/textarea/contenteditable（evaluate JS 复刻 InputEvent dispatch）
- [ ] 4.4 `send_danmaku(account_id, message, options)`：click 发送，缺失回退 Enter，检查清空+回车补发
- [ ] 4.5 `detect_security_verification(task_page)`：9 关键词 + 8 DOM 选择器 + URL 命中
- [ ] 4.6 `SubAccountVerificationRequiredError` 等价错误类型
- [ ] 4.7 sendLocks per-account 串行（tokio::sync::Mutex per-account）

### 5. 互动历史与运行时状态
- [ ] 5.1 `record_interaction(account_id, result)` 落库 sub_account_interactions
- [ ] 5.2 SubAccountRuntime/LiveRoomStatus 结构体 + RwLock<HashMap> per-account
- [ ] 5.3 `get_runtime(account_id)` / 状态变化广播

### 6. IPC 命令层
- [ ] 6.1 `crates/tauri-app/src/commands/sub_account.rs` 新建
- [ ] 6.2 CRUD: `save_sub_account`/`list_sub_accounts`/`unbind_sub_account`/`rename`
- [ ] 6.3 `batch_login`/`login_account`/`cancel_login`
- [ ] 6.4 `enter_live_room`/`exit_live_room`
- [ ] 6.5 `send_danmaku`（暴露给 auto-message/scene-play 调用）
- [ ] 6.6 状态变化 emit + `commands/mod.rs` 注册 + 前端 ipc.ts wrapper

### 7. 验证
- [ ] 7.1 `cargo test -p tauri-app --locked`（pick_input/send_button 评分 + 安全验证检测 + send_danmaku mock）
- [ ] 7.2 `cargo test -p profile-manager --locked`（sub_account_interactions migration + CRUD）
- [ ] 7.3 `cargo check --workspace --locked`
- [ ] 7.4 真号小号登录/发弹幕验收（单独确认）

## 验证命令
```bash
cargo test -p tauri-app --locked
cargo test -p profile-manager --locked
cargo check --workspace --locked
```

## Rollback 点
- 1 存储层 commit
- 2-3 登录+进直播间 commit
- 4 发弹幕 commit
- 5 互动+状态 commit
- 6 IPC 层 commit
- 7 验证后准备 archive

## 依赖前置
- `ks-platform-primitives`（ensure_auth 用 KUAISHOU_SUB_ACCOUNT_CONFIG）
- `comment-listener`（互动历史与评论流协作）

## Notes
- 小号是观众身份（kuaishou.com），与小店中控（zs.kwaixiaodian.com）不同。
- send_danmaku 是 auto-message/auto-reply/scene-play 的共用接口，定义稳定签名。
- 安全验证检测的 9 关键词 + 8 DOM 选择器全量迁移。
