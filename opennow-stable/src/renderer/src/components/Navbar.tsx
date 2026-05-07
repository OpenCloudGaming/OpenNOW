import type { ActiveSessionInfo, AuthUser, SavedAccount, SubscriptionInfo } from "@shared/gfn";
import { House, Library, Settings, User, Zap, Timer, HardDrive, X, Loader2, PlayCircle, Square, ChevronDown, Check, Plus } from "lucide-react";
import { useEffect, useRef, useState, type JSX } from "react";
import { createPortal } from "react-dom";
import { useTranslation } from "../i18n";

interface NavbarProps {
  currentPage: "home" | "library" | "settings";
  onNavigate: (page: "home" | "library" | "settings") => void;
  user: AuthUser | null;
  subscription: SubscriptionInfo | null;
  activeSession: ActiveSessionInfo | null;
  activeSessionGameTitle: string | null;
  isResumingSession: boolean;
  isTerminatingSession: boolean;
  onResumeSession: () => void;
  onTerminateSession: () => void;
  savedAccounts: SavedAccount[];
  onSwitchAccount: (userId: string) => void;
  onRemoveAccount: (userId: string) => void;
  onAddAccount: () => void;
  onLogoutAll: () => void;
}

type NavbarModalType = "time" | "storage" | null;

function getTierDisplay(tier: string): { labelKey: string; className: string } {
  const t = tier.toUpperCase();
  if (t === "ULTIMATE") return { labelKey: "app.labels.ultimate", className: "tier-ultimate" };
  if (t === "PRIORITY" || t === "PERFORMANCE") return { labelKey: "app.labels.priority", className: "tier-priority" };
  return { labelKey: "app.labels.free", className: "tier-free" };
}

