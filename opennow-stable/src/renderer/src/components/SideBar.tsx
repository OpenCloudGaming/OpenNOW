import type { JSX, ReactNode } from "react";
import { useTranslation } from "../i18n";

interface SideBarProps {
  title?: string;
  children?: ReactNode;
  className?: string;
  onClose?: () => void;
}

export default function SideBar({
  title,
  children,
  className = "",
  onClose,
}: SideBarProps): JSX.Element {
  const { t } = useTranslation();
  const classNames = ["sidebar", className].filter(Boolean).join(" ");
  const sidebarTitle = title ?? t("sidebar.title");

  return (
    <aside
      className={classNames}
      role="dialog"
      aria-label={sidebarTitle}
    >
      <div className="sidebar-header">
        <h3>{sidebarTitle}</h3>
        {onClose && (
          <button
            type="button"
            className="sidebar-close"
            onClick={onClose}
            aria-label={t("sidebar.closeSettings")}
          >
            ✕
          </button>
        )}
      </div>
      <div className="sidebar-body">
        {children}
      </div>
    </aside>
  );
}
