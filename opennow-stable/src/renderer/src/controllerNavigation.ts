import { useEffect, useRef, useState } from "react";

type Direction = "up" | "down" | "left" | "right";
type PageDirection = "prev" | "next";

interface UseControllerNavigationOptions {
  enabled: boolean;
  onNavigatePage?: (direction: PageDirection) => void;
  onBackAction?: () => boolean;
}

const INTERACTIVE_SELECTOR = [
  "button",
  "a[href]",
  "input:not([type='hidden'])",
  "select",
  "textarea",
  "[role='button']",
  "[tabindex]:not([tabindex='-1'])",
].join(",");

const DIRECTION_INITIAL_REPEAT_MS = 240;
const DIRECTION_REPEAT_MS = 110;

function isElementInteractive(el: Element): el is HTMLElement {
  return el instanceof HTMLElement;
}

function isElementVisible(el: HTMLElement): boolean {
  if (el.getClientRects().length === 0) return false;
  const style = window.getComputedStyle(el);
  if (style.visibility === "hidden" || style.display === "none") return false;
  return true;
}

function isElementDisabled(el: HTMLElement): boolean {
  if (el.getAttribute("aria-disabled") === "true") return true;
  if ("disabled" in el) {
    return Boolean((el as HTMLButtonElement | HTMLInputElement | HTMLSelectElement | HTMLTextAreaElement).disabled);
  }
  return false;
}

function getFocusScopeRoot(): ParentNode {
  const exitDialog = document.querySelector(".sv-exit");
  if (exitDialog) return exitDialog;

  const navbarModal = document.querySelector(".navbar-modal");
  if (navbarModal) return navbarModal;

  const loginDropdown = document.querySelector(".login-dropdown");
  if (loginDropdown?.parentElement) return loginDropdown.parentElement;

  const regionDropdown = document.querySelector(".region-dropdown");
  if (regionDropdown?.parentElement) return regionDropdown.parentElement;

  const streamLoading = document.querySelector(".sload");
  if (streamLoading) return streamLoading;

  return document;
}

function listInteractiveElements(): HTMLElement[] {
  const scopeRoot = getFocusScopeRoot();
  const candidates = Array.from(scopeRoot.querySelectorAll(INTERACTIVE_SELECTOR))
    .filter(isElementInteractive)
    .filter((el) => el.tabIndex >= 0)
    .filter((el) => !isElementDisabled(el) && isElementVisible(el));
  return candidates;
}

function getElementCenter(el: HTMLElement): { x: number; y: number } {
  const rect = el.getBoundingClientRect();
  return {
    x: rect.left + rect.width / 2,
    y: rect.top + rect.height / 2,
  };
}

function setControllerFocus(el: HTMLElement): void {
  document.querySelectorAll<HTMLElement>(".controller-focus").forEach((node) => {
    node.classList.remove("controller-focus");
  });
  el.classList.add("controller-focus");
  el.focus({ preventScroll: true });
  el.scrollIntoView({ block: "nearest", inline: "nearest" });
}

function adjustRangeInput(input: HTMLInputElement, direction: Direction): boolean {
  if (input.type !== "range") return false;
  if (direction !== "left" && direction !== "right" && direction !== "up" && direction !== "down") return false;

  const min = Number.parseFloat(input.min || "0");
  const max = Number.parseFloat(input.max || "100");
  const step = Number.parseFloat(input.step || "1");
  const current = Number.parseFloat(input.value || "0");

  if (!Number.isFinite(min) || !Number.isFinite(max) || !Number.isFinite(step) || !Number.isFinite(current)) {
    return false;
  }

  const delta = direction === "left" || direction === "down" ? -step : step;
  const next = Math.max(min, Math.min(max, current + delta));
  if (next === current) return true;

  input.value = String(next);
  input.dispatchEvent(new Event("input", { bubbles: true }));
  input.dispatchEvent(new Event("change", { bubbles: true }));
  return true;
}

