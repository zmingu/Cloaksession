# Cloaksession 项目记忆交接文档

> 给其他 AI 的项目上下文。先读「接手摘要」与「历史冲突处理」，再按需查阅后面的完整记忆。

- 导出日期：2026-10-01。
- 项目目录：`F:\Cloaksession`。
- 来源：本项目持久记忆库的全部 **54 条 active 记忆**，不是整个聊天历史，也不是完整源码审计。
- 分类：事实 6 条、决策 25 条、偏好 2 条、坑点 8 条、任务状态 13 条。
- 最新记忆更新时间：`2026-10-01 03:57:28`。各条时间按记忆库原值保留，不作时区换算。
- 本文保留原始记忆内容及其 ID、更新时间、重要度；仅添加阅读导航、摘要和历史状态提示。重要度是原记忆库的检索/注入权重，不代表实施优先级。
- 文中的「已测试」「已提交」「已推送」均是历史会话记录，**本次导出没有重新运行测试、核验远端或验证真实业务页面**。
- 不含 API Key、Webhook 密钥、Cookie、登录态、真实身份证资料或证件图片；包含本机路径、私有仓库名称、插件 ID 等项目定位信息，请只分享给可信对象。

## 一、给接手 AI 的使用说明

1. 这是上下文，不是新的执行授权。不要因为旧记录中出现「下一步」就自动启动任务、修改配置、上传证件照片或操作真实账号。
2. 读取项目 `AGENTS.md`，开发前读取 `.trellis/workflow.md` 及目标包/层的 `.trellis/spec/`。任务资料位于 `.trellis/tasks/`，会话日志位于 `.trellis/workspace/`。
3. 先定位并读取真实代码，再修改；本地路径、版本号、提交号、测试数量、安装目录均可能过时。
4. 同一事项出现多条记录时，结合时间、后续明确决策和当前代码判断；不要把早期「待定」「未提交」当成最新状态。下面已列出主要冲突。
5. 不要把参考项目的 `Account.profileId`（平台用户 ID）当成 Cloaksession 的浏览器环境 ID。
6. 涉及真实账号、商品、开播、权限保存和资料采集时，重新确认范围及副作用。历史 Git 授权不等于本轮授权；执行 Git 操作前应再次征求用户确认。
7. 本机环境为 Windows / PowerShell。文中本机路径仅用于定位，换机器后需调整。

## 二、接手摘要（整合阅读层，不替代原始记录）

### 2.1 项目方向与架构边界

- 主要方向：多账号及独立浏览器环境管理、重复网页流程自动化、快手相关业务功能。
- 参考项目：`F:\jieger`，私有仓库 `zmingu/jieger`，Electron + React + Playwright 的快手直播中控工具。
- 迁移目标：把 jieger 的业务功能迁入 Cloaksession，**只迁功能，不迁旧账号、配置、话术、历史记录或登录态**。
- 用户后续批准的业务自动化路线是 **Rust + chromiumoxide**，不新增 Node 业务 worker；Chromix 启动桥暂留。早期 Node/Playwright worker 方案不是最终架构。
- 磁力金牛必须独立 Profile，不能和快手小店共用登录态；跨小店/直播/直播伴侣的 Cookie 共享或免登录已延期。
- 用户选择 Chromix。历史已验证版本为 `151.0.7922.173`；Windows 字体、扩展路径和解压兼容问题见坑点部分。

### 2.2 最新实现状态

- Rust/CDP 页面绑定、任务取消/等待/同页协调、业务账号绑定与金牛隔离、小店身份检测/头像等已有阶段性实现及测试记录。
- 最新集成记录称：账号初始化接线已合入并推送 `origin/main`，合并提交 `bf100f1`。
- **自动初始化监控 `start_kuaishou_init_monitor` 故意未启动**；目前通过 `kuaishou_init_retry` 手动触发，UI 按钮为「执行 / 补做初始化」。不要把「自动检测是产品目标」误读成「全部自动初始化已启用」。
- 暂未开启原因：切片保存语义尚未在真号上验证；证件图片 canvas 读取也需受控验证，跨域污染可能失败。
- Windows 本地 OCR 已有实现与真实 WinRT 测试；必须保持固定 OS 工作线程及长生命周期 MTA，不能每请求初始化/销毁 COM。
- 最新记录仍有四项提案未正式批准；UI Playwright 测试依赖本机 Chrome，新 IPC 尚未加入 `tests/tauriMock.ts`。
- 提交、测试数量和工作树清理情况都是记录时状态，接手时按实际文件重新核验。

### 2.3 已确认的业务规则

**账号初始化及资料**

- ID/头像登录后刷新；主体资料首次获取；切片全自动发布权限仅首次关闭，不在以后登录时反复检查关闭。
- 各个已打开环境独立检测、互不干扰；保留重检入口。
- 身份资料优先读「主体信息」Tab 明文；缺失再看「达人主体」；正常授权渠道拿不到完整文字时才用本地 OCR。
- OCR 不自动转云端；自动校验与人工确认并存。自动流程不中断等待人工，先完成可做部分，标「待核对」，事后核对。
- 待核对资料允许复制，但须标「未核对」。复制目标包括快手 ID、姓名、身份证号。
- 存档最小业务范围：姓名、身份证号、证件照片、快手 ID、初始化/核对状态；SQLite + 本地附件，照片保存实际文件而非网页链接。
- 删除浏览器环境仍保留档案及照片；备份同时带上数据库和附件。
- 用户在获知风险后选择明文存档，不用 Base64，也不加密。这是历史产品决策，不意味着数据已受保护或已合规。
- UI 采用混合式 C：保留 Profile 名称/emoji，卡片补充快手头像、ID、状态、重检；「快手详情」打开独立大面板，不是侧滑，不新增顶级导航页；搜索复用 Profiles 搜索框并覆盖孤立档案。

**不要混淆两个商品流程**

- CPS「加货架」：`cps.kwaixiaodian.com`，开播前准备，手动粘贴多个商品文本，按 `ID[:：]\s*(\d+)` 这类明确标记解析，列清单确认后执行；第一版仅当前账号，不属于登录初始化。jieger 没有可直接复用的 CPS 逻辑。
- 跟播助手「上车/小黄车」：`zs.kwaixiaodian.com/page/helper`，jieger 有 `shopHelperActions.ts` 可参考，但不等于 CPS 加货架。
- 跟播助手早期版本先考虑开播前通用设置，所有账号共用一套、开播前手动套用；定时上下车与直播互动推迟。具体选项待页面分析。

### 2.4 独立工具及开发辅助

