import { invoke } from "@tauri-apps/api/core";

export type ValidationCheck = "passed" | "failed" | "unavailable";
export type SubjectSource = "main-tab" | "talent-tab-plaintext" | "ocr";
export type SubjectReviewStatus = "pending-review" | "confirmed";
export interface SubjectValidation { name: ValidationCheck; structure: ValidationCheck; checksum: ValidationCheck; birthDate: ValidationCheck; visibleName: ValidationCheck; visibleIdCard: ValidationCheck }
export interface SubjectAttachment { key: string; sha256: string; mimeType: string; byteLen: number; width: number; height: number }
export interface KuaishouSubjectArchive {
  platformUserId: string; realName: string; idCard: string; source: SubjectSource;
  evidence: { realName: string | null; idCard: string | null }; attachments: SubjectAttachment[];
  validation: SubjectValidation; reviewStatus: SubjectReviewStatus; revision: number;
  sourceProfileId: string | null; createdAt: string; updatedAt: string; confirmedAt: string | null;
}
export interface KuaishouArchiveProfile { profileId: string; name: string }
export interface KuaishouSubjectSummary {
  platformUserId: string; realName: string; maskedIdCard: string; nickname: string | null; avatarKey: string | null;
  source: SubjectSource; reviewStatus: SubjectReviewStatus; revision: number; updatedAt: string; profiles: KuaishouArchiveProfile[];
}
export interface KuaishouSubjectPage { items: KuaishouSubjectSummary[]; total: number; offset: number; limit: number }
export type KuaishouInitErrorCode = "interrupted-needs-verification" | "context-changed" | "timed-out" | "page-unsupported" | "page-crashed" | "attachment-unavailable" | "ocr-unavailable" | "ocr-failed" | "validation-failed" | "persistence-unverified";
export interface KuaishouInitStepRecord { platformUserId: string; step: "subject" | "slice"; state: "pending" | "running" | "done" | "failed"; attempts: number; nextRetryAt: string | null; lastErrorCode: KuaishouInitErrorCode | null; completedAt: string | null; updatedAt: string }
export interface KuaishouSubjectDetail { archive: KuaishouSubjectArchive; nickname: string | null; avatarKey: string | null; profiles: KuaishouArchiveProfile[]; steps: KuaishouInitStepRecord[] }

