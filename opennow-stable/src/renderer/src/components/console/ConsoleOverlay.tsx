import type { JSX, ReactNode } from "react";

export interface ConsoleOverlayProps {
  label: string;
  eyebrow?: string;
  title?: string;
  children: ReactNode;
}

/**
 * Full-screen console overlay.
 *
 * Deliberately not `ui/ModalSurface`: that wraps AnimatePresence, and the
 * console tree is motion-free (see ConsoleRow). Entrance is CSS; there is no
 * exit animation — the component simply unmounts. The console shell already
 * marks the page behind it `inert`, so no focus trap is duplicated here.
 */
export function ConsoleOverlay({ label, eyebrow, title, children }: ConsoleOverlayProps): JSX.Element {
  return (
    <div className="console-overlay" role="dialog" aria-modal="true" aria-label={label}>
      <div className="console-overlay-panel">
        {eyebrow && <span className="console-overlay-eyebrow">{eyebrow}</span>}
        {title && <h3 className="console-overlay-title">{title}</h3>}
        {children}
      </div>
    </div>
  );
}
