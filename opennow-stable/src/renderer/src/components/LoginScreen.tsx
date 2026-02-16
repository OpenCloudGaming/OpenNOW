import { useState, useRef, useEffect } from "react";
import type { JSX } from "react";
import { LogIn, ChevronDown, Zap } from "lucide-react";
import type { LoginProvider } from "@shared/gfn";

export interface LoginScreenProps {
  providers: LoginProvider[];
  selectedProviderId: string;
  onProviderChange: (id: string) => void;
  onLogin: () => void;
  isLoading: boolean;
  error: string | null;
  isInitializing?: boolean;
  statusMessage?: string;
}

export function LoginScreen({
  providers,
  selectedProviderId,
  onProviderChange,
  onLogin,
  isLoading,
  error,
  isInitializing = false,
  statusMessage,
}: LoginScreenProps): JSX.Element {
  const [isDropdownOpen, setIsDropdownOpen] = useState(false);
  const dropdownRef = useRef<HTMLDivElement>(null);

  const selectedProvider = providers.find((p) => p.idpId === selectedProviderId);

  useEffect(() => {
    function handleClickOutside(event: MouseEvent) {
      if (dropdownRef.current && !dropdownRef.current.contains(event.target as Node)) {
        setIsDropdownOpen(false);
      }
    }
    document.addEventListener("mousedown", handleClickOutside);
    return () => document.removeEventListener("mousedown", handleClickOutside);
  }, []);

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
            <Zap size={20} strokeWidth={2.5} />
          </div>
          <span className="login-brand-name">OpenNOW</span>
        </div>

        {/* Card */}
        <div className="login-card">
          <div className="login-card-header">
            <h1>Sign in</h1>
            <p>Cloud gaming, open source.</p>
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
            <label className="login-label">Provider</label>
            <button
              className={`login-select ${isDropdownOpen ? "open" : ""}`}
              onClick={() => setIsDropdownOpen(!isDropdownOpen)}
              disabled={isLoading || isInitializing}
              type="button"
            >
              <span className="login-select-text">
                {isInitializing
                  ? "Loading..."
                  : selectedProvider?.displayName ?? "Select provider"}
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

          <button
            className={`login-button ${isLoading || isInitializing ? "loading" : ""}`}
            onClick={onLogin}
            disabled={isLoading || isInitializing || !selectedProviderId}
            type="button"
          >
            {isLoading || isInitializing ? (
              <>
                <span className="login-spinner" />
                <span>{isInitializing ? "Restoring Session..." : "Connecting..."}</span>
              </>
            ) : (
              <>
                <LogIn size={18} />
                <span>Sign In</span>
              </>
            )}
          </button>
        </div>

        <p className="login-footer">Open-source cloud gaming client</p>
      </div>
    </div>
  );
}