- `tools/danmaku-pipeline/`：独立 Rust crate/workspace，录像转写输入 → 识别 → 弹幕时间线 JSON；已有规则/LLM/自动回退检测器及离线验证。上游只产 JSON，下游发送端在 jieger/另一会话消费，不能把发送端当成已在本仓库完成。
- 弹幕脚本契约：`CONTRACT.md`、`schema/danmaku-script.schema.json`、`examples/danmaku-script.example.json`。以 `send_at` 为发送触发权威，read 弹幕应早于主播念出时间。
- Snow 钉钉通知：常驻客户端脚本负责监听/推送，面板插件适合作配置/日志；面板关闭后插件自身监控停止。两种形态的存储不互通。
- Snow v0.4.12 索引历史可用组合：Voyage `voyage-4-large`、1024 维、`embeddingType=mistral`；Jina `jina-reranker-v3.5` 重排。版本相关，不应不经检查直接照搬。

## 三、历史冲突与接续注意事项

| 事项 | 早期记录 | 后续记录 / 接手时应如何理解 |
| --- | --- | --- |
| 全功能迁移架构 | M52 推荐独立 Node/Playwright worker，尚未选择 | M51 用户批准 Rust + chromiumoxide，业务不新增 Node worker |
| 初始化实施阶段 | M53/M54 仍需求访谈，未授权实施 | 后续任务已有实现；M43/M44 记录集成已合入，但不代表未批准提案自动获批 |
| local-ocr 暂停、未推送、草稿未集成 | M45 为当时暂停节点 | M43/M44 更新时间更晚，记录已集成和推送；不要自动恢复旧备份或覆盖当前代码 |
| CDP/业务账号/身份检测「未提交」 | M48–M51 的阶段性记录 | M44/M45 有后续分组提交记录；最新状态须重新核验 |
| 自动初始化 | M19 是自动检测产品目标 | M43/M44 明确整个初始化监控默认关闭，只能手动补做 |
| 主体存储/OCR/复制门槛 | M54 中部分选择待定 | M14–M16、M29–M31 等后续确认：本地 OCR、明文 SQLite + 附件、待核对可复制并标注 |
| UI 展示位置/重检入口 | M18/M19 尚待定 | M09 确认混合式 C 及重检入口 |
| CPS 单次多商品、人工确认、账号范围 | M06/M25/M26 含待定项，M24 尚待确认账号范围 | M24 确认清单确认流程，M23 确认仅当前账号 |
| 跟播助手通用设置时机 | M21/M22 中执行时机尚待定 | M20 明确手动触发；M21 明确所有账号共用一套，设置内容仍未定 |
| 头像读取及验收 | M38/M48 记录 canvas 降级和限制 | M47 记录真实落盘及缓存复用通过，但未视觉验收原生 UI；不能恢复已证实有额外请求的 `Page.getResourceContent` 方案 |
| 索引配置与结果 | M39 当时仍待 Jina key | M34 是后续可用配置和成功索引记录；本文不含任何 key |

> 原文中早期尚未确认的内容被保留，是为了可追溯，不是要求重新推翻已确认决策。若时间与当前代码不一致，以核查结果和用户最新指示为准。

## 四、完整记忆：事实（6 条）

### M01 · jieger项目参考价值与文件定位清单

- 记忆 ID：`0363562568204386304`；类型：`fact`；重要度：5；更新：`2026-09-30 13:47:50`。

jieger参考价值清单（F:\jieger，私有仓zmingu/jieger，--depth 1，Electron+React+Playwright快手直播中控）。下次做快手相关二开前先看这份清单再定位文件：
1. 登录检测/连接原语：electron/main/platforms/kuaishou/connection.ts — kuaishouConnect(kuaishouLogin、ensureKuaishouAuth，login.kwaixiaodian.com→zs./s.kwaixiaodian.com URL判定+loggedInSelector。
2. 平台URL/选择器配置：electron/main/utils/platformConfig.ts — KUAISHOU_CONFIG(loginUrl/storeLoginUrl/storeHomeUrl/verify pattern/loggedInSelector/inLiveControlSelector)、KUAISHOU_JINNIU_CONFIG。
3. 快手ID+头像抓取：electron/main/platforms/kuaishou/liveControl.ts（normalizeKuaishouProfileId、extractKuaishouProfileIdFromUrl、右上角hover tooltip提取ID）、electron/main/tasks/subAccount/index.ts（.seller-main-avatar img头像、s.kwaixiaodian.com/zone/home抓取）。
4. 中控台选择器骨架（live.kuaishou.com/shop/live，占位需实测）：electron/main/platforms/kuaishou/selectors.ts。
5. 跟播助手商品上车/下车（zs.kwaixiaodian.com/page/helper，可复用于“自动上车”第二批以后）：electron/main/platforms/kuaishou/shopHelperActions.ts + shopHelperSelectors.ts（Tab切换、搜索框input[placeholder='输入商品ID/商品名称搜索']、更新小黄车、readId正则匹配"ID:"）。
6. 账号表结构/迁移：electron/main/services/database/migrations/index.ts（accounts含profile_id/profile_name/avatar_url/storage_state_path/usage/ua/viewport；jinniu_accounts；scenes等）、repositories/account.ts。
7. 扫码登录态接口：electron/main/tasks/mateLogin/index.ts（scanResult/acceptResult，user_id/headurl）。
8. 加密：electron/main/utils/secureCrypto.ts + tests/electron/secureCrypto.test.ts。
9. jieger没有的（Cloaksession二开新增）：主体资料/达人主体/身份证OCR、切片全自动发布权限关闭、cps.kwaixiaodian.com货架。

### M02 · 钉钉通知脚本 v1.1 修复要点（误报/重试/去重）

- 记忆 ID：`0363678698347200512`；类型：`fact`；重要度：3；更新：`2026-09-30 21:29:18`。

钉钉通知脚本已升级到 v1.1.0（scriptId da3a8fb7-e60d-4088-8c3a-adb8ad25e5a2，源码 F:\Cloaksession\snow-plugins\dingtalk-notify.user.js）。用户已填好 Webhook + 加签密钥，测试通知返回 ok:true（端到端已验证，crypto.subtle HMAC-SHA256 在客户端脚本隔离世界可用）。
v1.1 修掉的误报问题（v1.0 实测暴露）：
1) 会话消失后立即读 messages 会读到上一轮 → 加 END_GRACE_MS=4000 抖动等待 + READ_DELAY_MS=3000 延迟读库。
2) 同一轮因 key 漂移（sessionKey ↔ conversationId ↔ directoryId#title）被重复上报 → notifiedConv[conversationId]={runId,at} 按 runId 精确去重（不能用时间窗口，会误吞真正连续的两轮）。
3) 未落库会话（conversationId 为空）发虚假「完成」→ 改为只记跳过日志。
4) 未配置 Webhook 时日志被刷屏 → emit 直接 return 不写日志；pushLog 对连续同类结果合并计数。
5) 发送失败无重试 → sendWithRetry 对网络类错误退避 1s/2s 重试 2 次，钉钉 errcode 410100/130101 视为可重试，其余不重试。
6) 新增 minDurationSec 选项（UI 有输入框），短于该时长的运行只记日志不推送。
注意：重新 config-set 安装会从源码重解析元数据，会覆盖用户在脚本插件列表里手改的 @name，源码必须与期望名字保持一致。

