# Implement — jieger业务前端补齐

## 执行顺序

1. 父任务只做规划（prd/design/implement + research/command-surface.md），不写业务代码。
2. 子任务按 A → B → C → D → E 串行执行（单人开发；共享 `types.ts` / i18n 字典，串行避免冲突）。
   - 每组内：`lib/*.ts` 封装 → `types.ts` 类型 → 页面组件 → 导航注册 → i18n 中英 → Playwright mock 用例。
3. 每组完成后：`npm run build`（ui）+ 字典测试 + `npx playwright test`（相关用例）+ `cargo check -p tauri-app`（确认没碰后端）。
4. 全部 5 组完成后：父任务做最终集成检查（导航无冲突、key 无重复、AC1–AC5），再 commit。

## 每组验证命令（ui 目录）

```powershell
cd crates/tauri-app/ui
npm run build
node --experimental-strip-types src/i18n/dictionaries.test.mjs
npx playwright test --config playwright.config.ts <本组用例>
```

Rust 侧只做只读确认（无修改预期）：

```powershell
$env:CARGO_TARGET_DIR='D:/cargo-target'
cargo check -p tauri-app
```

## 回滚点

- 每组独立 commit；某组返工只 revert 该组提交，不动其他组。
- `types.ts` / i18n 冲突时以 design.md 的前缀约定为准，后写者 rebase。

## Review gates

- 每组 `task.py start` 前：prd（已写）+ 本 implement + 父 design 已审。
- 每组完成后：trellis-check（或手工 `npm run build` + 字典测试 + 用例）再合入。
- 父任务收尾：全量 AC1–AC5 核对后 commit（Phase 3.4 流程，另行确认）。
