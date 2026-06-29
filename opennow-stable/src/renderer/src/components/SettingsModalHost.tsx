import { useEffect, useState, type JSX, type ReactNode } from "react";
import { createPortal } from "react-dom";
import { AnimatePresence, m } from "motion/react";
import { smoothEase } from "./MotionProvider";
import { useTranslation } from "../i18n";

const overlayExitTransition = {
  duration: 0.22,
  ease: smoothEase,
} as const;

export interface SettingsModalHostProps {
  open: boolean;
  onClose: () => void;
  onExitComplete?: () => void;
  children: ReactNode;
}

export function SettingsModalHost({
  open,
  onClose,
  onExitComplete,
  children,
}: SettingsModalHostProps): JSX.Element | null {
  const { t } = useTranslation();
  const [contentReady, setContentReady] = useState(false);

  useEffect(() => {
    if (!open) {
      return undefined;
    }

    setContentReady(false);
    const frame = window.requestAnimationFrame(() => {
      setContentReady(true);
    });
    return () => window.cancelAnimationFrame(frame);
  }, [open]);

  const handleExitComplete = (): void => {
    setContentReady(false);
    onExitComplete?.();
  };

  if (typeof document === "undefined") {
    return null;
  }

  return createPortal(
    <AnimatePresence initial={false} onExitComplete={handleExitComplete}>
      {open ? (
        <m.div
          key="settings-modal"
          className="animated-modal-overlay"
          role="dialog"
          aria-modal="true"
          aria-label={t("settings.title")}
          initial={false}
          animate={{ opacity: 1 }}
          exit={{ opacity: 0 }}
          transition={overlayExitTransition}
        >
          <button
            type="button"
            className="animated-modal-scrim"
            onClick={onClose}
            aria-label={t("app.actions.close")}
          />

          <div
            className="animated-modal-panel settings-modal"
            onClick={(event) => event.stopPropagation()}
          >
            {contentReady ? children : (
              <div className="settings-modal-placeholder" aria-hidden />
            )}
          </div>
        </m.div>
      ) : null}
    </AnimatePresence>,
    document.body,
  );
}
