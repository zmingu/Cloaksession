# jieger 全功能迁移研究与分批草案

日期：2026-09-30。状态：研究草案，未批准实施，不替代现有账号初始化 PRD/design/implement，不表示全功能任务已开始。

## 已确认边界

- 只迁业务功能，不导入 jieger 旧账户、数据库、配置、话术、历史记录和登录态。
- 本地 F:/jieger 经授权 fetch --prune 与 pull --ff-only，main 与 origin/main 一致，提交 8a8d8a2。
- 磁力金牛必须使用独立 Profile，不能与小店共用 Cookie、context 或有效用户数据目录。业务关联不等于环境共享。
- 小店、快手直播、直播伴侣之间的 Cookie 共享和免登录延期，不作为迁移前提。各入口按自身登录流程工作。
- 账号初始化、主体资料档案、切片权限、CPS 加货架原需求保留；CPS 与跟播助手小黄车上车不同。
- 平台账户身份与浏览器环境 ID 分离。jieger Account.profileId 是平台用户 ID（electron/main/services/database/repositories/account.ts:20-23；tasks/mateLogin/index.ts:347-350），不可直接充当目标 Profile ID。

## 源码迁移映射

以下源路径相对 F:/jieger。实现在场不等于线上验证通过。

| 模块 | 主要源码证据 | 迁移注意 |
| --- | --- | --- |
| 账户、小号、分组、批量登录、互动历史 | tasks/subAccount/index.ts:909-1566；tasks/subAccount/batchLogin.ts:41-119（均在 electron/main 下） | 拆开业务身份与 Profile；停止、删除和清理语义重验 |
| 金牛大户/子户与达人授权 | electron/main/tasks/jinniu/index.ts:306-588；bindCreator.ts:39-210 | 独立 Profile，平台子户不是浏览器 Profile |
| 投放创编与提交 | electron/main/tasks/jinniuPromote/index.ts:146-370 | 金额、达人、素材确认与最终提交需显式分离 |
| 伴侣二维码登录 | electron/main/tasks/mateLogin/index.ts:265-421 | HTTP Token 认证，不等于浏览器 Cookie；不迁旧 Token |
| 取码、开关播、FFmpeg 推流 | electron/main/tasks/liveLaunch/index.ts:55-145,190-472；services/kuaishou/liveMate.ts:241-315 | 取码有真实开播副作用；本地停流与平台关播必须区分 |
| 小店中控、慧播 | electron/main/tasks/liveControl/index.ts:95-286；platforms/kuaishou/huiboActions.ts:19-134 | 慧播是平台已有视频选择，不含本地上传/剪辑能力 |
| 直播监控 | electron/main/tasks/liveRoomMonitor/index.ts:322-838 | 当前全局单监控，依赖场景；目标房间必须绑定 |
| 小号时间轴场景 | electron/main/tasks/scenePlay/index.ts:138-399 | 后台含评论/点赞/关注，UI 能力与后台有差异；防错房发送 |
| 主播互动 | electron/main/tasks/autoMessage/index.ts:43-215 | 时间轴、变量、启动置顶；置顶需实测 |
| 商品即时动作及时间轴 | electron/main/tasks/shopHelper/index.ts:42-146；tasks/shopProductScript/index.ts:116-284 | 跟播助手上车、下车、讲解，不是 CPS |
| 自动弹品 | electron/main/tasks/autoPopUp/index.ts:21-284；shortcutManager.ts:25-85 | 队列、间隔、重试、快捷键，部分仅后台可用 |
| 评论采集、自动回复 | electron/main/tasks/commentListener/index.ts:37-130；autoReply/index.ts:62-153；replyEngine.ts:29-123 | 当前采集真实输出主要是评论，不能宣称所有直播事件均可采集 |
| AI 服务与密钥 | electron/main/tasks/aiChat/index.ts:33-141；providers/openaiCompatible.ts:21-131 | 多 Provider；超时、取消、费用与公开发送限制需补齐 |
| 统计导出、诊断、设置 | electron/main/services/excel/exporter.ts:51-235；services/diagnostics/index.ts:38-222 | 敏感信息脱敏；不重复搬 Electron 更新和浏览器管理 |

## 不应当作成熟功能照搬

