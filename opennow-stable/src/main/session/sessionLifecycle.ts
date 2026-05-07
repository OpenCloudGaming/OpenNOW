import { stopSession, getActiveSessions } from "../gfn/cloudmatch";
import { isActiveCreateSessionConflict } from "./sessionSelection";

export async function stopActiveSessionsForCreate(params: {
  token: string;
  streamingBaseUrl: string;
  zone: string;
  appId: string;
}): Promise<void> {
  const { token, streamingBaseUrl, zone, appId } = params;
  const numericAppId = Number.parseInt(appId, 10);
  const activeSessions = await getActiveSessions(token, streamingBaseUrl);
  const sessionsToStop = activeSessions.filter(isActiveCreateSessionConflict);
  if (sessionsToStop.length === 0) {
    return;
  }

  console.log(
    `[CreateSession] Force-new requested; stopping ${sessionsToStop.length} existing active session(s) before create.`,
  );

  for (const activeSession of sessionsToStop) {
    if (!activeSession.serverIp) {
      console.warn(
        `[CreateSession] Cannot stop existing session ${activeSession.sessionId} (appId=${activeSession.appId}, status=${activeSession.status}) because serverIp is missing.`,
      );
      continue;
    }
    console.log(
      `[CreateSession] Stopping existing session id=${activeSession.sessionId}, appId=${activeSession.appId}, status=${activeSession.status}` +
        `${activeSession.appId === numericAppId ? " (same app)" : ""}.`,
    );
    await stopSession({
      token,
      streamingBaseUrl,
      serverIp: activeSession.serverIp,
      zone,
      sessionId: activeSession.sessionId,
    });
  }
}
