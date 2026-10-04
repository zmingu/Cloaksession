# 前端C组-话术与场控

## Goal

为 `auto_message`（3）、`auto_reply`（3）、`scene_play`（11）共 17 个命令补齐 TS 封装、类型、页面、导航、i18n。命令签名见父任务 `research/command-surface.md §8–10`。

## Requirements

- R1 `lib/autoMessage.ts`：`start/stop/status` 封装；`MessageLine{offsetSec, message, accountId}` / `AutoMessageStarted` / `AutoMessageState` 类型。**不得封装或提供 `insertRandomSpace` 选项**（安全约束，AC5 卡点）。
- R2 定时消息页：行列表编辑（offset/message/accountId）、启动/停止/状态轮询。
- R3 `lib/autoReply.ts`：`preview/question+goods`（直接返回 `ReplyResult` 非 Result）、`history`、`record` 封装；`GoodsKnowledge` / `ReplyResult` / `AutoReplyRecord` 类型。
- R4 自动回复页：商品知识库编辑（`qa` 问答对）、问题预览、历史记录查询。
- R5 `lib/scenes.ts`：11 命令全封装；`Scene` / `SceneLine` / `PlaySceneOptions` / `PlayStarted` 类型；`scene_update` 的 `groupId` 三态（不改/置空/设值）在 design 注明表达方式。
- R6 场控页：场景 CRUD、行编辑 + 排序、播放/停止（二次确认）。
- R7 i18n 前缀：`biz.msg.*`、`biz.reply.*`、`biz.scene.*`，中英同步。
- R8 导航：在业务分组下注册 3 个入口。

## Acceptance Criteria

- [ ] 17 命令 TS 封装齐全（`insertRandomSpace` 无任何前端入口），`npm run build` 通过。
- [ ] 字典测试通过；3 个页面 IPC-mock 渲染用例通过。
- [ ] 桌面内可完成：话术/回复/场景的 CRUD 只读验证；播放类写操作有二次确认。
