import type { JSX } from "react";
import { ButtonA, ButtonB, ButtonPSCircle, ButtonPSCross } from "../ControllerButtons";
import { spotlightEntryHasGame } from "./helpers";
import type { GameInfo } from "@shared/gfn";
import type { GameSubcategory, HomeRootPlane, SettingsSubcategory, SpotlightEntry } from "./types";

interface FooterHintsProps {
  localVideoPlayerOpen?: boolean;
  topLevelRowBehaviorActive: boolean;
  topCategory: string;
  settingsSubcategory: SettingsSubcategory;
  editingThemeChannel: "r" | "g" | "b" | null;
  mediaSubcategory: "root" | "Videos" | "Screenshots";
  gameSubcategory: GameSubcategory;
  gamesHubOpen: boolean;
  gamesRootPlane: "spotlight" | "categories";
  /** Games tab spotlight row; when false, root Games uses categories row only. */
  gamesDualShelf?: boolean;
  homeRootPlane?: HomeRootPlane;
  homeDualShelf?: boolean;
  spotlightEntries: SpotlightEntry[];
  spotlightIndex: number;
  currentStreamingGame: GameInfo | null | undefined;
  selectedGame: GameInfo | undefined;
  controllerType: "ps" | "xbox" | "nintendo" | "generic";
  renderFaceButton: (kind: "primary" | "secondary" | "tertiary", className: string, size: number) => JSX.Element;
}

