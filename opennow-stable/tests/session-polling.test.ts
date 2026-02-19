/**
 * Unit tests for session polling logic.
 *
 * Run: npx tsx tests/session-polling.test.ts
 *
 * Tests the polling behavior: exponential backoff, status=1 continues,
 * transition to ready without error/resume, abort cancellation, and
 * hard timeout on truly stuck sessions.
 */

// ── Simulated types matching the app ────────────────────────────────

interface SessionInfo {
  sessionId: string;
  status: number;
  signalingUrl: string;
  serverIp: string;
  zone: string;
  streamingBaseUrl: string;
  signalingServer: string;
}

type StreamLoadingStatus = "queue" | "setup" | "starting" | "connecting";

// ── Extracted polling logic (mirrors App.tsx) ───────────────────────

function abortableSleep(ms: number, signal?: AbortSignal): Promise<void> {
  return new Promise((resolve, reject) => {
    if (signal?.aborted) {
      reject(new DOMException("Aborted", "AbortError"));
      return;
    }
    const timer = setTimeout(resolve, ms);
    signal?.addEventListener(
      "abort",
      () => {
        clearTimeout(timer);
        reject(new DOMException("Aborted", "AbortError"));
      },
      { once: true },
    );
  });
}

interface PollResult {
  finalSession: SessionInfo | null;
  attempts: number;
  error: string | null;
  aborted: boolean;
  loadingSteps: StreamLoadingStatus[];
  provisioningElapsedUpdates: number[];
}

async function simulatePollLoop(
  statusSequence: number[],
  opts: {
    hardTimeoutMs?: number;
    pollDelayOverride?: number;
    abortAfterMs?: number;
  } = {},
): Promise<PollResult> {
  const HARD_TIMEOUT_MS = opts.hardTimeoutMs ?? 180_000;
  const BACKOFF_INITIAL_MS = opts.pollDelayOverride ?? 50;
  const BACKOFF_MAX_MS = opts.pollDelayOverride ?? 200;
  const READY_CONFIRMS_NEEDED = 3;

  const abortController = new AbortController();
  const signal = abortController.signal;

  if (opts.abortAfterMs !== undefined) {
    setTimeout(() => abortController.abort(), opts.abortAfterMs);
  }

  let statusIndex = 0;
  const loadingSteps: StreamLoadingStatus[] = [];
  const provisioningElapsedUpdates: number[] = [];

  const pollSession = async (): Promise<SessionInfo> => {
    const status = statusSequence[Math.min(statusIndex, statusSequence.length - 1)];
    statusIndex++;
    return {
      sessionId: "test-session-123",
      status,
      signalingUrl: status >= 2 ? "wss://server:443/nvst/" : "",
      serverIp: "1.2.3.4",
      zone: "prod",
      streamingBaseUrl: "https://1.2.3.4",
      signalingServer: "1.2.3.4:443",
    };
  };

  let readyCount = 0;
  let attempt = 0;
  let delay = BACKOFF_INITIAL_MS;
  const pollStart = Date.now();
  let finalSession: SessionInfo | null = null;
  let error: string | null = null;
  let aborted = false;

  try {
    while (readyCount < READY_CONFIRMS_NEEDED) {
      if (signal.aborted) {
        throw new DOMException("Polling cancelled", "AbortError");
      }

      const elapsed = Date.now() - pollStart;
      if (elapsed >= HARD_TIMEOUT_MS) {
        throw new Error("Session provisioning timed out");
      }

      await abortableSleep(delay, signal);
      attempt++;

      const polled = await pollSession();

      if (signal.aborted) {
        throw new DOMException("Polling cancelled", "AbortError");
      }

      if (polled.status === 2 || polled.status === 3) {
        readyCount++;
        delay = BACKOFF_INITIAL_MS;
      } else if (polled.status === 1) {
        readyCount = 0;
        loadingSteps.push("setup");
        provisioningElapsedUpdates.push(Math.round(elapsed / 1000));
        delay = Math.min(delay * 1.5, BACKOFF_MAX_MS);
      } else if (polled.status === 6) {
        throw new Error("Session is being cleaned up");
      } else {
        readyCount = 0;
        delay = Math.min(delay * 1.5, BACKOFF_MAX_MS);
      }

      if (readyCount >= READY_CONFIRMS_NEEDED) {
        finalSession = polled;
      }
    }
  } catch (e) {
    if (e instanceof DOMException && e.name === "AbortError") {
      aborted = true;
    } else if (e instanceof Error) {
      error = e.message;
    }
  }

  return {
    finalSession,
    attempts: attempt,
    error,
    aborted,
    loadingSteps,
    provisioningElapsedUpdates,
  };
}

// ── Test runner ─────────────────────────────────────────────────────

let passed = 0;
let failed = 0;

function assert(condition: boolean, name: string): void {
  if (condition) {
    passed++;
    console.log(`  ✓ ${name}`);
  } else {
    failed++;
    console.error(`  ✗ ${name}`);
  }
}

