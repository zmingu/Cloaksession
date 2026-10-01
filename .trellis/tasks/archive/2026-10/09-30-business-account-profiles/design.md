# 设计

## 最小改动与归属
- multizen-core新增business模块，不改Profile JSON；camelCase账户/状态/输入，枚举kebab-case、null显式输出、id缺省。
- profile-manager新business_accounts模块共享原Connection，新增account与profile scope表；事务内校验唯一、绑定、kind、reservation，失败整体回滚。
- browser-launcher新增data_dir helper统一默认目录及有效Chromix目录解析，提供canonical nearest-existing-parent和组件重叠判断。有效目录校验位于launcher/driver，不倒置pm依赖。
- 浏览器handle保留启动时目录快照；验证既查当前配置也查存活handle快照，避免运行中修改DB隐藏使用路径。
- Tauri沿LauncherCmd/oneshot在原launcher线程执行CRUD和运行检查；save、update携带启动时engine/global Chromix设置。UI/MCP共享Launch分支在spawn/副作用前执行目录验证。options先构造候选验证后写入；持有scope运行时不许改变有效目录，相同options和普通metadata编辑允许。

## 固定IPC
business_accounts_list() -> BusinessAccount[]；business_accounts_profile_state(profileId) -> {account,scope}；business_accounts_save({input}) -> BusinessAccount；business_accounts_unbind({id}) -> void。
BusinessAccount={id,kind,displayName,platformUserId:null|string,profileId:null|string,createdAt,updatedAt}。
SaveBusinessAccountInput={id?:null|string,profileId,kind,displayName,platformUserId:null|string}。

## 限制
隔离保证针对应用配置与应用拥有的运行进程，不针对外部恶意重映射目录或外部浏览器；目录验证不是文件系统锁或域名访问控制。设置仍为启动时快照，保存全局设置在应用重启后生效。未知SDK字段保留，不推测其语义；已知会改变持久目录的嵌套/args选项明确拒绝。任何不可靠路径必须fail closed，测试应明确平台限制。不会复制/清理Cookie，不会改变业务登录态。
