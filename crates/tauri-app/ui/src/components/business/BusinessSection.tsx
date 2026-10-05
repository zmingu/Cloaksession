import type { JSX } from "react";

import { useT } from "../../i18n/LanguageProvider";

/**
 * 「其它」section 壳。
 *
 * 原「业务」板块的 15 个页签已按分类搬迁到「直播」：
 *   开播准备 / 直播互动（三个子区）/ 投流。
 * 这里只保留空态占位（组件壳留作后续使用）。
 */
export function BusinessSection(): JSX.Element {
  const t = useT();
  return (
    <div className="flex-1 flex flex-col min-w-0 min-h-0 px-6 py-4">
      <h2 className="text-[15px] font-semibold">{t("nav.business")}</h2>
      <div className="mt-2 text-[13px] text-muted-foreground">{t("live.wip")}</div>
    </div>
  );
}
