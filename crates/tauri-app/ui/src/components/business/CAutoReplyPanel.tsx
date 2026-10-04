import { useState, type JSX } from "react";

import { Button } from "../atoms/Button";
import { useT } from "../../i18n/LanguageProvider";
import { autoReply } from "../../lib/autoReply";
import type { AutoReplyRecord, ReplyResult } from "../../types";

/**
 * C-group: 自动回复（关键词）.
 *
 * Keyword-hit preview against caller-supplied goods knowledge (no DB, no
 * listener, no danmaku), plus persisted `auto_reply_records` history and a
 * confirmed manual record write for preview verification / seeding.
 */
export function CAutoReplyPanel(): JSX.Element {
  const t = useT();
  const [question, setQuestion] = useState("");
  const [goodsTitle, setGoodsTitle] = useState("");
  const [goodsTokens, setGoodsTokens] = useState("");
  const [preview, setPreview] = useState<ReplyResult | null>(null);
  const [accountId, setAccountId] = useState("");
  const [history, setHistory] = useState<AutoReplyRecord[] | null>(null);
  const [confirmingRecord, setConfirmingRecord] = useState(false);
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  async function doPreview(): Promise<void> {
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const result = await autoReply.preview(question.trim(), [
        {
          goodsId: "preview-goods",
          title: goodsTitle.trim(),
          tokens: goodsTokens
            .split(/[,，]/)
            .map((s) => s.trim())
            .filter((s) => s !== ""),
        },
      ]);
      setPreview(result);
    } catch (e) {
      setError(t("biz.reply.failedToast", { detail: String(e) }));
    } finally {
      setBusy(false);
    }
  }

  async function doHistory(): Promise<void> {
    setBusy(true);
    setError(null);
    try {
      setHistory(await autoReply.history(accountId.trim()));
    } catch (e) {
      setError(t("biz.reply.failedToast", { detail: String(e) }));
    } finally {
      setBusy(false);
    }
  }

  async function doRecord(): Promise<void> {
    if (!preview?.reply) return;
    setBusy(true);
    setError(null);
    try {
      await autoReply.record(
        accountId.trim() === "" ? "preview" : accountId.trim(),
        question.trim(),
        preview.reply,
        preview.source,
        preview.goodsId,
      );
      setNotice(t("biz.reply.recordedToast"));
      if (accountId.trim() !== "") setHistory(await autoReply.history(accountId.trim()));
    } catch (e) {
      setError(t("biz.reply.failedToast", { detail: String(e) }));
    } finally {
      setBusy(false);
      setConfirmingRecord(false);
    }
  }

  return (
    <section aria-label={t("biz.reply.title")} className="space-y-4">
      <p className="text-[12px] text-slate-400">{t("biz.reply.desc")}</p>

      <div className="grid grid-cols-2 gap-2">
        <label className="col-span-2 flex flex-col gap-1 text-[12px] text-slate-300">
          {t("biz.reply.form.question")}
          <input
            aria-label={t("biz.reply.form.question")}
            className="rounded-md bg-white/5 px-2 py-1.5 text-slate-100"
            value={question}
            onChange={(e) => setQuestion(e.target.value)}
          />
        </label>
        <label className="flex flex-col gap-1 text-[12px] text-slate-300">
          {t("biz.reply.form.goodsTitle")}
          <input
            aria-label={t("biz.reply.form.goodsTitle")}
            className="rounded-md bg-white/5 px-2 py-1.5 text-slate-100"
            value={goodsTitle}
            onChange={(e) => setGoodsTitle(e.target.value)}
          />
        </label>
        <label className="flex flex-col gap-1 text-[12px] text-slate-300">
          {t("biz.reply.form.goodsTokens")}
          <input
            aria-label={t("biz.reply.form.goodsTokens")}
            className="rounded-md bg-white/5 px-2 py-1.5 text-slate-100"
            value={goodsTokens}
            onChange={(e) => setGoodsTokens(e.target.value)}
          />
        </label>
      </div>
      <div className="flex items-center gap-2">
        <Button variant="primary" disabled={busy || question.trim() === ""} onClick={() => void doPreview()}>
          {t("biz.reply.preview")}
        </Button>
        <Button
          variant="secondary"
          disabled={busy || preview?.reply == null}
          onClick={() => setConfirmingRecord(true)}
        >
          {t("biz.reply.record.add")}
        </Button>
      </div>

      <div aria-label={t("biz.reply.preview")}>
        {preview == null ? (
          <p className="text-[12px] text-slate-500">{t("biz.reply.preview.empty")}</p>
        ) : (
          <div className="space-y-1 text-[12px] text-slate-300">
            <p>
              {t("biz.reply.reply.label")}: {preview.reply ?? "—"}
            </p>
            <p>
              {t("biz.reply.source.label")}: {preview.source}
            </p>
          </div>
        )}
      </div>

      {confirmingRecord && (
        <div role="group" aria-label={t("biz.reply.confirm.recordTitle")} className="rounded-lg border border-amber-400/25 p-3 space-y-2">
          <p className="text-[12px] text-slate-300">{t("biz.reply.confirm.recordBody")}</p>
          <div className="flex gap-2">
            <Button variant="primary" size="sm" disabled={busy} onClick={() => void doRecord()}>
              {t("biz.reply.confirm.confirm")}
            </Button>
            <Button variant="ghost" size="sm" disabled={busy} onClick={() => setConfirmingRecord(false)}>
              {t("biz.reply.confirm.cancel")}
            </Button>
          </div>
        </div>
      )}

      <div className="flex items-center gap-2">
        <label className="flex flex-1 flex-col gap-1 text-[12px] text-slate-300">
          {t("biz.reply.form.account")}
          <input
            aria-label={t("biz.reply.form.account")}
            className="rounded-md bg-white/5 px-2 py-1.5 text-slate-100"
            value={accountId}
            onChange={(e) => setAccountId(e.target.value)}
          />
        </label>
        <Button variant="ghost" disabled={busy} onClick={() => void doHistory()}>
          {t("biz.reply.history.refresh")}
        </Button>
      </div>

      <div aria-label={t("biz.reply.history.title")}>
        {history == null || history.length === 0 ? (
          <p className="text-[12px] text-slate-500">{t("biz.reply.history.empty")}</p>
        ) : (
          <ul className="space-y-1">
            {history.map((record) => (
              <li key={record.id} className="text-[12px] text-slate-300">
                {record.content} → {record.reply} ({record.source})
              </li>
            ))}
          </ul>
        )}
      </div>

      {notice && <p role="status" className="text-[12px] text-emerald-300">{notice}</p>}
      {error && <p role="alert" className="text-[12px] text-red-300">{error}</p>}
    </section>
  );
}
