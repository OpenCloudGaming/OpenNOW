import type { JSX } from "react";
import openNowLogoMarkUrl from "../assets/opennow-logo-mark.png";

export function OpenNowLogoMark({ className }: { className?: string }): JSX.Element {
  return (
    <img
      src={openNowLogoMarkUrl}
      alt=""
      aria-hidden="true"
      className={className}
      draggable={false}
    />
  );
}