### M03 · 钉钉通知插件 com.snow.dingtalk-notifier 实现要点

- 记忆 ID：`0363672165907464192`；类型：`fact`；重要度：3；更新：`2026-09-30 21:03:21`。

已开发并安装「钉钉通知助手」插件：源码 F:\Cloaksession\snow-plugins\com.snow.dingtalk-notifier，安装目录 C:\Users\Administrator\.snowapp\plugins\com.snow.dingtalk-notifier（pluginId com.snow.dingtalk-notifier，esm，privacy 声明 messages）。
关键实现要点（下次改插件先看这里）：
1) 插件是 esm 渲染进程模块，只有面板挂载时才运行——监控引擎放在 Panel 的 useEffect 里（依赖 ready 而非 config 对象，避免配置变更重启订阅），面板关闭即停止监听。
2) 触发信号：api.metadata.subscribe("runtime") 实时推送；streamingSessions/conversation.isStreaming 从「运行中」变为消失即判定本轮结束（挂载时已存在的会话不补发，用 primed 标志）；attentionRequiredConversationIds 用于「需要你处理」（待确认/待回答/待批准）。
3) 完成 vs 出错：运行结束后用 api.metadata.get("messages", {params:{conversationId}}) 读最后一条 assistant 消息的 status / interruptionReason（该域需在 privacy 声明 messages）。
4) 推送用 api.net.fetch POST 钉钉自定义机器人 markdown；加签用 crypto.subtle HMAC-SHA256 + btoa，无 Web Crypto 时回退提示 SIGN_UNAVAILABLE。
5) 配置与事件历史存 api.storage.setJson（app_plugin_values 表）。
6) 改完源码后必须重新 config-set scope=plugins key=<pluginId> value={sourceDir:"..."} 原地更新，否则安装目录不会变。

### M04 · 参考项目jieger已克隆并定位关键逻辑

- 记忆 ID：`0363561766375096320`；类型：`fact`；重要度：3；更新：`2026-09-30 13:44:39`。

已克隆参考项目 jieger（私有仓 zmingu/jieger，F:\jieger，--depth 1）。它是 Electron+React+Playwright 的“介个助手”快手直播中控自动化工具，含账号登录、DB迁移、加密。关键发现：①account表已有profile_id/profile_name/avatar_url列（migrations v8附近），liveControl.ts/subAccount/index.ts已实现从s.kwaixiaodian.com/zone/home抓取快手ID+头像（.seller-main-avatar img、右上角hover tooltip提取ID）；②shopHelperActions.ts已实现跟播助手zs.kwaixiaodian.com/page/helper的商品上车(addToCart)/下车(Tab切换“小黄车商品/待上车商品”、搜索框input[placeholder='输入商品ID/商品名称搜索']、“更新小黄车”按钮)，readId用正则匹配“ID:”标记——与我们确认的解析规则一致；③mateLogin.ts有扫码登录态(user_id/headurl)。注意：未发现“资质主体信息/达人主体/身份证OCR”“切片全自动发布权限”“cps.kwaixiaodian.com货架”相关逻辑，这三块是Cloaksession二开新增。当前为只读参考，未授权改Cloaksession代码。

### M05 · Profile 头像现状为 emoji 字段

- 记忆 ID：`0363557800362868736`；类型：`fact`；重要度：2；更新：`2026-09-30 13:28:54`。

代码事实：UI 侧 Profile 已有 icon 字段（types.ts），ProfileTile.tsx/ProfileRow.tsx 用 Avatar 渲染 emoji 头像，profileEmoji(profile.icon, name, tags, id) 决定默认表情。当前 icon 存的是 emoji 文本，不是图片。若要在卡片上显示快手真实头像，需要扩展展示方式（如图片数据URI或新字段），属于待定实现细节。

### M06 · 快分销商品文本样例与ID标记格式

- 记忆 ID：`0363554980360323072`；类型：`fact`；重要度：2；更新：`2026-09-30 13:17:41`。

快分销加货架文本样例（用户实际复制）：
"宬福 60%
钟薬师粉葛粉
ID: 27018938082376"
格式观察：前面是商品名称等描述行，最后一行以“ID:”+数字标记商品ID（样例14位）。建议解析规则：优先匹配 ID[:：]\s*(\d+) 这类带标记的行，不按“长数字”猜测。仍待确认：一次粘贴是否可能含多个商品、是否总有ID标记、解析后是否需人工确认、加货架成功判定与重复加购处理。

## 五、完整记忆：决策（25 条）

### M07 · 弹幕脚本 JSON 契约 v1.0 位置与要点

- 记忆 ID：`0363680787643269120`；类型：`decision`；重要度：4；更新：`2026-09-30 21:37:36`。

直播录像→弹幕脚本→快手小号真发。上下游解耦契约已定于 tools/danmaku-pipeline/：CONTRACT.md(说明)、schema/danmaku-script.schema.json(JSON Schema draft2020，已 ajv 校验通过)、examples/danmaku-script.example.json。

关键约定：时间轴以视频起点=0秒(float毫秒)；事件 type=read(主播真念过,已还原原文,必填 spoken_at)/filler(氛围弹幕,比例可调)；read 核心公式 send_at≈spoken_at-lead_time_sec(默认3~8s,弹幕先发主播后念);发送端以 send_at 为权威触发时间。事件字段:id/type/send_at/text(必填)+spoken_at/lead_time_sec/confidence/source_text/account_hint(auto或稳定虚拟观众ID,不含真实账号)/persona/note。

职责边界:上游(本仓库tools/,Rust,不用Python)只产JSON;下游发送端(jieger/另一会话)负责账号映射/拟人化/限速抖动/风控。管线分3段:①ffmpeg抽音频+ASR(词级时间戳)②LLM判定念/回应弹幕+还原原文③生成时间线。已确认:录像基本单主播不做分人物分离;平台快手。

> 导出说明：原记录「0秒(float毫秒)」的单位表述不够清晰，接手实现时读取实际 CONTRACT/schema，不能仅凭这句换算时间单位。

### M08 · 通知类需求应选常驻客户端脚本而非 Hook（含 Hook 上下文实测结论）

