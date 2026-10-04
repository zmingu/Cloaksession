# 前端D组-商品与弹窗

## Goal

为 `shop_product_script`（9）、`shop_helper`（4）、`auto_popup`（10）共 23 个命令补齐 TS 封装、类型、页面、导航、i18n。命令签名见父任务 `research/command-surface.md §7,11,12`。

## Requirements

- R1 `lib/productScripts.ts`：9 命令全封装；`ShopProductScript` / `Detail` / `Line` / 各 Input 类型；`UpdateShopProductScriptInput.description` 的 `Option<Option<string>>` 三态在 design 注明。
- R2 商品话术页：脚本 CRUD、行编辑 + 排序。
- R3 `lib/shopHelper.ts`：4 命令全封装；`HelperGoodInfo` / `HelperGoodActionResult` 类型；订阅 `shop-helper:goods-changed`。
- R4 小店助手页：商品读取/tab 切换/加车/移车（写操作二次确认 + 结果展示）。
- R5 `lib/autoPopup.ts`：10 命令全封装；`AutoPopUpConfig` / `ConfigPatch` / `GoodsInfo` / `ScanReport` / `ShortcutRegisterResult` / `AutoPopUpStatus` 类型；订阅 `auto-popup:state` / `auto-popup:event`。
- R6 自动弹窗页：配置编辑、商品列表、扫描报告、讲解一次、启停、快捷键注册/触发（桌面验证快捷键冲突时以后端返回为准）。
- R7 i18n 前缀：`biz.pscript.*`、`biz.helper.*`、`biz.popup.*`，中英同步。
- R8 导航：在业务分组下注册 3 个入口。

## Acceptance Criteria

- [ ] 23 命令 TS 封装齐全，`npm run build` 通过。
- [ ] 字典测试通过；3 个页面 IPC-mock 渲染用例通过。
- [ ] 桌面内可完成：话术脚本 CRUD、商品读取只读验证、弹窗状态只读验证；加车/讲解类写操作有二次确认。
