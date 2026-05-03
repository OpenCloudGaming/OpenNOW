import type { CSSProperties, JSX } from "react";
import { Ps5LoadingScreen } from "../../Ps5LoadingScreen";
import { CurrentClock, RemainingPlaytimeIndicator } from "../../ElapsedSessionIndicators";
import { ControllerGameHub } from "../ControllerGameHub";
import { LIBRARY_SORT_LABEL } from "./constants";
import { AllGamesBrowseSection } from "./AllGamesBrowseSection";
import { TopLevelShelfSection } from "./TopLevelShelfSection";
import { MediaHubSection } from "./MediaHubSection";
import { CurrentDetailPanel } from "./CurrentDetailPanel";
import { DetailRail } from "./DetailRail";
import { OptionsSheet } from "./OptionsSheet";
import { FooterHints } from "./FooterHints";
import { LibraryChrome } from "./LibraryChrome";

export function ControllerLibraryLayout(props: Record<string, any>): JSX.Element {
  const {
    isLoading,
    topCategory,
    wrapperClassNameWithRow,
    wrapperThemeVars,
    currentStreamingGame,
    inStreamMenu,
    endSessionConfirm,
    parallaxBackdropTiles,
    heroBackdropUrl,
    settings,
    subscriptionInfo,
    sessionStartedAtMs,
    isStreaming,
    userAvatarUrl,
    userName,
    categoryIndex,
    TOP_CATEGORIES,
    getCategoryIcon,
    gameSubcategory,
    gamesHubOpen,
    selectedGame,
    gameHubScreenshotUrls,
    playtimeData,
    selectedVariantId,
    librarySortId,
    gamesHubTiles,
    gamesHubFocusIndex,
    categorizedGames,
    focusMotionKey,
    selectedVariantByGameId,
    favoriteGameIdSet,
    selectedIndex,
    itemsContainerRef,
    listTranslateX,
    topLevelShelfActive,
    selectedTopLevelItemLabel,
    gamesRootPlane,
    spotlightEntries,
    spotlightIndex,
    displayItems,
    topLevelShelfIndex,
    gamesDualShelf,
    cloudSessionResumable,
    onResumeCloudSession,
    spotlightTrackRef,
    spotlightShelfTranslateX,
    topLevelMenuTrack,
    mediaSubcategory,
    selectedMediaItem,
    mediaAssetItems,
    mediaHubPlaceholderCount,
    mediaLoading,
    mediaError,
    mediaHubSlots,
    selectedMediaIndex,
    mediaThumbById,
    detailVisible,
    pendingSwitchGameCover,
    attachPosterRef,
    metaMaxWidth,
    sessionCounterEnabled,
    ps5Row,
    canEnterDetailRow,
    detailRailItems,
    detailRailIndex,
    optionsOpen,
    optionsEntries,
    optionsFocusIndex,
    topLevelRowBehaviorActive,
    editingThemeChannel,
    selectedGameForHints,
    controllerType,
    renderFaceButton,
  } = props;

  if (isLoading && topCategory !== "settings" && topCategory !== "current" && topCategory !== "media") {
    return (
      <div className={wrapperClassNameWithRow} style={wrapperThemeVars}>
        <div className="xmb-bg-layer">
          <div className="xmb-bg-gradient" />
        </div>
        <Ps5LoadingScreen
          title="Loading your library"
          subtitle="Please wait"
          backdropImageUrl={currentStreamingGame?.imageUrl}
        />
      </div>
    );
  }

  return (
    <div className={wrapperClassNameWithRow} style={wrapperThemeVars}>
      {inStreamMenu && endSessionConfirm ? (
        <div className="xmb-end-session-banner" role="status">
          End session? Press Enter again to confirm, or Back to cancel.
        </div>
      ) : null}
      <div className="xmb-bg-layer">
        {parallaxBackdropTiles.length > 0 ? (
          <div className="xmb-ps5-parallax-field" aria-hidden>
            {parallaxBackdropTiles.map((tile: any, idx: number) => (
              <div
                key={`${tile.src}-${idx}`}
                className={`xmb-ps5-parallax-tile xmb-ps5-parallax-row-${tile.lane}`}
                style={
                  {
                    backgroundImage: `url(${tile.src})`,
                    left: `${tile.left}%`,
                    "--parallax-delay": `${tile.delaySec}s`,
                    "--parallax-scale": String(tile.scale),
                    "--parallax-x-from": `${tile.xFrom}vw`,
                    "--parallax-x-to": `${tile.xTo}vw`,
                    "--parallax-rot-from": `${tile.rotFrom}deg`,
                    "--parallax-rot-to": `${tile.rotTo}deg`,
                  } as CSSProperties
                }
              />
            ))}
          </div>
        ) : null}
        {heroBackdropUrl ? (
          <div
            className={`xmb-ps5-hero-art ${settings.controllerBackgroundAnimations ? "xmb-ps5-hero-art--motion" : ""}`}
            style={{ backgroundImage: `url(${heroBackdropUrl})` }}
            aria-hidden
          />
        ) : null}
        <div className="xmb-bg-gradient" />
        <div className="xmb-bg-overlay" />
      </div>

      <LibraryChrome
        logoUrl={new URL("../../../assets/opennow-logo.png", import.meta.url).toString()}
        clockElement={(
          <div className="xmb-clock-wrap">
            <CurrentClock className="xmb-clock" />
            <div className="xmb-remaining-playtime">
              <RemainingPlaytimeIndicator
                subscriptionInfo={subscriptionInfo}
                startedAtMs={sessionStartedAtMs}
                active={isStreaming}
                className="xmb-remaining-playtime-text"
              />
            </div>
          </div>
        )}
        userAvatarUrl={userAvatarUrl}
        userName={userName}
        categoryIndex={categoryIndex}
        topCategories={TOP_CATEGORIES}
        getCategoryIcon={getCategoryIcon}
      />

      {topCategory === "all" && gameSubcategory !== "root" && gamesHubOpen && selectedGame ? (
        <ControllerGameHub
          game={selectedGame}
          screenshotUrls={gameHubScreenshotUrls}
          playtimeData={playtimeData}
          selectedVariantId={selectedVariantId}
          currentStreamingGame={currentStreamingGame}
          librarySortLabel={gameSubcategory === "all" ? LIBRARY_SORT_LABEL[librarySortId as keyof typeof LIBRARY_SORT_LABEL] : null}
          tiles={gamesHubTiles}
          focusIndex={gamesHubFocusIndex}
          inStreamMenu={inStreamMenu}
        />
      ) : null}

      {topCategory === "all" && gameSubcategory !== "root" && !gamesHubOpen && (
        <AllGamesBrowseSection
          isLoading={isLoading}
          categorizedGames={categorizedGames}
          selectedGame={selectedGame}
          focusMotionKey={focusMotionKey}
          gameSubcategory={gameSubcategory}
          librarySortId={librarySortId}
          playtimeData={playtimeData}
          selectedVariantByGameId={selectedVariantByGameId}
          favoriteGameIdSet={favoriteGameIdSet}
          selectedIndex={selectedIndex}
          itemsContainerRef={itemsContainerRef}
          listTranslateX={listTranslateX}
        />
      )}

      <TopLevelShelfSection
        topLevelShelfActive={topLevelShelfActive}
        focusMotionKey={focusMotionKey}
        selectedTopLevelItemLabel={selectedTopLevelItemLabel}
        topCategory={topCategory}
        gameSubcategory={gameSubcategory}
        gamesRootPlane={gamesRootPlane}
        spotlightEntries={spotlightEntries}
        spotlightIndex={spotlightIndex}
        displayItems={displayItems}
        topLevelShelfIndex={topLevelShelfIndex}
        currentStreamingGame={currentStreamingGame}
        playtimeData={playtimeData}
        gamesDualShelf={gamesDualShelf}
        cloudSessionResumable={cloudSessionResumable}
        onResumeCloudSession={onResumeCloudSession}
        spotlightTrackRef={spotlightTrackRef}
        spotlightShelfTranslateX={spotlightShelfTranslateX}
        topLevelMenuTrack={topLevelMenuTrack}
      />

      {topCategory === "media" && mediaSubcategory !== "root" && (
        <MediaHubSection
          focusMotionKey={focusMotionKey}
          selectedMediaItem={selectedMediaItem}
          mediaSubcategory={mediaSubcategory}
          mediaAssetItems={mediaAssetItems}
          mediaHubPlaceholderCount={mediaHubPlaceholderCount}
          itemsContainerRef={itemsContainerRef}
          listTranslateX={listTranslateX}
          mediaLoading={mediaLoading}
          mediaError={mediaError}
          mediaHubSlots={mediaHubSlots}
          selectedMediaIndex={selectedMediaIndex}
          mediaThumbById={mediaThumbById}
        />
      )}

      <div className={`xmb-detail-layer ${detailVisible ? "visible" : ""}`}>
        <CurrentDetailPanel
          topCategory={topCategory}
          pendingSwitchGameCover={pendingSwitchGameCover}
          currentStreamingGame={currentStreamingGame}
          attachPosterRef={attachPosterRef}
          metaMaxWidth={metaMaxWidth}
          selectedVariantByGameId={selectedVariantByGameId}
          playtimeData={playtimeData}
          sessionCounterEnabled={sessionCounterEnabled}
          sessionStartedAtMs={sessionStartedAtMs}
          isStreaming={isStreaming}
        />
      </div>

      <DetailRail
        ps5Row={ps5Row}
        canEnterDetailRow={canEnterDetailRow}
        detailRailItems={detailRailItems}
        detailRailIndex={detailRailIndex}
      />

      <OptionsSheet
        optionsOpen={optionsOpen}
        optionsEntries={optionsEntries}
        optionsFocusIndex={optionsFocusIndex}
      />

      <FooterHints
        topLevelRowBehaviorActive={topLevelRowBehaviorActive}
        topCategory={topCategory}
        settingsSubcategory={props.settingsSubcategory}
        editingThemeChannel={editingThemeChannel}
        mediaSubcategory={mediaSubcategory}
        gameSubcategory={gameSubcategory}
        gamesHubOpen={gamesHubOpen}
        gamesRootPlane={gamesRootPlane}
        spotlightEntries={spotlightEntries}
        spotlightIndex={spotlightIndex}
        currentStreamingGame={currentStreamingGame}
        selectedGame={selectedGameForHints ?? selectedGame}
        controllerType={controllerType}
        renderFaceButton={renderFaceButton}
      />
    </div>
  );
}
