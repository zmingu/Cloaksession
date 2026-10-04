import { invoke } from "@tauri-apps/api/core";

import type {
  AutoReplyRecord,
  GoodsKnowledge,
  ReplyResult,
} from "../types";

/**
 * 自动回复（关键词，jieger `tasks/autoReply` + `replyEngine`）。
 *
 * Covers the 3 `auto_reply_*` commands: keyword-hit preview plus
 * `auto_reply_records` history/manual record. The live consumer resolves
 * and records through its own seams; these serve UI preview/testing and
 * history inspection.
 */
export const autoReply = {
  /**
   * `auto_reply_preview` → `ReplyResult`. Pure keyword-hit preview:
   * resolves `question` against caller-supplied goods knowledge.
   * No DB, no listener, no danmaku.
   */
  preview: (
    question: string,
    goods: GoodsKnowledge[],
  ): Promise<ReplyResult> =>
    invoke<ReplyResult>("auto_reply_preview", { question, goods }),

  /**
   * `auto_reply_history` → `AutoReplyRecord[]`, newest first.
   * Empty `accountId` lists all accounts. `limit` defaults to 50.
   */
  history: (accountId: string, limit?: number): Promise<AutoReplyRecord[]> =>
    invoke<AutoReplyRecord[]>("auto_reply_history", {
      accountId,
      limit: limit ?? null,
    }),

  /**
   * `auto_reply_record` → `AutoReplyRecord`. Manual/test write to
   * `auto_reply_records` (preview verification, seeding). Live replies
   * are recorded by the consumer's recorder, not here.
   */
  record: (
    accountId: string,
    content: string,
    reply: string,
    source: string,
    goodsId?: string | null,
  ): Promise<AutoReplyRecord> =>
    invoke<AutoReplyRecord>("auto_reply_record", {
      accountId,
      content,
      reply,
      source,
      goodsId: goodsId ?? null,
    }),
};