export function FooterHints({
  localVideoPlayerOpen = false,
  topLevelRowBehaviorActive,
  topCategory,
  settingsSubcategory,
  editingThemeChannel,
  mediaSubcategory,
  gameSubcategory,
  gamesHubOpen,
  gamesRootPlane,
  gamesDualShelf = true,
  homeRootPlane = "spotlight",
  homeDualShelf = false,
  spotlightEntries,
  spotlightIndex,
  currentStreamingGame,
  selectedGame,
  controllerType,
  renderFaceButton,
}: FooterHintsProps): JSX.Element {
  if (localVideoPlayerOpen) {
    return (
      <div className="xmb-footer">
        <div className="xmb-btn-hint">
          {controllerType === "ps" ? (
            <ButtonPSCircle className="xmb-btn-icon" size={24} />
          ) : (
            <ButtonB className="xmb-btn-icon" size={24} />
          )}
          <span>Close</span>
        </div>
      </div>
    );
  }

  return (
    <div className="xmb-footer">
      {topLevelRowBehaviorActive ? (
        topCategory === "current" && homeDualShelf ? (
          <>
            <div className="xmb-btn-hint"><span>Rows · ↑ / ↓</span></div>
            <div className="xmb-btn-hint">
              {controllerType === "ps" ? (
                <ButtonPSCross className="xmb-btn-icon" size={24} />
              ) : (
                <ButtonA className="xmb-btn-icon" size={24} />
              )}
              <span>
                {homeRootPlane === "spotlight" && spotlightEntries[spotlightIndex]?.kind === "cloudResume"
                  ? spotlightEntries[spotlightIndex].busy
                    ? "Please wait"
                    : "Resume session"
                  : homeRootPlane === "spotlight" && spotlightEntryHasGame(spotlightEntries[spotlightIndex])
                    ? "Game hub"
                    : homeRootPlane === "spotlight"
                      ? "Enter"
                      : "Select"}
              </span>
            </div>
            <div className="xmb-btn-hint"><span className="xmb-btn-keycap">L1</span> <span>Prev Section</span></div>
            <div className="xmb-btn-hint"><span className="xmb-btn-keycap">R1</span> <span>Next Section</span></div>
          </>
        ) : (
          <>
            <div className="xmb-btn-hint">
              {controllerType === "ps" ? (
                <ButtonPSCross className="xmb-btn-icon" size={24} />
              ) : (
                <ButtonA className="xmb-btn-icon" size={24} />
              )}
              <span>Select</span>
            </div>
            <div className="xmb-btn-hint"><span className="xmb-btn-keycap">L1</span> <span>Prev Section</span></div>
            <div className="xmb-btn-hint"><span className="xmb-btn-keycap">R1</span> <span>Next Section</span></div>
          </>
        )
      ) : topCategory === "current" && !gamesHubOpen ? (
        <div className="xmb-btn-hint" style={{ margin: "0 auto" }}>
          {controllerType === "ps" ? (
            <ButtonPSCross className="xmb-btn-icon" size={24} />
          ) : (
            <ButtonA className="xmb-btn-icon" size={24} />
          )}
          <span>Select</span>
        </div>
      ) : topCategory === "settings" ? (
        <>
          {settingsSubcategory === "root" || settingsSubcategory === "Theme" ? (
            <div className="xmb-btn-hint">
              {controllerType === "ps" ? (
                <ButtonPSCross className="xmb-btn-icon" size={24} />
              ) : (
                <ButtonA className="xmb-btn-icon" size={24} />
              )}
              <span>Enter</span>
            </div>
          ) : settingsSubcategory === "ThemeStyle" ? (
            <>
              <div className="xmb-btn-hint">
                {controllerType === "ps" ? (
                  <ButtonPSCircle className="xmb-btn-icon" size={24} />
                ) : (
                  <ButtonB className="xmb-btn-icon" size={24} />
                )}
                <span>Back</span>
              </div>
              <div className="xmb-btn-hint">
                {controllerType === "ps" ? (
                  <ButtonPSCross className="xmb-btn-icon" size={24} />
                ) : (
                  <ButtonA className="xmb-btn-icon" size={24} />
                )}
                <span>Select</span>
              </div>
            </>
          ) : settingsSubcategory === "ThemeColor" ? (
            <>
              <div className="xmb-btn-hint">
                {controllerType === "ps" ? (
                  <ButtonPSCircle className="xmb-btn-icon" size={24} />
                ) : (
                  <ButtonB className="xmb-btn-icon" size={24} />
                )}
                <span>Back</span>
              </div>
              <div className="xmb-btn-hint">
                {controllerType === "ps" ? (
                  <ButtonPSCross className="xmb-btn-icon" size={24} />
                ) : (
                  <ButtonA className="xmb-btn-icon" size={24} />
                )}
                <span>{editingThemeChannel ? "Confirm" : "Adjust"}</span>
              </div>
            </>
          ) : (
            <>
              <div className="xmb-btn-hint">
                {controllerType === "ps" ? (
                  <ButtonPSCircle className="xmb-btn-icon" size={24} />
                ) : (
                  <ButtonB className="xmb-btn-icon" size={24} />
                )}
                <span>Back</span>
              </div>
              <div className="xmb-btn-hint">
                {controllerType === "ps" ? (
                  <ButtonPSCross className="xmb-btn-icon" size={24} />
                ) : (
                  <ButtonA className="xmb-btn-icon" size={24} />
                )}
                <span>Toggle</span>
              </div>
            </>
          )}
        </>
      ) : topCategory === "media" ? (
        <>
          {mediaSubcategory === "root" ? (
            <div className="xmb-btn-hint">
              {controllerType === "ps" ? (
                <ButtonPSCross className="xmb-btn-icon" size={24} />
              ) : (
                <ButtonA className="xmb-btn-icon" size={24} />
              )}
              <span>Enter</span>
            </div>
          ) : (
            <>
              <div className="xmb-btn-hint"><span>Browse · Left / Right</span></div>
              <div className="xmb-btn-hint">
                {controllerType === "ps" ? (
                  <ButtonPSCross className="xmb-btn-icon" size={24} />
                ) : (
                  <ButtonA className="xmb-btn-icon" size={24} />
                )}
                <span>{mediaSubcategory === "Videos" ? "Play" : "Open Folder"}</span>
              </div>
              <div className="xmb-btn-hint">{renderFaceButton("tertiary", "xmb-btn-icon", 24)} <span>Options</span></div>
              <div className="xmb-btn-hint">
                {controllerType === "ps" ? (
                  <ButtonPSCircle className="xmb-btn-icon" size={24} />
                ) : (
                  <ButtonB className="xmb-btn-icon" size={24} />
                )}
                <span>Back To Media</span>
              </div>
            </>
          )}
        </>
      ) : (topCategory === "all" && gameSubcategory !== "root") ||
        (topCategory === "current" && gamesHubOpen) ? (
        gamesHubOpen ? (
          <>
            <div className="xmb-btn-hint"><span>Actions · Left / Right</span></div>
            <div className="xmb-btn-hint">
              {controllerType === "ps" ? (
                <ButtonPSCross className="xmb-btn-icon" size={24} />
              ) : (
                <ButtonA className="xmb-btn-icon" size={24} />
              )}
              <span>Confirm</span>
            </div>
            <div className="xmb-btn-hint">
              {controllerType === "ps" ? (
                <ButtonPSCircle className="xmb-btn-icon" size={24} />
              ) : (
                <ButtonB className="xmb-btn-icon" size={24} />
              )}
              <span>{topCategory === "current" ? "Back to Home" : "Back"}</span>
            </div>
            <div className="xmb-btn-hint">{renderFaceButton("tertiary", "xmb-btn-icon", 24)} <span>Options</span></div>
          </>
        ) : (
          <>
            <div className="xmb-btn-hint"><span>Browse · Left / Right</span></div>
            <div className="xmb-btn-hint"><span>Library filters · Up</span></div>
            <div className="xmb-btn-hint">{renderFaceButton("primary", "xmb-btn-icon", 24)} <span>Game hub</span></div>
            <div className="xmb-btn-hint"><span>Hub · Down</span></div>
            {gameSubcategory === "all" ? (
              <div className="xmb-btn-hint">{renderFaceButton("secondary", "xmb-btn-icon", 24)} <span>Sort</span></div>
            ) : null}
            <div className="xmb-btn-hint">{renderFaceButton("tertiary", "xmb-btn-icon", 24)} <span>Options</span></div>
          </>
        )
      ) : topCategory === "all" && gameSubcategory === "root" ? (
        <>
          <div className="xmb-btn-hint">
            {controllerType === "ps" ? (
              <ButtonPSCross className="xmb-btn-icon" size={24} />
            ) : (
              <ButtonA className="xmb-btn-icon" size={24} />
            )}
            <span>
              {gamesDualShelf && gamesRootPlane === "spotlight" && spotlightEntries[spotlightIndex]?.kind === "cloudResume"
                ? spotlightEntries[spotlightIndex].busy
                  ? "Please wait"
                  : "Resume session"
                : gamesDualShelf && gamesRootPlane === "spotlight" && spotlightEntryHasGame(spotlightEntries[spotlightIndex])
                  ? "View in library"
                  : "Enter"}
            </span>
          </div>
          {gamesDualShelf && gamesRootPlane === "spotlight" && spotlightEntryHasGame(spotlightEntries[spotlightIndex]) ? (
            <div className="xmb-btn-hint">{renderFaceButton("tertiary", "xmb-btn-icon", 24)} <span>Options</span></div>
          ) : null}
          <div className="xmb-btn-hint"><span className="xmb-btn-keycap">L1</span> <span>Prev Section</span></div>
          <div className="xmb-btn-hint"><span className="xmb-btn-keycap">R1</span> <span>Next Section</span></div>
        </>
      ) : (
        <>
          <div className="xmb-btn-hint">{renderFaceButton("primary", "xmb-btn-icon", 24)} <span>{currentStreamingGame && selectedGame && currentStreamingGame.id !== selectedGame.id ? "Switch" : "Play"}</span></div>
          <div className="xmb-btn-hint">{renderFaceButton("tertiary", "xmb-btn-icon", 24)} <span>Options</span></div>
        </>
      )}
    </div>
  );
}