function setRangeEditMode(input: HTMLInputElement | null): void {
  document.querySelectorAll<HTMLInputElement>(".controller-range-editing").forEach((node) => {
    node.classList.remove("controller-range-editing");
  });
  if (input) {
    input.classList.add("controller-range-editing");
  }
}

function moveFocus(direction: Direction): void {
  const current = document.activeElement instanceof HTMLElement ? document.activeElement : null;
  if (
    current instanceof HTMLInputElement
    && current.type === "range"
    && current.classList.contains("controller-range-editing")
    && adjustRangeInput(current, direction)
  ) {
    return;
  }

  const items = listInteractiveElements();
  if (items.length === 0) return;

  const active = current && items.includes(current) ? current : null;
  if (!active) {
    setControllerFocus(items[0]);
    return;
  }

  const origin = getElementCenter(active);
  let best: { element: HTMLElement; score: number } | null = null;

  for (const candidate of items) {
    if (candidate === active) continue;
    const center = getElementCenter(candidate);
    const dx = center.x - origin.x;
    const dy = center.y - origin.y;

    const inDirection =
      (direction === "up" && dy < -4) ||
      (direction === "down" && dy > 4) ||
      (direction === "left" && dx < -4) ||
      (direction === "right" && dx > 4);
    if (!inDirection) continue;

    const primary = direction === "up" || direction === "down" ? Math.abs(dy) : Math.abs(dx);
    const secondary = direction === "up" || direction === "down" ? Math.abs(dx) : Math.abs(dy);
    const alignment = secondary / Math.max(primary, 1);
    const score = primary + secondary * 0.55 + alignment * 140;

    if (!best || score < best.score) {
      best = { element: candidate, score };
    }
  }

  if (best) {
    setControllerFocus(best.element);
    return;
  }

  if (direction === "left" || direction === "up") {
    setControllerFocus(items[items.length - 1]);
  } else {
    setControllerFocus(items[0]);
  }
}

function activateFocusedElement(): void {
  const active = document.activeElement instanceof HTMLElement ? document.activeElement : null;
  if (!active || isElementDisabled(active) || !isElementVisible(active)) {
    const items = listInteractiveElements();
    if (items.length > 0) {
      setControllerFocus(items[0]);
    }
    return;
  }

  if (active instanceof HTMLInputElement) {
    if (active.type === "checkbox" || active.type === "radio") {
      active.click();
      return;
    }
    if (active.type === "range") {
      if (active.classList.contains("controller-range-editing")) {
        setRangeEditMode(null);
      } else {
        setRangeEditMode(active);
      }
      return;
    }
  }

  if (active.classList.contains("game-card")) {
    const playButton = active.querySelector<HTMLButtonElement>(".game-card-play-button");
    if (playButton && !playButton.disabled) {
      playButton.click();
      return;
    }
  }

  active.click();
}

function triggerBackAction(onBackAction?: () => boolean): void {
  const openRegionToggle = document.querySelector<HTMLButtonElement>(".region-selected.open");
  if (openRegionToggle) {
    openRegionToggle.click();
    return;
  }

  const openLoginToggle = document.querySelector<HTMLButtonElement>(".login-select.open");
  if (openLoginToggle) {
    openLoginToggle.click();
    return;
  }

  const active = document.activeElement;
  if (active instanceof HTMLInputElement && active.type === "range" && active.classList.contains("controller-range-editing")) {
    setRangeEditMode(null);
    return;
  }
  if (active instanceof HTMLInputElement || active instanceof HTMLTextAreaElement) {
    active.blur();
    return;
  }

  if (onBackAction?.()) {
    return;
  }

  window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
}

