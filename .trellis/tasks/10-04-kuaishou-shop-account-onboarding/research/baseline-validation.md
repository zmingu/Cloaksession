# 基线校验 + 临时文件清理（节点 node-1）

- 任务：快手小店账号建号向导
- 分支：main，HEAD = d6d2ff8
- 执行时间：2026-10-04 20:46 ~ 20:49（本地）
- 环境：Windows / PowerShell，工作目录 `F:\Cloaksession`
- 说明：本节点只做「清理临时文件 + 观测」，未修改任何源码或测试文件，未执行任何 git 命令。

---

## 1) 临时文件清理

| 文件 | 清理前 | 清理后 |
| --- | --- | --- |
| `crates/tauri-app/ui/tests/_shot2.spec.ts` | 存在（2225 bytes） | 已删除 |
| `crates/tauri-app/ui/shot-wizard.png` | 存在（62946 bytes） | 已删除 |

命令（幂等，`-ErrorAction SilentlyContinue` 保证不存在也不报错）：

```powershell
Remove-Item "crates/tauri-app/ui/tests/_shot2.spec.ts" -ErrorAction SilentlyContinue
Remove-Item "crates/tauri-app/ui/shot-wizard.png" -ErrorAction SilentlyContinue
```

结果：两文件清理前均存在，删除后再次查询为空 → **已清理**。退出码 0。

---

## 2) 全量基线校验（命令 + 退出码 + 关键统计）

### 前端（工作目录 `F:\Cloaksession\crates\tauri-app\ui`）

| # | 命令 | 退出码 | 关键输出 |
| --- | --- | --- | --- |
| 1 | `npx tsc -b --force` | `TSC_EXIT:0` | 无错误输出（`--force` 全量重建，2.0s） |
| 2 | `node --test src/i18n/dictionaries.test.mjs` | `NODE_TEST_EXIT:0` | `ℹ tests 6 / pass 6 / fail 0`（6 个用例全部 ✔） |
| 3 | `npm run build` | `BUILD_EXIT:0` | `vite v6.4.3`，`✓ 1681 modules transformed`，`✓ built in 1.49s`；仅 chunk >500kB 提示（非错误） |
| 4 | `npx playwright test tests/kuaishou-shop.spec.ts tests/kuaishou-shop-wizard.spec.ts --project=desktop-chrome` | `PW_EXIT:0` | `Running 2 tests using 2 workers` → **2 passed (5.7s)** |

命令 2 用例明细（全部 ✔）：
```
✔ en and zh-CN expose identical key sets
✔ every key has an identical named-placeholder set across dictionaries
✔ translate interpolates named placeholders as plain text
✔ translate never interprets HTML in interpolated params
✔ missing params are left visible rather than dropped
✔ normalizeLanguage only accepts the two wire values
```

命令 3 产物：
```
dist/index.html                     0.59 kB │ gzip:   0.37 kB
dist/assets/index-D--AwMWo.css     44.31 kB │ gzip:   8.86 kB
dist/assets/index-HKlD0Vn2.js   1,334.27 kB │ gzip: 410.79 kB
✓ built in 1.49s
```
（stderr 仅 chunk size warning，不影响退出码）

命令 4（目标 spec，全部通过）：
```
ok 1 [desktop-chrome] › tests\kuaishou-shop.spec.ts:10:1 › shop tab lists browser profiles as shop accounts with a launch control (2.6s)
ok 2 [desktop-chrome] › tests\kuaishou-shop-wizard.spec.ts:14:1 › shop account wizard: create, show QR, detect, done, close hidden browser (4.6s)
2 passed (5.7s)
```

### Rust（工作目录 `F:\Cloaksession`）

| # | 命令 | 退出码 | 关键输出 |
| --- | --- | --- | --- |
| 5 | `cargo check --workspace --all-targets` | `CARGO_CHECK_EXIT:0` | `Finished dev profile ... in 0.33s`；**1 个预存在 warning**（见下） |
| 6 | `cargo test -p browser-launcher` | `BL_EXIT:0` | 全部 `test result: ok`；合计 **52 passed / 0 failed / 1 ignored** |
| 7 | `cargo test -p mcp-server` | `MCP_EXIT:0` | 全部 `test result: ok`；合计 **45 passed / 0 failed** |
| 8 | `cargo test -p tauri-app` | `TAURI_EXIT:0` | **222 passed / 0 failed / 2 ignored**（主 lib 224 tests，另有 4 tests bin 全通过） |

命令 5 的预存在 warning（**属预存在，本节点不修**）：
```
warning: function `verify_initialization_identity` is never used
   --> crates\cdp-driver\tests\..\..\tauri-app\src\driver\identity\extract.rs:291:21
warning: `cdp-driver` (test "identity_browser") generated 1 warning
```
即：cdp-driver 的 `identity_browser` 测试中 `verify_initialization_identity` never used（dead_code）。

命令 6 browser-launcher 各 test target 统计：
```
0 tests  → ok
15 tests → ok. 15 passed
5 tests  → ok. 5 passed
7 tests  → ok. 7 passed
5 tests  → ok. 5 passed
0 tests  → ok. 0 passed; 1 ignored
5 tests  → ok. 5 passed
3 tests  → ok. 3 passed
5 tests  → ok. 5 passed
7 tests  → ok. 7 passed
0 tests  → ok
```

