# 设计与边界

最小缺口：BoundPage 固定 target 但没有同页任务租约、整步取消和 selector 等待。新增 task_page.rs 在驱动层负责，BoundPage 仍共享 page_ops，MCP 的等待只作参考，不反向依赖。

- BrowserSession 持有 Weak<tokio::sync::Mutex<()>> 表；按 session + target 隔离。获得 OwnedMutexGuard 后重新 bind，未知/关闭目标报 Driver，无 active 变更/fallback。每次取锁清除失效 Weak；表不强持有锁。
- TaskPage 私有持有 BoundPage/guard/cancel/state，所有动作 &mut self，不实现 Deref、不提供 raw binding。release(self)/drop 释放。只有新接口协作互斥，不限制 legacy、普通 binding、用户和另一 BrowserSession。
- TaskCancel 基于现有 tokio watch，Clone、单向 true、无重置。统一 TaskError::Cancelled/TimedOut/Driver/InvalidOptions/Interrupted。每次 acquisition/operation 接受 Duration；deadline 覆盖排队+验证、单个 CDP await、轮询及 sleep。biased select 先取消再 deadline 再动作，零 timeout 不派发。
- 超时/取消后租约终止；已开始但被调用方丢弃的动作 future 也以 Interrupted 终止租约。普通 Driver 错误传播，不做 retry。取消只是停止本地等待/后续 dispatch，不撤回已发送命令、不证明浏览器停止；多步输入可能停在 keyDown/mousePressed 后，没有浏览器事务回滚。
- selector 用 serde_json 编码，attached/detached/visible/hidden：visible=存在+computed display非none、visibility非hidden/collapse、正宽高；hidden 包括不存在。不测遮挡/viewport/opacity/稳定性/可点击性。invalid selector/JS/CDP/观察对象反序列化错误直接失败。poll_interval>0。
- navigate 复用 BoundPage 内部无额外计时器入口，外部 TaskPage 给 typed timeout；旧 BoundPage 毫秒/错误接口保持。

文件：新增 task_page.rs/tests/task_control.rs；session.rs 的锁表字段；bound_page.rs 的租约/内部导航入口；lib.rs 导出；提取测试 peer 到 tests/common/mod.rs；spec 与任务文档。无新依赖/lock更新，无业务/UI/账号/金牛/Chromix桥/Node worker改动，无真实浏览器/平台/Git。保留前批 Windows pathToFileURL 测试修复。
