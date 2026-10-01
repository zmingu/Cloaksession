# 批次1结果：核心DTO、主体档案与步骤持久化

实现与离线验证完成。仅修改 `crates/multizen-core`、`crates/profile-manager` 及本结果文档，测试使用临时SQLite。未执行Git、真实用户DB写入、停止用户程序或平台读写；本代理未修改根Cargo/lock、tauri-app、cdp-driver、UI及local-ocr。核心新增模块 `multizen_core::kuaishou_account` 在crate根重导出全部类型；`profile_manager::KuaishouInitLease` 在crate根重导出。

## 已实现接口（同步，原ProfileManager连接）

```rust
// 以下省略方法共同的 &self；Result = multizen_core::Result。
kuaishou_subject_save_candidate(lease: &KuaishouInitLease, expected_revision: u64, candidate: SubjectCandidate) -> Result<KuaishouSubjectArchive>;
kuaishou_subject_detail(platform_user_id: &str) -> Result<Option<KuaishouSubjectDetail>>;
kuaishou_subject_list(query: &KuaishouSubjectQuery) -> Result<KuaishouSubjectPage>;
kuaishou_subject_correct(input: CorrectSubjectInput) -> Result<KuaishouSubjectArchive>;
kuaishou_subject_confirm(input: ConfirmSubjectInput, verified_attachments: &[SubjectAttachment]) -> Result<KuaishouSubjectArchive>;
kuaishou_subject_update_ocr(input: CorrectSubjectInput, verified_attachments: &[SubjectAttachment]) -> Result<KuaishouSubjectArchive>;
kuaishou_subject_attachment_referenced(key: &str) -> Result<bool>;
kuaishou_init_claim(context: &KuaishouInitContext, step: KuaishouInitStep) -> Result<Option<KuaishouInitLease>>;
kuaishou_init_steps(platform_user_id: &str) -> Result<Vec<KuaishouInitStepRecord>>;
kuaishou_init_complete_subject(lease: &KuaishouInitLease, expected_revision: u64, verified_attachments: &[SubjectAttachment]) -> Result<()>;
kuaishou_init_complete_slice(lease: &KuaishouInitLease, verification: &SliceVerification) -> Result<()>;
kuaishou_init_fail(lease: &KuaishouInitLease, code: KuaishouInitErrorCode, retry_after_seconds: u32) -> Result<bool>;
kuaishou_init_recover_interrupted() -> Result<usize>;
kuaishou_init_retry_failed(platform_user_id: &str) -> Result<usize>;
```

## 集成必须遵守

