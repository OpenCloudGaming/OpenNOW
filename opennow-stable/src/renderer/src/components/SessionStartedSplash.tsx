import { useEffect } from "react";
import { AnimatePresence, m } from "motion/react";
import { Check } from "lucide-react";
import type { JSX } from "react";
import { smoothEase } from "./MotionProvider";
import { useTranslation } from "../i18n";

const SPLASH_VISIBLE_MS = 3000;

const overlayVariants = {
  hidden: { opacity: 0 },
  visible: {
    opacity: 1,
    transition: { duration: 0.35, ease: smoothEase },
  },
  exit: {
    opacity: 0,
    transition: { duration: 0.55, ease: smoothEase, delay: 0.08 },
  },
} as const;

const backdropVariants = {
  hidden: { opacity: 0, scale: 1.04 },
  visible: {
    opacity: 1,
    scale: 1,
    transition: { duration: 0.7, ease: smoothEase },
  },
  exit: {
    opacity: 0,
    scale: 1.08,
    transition: { duration: 0.5, ease: smoothEase },
  },
} as const;

const cardVariants = {
  hidden: {
    opacity: 0,
    scale: 0.9,
    y: 32,
    filter: "blur(10px)",
  },
  visible: {
    opacity: 1,
    scale: 1,
    y: 0,
    filter: "blur(0px)",
    transition: {
      duration: 0.72,
      ease: smoothEase,
      delay: 0.06,
    },
  },
  exit: {
    opacity: 0,
    scale: 1.05,
    y: -18,
    filter: "blur(6px)",
    transition: { duration: 0.48, ease: smoothEase },
  },
} as const;

const contentVariants = {
  hidden: { opacity: 0 },
  visible: {
    opacity: 1,
    transition: {
      delayChildren: 0.22,
      staggerChildren: 0.09,
    },
  },
  exit: {
    opacity: 0,
    transition: { duration: 0.2 },
  },
} as const;

const itemVariants = {
  hidden: { opacity: 0, y: 16, filter: "blur(4px)" },
  visible: {
    opacity: 1,
    y: 0,
    filter: "blur(0px)",
    transition: { duration: 0.48, ease: smoothEase },
  },
} as const;

const emblemVariants = {
  hidden: { opacity: 0, scale: 0.55, rotate: -18 },
  visible: {
    opacity: 1,
    scale: 1,
    rotate: 0,
    transition: {
      type: "spring",
      stiffness: 340,
      damping: 22,
      mass: 0.85,
    },
  },
  exit: {
    opacity: 0,
    scale: 1.12,
    transition: { duration: 0.3, ease: smoothEase },
  },
} as const;

export interface SessionStartedSplashProps {
  visible: boolean;
  gameTitle: string;
  onFinished: () => void;
}

export function SessionStartedSplash({
  visible,
  gameTitle,
  onFinished,
}: SessionStartedSplashProps): JSX.Element | null {
  const { t } = useTranslation();

  useEffect(() => {
    if (!visible) {
      return undefined;
    }
    const timer = window.setTimeout(onFinished, SPLASH_VISIBLE_MS);
    return () => window.clearTimeout(timer);
  }, [onFinished, visible]);

  return (
    <AnimatePresence mode="wait">
      {visible && (
        <m.div
          className="sv-ready-splash"
          role="status"
          aria-live="polite"
          variants={overlayVariants}
          initial="hidden"
          animate="visible"
          exit="exit"
        >
          <m.div
            className="sv-ready-splash-backdrop"
            variants={backdropVariants}
            initial="hidden"
            animate="visible"
            exit="exit"
          >
            <span className="sv-ready-splash-orb sv-ready-splash-orb--a" aria-hidden />
            <span className="sv-ready-splash-orb sv-ready-splash-orb--b" aria-hidden />
            <span className="sv-ready-splash-orb sv-ready-splash-orb--c" aria-hidden />
          </m.div>

          <m.div
            className="sv-ready-splash-card"
            variants={cardVariants}
            initial="hidden"
            animate="visible"
            exit="exit"
          >
            <span className="sv-ready-splash-card-glow" aria-hidden />
            <span className="sv-ready-splash-card-shimmer" aria-hidden />

            <m.div
              className="sv-ready-splash-emblem"
              variants={emblemVariants}
              aria-hidden
            >
              <span className="sv-ready-splash-ring sv-ready-splash-ring--outer" />
              <span className="sv-ready-splash-ring sv-ready-splash-ring--mid" />
              <span className="sv-ready-splash-ring sv-ready-splash-ring--inner" />
              <span className="sv-ready-splash-emblem-core">
                <Check size={28} strokeWidth={2.5} />
              </span>
            </m.div>

            <m.div
              className="sv-ready-splash-copy"
              variants={contentVariants}
              initial="hidden"
              animate="visible"
              exit="exit"
            >
              <m.p className="sv-ready-splash-kicker" variants={itemVariants}>
                {t("stream.sessionStarted.kicker")}
              </m.p>
              <m.h2 className="sv-ready-splash-title" variants={itemVariants}>
                {t("stream.sessionStarted.title")}
              </m.h2>
              <m.p className="sv-ready-splash-game" variants={itemVariants}>
                {gameTitle}
              </m.p>
            </m.div>

            <div
              className="sv-ready-splash-progress"
              aria-hidden
              style={{ animationDuration: `${SPLASH_VISIBLE_MS}ms` }}
            />
          </m.div>
        </m.div>
      )}
    </AnimatePresence>
  );
}
