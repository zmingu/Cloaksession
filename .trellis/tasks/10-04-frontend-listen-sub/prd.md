# 前端B组-监听与小号

## Goal

为 `comment_listener`（7）、`sub_account`（8）共 15 个命令补齐 TS 封装、类型、页面、导航、i18n。命令签名见父任务 `research/command-surface.md §5–6`。

## Requirements

- R1 `lib/commentListener.ts`：`start/stop/status/statusAll/recent/history/next` 全封装；`CommentEvent`（`sendAt: f64` 秒精度保留）、`ListenerStatus`、`LiveEventRow` 类型。
- R2 监听页：启停 + 状态 + 实时事件流（`comment_events_next` 长轮询或定时 `recent`，二选一并在 design 注明）+ 历史查询。
- R3 `lib/subAccounts.ts`：8 命令全封装；`SaveBusinessAccountInput` / `BusinessAccount` / `SubAccountLoginResult` / `SubAccountInteraction` 类型；与现有 `businessAccounts.ts` 复用 `BusinessAccount`（不得重复定义冲突，以 design 为准）。
- R4 小号页：列表/新增/解绑（二次确认）、单登/批量登录、进直播间、发弹幕（二次确认 + 结果展示）、互动记录查询。
- R5 i18n 前缀：`biz.comments.*`、`biz.sub.*`，中英同步。
- R6 导航：在业务分组下注册 2 个入口。

## Acceptance Criteria

- [ ] 15 命令 TS 封装齐全，`npm run build` 通过。
- [ ] 字典测试通过；2 个页面 IPC-mock 渲染用例通过。
- [ ] 桌面内可完成：监听启停 + 事件流只读验证、小号列表/登录/互动记录只读验证。
