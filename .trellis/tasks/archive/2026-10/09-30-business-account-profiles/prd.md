# 第三批：业务账号 Profile 关联与金牛独立环境

## 授权与边界
用户已批准 Rust/CDP 迁移并连续要求继续，本次明确批准登记入口（不是登录）、实际目录隔离、解绑不清 Cookie 因而持久保留专用 scope。原 09-30-kuaishou-account-init PRD 仅存档，本批不实施登录识别、OCR、主体信息或平台操作。无旧数据导入、无密码/token、无新增 Node worker、不改 jieger、不执行 Git、不 commit/archive。

## 需求
- 业务类型 kuaishou-shop/live/mate/sub 和 jinniu；别名必填（trim，最多100字符），平台ID空白归null（最多128字符），只表示手工登记 metadata。
- 每记录仅一个 Profile，每 Profile 最多一个记录；kind 不可改；有 id 编辑/重绑定原记录，绑定其他 Profile 必须先显式解绑；同kind+非空平台ID唯一，不自动合并。
- 列表含孤立记录；解绑保留账号、目录、Cookie和scope；删Profile账号外键 SET NULL，scope CASCADE。
- scope 首次绑定确定为 kuaishou/jinniu，跨scope重新绑定拒绝。
- 绑定/编辑/解绑校验真实launcher运行状态，仅停用时允许。
- 金牛有效数据目录不能与任何其他Profile的有效目录重叠；反向启动亦拒绝。绑定前、共用UI/MCP启动前以及有关options更新前验证。运行快照不能被后来的DB编辑绕过。
- 保留显式Chromix顶层userDataDir及未知配置；按global.with_profile_options浅合并。嵌套目录与raw目录args不能绕过。特殊路径无法可靠判断时明确拒绝。
- 这是数据目录隔离，不是域名防火墙；不阻止用户打开其它网站。
- 同一任务包含并行UI交付：手工登记/编辑/解绑、孤立档案重新绑定、保留scope展示、非登录提示与错误显示；UI由另一代理实现及验证，后端验收不代表UI完成。

## 验收
- 新库/旧库/幂等/重开、serde null/default、唯一性、解绑scope、删除保留账号及事务失败原子性。
- 默认/覆盖/浅合并目录、Windows大小写/斜杠/extended/..和符号链接正规化，双向冲突、不影响无关Profile。
- Tauri命令注册与driver错误前置不启动；模拟运行测试不触发真实平台。
- cargo test --workspace --locked 与 cargo check --workspace --locked，保留in_progress交集成复审。
