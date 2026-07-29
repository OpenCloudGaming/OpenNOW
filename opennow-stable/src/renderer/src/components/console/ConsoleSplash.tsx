import type { JSX } from "react";
import { OpenNowLogoMark } from "../OpenNowLogoMark";
import { useTranslation } from "../../i18n";

/**
 * Branded hand-off shown for a beat when console mode starts, before the
 * profile picker. Purely decorative — it advances on a timer and any button
 * press skips it, so it can never hold the user up.
 */
export function ConsoleSplash(): JSX.Element {
  const { t } = useTranslation();

  return (
    <div className="console-splash" role="status" aria-label={t("console.splash.title")}>
      <div className="console-splash-glow" aria-hidden="true" />
      <div className="console-splash-mark">
        <OpenNowLogoMark className="console-splash-logo" />
      </div>
      <h1 className="console-splash-title">{t("console.splash.title")}</h1>
      <p className="console-splash-subtitle">{t("console.splash.subtitle")}</p>
      <div className="console-splash-bar" aria-hidden="true"><span /></div>
    </div>
  );
}
