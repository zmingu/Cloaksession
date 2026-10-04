# Design — jieger业务前端补齐

## 导航结构

- `Sidebar.tsx` / `LeftRail.tsx` / `App.tsx` 的 `Section` 类型扩展：新增 `"business"` section（或业务分组下的子导航，implement 时二选一，默认独立 section 以避免改动现有三项布局）。
- 业务 section 内按 5 组设子导航：账号开播（A）、监听小号（B）、话术场控（C）、商品弹窗（D）、金牛跟播（E）。
- 快捷键：现有 `1/2/,` 保留；业务 section 用 `3`，组内子项不用数字快捷键（避免冲突）。
- `App.tsx` 的 `section` 持久化（`usePersistedState<"section">`）需兼容旧值；新增 section 不做旧数据迁移。

## 文件布局

```
ui/src/lib/
  kuaishouAuth.ts      # A
  mateLogin.ts         # A
  liveLaunch.ts        # A
  liveRoomMonitor.ts   # A
  commentListener.ts   # B
  subAccounts.ts       # B（复用 businessAccounts.ts 的 BusinessAccount，不得重定义）
  autoMessage.ts       # C
  autoReply.ts         # C
  scenes.ts            # C
  productScripts.ts    # D
  shopHelper.ts        # D
  autoPopup.ts         # D
  jinniuPromote.ts     # E（新建）
  # bindCreator → ipc.ts 已有；huiboLive.ts 已有
ui/src/types.ts        # 各模块类型（枚举字面量以后端 serde 为准）
ui/src/components/business/
  <Group>*.tsx         # 每组 2–4 个页面组件，复用 atoms/Button/Modal/Pill
ui/src/i18n/en.ts + zh-CN.ts  # 前缀见下
ui/tests/              # 每组 IPC-mock 渲染用例
```

## 共享约定（防冲突）

- `types.ts`：各组追加各自模块类型；`BusinessAccount` 已存在，B 组直接 import，禁止重定义。
- i18n key 前缀（全局唯一）：`biz.auth.*`（A-auth）、`biz.mate.*`（A-mate）、`biz.live.*`（A-launch）、`biz.monitor.*`（A-monitor）、`biz.comments.*`（B）、`biz.sub.*`（B）、`biz.msg.*`（C-msg）、`biz.reply.*`（C-reply）、`biz.scene.*`（C-scene）、`biz.pscript.*`（D-script）、`biz.helper.*`（D-helper）、`biz.popup.*`（D-popup）、`biz.jinniu.*`（E）、`biz.bind.*`（E）、`biz.huibo.*`（E）。
- 事件订阅：`listen` 必须 `await` 注册/清理；放在页面级 `useEffect`，卸载时 unlisten；不得在模块顶层订阅。
- 三态字段：`scene_update.groupId`（`Option<Option<string>>`）与 `UpdateShopProductScriptInput.description` 用 `{ mode: "keep" | "clear" | "set", value? }` 表达，implement 时落实。
- `sendAt: f64`（秒，浮点）原样透传，不得转毫秒。
- `insertRandomSpace`：C 组不得出现在任何类型、表单、IPC 参数中。

## 页面模式

- 列表页：加载态 → 空态 → 表格/卡片；i18n 空态文案。
- 启停页：状态轮询（`setInterval` + 清理）或事件驱动，二选一在各组 implement 注明。
- 写操作：二次确认（复用 `Confirm` 组件）+ 后端 error 原文展示 + 操作级中文模板（不改 `MultizenError::Display`，只做前端文案）。
- QR 页（mate）：`qrImageDataUrl` 直接 `<img>`；终态机（success/expired/cancelled/error）对应 UI 状态。

## 兼容与降级

- 后端命令缺失（旧构建）时页面展示不可用提示，不崩溃（参考 `refreshGroups` 的 try/catch 降级）。
- Playwright 用例 mock Tauri IPC（现有模式），只证渲染路径，不证真号链路；真号验收保持手动、逐个、只读先行。