const object = (v: unknown): v is Record<string, unknown> => !!v && typeof v === "object" && !Array.isArray(v);
const text = (v: unknown): v is string => typeof v === "string";
const nullable = (v: unknown) => v === null || text(v);
const integer = (v: unknown) => typeof v === "number" && Number.isSafeInteger(v) && v >= 0;
const source = (v: unknown) => ["main-tab", "talent-tab-plaintext", "ocr"].includes(String(v));
const review = (v: unknown) => ["pending-review", "confirmed"].includes(String(v));
const association = (v: unknown) => Array.isArray(v) && v.every(p => object(p) && text(p.profileId) && text(p.name));
export const validationLabels: Record<keyof SubjectValidation, string> = { name: "姓名", structure: "18位结构", checksum: "校验码", birthDate: "出生日期", visibleName: "页面可见姓名", visibleIdCard: "页面可见证件号" };
function archive(v: unknown, id: string): KuaishouSubjectArchive {
  if (!object(v) || v.platformUserId !== id || !text(v.realName) || !text(v.idCard) || !source(v.source) || !review(v.reviewStatus) || !integer(v.revision)
    || !object(v.evidence) || !nullable(v.evidence.realName) || !nullable(v.evidence.idCard)
    || !object(v.validation) || !Object.keys(validationLabels).every(k => ["passed", "failed", "unavailable"].includes(String((v.validation as Record<string, unknown>)[k])))
    || !Array.isArray(v.attachments) || v.attachments.length > 8 || !v.attachments.every(a => object(a) && text(a.key) && /^[a-f0-9]{64}\.(png|jpg|webp)$/.test(a.key) && text(a.sha256) && text(a.mimeType) && [a.byteLen, a.width, a.height].every(integer))
    || !nullable(v.sourceProfileId) || !text(v.createdAt) || !text(v.updatedAt) || !nullable(v.confirmedAt)) throw new Error("主体档案响应无效，请刷新");
  // SAFETY: every field above is validated (types, enum membership, attachment shape); the runtime object now matches KuaishouSubjectArchive.
  return v as unknown as KuaishouSubjectArchive;
}
function steps(v: unknown, id: string): KuaishouInitStepRecord[] {
  if (!Array.isArray(v) || !v.every(s => object(s) && s.platformUserId === id && ["subject", "slice"].includes(String(s.step)) && ["pending", "running", "done", "failed"].includes(String(s.state)) && integer(s.attempts) && nullable(s.nextRetryAt) && nullable(s.lastErrorCode) && nullable(s.completedAt) && text(s.updatedAt))) throw new Error("初始化状态响应无效");
  return v as KuaishouInitStepRecord[];
}
export function canConfirm(a: KuaishouSubjectArchive): boolean {
  const v = a.validation;
  return a.attachments.length > 0 && [v.name, v.structure, v.checksum, v.birthDate].every(s => s === "passed") && v.visibleName !== "failed" && v.visibleIdCard !== "failed";
}
export function subjectCopy(a: KuaishouSubjectArchive): string {
  return `${a.reviewStatus === "confirmed" ? "已核对" : "未核对"}\n快手ID：${a.platformUserId}\n姓名：${a.realName}\n身份证号：${a.idCard}`;
}
/** Three-line clipboard payload for the identity detail dialog (no review prefix). */
export function identityCopyText(platformUserId: string, realName: string, idCard: string): string {
  return `快手ID：${platformUserId}\n姓名：${realName}\n身份证号：${idCard}`;
}
export const kuaishouSubject = {
  list: async (search: string, offset: number, limit = 10): Promise<KuaishouSubjectPage> => {
    const v = await invoke<unknown>("kuaishou_subject_list", { query: { search, offset, limit } });
    if (!object(v) || !integer(v.total) || v.offset !== offset || v.limit !== limit || !Array.isArray(v.items) || !v.items.every(s => object(s) && text(s.platformUserId) && text(s.realName) && text(s.maskedIdCard) && nullable(s.nickname) && nullable(s.avatarKey) && source(s.source) && review(s.reviewStatus) && integer(s.revision) && text(s.updatedAt) && association(s.profiles))) throw new Error("档案列表响应无效");
    // SAFETY: total/offset/limit and every item field are validated above; the value matches KuaishouSubjectPage.
    return v as unknown as KuaishouSubjectPage;
  },
  detail: async (platformUserId: string): Promise<KuaishouSubjectDetail | null> => {
    const v = await invoke<unknown>("kuaishou_subject_detail", { platformUserId });
    if (v === null) return null;
    if (!object(v) || !nullable(v.nickname) || !nullable(v.avatarKey) || !association(v.profiles)) throw new Error("档案详情响应无效");
    return { archive: archive(v.archive, platformUserId), steps: steps(v.steps, platformUserId), nickname: v.nickname as string | null, avatarKey: v.avatarKey as string | null, profiles: v.profiles as KuaishouArchiveProfile[] };
  },
  correct: async (input: { platformUserId: string; expectedRevision: number; realName: string; idCard: string }) => archive(await invoke("kuaishou_subject_correct", { input }), input.platformUserId),
  confirm: async (input: { platformUserId: string; expectedRevision: number }) => archive(await invoke("kuaishou_subject_confirm", { input }), input.platformUserId),
  reocr: async (platformUserId: string, expectedRevision: number) => archive(await invoke("kuaishou_subject_reocr", { platformUserId, expectedRevision }), platformUserId),
  attachment: async (key: string): Promise<string> => {
    const v = await invoke<unknown>("kuaishou_subject_attachment", { key });
    if (!text(v) || v.length > 24 * 1024 * 1024 || !/^data:image\/(png|jpeg|webp);base64,[A-Za-z0-9+/]+={0,2}$/.test(v)) throw new Error("照片不可用");
    return v;
  },
  steps: async (platformUserId: string) => steps(await invoke("kuaishou_init_steps", { platformUserId }), platformUserId),
  retry: (profileId: string): Promise<void> => invoke("kuaishou_init_retry", { profileId }),
  availability: async (): Promise<{ available: boolean; message: string }> => {
    const v = await invoke<unknown>("kuaishou_ocr_availability", {});
    if (!object(v) || typeof v.available !== "boolean" || !text(v.message)) throw new Error("OCR 状态不可用");
    return { available: v.available, message: v.message };
  },
};
