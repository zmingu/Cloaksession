# 技术设计

## 边界与数据流

BrowserSession::bind_page(target_id) -> chromiumoxide Browser::get_page -> BoundPage<'_> 持有 Page 与 &BrowserSession。生命周期借用阻止绑定比 session 活得更久，engine/safe 策略继续归 session 所有。操作只引用持有的 Page，不重新选择 active_page，也不重试到别的 target。同一页面仍可能被多个绑定/旧工具同时操作；不是锁、租约或完整任务调度器。

## 共享与兼容

把 tools.rs 的实际页面操作移动至内部 page_ops.rs，内部 PageOperations 接受 session、固定 Page 与显式 Legacy/Bound 策略。BrowserSession 的公共工具方法仍各自选择 active_page；旧 navigate 的创建/回退路径保留。BoundPage 调用同一实现的严格策略：Input/focus 错误传播，focus=false 报错；导航的 timeout 包住完整页面操作（取消等待不撤销已发送导航）。旧 Input 忽略错误及导航等待语义保持，不借本批全修。

chromiumoxide 0.9.1 Page::screenshot 内部调用 activate；绑定截图改用固定页 Page.captureScreenshot，返回同样 PNG/base64，不激活。旧截图继续原有路径。

new_bound_page 使用 CreateTargetParams 的 background=true 请求创建背景页，不改 session.active_page；浏览器是否尊重背景请求不能由库保证。bind_page 只查附着 target，不调用 Target.activateTarget/Page.bringToFront。目标关闭后命令经原 Page/session 通道报错，无重新查找、重新附着或 fallback。

## 文件与验证

改动 session.rs（入口和注释）、tools.rs（兼容薄封装）、lib.rs（内部模块）、新增 bound_page.rs/page_ops.rs；测试增加本机随机端口模拟 HTTP + WebSocket CDP，使用真 chromiumoxide handler 驱动，不启动 Chromium。增加测试依赖及 lock 直接依赖边；相关 spec 和本任务文档同步。

不改 Tauri/MCP/Chromix 调用者。模拟覆盖两目标路由、active 切换、未知/关闭目标、命令错误、无激活、旧方法兼容。真实浏览器行为（DOM、输入焦点、引擎兼容）不由模拟证明。
