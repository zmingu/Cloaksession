# 快手小店只读身份检测与本地头像缓存

## Goal

实现原账号初始化R1/R4身份部分：每个已运行环境自动读取准确小店页的账号ID、昵称与头像，保留独立本地身份档案。不代表后台认证、初始化完成或OCR核对。

## Requirements

- 只读准确 https://s.kwaixiaodian.com hostname，不导航/激活/填表，不访问Cookie/token，不扫描整页ID。静态JS同次返回URL及账号区数据，Rust二次校验；ID为5..32位ASCII数字全串。
- 不同页面ID冲突，手工登记平台ID不一致冲突；保留人工账号、别名、绑定。金牛scope/非小店业务绑定跳过；无绑定已运行环境也检测；未运行手动检测closed且不启动。
- 固定Snapshot：profileId,status(unknown/detected/not-detected/conflict/closed/skipped/error),platformUserId,nickname,avatarKey,checkedAt,lastSeenAt,message，后六项可null。历史字段不是当前登录证明。
- 固定IPC：kuaishou_identity_list()、kuaishou_identity_detect(profileId)、kuaishou_identity_avatar(avatarKey)。头像仅本地data:image/png|jpeg|webp|gif;base64或null，不接任意路径、不热链。
- Rust应用挂一次后台轮询，有限并发、不重入、手动自动共用gate；所有等待有界，shutdown取消，关闭/重开/删除/手工解绑后迟到结果不能污染。重启旧detected降级。
- 平台档案按ID唯一、观察按Profile；删Profile只删观察。头像可信yximgs.com及子域HTTPS443无userinfo、无重定向、2MiB/timeout/magic、内容hash名和同目录临时原子写；读取需DB引用、安全key、尺寸/符号链接限制。
- 下载继承启动时有效代理，无法确认SDK/PAC/environment则警告跳过头像但保留ID；成功URL复用，换ID不沿用旧图；失败有限退避。

## Acceptance Criteria

- 离线Rust覆盖域/解码/无ID/冲突/手工mismatch/生命周期/迁移/缓存/头像安全/调度取消与不重入。
- cargo test --workspace --locked 与 cargo check --workspace --locked通过，精确记录计数；不运行ignored或真实平台。
- 不修改ui、Chromix桥、Node业务worker；不执行任何Git含脚本隐式Git。任务保持in_progress交主代理复审。

## Authorization

用户持续批准全功能Rust迁移并明确本批范围及继续。基线Rust202通过/0失败/2忽略，UI34；本批不包含主体、OCR、切片或自动登录。