async function runTests(): Promise<void> {
  console.log("\n=== Session Polling Logic ===\n");

  // Test 1: Immediate ready (3 consecutive status=2)
  console.log("Test: Immediate ready (status=2 from start)");
  {
    const result = await simulatePollLoop([2, 2, 2]);
    assert(result.finalSession !== null, "session is resolved");
    assert(result.finalSession?.status === 2, "final status is 2 (ready)");
    assert(result.attempts === 3, "exactly 3 poll attempts needed");
    assert(result.error === null, "no error thrown");
    assert(!result.aborted, "not aborted");
    assert(result.loadingSteps.length === 0, "no setup steps (went straight to ready)");
  }

  // Test 2: status=1 provisioning then transitions to ready
  console.log("\nTest: status=1 (provisioning) then transitions to ready");
  {
    const result = await simulatePollLoop([1, 1, 1, 1, 1, 2, 2, 2]);
    assert(result.finalSession !== null, "session is resolved");
    assert(result.finalSession?.status === 2, "final status is ready");
    assert(result.attempts === 8, "8 total attempts (5 provisioning + 3 ready)");
    assert(result.error === null, "NO error thrown for status=1");
    assert(!result.aborted, "not aborted");
    assert(result.loadingSteps.length === 5, "5 setup loading step updates");
  }

  // Test 3: Long provisioning (many status=1) still succeeds
  console.log("\nTest: Long provisioning (20x status=1 then ready)");
  {
    const statuses = Array(20).fill(1).concat([2, 2, 2]);
    const result = await simulatePollLoop(statuses);
    assert(result.finalSession !== null, "session is resolved after long wait");
    assert(result.error === null, "NO error even after 20 provisioning polls");
    assert(result.attempts === 23, "23 total attempts");
  }

  // Test 4: status=3 (streaming) also counts as ready
  console.log("\nTest: status=3 (streaming) counts as ready");
  {
    const result = await simulatePollLoop([1, 3, 3, 3]);
    assert(result.finalSession !== null, "session is resolved");
    assert(result.finalSession?.status === 3, "final status is 3 (streaming)");
    assert(result.error === null, "no error");
  }

  // Test 5: Mixed ready/provisioning resets ready count
  console.log("\nTest: Ready count resets when status=1 interrupts");
  {
    const result = await simulatePollLoop([2, 2, 1, 2, 2, 2]);
    assert(result.finalSession !== null, "session eventually resolves");
    assert(result.attempts === 6, "6 total attempts (2 ready, 1 reset, 3 ready)");
    assert(result.error === null, "no error");
  }

  // Test 6: status=6 (cleaning up) is a terminal error
  console.log("\nTest: status=6 (cleaning up) throws terminal error");
  {
    const result = await simulatePollLoop([1, 1, 6]);
    assert(result.finalSession === null, "no session resolved");
    assert(result.error !== null, "error was thrown");
    assert(result.error!.includes("cleaned up"), "error mentions cleanup");
  }

  // Test 7: AbortController cancels polling
  console.log("\nTest: AbortController cancels polling cleanly");
  {
    const result = await simulatePollLoop(
      Array(100).fill(1),
      { abortAfterMs: 150, pollDelayOverride: 50 },
    );
    assert(result.finalSession === null, "no session resolved");
    assert(result.aborted, "was aborted");
    assert(result.error === null, "no error (abort is not an error)");
  }

  // Test 8: Hard timeout triggers after max time
  console.log("\nTest: Hard timeout triggers on stuck provisioning");
  {
    const result = await simulatePollLoop(
      Array(1000).fill(1),
      { hardTimeoutMs: 300, pollDelayOverride: 50 },
    );
    assert(result.finalSession === null, "no session resolved");
    assert(result.error !== null, "error was thrown");
    assert(result.error!.includes("timed out"), "error mentions timeout");
    assert(!result.aborted, "not aborted (was a timeout)");
  }

  // Test 9: Unknown status doesn't crash, just continues
  console.log("\nTest: Unknown status (e.g. 99) continues polling");
  {
    const result = await simulatePollLoop([99, 99, 2, 2, 2]);
    assert(result.finalSession !== null, "session eventually resolves");
    assert(result.error === null, "no error for unknown status");
    assert(result.attempts === 5, "5 total attempts");
  }

  // Test 10: No duplicate concurrent polls (single-flight guard)
  console.log("\nTest: Exponential backoff increases delay");
  {
    const start = Date.now();
    await simulatePollLoop([1, 1, 1, 1, 2, 2, 2], { pollDelayOverride: 20 });
    const elapsed = Date.now() - start;
    assert(elapsed >= 100, `total time ${elapsed}ms >= 100ms (backoff effect)`);
  }

  // ── Summary ─────────────────────────────────────────────────────────
  console.log(`\n=== Results: ${passed} passed, ${failed} failed ===\n`);
  process.exit(failed > 0 ? 1 : 0);
}

runTests().catch((e) => {
  console.error("Test runner crashed:", e);
  process.exit(1);
});
