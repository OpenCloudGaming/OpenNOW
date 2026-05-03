import type { JSX } from "react";

export interface Ps5ThreeDotsProps {
  /** Default matches PS5 system loader scale; `lg` for full-screen */
  size?: "md" | "lg";
  className?: string;
}

/**
 * PS5 system-style loader: three horizontal white dots with a sequential pulse
 * (matches the console “please wait” indicator, not a themed accent ring).
 */
export function Ps5ThreeDots({ size = "md", className = "" }: Ps5ThreeDotsProps): JSX.Element {
  const dim = size === "lg" ? "ps5-load-dots--lg" : "";
  return (
    <div className={`ps5-load-dots ${dim} ${className}`.trim()} aria-hidden>
      <span className="ps5-load-dot" />
      <span className="ps5-load-dot" />
      <span className="ps5-load-dot" />
    </div>
  );
}

/** @deprecated Alias for {@link Ps5ThreeDots} — PS5 uses three dots, not spinner arcs */
export const Ps5LoadingSpinner = Ps5ThreeDots;
export type Ps5LoadingSpinnerProps = Ps5ThreeDotsProps;

export interface Ps5LoadingScreenProps {
  /** Used for `aria-label` */
  title?: string;
  /** Optional second line under the dots (hybrid PS5 + minimal copy) */
  subtitle?: string;
  /** Optional blurred backdrop (e.g. game art), heavily darkened like the console */
  backdropImageUrl?: string | null;
  className?: string;
}

/**
 * Full-viewport PS5-style wait: black field, optional dimmed art, three-dot loader,
 * optional visible subtitle under dots (matches controller stream loading hybrid).
 */
export function Ps5LoadingScreen({
  title = "Loading",
  subtitle,
  backdropImageUrl,
  className = "",
}: Ps5LoadingScreenProps): JSX.Element {
  const announced = subtitle ? `${title}. ${subtitle}` : title;

  return (
    <div
      className={`ps5-load-screen ${backdropImageUrl ? "ps5-load-screen--art" : ""} ${className}`.trim()}
      role="status"
      aria-live="polite"
      aria-busy="true"
      aria-label={announced}
    >
      {backdropImageUrl ? (
        <div className="ps5-load-screen-hero" style={{ backgroundImage: `url(${backdropImageUrl})` }} aria-hidden />
      ) : null}

      <div className="ps5-load-screen-scrim" aria-hidden />

      <div className="ps5-load-screen-center">
        <Ps5ThreeDots size="lg" />
        <div className="ps5-load-screen-text-stack">
          <p className="ps5-load-screen-heading">{title}</p>
          {subtitle ? <p className="ps5-load-screen-status">{subtitle}</p> : null}
        </div>
      </div>
    </div>
  );
}