- 记忆 ID：`0363676156900966400`；类型：`decision`；重要度：3；更新：`2026-09-30 21:19:12`。

结论：插件/Hook/客户端脚本三种形态的取舍（已实测验证）。
1) Hook 实测事件上下文（node 探针写 stdin 日志得出）：beforeToolCall → {"args":{...},"cwd","toolName"}；onStop → {"conversationId","cwd","reason":"completed"}。即 Hook 上下文只有 conversationId/cwd，无会话标题/耗时/Token；且 Hook 全部 9 个类型都在生命周期链路上，**拿不到「等待确认/等待回答/等待批准 Plan」这类循环中间挂起态**。
2) 项目级 Hook 规则数组非空即取代全局，不会合并 → 想全项目生效必须逐项目配置，否则静默失效。
3) 面板插件（esm）只在面板挂载时运行，关面板即停；只有客户端 UI 脚本 @snow-scope global 是真正常驻（应用启动即执行、随视图切换不卸载）。
4) 常驻脚本可用的能力与插件同源：snow.metadata.subscribe("runtime") 实时推送、snow.metadata.get("messages") 读 status/interruptionReason、沙箱档全局 fetch 已替换为跨域实现、@snow-privacy 声明敏感域。
5) 结论：钉钉通知 = 常驻客户端脚本（监听+推送）+ 面板插件（配置中心/日志）；两者存储不互通（app_plugin_values vs userscript_values），配置各存一份。
6) 实测踩坑：应用启动时首个 runtime 快照里已有的 attentionRequiredConversationIds 会把历史遗留会话误报为「需要处理」；修法是只对「本轮出现过 streaming 活动（recentActive，5 分钟内）」的 conversationId 发通知，并用标题二次比对过滤复用 id 的新会话。脚本 startWatch 时须把 sessions/attentionSeen/recentActive 归零、primed=false，首帧只作基线。
7) 客户端脚本 @match 可写 snow://client/*；仅 @snow-target client 时 matches 解析为空数组。

### M09 · 界面方案采用混合式C

- 记忆 ID：`0363565942358376448`；类型：`decision`；重要度：3；更新：`2026-09-30 14:01:15`。

界面方案确认：采用方向C（混合式）——Profile卡片上轻量展示快手头像（小图）、快手ID、初始化状态标记（完成/未完成/待核对）、「重新检测」按钮；点击「快手详情」打开独立大面板（非侧滑），内放主体资料、证件照片、OCR人工确认、一键复制。档案搜索复用Profiles页搜索框，能搜到已删环境的档案。不新增顶级导航页（区别于方向B），也不全塞进ProfileEditSheet（区别于方向A）。

### M10 · CPS加货架与跟播助手上车是两件事需区分

- 记忆 ID：`0363562252209717248`；类型：`decision`；重要度：3；更新：`2026-09-30 13:46:35`。

重要澄清：用户明确区分两件不同的事。①“加货架”=cps.kwaixiaodian.com快分销选品中心把商品加入货架（第一版手动触发、粘贴ID文本、列清单确认、仅当前账号）——这是当前第一批之后的第二批工作，jieger没有对应逻辑，需全新实现。②“自动上车/加进小黄车”=zs.kwaixiaodian.com/page/helper跟播助手页的小黄车上车，jieger的shopHelperActions.ts已实现，属于后面要做的功能，不在当前范围。之前整理的需求表里“快分销加货架”指的就是CPS页，不要与跟播助手上车混淆。

### M11 · 跨业务Cookie共享免登录延期

- 记忆 ID：`0363573422077476864`；类型：`decision`；重要度：2；更新：`2026-09-30 14:30:58`。

全功能迁移中，用户明确暂不处理快手小店、快手直播、直播伴侣之间的 Cookie 共享或免登录，后续需要再验证实现。不得将共享登录态作为迁移前提；磁力金牛仍须独立 Profile。

### M12 · 磁力金牛必须独立Profile隔离登录态

- 记忆 ID：`0363572785705091072`；类型：`decision`；重要度：2；更新：`2026-09-30 14:28:26`。

用户明确纠正：磁力金牛账号不能与快手小店等账号共用一个 Profile，否则登录态会相互挤出。迁移必须将磁力金牛放在独立 Profile，与快手小店环境隔离；业务关联不等于共享 Cookie/浏览器会话。其他账号类型是否可以共用环境需逐类确认，不能仅按不同站点推断可共存。

### M13 · jieger全功能迁移只迁功能不迁旧数据

- 记忆 ID：`0363572594440634368`；类型：`decision`；重要度：2；更新：`2026-09-30 14:27:41`。

用户要求将 jieger 全部业务功能迁入 Cloaksession，明确只迁功能，不导入旧账号、配置、话术、历史记录或登录态。2026-09-30 已经用户授权 fetch --prune 与 pull --ff-only，F:\jieger main 与 origin/main 一致，短提交 8a8d8a2。注意 jieger Account.profileId 是平台用户ID，不是 Cloaksession 浏览器环境ID。

### M14 · 待核对资料允许复制但标注未核对

- 记忆 ID：`0363559098294435840`；类型：`decision`；重要度：2；更新：`2026-09-30 13:34:03`。

主体资料复制规则确认：即使处于“待核对”状态也允许一键复制，但需标注“未核对”，不强制先核对再复制。

### M15 · OCR确认不中断自动初始化流程

- 记忆 ID：`0363558831222128640`；类型：`decision`；重要度：2；更新：`2026-09-30 13:32:59`。

OCR人工确认与自动初始化的冲突处理确认：不中断自动流程，先把能做的做完，账号标为“待核对”；用户事后在账号详情中对照证件照片确认，确认后才标“已核对”。不采用当场暂停等待确认。

### M16 · 主体资料存档采用最小范围

- 记忆 ID：`0363558592427819008`；类型：`decision`；重要度：2；更新：`2026-09-30 13:32:03`。

主体资料存档范围确认：按最小范围——只存姓名、身份证号、证件照片，加快手ID与初始化/核对状态；资质页其他主体信息（企业名称、统一社会信用代码、有效期等）暂不落库，以后需要再加。

### M17 · 初始化失败自动重试后提示并保留未完成状态

- 记忆 ID：`0363558409879126016`；类型：`decision`；重要度：2；更新：`2026-09-30 13:31:19`。

账号初始化失败处理确认：某一步失败先自动重试，仍失败再提示用户，并保留“未完成”状态，下次登录/检测时补做。提示的具体展示形式（卡片状态标记或弹窗）待定。

### M18 · 账号信息采用补充式展示

- 记忆 ID：`0363558102306619392`；类型：`decision`；重要度：2；更新：`2026-09-30 13:30:06`。