- 启动时、任何初始化worker之前，显式调用 `kuaishou_init_recover_interrupted()` 一次。迁移/只读查询/新开连接不自动撤销其他存活执行的lease。running恢复failed，error=`interrupted-needs-verification`，下轮重验后补做，不直接done。
- `KuaishouInitContext { platform_user_id, profile_id, session_id, expected_business: BusinessProfileState }`。claim/save/complete事务检查有效identity观察、相同session、完整业务状态与Profile存在；runtime仍须持registry当前session锁通过同步DB提交。
- `KuaishouInitLease`字段私有、不可由IPC反序列化；仅claim创建，可Clone供launcher消息传递；有`context()`、`step()`、`attempts()`。同账号跨Profile、跨步骤、跨连接最多1个running。done永不被claim/retry/recover重置。无定时强制接管，取消必须完成清理后fail释放。
- `SubjectCandidate { real_name, id_card, source, evidence, attachments }` 是内部类型，无Deserialize；revision=0创建，其余CAS更新。保存候选不标done、不标confirmed。
- `SubjectSource`: `main-tab|talent-tab-plaintext|ocr`；`SubjectReviewStatus`: `pending-review|confirmed`；`ValidationCheck`: `passed|failed|unavailable`。校验结果中的无法比对不冒充通过，字段有效且无明确冲突时允许人工对照图确认。
- `CorrectSubjectInput { platformUserId, expectedRevision, realName, idCard }` 为严格camelCase IPC输入，拒绝额外status/source/evidence/attachment字段。`ConfirmSubjectInput { platformUserId, expectedRevision }` 同理。照片、原页面evidence不能经修正输入替换。
- `SubjectAttachment { key, sha256, mimeType, byteLen, width, height }`。key必须64位小写SHA256加`.png|.jpg|.webp`，MIME严格对应。最多8图，每图1..16MiB，宽高1..10000且<=4000万像素。空附件可保存未完成候选，但不能确认/完成采集。
- 附件owner负责应用级目录原子落位、真实hash/解码/安全读取；确认、OCR更新、完成subject的`verified_attachments`必须来自本次实际读取，禁止直接相信UI或DB元数据；数据库按完整metadata与档案revision再比较。DB本身不读物理文件，不将URL视为附件。
- `SliceVerification { platform_user_id, all_four_disabled: [bool;4], persisted_readback: bool }` 仅backend内部类型，无Deserialize，必须由runtime关闭设置后回读/刷新验证构造，不能把它作为IPC输入或仅凭点击构造。
- corrections/显式re-OCR重置人工核对状态，保留采集完成的历史，不改slice状态；无泛化set-status方法。自动校验失败可存/修正但不可确认。
- `KuaishouSubjectQuery { search, offset:u32, limit:u32 }`，limit=1..100；搜索姓名/平台ID/证件号为参数化字面子串，`%`、`_`、`\\`不是通配符。列表无完整证件号/附件/evidence；详情才返回必要敏感字段。
- 关联环境通过identity_observations动态投影（历史关联，不是当前登录声明）；头像昵称取现有identities，不重建。删Profile不删主体/步骤；sourceProfileId仅保留历史来源字符串，无外键级联。

## 公开DTO字段（wire为camelCase，Option显式null）

- `SubjectVisibleEvidence`: `realName: string|null`, `idCard: string|null`。只存字段级可见证据；mask支持`* ＊ • ●`，连续mask作为可变长度隐藏片段，前后可见片段锚定；没有任何可见内容为unavailable。
- `SubjectValidation`: `name, structure, checksum, birthDate, visibleName, visibleIdCard`，均为ValidationCheck；`can_confirm()`只接受姓名/18位结构/校验码/非未来有效公历日期Passed，且可见比对不为Failed。仅trim与ASCII末位x大写，不替换OCR易混淆数字。不是法律身份真实性证明。
- `KuaishouSubjectArchive`: `platformUserId, realName, idCard, source, evidence, attachments, validation, reviewStatus, revision:u64, sourceProfileId:string|null, createdAt, updatedAt, confirmedAt:string|null`。
- `KuaishouArchiveProfile`: `profileId, name`。
- `KuaishouSubjectSummary`: `platformUserId, realName, maskedIdCard, nickname:string|null, avatarKey:string|null, source, reviewStatus, revision:u64, updatedAt, profiles:KuaishouArchiveProfile[]`。
- `KuaishouSubjectPage`: `items:KuaishouSubjectSummary[], total:u64, offset:u32, limit:u32`。
- `KuaishouSubjectDetail`: `archive:KuaishouSubjectArchive, nickname:string|null, avatarKey:string|null, profiles:KuaishouArchiveProfile[], steps:KuaishouInitStepRecord[]`。
- `KuaishouInitStepRecord`: `platformUserId, step:subject|slice, state:pending|running|done|failed, attempts:u32, nextRetryAt:string|null, lastErrorCode:KuaishouInitErrorCode|null, completedAt:string|null, updatedAt`。每个账号首次claim创建两个步骤；steps查询未创建的账号返回空数组。时间都是UTC RFC3339字符串。
- `KuaishouInitErrorCode`: `interrupted-needs-verification|context-changed|timed-out|page-unsupported|attachment-unavailable|ocr-unavailable|ocr-failed|validation-failed|persistence-unverified`。不能直接存平台异常原文或URL。
- 核心函数：`normalize_subject_id_card(&str)->String`、`masked_subject_id_card(&str)->String`、`compare_subject_visible(value:&str,evidence:Option<&str>)->ValidationCheck`、`validate_subject_fields(real_name:&str,id_card:&str,evidence:&SubjectVisibleEvidence,today:&str)->SubjectValidation`、`valid_subject_attachment_key(&str)->bool`、`valid_subject_attachments(&[SubjectAttachment])->bool`、`SubjectAttachment::is_valid()->bool`。today由backend给出UTC YYYY-MM-DD，不接受renderer日期。

