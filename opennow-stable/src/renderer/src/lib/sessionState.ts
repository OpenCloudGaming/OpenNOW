import type { GameInfo, SessionInfo } from "@shared/gfn";

import type { LaunchErrorState, StreamLoadingStatus, StreamStatus } from "./appTypes";

type TranslateFunction = typeof import("../i18n").t;

export function isSessionReadyForConnect(status: number): boolean {
  return status === 2 || status === 3;
}

export function isSessionInQueue(session: SessionInfo): boolean {
  // Official client treats seat setup step 1 as queue state even when queuePosition reaches 1.
  // Fallback to queuePosition-based inference for payloads that do not expose seatSetupStep.
  if (session.seatSetupStep === 1) {
    return true;
  }
  return (session.queuePosition ?? 0) > 1;
}

export function isSessionLimitError(error: unknown): boolean {
  if (error && typeof error === "object" && "gfnErrorCode" in error) {
    const candidate = error.gfnErrorCode;
    if (typeof candidate === "number") {
      return candidate === 3237093643;
    }
  }
  if (error instanceof Error) {
    const msg = error.message.toUpperCase();
    return msg.includes("SESSION LIMIT") || msg.includes("DUPLICATE SESSION");
  }
  return false;
}

export function isInsufficientPlayabilityError(error: unknown): boolean {
  if (error && typeof error === "object" && "gfnErrorCode" in error) {
    const candidate = error.gfnErrorCode;
    if (typeof candidate === "number") {
      return candidate === 3237093718;
    }
  }
  if (error instanceof Error) {
    return error.message.toUpperCase().includes("INSUFFICIENT_PLAYABILITY");
  }
  return false;
}

export function toLoadingStatus(status: StreamStatus): StreamLoadingStatus {
  switch (status) {
    case "queue":
    case "setup":
    case "starting":
    case "connecting":
      return status;
    default:
      return "queue";
  }
}

export function toCodeLabel(code: number | undefined): string | undefined {
  if (code === undefined) return undefined;
  if (code === 3237093643) return `SessionLimitExceeded (${code})`;
  if (code === 3237093718) return `SessionInsufficientPlayabilityLevel (${code})`;
  return `GFN Error ${code}`;
}

export function extractLaunchErrorCode(error: unknown): number | undefined {
  if (error && typeof error === "object") {
    if ("gfnErrorCode" in error) {
      const directCode = error.gfnErrorCode;
      if (typeof directCode === "number") return directCode;
    }
    if ("statusCode" in error) {
      const statusCode = error.statusCode;
      if (typeof statusCode === "number" && statusCode > 0 && statusCode < 255) {
        return 3237093632 + statusCode;
      }
    }
  }
  if (error instanceof Error) {
    const match = error.message.match(/\b(3237\d{6,})\b/);
    if (match) {
      const code = Number(match[1]);
      if (Number.isFinite(code)) return code;
    }
  }
  return undefined;
}

function firstText(value: string | string[] | undefined): string {
  if (typeof value === "string") return value.trim();
  if (Array.isArray(value)) {
    return value.find((entry) => typeof entry === "string" && entry.trim().length > 0)?.trim() ?? "";
  }
  return "";
}

function formatCatalogSkuString(template: string, sku: string): string {
  return template.replace(/\{\{\s*SKU\s*\}\}|\{\s*SKU\s*\}/g, sku).trim();
}

function toInsufficientPlayabilityState(
  t: TranslateFunction,
  stage: StreamLoadingStatus,
  code: number | undefined,
  game?: Pick<GameInfo, "title" | "membershipTierLabel" | "catalogSkuStrings"> | null,
): LaunchErrorState {
  const catalogHeader = firstText(game?.catalogSkuStrings?.SKU_BASED_UNPLAYABLE_DIALOG_HEADER);
  const catalogBody = firstText(game?.catalogSkuStrings?.SKU_BASED_UNPLAYABLE_DIALOG_BODY_UPGRADE);
  const tier = game?.membershipTierLabel?.trim();
  const title = catalogHeader || t("errors.insufficientPlayabilityTitle");
  const description = catalogBody
    ? formatCatalogSkuString(catalogBody, catalogHeader || tier || t("errors.insufficientPlayabilityTitle"))
    : tier
      ? t("errors.insufficientPlayabilityTierDescription", { tier })
      : t("errors.insufficientPlayabilityDescription");

  return {
    stage,
    title,
    description,
    codeLabel: toCodeLabel(code),
  };
}

export function toLaunchErrorState(
  t: TranslateFunction,
  error: unknown,
  stage: StreamLoadingStatus,
  game?: Pick<GameInfo, "title" | "membershipTierLabel" | "catalogSkuStrings"> | null,
): LaunchErrorState {
  const unknownMessage = t("errors.launchUnknown");

  const titleFromError =
    error && typeof error === "object" && "title" in error && typeof error.title === "string"
      ? error.title.trim()
      : "";
  const descriptionFromError =
    error && typeof error === "object" && "description" in error && typeof error.description === "string"
      ? error.description.trim()
      : "";
  const statusDescription =
    error && typeof error === "object" && "statusDescription" in error && typeof error.statusDescription === "string"
      ? error.statusDescription.trim()
      : "";
  const messageFromError = error instanceof Error ? error.message.trim() : "";
  const combined = `${statusDescription} ${messageFromError}`.toUpperCase();
  const code = extractLaunchErrorCode(error);

  if (isInsufficientPlayabilityError(error) || combined.includes("INSUFFICIENT_PLAYABILITY")) {
    return toInsufficientPlayabilityState(t, stage, code, game);
  }

  if (
    isSessionLimitError(error) ||
    combined.includes("SESSION_LIMIT") ||
    combined.includes("DUPLICATE SESSION")
  ) {
    return {
      stage,
      title: t("errors.duplicateSessionTitle"),
      description: t("errors.duplicateSessionDescription"),
      codeLabel: toCodeLabel(code),
    };
  }

  return {
    stage,
    title: titleFromError || t("errors.launchFailedTitle"),
    description: descriptionFromError || messageFromError || statusDescription || unknownMessage,
    codeLabel: toCodeLabel(code),
  };
}

export function streamStatusToLoadingStage(status: StreamStatus): StreamLoadingStatus {
  if (status === "queue" || status === "setup" || status === "starting" || status === "connecting") {
    return status;
  }
  return "connecting";
}
