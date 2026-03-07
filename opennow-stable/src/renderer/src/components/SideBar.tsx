import type { JSX, ReactNode } from "react";

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
  const classNames = ["sidebar", className].filter(Boolean).join(" ");

  return (
    <aside
      className={classNames}
      role="dialog"
      aria-label={title ?? "Sidebar"}
    >
      <div className="sidebar-header">
        <h3>{title ?? "Sidebar"}</h3>
        {onClose && (
          <button
            type="button"
            className="sidebar-close"
            onClick={onClose}
            aria-label="Close settings"
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