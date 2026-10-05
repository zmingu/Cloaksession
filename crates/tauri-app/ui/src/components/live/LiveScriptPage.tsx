import type { JSX } from "react";

import { useT } from "../../i18n/LanguageProvider";

/**
 * 开播准备 › 脚本处理占位入口。
 *
 * 多视频多脚本：每个卖货视频对应一份弹幕脚本（read/cta/cue），开播前选用。
 * 完整工坊（导入转写 → 配 LLM → 生成 → 三类预览 → 存库 → 推送上车/发言/话术库）
 * 随后接入；本占位只定版式与测试锚点，不触碰任何 IPC。
 */
export function LiveScriptPage(): JSX.Element {
  const t = useT();

  return (
    <section
      data-testid="live-prepare-script"
      aria-label={t("live.prepare.script")}
      className="flex flex-col gap-4 p-6 max-w-3xl"
    >
      <div>
        <h2 className="text-lg font-semibold">{t("live.prepare.script")}</h2>
        <p className="text-sm text-muted-foreground mt-1">{t("live.prepare.scriptDesc")}</p>
      </div>
    </section>
  );
}