export function Navbar({
  currentPage,
  onNavigate,
  user,
  subscription,
  activeSession,
  activeSessionGameTitle,
  isResumingSession,
  isTerminatingSession,
  onResumeSession,
  onTerminateSession,
  savedAccounts,
  onSwitchAccount,
  onRemoveAccount,
  onAddAccount,
  onLogoutAll,
}: NavbarProps): JSX.Element {
  const { t } = useTranslation();
  const [modalType, setModalType] = useState<NavbarModalType>(null);
  const [accountDropdownOpen, setAccountDropdownOpen] = useState(false);
  const accountContainerRef = useRef<HTMLDivElement | null>(null);

  const navItems = [
    { id: "home" as const, label: t("navigation.home"), icon: House },
    { id: "library" as const, label: t("navigation.library"), icon: Library },
    { id: "settings" as const, label: t("navigation.settings"), icon: Settings },
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
    return t("app.units.percent", { value: rounded });
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
      ? t("navbar.time.unlimitedTime")
      : t("app.units.hoursLeft", { value: formatHours(timeLeft) })
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
      ? t("app.units.gbLeft", { value: formatGb(storageLeft ?? 0) })
      : storageTotal !== undefined
        ? t("app.units.gbTotal", { value: formatGb(storageTotal) })
        : null;

  const spanStart = formatDateTime(subscription?.currentSpanStartDateTime);
  const spanEnd = formatDateTime(subscription?.currentSpanEndDateTime);
  const firstEntitlementStart = formatDateTime(subscription?.firstEntitlementStartDateTime);
  const modalTitle = modalType === "time" ? t("navbar.playtimeDetails") : t("navbar.storageDetails");
  const activeSessionTitle = activeSessionGameTitle?.trim() || null;
  const activeUserId = user?.userId ?? null;

  useEffect(() => {
    if (!accountDropdownOpen) return;
    const onDocumentPointerDown = (event: MouseEvent) => {
      if (!(event.target instanceof Node) || !accountContainerRef.current?.contains(event.target)) {
        setAccountDropdownOpen(false);
      }
    };
    window.addEventListener("mousedown", onDocumentPointerDown);
    return () => window.removeEventListener("mousedown", onDocumentPointerDown);
  }, [accountDropdownOpen]);

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
                title={t("app.actions.close")}
              >
                <X size={16} />
              </button>
            </div>

            {modalType === "time" && (
              <div className="navbar-modal-body">
                {!subscription.isUnlimited && timeTotal > 0 && (
                  <div className="navbar-meter">
                    <div className="navbar-meter-head">
                      <span>{t("navbar.time.timeUsage")}</span>
                      <strong>{t("navbar.time.used", { percent: formatPercent(timeUsedRatio * 100) })}</strong>
                    </div>
                    <div className="navbar-meter-track">
                      <span
                        className={`navbar-meter-fill navbar-meter-fill--${timeTone}`}
                        style={{ width: `${timeUsedRatio * 100}%` }}
                      />
                    </div>
                    <div className="navbar-meter-legend">
                      <span>{t("app.units.hoursUsed", { value: formatHours(timeUsed) })}</span>
                      <span>{t("app.units.hoursLeft", { value: formatHours(timeLeft) })}</span>
                    </div>
                  </div>
                )}
                <div className="navbar-modal-row"><span>{t("navbar.time.tier")}</span><strong>{subscription.membershipTier}</strong></div>
                {subscription.subscriptionType && (
                  <div className="navbar-modal-row"><span>{t("navbar.time.type")}</span><strong>{subscription.subscriptionType}</strong></div>
                )}
                {subscription.subscriptionSubType && (
                  <div className="navbar-modal-row"><span>{t("navbar.time.subType")}</span><strong>{subscription.subscriptionSubType}</strong></div>
                )}
                <div className="navbar-modal-row"><span>{t("navbar.time.timeLeft")}</span><strong>{subscription.isUnlimited ? t("app.labels.unlimited") : t("app.units.hours", { value: formatHours(timeLeft) })}</strong></div>
                <div className="navbar-modal-row"><span>{t("navbar.time.totalTime")}</span><strong>{subscription.isUnlimited ? t("app.labels.unlimited") : t("app.units.hours", { value: formatHours(timeTotal) })}</strong></div>
                <div className="navbar-modal-row"><span>{t("navbar.time.usedTime")}</span><strong>{t("app.units.hours", { value: formatHours(timeUsed) })}</strong></div>
                <div className="navbar-modal-row"><span>{t("navbar.time.allotted")}</span><strong>{t("app.units.hours", { value: formatHours(allottedHours) })}</strong></div>
                <div className="navbar-modal-row"><span>{t("navbar.time.purchased")}</span><strong>{t("app.units.hours", { value: formatHours(purchasedHours) })}</strong></div>
                <div className="navbar-modal-row"><span>{t("navbar.time.rolledOver")}</span><strong>{t("app.units.hours", { value: formatHours(rolledOverHours) })}</strong></div>
                {firstEntitlementStart && (
                  <div className="navbar-modal-row"><span>{t("navbar.time.firstEntitlement")}</span><strong>{firstEntitlementStart}</strong></div>
                )}
                {spanStart && <div className="navbar-modal-row"><span>{t("navbar.time.periodStart")}</span><strong>{spanStart}</strong></div>}
                {spanEnd && <div className="navbar-modal-row"><span>{t("navbar.time.periodEnd")}</span><strong>{spanEnd}</strong></div>}
                {subscription.notifyUserWhenTimeRemainingInMinutes !== undefined && (
                  <div className="navbar-modal-row"><span>{t("navbar.time.notifyAtGeneral")}</span><strong>{t("app.units.minutes", { value: subscription.notifyUserWhenTimeRemainingInMinutes })}</strong></div>
                )}
                {subscription.notifyUserOnSessionWhenRemainingTimeInMinutes !== undefined && (
                  <div className="navbar-modal-row"><span>{t("navbar.time.notifyAtInSession")}</span><strong>{t("app.units.minutes", { value: subscription.notifyUserOnSessionWhenRemainingTimeInMinutes })}</strong></div>
                )}
                {subscription.state && <div className="navbar-modal-row"><span>{t("navbar.time.planState")}</span><strong>{subscription.state}</strong></div>}
                {subscription.isGamePlayAllowed !== undefined && (
                  <div className="navbar-modal-row"><span>{t("navbar.time.gameplayAllowed")}</span><strong>{subscription.isGamePlayAllowed ? t("navbar.time.yes") : t("navbar.time.no")}</strong></div>
                )}
              </div>
            )}

            {modalType === "storage" && (
              <div className="navbar-modal-body">
                {storageHasData && (
                  <div className="navbar-meter">
                    <div className="navbar-meter-head">
                      <span>{t("navbar.storage.storageUsage")}</span>
                      <strong>{t("navbar.time.used", { percent: formatPercent(storageUsedRatio * 100) })}</strong>
                    </div>
                    <div className="navbar-meter-track">
                      <span
                        className={`navbar-meter-fill navbar-meter-fill--${storageTone}`}
                        style={{ width: `${storageUsedRatio * 100}%` }}
                      />
                    </div>
                    <div className="navbar-meter-legend">
                      <span>{t("app.units.gbUsed", { value: formatGb(storageUsed ?? 0) })}</span>
                      <span>{t("app.units.gbLeft", { value: formatGb(storageLeft ?? 0) })}</span>
                    </div>
                  </div>
                )}
                <div className="navbar-modal-row"><span>{t("navbar.storage.storageLeft")}</span><strong>{storageLeft !== undefined ? t("app.units.gb", { value: formatGb(storageLeft) }) : t("navbar.storage.notAvailable")}</strong></div>
                <div className="navbar-modal-row"><span>{t("navbar.storage.storageUsed")}</span><strong>{storageUsed !== undefined ? t("app.units.gb", { value: formatGb(storageUsed) }) : t("navbar.storage.notAvailable")}</strong></div>
                <div className="navbar-modal-row"><span>{t("navbar.storage.storageTotal")}</span><strong>{storageTotal !== undefined ? t("app.units.gb", { value: formatGb(storageTotal) }) : t("navbar.storage.notAvailable")}</strong></div>
                {subscription.storageAddon?.regionName && (
                  <div className="navbar-modal-row"><span>{t("navbar.storage.storageRegion")}</span><strong>{subscription.storageAddon.regionName}</strong></div>
                )}
                {subscription.storageAddon?.regionCode && (
                  <div className="navbar-modal-row"><span>{t("navbar.storage.storageRegionCode")}</span><strong>{subscription.storageAddon.regionCode}</strong></div>
                )}
                {subscription.serverRegionId && (
                  <div className="navbar-modal-row"><span>{t("navbar.storage.serverRegionVpc")}</span><strong>{subscription.serverRegionId}</strong></div>
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
          <div className="navbar-session-actions">
            <button
              type="button"
              className={`navbar-session-resume${isResumingSession ? " is-loading" : ""}`}
              title={
                activeSession.serverIp
                  ? activeSessionTitle
                    ? t("session.resumeActiveCloudSessionTitle", { title: activeSessionTitle })
                    : t("session.resumeActiveCloudSession")
                  : t("session.activeSessionMissingServerAddress")
              }
              onClick={onResumeSession}
              disabled={isResumingSession || isTerminatingSession || !activeSession.serverIp}
            >
              {isResumingSession ? <Loader2 size={14} className="navbar-session-resume-spin" /> : <PlayCircle size={14} />}
              <span className="navbar-session-resume-text">{t("app.actions.resume")}</span>
              {activeSessionTitle && <span className="navbar-session-resume-game">{activeSessionTitle}</span>}
            </button>
            <button
              type="button"
              className={`navbar-session-terminate${isTerminatingSession ? " is-loading" : ""}`}
              title={
                activeSessionTitle
                  ? t("session.terminateActiveCloudSessionTitle", { title: activeSessionTitle })
                  : t("session.terminateActiveCloudSession")
              }
              onClick={onTerminateSession}
              disabled={isResumingSession || isTerminatingSession}
            >
              {isTerminatingSession ? <Loader2 size={14} className="navbar-session-resume-spin" /> : <Square size={12} />}
              <span className="navbar-session-terminate-text">{t("session.terminate")}</span>
            </button>
          </div>
        )}
        {(timeLabel || storageLabel) && (
          <div className="navbar-subscription" aria-label={t("navbar.subscriptionDetails")}>
            {timeLabel && (
              <button
                type="button"
                className={`navbar-subscription-chip navbar-subscription-chip--${timeTone}`}
                title={t("navbar.showPlaytimeDetails")}
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
                title={t("navbar.showStorageDetails")}
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
            <div className="navbar-account-container" ref={accountContainerRef}>
              <button
                type="button"
                className="navbar-user navbar-user--clickable"
                onClick={() => setAccountDropdownOpen((previous) => !previous)}
                aria-haspopup="menu"
                aria-expanded={accountDropdownOpen}
                aria-controls="navbar-account-dropdown"
              >
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
                    <span className={`navbar-tier ${tierInfo.className}`}>{t(tierInfo.labelKey)}</span>
                  )}
                </div>
                <ChevronDown
                  size={14}
                  className={`navbar-user-chevron${accountDropdownOpen ? " is-open" : ""}`}
                />
              </button>
              {accountDropdownOpen && (
                <div
                  id="navbar-account-dropdown"
                  className="navbar-account-dropdown"
                  role="menu"
                  aria-labelledby="navbar-account-dropdown-header"
                >
                  <div id="navbar-account-dropdown-header" className="navbar-account-dropdown-header">
                    {t("auth.accounts.switchAccount")}
                  </div>
                  <ul className="navbar-account-list">
                    {savedAccounts.map((account) => {
                      const accountTierInfo = getTierDisplay(account.membershipTier);
                      const isActive = activeUserId === account.userId;
                      const canRemove = !isActive && savedAccounts.length > 1;
                      return (
                        <li
                          key={account.userId}
                          role="none"
                          className={`navbar-account-item${isActive ? " navbar-account-item--active" : ""}`}
                        >
                          <button
                            type="button"
                            className="navbar-account-item-main"
                            role="menuitem"
                            onClick={() => {
                              if (!isActive) {
                                void onSwitchAccount(account.userId);
                              }
                              setAccountDropdownOpen(false);
                            }}
                            disabled={isActive}
                          >
                            {account.avatarUrl ? (
                              <img
                                src={account.avatarUrl}
                                alt={account.displayName}
                                className="navbar-account-item-avatar"
                              />
                            ) : (
                              <div className="navbar-avatar-fallback navbar-account-item-avatar">
                                <User size={12} />
                              </div>
                            )}
                            <div className="navbar-account-item-info">
                              <span className="navbar-account-item-name">{account.displayName}</span>
                              {account.email && <span className="navbar-account-item-email">{account.email}</span>}
                            </div>
                            <div className="navbar-account-item-right">
                                <span className={`navbar-account-item-tier ${accountTierInfo.className}`}>
                                  {t(accountTierInfo.labelKey)}
                                </span>
                                {isActive && (
                                <span className="navbar-account-item-check" aria-label={t("auth.accounts.activeAccount")}>
                                  <Check size={14} />
                                </span>
                              )}
                            </div>
                          </button>
                          {canRemove && (
                            <button
                              type="button"
                              className="navbar-account-remove"
                              role="menuitem"
                              aria-label={t("auth.accounts.removeNamedAccount", { name: account.displayName })}
                              onClick={() => {
                                setAccountDropdownOpen(false);
                                onRemoveAccount(account.userId);
                              }}
                            >
                              <X size={12} />
                            </button>
                          )}
                        </li>
                      );
                    })}
                  </ul>
                  <div className="navbar-account-divider" aria-hidden="true" />
                  <button
                    type="button"
                    className="navbar-account-add"
                    role="menuitem"
                    onClick={() => {
                      onAddAccount();
                      setAccountDropdownOpen(false);
                    }}
                  >
                    <Plus size={14} />
                    <span>{t("auth.accounts.addAccount")}</span>
                  </button>
                  <button
                    type="button"
                    className="navbar-account-signout-all"
                    role="menuitem"
                    onClick={() => {
                      setAccountDropdownOpen(false);
                      onLogoutAll();
                    }}
                  >
                    {t("auth.accounts.signOutAllAccounts")}
                  </button>
                </div>
              )}
            </div>
          </>
        ) : (
          <div className="navbar-guest">
            <User size={14} />
            <span>{t("auth.accounts.guest")}</span>
          </div>
        )}
      </div>
      {modal}
    </nav>
  );
}
