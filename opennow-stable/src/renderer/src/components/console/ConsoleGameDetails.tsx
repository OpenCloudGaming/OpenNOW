import { useEffect, useState } from "react";
import type { JSX } from "react";
import type { GameInfo } from "@shared/gfn";
import {
  getControllerHeroBackgroundCandidates,
  getGameLogoUrl,
  getGameStoreSummary,
  getPlayerSummary,
  getPrimaryGenre,
} from "../../lib/controllerCatalogUi";
import { getGameScreenshots } from "../../lib/consoleGameMedia";
import { withImageWidth } from "../../lib/consoleImageSizing";
import { useConsoleImageWidths } from "../../hooks/useConsoleImageWidths";
import { useTranslation } from "../../i18n";

export interface ConsoleGameDetailsAction {
  id: string;
  label: string;
  tone?: "primary" | "secondary" | "danger";
  disabled?: boolean;
  onSelect: () => void;
}

export interface ConsoleGameDetailsProps {
  game: GameInfo;
  actions: ConsoleGameDetailsAction[];
  focusedActionIndex: number;
  onFocusAction: (index: number) => void;
  onClose: () => void;
}

/**
 * Full-screen game detail sheet, opened by A from any shelf.
 *
 * There is no trailer: NVIDIA's catalog API exposes no video for a title, only
 * still artwork, so the media strip shows screenshots rather than offering a
 * play-trailer button that could not do anything.
 */
export function ConsoleGameDetails({
  game,
  actions,
  focusedActionIndex,
  onFocusAction,
  onClose,
}: ConsoleGameDetailsProps): JSX.Element {
  const { t } = useTranslation();
  const imageWidths = useConsoleImageWidths();
  const heroUrl = withImageWidth(getControllerHeroBackgroundCandidates(game)[0], imageWidths.billboard);
  const logoUrl = withImageWidth(getGameLogoUrl(game), imageWidths.screenshot);
  const screenshots = getGameScreenshots(game);
  const players = getPlayerSummary(game);
  const genre = getPrimaryGenre(game);

  const [activeShot, setActiveShot] = useState(0);
  useEffect(() => setActiveShot(0), [game.id]);

  return (
    <div className="console-details" role="dialog" aria-modal="true" aria-label={game.title}>
      <div className="console-details-backdrop">
        {heroUrl && <img src={heroUrl} alt="" decoding="async" />}
        <span className="console-details-scrim" />
      </div>

      <div className="console-details-body">
        <div className="console-details-main">
          {logoUrl
            ? <img className="console-details-logo" src={logoUrl} alt={game.title} decoding="async" />
            : <h2 className="console-details-title">{game.title}</h2>}

          <div className="console-details-chips">
            <span className="console-billboard-meta-chip">
              {getGameStoreSummary(game, t("library.storeNotListed"))}
            </span>
            {genre && <span className="console-billboard-meta-chip">{genre}</span>}
            {game.isInLibrary && (
              <span className="console-billboard-meta-chip console-billboard-meta-chip--accent">
                {t("home.controller.owned")}
              </span>
            )}
            {game.contentRatings?.length ? (
              <span className="console-billboard-meta-chip">{game.contentRatings[0]}</span>
            ) : null}
          </div>

          <p className="console-details-description">
            {game.longDescription || game.description || game.featureLabels?.join(" / ") || t("library.loadingGameDetails")}
          </p>

          <div className="console-details-actions">
            {actions.map((action, index) => (
              <button
                key={action.id}
                type="button"
                className={`console-action console-action--${action.tone ?? "secondary"}${index === focusedActionIndex ? " is-focused" : ""}`}
                disabled={action.disabled}
                onClick={() => {
                  onFocusAction(index);
                  action.onSelect();
                }}
              >
                {action.label}
              </button>
            ))}
          </div>

          <dl className="console-details-facts">
            {game.developerName && (
              <div><dt>{t("library.detailLabels.developer")}</dt><dd>{game.developerName}</dd></div>
            )}
            {game.publisherName && (
              <div><dt>{t("library.detailLabels.publisher")}</dt><dd>{game.publisherName}</dd></div>
            )}
            {players && <div><dt>{t("library.detailLabels.players")}</dt><dd>{players}</dd></div>}
            {game.genres?.length ? (
              <div><dt>{t("library.detailLabels.genres")}</dt><dd>{game.genres.slice(0, 4).join(", ")}</dd></div>
            ) : null}
            {game.supportedControls?.length ? (
              <div><dt>{t("library.detailLabels.controls")}</dt><dd>{game.supportedControls.slice(0, 4).join(", ")}</dd></div>
            ) : null}
            {game.nvidiaTech?.length ? (
              <div><dt>{t("library.detailLabels.nvidiaTech")}</dt><dd>{game.nvidiaTech.slice(0, 4).join(", ")}</dd></div>
            ) : null}
          </dl>
        </div>

        {screenshots.length > 0 && (
          <div className="console-details-media">
            <img
              className="console-details-shot"
              src={withImageWidth(screenshots[Math.min(activeShot, screenshots.length - 1)], imageWidths.screenshot)}
              alt=""
              decoding="async"
            />
            {screenshots.length > 1 && (
              <div className="console-details-thumbs">
                {screenshots.slice(0, 6).map((shot, index) => (
                  <button
                    key={shot}
                    type="button"
                    className={`console-details-thumb${index === activeShot ? " is-active" : ""}`}
                    aria-label={t("library.screenshotNumber", { index: index + 1 })}
                    onClick={() => setActiveShot(index)}
                  >
                    <img src={withImageWidth(shot, imageWidths.thumb)} alt="" loading="lazy" decoding="async" />
                  </button>
                ))}
              </div>
            )}
          </div>
        )}
      </div>

      <button type="button" className="console-details-close" onClick={onClose} aria-label={t("app.actions.back")}>
        <span className="console-hint-glyph console-hint-glyph--b" aria-hidden="true">B</span>
        <span>{t("app.actions.back")}</span>
      </button>
    </div>
  );
}
