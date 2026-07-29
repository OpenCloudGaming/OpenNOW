import { useCallback, useMemo, type JSX } from "react";
import type { AppAccentColor, Settings } from "@shared/gfn";
import { useTranslation } from "../../../i18n";
import { SelectDropdown } from "../../ui/SelectDropdown";
import { SettingRange } from "../SettingRange";
import {
  accentColorOptions,
  getAppLanguageLabel,
  POSTER_SIZE_MAX,
  POSTER_SIZE_MIN,
  POSTER_SIZE_STEP,
} from "../settingsFormatters";

export interface SettingsInterfaceSectionProps {
  settings: Settings;
  showAll: boolean;
  handleChange: <K extends keyof Settings>(key: K, value: Settings[K]) => void;
  handlePreview: <K extends keyof Settings>(key: K, value: Settings[K]) => void;
  onSaved: () => void;
}

export function SettingsInterfaceSection({ settings, showAll, handleChange, handlePreview, onSaved }: SettingsInterfaceSectionProps): JSX.Element {
  const { locale, availableLocales, setLocale, t } = useTranslation();
  const posterSizePercent = Math.round(settings.posterSizeScale * 100);

  const appLanguageOptions = useMemo(
    () => availableLocales.map((value) => ({ value, label: getAppLanguageLabel(value) })),
    [availableLocales],
  );

  const accentDropdownOptions = useMemo(
    () => accentColorOptions.map((option) => ({
      value: option.value,
      label: (
        <span className="settings-accent-option">
          <span
            className="settings-accent-swatch"
            aria-hidden="true"
            style={{
              background: option.hex,
              boxShadow: `0 0 0 1px color-mix(in srgb, ${option.hex} 48%, rgba(255, 255, 255, 0.22))`,
            }}
          />
          <span>{t(option.labelKey)}</span>
        </span>
      ),
    })),
    [locale, t],
  );

  const handleAppLanguageChange = useCallback((nextLocale: string): void => {
    void setLocale(nextLocale)
      .then(onSaved)
      .catch((error) => {
        console.warn("[Settings] Failed to change app language:", error);
      });
  }, [onSaved, setLocale]);

  return (
    <>
      {/* ── Appearance ── */}
      <section className="settings-section">
        {showAll && <div className="settings-section-context">{t("settings.sections.interface")}</div>}
        <div className="settings-section-header">
          <h2>{t("settings.interface.appearance")}</h2>
        </div>
        <div className="settings-rows">
          <div className="settings-row">
            <label className="settings-label" htmlFor="settings-interface-app-language">
              {t("settings.interface.appLanguage")}
              <span className="settings-hint">{t("settings.interface.appLanguageHint")}</span>
            </label>
            <div className="settings-row-control">
              <SelectDropdown
                id="settings-interface-app-language"
                value={locale}
                options={appLanguageOptions}
                onChange={handleAppLanguageChange}
              />
            </div>
          </div>

          <div className="settings-row">
            <label className="settings-label" htmlFor="appTheme">
              {t("settings.interface.theme") || "Theme"}
              <span className="settings-hint">{t("settings.interface.themeHint") || "Choose a light, dark, or system-matching theme."}</span>
            </label>
            <div className="settings-row-control">
              <SelectDropdown
                id="appTheme"
                value={settings.appTheme}
                options={[
                  { value: "auto", label: t("settings.interface.themeAuto") || "Auto" },
                  { value: "light", label: t("settings.interface.themeLight") || "Light" },
                  { value: "dark", label: t("settings.interface.themeDark") || "Dark" },
                ]}
                onChange={(value) => handleChange("appTheme", value as any)}
                ariaLabel={t("settings.interface.theme") || "Theme"}
              />
            </div>
          </div>

          <div className="settings-row settings-row--column">
            <div className="settings-row-top settings-row-top--compact">
              <label className="settings-label settings-label--wrap" htmlFor="settings-interface-translucent-ui">
                <span className="settings-label-title">{t("settings.interface.translucentUI") || "Translucent UI"}</span>
              </label>
              <label className="settings-toggle">
                <input
                  id="settings-interface-translucent-ui"
                  type="checkbox"
                  checked={settings.translucentUI}
                  onChange={(e) => handleChange("translucentUI", e.target.checked)}
                />
                <span className="settings-toggle-track" />
              </label>
            </div>
            <span className="settings-subtle-hint">{t("settings.interface.translucentUIHint") || "Enable glassmorphism and translucent overlays."}</span>
          </div>

          <div className="settings-row">
            <label className="settings-label" htmlFor="settings-interface-accent-color">
              {t("settings.interface.accentColor")}
              <span className="settings-hint">{t("settings.interface.accentColorHint")}</span>
            </label>
            <div className="settings-row-control">
              <SelectDropdown
                id="settings-interface-accent-color"
                value={settings.appAccentColor}
                options={accentDropdownOptions}
                onChange={(value) => handleChange("appAccentColor", value as AppAccentColor)}
              />
            </div>
          </div>

          {/* Appearance toggles */}
          <div className="settings-row settings-row--column">
            <div className="settings-row-top settings-row-top--compact">
              <label className="settings-label settings-label--wrap" htmlFor="settings-interface-hide-stream-buttons">
                <span className="settings-label-title">{t("settings.interface.hideStreamOverlayButtons")}</span>
              </label>
              <label className="settings-toggle">
                <input
                  id="settings-interface-hide-stream-buttons"
                  type="checkbox"
                  checked={settings.hideStreamButtons}
                  onChange={(e) => handleChange("hideStreamButtons", e.target.checked)}
                />
                <span className="settings-toggle-track" />
              </label>
            </div>
            <span className="settings-subtle-hint">{t("settings.interface.hideStreamOverlayButtonsHint")}</span>
          </div>

          <div className="settings-row settings-row--column">
            <div className="settings-row-top settings-row-top--compact">
              <label className="settings-label settings-label--wrap" htmlFor="settings-interface-show-stats-on-launch">
                <span className="settings-label-title">{t("settings.interface.showStatsOnStreamLaunch")}</span>
              </label>
              <label className="settings-toggle">
                <input
                  id="settings-interface-show-stats-on-launch"
                  type="checkbox"
                  checked={settings.showStatsOnLaunch}
                  onChange={(e) => handleChange("showStatsOnLaunch", e.target.checked)}
                />
                <span className="settings-toggle-track" />
              </label>
            </div>
            <span className="settings-subtle-hint">{t("settings.interface.showStatsOnStreamLaunchHint")}</span>
          </div>

          <div className="settings-row settings-row--column">
            <div className="settings-row-top settings-row-top--compact">
              <label className="settings-label settings-label--wrap" htmlFor="settings-interface-hide-server-selector">
                <span className="settings-label-title">{t("settings.interface.hideServerSelector")}</span>
              </label>
              <label className="settings-toggle">
                <input
                  id="settings-interface-hide-server-selector"
                  type="checkbox"
                  checked={settings.hideServerSelector}
                  onChange={(e) => handleChange("hideServerSelector", e.target.checked)}
                />
                <span className="settings-toggle-track" />
              </label>
            </div>
            <span className="settings-subtle-hint">{t("settings.interface.hideServerSelectorHint")}</span>
          </div>

          <div className="settings-row settings-row--column">
            <div className="settings-row-top settings-row-top--compact">
              <label className="settings-label settings-label--wrap" htmlFor="settings-interface-show-anti-afk-indicator">
                <span className="settings-label-title">{t("settings.interface.showAntiAfkIndicator")}</span>
              </label>
              <label className="settings-toggle">
                <input
                  id="settings-interface-show-anti-afk-indicator"
                  type="checkbox"
                  checked={settings.showAntiAfkIndicator}
                  onChange={(e) => handleChange("showAntiAfkIndicator", e.target.checked)}
                />
                <span className="settings-toggle-track" />
              </label>
            </div>
            <span className="settings-subtle-hint">{t("settings.interface.showAntiAfkIndicatorHint")}</span>
          </div>

          <div className="settings-row settings-row--column">
            <div className="settings-row-top settings-row-top--compact">
              <label className="settings-label settings-label--wrap" htmlFor="settings-interface-auto-fullscreen">
                <span className="settings-label-title">{t("settings.interface.autoFullScreen")}</span>
              </label>
              <label className="settings-toggle">
                <input
                  id="settings-interface-auto-fullscreen"
                  type="checkbox"
                  checked={settings.autoFullScreen}
                  onChange={(e) => handleChange("autoFullScreen", e.target.checked)}
                />
                <span className="settings-toggle-track" />
              </label>
            </div>
            <span className="settings-subtle-hint">{t("settings.interface.autoFullScreenHint")}</span>
          </div>

          <div className="settings-row settings-row--column">
            <div className="settings-row-top settings-row-top--compact">
              <label className="settings-label settings-label--wrap" htmlFor="settings-interface-controller-mode">
                <span className="settings-label-title">{t("settings.interface.controllerMode")}</span>
              </label>
              <label className="settings-toggle">
                <input
                  id="settings-interface-controller-mode"
                  type="checkbox"
                  checked={settings.controllerMode}
                  onChange={(e) => handleChange("controllerMode", e.target.checked)}
                />
                <span className="settings-toggle-track" />
              </label>
            </div>
            <span className="settings-subtle-hint">{t("settings.interface.controllerModeHint")}</span>
          </div>

          <div className="settings-row settings-row--column">
            <div className="settings-row-top settings-row-top--compact">
              <label className="settings-label settings-label--wrap" htmlFor="settings-interface-console-profile-picker">
                <span className="settings-label-title">{t("settings.interface.consoleProfilePicker")}</span>
              </label>
              <label className="settings-toggle">
                <input
                  id="settings-interface-console-profile-picker"
                  type="checkbox"
                  checked={settings.consoleProfilePickerOnLaunch}
                  onChange={(e) => handleChange("consoleProfilePickerOnLaunch", e.target.checked)}
                />
                <span className="settings-toggle-track" />
              </label>
            </div>
            <span className="settings-subtle-hint">{t("settings.interface.consoleProfilePickerHint")}</span>
          </div>

          <div className="settings-row settings-row--column">
            <div className="settings-row-top settings-row-top--compact">
              <label className="settings-label settings-label--wrap" htmlFor="settings-interface-escape-exits-fullscreen">
                <span className="settings-label-title">{t("settings.interface.escapeExitsFullscreen")}</span>
              </label>
              <label className="settings-toggle">
                <input
                  id="settings-interface-escape-exits-fullscreen"
                  type="checkbox"
                  checked={Boolean(settings.allowEscapeToExitFullscreen)}
                  onChange={(e) => handleChange("allowEscapeToExitFullscreen", e.target.checked)}
                />
                <span className="settings-toggle-track" />
              </label>
            </div>
            <span className="settings-subtle-hint">{t("settings.interface.escapeExitsFullscreenHint")}</span>
          </div>

          <div className="settings-row settings-row--column">
            <div className="settings-row-top settings-row-top--compact">
              <label className="settings-label settings-label--wrap" htmlFor="settings-interface-discord-rich-presence">
                <span className="settings-label-title">{t("settings.interface.discordRichPresence")}</span>
              </label>
              <label className="settings-toggle">
                <input
                  id="settings-interface-discord-rich-presence"
                  type="checkbox"
                  checked={settings.discordRichPresence}
                  onChange={(e) => handleChange("discordRichPresence", e.target.checked)}
                />
                <span className="settings-toggle-track" />
              </label>
            </div>
            <span className="settings-subtle-hint">{t("settings.interface.discordRichPresenceHint")}</span>
          </div>

          <div className="settings-row settings-row--column">
            <div className="settings-row-top">
              <label className="settings-label" htmlFor="settings-interface-poster-size">{t("settings.interface.posterSize")}</label>
              <span className="settings-value-badge">{posterSizePercent}%</span>
            </div>
            <SettingRange
              id="settings-interface-poster-size"
              className="settings-slider"
              min={POSTER_SIZE_MIN}
              max={POSTER_SIZE_MAX}
              step={POSTER_SIZE_STEP}
              value={posterSizePercent}
              normalize={(value) => value / 100}
              onPreview={(value) => handlePreview("posterSizeScale", value)}
              onCommit={(value) => handleChange("posterSizeScale", value)}
            />
            <span className="settings-subtle-hint">{t("settings.interface.posterSizeHint")}</span>
          </div>

          <div className="settings-row settings-row--column">
            <div className="settings-row-top settings-row-top--compact">
              <label className="settings-label settings-label--wrap" htmlFor="settings-interface-show-session-time-remaining">
                <span className="settings-label-title">{t("settings.interface.showSessionTimeRemainingInStatsOverlay")}</span>
              </label>
              <label className="settings-toggle">
                <input
                  id="settings-interface-show-session-time-remaining"
                  type="checkbox"
                  checked={settings.showSessionTimeRemainingInStatsOverlay}
                  onChange={(e) => handleChange("showSessionTimeRemainingInStatsOverlay", e.target.checked)}
                />
                <span className="settings-toggle-track" />
              </label>
            </div>
            <span className="settings-subtle-hint">{t("settings.interface.showSessionTimeRemainingInStatsOverlayHint")}</span>
          </div>

          {/* Session Counter */}
          <div className="settings-row settings-row--column">
            <div className="settings-row-top settings-row-top--compact">
              <label className="settings-label settings-label--wrap" htmlFor="settings-interface-session-counter">
                <span className="settings-label-title">{t("settings.interface.sessionElapsedCounter")}</span>
              </label>
              <label className="settings-toggle">
                <input
                  id="settings-interface-session-counter"
                  type="checkbox"
                  checked={settings.sessionCounterEnabled}
                  onChange={(e) => handleChange("sessionCounterEnabled", e.target.checked)}
                />
                <span className="settings-toggle-track" />
              </label>
            </div>
            <span className="settings-subtle-hint">{t("settings.interface.sessionElapsedCounterHint")}</span>
          </div>

          {settings.sessionCounterEnabled && (
            <>
              <div className="settings-row settings-row--column">
                <div className="settings-row-top">
                  <label className="settings-label" htmlFor="settings-interface-session-timer-reappear">{t("settings.interface.sessionTimerReappear")}</label>
                  <span className="settings-value-badge">
                    {settings.sessionClockShowEveryMinutes === 0
                      ? t("settings.interface.off")
                      : t("settings.interface.everyMinutes", { count: settings.sessionClockShowEveryMinutes })}
                  </span>
                </div>
                <SettingRange
                  id="settings-interface-session-timer-reappear"
                  className="settings-slider"
                  min={0}
                  max={120}
                  step={5}
                  value={settings.sessionClockShowEveryMinutes}
                  onPreview={(value) => handlePreview("sessionClockShowEveryMinutes", value)}
                  onCommit={(value) => handleChange("sessionClockShowEveryMinutes", value)}
                />
                <span className="settings-subtle-hint">{t("settings.interface.sessionTimerReappearHint")}</span>
              </div>

              <div className="settings-row settings-row--column">
                <div className="settings-row-top">
                  <label className="settings-label" htmlFor="settings-interface-session-timer-visible-time">{t("settings.interface.sessionTimerVisibleTime")}</label>
                  <span className="settings-value-badge">
                    {t("app.units.seconds", { value: settings.sessionClockShowDurationSeconds })}
                  </span>
                </div>
                <SettingRange
                  id="settings-interface-session-timer-visible-time"
                  className="settings-slider"
                  min={5}
                  max={120}
                  step={5}
                  value={settings.sessionClockShowDurationSeconds}
                  onPreview={(value) => handlePreview("sessionClockShowDurationSeconds", value)}
                  onCommit={(value) => handleChange("sessionClockShowDurationSeconds", value)}
                />
                <span className="settings-subtle-hint">{t("settings.interface.sessionTimerVisibleTimeHint")}</span>
              </div>
            </>
          )}
        </div>
      </section>

    </>
  );
}