账号信息展示方式确认：采用补充式——保留现有 Profile 名称与 emoji 头像，另外新增显示快手头像与快手 ID，未登录快手的环境不受影响；不做替换。具体展示位置（卡片角落/详情页）待定。

### M19 · 账号初始化采用自动检测触发

- 记忆 ID：`0363557386125017088`；类型：`decision`；重要度：2；更新：`2026-09-30 13:28:45`。

账号初始化触发方式确认：自动检测、每个已打开环境各自检测并各自初始化，互不干扰（非仅当前环境）。仍需确认登录判定标准（建议以能读出快手ID为准，读不到则不动作）、是否保留手动“重新检测/初始化”兜底入口、ID与头像在软件中的展示位置。

### M20 · 跟播助手开播前设置手动触发

- 记忆 ID：`0363556965750898688`；类型：`decision`；重要度：2；更新：`2026-09-30 13:25:35`。

跟播助手开播前通用设置确认按“手动触发”（开播前手动套用），具体设置项与推荐值待分析 zs.kwaixiaodian.com/page/helper 页面后给出；用户明确此项不着急，先把前面的模块做起来。定时上下车、直播间互动仍推迟。用户已表达“把前面的先做”的推进意愿，但尚未确认第一批范围、未同意创建Trellis任务、未进入规划。

### M21 · 跟播助手开播前设置所有账号共用一套

- 记忆 ID：`0363556723970244608`；类型：`decision`；重要度：2；更新：`2026-09-30 13:24:37`。

跟播助手开播前通用设置：所有账号共用一套配置（暂不做按账号覆盖）。具体设置项与推荐值待分析 zs.kwaixiaodian.com/page/helper 页面后给出。执行时机（首次登录套用一次、每次登录检查，或开播前手动触发）尚未确认。

### M22 · 跟播助手第一版只做开播前通用设置

- 记忆 ID：`0363556476112044032`；类型：`decision`；重要度：2；更新：`2026-09-30 13:23:38`。

跟播助手第一版范围确认：只做“开播前的通用设置”，用户视其为账号初始化的一部分；具体设置项待分析 zs.kwaixiaodian.com/page/helper 页面后由我给出推荐建议。开播后的定时上下车与直播间互动明确推迟到后续版本，不进第一版。仍待确认：这些通用设置各账号是否一致（决定是否需按账号分别存储）、执行时机（首次一次还是每次开播前套用）。

### M23 · 快分销加货架第一版仅当前账号

- 记忆 ID：`0363555474231230464`；类型：`decision`；重要度：2；更新：`2026-09-30 13:19:39`。

快分销加货架第一版范围确认：仅作用于当前打开的账号，不做多账号批量。账号选择/切换与批量执行留待后续版本。仍待实测：按商品ID搜索并加货架的页面操作、加货架成功判定、已在货架商品如何处理。

### M24 · 快分销加货架执行流程与确认步骤

- 记忆 ID：`0363555306291298304`；类型：`decision`；重要度：2；更新：`2026-09-30 13:18:59`。

快分销加货架流程确认：手动触发→用户粘贴含多个商品的文本→按“ID:”标记解析出全部商品ID→列出清单等用户确认→确认后再执行加货架。仍待确认：一次执行针对一个还是多个账号、已在货架商品的处理、失败/部分失败处理与汇报方式。另需实测：快分销页面如何按ID搜索并加货架、加货架成功的页面判定。

### M25 · 快分销商品文本支持一次多个商品ID

- 记忆 ID：`0363555081786982400`；类型：`decision`；重要度：2；更新：`2026-09-30 13:18:06`。

快分销加货架解析规则确认：一次粘贴文本可能包含多个商品，因此按“多条”解析——凡出现“ID:”标记后的数字各识别为一个商品ID，全部列出。解析后是否需人工确认再执行、重复商品处理、成功/失败判定仍待确认。

### M26 · 快分销加货架由粘贴商品文本手动触发

- 记忆 ID：`0363554433041399808`；类型：`decision`；重要度：2；更新：`2026-09-30 13:15:31`。

快分销加货架：用户确认手动触发；执行时用户会复制一段商品信息文本，软件需从中找出商品ID，再据此加货架。文本样例格式、是否一次多个商品、ID是否有固定标记、解析后是否需要人工确认、加货架成功/失败的判定与重复加购处理均待确认。加错商品会影响实际经营，建议解析后先展示待加ID列表供确认。

### M27 · 快分销加货架属于开播前准备非初始化

- 记忆 ID：`0363554042090323968`；类型：`decision`；重要度：2；更新：`2026-09-30 13:13:58`。

用户确认快分销加货架商品属于开播前准备，需要时手动触发，不属于账号登录初始化。开播前准备与跟播助手（直播设置、互动、定时上下车）的关系、触发方式、是否批量多账号仍待讨论。

### M28 · 切片权限仅首次登录设置一次

- 记忆 ID：`0363553819741880320`；类型：`decision`；重要度：2；更新：`2026-09-30 13:13:05`。

用户决定切片权限（/zone/short-video-b/slice 关闭所有全自动发布权限）只在首次登录设置一次，后续登录不自动检查或重复关闭。已提示风险：平台或人工可能再次开启，届时不会被自动纠正，需手动处理。

### M29 · 主体资料存档采用明文不加密

- 记忆 ID：`0363553578632314880`；类型：`decision`；重要度：2；更新：`2026-09-30 13:12:07`。

用户决定主体资料存档不做加密、也不做Base64，直接明文存储（已告知Base64非加密、项目已有aes-gcm+scrypt可复用，用户仍选择明文）。风险提示：存档文件被拷贝或他人操作本机可直接读取身份证号与证件照片。身份证号精确搜索无需额外索引。当前仍是需求讨论，未授权实施。

### M30 · 主体资料存档方案：本地SQLite+附件目录

- 记忆 ID：`0363553090478243840`；类型：`decision`；重要度：2；更新：`2026-09-30 13:10:11`。

用户已接受主体资料存档方案：本地SQLite保存快手ID、姓名、身份证号、资料来源、核对状态等字段便于搜索；证件照片与需长期保留的头像存本地附件目录，数据库只记对应关系；照片存文件本身而非网页链接；删除浏览器环境不删除档案；备份需数据库与附件一起。加密方式与保留期限尚未定。当前仍是需求讨论，未授权实施。

### M31 · OCR选型以证件照片隐私为优先

- 记忆 ID：`0363547947439456256`；类型：`decision`；重要度：2；更新：`2026-09-30 13:05:03`。