命令 7 mcp-server 各 test target 统计：
```
4 tests  → ok. 4 passed
5 tests  → ok. 5 passed
1 test   → ok. 1 passed
0 tests  → ok
9 tests  → ok. 9 passed
20 tests → ok. 20 passed
6 tests  → ok. 6 passed
0 tests  → ok
```

命令 8 tauri-app 统计：
```
running 224 tests → test result: ok. 222 passed; 0 failed; 2 ignored (6.58s)
running 4 tests   → test result: ok. 4 passed (15.00s)
running 0 tests   → ok
```

---

## 3) 预存在破损 spec 失败证据（供下游 node-2 修复）

命令：
```powershell
npx playwright test tests/wave4-verify.spec.ts tests/chromix-settings.spec.ts tests/d-shop-popup.spec.ts --project=desktop-chrome --reporter=list
```
退出码：`PW_BROKEN_EXIT:1`
统计：`Running 14 tests using 2 workers` → **4 failed / 10 passed (1.8m)**

### 失败用例清单

| # | Spec / 行 | 用例名 | 失败原因摘要 |
| --- | --- | --- | --- |
| 1 | `tests/chromix-settings.spec.ts:44:1` | explicit save preserves full SDK JSON and unknown keys across navigation and reload | Test timeout 45000ms；`locator.click` 等待 `getByTitle('Profiles · ⌘1', { exact: true })` 超时（找不到该元素） |
| 2 | `tests/chromix-settings.spec.ts:107:1` | Profile Chromix JSON autosaves independently and survives closing and reopening | 同上，第 108 行等待 `getByTitle('Profiles · ⌘1', { exact: true })` 超时 |
| 3 | `tests/d-shop-popup.spec.ts:110:1` | auto popup goods, scan, and shortcuts round-trip | 第 121 行 `getByRole('button', { name: 'Register shortcuts', exact: true })` 点击被拦截：另一 `<button class="flex items-center gap-2.5 flex-shrink-0 cursor-pointer text-left w-full">`（位于 `div.flex-shrink-0.flex.flex-col.overflow-hidden` 子树）intercepts pointer events → 45s 超时 |
| 4 | `tests/wave4-verify.spec.ts:6:1` | wave4: zh switch translates migrated domains, leaves profile data alone | 第 13 行等待 `getByTitle('Profiles · ⌘1', { exact: true })` 超时 |

### 关键错误原文

失败 1 / 2 / 4（同一根因——侧栏导航标题 `Profiles · ⌘1` 找不到）：
```
Error: locator.click: Test timeout of 45000ms exceeded.
Call log:
  - waiting for getByTitle('Profiles · ⌘1', { exact: true })
```

失败 3（元素被其它可点击子树拦截）：
```
Error: locator.click: Test timeout of 45000ms exceeded.
  - waiting for getByRole('button', { name: 'Register shortcuts', exact: true })
    - locator resolved to <button type="button" data-variant="secondary" class="mz-button ...">Register shortcuts</button>
    - attempting click action
      - <button type="button" class="flex items-center gap-2.5 flex-shrink-0 cursor-pointer text-left w-full">…</button>
        from <div class="flex-shrink-0 flex flex-col overflow-hidden">…</div> subtree intercepts pointer events
```

### 通过用例（同一批，供对照，说明 spec 文件本身可加载）
```
ok  3 chromix-settings.spec.ts:138 invalid JSON, non-object roots and non-string environment values never overwrite settings
ok  4 chromix-settings.spec.ts:175 save failure retains the draft and permits retry without false success
ok  5 chromix-settings.spec.ts:189 legacy engines keep their defaults and Chromix data; binary reset sends an empty string
ok  6 chromix-settings.spec.ts:225 SDK examples, official link and runtime boundaries fit narrow settings viewports
ok  7 d-shop-popup.spec.ts:15 product scripts render, create, and delete with two-step confirm
ok  8 d-shop-popup.spec.ts:40 product script detail edits lines and reorders
ok  9 d-shop-popup.spec.ts:59 shop helper reads goods, boards with confirm, and shows the event panel
ok 10 d-shop-popup.spec.ts:82 auto popup starts, explains once with confirm, and stops with confirm
ok 13 wave4-verify.spec.ts:104 wave4: onboarding name step renders in both languages
ok 14 wave4-verify.spec.ts:114 wave4: onboarding name step renders in Chinese
```

---

## 结论

1. **临时文件已清理**：`_shot2.spec.ts`、`shot-wizard.png` 均确认删除。
2. **前端基线全绿**：tsc 0、i18n 6 pass、build 成功、目标 spec 2 passed。
3. **Rust 基线全绿**：cargo check exit 0（仅 1 个预存在 warning），browser-launcher 52 passed、mcp-server 45 passed、tauri-app 222 passed 全部 0 failed。
4. **预存在破损**：`wave4-verify` / `chromix-settings` / `d-shop-popup` 三 spec 共 4 failed（3 个因侧栏 `Profiles · ⌘1` 元素定位失败，1 个因 `Register shortcuts` 按钮被其它可点击子树拦截）。这些**不是本任务引入**，交由 node-2 修复。
5. 全程未修改源码/测试，未执行 git 命令，符合节点约束。
