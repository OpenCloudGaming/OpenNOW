export const GFN_PAYWALL_API_BASE_URL = "https://api-prod.nvidia.com/gfn-paywall-api/api/v2";

interface PaywallResponseWithMessage {
  message?: unknown;
  errors?: {
    errorMessage?: unknown;
  };
}

export function buildPaywallHeaders(idToken: string): Record<string, string> {
  return {
    Accept: "application/json",
    "Content-Type": "application/json",
    idToken,
  };
}

export async function readPaywallJson(response: Response): Promise<unknown> {
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

export function parsePaywallMessage(payload: unknown): string | undefined {
  if (!payload || typeof payload !== "object") {
    return undefined;
  }

  const response = payload as PaywallResponseWithMessage;
  const statuspageMessage = (payload as { error?: { message?: unknown } }).error?.message;
  const message = response.message ?? response.errors?.errorMessage ?? statuspageMessage;
  return typeof message === "string" && message.trim().length > 0 ? message : undefined;
}

export function tokenCandidates(idToken: string, alternates: string[] | undefined): string[] {
  return [...new Set([idToken, ...(alternates ?? [])].map((token) => token.trim()).filter(Boolean))];
}

export function resolvePaywallTokenCandidates(
  idToken: string,
  alternates: string[] | undefined,
): string[] {
  return tokenCandidates(idToken, alternates);
}
