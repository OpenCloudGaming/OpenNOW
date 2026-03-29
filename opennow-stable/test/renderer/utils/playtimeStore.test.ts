import { describe, expect, it } from "vitest";

import {
  endPlaytimeSession,
  formatLastPlayed,
  formatPlaytime,
  loadPlaytimeStore,
  startPlaytimeSession,
} from "../../../src/renderer/src/utils/playtimeStore";

describe("playtime store helpers", () => {
  it("formats playtime durations", () => {
    expect(formatPlaytime(0)).toBe("Never played");
    expect(formatPlaytime(59)).toBe("< 1 min");
    expect(formatPlaytime(60)).toBe("1 m");
    expect(formatPlaytime(3600)).toBe("1 h");
    expect(formatPlaytime(3660)).toBe("1 h 1 m");
  });

  it("formats last played dates relative to a provided current date", () => {
    const now = new Date("2026-03-29T12:00:00.000Z");

    expect(formatLastPlayed(null, now)).toBe("Never");
    expect(formatLastPlayed("2026-03-29T01:00:00.000Z", now)).toBe("Today");
    expect(formatLastPlayed("2026-03-28T01:00:00.000Z", now)).toBe("Yesterday");
    expect(formatLastPlayed("2026-03-25T01:00:00.000Z", now)).toBe("4 days ago");
    expect(formatLastPlayed("2026-03-01T01:00:00.000Z", now)).toBe("4 wk ago");
  });

  it("parses invalid storage payloads safely", () => {
    expect(loadPlaytimeStore({ getItem: () => null })).toEqual({});
    expect(loadPlaytimeStore({ getItem: () => "{invalid" })).toEqual({});
  });

  it("accumulates session metadata and elapsed time deterministically", () => {
    const started = startPlaytimeSession({}, "game-1", Date.parse("2026-03-29T12:00:00.000Z"));

    expect(started).toEqual({
      "game-1": {
        totalSeconds: 0,
        lastPlayedAt: "2026-03-29T12:00:00.000Z",
        sessionCount: 1,
      },
    });

    expect(endPlaytimeSession(started, "game-1", 1_000, 4_999)).toEqual({
      "game-1": {
        totalSeconds: 3,
        lastPlayedAt: "2026-03-29T12:00:00.000Z",
        sessionCount: 1,
      },
    });
    expect(endPlaytimeSession(started, "game-1", 1_000, 1_999)).toBe(started);
  });
});
