export interface SessionErrorInfo {
  httpStatus: number;
  statusCode: number;
  statusDescription?: string;
  unifiedErrorCode?: number;
  sessionErrorCode?: number;
  gfnErrorCode: number;
  title: string;
  description: string;
}

export const SESSION_ERROR_TRANSPORT_KIND = "opennow.session-error" as const;
const SESSION_ERROR_TRANSPORT_PREFIX = "__OPENNOW_SESSION_ERROR__:";

export interface SerializedSessionError extends SessionErrorInfo {
  kind: typeof SESSION_ERROR_TRANSPORT_KIND;
  name: "SessionError";
  message: string;
}

export function toSerializedSessionError(info: SessionErrorInfo): SerializedSessionError {
  return {
    ...info,
    kind: SESSION_ERROR_TRANSPORT_KIND,
    name: "SessionError",
    message: info.description,
  };
}

export function isSerializedSessionError(error: unknown): error is SerializedSessionError {
  return Boolean(
    error &&
      typeof error === "object" &&
      "kind" in error &&
      error.kind === SESSION_ERROR_TRANSPORT_KIND &&
      "name" in error &&
      error.name === "SessionError" &&
      "gfnErrorCode" in error &&
      typeof error.gfnErrorCode === "number" &&
      "title" in error &&
      typeof error.title === "string" &&
      "description" in error &&
      typeof error.description === "string",
  );
}

export function serializeSessionErrorTransport(info: SessionErrorInfo): string {
  return `${SESSION_ERROR_TRANSPORT_PREFIX}${JSON.stringify(toSerializedSessionError(info))}`;
}

export function parseSerializedSessionErrorTransport(message: string): SerializedSessionError | null {
  const markerIndex = message.indexOf(SESSION_ERROR_TRANSPORT_PREFIX);
  if (markerIndex < 0) {
    return null;
  }

  const payload = message.slice(markerIndex + SESSION_ERROR_TRANSPORT_PREFIX.length);

  try {
    const parsed: unknown = JSON.parse(payload);
    return isSerializedSessionError(parsed) ? parsed : null;
  } catch {
    return null;
  }
}
