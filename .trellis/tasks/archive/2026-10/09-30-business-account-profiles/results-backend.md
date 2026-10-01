# 第三批后端结果与复核

日期：2026-09-30。任务保持 `in_progress`，交主代理集成；本文不代表 UI 验收完成。

## 本轮接续说明

接到本子任务时，磁盘已存在本批任务（prd/design/implement/context、in_progress）及主体后端代码/spec，但尚无结果文件，实施清单也未收尾。未覆盖或重复创建 task.json。本轮重新阅读 AGENTS、四层 backend 规范、shared guides、原初始化 PRD、前批 review、task.py/task_store/task_utils/git/config.yaml；以进程内 mock `common.git.run_git` + 拒绝 `subprocess.Popen` 运行 packages、validate、start 及 context 更新。没有 Git 子进程，也未修改 Trellis 脚本。

## 固定 API（字段及枚举与 UI 合同一致）

```ts
type BusinessAccountKind = 'kuaishou-shop' | 'kuaishou-live' | 'kuaishou-mate' | 'kuaishou-sub' | 'jinniu';
type BusinessAccount = {
  id: string; kind: BusinessAccountKind; displayName: string;
  platformUserId: string | null; profileId: string | null;
  createdAt: string; updatedAt: string;
};
type SaveBusinessAccountInput = {
  id?: string | null; profileId: string; kind: BusinessAccountKind;
  displayName: string; platformUserId: string | null;
};
type BusinessProfileState = {
  account: BusinessAccount | null;
  scope: 'jinniu' | 'kuaishou' | null;
};
```

| Invoke 名称 | JS 参数 | 返回 |
| --- | --- | --- |
| business_accounts_list | 无 | BusinessAccount[]，含孤立档案 |
| business_accounts_profile_state | `{ profileId }` | BusinessProfileState |
| business_accounts_save | `{ input }` | BusinessAccount |
| business_accounts_unbind | `{ id }` | void |

只在省略 id 或 id:null 时新建；显式字符串 id 必须精确命中已有记录，空串/空白/未知 id 不能隐式新建。别名 trim 后 1..100 Unicode 字符，平台 ID trim 后空白归 null、最多128字符，不允许控制字符。业务 kind 不可改，(kind,非空平台ID) 唯一，无自动合并。Profile 被占用或账号绑定别处时拒绝覆盖/抢占。

## 已完成后端

- 独立业务账号及 profile scope 两表；复用原 ProfileManager Connection 与专用 launcher 线程，不给 Profile 塞业务字段。
- 事务内登记/编辑/重绑定/预留 scope。错误回滚；unbind 只断开关联、保留账号和 scope，不读写 Cookie。删 Profile FK SET NULL 保留档案，scope CASCADE。
- UI/embedded MCP 的同一 LauncherCmd::Launch 分支在 mark_opened、代理、Node spawn、CDP attach 前验证实际目录。全局 Chromix options 与 profile options 依既有浅合并规则；顶层 userDataDir 有效覆盖默认值；CFT/CloakBrowser 默认目录也共享 helper。
- 金牛双向目录重叠校验（相同、祖先/后代均拒绝），涵盖未绑定普通 Profile 和 live handle 原始目录快照。unbind 后 scope 保留不能跨域绑定，仍参与目录校验。
- 保存/编辑/解绑看 launcher 的 liveness 而非 UI cached-running；运行中的 scope Profile 不可改有效目录/删除。运行中未绑定 Profile 即使更新配置，其原始占用目录快照仍被比较。
- 路径规范化覆盖 Windows 斜杠、大小写 ordinal 比较、本地盘符 extended 前缀、普通 ..、既有 junction/symlink、最近存在父目录加缺失后缀。无法可靠确认的受保护路径显式错误，不移动/覆盖/删除用户目录。
- 本轮修正显式空 id 隐式新建偏差、无相关金牛 scope 时普通（含 kuaishou scope）路径兼容；补 null/空白编辑清空、重绑定 UPDATE 回滚测试；IPC 字符串错误保留原因并提供中文操作提示。

## 文件范围

Rust 主体及模块注册：

- `crates/multizen-core/src/{business.rs,lib.rs}`
- `crates/profile-manager/src/{business_accounts.rs,lib.rs,manager.rs,migrate.rs}`
- `crates/profile-manager/tests/business_accounts.rs`
- `crates/browser-launcher/src/{business_guard.rs,data_dir.rs,driver.rs,chromix.rs,lib.rs}`
- `crates/browser-launcher/tests/{business_guard.rs,data_dir.rs}`
- `crates/tauri-app/src/commands/{business_accounts.rs,mod.rs}`
- `crates/tauri-app/src/driver.rs`、`src/driver/{business.rs,business_tests.rs}`、`src/lib.rs`
- `crates/tauri-app/build.rs`（既有本批 MSVC 测试 harness Common Controls manifest 修复）
- `crates/tauri-app/Cargo.toml` 本批使用现有 tempfile 3 作 dev dependency；本轮接续未更改依赖或 lockfile。

已核对 manager.delete 完整边界仍为先删行再 best-effort 清 data_dir，未改清理语义。本轮 rustfmt 仅列出的21个 Rust 文件，`skip_children=true`，没有格式其他 Rust/UI 文件。

规范：multizen-core contracts；profile-manager business-accounts + index；browser-launcher business-isolation + index；tauri-app runtime/ipc。本轮同步显式 id、普通 Profile 兼容和中文错误说明；任务 context 增加两个新业务规范。

## 离线验证

最终（未带测试名称过滤器）：

- `cargo test --workspace --locked`：退出0，**202通过 / 0失败 / 2忽略**，含原1项 compile-fail doctest。相对前批177通过新增25项：core1、存储8、目录5、guard5、Tauri6。
- `cargo check --workspace --locked`：退出0，无警告。
- `rustfmt --edition 2021 --config skip_children=true`：21个本批 Rust 文件，成功。
- Trellis context validate：两个清单各10条，路径均有效（更新后再次验证）。
- 日志：`backend-test.log`、`backend-check.log`（本任务目录）。

测试包含临时SQLite、trigger失败注入、路径/junction与原 Node fake-SDK/本批 protocol-only fake bridge。后者仅用于离线协议测试，不是新增业务 worker；未启动真实浏览器、访问平台或读取真实 Cookie/Token。两个真实浏览器测试仍 ignored。UI npm/组件测试由并行代理交付，本子代理没有运行也不认证。

## 限制与主代理复核事项

1. 手工登记不是登录成功；不自动识别账号、不复制Cookie、不导入旧业务数据，无平台权限/开播/投放操作。
2. scope 是应用登记环境的约束，不是网址防火墙。无法阻止用户手动访问站点、外部浏览器、恶意文件系统在 check/use 间重映射或直接改数据库；不维护停止状态下曾经使用过的所有历史目录。
3. 全局 engine/Chromix 配置仍采用 driver 启动快照；重启后生效。底层 launch/launch_with_chromix 是进程原语，新应用入口必须走共享 gate。
4. 不支持/拒绝 UNC、设备/盘符相对/根相对、ADS、尾随点空格/DOS名等不可靠路径；reparse 路径结合 .. 明确拒绝。Windows junction 实测；未在 macOS/其他文件系统验证大小写行为。
5. Chromix 用 supervisor 实际存活状态；旧 CFT/Cloak handle 保守视为 running 直到显式 close（可能拒绝已自行退出的旧进程，但不误放行），不是 UI 缓存。
6. 未执行 Git、commit、archive、真实浏览器、UI 操作；任务仍 in_progress，后端通过不能替代主代理/UI集成复审。
