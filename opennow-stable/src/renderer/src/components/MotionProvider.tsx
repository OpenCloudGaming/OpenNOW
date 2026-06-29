import type { JSX, PropsWithChildren } from "react";
import { LazyMotion, MotionConfig, domAnimation } from "motion/react";

export const smoothEase = [0.16, 1, 0.3, 1] as const;

export const pageTransition = {
  duration: 0.24,
  ease: smoothEase,
} as const;

/** Shared reveal timing for floating panels (stats HUD, settings modal, etc.). */
export const surfaceRevealTransition = {
  duration: 0.34,
  ease: smoothEase,
} as const;

export const panelSpring = {
  type: "spring",
  stiffness: 420,
  damping: 36,
  mass: 0.85,
} as const;

export function MotionProvider({ children }: PropsWithChildren): JSX.Element {
  return (
    <LazyMotion features={domAnimation}>
      <MotionConfig reducedMotion="user" transition={panelSpring}>
        {children}
      </MotionConfig>
    </LazyMotion>
  );
}
