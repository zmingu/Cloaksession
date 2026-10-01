# 真实页面只读结构调研（2026-10-01）

## 范围与执行边界

- Windows；已先读本任务 prd.md、design.md、implement.md，未进入实现阶段。
- 重新确认 localhost:58774 CDP 可用，Chrome 151、协议 1.3。45908 是用户提供的 app PID，不是端口；前次将其作为端口检查的解释错误，已撤回，不构成 app 可用性结论。本次直接连接现有浏览器，不使用 TaskPage，不建立 Node 业务 worker。
- 原用户页为小店主页。本次通过原生 Node WebSocket/CDP 新建两个后台调研标签，访问指定资质页、切片页。两页均正常加载且呈现业务数据/界面，未跳登录页；未读取账号标识，故这不是产品“读到账号标识才算登录”的验收。
- 操作仅为：创建调研标签、DOM 脱敏求值、切换主体信息/达人主体信息 Tab、滚动切片内容、关闭本次两个自建标签。结束时确认原主页仍在，page 数量恢复为 1。
- 未修改产品代码或规划三文档；未运行 Git；未切换权限、点击确认/保存、提交表单、修改账号设置；未读取 cookie、token、存储或网络响应；未截图、下载或输出证件图片内容。
- 页面端只返回标签、结构/class、数量、遮罩布尔值/长度、类型关键词及图片域名；未输出姓名、证件号码、图片完整地址。未检查框架数据对象、接口或原始 HTML。

## 一、资质管理

调研路由：`https://s.kwaixiaodian.com/zone/shop/info/qualification`

### 1. Tab 实际标签

`[role="tab"]` 共 4 个：

1. 主体信息
2. **达人主体信息**（不是规划中的“达人主体”）
3. 品牌资质（未点击）
4. 店铺经营许可（未点击）

选中态为 `aria-selected="true"`；激活内容面板为 `.ant-tabs-tabpane-active`。

**重要陷阱**：切到达人页后，主体页 DOM 仍保留；inactive 面板甚至可有非空 client rect。不能仅用 `getClientRects()` 或全页 `.item-title` 判定当前页字段，必须限定激活面板。否则会把主体页未遮罩字段误当成达人页结果。

### 2. 主体信息字段

行结构：`.section-item > .item-title` 与同级 `.item-content`。

| label | 文本长度 | 常见遮罩字符命中 | 备注 |
|---|---:|---|---|
| 经营者姓名 | 3 | false | CSS text-security 未开启 |
| 证件类型 | 9 | false | 命中“大陆 / 居民 / 身份证”类型关键词 |
| 经营者证件号码 | 18 | false | 浏览器内检查 17 位数字加末位数字/X 的形态为 true；未验校验码或真实性 |
| 经营者证件有效期 | 1 | false | 仅统计长度，不解释内容 |
| 经营者证件 | 0 | false | 包含 2 个 img |

遮罩检测使用常见 `* / ＊ / • / ●` 字符；未命中不等于完成法律真实性或视觉可读性验证。本样本主体姓名/号码可作为 DOM 文本提取候选，但此次没有导出值。

证件行图片证据：

- `img` 数量 2，已加载；canvas 数量 0。
- 两图 scheme 均为 `https:`，来源域均为 `s.kwaixiaodian.com`，未发现该行 blob 载体。
- 图片 class：`sc-kDThTU bBpNfQ`；外包装：`sc-jrQzUz enWSfS`。
- 不曾打开预览或读取像素，因此不能确认图片是原件、占位图、遮罩缩略图还是可用于 OCR 的清晰文件；“已加载”只代表 naturalWidth > 0。

### 3. 达人主体信息字段

行结构：`.ant-row-flex`，label 列 `.ant-col.xo5JsJaXplMilrOn7xhg`，value 列 `.ant-col.ZM8cR7N8Z0afMBjK3QvT`。

| label | 文本长度 | 常见遮罩字符命中 | 图片数 |
|---|---:|---|---:|
| 分销者姓名 | 3 | true | 0 |
| 证件类型 | 9 | false | 0 |
| 证件号 | 16 | true | 0 |
| 证件有效期 | 10 | false | 0 |
| 证件照片 | 0 | false | 2 |

- 证件类型也命中“大陆 / 居民 / 身份证”关键词。
- **该样本达人 DOM 姓名和证件号有遮罩，不应承诺达人页可获得完整明文。** 未检查服务器响应，不能推断服务端是否返回完整值。
- “证件照片”行有 2 个 HTTPS img，域均为 `s.kwaixiaodian.com`；class `sc-ieebsP fjpJNi`，包装 `sc-bkkfTU bXNcFD`；该行 canvas 为 0，无 blob 来源。
- 激活面板另外有 1 个 HTTPS img，域 `p5-ec.eckwai.com`、class `APytblw8dU8f82nvnGXl`；该图不在“证件照片”字段内，**不能当作证件附件**。未判定其内容。

