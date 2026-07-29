import { Lock, Plus, Settings } from "lucide-react";
import type { CSSProperties, JSX } from "react";
import type { SavedAccount } from "@shared/gfn";
import { useTranslation } from "../../i18n";

function getTierLabelKey(tier: string): string {
  const normalized = tier.toUpperCase();
  if (normalized === "ULTIMATE") return "app.labels.ultimate";
  if (normalized === "PRIORITY" || normalized === "PERFORMANCE") return "app.labels.priority";
  return "app.labels.free";
}

export interface ConsoleProfileTileProps {
  account: SavedAccount;
  index: number;
  isFocused: boolean;
  isActive: boolean;
  onSelect: () => void;
}

export function ConsoleProfileTile({
  account,
  index,
  isFocused,
  isActive,
  onSelect,
}: ConsoleProfileTileProps): JSX.Element {
  const { t } = useTranslation();

  return (
    <button
      type="button"
      className={`console-profile-tile${isFocused ? " is-focused" : ""}${account.hasPin ? " console-profile-tile--locked" : ""}`}
      style={{ "--i": index } as CSSProperties}
      data-console-profile={account.userId}
      onClick={onSelect}
      aria-label={account.hasPin
        ? t("console.profiles.lockedProfile", { name: account.displayName })
        : account.displayName}
    >
      <span className="console-profile-avatar">
        {account.avatarUrl
          ? <img src={account.avatarUrl} alt="" />
          : <span className="console-profile-avatar-fallback">{account.displayName.slice(0, 1).toUpperCase()}</span>}
        {account.hasPin && (
          <span className="console-profile-lock" aria-hidden="true"><Lock /></span>
        )}
      </span>
      <span className="console-profile-name">{account.displayName}</span>
      <span className="console-profile-tier">{t(getTierLabelKey(account.membershipTier))}</span>
      {isActive && <span className="console-profile-active">{t("console.profiles.signedIn")}</span>}
    </button>
  );
}

export interface ConsoleProfileActionTileProps {
  action: "add" | "manage";
  index: number;
  isFocused: boolean;
  onSelect: () => void;
}

/** "Add account" / "Manage profiles" — same tile shape, no avatar. */
export function ConsoleProfileActionTile({
  action,
  index,
  isFocused,
  onSelect,
}: ConsoleProfileActionTileProps): JSX.Element {
  const { t } = useTranslation();
  const label = action === "add" ? t("auth.accounts.addAccount") : t("console.profiles.manage");
  const Icon = action === "add" ? Plus : Settings;

  return (
    <button
      type="button"
      className={`console-profile-tile${isFocused ? " is-focused" : ""}`}
      style={{ "--i": index } as CSSProperties}
      data-console-profile-action={action}
      onClick={onSelect}
      aria-label={label}
    >
      <span className="console-profile-avatar">
        <span className="console-profile-avatar-action">
          <Icon size={40} strokeWidth={1.5} />
        </span>
      </span>
      <span className="console-profile-name">{label}</span>
    </button>
  );
}
