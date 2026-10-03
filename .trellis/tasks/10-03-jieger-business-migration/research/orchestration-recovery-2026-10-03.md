# 编排恢复检查点 — 2026-10-03

## 当前结论（2026-10-04 更新：15/15 已合入集成分支，未提交）

15 个子任务已全部合入集成分支 `zmingu/jieger-integration`（自 `3fc4a8c` 切出），**不是已交付**。合并后验证全绿：`cargo check --workspace`、`profile-manager` 全过、`tauri-app` 222 passed、ui `tsc` 干净（`CARGO_TARGET_DIR=D:/cargo-target`）。详见 `research/integration-2026-10-04.md`。

关键裁决：`sub_account_interactions` 按 jieger 源头收敛为单表（scene-play schema 为准，sub-account-v2 自创 schema 已删除；`enter_live_room` 改返 `Result<()>`，与源头一致不记日志）。pi 两工作区的 30+ 文件 rustfmt 噪音已丢弃。`listen` 注解堆未动（tsc 干净）。

仍待后续：真号浏览器验收、UI 屏幕接线、`ensure_auth` 替换 huibo 门禁、Chromix 端到端验收。本次未提交，待用户确认后经 Trellis Phase 3.4 落盘。

---

以下为 2026-10-03 的独立回报阶段记录（已 superseded，仅保留作证据）：

15 个子任务全部有 Orca succeeded 的独立实现回报，**不是 15 个功能已交付**。主工作区 `F:/Cloaksession` 的 HEAD 仍为规划提交 `3fc4a8c`；各 worker 工作区实现留在各自未提交文件中（bind-creator-pi、huibo-live-pi 基线为含规划的 main，其余多为基线 `333d049`）。尚无合并后的统一构建、真实业务接线、完整 UI、Chromix 端到端验收。不要以单工作区测试通过替代这些验收。

其中 bind-creator（pi `ctx_2266592d63cd`）与 huibo-live（pi `ctx_6f4ddb18067d`）两单由主代理独立复验通过（见各任务 research/pi-retry-2026-10-03.md）；其余 13 单沿用既有 worker 回报口径，缺口见下表，尚未逐一重审。

本文件是本次实际核查记录，后续必须刷新。Orca Run 为 `run_62865bc71bf1`。

## 子任务与实现位置

工作区前缀：`F:/orca_worktrees/Cloaksession/`。下表 succeeded 仅表示该 dispatch 的 worker 回报；缺口源自既有回报，尚未对所有代码重新审计。

| 任务 | 工作区后缀 | Dispatch | 状态/明确缺口 |
|---|---|---|---|
| 平台原语 | ks-platform-primitives-v2 | ctx_ac3a9440f29d | succeeded；headless 恢复编排、真实 DOM 校准未验收 |
| 伴侣登录 | mate-login | ctx_f5f4aefd1122 | succeeded；前端接线未完成 |
| 开播 | live-launch | ctx_2f0b8c723cbf | succeeded；回报明确凭据仍可能为 placeholder，强身份校验未接通 |
| 评论监听 | comment-listener | ctx_f10eaa23a7cd | succeeded；需复核真实事件与调度脚本 schema 的错误混同 |
| 小号体系 | sub-account-v2 | ctx_e796ee221109 | succeeded；需复核实际发送证据、验证码停机、身份绑定与跨模块调用 |
| 商品脚本 | shop-product-script-v3 | ctx_d7485de59570 | succeeded；派发接口仍是 trait，需要真实上/下车和讲解实现 |
| 直播监控 | live-room-monitor | ctx_858760b62c58 | succeeded；回报明确 LiveRoomTrigger 为 Noop 占位，尚需注入真实下游 |
| 定时发言 | auto-message | ctx_ca4b84b1ab7c | succeeded；发送端 seam、UI 未接通；需复核与 jieger 原功能语义一致性 |
| 自动回复 | auto-reply | ctx_5ce3be66cbc6 | succeeded；监听器生产接线及 PM-backed ReplyRecorder 未接通；此前主代理一次验证因编译错误失败，不能算独立复验通过 |
| 场景剧本 | scene-play-v2 | ctx_601de4f8d716 | succeeded；与 sub-account 重复建 interactions 表的合并风险待核查 |
| 自动弹品 | auto-popup | ctx_425a1ea31871 | succeeded；global-shortcut 实际插件接线、知识来源实现待接通 |
| 跟播助手 | shop-helper | ctx_1c67fd98685b | succeeded；真实上车及 UI 未验收 |
| 金牛推广 | jinniu-promote | ctx_bf4f2036f312 | succeeded；原源码无 F8/F9 task，仅 selectors，不能宣称完整迁移 F8/F9；UI 未接通 |
| 达人授权 | bind-creator-pi | ctx_2266592d63cd | pi worker_done succeeded 2026-10-03T14:55:45Z；主代理独立复验通过（profile-manager 含 3 新测试/tauri-app 78 passed/workspace check clean，CARGO_TARGET_DIR=D:）；实现在 bind-creator-pi 工作区未合并；旧 opencode 两次尝试（ctx_a4878023f974、ctx_57d6baa10cdf）均已 abandon 隔离；终端已 release，delivery 已 ack |
| 慧播开播 | 未派发 | 无 | 排在 bind-creator 后，尚未实现 |