export function useControllerNavigation({
  enabled,
  onNavigatePage,
  onBackAction,
}: UseControllerNavigationOptions): boolean {
  const [controllerConnected, setControllerConnected] = useState(false);
  const connectedRef = useRef(false);
  const frameRef = useRef<number | null>(null);

  const directionStateRef = useRef<Record<Direction, { pressed: boolean; nextRepeatAt: number }>>({
    up: { pressed: false, nextRepeatAt: 0 },
    down: { pressed: false, nextRepeatAt: 0 },
    left: { pressed: false, nextRepeatAt: 0 },
    right: { pressed: false, nextRepeatAt: 0 },
  });

  const actionStateRef = useRef({
    a: false,
    b: false,
    lb: false,
    rb: false,
  });

  useEffect(() => {
    if (!controllerConnected || !enabled) return;
    const items = listInteractiveElements();
    if (items.length === 0) return;

    const current = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    if (!current || !items.includes(current) || !isElementVisible(current) || isElementDisabled(current)) {
      setControllerFocus(items[0]);
    }
  }, [controllerConnected, enabled]);

  useEffect(() => {
    function updateConnected(next: boolean): void {
      if (connectedRef.current !== next) {
        connectedRef.current = next;
        setControllerConnected(next);
      }
    }

    const tick = (): void => {
      const pads = navigator.getGamepads ? navigator.getGamepads() : [];
      const pad = Array.from(pads).find((candidate) => candidate?.connected) ?? null;
      updateConnected(Boolean(pad));

      if (!pad || !enabled) {
        for (const state of Object.values(directionStateRef.current)) {
          state.pressed = false;
        }
        actionStateRef.current = { a: false, b: false, lb: false, rb: false };
        frameRef.current = window.requestAnimationFrame(tick);
        return;
      }

      const now = performance.now();

      const up = Boolean(pad.buttons[12]?.pressed || pad.axes[1] < -0.55);
      const down = Boolean(pad.buttons[13]?.pressed || pad.axes[1] > 0.55);
      const left = Boolean(pad.buttons[14]?.pressed || pad.axes[0] < -0.55);
      const right = Boolean(pad.buttons[15]?.pressed || pad.axes[0] > 0.55);
      const a = Boolean(pad.buttons[0]?.pressed);
      const b = Boolean(pad.buttons[1]?.pressed);
      const lb = Boolean(pad.buttons[4]?.pressed);
      const rb = Boolean(pad.buttons[5]?.pressed);
      const scopedToDocument = getFocusScopeRoot() === document;

      const handleDirection = (direction: Direction, pressed: boolean): void => {
        const state = directionStateRef.current[direction];
        if (!pressed) {
          state.pressed = false;
          return;
        }

        if (!state.pressed) {
          state.pressed = true;
          state.nextRepeatAt = now + DIRECTION_INITIAL_REPEAT_MS;
          moveFocus(direction);
          return;
        }

        if (now >= state.nextRepeatAt) {
          state.nextRepeatAt = now + DIRECTION_REPEAT_MS;
          moveFocus(direction);
        }
      };

      handleDirection("up", up);
      handleDirection("down", down);
      handleDirection("left", left);
      handleDirection("right", right);

      if (a && !actionStateRef.current.a) {
        activateFocusedElement();
      }
      if (b && !actionStateRef.current.b) {
        triggerBackAction(onBackAction);
      }
      if (scopedToDocument && lb && !actionStateRef.current.lb) {
        onNavigatePage?.("prev");
      }
      if (scopedToDocument && rb && !actionStateRef.current.rb) {
        onNavigatePage?.("next");
      }

      actionStateRef.current = { a, b, lb, rb };
      frameRef.current = window.requestAnimationFrame(tick);
    };

    frameRef.current = window.requestAnimationFrame(tick);
    return () => {
      if (frameRef.current !== null) {
        window.cancelAnimationFrame(frameRef.current);
      }
      setRangeEditMode(null);
      document.querySelectorAll<HTMLElement>(".controller-focus").forEach((node) => {
        node.classList.remove("controller-focus");
      });
    };
  }, [enabled, onBackAction, onNavigatePage]);

  return controllerConnected;
}
