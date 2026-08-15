import { Check, Crown, X } from "lucide-react";
import {
  useId,
  useLayoutEffect,
  useRef,
  useState,
  type JSX,
} from "react";
import type { EntitledResolution, GameInfo, GameStreamProfiles, Settings } from "@shared/gfn";
import { getControllerHeroBackgroundCandidates, getPlayerSummary } from "../lib/controllerCatalogUi";
import { getActiveStoreOption, getStoreOptions } from "../lib/gameCardStores";
import { getRequiredPaidMembershipTier } from "../lib/premiumMembership";
import { useTranslation } from "../i18n";
import { getStoreDisplayName, getStoreIconComponent } from "./GameCard";
import { ModalSurface } from "./ui/ModalSurface";
import { GameStreamProfileSection } from "./GameStreamProfileSection";

export interface GameDetailModalProps {
  open: boolean;
  game: GameInfo | null;
  selectedVariantId?: string;
  onClose: () => void;
  onExitComplete?: () => void;
  onPlay: (game: GameInfo, variantId?: string) => void;
  onSelectVariant: (variantId: string) => void;
  settings: Settings;
  entitledResolutions: EntitledResolution[];
  onGameStreamProfilesPreview: (profiles: GameStreamProfiles) => void;
  onGameStreamProfilesChange: (profiles: GameStreamProfiles) => void;
}

function GameDetailHero({ game }: { game: GameInfo }): JSX.Element {
  const candidates = getControllerHeroBackgroundCandidates(game);
  const [candidateIndex, setCandidateIndex] = useState(0);
  const imageUrl = candidates[candidateIndex];

  if (!imageUrl) {
    return <div className="game-detail-hero-image game-detail-hero-image--empty" />;
  }

  return (
    <img
      className="game-detail-hero-image"
      src={imageUrl}
      alt=""
      decoding="async"
      fetchPriority="high"
      onError={() => setCandidateIndex((index) => index + 1)}
    />
  );
}

function ExpandableDescription({
  descriptionId,
  text,
}: {
  descriptionId: string;
  text: string;
}): JSX.Element {
  const { t } = useTranslation();
  const [expanded, setExpanded] = useState(false);
  const [isClamped, setIsClamped] = useState(false);
  const descriptionRef = useRef<HTMLParagraphElement | null>(null);

  useLayoutEffect(() => {
    const element = descriptionRef.current;
    if (!element) return undefined;

    const measure = (): void => {
      setIsClamped(element.scrollHeight - element.clientHeight > 2);
    };
    measure();

    const observer = typeof ResizeObserver === "undefined" ? null : new ResizeObserver(measure);
    observer?.observe(element);
    return () => observer?.disconnect();
  }, []);

  return (
    <div className="game-detail-description-block">
      <p
        ref={descriptionRef}
        id={descriptionId}
        className={`game-detail-description${expanded ? " is-expanded" : ""}`}
      >
        {text}
      </p>
      {(isClamped || expanded) && (
        <button
          type="button"
          className="game-detail-readmore"
          aria-expanded={expanded}
          aria-controls={descriptionId}
          onClick={() => setExpanded((value) => !value)}
        >
          {expanded ? t("app.actions.showLess") : t("app.actions.readMore")}
        </button>
      )}
    </div>
  );
}