## 本次恢复已做

- 重新读取版本匹配的 Orca orchestration、messaging、recovery、coordinator-loop 指南。
- 核对 bind-creator 的 Task `task_f6105cbcd616`、Dispatch、终端 `term_907e771e-3495-4f43-b111-177d59b9ecb0` 与 worktree 映射。不要依据 worker-list 返回顺序推断 agent 名称或任务。
- 向原 Dispatch 发出路径纠正/阻塞询问消息 `msg_61ac261c893b`。发送成功只证明入队，未收到回复前不能说 worker 已读。
- 原终端 `tui-idle` 为 satisfied 后发送一次读取收件箱的继续提示；request `ef36b6da-f03d-40ae-827e-33f9b40adfcc` 只报告 input_accepted，provider 无法证明 turn_started。不得重复发送或据此认定正在实现。
- 主工作区 Trellis `10-03-bind-creator` 已 start，状态为 in_progress；branch 记录修正为 `zmingu/bind-creator`。这只恢复追踪状态，不构成验收。
- 清理历史已停止、Orca nextAction=release 的四个尝试的终端所有权（未删除任何 worktree）：ctx_459d25211c16、ctx_f0740eb03bff、ctx_e708316f3c0d、ctx_5f7d464861f9。随后 reclaimable 查询为空。user_owned 终端保留，不强关。

## 已确认的编排缺陷与修正规则

1. **子工作区基线错误**：worker-start 的默认 base 是 origin/main=333d049，不含 main=3fc4a8c 的规划。不能再用缺失的相对路径派活。后续新工作区应显式指定合适的 base，并验证 HEAD 和任务文档实际存在；当前 worker 可只读主工作区的绝对文档路径。
2. **依赖未合并**：另一个工作区 succeeded 不等于当前工作区包含依赖。不得把尚不存在的 adapter 替换为总成功 Noop 后宣称功能通过。整合需独立工作区、保留源工作区，逐项解决公共 driver/lib/migration 冲突并接通生产调用。
3. **文档也有事实错误**：TaskPage 操作要求独占 `&mut`；TaskCancel 丢弃 handle 不会取消；命令注册在 lib.rs；KuaishouMate/KuaishouSub kind 已存在；商品动作时间轴不是 GoodsKnowledge 数据源；点赞不是幂等写操作。不直接照搬错误草案。
4. **计数标准**：分别记录 worker_report、主代理完整退出码复验、跨层接线、合并构建、UI 测试、Chromix 实测。不要把上下文 token 百分比当任务完成度；不要截取测试套件前两行当全部测试通过。
5. **真实账号写操作**：不自动执行直播、推广、授权、发送、关注/点赞；遇验证流程停下交给用户。不实现规避平台检测的随机插空格/时间扰动功能。非幂等动作不在未知结果后自动补发。
6. **等待规则**：只保留一个 actionable waiter；无消息、空工作区、stale/missing_status 都不是退出证据。不得据此 stop/abandon/retry。不能把 ScheduleWakeup 用作未请求的无限轮询，也不能让旧提醒覆盖真实状态。
7. **限制**：默认只派 opencode，串行。用户 2026-10-03 明确把 bind-creator 剩余工作换 pi 做（opencode 额度限制），pi 该单成功。pi 可作为 opencode 限流时的 fallback，但必须确认任务文本真正进入 TUI（`input_accepted` 不算数；终端 `Working` + 工作区实质文件才算数，必要时用 `terminal send` 补发一次）。保留主工作区已有 `.gitignore` 和 `crates/tauri-app/Cargo.toml` 修改，不清理用户数据、不提交/推送未获授权内容。

## 慧播任务派发前需覆盖原草案的事实

直接查 `F:/jieger/electron/main/platforms/kuaishou/huiboActions.ts` 与 `F:/jieger/src/types/entities.ts:313`：
- 慧播是录播视频开播，不是模糊的跟播/回播监听器。列表 URL 为 `https://zs.kwaixiaodian.com/page/record-live/upload`。
- HuiboVideo 字段：id(replayId)、name、uploadTime、segments、size、duration(字符串)、status(processing/success/failed/live)、usageCount、expiryTime、goodsCount。原草案 thumbnail/title/i64 duration 无源码依据。
- 原实现列表获取会点击“去使用”取 replayId 并返回；它不是纯只读抓取。迁移设计应明确导航/点击副作用，优先读可证明的链接参数，禁止把列表抓取当隐含开播授权。
- 开播是选定 replayId 后 step=1 下一步、step=2 去开播；原代码只等 3 秒就记“成功”，不是成功证明。迁移必须有身份/页面/目标校验及结果读回，未知则返回未确认，不能重试写动作或伪造成功。
- bind-creator 未 settlement 前不并行派此任务；之后仍需合并接线验收，不是做完第 15 个独立模块即可结案。
