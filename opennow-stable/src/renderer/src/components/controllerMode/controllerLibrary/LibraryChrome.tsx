import type { JSX } from "react";
import { CATEGORY_ACTIVE_HALF_WIDTH_PX, CATEGORY_STEP_PX } from "./constants";

interface LibraryChromeProps {
  logoUrl: string;
  clockElement: JSX.Element;
  userAvatarUrl?: string;
  userName: string;
  categoryIndex: number;
  topCategories: Array<{ id: string; label: string }>;
  getCategoryIcon: (id: string) => JSX.Element;
}

export function LibraryChrome({
  logoUrl,
  clockElement,
  userAvatarUrl,
  userName,
  categoryIndex,
  topCategories,
  getCategoryIcon,
}: LibraryChromeProps): JSX.Element {
  return (
    <>
      <div className="xmb-top-right">
        {clockElement}
        <div className="xmb-user-badge">
          {userAvatarUrl ? (
            <img
              src={userAvatarUrl}
              alt={userName}
              className="xmb-user-avatar"
            />
          ) : (
            <div className="xmb-user-avatar" />
          )}
          <div className="xmb-user-name">{userName}</div>
        </div>
      </div>

      <div className="xmb-top-left">
        <div className="xmb-logo" aria-hidden>
          <img src={logoUrl} alt="OpenNow" />
        </div>
      </div>

      <div
        className="xmb-categories-container"
        style={{ transform: `translate(${-categoryIndex * CATEGORY_STEP_PX - CATEGORY_ACTIVE_HALF_WIDTH_PX}px, -50%)` }}
      >
        {topCategories.map((cat, idx) => {
          const isActive = idx === categoryIndex;
          return (
            <div key={cat.id} className={`xmb-category-item ${isActive ? "active" : ""}`}>
              <div className="xmb-category-icon-wrap">{getCategoryIcon(cat.id)}</div>
              <div className="xmb-category-label">{cat.label}</div>
            </div>
          );
        })}
      </div>
    </>
  );
}