export function GameDetailModal({
  open,
  game,
  selectedVariantId,
  onClose,
  onExitComplete,
  onPlay,
  onSelectVariant,
  settings,
  entitledResolutions,
  onGameStreamProfilesPreview,
  onGameStreamProfilesChange,
}: GameDetailModalProps): JSX.Element {
  const { t } = useTranslation();
  const generatedId = useId();
  const titleId = `${generatedId}-title`;
  const descriptionId = `${generatedId}-description`;

  if (!game) {
    return (
      <ModalSurface
        open={false}
        onClose={onClose}
        onExitComplete={onExitComplete}
        overlayClassName="game-detail-overlay"
        backdropClassName="game-detail-backdrop"
        panelClassName="game-detail-panel"
        motion="large"
      >
        <div />
      </ModalSurface>
    );
  }

  const storeOptions = getStoreOptions(game, selectedVariantId);
  const activeStoreOption = getActiveStoreOption(storeOptions);
  const requiredPaidMembershipTier = getRequiredPaidMembershipTier(game);
  const playerSummary = getPlayerSummary(game);
  const description = game.description
    || game.longDescription
    || game.featureLabels?.join(" / ")
    || t("library.loadingGameDetails");
  const metadata = [
    game.developerName ? t("library.developer", { developer: game.developerName }) : null,
    game.publisherName ? t("library.publisher", { publisher: game.publisherName }) : null,
    playerSummary ? t("library.players", { players: playerSummary }) : null,
    game.genres?.length ? t("library.genres", { genres: game.genres.slice(0, 4).join(", ") }) : null,
    game.supportedControls?.length
      ? t("library.controls", { controls: game.supportedControls.slice(0, 4).join(", ") })
      : null,
    game.nvidiaTech?.length ? t("library.nvidiaTech", { tech: game.nvidiaTech.slice(0, 4).join(", ") }) : null,
    game.contentRatings?.length
      ? t("library.rating", { rating: game.contentRatings.slice(0, 2).join(", ") })
      : null,
  ].filter((value): value is string => Boolean(value));

  const handlePlay = (): void => {
    onPlay(game, activeStoreOption?.variantId ?? selectedVariantId);
  };

  return (
    <ModalSurface
      open={open}
      onClose={onClose}
      onExitComplete={onExitComplete}
      onConfirm={handlePlay}
      overlayClassName="game-detail-overlay"
      backdropClassName="game-detail-backdrop"
      panelClassName="game-detail-panel"
      motion="large"
      ariaLabelledBy={titleId}
      ariaDescribedBy={descriptionId}
      backdropLabel={t("app.actions.close")}
    >
      <header className="game-detail-hero">
        <GameDetailHero key={game.id} game={game} />
        <div className="game-detail-hero-scrim" />
        <button
          type="button"
          className="game-detail-close"
          onClick={onClose}
          aria-label={t("app.actions.close")}
        >
          <X size={19} />
        </button>
        <div className="game-detail-heading">
          <span className="game-detail-eyebrow">{t("gameDetails.eyebrow")}</span>
          <h2 id={titleId} className="game-detail-title">{game.title}</h2>
        </div>
      </header>

      <div className="game-detail-body">
        {requiredPaidMembershipTier && (
          <div className="game-detail-membership-warning" role="note">
            <span className="game-detail-membership-icon" aria-hidden="true">
              <Crown size={18} strokeWidth={2.2} />
            </span>
            <span>
              <strong>{t("gameDetails.premiumRequired")}</strong>
              <span>{t("gameDetails.freeTierUnavailable", { tier: requiredPaidMembershipTier })}</span>
            </span>
          </div>
        )}

        <ExpandableDescription
          key={game.id}
          descriptionId={descriptionId}
          text={description}
        />

        {metadata.length > 0 && (
          <section className="game-detail-section" aria-labelledby={`${generatedId}-metadata`}>
            <h3 id={`${generatedId}-metadata`} className="game-detail-section-title">
              {t("gameDetails.details")}
            </h3>
            <ul className="game-detail-meta">
              {metadata.map((row) => <li key={row}>{row}</li>)}
            </ul>
          </section>
        )}

        {storeOptions.length > 0 && (
          <section className="game-detail-section" aria-labelledby={`${generatedId}-stores`}>
            <h3 id={`${generatedId}-stores`} className="game-detail-section-title">
              {t("library.chooseStore")}
            </h3>
            <div className="game-detail-stores-row">
              {storeOptions.map((option) => {
                const StoreIcon = getStoreIconComponent(option.store);
                const storeName = getStoreDisplayName(option.store);
                const ownershipLabel = option.isOwned ? t("gameCard.owned") : t("gameDetails.notOwned");
                const className = [
                  "game-detail-store-chip",
                  option.isActive ? "active" : "",
                  option.isOwned ? "owned" : "not-owned",
                ].filter(Boolean).join(" ");

                return (
                  <button
                    key={option.storeKey}
                    type="button"
                    className={className}
                    aria-label={t("gameDetails.storeOption", { store: storeName, ownership: ownershipLabel })}
                    aria-pressed={option.isActive}
                    onClick={() => onSelectVariant(option.variantId)}
                  >
                    <StoreIcon />
                    <span className="game-detail-store-name">{storeName}</span>
                    <span className="game-detail-store-ownership">
                      {option.isOwned && <Check size={12} aria-hidden="true" />}
                      {ownershipLabel}
                    </span>
                  </button>
                );
              })}
            </div>
          </section>
        )}

        <GameStreamProfileSection
          game={game}
          settings={settings}
          entitledResolutions={entitledResolutions}
          onPreview={onGameStreamProfilesPreview}
          onChange={onGameStreamProfilesChange}
        />
      </div>

      <footer className="game-detail-actions">
        <button type="button" className="game-detail-play" onClick={handlePlay}>
          {activeStoreOption
            ? t("gameDetails.playOn", { store: getStoreDisplayName(activeStoreOption.store) })
            : t("app.actions.play")}
        </button>
        <button type="button" className="game-detail-cancel" onClick={onClose}>
          {t("app.actions.close")}
        </button>
      </footer>
    </ModalSurface>
  );
}