用户最终确认第一版使用本地OCR，不再选用微软云端OCR或自托管OCR。OCR仅在“主体信息”无明文、“达人主体”拿不到完整文字时作为备用，用于识别身份证照片；仍保留自动校验+人工确认两重保护，识别失败或不确定时人工对照修正，不自动转云端。本地处理不等同于已加密/已合规，存档保护、模型选型与识别效果实测待定。当前仍是需求讨论，未授权实施、采购或上传证件照片。

## 六、完整记忆：偏好（2 条）

### M32 · 二开初步方向：多账号环境管理与网页流程自动化

- 记忆 ID：`0363527075387375616`；类型：`preference`；重要度：2；更新：`2026-09-30 11:26:48`。

二开需求访谈中，用户选择关注：管理多个账号及各自独立的浏览器环境；自动完成某个重复网页操作流程。具体网站、操作流程、使用规模与第一版边界尚未确定。用户希望逐步挖掘需求，目前不是实施授权。

### M33 · 用户选择 Chromix 作为使用引擎

- 记忆 ID：`0363438132423786496`；类型：`preference`；重要度：2；更新：`2026-09-30 06:13:07`。

用户选择 Chromix。2026-09-30 已将本机 com.cloaksession.browser/settings.json 的 browserEngine 设为 chromix，Node 使用 D:\Applications\Scoop\apps\nodejs-lts\current\node.exe，设置已备份。stable 151.0.7922.173 已安装SDK缓存，My first profile 已真实启动并验证 CDP；伴随扩展路径已修复，Companion ENABLED且0清单/运行时错误。

## 七、完整记忆：坑点（8 条）

### M34 · 代码库索引完整配置与 5 个踩坑（Voyage 嵌入 + Jina 重排）

- 记忆 ID：`0363661711231451136`；类型：`pitfall`；重要度：3；更新：`2026-09-30 20:21:48`。

Snow App v0.4.12 代码库索引最终可用配置（项目 local:F:\Cloaksession）：
- 嵌入：voyage-4-large @ https://api.voyageai.com/v1/embeddings，embeddingType=mistral，1024 维
- 重排：jina-reranker-v3.5 @ https://api.jina.ai/v1/rerank（app 固定发 top_n、解析 results 数组，只有 Jina 兼容；Voyage 用 top_k 且返回 data，参数和响应都不兼容）
- 分批：batchMaxLines=2、batchConcurrency=1；分块 modelContextLength=32000、200 行/块、重叠 20
踩坑顺序：① embeddingType 决定参数名（jina/openai 发 dimensions，仅 mistral 发 output_dimension）→ Voyage 须设 mistral；② Voyage 未绑卡限速 3 RPM（绑卡后 Tier1=2000RPM，免费 200M token 仍有效，约几分钟生效）；③ 单批 token 上限 12 万，batchMaxLines 过大（10×25600 tokens）会 TOO_MANY_TOKENS_IN_BATCH → 降到 2；④ 分块器 token 上限 = modelContextLength×0.8，用 o200k 估算，换 Jina 时因分词器差异会误判超限（Jina v3 上限 8194）。
自动化方法：原生模块 D:/Program Files/Snow App/resources/app.asar.unpacked/native/snow_native.win32-x64-msvc.node 可用 node 直接 require（ABI 匹配），导出 startCodebaseEmbedding(projectId, sessionId, cb) / clearCodebaseIndex / getCodebaseIndexStats / previewCodebaseScan 等，可脱离 UI 驱动索引；向量表名 cb_vec_<projectId的sha256前16位>，存在 ~/.snowapp/snowapp.db。
最终结果：388 向量 / 235 文件 / 全部 voyage-4-large / 维度 1024，零错误。

### M35 · Windows OCR必须保留MTA生命周期

- 记忆 ID：`0363762487203233792`；类型：`pitfall`；重要度：2；更新：`2026-10-01 03:02:15`。

独立crate local-ocr Windows真实OCR测试发现：每请求RoInitialize/RoUninitialize后再次创建zh-Hans-CN引擎会STATUS_ACCESS_VIOLATION。改用进程固定2个OS线程+thread-local MTA长生命周期，真实合成图重复/并发OCR通过。保持超时后工作许可直到原生请求终态；勿改回每请求COM teardown。接口/证据见任务09-30-kuaishou-account-init/results-ocr.md。

### M36 · Chromix中文方框修复与小店扫码专用入口

- 记忆 ID：`0363717529951830016`；类型：`pitfall`；重要度：2；更新：`2026-10-01 00:03:36`。

修复Chromix151 Windows中文方框：SDK默认及fontsDir仍失败，带中文别名字体白名单失效；自有windows-fonts.mjs生成已装字体ASCII族名白名单，保留显式策略，不改vendor。真实隔离字体测试修前Times New Roman修后Microsoft YaHei。新增卡片“快手小店扫码”显式launch entry新开登录页，保留通用启动/startUrl，金牛scope拒绝。check与测试过，用户dev原生复测待做。

### M37 · Chromix Windows 中文方框与非ASCII字体白名单

- 记忆 ID：`0363716863132991488`；类型：`pitfall`；重要度：2；更新：`2026-10-01 00:00:57`。

Windows Chromix 151.0.7922.173 本地CDP字体fixture复现：默认及fontsDir整个Windows字体（含中文族名别名）中文选Times New Roman；同批字体仅ASCII族名白名单选Microsoft YaHei。自有bridge windows-fonts.mjs复用vendor解析器注入ASCII白名单到各args层，显式策略/白名单不覆盖，vendor未改。test/windows-fonts-smoke.mjs是隔离真实渲染检查，不改用户DB。

### M38 · Page.getResourceContent并非头像纯缓存读取

- 记忆 ID：`0363687092428767232`；类型：`pitfall`；重要度：2；更新：`2026-09-30 22:02:39`。

2026-09-30并行C专用Chrome153隔离fixture发现：Page.getResourceContent读已加载跨域PNG成功，却额外请求主文档（Fetch3→4、Network3→3）；MISS -32000无新增不能证明命中安全。身份头像不得当cache-only使用；降级现有img canvas，CORS污染跳过，仅明确代理fallback。见身份任务results-avatar-cache与identity_browser测试。

### M39 · 代码库索引：embedding 用 Voyage 须设 embeddingType=mistral，rerank 必须用 Jina

- 记忆 ID：`0363648528496230400`；类型：`pitfall`；重要度：2；更新：`2026-09-30 19:29:25`。

