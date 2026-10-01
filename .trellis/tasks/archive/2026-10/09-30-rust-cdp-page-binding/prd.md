# Rust/CDP 指定页面绑定基础

## 目标与批准

2026-09-30 用户在明确第一批边界后回复“行，开始做吧”：业务自动化统一 Rust + chromiumoxide，保留 Chromix Node 启动桥，不新增 Node 业务 worker。授权创建并实施本独立任务；不是账户初始化任务，也不是全功能迁移完工。

## 需求与约束

- 长任务持有指定 target 的页面句柄，不受 session.active_page 切换影响。
- 绑定不激活页面、不改变 active_page；不存在或已关闭的目标报错，禁止回退到其他页面。
- 提供导航、求值、截图、点击、输入与文本提取；新操作传播底层失败。
- 保留已有 BrowserSession 方法签名及 MCP/Tauri 的 active-page 行为，复用页面操作逻辑而非复制整套工具。
- 可以新建不修改 active_page 的任务页；浏览器实际前台行为须如实描述。
- 不接入业务 UI、账户模型、调度/互斥、取消框架、真实开播或投放；不修改 Chromix 桥，不修改 F:/jieger，不运行任何 Git 命令。

## 验收标准

- 两个目标可独立绑定，切换 active 后旧绑定仍操作原页；未知 target 不回退。
- 模拟 CDP 实际收发命令验证成功、失败及已关闭页面，不能仅用源码字符串断言。
- 既有 active-page 接口保持兼容；绑定操作不发送激活命令。
- cargo test -p cdp-driver --locked 与 cargo check --workspace --locked 通过或明确列出既有阻塞。
- 不自动连接用户浏览器、不运行现有会关闭浏览器的 ignored integration；真实浏览器验证须单独报告。
