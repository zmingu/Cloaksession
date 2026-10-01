# 绑定基础源码调查

- session.rs active_page 为 Mutex<Option<Page>>，Tauri ProfileRegistry 持有 Arc<BrowserSession>；多调用并不构成原子任务。driver.rs 622-657 和 705-717、mcp-server/tools.rs 的现有调用保持不动。
- chromiumoxide 0.9.1 browser/mod.rs get_page 使用 HandlerMessage::GetPage，未匹配返回 NotFound，不激活；new_page 接受 CreateTargetParams，可设置 background。不能保证外部浏览器尊重背景创建。
- chromiumoxide handler/page.rs screenshot 首先 activate()，所以 BoundPage 截图需直接 CaptureScreenshotParams，避免隐藏切页。
- 旧 tools.rs click/type_text 丢弃部分错误，navigate timeout 只包 sleep。本批对旧入口保留，通过显式内部策略令新绑定入口传播错误，避免新业务把关闭目标误判为成功。
- 迁移研究 .trellis/tasks/09-30-kuaishou-account-init/research/jieger-full-migration.md 是历史研究；其中独立 Node worker 推荐已被用户否决，当前决策是 Rust+chromiumoxide，Chromix Node 仅保留原启动桥。
- task.py create 无条件查当前分支，start 在分支为空时查分支；config.yaml 的生命周期 hooks 仅注释。执行时进程内 mock Git 查询并禁止 Popen，保留创建/上下文门禁/启动状态逻辑。branch/base_branch 不伪造。