### 4. 个人/企业覆盖与翻页缺口

- 本样本只观察到自然人姓名与居民身份证字段；激活面板未找到精确为“个人 / 企业 / 个体工商户 / 个人店 / 企业店”的类型值。
- **不能据此断言店铺法律主体属于个人、个体工商户或企业。** 未观察企业名称、统一社会信用代码、法人分支，不能编造企业选择器。
- 主体面板 clientHeight/scrollHeight 均 458；达人面板均 435；两面板分页 class 数量均 0。当前样本字段无面板内部纵向滚动缺口，但不代表其他账号/资质类型不存在多段、多页或懒加载。

### 5. selector 建议（只建议，未写实现）

- 按 `[role="tab"]` 的 **精确文本** 找“主体信息”或“达人主体信息”，验证唯一命中及 `aria-selected`，再限定 `.ant-tabs-tabpane-active`。
- 主体：遍历激活面板 `.section-item`，匹配 `.item-title` 精确标签，取该行 `.item-content`。不要依赖行号。
- 达人：在激活面板内按 `.ant-row-flex` 首个列精确 label 定位，再取同一行第二列；hash class 仅辅助验证，不作唯一长期契约。
- 图片只能在已定位的“经营者证件”/“证件照片”字段 value 内查 `img`，不能全页抓图；需另行验证原图、预览、实际图片可用性。
- 未来提取应把 masked=true 判为不完整；证件格式及校验码需独立验证。OCR 尚未证明可用，本次未读取照片内容。

## 二、直播切片

调研路由：`https://s.kwaixiaodian.com/zone/short-video-b/slice`

### 1. 首次只读调研（设置未打开时）

- 页面有“直播切片”“自动发布”标签；业务 Tab 为待发布、已发布、发布失败。后两页未点击，不是权限分页。
- 实际入口是 **“修改设置”**，不是“去设置”；为 `button.kwaishop-tianhe-shortVideoB-pc-btn`，type=button，未 disabled、不在 form 内。初次因副作用不明确没有点击。
- 设置未打开时，页面 switch 数量 0，仅 1 个 disabled=true、checked=false 的表格 checkbox，不能用来判断权限状态。
- 隐藏 `[role="dialog"]` 包含“取 消”“确 定”（后者 class 含 `sure___rpn0p`）；不能把隐藏通用弹层当作权限确认流程证据。
- 首次内容 `.js-page-content` 高 570、scrollHeight 946，滚至底部 376 后控件数量不变；首页无分页 class。此结论仅适用于未打开设置的首页。

### 2. 用户手动打开后的补充检查（解除入口调研阻塞）

用户明确确认“已打开设置界面，可以只读检查”。重新发现原用户标签已由用户导航到切片页，设置保持打开。补充检查仅执行 DOM 求值：**未点击、滚动、关闭该用户标签或抽屉，未更改任何控件，也未点击保存/确认**。

- 可见设置为抽屉，标题 **“直播切片托管设置”**。
- 容器 `.kwaishop-tianhe-shortVideoB-pc-drawer-content`，标题 `.kwaishop-tianhe-shortVideoB-pc-drawer-title`，内容 `.kwaishop-tianhe-shortVideoB-pc-drawer-body`。
- 抽屉没有 `role=dialog`；只查 `[role=dialog]` 会漏掉真实设置。之前隐藏的确认 modal 此时仍隐藏。
- 主要权限区 `.WuFhZjfBowihPEVt5UUF`，用其子标题 `.gh6LEC5pPxFLJTnYMjOy` 的精确文本区分“全自动发布权限”和“智能生产权限”。hash class 仅为本次证据。

### 3. 全自动发布权限（不是 switch，而是 checkbox）

“全自动发布权限”区标记 **开启4/4**。四个主项位于 `.collapseHeader`，每个主项恰好一个 `label input[type="checkbox"]`：

| 精确 label | checked | disabled |
|---|---|---|
| 直播中商品详解切片权限托管 | true | false |
| 直播爆品切片返场权限托管 | true | false |
| 直播引流片段发布权限托管 | true | false |
| 切片个人主页展示位置 | true | false |

第四项虽名称像展示配置，但确实与前三项同属该权限区、同为 collapseHeader 主 checkbox，且区头显示 4/4；实现前不要擅自只处理前三项，也不要据此扩展为所有 checkbox。

同一区域还有第 5 个 checkbox：**限制单位时间切片发布量**，checked=false、disabled=false，不在 `.collapseHeader`，不得误当作第五个全自动发布主权限。

权限区下属 **12 个 radio** 均 disabled=false，当前状态：

