import { useEffect } from "react";
import { AnimatePresence, m } from "motion/react";
import type { JSX } from "react";
import { smoothEase } from "./MotionProvider";
import { useTranslation } from "../i18n";

const SPLASH_VISIBLE_MS = 2800;

export interface SessionStartedSplashProps {
  visible: boolean;
  gameTitle: string;
  onFinished: () => void;
}

export function SessionStartedSplash({
  visible,
  gameTitle,
  onFinished,
}: SessionStartedSplashProps): JSX.Element | null {
  const { t } = useTranslation();

  useEffect(() => {
    if (!visible) {
      return undefined;
    }
    const timer = window.setTimeout(onFinished, SPLASH_VISIBLE_MS);
    return () => window.clearTimeout(timer);
  }, [onFinished, visible]);

  return (
    <AnimatePresence>
      {visible && (
        <m.div
          className="sv-ready-splash"
          role="status"
          aria-live="polite"
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          exit={{ opacity: 0 }}
          transition={{ duration: 0.45, ease: smoothEase }}
        >
          <m.div
            className="sv-ready-splash-backdrop"
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            transition={{ duration: 0.55, ease: smoothEase }}
          />
          <m.div
            className="sv-ready-splash-card"
            initial={{ opacity: 0, scale: 0.92, y: 18 }}
            animate={{ opacity: 1, scale: 1, y: 0 }}
            exit={{ opacity: 0, scale: 1.04, y: -10 }}
            transition={{ duration: 0.55, ease: smoothEase }}
          >
            <div className="sv-ready-splash-ring" aria-hidden>
              <span className="sv-ready-splash-ring-core" />
              <span className="sv-ready-splash-ring-pulse" />
            </div>
            <p className="sv-ready-splash-kicker">{t("stream.sessionStarted.kicker")}</p>
            <h2 className="sv-ready-splash-title">{t("stream.sessionStarted.title")}</h2>
            <p className="sv-ready-splash-game">{gameTitle}</p>
          </m.div>
        </m.div>
      )}
    </AnimatePresence>
  );
}
