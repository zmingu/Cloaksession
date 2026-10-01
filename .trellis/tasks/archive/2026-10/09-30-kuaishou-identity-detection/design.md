# Design

## 最小行为差与边界

前批仅TaskPage与手工业务登记。本批增加独立身份观察、后台检测与本地头像，不修改Profile人工资料及UI。源选择器证据为F:/jieger/electron/main/tasks/subAccount/index.ts:200-257，未实测真实平台。

## 数据流

应用startup -> 弱引用driver的周期任务 -> registry运行session快照 -> launcher读取Profile/业务状态/上次观察 -> TaskPage有限锁等待读静态JS -> Rust域/ID/多target校验 -> 可信URL代理下载/缓存 -> launcher回查业务状态+Profile存在，并在registry当前session校验内同步事务写入 -> IPC列表按当前世代投影。

- multizen-core：固定KuaishouIdentitySnapshot和状态枚举；profile-manager：独立平台档案/观察表与迁移，内部记录带session_id及成功avatar_url供复用。观测与手工表分离，业务状态变化拒绝写入。
- tauri-app：driver/identity子模块负责轮询/gate/超时，driver命令仅短DB操作不等待CDP或网络。registry保存每session UUID世代、Arc和取消信号；remove取消。连接期间remove不得重新插入旧连接；相同并发连接只留一个。
- CDP：仅TaskPage::evaluate静态extractor，不读取active page。准确URL先筛选并在同次返回值二次校验，所有匹配target全检，任一读取错误不伪装成功。
- 头像：启动时快照有效网络配置；明确静态proxy才代理下载，复杂覆盖/PAC/环境差异保守跳过。文件不在Profile目录而在独立kuaishou-avatars目录。缓存由成功身份+URL引用，失败有内存退避。
- 重启：每session新UUID。列表无匹配世代返回unknown/closed，保留历史ID/昵称/头像及lastSeenAt，不以旧detected宣称在线。

## 兼容与限制

不改既有Profile wire、manual accounts、旧表或启动页面行为。同步短registry锁可包住DB提交以与close原子互斥，不在锁内等待CDP/网络。cancel无法撤回已发CDP，但本批仅只读。读DOM不是服务端认证；头像magic并非完整图片解码。真实页面、原生Tauri端到端未验收。
