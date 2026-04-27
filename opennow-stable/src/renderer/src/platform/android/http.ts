import { CapacitorHttp, type HttpOptions, type HttpResponse } from "@capacitor/core";

export class NativeHttpError extends Error {
  constructor(
    public readonly status: number,
    public readonly body: string,
    public readonly data: HttpResponse["data"],
  ) {
    super(`HTTP ${status}: ${(body || "<empty body>").slice(0, 400)}`);
    this.name = "NativeHttpError";
  }
}

export function isNativeHttpError(error: unknown): error is NativeHttpError {
  return error instanceof NativeHttpError;
}

function normalizeHeaders(headers?: Record<string, string>): Record<string, string> {
  return Object.fromEntries(
    Object.entries(headers ?? {}).filter((entry): entry is [string, string] => typeof entry[1] === "string"),
  );
}

function formatResponseBody(data: HttpResponse["data"]): string {
  if (typeof data === "string") {
    return data;
  }
  if (data == null) {
    return "";
  }
  try {
    return JSON.stringify(data);
  } catch {
    return String(data);
  }
}

async function parseResponse<T>(response: HttpResponse, responseType: "json" | "text"): Promise<T> {
  if (response.status < 200 || response.status >= 300) {
    const body = formatResponseBody(response.data);
    throw new NativeHttpError(response.status, body, response.data);
  }

  if (responseType === "text") {
    if (typeof response.data === "string") return response.data as T;
    return JSON.stringify(response.data) as T;
  }

  if (typeof response.data === "string") {
    if (response.data.trim().length === 0) {
      return undefined as T;
    }
    return JSON.parse(response.data) as T;
  }
  return response.data as T;
}

export async function nativeRequest<T>(options: HttpOptions, responseType: "json" | "text" = "json"): Promise<T> {
  const response = await CapacitorHttp.request({
    ...options,
    headers: normalizeHeaders(options.headers),
    responseType,
  });
  return parseResponse<T>(response, responseType);
}
