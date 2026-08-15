import { Gauge } from "lucide-react";
import { useMemo, type JSX } from "react";
import type {
  EntitledResolution,
  GameInfo,
  GameStreamProfile,
  GameStreamProfiles,
  Settings,
} from "@shared/gfn";
import {
  expandEntitledStreamResolutions,
  resolveEntitledStreamProfile,
  resolveGameStreamProfile,
} from "@shared/gfn";
import { useTranslation } from "../i18n";
import {
  getFpsForResolution,
  groupResolutions,
  STATIC_FPS_PRESETS,
  STATIC_RESOLUTION_PRESETS,
} from "./settings/settingsFormatters";
import { SettingRange } from "./settings/SettingRange";
import { SelectDropdown, type SelectDropdownOption } from "./ui/SelectDropdown";

interface GameStreamProfileSectionProps {
  game: GameInfo;
  settings: Settings;
  entitledResolutions: EntitledResolution[];
  onPreview: (profiles: GameStreamProfiles) => void;
  onChange: (profiles: GameStreamProfiles) => void;
}

function replaceProfile(
  profiles: GameStreamProfiles,
  gameId: string,
  profile: GameStreamProfile | null,
): GameStreamProfiles {
  const next = { ...profiles };
  if (profile) {
    next[gameId] = profile;
  } else {
    delete next[gameId];
  }
  return next;
}

export function GameStreamProfileSection({
  game,
  settings,
  entitledResolutions,
  onPreview,
  onChange,
}: GameStreamProfileSectionProps): JSX.Element {
  const { t } = useTranslation();
  const savedProfile = settings.gameStreamProfiles[game.id];
  const hasOverride = Boolean(savedProfile);
  const fallbackProfile = resolveGameStreamProfile(settings);
  const expandedEntitlements = useMemo(
    () => expandEntitledStreamResolutions(entitledResolutions),
    [entitledResolutions],
  );
  const requestedProfile = savedProfile ?? fallbackProfile;
  const entitledProfile = expandedEntitlements.length > 0
    ? resolveEntitledStreamProfile(expandedEntitlements, requestedProfile)
    : null;
  const displayedProfile: GameStreamProfile = {
    ...requestedProfile,
    ...entitledProfile,
  };
  const resolutionOptions = useMemo<SelectDropdownOption[]>(() => {
    if (expandedEntitlements.length === 0) {
      return STATIC_RESOLUTION_PRESETS.map(({ value, label }) => ({ value, label }));
    }
    return groupResolutions(expandedEntitlements).flatMap((group) =>
      group.resolutions.map(({ value, label }) => ({ value, label, group: group.category })),
    );
  }, [expandedEntitlements]);
  const fpsOptions = expandedEntitlements.length > 0
    ? getFpsForResolution(expandedEntitlements, displayedProfile.resolution)
    : STATIC_FPS_PRESETS.map(({ value }) => value);

  const saveProfile = (profile: GameStreamProfile): void => {
    onChange(replaceProfile(settings.gameStreamProfiles, game.id, profile));
  };
  const previewProfile = (profile: GameStreamProfile): void => {
    onPreview(replaceProfile(settings.gameStreamProfiles, game.id, profile));
  };

  return (
    <section className="game-detail-section game-stream-profile" aria-labelledby="game-stream-profile-title">
      <div className="game-stream-profile-heading">
        <div className="game-stream-profile-title-wrap">
          <span className="game-stream-profile-icon" aria-hidden="true"><Gauge size={17} /></span>
          <div>
            <h3 id="game-stream-profile-title" className="game-detail-section-title">
              {t("gameDetails.streamProfile.title")}
            </h3>
            <p>{t("gameDetails.streamProfile.description")}</p>
          </div>
        </div>
        <span className={`game-stream-profile-badge${hasOverride ? " is-custom" : ""}`}>
          {hasOverride
            ? t("gameDetails.streamProfile.custom")
            : t("gameDetails.streamProfile.global")}
        </span>
      </div>

      <div className="game-stream-profile-toggle-row">
        <div>
          <strong>{t("gameDetails.streamProfile.useCustom")}</strong>
          <span>{t("gameDetails.streamProfile.useCustomHint")}</span>
        </div>
        <label className="settings-toggle">
          <input
            type="checkbox"
            checked={hasOverride}
            aria-label={t("gameDetails.streamProfile.useCustom")}
            onChange={(event) => {
              onChange(replaceProfile(
                settings.gameStreamProfiles,
                game.id,
                event.currentTarget.checked ? displayedProfile : null,
              ));
            }}
          />
          <span className="settings-toggle-track" />
        </label>
      </div>

      {hasOverride ? (
        <div className="game-stream-profile-controls">
          <label className="game-stream-profile-field" htmlFor="game-stream-profile-resolution">
            <span>{t("settings.video.resolution")}</span>
            <SelectDropdown
              id="game-stream-profile-resolution"
              value={displayedProfile.resolution}
              options={resolutionOptions}
              onChange={(resolution) => {
                const availableFps = expandedEntitlements.length > 0
                  ? getFpsForResolution(expandedEntitlements, resolution)
                  : STATIC_FPS_PRESETS.map(({ value }) => value);
                const fps = availableFps.includes(displayedProfile.fps)
                  ? displayedProfile.fps
                  : (availableFps.at(-1) ?? displayedProfile.fps);
                saveProfile({ ...displayedProfile, resolution, fps });
              }}
              menuClassName="select-dropdown__menu--grouped"
            />
          </label>

          <div className="game-stream-profile-field">
            <span>{t("settings.video.fps")}</span>
            <div className="settings-chip-row">
              {fpsOptions.map((fps) => (
                <button
                  key={fps}
                  type="button"
                  className={`settings-chip${displayedProfile.fps === fps ? " active" : ""}`}
                  aria-pressed={displayedProfile.fps === fps}
                  onClick={() => saveProfile({ ...displayedProfile, fps })}
                >
                  {fps}
                </button>
              ))}
            </div>
          </div>

          <div className="game-stream-profile-field game-stream-profile-bitrate">
            <div>
              <span>{t("settings.video.maxBitrate")}</span>
              <strong>{displayedProfile.maxBitrateMbps} Mbps</strong>
            </div>
            <SettingRange
              id="game-stream-profile-bitrate"
              className="settings-slider"
              min={5}
              max={150}
              step={5}
              value={displayedProfile.maxBitrateMbps}
              onPreview={(maxBitrateMbps) => previewProfile({ ...displayedProfile, maxBitrateMbps })}
              onCommit={(maxBitrateMbps) => saveProfile({ ...displayedProfile, maxBitrateMbps })}
            />
          </div>
        </div>
      ) : (
        <div className="game-stream-profile-summary">
          <span>{displayedProfile.resolution.replace("x", " × ")}</span>
          <i />
          <span>{displayedProfile.fps} FPS</span>
          <i />
          <span>{displayedProfile.maxBitrateMbps} Mbps</span>
        </div>
      )}
    </section>
  );
}
