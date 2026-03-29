import { describe, expect, it } from "vitest";

import {
  createRedactedLogExport,
  formatLogEntry,
  redactSensitiveData,
} from "../../src/shared/logger";

describe("logger utilities", () => {
  it("redacts sensitive values including tokens, emails, ips, and passwords", () => {
    const redacted = redactSensitiveData(
      'user=test@example.com Authorization: Bearer abcdefghijklmnopqrstuvwxyz password=secret123 ip=203.0.113.5',
    );

    expect(redacted).toContain("[Redacted for privacy]");
    expect(redacted).toContain("Authorization: [Redacted for privacy]");
    expect(redacted).toContain("password: [Redacted for privacy]");
    expect(redacted).toContain("[Redacted IP]");
    expect(redacted).not.toContain("test@example.com");
    expect(redacted).not.toContain("203.0.113.5");
  });

  it("formats log entries and produces redacted exports", () => {
    const entry = {
      timestamp: Date.parse("2026-03-29T12:00:00.000Z"),
      level: "warn" as const,
      prefix: "Auth",
      message: "session created",
      args: [{ client_token: "abcdefghijklmnopqrstuvwxyz123456" }, "user@example.com"],
    };

    expect(formatLogEntry(entry)).toBe(
      '2026-03-29T12:00:00.000Z  WARN [Auth] session created {"client_token":"abcdefghijklmnopqrstuvwxyz123456"} user@example.com',
    );

    const exported = createRedactedLogExport([entry]);
    expect(exported).toContain("2026-03-29T12:00:00.000Z  WARN [Auth] session created");
    expect(exported).not.toContain("abcdefghijklmnopqrstuvwxyz123456");
    expect(exported).not.toContain("user@example.com");
  });
});
