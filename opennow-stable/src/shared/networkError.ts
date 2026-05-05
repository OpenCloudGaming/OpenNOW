/**
 * Format Node/network error chains for logs and IPC. Undici `fetch` often throws
 * `TypeError: fetch failed` with real details on `error.cause` (DNS, TLS, etc.).
 */

const MAX_CAUSE_DEPTH = 8;

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

function errnoExtras(err: Error): string {
  const r = err as Error &
    Partial<{
      code: string;
      errno: number;
      syscall: string;
      path: string;
      address: string;
      port: number;
      hostname: string;
    }>;

  const parts: string[] = [];
  if (typeof r.code === "string" && r.code.length > 0) parts.push(`code=${r.code}`);
  if (typeof r.errno === "number" && Number.isFinite(r.errno)) parts.push(`errno=${r.errno}`);
  if (typeof r.syscall === "string" && r.syscall.length > 0) parts.push(`syscall=${r.syscall}`);
  if (typeof r.path === "string" && r.path.length > 0) parts.push(`path=${r.path}`);
  if (typeof r.address === "string" && r.address.length > 0) parts.push(`address=${r.address}`);
  if (typeof r.port === "number" && Number.isFinite(r.port)) parts.push(`port=${r.port}`);
  if (typeof r.hostname === "string" && r.hostname.length > 0) parts.push(`hostname=${r.hostname}`);

  return parts.length > 0 ? ` [${parts.join(", ")}]` : "";
}

function formatOneLevel(err: Error): string {
  const msg = err.message?.trim() ? err.message : "(no message)";
  return `${err.name}: ${msg}${errnoExtras(err)}`;
}

function nextCause(current: unknown): unknown {
  if (!(current instanceof Error)) return undefined;
  const { cause } = current;
  if (cause === undefined || cause === null) return undefined;
  if (cause instanceof Error) return cause;
  if (typeof cause === "string") return cause;
  if (typeof cause === "number" || typeof cause === "boolean" || typeof cause === "bigint") return String(cause);
  if (isRecord(cause) && typeof cause.message === "string") {
    return `${cause.name && typeof cause.name === "string" ? `${cause.name}: ` : ""}${cause.message}`;
  }
  try {
    return JSON.stringify(cause);
  } catch {
    return String(cause);
  }
}

/**
 * Multi-line string: outer error first, then indented `caused by:` lines.
 * Safe for log export and console output.
 */
export function formatErrorChainForLog(error: unknown): string {
  if (error === undefined) return "undefined";
  if (error === null) return "null";
  if (!(error instanceof Error)) {
    if (typeof error === "string") return error;
    try {
      return JSON.stringify(error);
    } catch {
      return String(error);
    }
  }

  const lines: string[] = [];
  const seen = new WeakSet<Error>();

  let current: unknown = error;
  let depth = 0;

  while (depth < MAX_CAUSE_DEPTH && current != null) {
    if (current instanceof Error) {
      if (seen.has(current)) {
        lines.push(`${"  ".repeat(depth)}(cycle — ${current.name})`);
        break;
      }
      seen.add(current);
      const prefix = depth === 0 ? "" : `${"  ".repeat(depth)}caused by: `;
      lines.push(`${prefix}${formatOneLevel(current)}`);
      current = nextCause(current);
    } else if (typeof current === "string") {
      lines.push(`${"  ".repeat(depth)}caused by: ${current}`);
      break;
    } else {
      lines.push(`${"  ".repeat(depth)}caused by: ${String(current)}`);
      break;
    }
    depth++;
  }

  if (depth >= MAX_CAUSE_DEPTH && current instanceof Error) {
    lines.push(`${"  ".repeat(depth)}… (max depth ${MAX_CAUSE_DEPTH})`);
  }

  return lines.join("\n");
}

function chainToOneLine(chain: string): string {
  return chain.replace(/\s*\n+\s*/g, " | ").replace(/\s+/g, " ").trim();
}

/**
 * Returns an Error whose `message` includes the full cause chain in one line, so IPC/electron
 * does not drop `cause`. If there is no cause, returns the original Error instance.
 */
export function enrichErrorForIpc(error: unknown): Error {
  if (!(error instanceof Error)) {
    return new Error(chainToOneLine(formatErrorChainForLog(error)));
  }

  if (!error.cause) {
    return error;
  }

  return new Error(chainToOneLine(formatErrorChainForLog(error)));
}