Snow App 代码库索引（v0.4.12）配置陷阱：
1) embeddingType 决定请求体参数名：jina/openai 分支发 "dimensions"，只有 mistral 分支发 "output_dimension"。Voyage (api.voyageai.com) 只认 output_dimension，用 dimensions 会 400 "Argument 'dimensions' is not supported"。→ 指向 Voyage 时把 embeddingType 设为 mistral（UI 标签显示 Mistral，但走的是 OpenAI 兼容 + output_dimension 分支，端点/baseUrl 原样使用）。
2) rerank 不能用 Voyage：app 固定发 top_n（Voyage 只认 top_k），且 app 解析写死读 results 数组（Voyage 返回 data 数组）。Jina (api.jina.ai/v1/rerank) 两者都兼容。rerank 失败会静默回退原始顺序，不报错，容易误以为生效。
3) 索引器 load_codebase_settings 用 camelCase 反序列化 system_settings.codebase_settings 扁平字段，不读 configJson（configJson 残留旧值无害）。
4) 改配置后必须重建索引；本次排查时 codebase_embed_sessions 0 行、无向量表 = 一个文件都没嵌成功。
当前配置：embedding=voyage-4-large/1024维/mistral 类型；rerank=jina-reranker-v3.5@api.jina.ai/v1/rerank，rerankingApiKey 待用户提供 Jina key。

### M40 · Chromix Windows stable 包的反斜杠解压兼容问题

- 记忆 ID：`0363448251622588416`；类型：`pitfall`；重要度：2；更新：`2026-09-30 06:13:35`。

Windows stable v151.0.7922.173 的 chromix-win-x64.zip 使用反斜杠条目，固定SDK yauzl strictFileNames 拒绝。已从SDK指定GitHub release下载并按SHA256SUMS验证，对条目做路径/类型/重名校验后用.NET解压至 C:\Users\Administrator\.cache\chromix\v151.0.7922.173\win-x64。SDK binaryInfo installed=true，真实启动成功；未改vendor解压逻辑，未来其他版本仍需验证。

### M41 · Chromix 托管扩展路径须避开 SDK 文件 URL 转换

- 记忆 ID：`0363447752231976960`；类型：`pitfall`；重要度：2；更新：`2026-09-30 06:11:36`。

固定 Chromix SDK 的 resolveAbs 用 file URL.pathname，导致应用托管 Windows 扩展 C:\... 变 /C:/...。已在 resources/chromix/bridge.mjs 将默认托管扩展转为原始 --load-extension/--disable-extensions-except flags，覆盖 top/launch/context args；保留用户显式SDK选项，上游vendor不改。managed-extensions.test.mjs回归；40项Node测试、cargo check、UI build通过，真机Companion ENABLED且0错误。

## 八、完整记忆：任务状态（13 条）

### M42 · 弹幕管线 danmaku-pipeline 骨架实现状态

- 记忆 ID：`0363697333266579456`；类型：`task_state`；重要度：4；更新：`2026-09-30 23:40:46`。

 tools/danmaku-pipeline/ 已落地 Rust 独立 crate（自成 workspace 空表，与主项目解耦；不用 Python）。已合入 main（骨架 74368b7，LLM 识别器 23e97e1）。

已实现(离线可跑)：转写(whisper verbose_json/SRT 导入)→识别→组装(send_at=spoken_at-lead，过滤置信度，补 filler，排序，重排id evt-000N，轮换 account_hint viewer-N)→符合契约 JSON。
识别器三选一 --detector rule|llm|auto：
- rule(detect/rule.rs)：提示语规则版，离线兜底。
- llm(detect/llm.rs)：语义识别隐式回应("对这个纯棉的"→反推"是纯棉的吗")+还原口语+置信度+persona。纯函数 build_messages/parse_llm_json(容忍markdown围栏)/map_to_detected(用seg索引定位spoken_at)。HTTP 走 Cargo feature llm-http(reqwest可选,默认关)；离线用 --llm-response <file> canned 回放验证。env: DANMAKU_LLM_BASE_URL/API_KEY/MODEL。
- auto(FallbackDetector)：LLM为主,空则回退规则。
坑：独立 crate 加 reqwest 后 cargo 独立解析 lock 会选到 yank 的 chacha20(经quinn链)→用 Copy-Item ../../Cargo.lock ./ 复用父项目已验证版本解决,已剪枝无父crate残留。默认构建+llm-http构建+canned回放三路径均验证,ajv校验valid。

预留未接入：asr/whisper_cpp.rs(本地转写,feature门控占位)、asr/extract_audio(ffmpeg已写需系统装)。ASR-RUNBOOK.md 给了 faster-whisper/openai-whisper/whisperX 产 verbose_json 命令。下一步:真实录像跑LLM调prompt。发送端(④)在jieger/另一会话按schema消费。

### M43 · 账号初始化集成已合入main（自动监控默认关闭）

- 记忆 ID：`0363776382768807936`；类型：`task_state`；重要度：2；更新：`2026-10-01 03:57:28`。

2026-10-01 已接线并推送 origin/main（bf100f1）：account_init 草稿（LauncherCmd::Init、InitRuntime、9个 kuaishou_subject_*/kuaishou_init_* IPC、local-ocr 依赖、SubjectPanel/InitSummary UI）。启动时调用 kuaishou_init_recover_interrupted。自动初始化监控 start_kuaishou_init_monitor 故意未启动，只能通过 kuaishou_init_retry 手动触发（UI 按钮“执行 / 补做初始化”），因为切片保存语义尚未在真号上验证。待办：真号受控验证切片保存语义和证件图片 canvas 读取（跨域污染可能失败），验证后再开启自动监控；四项提案尚未得到正式批准；UI Playwright 测试需要本机 Chrome 或执行 npx playwright install chrome；新增 IPC 尚未加入 tests/tauriMock.ts。

### M44 · 工作树清理与main未提交堆积整合进度

- 记忆 ID：`0363769773137100800`；类型：`task_state`；重要度：2；更新：`2026-10-01 03:57:28`。

2026-10-01 整合完成：OCR会话按5组提交(6c41f16/eba619c/798b090/f3f7840/b239539)；账号初始化草稿在工作树 feat/kuaishou-init-integration 接线(8efc950)+.gitignore忽略.snow/.scratch/snow-plugins(5478775)，合并 bf100f1 已推 origin/main(zmingu/Cloaksession)。git status 干净，只剩 main 一个工作树/分支。自动初始化监控 start_kuaishou_init_monitor 故意未启动，只能手动 kuaishou_init_retry 触发（切片保存语义未真号验证）。main上原草稿已移入 .scratch/init-integration-paused-20261001/moved-from-main-before-merge/。

### M45 · local-ocr可编译节点已分五组提交main并暂停

- 记忆 ID：`0363770741752889344`；类型：`task_state`；重要度：2；更新：`2026-10-01 03:35:03`。

用户要求在local-ocr节点暂停。main已5次精确文件提交：6c41f16 CDP、eba619c业务金牛、798b090身份头像/字体扫码、f3f7840初始化数据层、b239539 local-ocr。check/UI build通过，OCR13普通+1真实WinRT通过。未推送。后续Tauri/UI集成草稿未完成且未提交；15文件备份.scratch/init-integration-paused-20261001/，恢复说明RESTORE-NOTES.md；移除11行接线后恢复编译，8草稿原地保留，暂停勿自动续做。