- 商品详解 → 智能关联商品：“开启”true、“关闭”false。
- 商品详解 → 发布后自动设为私密作品：“不设私”true、“下播后设私”false、延时设私选项 false。
- 直播爆品 → 发布后自动设置为私密作品：“不设私”true、延时设私选项 false。
- 直播引流 → 发布后自动设置为私密作品：“不设私”true、“下播后设私”false、延时设私选项 false。
- 切片个人主页展示位置：“直播片段”false、“作品”true。
- 三个延时设私 radio 的 `closest(label).textContent` 为空；说明文本“发布后”“天后设私”在周边结构，不应依赖 label 文本定位这些子项。本次没有读取或修改数值输入。

### 4. 智能生产权限（独立，不能误关）

抽屉只有 **1 个 `[role="switch"]`**，它属于 **“智能生产权限”**，不是“全自动发布权限”。

- 元素为 `button`、type=button，`aria-checked="true"`、disabled=false。
- class：`kwaishop-tianhe-shortVideoB-pc-switch kwaishop-tianhe-shortVideoB-pc-switch-checked`。
- 外层行 `.dQiUBIcC4s9oFuX909F4`，位于另一个权限区。
- **原计划“关闭所有全自动发布开关”不能实现为遍历全部 switch 关闭**，否则只会改动不同业务含义的智能生产权限，并漏掉四个目标 checkbox。

### 5. 保存/确认形态与静态 DOM 的证据限度

- 可见抽屉按钮包括“已托管”（disabled=true）和“服务协议”（disabled=false），均 type=button。
- 抽屉另有 6 个 `span[role="button"]`，是三组数字输入增减器（handler-up/down、aria-disabled=false），不是保存按钮。加上智能生产 switch，`button,[role=button]` 合计 9 个。
- 抽屉内未观察到精确“保存”“确认”“确定”“取消”的动作标签；“关闭”文本属于智能关联商品的 radio 选项，不能作为关闭抽屉按钮使用。
- 抽屉内 form 数量 0；底部 `.Lx9IeKKqquDjt0IObewd` 实际是“服务协议”容器，不是保存栏。
- **不能由“没有保存按钮”推断 checkbox 即时提交或自动保存。** 未切换权限、未查看事件处理器/框架状态、未读取网络响应，所以实际请求、持久化时机、二次确认、生效延迟、失败行为、回读验证仍未知。
- 页面保留的隐藏通用确认 modal 与权限变更的关联尚未证实；未来不得预先盲点“确 定”。

### 6. 滚动覆盖与 selector 建议

- 抽屉 body clientHeight=980、scrollHeight=1011、scrollTop=0，有 31 像素纵向溢出。本次保留用户滚动位置，不触发滚动或折叠；当前挂载 DOM 已枚举全部上述控件。
- 抽屉分页 class 数量 0、`aria-expanded` 控件数量 0；四组子配置已挂载，但不能排除别的样本或权限变更后动态新增项。未验证滚动触发的懒加载。
- 入口候选仍为 `.js-page-content button` 精确文本“修改设置”；本次由用户打开，代理未验证入口点击副作用。
- 设置定位：要求可见 `.kwaishop-tianhe-shortVideoB-pc-drawer-content` 且标题精确为“直播切片托管设置”，不可只找 `[role=dialog]`。
- 在标题为“全自动发布权限”的区块内，逐个按 `.collapseHeader label` 精确文本匹配四个主项，读取对应 `input[type=checkbox].checked/disabled`；验证标签集合唯一且区头计数一致，不用行序号或全页 checkbox。
- “智能生产权限”、限量 checkbox、radio、数字输入全部明确排除。未知标签/禁用态/主项数量变化应停止并报告，不能自动扩大修改范围。
- 未来若授权执行修改，仍需逐项校验页面状态并验证持久化；本次只提供 selector 与静态状态证据，不构成修改流程验收。

## 三、可实现程度与人工确认项

- **主体 DOM 提取：可进入后续设计细化**，两 Tab 与自然人字段有真实结构证据。
- **达人完整明文：本样本不满足**；照片/OCR 仅载体结构已证实，原件可用性、遮罩/清晰度、下载链路和识别准确率未验证。
- **个人/企业通用支持：尚未验证企业及个体工商户分支**，需对应合法授权样本。
- **全自动发布权限的入口/结构调研阻塞已解除**：真实目标是 4 个主 checkbox，当前均开启；唯一 switch 是独立智能生产权限。可据此修正规划和 selector 设计，不能宣称关闭操作已完成。
- **剩余动态未知**：切换后是否即时保存、是否二次确认、持久化/失败回读及动态新增项，静态 DOM 无法证明；需要后续单独授权的交互验收或用户说明，不在本次只读范围内。
- 45908 为 PID，不是端口；不存在由本次调研得出的 app 端口阻塞项。
- 本次仅新增并补充本文档，无产品变更，不运行产品编译；未进入规划之后的实现阶段。用户当前设置标签和所有权限状态保持原样。

