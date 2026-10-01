# Rust/CDP 任务控制基础（第二批）

## 目标与批准

用户已批准 Rust + chromiumoxide，第一批后回复“继续”。本独立续作只为 cdp-driver 增加 fixed-target 任务控制，不替用户决定账号/Profile关系。

## 需求与验收

- 同 BrowserSession 同 target 的新 TaskPage 接口串行，多个 binding 共锁；不同 target 和不同 session 不串锁。排队可取消/超时，得锁后确认 target 仍在，无 active 切换/fallback。
- 一次性可克隆取消句柄；预取消不发命令，排队/轮询/慢 CDP await 可结束；typed Cancelled/TimedOut/Driver，取消/超时不能继续后续动作。所有动作可变借用，租约不暴露绕锁入口，drop/release释放。
- attached/detached/visible/hidden 等待；JSON编码 selector，正 poll_interval，whole operation deadline，JS/CDP/选择器错误立即失败，不冒充未出现。
- 复用 BoundPage 的 navigate/evaluate/screenshot/click/type_text/extract，不复制 page_ops，不污染旧接口。
- 离线真 chromiumoxide handler + 模拟 HTTP/WebSocket 验证多页路由、同页排队/释放、多binding、取消/超时、错误无fallback和 abort/drop。模拟不执行真实 JS/DOM。
- 执行三条 --locked 质量门禁并记录精确计数；保留 in_progress 供复审。

## 范围之外与限制

不改业务/UI/金牛/账号/Chromix桥，不新增Node worker/依赖版本，不触发真实浏览器/平台，不跑Git（包括脚本隐含调用），不跑ignored浏览器关闭测试，保留前批Windows测试修复。互斥仅新接口协作，不覆盖legacy/用户/另一个BrowserSession。取消不撤回已派发CDP，不证明浏览器停止；输入可能停在按下未释放，不自动重试副作用。