### M46 · 账号初始化剩余功能最终规划待批准

- 记忆 ID：`0363753327745073152`；类型：`task_state`；重要度：2；更新：`2026-10-01 02:25:51`。

账号初始化剩余任务本轮重写prd/design/implement及implementation-ready.md、JSONL，仍planning待最终批准。真实切片抽屉4个全自动发布主checkbox均开，智能生产独立switch不动；保存语义未实测。资质主体自然人文本不遮罩，达人遮罩，须限定active Tab。待批准：Windows系统OCR缺资源不自动下载；自建任务页成功关闭失败留；校验失败不强制确认；未知企业结构失败不猜。

### M47 · 跨域头像浏览器GET修复已通过隔离测试

- 记忆 ID：`0363745543666302976`；类型：`task_state`；重要度：2；更新：`2026-10-01 01:56:19`。

2026-10-01用户重启dev复测后主代理只读验收：主页/zone/home；观测detected，avatarKey非空，message“头像复用已验证的本地缓存”，本地JPEG12695字节magic ffd8ff、SHA256与文件名一致。头像真实落盘及缓存复用已通过；主代理未视觉复核原生UI。下一步建议原账号初始化剩余：先资质/切片页面只读调研，再主体档案/OCR核对/首次权限设置。

### M48 · Rust迁移第四批小店身份识别与头像已实现

- 记忆 ID：`0363691085204848640`；类型：`task_state`；重要度：2；更新：`2026-09-30 22:18:31`。

第四批小店身份检测已实现：Rust5秒/4并发只读s.kwaixiaodian.com顶栏，独立观察档案、冲突不覆盖人工登记；卡片/列表+详情+重检。主代理Rust250/0/3、UI74/74、build/check过、隔离Chrome153本地fixture1通过。getResourceContent实测额外请求，生产改已加载img canvas；无CORS且无明确代理仍可能无头像。生命周期/Modal抢焦点已修。最终 .trellis/tasks/09-30-kuaishou-identity-detection/review.md，真实平台未验收/未提交。

### M49 · Rust迁移第三批账号绑定与金牛隔离已实现

- 记忆 ID：`0363653927710523392`；类型：`task_state`；重要度：2；更新：`2026-09-30 19:50:52`。

第三批业务账号Profile已实现，任务 .trellis/tasks/09-30-business-account-profiles/review.md。入口Profile编辑General业务账号登记；显式保存/解绑/孤立档案重绑，不代表登录。独立业务表和持久scope，金牛正反有效目录冲突校验共用UI/MCP启动门禁，运行账号修改需关环境。主代理Rust202/0/2、cargo check与UI build过、UI mock34/34。未真机/原生端到端、未提交；下一批小店登录检测和身份读取。

### M50 · Rust CDP迁移第二批任务控制已实现

- 记忆 ID：`0363636802959949824`；类型：`task_state`；重要度：2；更新：`2026-09-30 18:42:49`。

Rust/CDP第二批 .trellis/tasks/09-30-rust-cdp-task-control 已实现TaskPage协作租约、session-local Weak锁、TaskCancel(watch)、typed超时取消和selector四态等待。全workspace主代理复跑177通过/0失败/2忽略，cargo check通过。互斥仅新接口同session/target；取消不撤销已发CDP。未真机/业务UI/账号接入、未提交，最终review.md；后续账号Profile绑定+金牛有效目录隔离。

### M51 · Rust CDP迁移第一批页面绑定已实现

- 记忆 ID：`0363595642891894784`；类型：`task_state`；重要度：2；更新：`2026-09-30 15:59:16`。

用户批准业务浏览器自动化统一Rust+chromiumoxide，不新增Node业务worker，Chromix启动桥暂留。首批任务 .trellis/tasks/09-30-rust-cdp-page-binding 实现BoundPage和共享page_ops，不依赖active_page；保持旧MCP。主代理全工作区测试158通过/0失败/2忽略，cargo check通过；修Windows测试fileURL和分隔符。未真机验收、未提交，后续等待/取消/同页协调、账号绑定与金牛目录隔离。以review.md为最终记录。

### M52 · jieger全功能迁移研究进度

- 记忆 ID：`0363578444886540288`；类型：`task_state`；重要度：2；更新：`2026-09-30 14:50:56`。

全功能迁移已完成双代理只读盘点，研究落在 .trellis/tasks/09-30-kuaishou-account-init/research/jieger-full-migration.md；原任务仍 planning，未改产品代码。发现取码实际开播并黑屏推流、金牛随机文案+真实提交、白名单纯占位。推荐 Tauri/Rust 宿主+独立Node/Playwright业务worker，尚待用户选择，非既定架构；全功能最终PRD/design/implement尚未完成。

### M53 · 二开第一批范围与规划状态

- 记忆 ID：`0363557141316075520`；类型：`task_state`；重要度：2；更新：`2026-09-30 13:26:17`。

用户同意第一批范围=账号初始化三件事（ID/头像、主体资料、切片权限），但明确暂不创建Trellis任务，继续把细节聊完。快分销加货架与跟播助手设置列为第二批。未授权写代码、未进入规划阶段。当前项目内仅有一个无关任务 09-30-chinese-localization(in_progress)，不可当作本次二开授权。

### M54 · 快手小店二开需求清单与待定触发方式

- 记忆 ID：`0363538091840077824`；类型：`task_state`；重要度：2；更新：`2026-09-30 12:40:57`。

快手小店二开需求访谈，非执行授权。登录后ID头像每次刷新；主体资料仅首次获取；关闭所有切片全自动发布权限仅首次设置。资料为一份档案：优先主体信息Tab明文姓名/身份证号；缺失则转达人主体，若正常授权渠道拿不到完整文字则OCR身份证照片。用户已确认OCR结果须自动校验+人工确认两重保护；具体校验失败处理、复制门槛和本地/云端OCR尚待定。需详情查看资料/照片、一键复制“快手ID+姓名+身份证号”、本地存档搜索；删除浏览器环境仍保留档案及照片。SQLite+本地附件为建议未拍板，加密/失败补做待定。商品加货架及跟播设置/互动/定时上下车另谈，不在登录初始化内。

## 九、交给其他 AI 时可附上的一句话

> 请先阅读这份 Cloaksession 项目记忆，重点关注接手摘要、历史冲突、最新账号初始化集成状态及未批准事项。把它当作历史上下文而非执行授权；结合当前代码核实后，再按我本轮提出的任务继续。
