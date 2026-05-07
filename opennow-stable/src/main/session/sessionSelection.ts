import type { ActiveSessionInfo } from "@shared/gfn";

const AUTO_RESUME_SESSION_STATUSES = new Set([2, 3]);
const ACTIVE_CREATE_SESSION_STATUSES = new Set([1, 2, 3]);

export function isAutoResumeReadySession(entry: ActiveSessionInfo): boolean {
  return (
    entry.serverIp != null && AUTO_RESUME_SESSION_STATUSES.has(entry.status)
  );
}

export function isActiveCreateSessionConflict(entry: ActiveSessionInfo): boolean {
  return ACTIVE_CREATE_SESSION_STATUSES.has(entry.status);
}

export function selectReadySessionToClaim(
  activeSessions: ActiveSessionInfo[],
  numericAppId: number,
): ActiveSessionInfo | null {
  return (
    activeSessions.find(
      (session) =>
        isAutoResumeReadySession(session) && session.appId === numericAppId,
    ) ??
    activeSessions.find((session) => isAutoResumeReadySession(session)) ??
    null
  );
}

export function selectLaunchingSession(
  activeSessions: ActiveSessionInfo[],
  numericAppId: number,
): ActiveSessionInfo | null {
  return (
    activeSessions.find(
      (session) =>
        session.serverIp &&
        session.appId === numericAppId &&
        session.status === 1,
    ) ??
    activeSessions.find(
      (session) => session.serverIp && session.status === 1,
    ) ??
    null
  );
}
