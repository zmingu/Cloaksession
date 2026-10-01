# 第四批后端结果：快手小店只读身份

2026-09-30。任务保持 in_progress，交主代理复审/原生联调；未commit/archive，未执行Git。

## 固定API

| 命令 | JS参数 | resolved值 |
| --- | --- | --- |
| kuaishou_identity_list | 无 | KuaishouIdentitySnapshot[] |
| kuaishou_identity_detect | {profileId} | KuaishouIdentitySnapshot |
| kuaishou_identity_avatar | {avatarKey} | 本地data:image/png/jpeg/webp/gif;base64字符串或null |

Snapshot精确字段：profileId、status、platformUserId、nickname、avatarKey、checkedAt、lastSeenAt、message。状态七值不变，nullable显式null。Tauri State异步命令内部Result包装不改变JS resolve合同；list存储失败可reject，detect失败返回error快照，头像失败返回null。

## 实现与主要文件

- `crates/multizen-core/src/kuaishou_identity.rs` + lib.rs：独立合同、5..32位ASCII全串ID和scope判断。
- `crates/profile-manager/src/kuaishou_identity.rs` + lib.rs/migrate.rs；`tests/kuaishou_identity.rs`：独立平台ID档案/按Profile观察表、业务状态回查、事务写、删除保留档案头像引用。
- `crates/tauri-app/src/driver/identity/{mod.rs,extract.rs,extract.js,avatar.rs,tests.rs}`：固定TaskPage读、域/解码/多target冲突；5秒后台周期、最多4并发、手动自动同gate、有限总等待；本地hash头像缓存、代理安全和退避。
- `crates/tauri-app/src/registry.rs`：UUID世代、OnceCell并发连接共享、remove取消、防迟到attach重新插入、原子当前slot回查。
- `crates/tauri-app/src/driver.rs`：启动网络快照、launcher准备slot、Identity命令仅DB工作、关闭/删除/退出失效。原business_tests.rs仅把两个现有fixture helper开放给同层新测试，未复制fake launcher。
- `crates/tauri-app/src/commands/kuaishou_identity.rs` + commands/mod.rs、lib.rs：固定IPC注册、应用setup挂一次监控、Exit同步取消。
- `crates/cdp-driver/src/task_page.rs`：将既有TaskCancel::cancelled公开供外围读任务取消；其余控制语义不变。
- `crates/cdp-driver/tests/kuaishou_identity.rs`：按路径导入生产reader并复用已有tests/common peer；不新增依赖、不复制数百行fixture。
- 新backend specs：`tauri-app/backend/kuaishou-identity.md`、`profile-manager/backend/kuaishou-identity.md`；相关index、模型合同、registry/TaskCancel规范同步。

共仅fmt本批18个Rust文件（skip_children=true），另1个必要静态JS；未改UI、Chromix桥、Node业务worker、Cargo依赖/lock。

## 行为要点

- 不导航、不激活、不填表、不读Cookie/token，不使用MCP active。准确HTTPS443小店hostname先筛选，同次JS返回URL再校验。
- 小店页面无IDnot-detected；多个IDconflict；手工ID不一致conflict；金牛scope/非shop绑定skipped。未启动手动closed且不启动浏览器。
- 保存时回查Profile与完整BusinessProfileState，核对当前Arc slot/UUID、取消/期限及响应存活。重启或重开旧detected变unknown/closed，历史字段不是登录证明。
- 头像仅可信yximgs.com及子域，拒userinfo/重定向/非HTTPS443、2MiB、超时、magic。DB引用+严格key+canonical parent+symlink/reparse/no-follow+内容hash校验后才返回本地dataURI。
- 支持明确Profile HTTP/SOCKS5或简单顶层SDK proxy；网络使用启动快照。环境proxy/no_proxy/Node差异、嵌套复杂SDK/PAC/旁路、无法确认的系统代理均警告跳过头像，绝不悄悄直连。ID仍成功；同ID+成功URL复用，变ID不沿用旧图，世代/URL变化重试，20..320秒失败退避。

## 实际验证

| 检查 | 结果 |
| --- | --- |
| cargo test --workspace --locked | **232通过、0失败、2忽略**，含1项compile-fail doctest；基线202/0/2 |
| cargo check --workspace --locked | 通过，无新增编译警告 |
| 新CDP reader测试单独执行 | 6通过（3项实际wire场景 + 复用reader的3项纯测试） |
| Tauri lib最终局部执行 | 28通过 |
| symlink测试 --nocapture复跑 | 1通过；本机成功创建符号链接并实际执行拒绝分支，无权限跳过提示 |

完整日志：本目录 `rust-test.log`、`rust-check.log`。新测试覆盖DTO/null、域/ID解码、同ID/不同ID、无页面/无ID/JS错误、取消/有限锁等待、逐target固定路由无导航/激活、旧DB与重开/删除档案、manual mismatch/解绑失效、关闭重开迟到保存/超期丢弃、弱引用监控与有界gate/调度、代理优先/复杂覆盖拒绝、头像大小magic/hash/符号链接。

## 明确局限/未验收

- 未连接真实快手页面、用户浏览器或CDN；未原生Tauri端到端验收。共享CDP peer只验证真实handler协议，不执行DOM；UI代理负责独立本地DOM fixture，其结果不冒充平台验收。
- 无明确静态代理的环境，本批不会猜测系统/PAC为直连，因此可能仅显示ID并提示头像未缓存。动态浏览器代理变更不跟踪；嵌套SDK代理保守跳过。
- magic不是完整图像解码。取消/崩溃可能留下未引用的同目录临时文件；不会返回给UI，未加清理调度器。
- legacy launcher的外部退出识别仍保守，CDP失败变error而非当前登录；本批不是通用进程监控。
- 未做主体资料、OCR、切片权限、自动登录、平台签名或其他迁移业务。

任务创建/启动先审查task.py与hooks，进程内mock common.git.run_git且禁止subprocess.Popen；脚本默认base_branch非真实证据，已置null。总TODO由主代理管理，本代理未add/update。
