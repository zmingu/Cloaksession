# 第三批主代理复审与验证

2026-09-30。业务账号/Profile 关联与金牛目录隔离。本批代码、接口及离线测试已复审，未提交/归档；任务保持 in_progress 以待原生应用和实际浏览器联调。

## 当前入口与行为

Profile 编辑 → General/常规 → 业务账号登记。组件在常规区，无新增顶级导航，不混入 Profile 自动保存。可登记账号类型、别名、可选平台ID，创建并绑定、编辑当前记录、选择未绑定档案重绑，明确确认后解绑。手动登记不是登录证明，未增加假登录状态或平台请求。

业务类型：快手小店、快手直播、快手直播伴侣、快手子账号、磁力金牛。业务账号本地ID、平台用户ID、浏览器Profile ID分开；每个Profile当前最多一份绑定，每份记录最多一个Profile。删除环境保留登记档案；解绑保留账号及环境scope，不清Cookie。金牛scope与快手scope不能跨用，不自动合并账号或复用登录态。

后端在共享launcher线程登记/更新/启动路径做目录校验，UI和MCP启动共用；金牛目录与其他配置及运行目录相同/包含重叠时拒绝。识别Windows常见路径别名与已有junction/symlink，无法可靠解析时明确报错，不覆盖用户配置。

## 复审证据

核对 Rust BusinessAccount/SaveBusinessAccountInput/BusinessProfileState 与前端 businessAccounts.ts 字段、枚举和4个invoke参数；lib.rs handler注册、driver.rs共享启动门禁、business_guard.rs运行快照、业务表事务与外键、UI独立保存和异步防串Profile状态。

UI已完成独立保存、错误/重试、重复提交、切换Profile迟到结果、解绑确认、scope保留与孤立档案重绑测试。后台错误String/Error可显示；保存成功但刷新失败不会伪造空状态或鼓励重复新建。

## 主代理独立执行

| 检查 | 结果 |
| --- | --- |
| cargo test --workspace --locked | 202通过、0失败、2忽略，含1项compile-fail doctest |
| cargo check --workspace --locked | 通过 |
| UI npm.cmd run build | 通过（tsc+vite）；仅大于500kB chunk警告 |
| UI npm.cmd test，PLAYWRIGHT_CHANNEL=chromium、CI=1 | 34/34通过：账号22项、既有12项，桌面/窄屏两配置 |

主代理Rust完整日志：main-rust-test.log。UI使用已安装本地Chromium和mock Tauri，无新增浏览器下载；测试环境变量完成后恢复。测试不是真实平台验收，也未进行原生Tauri前后端端到端联调。

## 本批未做与剩余风险

- 自动登录识别、快手头像、主体资料/OCR、切片权限、CPS/跟播商品、直播/金牛投放均未在本批实现；全功能迁移继续。
- 目录约束不是网址防火墙，不阻止用户手动进入其他站点、外部进程、直接数据库修改或检查后恶意目录重映射。
- 全局引擎/Chromix配置采用启动快照；未跟踪停止环境用过的全部历史目录。UNC等无法可靠处理的路径在隔离校验中拒绝。
- 运行中业务账号修改/解绑需先关闭Profile。旧CFT/Cloak对外部退出仍保守视为运行，需显式关闭。
- 无Cookie共享/迁移，无旧数据导入，无真实开播或付费动作，无Git操作。

下一批优先接入快手小店登录检测和账号身份读取，复用现有TaskPage，区分人工登记与平台检测结果；真实选择器须在可授权测试环境验证。