## 存储与实现细节

- 新增 `kuaishou_subject_archives`：平台ID主键指向已有identities，单份JSON文档包含typed档案数据；姓名/证件号/revision为JSON生成列，避免双写不一致。查询SQL只选列表需要的投影，不批量反序列化全部证件和附件。
- 新增 `kuaishou_init_steps`：`(platform_user_id,step)`唯一；running必须带私有lease token，done必须有completed_at；按账号running部分唯一索引阻止跨步骤/跨环境重入。所有业务值参数化。
- revision在候选保存、修正、确认、显式OCR更新时递增；版本不匹配返回固定中文错误，不覆盖新数据。
- 从SQL读取异常JSON时返回固定中文错误，不把serde的unexpected-value原文带出。证件相关候选/档案/确认修正内容/搜索输入Debug已脱敏。生产模块没有新增日志、panic、占位返回或unimplemented。
- 手动重试只清failed的退避，不重置running/done。fail只接受当前token，因此关闭/删除环境之后仍能由原owner释放失败；先完成在途任务清理是runtime义务，DB不假定CDP已停止。
- 本次未扩展spec文件（超出本代理文件所有权）；主代理可据本结果更新持久化和core契约spec。

## 验证证据

2026-10-01 Windows PowerShell执行：

```powershell
cargo test -p multizen-core -p profile-manager --locked
cargo clippy -p multizen-core -p profile-manager --locked --all-targets --no-deps -- -D warnings
```

均exit 0：**54项测试通过，0失败、0忽略**（core原3+新增4，profile-manager原37+新增10）；Clippy零警告。8个本批Rust文件的`rustfmt --edition 2021 --config skip_children=true --check ...`通过。未运行真实平台或用户数据库测试。

新增core测试：
1. 结构/校验码/末位x、闰年、无效日期、未来日期、非法姓名/日期输入。
2. mask可见片段锚定、压缩mask、Unavailable与Failed区分。
3. 附件key路径逃逸、MIME/hash/尺寸/字节边界、重复与空图拒绝。
4. camelCase/严格额外字段拒绝、Debug与搜索证件脱敏。

新增ProfileManager测试：
1. 原始profiles旧库升级、重复迁移、保存/reopen与done保留。
2. 删除环境后主体/多附件引用/步骤/已有昵称头像保留，孤立档案仍按姓名/ID/证件搜索，列表不含完整证件。
3. `%`、`_`、反斜线、SQL注入字符串按字面搜索；分页边界与稳定排序。
4. 过期确认/修正拒绝，确认前字段与图片完整元数据版本重验；修正/OCR回到待核对且保留原始证据；done采集历史不被重置。
5. 无效附件不落库、无效字段/空照片不能done、触发器模拟写失败事务回滚且无错误附件引用。
6. 同账号两Profile、两个DB连接共享排他lease；不同账号独立；换号后环境关联动态更新。
7. session改变/手工绑定改变/环境删除拒绝过期提交；原lease仍可安全失败释放，不覆盖手工资料。
8. 退避、重复fail幂等、attempt递增、旧token拒绝、四项关闭和持久回读缺一不可、done不能重检重置。
9. reopen后显式启动恢复将running转待验证failed、attempt保留、旧token撤销，无直接done。
10. 将合成号码注入非法持久枚举后读取返回脱敏中文错误且不panic。

## 后续集成边界

本批证明DTO/校验/持久化与安全调用契约，不证明实际图片读取、OCR准确率、平台保存语义、Tauri IPC或UI端到端。那些由各自owner完成。local-ocr代理报告其解码采用更严格10MiB/1600万像素限制，应用接收应遵守实际解码器限制，不为达到core上限放宽解码保护。数据库中的元数据比较不是物理文件校验的替代品。
