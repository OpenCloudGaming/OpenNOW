import { useState, useRef, useEffect } from "react";
import type { JSX } from "react";
import QRCode from "qrcode";
import { LogIn, ChevronDown, QrCode } from "lucide-react";
import type { AuthDeviceLoginChallenge, LoginProvider } from "@shared/gfn";
import { useTranslation } from "../i18n";
import { OpenNowLogoMark } from "./OpenNowLogoMark";

export interface LoginScreenProps {
  providers: LoginProvider[];
  selectedProviderId: string;
  onProviderChange: (id: string) => void;
  onLogin: () => void;
  onQrLogin: () => void;
  onCancelQrLogin: () => void;
  isLoading: boolean;
  error: string | null;
  isInitializing?: boolean;
  statusMessage?: string;
  qrLoginChallenge?: AuthDeviceLoginChallenge | null;
  isQrLoginPending?: boolean;
}

export function LoginScreen({
  providers,
  selectedProviderId,
  onProviderChange,
  onLogin,
  onQrLogin,
  onCancelQrLogin,
  isLoading,
  error,
  isInitializing = false,
  statusMessage,
  qrLoginChallenge,
  isQrLoginPending = false,
}: LoginScreenProps): JSX.Element {
  const { t } = useTranslation();
  const [isDropdownOpen, setIsDropdownOpen] = useState(false);
  const [qrCodeDataUrl, setQrCodeDataUrl] = useState<string | null>(null);
  const dropdownRef = useRef<HTMLDivElement>(null);

  const selectedProvider = providers.find((p) => p.idpId === selectedProviderId);
  const title = isInitializing ? t("auth.title.restoringSession") : t("auth.title.signIn");
  const subtitle = isInitializing ? t("auth.subtitle.checkingSavedAccounts") : t("app.description");
  const isQrLoginActive = Boolean(qrLoginChallenge) || isQrLoginPending;

  useEffect(() => {
    function handleClickOutside(event: MouseEvent) {
      if (dropdownRef.current && !dropdownRef.current.contains(event.target as Node)) {
        setIsDropdownOpen(false);
      }
    }
    document.addEventListener("mousedown", handleClickOutside);
    return () => document.removeEventListener("mousedown", handleClickOutside);
  }, []);

  useEffect(() => {
    let cancelled = false;
    setQrCodeDataUrl(null);

    if (!qrLoginChallenge) {
      return () => {
        cancelled = true;
      };
    }

    QRCode.toDataURL(qrLoginChallenge.verificationUriComplete, {
      errorCorrectionLevel: "M",
      margin: 1,
      scale: 8,
      color: {
        dark: "#07111f",
        light: "#ffffff",
      },
    }).then((dataUrl) => {
      if (!cancelled) {
        setQrCodeDataUrl(dataUrl);
      }
    }).catch(() => {
      if (!cancelled) {
        setQrCodeDataUrl(null);
      }
    });

    return () => {
      cancelled = true;
    };
  }, [qrLoginChallenge]);

  const handleProviderSelect = (providerId: string) => {
    onProviderChange(providerId);
    setIsDropdownOpen(false);
  };

  return (
    <div className="login-screen">
      <div className="login-bg">
        <div className="login-bg-orb login-bg-orb--1" />
        <div className="login-bg-orb login-bg-orb--2" />
        <div className="login-bg-orb login-bg-orb--3" />
        <div className="login-bg-noise" />
      </div>

      <div className="login-content">
        {/* Brand */}
        <div className="login-brand">
          <div className="login-brand-mark">
            <OpenNowLogoMark className="opennow-logo-mark" />
          </div>
          <span className="login-brand-name">OpenNOW</span>
        </div>

        {/* Card */}
        <div className="login-card">
          <div className="login-card-header">
            <h1>{title}</h1>
            <p>{subtitle}</p>
          </div>

          {error && (
            <div className="login-error">
              <span className="login-error-dot" />
              {error}
            </div>
          )}

          {isInitializing && statusMessage && (
            <div className="login-status" role="status" aria-live="polite">
              <span className="login-status-dot" />
              {statusMessage}
            </div>
          )}

          <div className="login-field" ref={dropdownRef}>
            <label className="login-label">{t("auth.provider.label")}</label>
            <button
              className={`login-select ${isDropdownOpen ? "open" : ""}`}
              onClick={() => setIsDropdownOpen(!isDropdownOpen)}
              disabled={isLoading || isInitializing || isQrLoginActive}
              type="button"
            >
              <span className="login-select-text">
                {isInitializing
                  ? t("auth.provider.loading")
                  : selectedProvider?.displayName ?? t("auth.provider.select")}
              </span>
              <ChevronDown
                size={16}
                className={`login-select-chevron ${isDropdownOpen ? "rotated" : ""}`}
              />
            </button>

            {isDropdownOpen && (
              <div className="login-dropdown">
                {providers.map((provider) => (
                  <button
                    key={provider.idpId}
                    className={`login-dropdown-item ${provider.idpId === selectedProviderId ? "selected" : ""}`}
                    onClick={() => handleProviderSelect(provider.idpId)}
                    type="button"
                  >
                    <span>{provider.displayName}</span>
                    {provider.idpId === selectedProviderId && (
                      <svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor">
                        <path d="M13.78 4.22a.75.75 0 010 1.06l-7.25 7.25a.75.75 0 01-1.06 0L2.22 9.28a.75.75 0 011.06-1.06L6 10.94l6.72-6.72a.75.75 0 011.06 0z" />
                      </svg>
                    )}
                  </button>
                ))}
              </div>
            )}
          </div>

          {isQrLoginActive && (
            <div className="login-qr-panel" role="status" aria-live="polite">
              <div className="login-qr-code">
                {qrLoginChallenge && qrCodeDataUrl ? (
                  <img src={qrCodeDataUrl} alt={t("auth.qr.alt")} />
                ) : (
                  <span className="login-spinner" />
                )}
              </div>
              <div className="login-qr-copy">
                <div className="login-qr-title">
                  {qrLoginChallenge ? t("auth.qr.title") : t("auth.qr.preparing")}
                </div>
                <p>
                  {qrLoginChallenge ? t("auth.qr.description") : t("auth.qr.preparingDescription")}
                </p>
                {qrLoginChallenge && <code>{qrLoginChallenge.userCode}</code>}
              </div>
              <button
                className="login-secondary-button"
                onClick={onCancelQrLogin}
                type="button"
              >
                {t("auth.actions.cancelQrLogin")}
              </button>
            </div>
          )}

          <div className="login-actions">
            <button
              className={`login-button ${isLoading || isInitializing ? "loading" : ""}`}
              onClick={onLogin}
              disabled={isLoading || isInitializing || isQrLoginActive || !selectedProviderId}
              type="button"
            >
              {isLoading || isInitializing ? (
                <>
                  <span className="login-spinner" />
                  <span>{isInitializing ? t("auth.actions.restoringSession") : t("auth.actions.connecting")}</span>
                </>
              ) : (
                <>
                  <LogIn size={18} />
                  <span>{t("auth.actions.signIn")}</span>
                </>
              )}
            </button>
            <button
              className={`login-secondary-button ${isLoading && !isQrLoginActive ? "disabled" : ""}`}
              onClick={onQrLogin}
              disabled={isLoading || isInitializing || isQrLoginActive || !selectedProviderId}
              type="button"
            >
              <QrCode size={18} />
              <span>{t("auth.actions.signInWithQr")}</span>
            </button>
          </div>
        </div>

        <p className="login-footer">{t("app.tagline")}</p>
      </div>
    </div>
  );
}
