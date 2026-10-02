# 第二批实施结果（2026-09-30）

## 交付与状态

任务 `.trellis/tasks/09-30-rust-cdp-task-control` 保持 **in_progress**，已完成本批实施和离线门禁，等待主代理复审；未 commit/archive。前批最新基线为 158通过/0失败/2忽略，本批新增 19 个通过项后为 177/0/2。

- TaskPage 在 BoundPage 之上持有 session/target 的 RAII 租约。多 binding 共用 session 内 Weak 锁表；不同 target/BrowserSession 独立。排队得锁后 re-bind 目标，无 activation/fallback。
- TaskCancel 复用现有 tokio watch；可克隆、单向取消、不因 handle drop 自动取消。没有新依赖，也未改 Cargo.toml/Cargo.lock。
- 统一 TaskError: Cancelled、TimedOut、Driver(MultizenError)、InvalidOptions、Interrupted。取消/timeout/已开始动作 future 被 drop 后不能继续使用同租约发动作。
- navigate/evaluate/screenshot/click/type_text/extract 复用 BoundPage；内部 navigation 入口避免旧字符串 timeout 抢先遮蔽 typed timeout。旧 API 签名/结果契约不变，page_ops.rs/tools.rs 未修改。
- wait_for_selector 支持 Attached/Detached/Visible/Hidden，JSON selector 编码，返回 attached/visible 观察对象后严格反序列化；正 poll_interval；deadline 包括 evaluate 和 sleep。JS/CDP/非法观察结果立即 Driver。

完整签名和矩阵：`.trellis/spec/cdp-driver/backend/task-control.md`。主要新增入口：

```rust
BrowserSession::task_page(&self, target_id: &str, cancel: TaskCancel, timeout: Duration)
    -> TaskResult<TaskPage<'_>>
BoundPage::into_task(self, cancel: TaskCancel, timeout: Duration)
    -> TaskResult<TaskPage<'session>>
TaskPage::wait_for_selector(&mut self, selector: &str, state: SelectorState,
    timeout: Duration, poll_interval: Duration) -> TaskResult<()>
```

其余 TaskPage 动作均 `&mut self`，每次追加 `timeout: Duration` 参数；target_id只读、release(self)，无Clone/Deref/raw binding。timeout为每次 acquisition/operation，不是整个多步骤任务寿命预算。

## 最终验证（全部退出码0）

| 命令 | 精确结果 | 证据 |
| --- | --- | --- |
| `cargo test -p cdp-driver --locked` | 38通过，0失败，1忽略（含1个compile-fail doctest） | cdp-driver-tests.log |
| `cargo test --workspace --locked` | 177通过，0失败，2忽略（含1个compile-fail doctest） | workspace-tests.log |
| `cargo check --workspace --locked` | 通过，无编译错误 | workspace-check.log |
| `rustfmt --edition 2021 --check --config skip_children=true` + 本批7个Rust文件 | 通过 | 终端结果 |

测试计数通过逐个 `test result` 汇总复核。新增 15 项任务 wire 测试 + 3 项单元测试 + 1 项 &mut 互斥编译失败测试；原 6 项 page_binding wire 测试继续通过。全 workspace 包括前批 Windows pathToFileURL/平台路径测试，原文件未改，7项 Chromix fixture测试通过。既有 registry_smoke 的缺失endpoint测试约50秒，本批未改其重试行为。

### 过程中的自身问题与修正

首轮 14 wire+3 unit 测试和三项门禁通过（175/0/2）。补强轮询sleep验证时两项新测试曾超时：只 poll 一次 Page::evaluate 后等 peer，chromiumoxide 尚可能停在内部 context 查找，动作没有继续 poll，CDP不会被发送。已将动作和 peer 通知放入 select 共同驱动，使用另一个 target 的 wire 往返作为响应屏障后 poll 入sleep，再取消。没有靠加长sleep掩盖问题，没有产品代码变更；最终15项和全部门禁重跑通过。最终日志覆盖的是修正后的完整成功运行。

## 测试证据范围

- 多binding同锁、排队释放/drop、取消/timeout不抢锁、已取消与可得锁同时ready时取消优先、不同target/session并行。
- 全受控方法固定target路由、legacy active独立、原始BoundPage不受协作锁覆盖。
- selector出现/消失/visible/hidden观察序列、JS exceptionDetails/CDP/观察格式错误、非法poll_interval。
- 单个慢evaluate/导航/selector整体timeout，慢evaluate及poll sleep取消，终止后无新dispatch。
- keyDown/mousePressed响应挂起时取消后无release/后续key派发，真实wire记录加B页round-trip屏障。
- abort owner自动释放、drop动作future使保留租约Interrupted、drop排队future安全、队列中目标关闭后rebind失败无fallback。

HTTP/WebSocket peer 通过 filesystem-copy 从既有 page_binding 提取，连接真实chromiumoxide handler；仅模拟协议，不执行JS、CSS选择器、真实DOM、PNG渲染或平台业务。观察数据/异常是脚本化响应，不能宣称真实DOM/输入actionability/引擎安全已验收。

## 改动文件

生产Rust：
- 新 `crates/cdp-driver/src/task_page.rs`
- 改 `crates/cdp-driver/src/session.rs`（session锁表）、`bound_page.rs`（into_task/内部导航）、`lib.rs`（导出）

测试Rust：
- 新 `crates/cdp-driver/tests/common/mod.rs`（提取并扩展离线peer）
- 新 `crates/cdp-driver/tests/task_control.rs`
- 改 `crates/cdp-driver/tests/page_binding.rs`（原测试体保留，导入common）

规范：改 `.trellis/spec/cdp-driver/backend/index.md`、`sessions.md`，新 `task-control.md`。
任务：task.json/prd.md/design.md/implement.md/implement.jsonl/check.jsonl/results.md，以及3份最终门禁日志；task.py记录当前子代理session的runtime任务指针。

## 限制与安全边界

互斥只覆盖同BrowserSession的新TaskPage，不能阻止legacy、普通binding、用户、另一个session或页面脚本。取消/timeout会保留锁直到release/drop；释放不等于浏览器停止。取消不能撤回已queued/dispatched的CDP，已发keydown/press可能没有释放，禁止当浏览器事务回滚或自动重试副作用。普通Driver错误不会终止租约，但绝不自动换页/重试。

visible仅computed display/visibility+正尺寸，不检查opacity/遮挡/viewport/稳定性/enabled/完整actionability。超时是Tokio协作式等待deadline，不是同步CPU工作强制抢占。Weak死项在下次acquisition清扫，无永久强锁表。

没有真实浏览器/平台操作，没有Node worker/Chromix桥/业务UI/金牛/账号模块改动，没有Git操作。task.py create/start在进程内patch common.git.run_git为空结果，并禁止subprocess.Popen；已检查无启用生命周期hooks，脚本未改，默认伪造main清空为null。后续账号关系/Profile隔离/业务接入均留给另行确认，不是本批决定。
