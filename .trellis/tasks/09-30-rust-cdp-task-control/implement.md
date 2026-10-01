# 实施计划

授权：第一批完成后用户回复“继续”，本批基础接口已批准，不是新的业务账号关系决定。独立续作，不添加跨任务 DAG。

1. [x] 读取前批 review、session/BoundPage/page_ops、规范与复用指南；检查 task.py/hooks（无启用 hooks），进程内 mock Git、禁止 Popen 创建任务。
2. [x] 补齐契约/context 并以相同无Git策略 start，清空未查询的 base_branch。
3. [x] 实现 session 级弱锁、TaskPage/TaskCancel、整步 typed deadline 与 selector waits。
4. [x] 提取现有 CDP peer；加离线路由/排队/取消/超时/关闭/abort/等待错误测试，明确模拟不执行 JS。
5. [x] 仅格式本批 Rust 文件；cargo test -p cdp-driver --locked（38通过/0失败/1忽略，含1个doctest）；cargo test --workspace --locked（177通过/0失败/2忽略）；cargo check --workspace --locked通过。禁止 ignored tests。
6. [x] 更新 spec/results，保留 in_progress 交主代理复审，不 commit/archive。最终证据和自身测试同步问题修正见 results.md。
