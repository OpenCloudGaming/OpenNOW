import type { PersistentStorageResetResult } from "@shared/gfn";

const GFN_PAYWALL_API_BASE_URL = "https://api-prod.nvidia.com/gfn-paywall-api/api/v2";

interface ResetPersistentStorageInput {
  idToken: string;
  storageRegion?: string | null;
}

interface PaywallResponseWithMessage {
  message?: unknown;
}

function normalizeStorageRegion(storageRegion: string | null | undefined): string | null {
  if (typeof storageRegion !== "string") {
    return null;
  }

  const trimmed = storageRegion.trim();
  return trimmed.length > 0 ? trimmed : null;
}

function buildResetStorageUrl(storageRegion: string | null): string {
  const regionQueryValue = storageRegion ?? "null";
  return `${GFN_PAYWALL_API_BASE_URL}/reset/storage?storageRegion=${encodeURIComponent(regionQueryValue)}`;
}

function buildPaywallHeaders(idToken: string): Record<string, string> {
  return {
    Accept: "application/json",
    "Content-Type": "application/json",
    idToken,
  };
}

function parseMessage(payload: unknown): string | undefined {
  if (!payload || typeof payload !== "object") {
    return undefined;
  }

  const message = (payload as PaywallResponseWithMessage).message;
  return typeof message === "string" && message.trim().length > 0 ? message : undefined;
}

async function readPaywallJson(response: Response): Promise<unknown> {
  const text = await response.text();
  if (!text.trim()) {
    return null;
  }

  try {
    return JSON.parse(text) as unknown;
  } catch {
    return text;
  }
}

export async function resetPersistentStorage(
  input: ResetPersistentStorageInput,
): Promise<PersistentStorageResetResult> {
  const storageRegion = normalizeStorageRegion(input.storageRegion);
  const response = await fetch(buildResetStorageUrl(storageRegion), {
    method: "POST",
    headers: buildPaywallHeaders(input.idToken),
    body: null,
  });

  const payload = await readPaywallJson(response);
  if (!response.ok) {
    const message = parseMessage(payload) ?? `Persistent storage reset failed with status ${response.status}`;
    throw new Error(message);
  }

  return {
    ok: true,
    storageRegion,
    message: parseMessage(payload),
  };
}
