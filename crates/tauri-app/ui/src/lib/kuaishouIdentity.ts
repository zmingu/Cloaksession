import { invoke } from "@tauri-apps/api/core";

/** Observed identity only; never a login assertion or a manual account record. */
export interface KuaishouIdentitySnapshot {
  profileId: string;
  status: "unknown" | "detected" | "not-detected" | "conflict" | "closed" | "skipped" | "error";
  platformUserId: string | null;
  nickname: string | null;
  avatarKey: string | null;
  checkedAt: string | null;
  lastSeenAt: string | null;
  message: string | null;
}

const STATUSES = new Set(["unknown", "detected", "not-detected", "conflict", "closed", "skipped", "error"]);
function decodeSnapshot(value: unknown): KuaishouIdentitySnapshot {
  if (!value || typeof value !== "object") throw new Error("身份检测结果格式无效");
  const item = value as Record<string, unknown>;
  if (typeof item.profileId !== "string" || !item.profileId || !STATUSES.has(String(item.status))
    || !["platformUserId", "nickname", "avatarKey", "checkedAt", "lastSeenAt", "message"]
      .every((key) => item[key] === null || typeof item[key] === "string")) {
    throw new Error("身份检测结果字段无效");
  }
  return value as KuaishouIdentitySnapshot;
}

export const kuaishouIdentity = {
  list: async (): Promise<KuaishouIdentitySnapshot[]> => {
    const result = await invoke<unknown>("kuaishou_identity_list");
    if (!Array.isArray(result)) throw new Error("身份检测列表格式无效");
    return result.map(decodeSnapshot);
  },
  detect: async (profileId: string): Promise<KuaishouIdentitySnapshot> => {
    const result = decodeSnapshot(await invoke<unknown>("kuaishou_identity_detect", { profileId }));
    if (result.profileId !== profileId) throw new Error("身份检测结果与请求的 Profile 不符");
    return result;
  },
  avatar: (avatarKey: string): Promise<string | null> => invoke("kuaishou_identity_avatar", { avatarKey }),
};

export const MAX_AVATAR_KEY_LENGTH = 256;
const MAX_IMAGE_CHARS = 4 * 1024 * 1024;

/** No SVG, HTML, network URLs or MIME-masquerading payloads reach an img src. */
export function safeIdentityImage(value: unknown): string | null {
  if (typeof value !== "string" || value.length > MAX_IMAGE_CHARS) return null;
  const match = /^data:image\/(png|jpeg|webp|gif);base64,([A-Za-z0-9+/]+={0,2})$/.exec(value);
  if (!match || match[2].length % 4 !== 0) return null;
  try {
    const header = atob(match[2].slice(0, 24));
    const valid = match[1] === "png" ? header.startsWith("\x89PNG\r\n\x1a\n")
      : match[1] === "jpeg" ? header.startsWith("\xff\xd8\xff")
      : match[1] === "gif" ? /^GIF8[79]a/.test(header)
      : header.startsWith("RIFF") && header.slice(8, 12) === "WEBP";
    return valid ? value : null;
  } catch {
    return null;
  }
}

/** Per-provider, memory-only LRU: 64 results / 16 MiB, 4 requests and 64 queued max. */
export class IdentityAvatarCache {
  private results = new Map<string, string | null>();
  private pending = new Map<string, Promise<string | null>>();
  private queue: Array<() => void> = [];
  private active = 0;
  private chars = 0;
  private disposed = false;

  get = (key: string): Promise<string | null> => {
    if (this.disposed || !key || key.length > MAX_AVATAR_KEY_LENGTH) return Promise.resolve(null);
    if (this.results.has(key)) {
      const value = this.results.get(key)!;
      this.results.delete(key);
      this.results.set(key, value);
      return Promise.resolve(value);
    }
    const existing = this.pending.get(key);
    if (existing) return existing;
    if (this.queue.length >= 64) return Promise.resolve(null);
    const promise = new Promise<string | null>((resolve) => {
      this.queue.push(() => {
        if (this.disposed) { resolve(null); return; }
        this.active++;
        void kuaishouIdentity.avatar(key).then(safeIdentityImage, () => null).then((value) => {
          if (!this.disposed) {
            this.results.set(key, value);
            this.chars += value?.length ?? 0;
            while (this.results.size > 64 || this.chars > 16 * 1024 * 1024) {
              const oldest = this.results.keys().next().value!;
              this.chars -= this.results.get(oldest)?.length ?? 0;
              this.results.delete(oldest);
            }
          }
          resolve(this.disposed ? null : value);
        }).finally(() => {
          this.pending.delete(key);
          this.active--;
          this.drain();
        });
      });
    });
    this.pending.set(key, promise);
    this.drain();
    return promise;
  };

  private drain(): void {
    while (this.active < 4 && this.queue.length) this.queue.shift()!();
  }

  dispose(): void {
    this.disposed = true;
    this.results.clear();
    this.chars = 0;
    this.drain(); // Resolve queued consumers without issuing more IPC.
    this.pending.clear();
  }
}
