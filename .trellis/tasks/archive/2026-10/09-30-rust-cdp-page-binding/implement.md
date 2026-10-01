# 执行计划

批准依据：用户已同意本批 Rust/CDP 绑定路线并明确回复“行，开始做吧”。

1. [x] 读取本包 spec、共享指南、迁移研究及 Tauri/MCP 调用边界。
2. [x] 检查 task.py create/start 与 hooks；创建独立任务并补齐规划和上下文。
3. [x] 激活任务；抽取固定页面共用操作，实现 BoundPage 和不更新 active 的新建入口。
4. [x] 增加浏览器免费模拟 CDP 测试并验证目标/错误/兼容契约。
5. [x] 运行 cargo test -p cdp-driver --locked（19通过/1忽略）；cargo check --workspace --locked（通过）；仅格式化本批 Rust 文件。
6. [x] 更新相关 spec 与执行结果，交主代理复审。保持任务 in_progress，不 commit/archive，不宣称全功能迁移完成。

执行证据与局限见 [results.md](./results.md)。主代理已初步复审并追加全 workspace 验证；真实浏览器验证未执行，最终门禁不得因本批清单完成而视为通过。

## 复审追加：Windows 测试夹具路径（2026-09-30）

主代理运行全 workspace 测试发现既有 browser-launcher/tests/chromix.rs 5 项失败：canonicalize 的 Windows extended-length 路径被手拼 file://，Node 报 ERR_INVALID_FILE_URL_PATH。主代理已告知用户并授权此次仅修测试夹具，不改产品启动桥。

- [x] 夹具改用 Node pathToFileURL + 动态 import，补普通/规范化扩展路径以及中文/空格/#/% 文件名的真实 Node 导入测试。
- [x] 复核 page_ops Legacy/Bound、绑定截图和 background create 路由：未发现新增问题，无需再改生产代码。
- [x] 已运行 cargo test --workspace --locked（仍因既有 Windows 路径断言失败）、cargo check --workspace --locked（通过）；追加 --no-fail-fast 得到 157通过/1失败/2忽略。执行已完成，但全 workspace 测试门禁未通过；未扩域修改剩余断言。

## 安全与复审

本批不运行 Git、不操作 F:/jieger、不连接真实用户浏览器、不运行 tests/integration.rs 的 ignored 用例。task.py 内含 Git 查询，通过本次 Python 进程内替换查询为无结果，并禁止 subprocess.Popen；不更改脚本。创建脚本默认填入的 main 将清空为 null，避免伪造分支。复审需核对旧工具选择/错误策略不变、新 API 不回退与截图不激活。

若需要撤回，只应由主代理基于实际文件差异审查本批修改，未经用户授权不得 Git 回滚。模拟测试不是平台 E2E；真实浏览器测试另行授权。
