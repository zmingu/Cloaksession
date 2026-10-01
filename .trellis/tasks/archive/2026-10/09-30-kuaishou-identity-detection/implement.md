# Implement

授权：用户持续批准继续；主代理管理总TODO，本代理不新建/改写其TODO。

1. [x] 读取AGENTS、四层backend索引/具体规范、guides、前批review及原PRD；检查task.py/common task_store/task_utils/config hooks。
2. [x] task.py create采用进程内mock common.git.run_git及禁止subprocess.Popen，未执行Git/外部hook；写prd/design/implement/context并安全start。脚本fallback base_branch不是真实Git证据，清空为null。
3. [x] 新增固定模型、独立迁移/存储、回查保护与离线测试。
4. [x] 新增TaskPage extractor/解码/多target聚合及有界检测；共享本地CDP peer验证逐target固定路由。
5. [x] registry世代取消、launcher命令、轮询共用gate、启动挂载及固定IPC。
6. [x] 头像可信URL、启动代理快照、限流下载、hash原子缓存及读取安全测试；系统/PAC不明确时警告不直连。
7. [x] 仅fmt本批18个Rust文件；cargo test --workspace --locked：232通过/0失败/2忽略，cargo check --workspace --locked通过；日志已记录。
8. [x] backend spec、results-backend及局限已记录，保持in_progress交主代理；结束get核对无遗留TODO。

不运行真实浏览器/平台或ignored测试，不执行Git，不改UI/Chromix桥/Node worker。验证不等于真实选择器或原生联调验收。

## 并行收尾A：已加载头像读取（2026-09-30）

- 最小差距：无明确代理时现有逻辑跳过头像，即便浏览器已加载该图片。先利用已加载图片，失败才走现有明确代理下载或跳过，ID不受影响。
- 范围仅cdp-driver窄资源能力、identity提取/头像编排、专属cached_resource测试与本文结果记录；不改driver.rs/registry.rs、三个IPC及EXTRACTOR的四字段wire DTO。
- 读取固定target/main frame，复核准确账户DOM的ID、currentSrc、complete/natural dimensions和会话世代；变化丢弃。严格HTTPS443/yximgs无userinfo、<=2MiB解码、magic/hash路径边界。
- `Page.getResourceContent`只有在本地验证能证明不额外取网后才采用；不能证明时明确降级为纯已加载img canvas，跨域污染失败跳过，绝不fetch/重设src或crossOrigin/截图页面。
- 已决策：C的独立Chrome153测试中，该命令读已加载PNG时额外请求主文档（Fetch3→4，Network3→3），MISS无新增也不能推翻命中风险；A已降级纯canvas并复用现有TaskPage::evaluate，无新增cdp-driver生产API。
- 复用tests/common本地wire peer；运行cdp-driver和tauri-app lib的locked测试。独立真实引擎本地fixture由并行C负责，不能混称真实平台验收。最终写results-avatar-cache.md。

## 2026-10-01 头像修复授权增量

本次仅头像授权将旧canvas-only失败策略扩展为受控浏览器公开头像GET，不扩大身份/主体/切片业务。最小实现、方法选择、源文件和验证见 results-avatar-browser-get.md；对应tauri-app/backend/kuaishou-identity.md及cdp-driver/backend/task-control.md增加当前契约。旧Page.getResourceContent禁用结论保持。

已完成浏览器响应流能力、Skip路径接入、账号/文档前后复核、取消清理串行保护、Chrome151独立fixture及Rust检查回归。当前仅代码/隔离引擎验收，真实用户重启后头像缓存仍由主代理独立复核；未执行Git、用户DB写入或用户进程重启。
