import type { JSX } from "react";
import type { SavedAccount } from "@shared/gfn";
import { useTranslation } from "../../i18n";
import { ConsoleHintBar } from "./ConsoleHintBar";
import { ConsoleProfileActionTile, ConsoleProfileTile } from "./ConsoleProfileTile";

export type ConsolePickerEntry =
  | { kind: "account"; account: SavedAccount }
  | { kind: "action"; action: "add" | "manage" };

export interface ConsoleProfilePickerProps {
  entries: ConsolePickerEntry[];
  focusedIndex: number;
  activeUserId: string | null;
  errorMessage?: string;
  onFocus: (index: number) => void;
  onSelect: (index: number) => void;
  onBack?: () => void;
}

/** "Who's playing?" — the console entry point. */
export function ConsoleProfilePicker({
  entries,
  focusedIndex,
  activeUserId,
  errorMessage,
  onFocus,
  onSelect,
  onBack,
}: ConsoleProfilePickerProps): JSX.Element {
  const { t } = useTranslation();

  return (
    <>
      <h1 className="console-gate-title">{t("console.profiles.whosPlaying")}</h1>
      {errorMessage && <p className="console-gate-error">{errorMessage}</p>}

      <div className="console-profile-grid" role="group" aria-label={t("console.profiles.whosPlaying")}>
        {entries.map((entry, index) => (entry.kind === "account" ? (
          <ConsoleProfileTile
            key={entry.account.userId}
            account={entry.account}
            index={index}
            isFocused={index === focusedIndex}
            isActive={entry.account.userId === activeUserId}
            onSelect={() => {
              onFocus(index);
              onSelect(index);
            }}
          />
        ) : (
          <ConsoleProfileActionTile
            key={entry.action}
            action={entry.action}
            index={index}
            isFocused={index === focusedIndex}
            onSelect={() => {
              onFocus(index);
              onSelect(index);
            }}
          />
        )))}
      </div>

      <ConsoleHintBar
        hints={[
          { glyph: "a", label: t("app.actions.select"), onSelect: () => onSelect(focusedIndex) },
          ...(onBack ? [{ glyph: "b" as const, label: t("app.actions.back"), onSelect: onBack }] : []),
        ]}
      />
    </>
  );
}
