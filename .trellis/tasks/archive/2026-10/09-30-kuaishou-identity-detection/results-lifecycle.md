# 并行收尾 B：生命周期与事件合同复核

2026-09-30。仅修改 `crates/tauri-app/src/driver.rs`、`registry.rs`、`driver/identity/tests.rs`，及 backend `ipc.md` / `runtime.md`。未修改 UI、identity/mod.rs / extract.rs / avatar.rs、cdp-driver、Cargo 配置或依赖。未执行 Git；未读取用户 settings/db；未连接真实平台、CDN或用户浏览器。测试使用临时 SQLite、既有协议 fake Node launcher 和不可达 loopback CDP endpoint。任务保持原状，TODO 由主代理管理。

## 确证修复

1. `RunningStateChange` 加 `rename_all_fields = "camelCase"`：三个 struct variants 均产生 `profileId`，而不是只改 enum variant 名。精确 JSON 测试覆盖 launched/closing/closed、closed.reason，并断言不存在 profile_id。App.tsx 的监听读取 change.profileId，但最后还会无条件 refresh；原 bug 不能描述为按钮永久禁用。backend/ipc.md 已更新为已修事实。
2. **旧 attach 失败不能关闭新浏览器代次**：原 driver 的 Chromix attach Err 无条件发送 `LauncherCmd::Close { profile_id }`。若旧连接在 close/reopen 后被取消，这条迟到清理会关闭新进程。现在 driver 捕获原 Arc slot，发送内部 `ClosePrepared`；launcher 串行循环执行原子 `registry.remove_current`，仅命中原 slot 时才关闭进程。回滚回复也不清除已有 replacement 的 running cache。
3. **bootstrap 后不再无条件写 running**：最终 running cache 与 launched/started 成功事件在 `registry.with_current` 的同一锁内提交；取消/替换后返回 launch invalidated error，不写回旧状态。
4. **旧 close 回复不清新 running**：close 的最终 cache / closed / stopped 发布使用 `with_absent`；若 launcher 已准备 replacement slot，旧回复不再覆盖新状态。

这些变化只影响内部 launcher/registry 同步，不改 IPC 命令、参数或 LaunchedProfile / Snapshot 返回合同。

## 本次新增四项测试

- `running_events_have_exact_camel_case_payloads`：三种事件精确 JSON。
- `old_attach_failure_rollback_carries_original_slot`：真实 `driver.launch` future 用手动 poll 精确停在 pending attach；remove/prepare 新代次后恢复，要求发送带原 Arc 的 ClosePrepared，而不是无条件 Close；迟到失败不清新 cache/slot。仅控制 launcher reply 时序，非真实浏览器。
- `launcher_rejects_old_generation_rollback_after_reopen`：复用已有 fake launcher，走真实 LauncherCmd 循环 launch → close → reopen → 旧 ClosePrepared，验证新进程 pid/startedAt 保持、slot 未变；当前代次 ClosePrepared 可关闭。不是单纯比较 UUID 的测试。
- `late_close_reply_does_not_clear_reopened_running_cache`：真实 `driver.close` future 在 reply 前暂停，准备新 slot 后返回旧成功回复，cache 保持 running、slot 保留。

## 只读复核结论

- prepare 复用同 endpoint/engine/marker 的未取消 slot 和启动网络快照；替换/移除取消旧 slot。
- OnceCell 共享连接；connect_slot 在取消和连接完成后的 current 回查中拒绝旧结果，连接自身不重新插入 registry。
- get_or_connect 会创建 slot，但目前应用 launch 不调用它，仅 registry smoke test 使用；应用走 prepare-before-reply + connect_prepared，旧 reply 不会重新创建已删除 entry。
- close / delete / shutdown 的 launcher 分支使 registry 失效；identity Save 在 registry current 锁下回查业务状态/期限/取消，已有 launcher-persistence 回归同时通过。
- App setup 挂一次 monitor；Exit 调 stop；poller 只在活跃任务持有强 driver 引用，idle Weak。没有改动 A 负责的 identity 模块。

## 实际验证

最终执行：

- `cargo test -p tauri-app --lib --locked`：**34 passed, 0 failed, 0 ignored**。B 新增4项；该总数同时包含 A 并行提交的头像测试，不冒充全部都是 B 新增。
- `cargo check -p tauri-app --locked`：通过，无编译警告。
- rustfmt 仅 B 的三个 Rust 文件，skip_children=true。
- 日志：`lifecycle-test.log`、`lifecycle-check.log`。出现短暂 Cargo build-dir 锁等待，随后全部通过；无待解决并发编译错误。

## 保留局限 / 未验收

- 未运行真实浏览器/bootstrap/native UI E2E；bootstrap 最终成功发布的 current-guard 已代码核对，但本轮没有完整成功 CDP/bootstrap wire 集成用例。四个新增测试覆盖合同、实际 driver future 时序和真实 launcher 命令循环，不宣称覆盖完整启动链路。
- UI running 是原有保守缓存；外部进程退出时 identity 可清 registry，但不会自动同步该缓存。已有 legacy liveness / channel-error / bootstrap-error 清理局限未扩大修复。
- chromium:status 的旧失败诊断仍可能晚到（当前 UI onChromiumStatus 为 no-op）；本次保证 running/成功事件/实际进程清理不覆盖 replacement，不宣称所有诊断事件具备代次 DTO。
- launch reply 的匹配仍依赖现有 startedAt:pid marker，slot 内提交/清理使用 Arc 身份；未改变前后端合同。
