# 技术设计 — 快手小店账号初始化

## 规划门禁

状态为planning，尚待本轮最终批准。PRD全部R1–R5与AC1–AC9继续有效；本设计细化不代表产品实现或平台写验收授权。待批准决策集中在 implementation-ready.md。

## 1. 当前基线与旧方案差异

- Rust/chromiumoxide负责业务，Chromix仅保留启动桥；不引入Node业务worker。研究中一次性Node/CDP工具不属于产品架构。
- `driver/identity`已有自动轮询、ID校验、多页面冲突、头像本地缓存、手动检测；R1后端/缓存已验收，UI视觉未单独核实。
- `kuaishou_identities`按platform_user_id持久化身份；`kuaishou_identity_observations`按profile保存即时观察，删环境级联删除观察但保留身份档案。
- `business_accounts`为人工登记与绑定，独立于自动观察；不自动覆盖别名、平台ID或绑定。scope、手工ID冲突均延续现有保护。
- 原`kuaishou_accounts`单表草案被以下增量模型取代，不重复维护昵称/头像，不以单个profile_id作为账号生命周期。
- ProfileManager原SQLite连接留在launcher线程，通过mpsc/oneshot访问；CDP、OCR、附件I/O不占用数据库事务。

## 2. 数据与附件设计（拟实施）

新增主体档案和步骤记录，表名可在实施中按规范确定：

| 记录 | 建议字段/约束 |
|---|---|
| 主体档案 | platform_user_id唯一、real_name、id_card、来源(main-tab/talent-tab-plaintext/ocr)、附件引用、校验结果、review_status、revision、时间 |
| 初始化步骤 | (platform_user_id,step)唯一，step=subject/slice；pending/running/done/failed、attempts、next_retry_at、last_error_code、completed_at |
| 附件 | 应用级受控key、类型、内容哈希、大小、图片解码元信息、档案引用；支持同一证件字段多个附件，不假设单张jpg |

身份刷新仍由现有身份模块负责，不拿“身份已检测”冒充初始化完成。资料采集完成与人工核对分开：完整文本+可用本地照片可进入待核对；待核对不反复采集、不阻塞切片。OCR失败/文本不完整/图片不可用保留未完成。

- 本地SQLite明文与应用级附件目录，不放profiles环境目录，不使用加密。
- 主体/步骤不随profiles级联删除；需要关联时用可空关系，账号本身按平台ID持续存在。
- 临时文件写入→内容/尺寸/解码验证→原子落位→数据库引用提交。失败清理只清理无引用附件；删除环境不删除资料。
- 附件只通过受控key读取；禁止任意路径IPC。日志不记录完整身份证号、图片、含敏感参数URL或整页原文。
- 确认请求携带revision，后端重验字段和照片版本；拒绝过期确认，避免并发覆盖。

## 3. 档案查询与UI

现有身份list从环境列表投影，不能用于孤立档案查询。新增独立档案查询/详情接口，经launcher访问数据库，按姓名、快手ID、身份证号参数化搜索并分页。

- 保留Profiles页现有搜索入口，结果分为环境与档案；孤立档案按平台ID去重，显示无关联环境/环境已删除，可看、复制，不提供启动浏览器。
- 复用ProfileTile/ProfileRow现有KuaishouIdentitySummary和详情入口，不再添加另一套身份头像；补充初始化未完成/待核对/完成标记，身份状态与初始化状态不能混淆。
- 扩展当前详情为大尺寸资料面板：头像/昵称/ID/关联环境、资料来源、照片、自动校验结果、可编辑候选、确认、重新识别、补做入口。
- 复制格式为快手ID+姓名+身份证号；待核对允许复制且明确标注。身份证与图片仅在必要查询/详情读取，不批量预载所有证件。
- 后端自动调度不依赖UI轮询，列表IPC仍为只读；手动重新检测与补做明确分离，不把现有只读detect偷偷改成权限写操作。

## 4. 真实页面映射与限制

完整依据：[research/live-page-structure-2026-10-01.md](research/live-page-structure-2026-10-01.md)。以下是单样本结构证据，不是写入/图片验收。

### 主体信息

- 路由`https://s.kwaixiaodian.com/zone/shop/info/qualification`。
- 精确Tab「主体信息」「达人主体信息」；验证aria-selected并限定`.ant-tabs-tabpane-active`。旧面板DOM保留，不能只凭client rect或全页字段定位。
- 主体行`.section-item`，精确`.item-title`→同级`.item-content`：经营者姓名、证件类型、经营者证件号码、经营者证件。
- 达人行`.ant-row-flex`，按精确标签取同行值：分销者姓名、证件类型、证件号、证件照片；hash class只作辅助。
- 本样本主体为未遮罩文字候选，号码仅验证18位形态；达人文字有遮罩，不承诺完整明文，更不推断服务器有完整值。
- 两证件行各2个同源HTTPS img，只读对应字段内图片；达人面板额外外域图片不得误收。
- 图片真实内容/清晰度/受保护读取未验证。必须在后续授权验证读取链路，落本地后真实解码/读取验证；URL、naturalWidth或文件头不能单独作为采集成功。
- 不套用公共头像GET（yximgs、无凭据）的能力获取证件；必要的受保护图像读取另立最小能力边界，不泛化任意网络/文件读取。
- 待批准：仅自然人经营者/分销者已验证字段；未知企业/法人结构失败待处理，不猜法律主体类型。