- src/pages/negative-whitelist/index.tsx:7-53：纯占位，无过滤执行链。
- electron/main/tasks/jinniuPromote/index.ts:321-344：随机文案占位；349-369：实际点击立即推广与确认。
- electron/main/tasks/liveLaunch/index.ts:307-311：getStreamCredentials 实际 startLiveMatePush 并启动黑屏保活。
- electron/main/platforms/kuaishou/goodsList.ts:104-118：置顶待实测。
- electron/main/tasks/autoPopUp/goodsKnowledge.ts:26-83：扫描和差异存在，applyCandidates 未接调用闭环。
- electron/main/tasks/subAccount/importExport.ts:60-62：同步接口直接成功；ipc/handlers/appHandler.ts:29-32：清全部登录态只记日志。
- electron/main/tasks/subAccount/index.ts:1220-1225：单账户清理未删除已保存状态。
- electron/main/services/updater/autoUpdater.ts:160-165：回滚未实现。
- electron/main/services/kuaishou/liveMate.ts:16：关闭 TLS 证书校验，不应迁入。
- electron/main/tasks/liveRoomMonitor/index.ts:813-838：停止监控不等于停止其已启动子任务。
- 原测试没有覆盖真实付费提交、开关播和平台 DOM E2E；资源目录不包含所需浏览器/FFmpeg 二进制。

## 目标底座与差距

以下路径相对 F:/Cloaksession。

- crates/multizen-core/src/profile.rs:124-142：环境模型可复用，无业务账户模型。
- crates/profile-manager/src/migrate.rs:4-44：现有迁移仅 profiles，无业务调度或账号表。
- crates/tauri-app/src/driver.rs:305-395：launcher 专用线程不是长任务执行器，不应塞入直播任务。
- crates/tauri-app/src/registry.rs:22-65 与 crates/cdp-driver/src/session.rs:129-183：会话与 active page 共享，需任务明确持有 target/page，避免互相切页。
- crates/tauri-app/resources/chromix/bridge.mjs:216-265：已有 Node/Playwright persistent context，但协议只启动/关闭，不是通用业务 worker。
- bridge.mjs:60-62 允许 userDataDir override：金牛隔离应检查规范化后的有效目录，不能仅检查 Profile UUID。
- crates/tauri-app/src/lib.rs:445-493：暂无业务账户/直播/投放命令。
- .trellis/spec/tauri-app/backend/ipc.md:36-45：已有事件字段兼容问题，新增业务事件必须用共享契约测试。
- .trellis/tasks/09-30-kuaishou-account-init/task.json:6：planning。其页面选择器与 OCR 尚未验证，不能计为已实现。

## 推荐技术路线（待确认）

保留 Tauri/Rust 作为宿主，管理 Profile、代理、指纹、业务存储及生命周期；使用独立 Node/Playwright 业务 worker 适配 jieger 的自动化代码。worker 连接宿主已启动的浏览器，不另建用户目录、不接管启动器、不移植 Electron 外壳。请求包含 taskId/accountId/profileId/targetId，回传结构化结果/进度；支持取消、超时、崩溃回收、按页面互斥。不要直接往现有 Chromix launch/close bridge 塞全部业务逻辑。

替代方案：全部改写 Rust/CDP。可减少新增业务 Node 层，但 Playwright locator/等待/页面语义重写量大；Chromix 当前仍依赖 Node，并不因此变为无 Node 应用。

两条路线均需统一身份映射、有效目录隔离、数据写入所有权、事件契约、页面并发和高风险操作保护。HTTP/AI/FFmpeg 是否继承环境代理需显式实现与验证，不能假定自动继承。

## 建议分批与可观察验收

1. 基础契约：业务账户与 Profile 绑定、金牛有效目录隔离、任务运行/取消/日志、目标页面归属。验收：两环境并发互不串号；取消后不再发新动作；崩溃显示失败而非成功。
2. 账户能力及原初始化任务：各入口独立登录、资料识别、档案和初始化；保留人工 OCR 核对及删环境不删档案要求。页面调研/OCR 没完成不得宣称自动化完工。
3. 跟播商品、CPS、场景与主播脚本：分开实现两类加货动作；测试幂等、目标房间、顺序、停止和失败恢复。
4. 直播伴侣/慧播/推流/监控：验证取码副作用提示、明确开播、停流/关播区分、进程清理，不用真实开播作为普通自动化测试。
5. 金牛授权、创编与投放：先预览账户/子户/达人/金额/素材，再显式提交；未确认时不产生投放；超时未知结果先核对，不能盲重试。
6. AI/评论/弹品/自动回复、统计导出、完整回归：AI 默认不自行公开发送；补取消/限流/脱敏；每个模块区分离线测试与人工平台验收。

顺序为草案，不是省略后续功能。纯占位功能列为缺口，是否补成新功能需独立范围确认，不以迁移名义自动扩大范围。

## 验证命令与实施门禁

- 根目录：cargo check --workspace --locked；cargo test --workspace --locked。
- crates/tauri-app/ui：npm.cmd run build（含 tsc）；npm.cmd test。
- crates/tauri-app/resources/chromix：npm.cmd test。
- 业务 worker 建立后再定义真实存在的测试脚本；不可沿用不存在的 UI typecheck script。
- 本轮仅研究与 Markdown 落档，未运行构建，也未做平台操作。
- 本文不是最终 PRD/design/implement；需确认技术路线及业务工作台入口、逐项整理要求，再完成最终规划复审后进入实施。
