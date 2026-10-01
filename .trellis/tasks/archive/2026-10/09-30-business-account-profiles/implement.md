# 实施与验证

授权：本轮主代理已说明登记入口、独立目录与持久scope；用户连续批准继续，按本次明确契约实施。

1. [x] 读取AGENTS、四层backend完整规范、shared guides、真实manager/launcher/driver与只读Chromix bridge、原账号初始化PRD。
2. [x] 检查task.py/hooks；原create/start使用进程内mock common.git.run_git、subprocess.Popen禁止。本轮发现任务已存在，复核后安全validate/start，未重复创建、未改脚本、未使用Git。
3. [x] 创建模型、迁移与事务CRUD，补存储回归；本轮修正显式空id隐式新建，新增nullable清空和UPDATE回滚测试。
4. [x] 实现有效目录解析、运行快照、双向隔离与候选更新门禁；无相关金牛scope时普通Profile保留旧行为。
5. [x] 增加Tauri async helpers/命令注册及driver模拟测试；失败有中文操作提示。
6. [x] 仅格式本批21个Rust文件，cargo test --workspace --locked：202通过/0失败/2忽略；cargo check --workspace --locked通过。
7. [x] 更新backend specs和results-backend.md；保持in_progress，不commit/archive，交主代理/UI集成。
8. [x] UI登记/编辑/解绑/孤立档案重绑定完成；主代理独立复跑UI build与34项mock界面测试全部通过，见review.md。
9. [x] 主代理完成跨层字段/命令/共享门禁复审并独立复跑Rust测试202通过/0失败/2忽略、cargo check通过；真实平台及原生Tauri端到端联调未执行，不能视为已验收。

基线177通过/0失败/2忽略。本批不运行ignored真实浏览器测试。任一不可解决风险如实报告，不用空API或前端提示替代后端验证。
