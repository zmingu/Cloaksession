# 前端E组-金牛与跟播

## Goal

为 `jinniu_promote`（6）、`bind_creator`（3，已有封装，需页面）、`huibo_live`（4，已有封装，需页面）共 13 个命令补齐页面、导航、i18n；前两组封装已存在，只补缺失的类型/页面。命令签名见父任务 `research/command-surface.md §13–15`。

## Requirements

- R1 `lib/jinniuPromote.ts`：6 命令全封装；`JinniuLiveUser(s)` / `StoreCreateTab` / `StoreCreatePhase1Config` 类型。
- R2 金牛推广页：建店向导（开 tab → 直播用户列表/选择 → phase1 配置 → phase2 应用 → 提交，每步展示后端返回，提交需二次确认）。
- R3 `bindCreator`（`ipc.ts` 已有）：补授权页（列表/同步只读先行 + 发起授权二次确认），复用 `types.ts` 的 `AuthorizeItem` / `AuthorizeListResult` / `BindCreatorResult`。
- R4 `huiboLive`（`lib/huiboLive.ts` 已有）：补跟播页（视频列表只读 + 去开播二次确认 + 直播状态 + 取消）；成功以重读的 `ShopLiveState` 为准，不以等待时长证明。
- R5 i18n 前缀：`biz.jinniu.*`、`biz.bind.*`、`biz.huibo.*`，中英同步。
- R6 导航：在业务分组下注册 3 个入口。

## Acceptance Criteria

- [ ] 6+0+0 新增封装（jinniu_promote 新写；另两组复用现有），`npm run build` 通过。
- [ ] 字典测试通过；3 个页面 IPC-mock 渲染用例通过。
- [ ] 桌面内可完成：授权列表同步、回放视频列表只读验证；发起授权/去开播/提交推广有二次确认。
