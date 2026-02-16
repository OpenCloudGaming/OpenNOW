import type { ActiveSessionInfo, AuthUser, SubscriptionInfo } from "@shared/gfn";
import { House, Library, Settings, User, LogOut, Zap, Timer, HardDrive, X, Loader2, PlayCircle } from "lucide-react";
import { useEffect, useState, type JSX } from "react";
import { createPortal } from "react-dom";

interface NavbarProps {
  currentPage: "home" | "library" | "settings";
  onNavigate: (page: "home" | "library" | "settings") => void;
  user: AuthUser | null;
  subscription: SubscriptionInfo | null;
  activeSession: ActiveSessionInfo | null;
  activeSessionGameTitle: string | null;
  isResumingSession: boolean;
  onResumeSession: () => void;
  onLogout: () => void;
}

type NavbarModalType = "time" | "storage" | null;

function getTierDisplay(tier: string): { label: string; className: string } {
  const t = tier.toUpperCase();
  if (t === "ULTIMATE") return { label: "Ultimate", className: "tier-ultimate" };
  if (t === "PRIORITY" || t === "PERFORMANCE") return { label: "Priority", className: "tier-priority" };
  return { label: "Free", className: "tier-free" };
}

export function Navbar({
  currentPage,
  onNavigate,
  user,
  subscription,
  activeSession,
  activeSessionGameTitle,
  isResumingSession,
  onResumeSession,
  onLogout,
}: NavbarProps): JSX.Element {
  const [modalType, setModalType] = useState<NavbarModalType>(null);

  const navItems = [
    { id: "home" as const, label: "Store", icon: House },
    { id: "library" as const, label: "Library", icon: Library },
    { id: "settings" as const, label: "Settings", icon: Settings },
  ];

  const tierInfo = user ? getTierDisplay(user.membershipTier) : null;
  const formatHours = (value: number): string => {
    if (!Number.isFinite(value)) return "0";
    const rounded = value >= 10 ? Math.round(value) : Math.round(value * 10) / 10;
    return Number.isInteger(rounded) ? String(rounded) : rounded.toFixed(1);
  };
  const formatGb = (value: number): string => {
    if (!Number.isFinite(value)) return "0";
    return Number.isInteger(value) ? String(value) : value.toFixed(1);
  };
  const formatPercent = (value: number): string => {
    if (!Number.isFinite(value)) return "0%";
    const rounded = Math.max(0, Math.min(100, Math.round(value)));
    return `${rounded}%`;
  };
  const formatDateTime = (value: string | undefined): string | null => {
    if (!value) return null;
    const parsed = new Date(value);
    if (Number.isNaN(parsed.getTime())) return null;
    return parsed.toLocaleString();
  };
  const clamp = (value: number): number => Math.min(1, Math.max(0, value));
  const toneByLeftRatio = (ratio: number): "good" | "warn" | "critical" => {
    if (ratio <= 0.15) return "critical";
    if (ratio <= 0.4) return "warn";
    return "good";
  };

  const timeTotal = subscription?.totalHours ?? 0;
  const timeLeft = subscription?.remainingHours ?? 0;
  const timeUsed = subscription?.usedHours ?? Math.max(timeTotal - timeLeft, 0);
  const allottedHours = subscription?.allottedHours ?? 0;
  const purchasedHours = subscription?.purchasedHours ?? 0;
  const rolledOverHours = subscription?.rolledOverHours ?? 0;
  const timeUsedRatio =
    subscription && !subscription.isUnlimited && timeTotal > 0 ? clamp(timeUsed / timeTotal) : 0;
  const timeLeftRatio =
    subscription && !subscription.isUnlimited && timeTotal > 0 ? clamp(timeLeft / timeTotal) : 1;
  const timeTone: "good" | "warn" | "critical" = subscription?.isUnlimited
    ? "good"
    : toneByLeftRatio(timeLeftRatio);
  const timeLabel = subscription
    ? subscription.isUnlimited
      ? "Unlimited time"
      : `${formatHours(timeLeft)}h left`
    : null;

  const storageTotal = subscription?.storageAddon?.sizeGb;
  const storageUsed = subscription?.storageAddon?.usedGb;
  const storageHasData = storageTotal !== undefined && storageUsed !== undefined;
  const storageLeft =
    storageHasData
      ? Math.max(storageTotal - storageUsed, 0)
      : undefined;
  const storageUsedRatio =
    storageHasData && storageTotal > 0 ? clamp(storageUsed / storageTotal) : 0;
  const storageLeftRatio =
    storageHasData && storageTotal > 0 ? clamp((storageLeft ?? 0) / storageTotal) : 1;
  const storageTone = toneByLeftRatio(storageLeftRatio);
  const storageLabel =
    storageHasData
      ? `${formatGb(storageLeft ?? 0)} GB left`
      : storageTotal !== undefined
        ? `${formatGb(storageTotal)} GB total`
        : null;

  const spanStart = formatDateTime(subscription?.currentSpanStartDateTime);
  const spanEnd = formatDateTime(subscription?.currentSpanEndDateTime);
  const firstEntitlementStart = formatDateTime(subscription?.firstEntitlementStartDateTime);
  const modalTitle = modalType === "time" ? "Playtime Details" : "Storage Details";
  const activeSessionTitle = activeSessionGameTitle?.trim() || null;

  useEffect(() => {
    if (!modalType) return;
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        setModalType(null);
      }
    };
    window.addEventListener("keydown", onKeyDown);
    const previousOverflow = document.body.style.overflow;
    document.body.style.overflow = "hidden";
    return () => {
      window.removeEventListener("keydown", onKeyDown);
      document.body.style.overflow = previousOverflow;
    };
  }, [modalType]);

  const modal = modalType && subscription
    ? createPortal(
        <div className="navbar-modal-backdrop" onClick={() => setModalType(null)}>
          <div
            className="navbar-modal"
            role="dialog"
            aria-modal="true"
            aria-label={modalTitle}
            onClick={(event) => event.stopPropagation()}
          >
            <div className="navbar-modal-header">
              <h3>{modalTitle}</h3>
              <button
                type="button"
                className="navbar-modal-close"
                onClick={() => setModalType(null)}
                title="Close"
              >
                <X size={16} />
              </button>
            </div>

            {modalType === "time" && (
              <div className="navbar-modal-body">
                {!subscription.isUnlimited && timeTotal > 0 && (
                  <div className="navbar-meter">
                    <div className="navbar-meter-head">
                      <span>Time Usage</span>
                      <strong>{formatPercent(timeUsedRatio * 100)} used</strong>
                    </div>
                    <div className="navbar-meter-track">
                      <span
                        className={`navbar-meter-fill navbar-meter-fill--${timeTone}`}
                        style={{ width: `${timeUsedRatio * 100}%` }}
                      />
                    </div>
                    <div className="navbar-meter-legend">
                      <span>{formatHours(timeUsed)}h used</span>
                      <span>{formatHours(timeLeft)}h left</span>
                    </div>
                  </div>
                )}
                <div className="navbar-modal-row"><span>Tier</span><strong>{subscription.membershipTier}</strong></div>
                {subscription.subscriptionType && (
                  <div className="navbar-modal-row"><span>Type</span><strong>{subscription.subscriptionType}</strong></div>
                )}
                {subscription.subscriptionSubType && (
                  <div className="navbar-modal-row"><span>Sub Type</span><strong>{subscription.subscriptionSubType}</strong></div>
                )}
                <div className="navbar-modal-row"><span>Time Left</span><strong>{subscription.isUnlimited ? "Unlimited" : `${formatHours(timeLeft)}h`}</strong></div>
                <div className="navbar-modal-row"><span>Total Time</span><strong>{subscription.isUnlimited ? "Unlimited" : `${formatHours(timeTotal)}h`}</strong></div>
                <div className="navbar-modal-row"><span>Used Time</span><strong>{formatHours(timeUsed)}h</strong></div>
                <div className="navbar-modal-row"><span>Allotted</span><strong>{formatHours(allottedHours)}h</strong></div>
                <div className="navbar-modal-row"><span>Purchased</span><strong>{formatHours(purchasedHours)}h</strong></div>
                <div className="navbar-modal-row"><span>Rolled Over</span><strong>{formatHours(rolledOverHours)}h</strong></div>
                {firstEntitlementStart && (
                  <div className="navbar-modal-row"><span>First Entitlement</span><strong>{firstEntitlementStart}</strong></div>
                )}
                {spanStart && <div className="navbar-modal-row"><span>Period Start</span><strong>{spanStart}</strong></div>}
                {spanEnd && <div className="navbar-modal-row"><span>Period End</span><strong>{spanEnd}</strong></div>}
                {subscription.notifyUserWhenTimeRemainingInMinutes !== undefined && (
                  <div className="navbar-modal-row"><span>Notify At (General)</span><strong>{subscription.notifyUserWhenTimeRemainingInMinutes} min</strong></div>
                )}
                {subscription.notifyUserOnSessionWhenRemainingTimeInMinutes !== undefined && (
                  <div className="navbar-modal-row"><span>Notify At (In Session)</span><strong>{subscription.notifyUserOnSessionWhenRemainingTimeInMinutes} min</strong></div>
                )}
                {subscription.state && <div className="navbar-modal-row"><span>Plan State</span><strong>{subscription.state}</strong></div>}
                {subscription.isGamePlayAllowed !== undefined && (
                  <div className="navbar-modal-row"><span>Gameplay Allowed</span><strong>{subscription.isGamePlayAllowed ? "Yes" : "No"}</strong></div>
                )}
              </div>
            )}

            {modalType === "storage" && (
              <div className="navbar-modal-body">
                {storageHasData && (
                  <div className="navbar-meter">
                    <div className="navbar-meter-head">
                      <span>Storage Usage</span>
                      <strong>{formatPercent(storageUsedRatio * 100)} used</strong>
                    </div>
                    <div className="navbar-meter-track">
                      <span
                        className={`navbar-meter-fill navbar-meter-fill--${storageTone}`}
                        style={{ width: `${storageUsedRatio * 100}%` }}
                      />
                    </div>
                    <div className="navbar-meter-legend">
                      <span>{formatGb(storageUsed ?? 0)} GB used</span>
                      <span>{formatGb(storageLeft ?? 0)} GB left</span>
                    </div>
                  </div>
                )}
                <div className="navbar-modal-row"><span>Storage Left</span><strong>{storageLeft !== undefined ? `${formatGb(storageLeft)} GB` : "N/A"}</strong></div>
                <div className="navbar-modal-row"><span>Storage Used</span><strong>{storageUsed !== undefined ? `${formatGb(storageUsed)} GB` : "N/A"}</strong></div>
                <div className="navbar-modal-row"><span>Storage Total</span><strong>{storageTotal !== undefined ? `${formatGb(storageTotal)} GB` : "N/A"}</strong></div>
                {subscription.storageAddon?.regionName && (
                  <div className="navbar-modal-row"><span>Storage Region</span><strong>{subscription.storageAddon.regionName}</strong></div>
                )}
                {subscription.storageAddon?.regionCode && (
                  <div className="navbar-modal-row"><span>Storage Region Code</span><strong>{subscription.storageAddon.regionCode}</strong></div>
                )}
                {subscription.serverRegionId && (
                  <div className="navbar-modal-row"><span>Server Region (VPC)</span><strong>{subscription.serverRegionId}</strong></div>
                )}
              </div>
            )}
          </div>
        </div>,
        document.body,
      )
    : null;

  return (
    <nav className="navbar">
      <div className="navbar-left">
        <div className="navbar-brand">
          <Zap size={16} strokeWidth={2.5} />
        </div>
        <span className="navbar-logo-text">OpenNOW</span>
      </div>

      <div className="navbar-nav">
        {navItems.map((item) => {
          const Icon = item.icon;
          const isActive = currentPage === item.id;

          return (
            <button
              key={item.id}
              onClick={() => onNavigate(item.id)}
              className={`navbar-link ${isActive ? "active" : ""}`}
            >
              <Icon size={16} />
              <span>{item.label}</span>
            </button>
          );
        })}
      </div>

      <div className="navbar-right">
        {activeSession && (
          <button
            type="button"
            className={`navbar-session-resume${isResumingSession ? " is-loading" : ""}`}
            title={
              activeSession.serverIp
                ? activeSessionTitle
                  ? `Resume active cloud session: ${activeSessionTitle}`
                  : "Resume active cloud session"
                : "Active session found (missing server address)"
            }
            onClick={onResumeSession}
            disabled={isResumingSession || !activeSession.serverIp}
          >
            {isResumingSession ? <Loader2 size={14} className="navbar-session-resume-spin" /> : <PlayCircle size={14} />}
            <span className="navbar-session-resume-text">Resume</span>
            {activeSessionTitle && <span className="navbar-session-resume-game">{activeSessionTitle}</span>}
          </button>
        )}
        {(timeLabel || storageLabel) && (
          <div className="navbar-subscription" aria-label="Subscription details">
            {timeLabel && (
              <button
                type="button"
                className={`navbar-subscription-chip navbar-subscription-chip--${timeTone}`}
                title="Show playtime details"
                onClick={() => setModalType("time")}
              >
                <Timer size={14} />
                <span>{timeLabel}</span>
              </button>
            )}
            {storageLabel && (
              <button
                type="button"
                className={`navbar-subscription-chip navbar-subscription-chip--${storageTone}`}
                title="Show storage details"
                onClick={() => setModalType("storage")}
              >
                <HardDrive size={14} />
                <span>{storageLabel}</span>
              </button>
            )}
          </div>
        )}
        {user ? (
          <>
            <div className="navbar-user">
              {user.avatarUrl ? (
                <img src={user.avatarUrl} alt={user.displayName} className="navbar-avatar" />
              ) : (
                <div className="navbar-avatar-fallback">
                  <User size={14} />
                </div>
              )}
              <div className="navbar-user-info">
                <span className="navbar-username">{user.displayName}</span>
                {tierInfo && (
                  <span className={`navbar-tier ${tierInfo.className}`}>{tierInfo.label}</span>
                )}
              </div>
            </div>
            <button onClick={onLogout} className="navbar-logout" title="Sign out">
              <LogOut size={16} />
            </button>
          </>
        ) : (
          <div className="navbar-guest">
            <User size={14} />
            <span>Guest</span>
          </div>
        )}
      </div>
      {modal}
    </nav>
  );
}
