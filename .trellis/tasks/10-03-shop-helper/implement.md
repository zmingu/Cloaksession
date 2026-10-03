# implement — shop-helper

## 执行清单

### 1. 商品操作基础
- [ ] 1.1 `crates/tauri-app/src/driver/shop_helper.rs` 建 HelperGoodStatus/HelperGoodTab/HelperGoodInfo 结构体
- [ ] 1.2 `HELPER_GOODS_DOM_HELPERS` JS 字符串常量（normalize/isVisible/readId/NOISE_TEXTS/isNoiseText）
- [ ] 1.3 `read_helper_goods(task_page, tab)` evaluate 注入 helpers + 读取商品列表
- [ ] 1.4 `switch_tab(task_page, tab)` 切换小黄车/待上车
- [ ] 1.5 `add_to_cart(task_page, goods_id)` / `remove_from_cart(task_page, goods_id)`
- [ ] 1.6 单元测试：readId 多策略（文本/id 属性/input value/data-id/嵌套）、NOISE_TEXTS 过滤、status 推断

### 2. 脚本化接口
- [ ] 2.1 `goods_on_shelf(account_id, goods_id)` 供 shop-product-script 调用
- [ ] 2.2 `goods_off_shelf(account_id, goods_id)` 供 shop-product-script 调用
- [ ] 2.3 与 ensure_auth 协作（先 ensure_auth 进跟播助手页）

### 3. IPC 命令层
- [ ] 3.1 `crates/tauri-app/src/commands/shop_helper.rs` 新建
- [ ] 3.2 `read_helper_goods`/`switch_tab`/`add_to_cart`/`remove_from_cart`
- [ ] 3.3 状态变化 emit + `commands/mod.rs` 注册 + 前端 ipc.ts wrapper

### 4. 验证
- [ ] 4.1 `cargo test -p tauri-app --locked`（readId + NOISE_TEXTS + status 推断 + mock evaluate）
- [ ] 4.2 `cargo check --workspace --locked`
- [ ] 4.3 真号上车验收（单独确认）

## 验证命令
```bash
cargo test -p tauri-app --locked
cargo check --workspace --locked
```

## Rollback 点
- 1 商品操作基础 commit
- 2 脚本化接口 commit
- 3 IPC 层 commit
- 4 验证后准备 archive

## 依赖前置
- `ks-platform-primitives`（ensure_auth 进 zs.kwaixiaodian.com/page/helper）

## Notes
- **与 CPS 加货架严格隔离**，代码不交叉。
- DOM helpers 用 evaluate 内嵌（快手 DOM 多变，比 locator 鲁棒）。
- readId 多策略全量复刻。
