import { Check, Play, X } from "lucide-react";
import { useEffect, useMemo, useRef } from "react";
import type { JSX } from "react";
import { createPortal } from "react-dom";
import type { GameInfo } from "@shared/gfn";
import { getStoreOptions } from "./GameCard";

interface LaunchStorePickerModalProps {
  game: GameInfo;
  selectedVariantId?: string;
  onSelectVariant: (variantId: string) => void;
  onConfirm: (variantId: string) => void;
  onCancel: () => void;
}

export function hasMultipleLaunchStoreOptions(game: GameInfo): boolean {
  return getStoreOptions(game).length > 1;
}

export function LaunchStorePickerModal({
  game,
  selectedVariantId,
  onSelectVariant,
  onConfirm,
  onCancel,
}: LaunchStorePickerModalProps): JSX.Element | null {
  const storeOptions = useMemo(() => getStoreOptions(game), [game]);
  const activeVariantId = storeOptions.some((option) => option.variantId === selectedVariantId)
    ? selectedVariantId
    : storeOptions[0]?.variantId;
  const launchButtonRef = useRef<HTMLButtonElement | null>(null);

  useEffect(() => {
    const focusTimer = window.setTimeout(() => launchButtonRef.current?.focus(), 0);
    const handleKeyDown = (event: KeyboardEvent): void => {
      if (event.key === "Escape") {
        onCancel();
      }
    };
    const previousOverflow = document.body.style.overflow;
    document.body.style.overflow = "hidden";
    window.addEventListener("keydown", handleKeyDown);
    return () => {
      window.clearTimeout(focusTimer);
      window.removeEventListener("keydown", handleKeyDown);
      document.body.style.overflow = previousOverflow;
    };
  }, [onCancel]);

  if (storeOptions.length <= 1 || !activeVariantId || typeof document === "undefined") {
    return null;
  }

  const activeStore = storeOptions.find((option) => option.variantId === activeVariantId) ?? storeOptions[0];

  return createPortal(
    <div className="launch-store-picker" role="dialog" aria-modal="true" aria-label={`Launch ${game.title}`}>
      <button
        type="button"
        className="launch-store-picker-backdrop"
        tabIndex={-1}
        onClick={onCancel}
        aria-label="Cancel launch"
      />
      <div className="launch-store-picker-panel">
        <div className="launch-store-picker-head">
          <div className="launch-store-picker-title">
            <span>Launch</span>
            <h3>{game.title}</h3>
          </div>
          <button type="button" className="launch-store-picker-close" onClick={onCancel} aria-label="Close">
            <X size={18} />
          </button>
        </div>

        <div className="launch-store-picker-options">
          {storeOptions.map((option) => {
            const selected = option.variantId === activeVariantId;
            return (
              <button
                key={option.storeKey}
                type="button"
                className={`launch-store-option${selected ? " is-selected" : ""}`}
                onClick={() => onSelectVariant(option.variantId)}
                aria-pressed={selected}
              >
                <span className="launch-store-option-icon">
                  <option.IconComponent />
                </span>
                <span className="launch-store-option-copy">
                  <strong>{option.displayName}</strong>
                  <span>{selected ? "Selected" : "Available"}</span>
                </span>
                <span className="launch-store-option-check" aria-hidden="true">
                  {selected && <Check size={16} />}
                </span>
              </button>
            );
          })}
        </div>

        <div className="launch-store-picker-actions">
          <button type="button" className="launch-store-picker-cancel" onClick={onCancel}>
            Cancel
          </button>
          <button
            ref={launchButtonRef}
            type="button"
            className="launch-store-picker-confirm"
            onClick={() => onConfirm(activeVariantId)}
          >
            <Play size={16} fill="currentColor" />
            <span>Launch {activeStore?.displayName ?? "Game"}</span>
          </button>
        </div>
      </div>
    </div>,
    document.body,
  );
}