### 全自动发布权限

- 路由`https://s.kwaixiaodian.com/zone/short-video-b/slice`；入口精确「修改设置」。用户已手动打开供只读检查，程序入口点击副作用仍待受控验证。
- 可见抽屉`.kwaishop-tianhe-shortVideoB-pc-drawer-content`，精确标题「直播切片托管设置」，没有role=dialog。
- 只在精确标题「全自动发布权限」区内定位`.collapseHeader label`的以下四个主checkbox，校验唯一标签集合、数量与区头计数：
  1. 直播中商品详解切片权限托管
  2. 直播爆品切片返场权限托管
  3. 直播引流片段发布权限托管
  4. 切片个人主页展示位置
- 样本为开启4/4。第四项不可自行省略；未知新增主项/禁用态/计数异常应停止报告，不能扩大范围。
- 明确排除：智能生产权限switch、限制单位时间切片发布量checkbox、所有radio、数字输入及服务协议。
- 没有观察到保存按钮，不代表即时保存；隐藏「确定」modal与该流程关系未知，不得盲点。
- 实施阶段先在本地fixture模拟流程；真实保存语义须受控验证。完成条件：四项关闭且关闭设置后重读/刷新持久验证通过；仅点击成功、内存DOM变化或无保存按钮不能标done。

## 5. 本地OCR与双重保护

待批准候选：Windows首版系统中文OCR，Rust直接WinRT集成，不靠PowerShell运行产品业务。

本机只读检查结果：AvailableRecognizerLanguages含en-US、zh-Hans-CN、zh-Hant-HK；TryCreateFromLanguage(zh-Hans-CN)成功，MaxImageDimension=10000。PATH无tesseract/paddleocr；当前Python无paddleocr/paddle/onnxruntime/rapidocr/pytesseract；项目扫描无onnx/traineddata/pdmodel。不是全盘模型清单，也不是识别准确率证明。

需真实补齐：Windows条件依赖、图像解码与内存流、语言检测、尺寸/方向处理、有界CPU任务、字段提取、错误分类。不得只定义OcrEngine trait或返回空候选后算完成。先用合成中文图片和虚构号码进行真实引擎测试；不自动下载资源、不上传证件、不自动转云端。其他OCR方案需要新增模型/运行时，不视为现成后备。

校验：18位结构、末位X规范化、校验码、有效出生日期及非未来日期；与页面可见片段比对。不能静默替换易混字符后宣称可靠，也不能把无可比片段当通过。验证结果不是法律真实性证明。

OCR结果始终先待核对；用户对照图片修正后后端重验。待批准规则：校验失败允许修正和未核对复制，不可强制变为已核对。人工确认不占用TaskPage、不阻塞切片流程。

## 6. 调度、TaskPage与恢复

- 复用registry SessionSlot、共享BrowserSession、session UUID与取消；每个环境独立，额外按平台ID去重防止同账号跨环境重复修改。
- 从当前有效、无冲突身份观察进入初始化；每次写操作前核对准确origin、账号ID、会话代次、业务scope/绑定，提交数据库再核对预期上下文。
- 待批准：自建专用任务页保留用户主页面；只关闭本次任务拥有的成功页面，失败页保留供排查。关闭浏览器/环境删除优先于保留页策略，不为保留页重启浏览器。
- 任务页创建/所有权/回收需要新增受控封装；TaskPage本身不创建、不关闭页。不能退回active-page路由或任意raw Page逃逸。
- acquire及每步有限超时，另有任务整体deadline；&mut TaskPage顺序执行。超时/取消/Interrupted后drop，恢复重新获取并核对状态。
- TaskPage仅是同一session/target协作锁，不防用户或旧接口干预；不能把锁当平台事务。
- 取消不能撤销已发点击。重试先读状态，只关闭仍开启项，绝不盲目toggle；状态不明先持久验证，仍无法判定则失败待补做。
- subject/slice独立记录；有限自动重试+退避，仍失败显示卡片未完成，下次检测补做。OCR待确认不算需要不断重新采集；已done的切片不在以后检测中持续纠正。
- 重启残留running恢复待验证/补做，不直接done。错误信息脱敏；人工重检不自动重置已完成步骤。

## 7. 兼容性与验收边界

保留现有profiles结构和手工业务档案语义；新增表与命令隔离。现有身份只读spec不能直接视为批准新增导航/写操作，实施前更新对应契约。
普通测试只用本地fixture/临时数据库/合成图；真实权限最终写验收前再确认。生产目标仍是无前置弹窗自动初始化，不因验收门禁改写R4.1。
风险：不同账号结构、图片质量与保护链路、系统语言缺失、保存语义/延迟、同账号并发和取消后在途副作用均须明确测试。不得把静态DOM研究、mock响应或引擎可创建当端到端完成。
