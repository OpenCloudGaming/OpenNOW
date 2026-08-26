import {
  closeSync,
  constants,
  createReadStream,
  createWriteStream,
  mkdtempSync,
  openSync,
  rmSync,
  type ReadStream,
  type WriteStream,
} from "node:fs";
import { spawn, spawnSync, type ChildProcessWithoutNullStreams } from "node:child_process";
import { tmpdir } from "node:os";
import { dirname, extname, join } from "node:path";

export interface MacOSLaunchServicesProcess {
  child: ChildProcessWithoutNullStreams;
  stdin: WriteStream;
  stdout: ReadStream;
  stderr: ReadStream;
  cleanup(): void;
}

function resolveBundlePath(executablePath: string): string {
  const bundlePath = dirname(dirname(dirname(executablePath)));
  if (extname(bundlePath).toLowerCase() !== ".app") {
    throw new Error(
      `The macOS native streamer must be an app-bundled executable, received: ${executablePath}`,
    );
  }
  return bundlePath;
}

function launchEnvironmentArguments(env: NodeJS.ProcessEnv): string[] {
  const args: string[] = [];
  for (const [name, value] of Object.entries(env)) {
    if (
      value !== undefined
      && (name.startsWith("OPENNOW_") || name.startsWith("RUST_"))
    ) {
      args.push("--env", `${name}=${value}`);
    }
  }
  return args;
}

/**
 * Launch the native macOS app through LaunchServices so it receives its own
 * application identity/responsibility coalition instead of inheriting
 * Electron's. Named pipes retain the existing newline-delimited JSON protocol
 * without making the streamer an Electron child process.
 */
export function launchMacOSStreamerApp(
  executablePath: string,
  env: NodeJS.ProcessEnv,
): MacOSLaunchServicesProcess {
  const bundlePath = resolveBundlePath(executablePath);
  const ipcDirectory = mkdtempSync(join(tmpdir(), "opennow-streamer-ipc-"));
  const stdinPath = join(ipcDirectory, "stdin");
  const stdoutPath = join(ipcDirectory, "stdout");
  const stderrPath = join(ipcDirectory, "stderr");
  const fifoPaths = [stdinPath, stdoutPath, stderrPath];
  const fifoResult = spawnSync("/usr/bin/mkfifo", fifoPaths, {
    encoding: "utf8",
  });
  if (fifoResult.error || fifoResult.status !== 0) {
    rmSync(ipcDirectory, { recursive: true, force: true });
    throw fifoResult.error ?? new Error(
      `Could not create macOS streamer IPC pipes: ${fifoResult.stderr.trim() || `exit ${fifoResult.status}`}`,
    );
  }

  const descriptors: number[] = [];
  let stdin: WriteStream | null = null;
  let stdout: ReadStream | null = null;
  let stderr: ReadStream | null = null;
  try {
    // O_RDWR prevents either endpoint from blocking while LaunchServices starts
    // the app and opens its corresponding side of each FIFO.
    const stdinDescriptor = openSync(stdinPath, constants.O_RDWR);
    descriptors.push(stdinDescriptor);
    const stdoutDescriptor = openSync(stdoutPath, constants.O_RDWR);
    descriptors.push(stdoutDescriptor);
    const stderrDescriptor = openSync(stderrPath, constants.O_RDWR);
    descriptors.push(stderrDescriptor);
    stdin = createWriteStream(stdinPath, { fd: stdinDescriptor, autoClose: true });
    stdout = createReadStream(stdoutPath, { fd: stdoutDescriptor, autoClose: true });
    stderr = createReadStream(stderrPath, { fd: stderrDescriptor, autoClose: true });
  } catch (error) {
    for (const descriptor of descriptors) {
      try {
        closeSync(descriptor);
      } catch {
        // Best-effort cleanup after a partially initialized IPC directory.
      }
    }
    rmSync(ipcDirectory, { recursive: true, force: true });
    throw error;
  }

  const child = spawn("/usr/bin/open", [
    "-W",
    "-n",
    "-g",
    "--stdin",
    stdinPath,
    "--stdout",
    stdoutPath,
    "--stderr",
    stderrPath,
    ...launchEnvironmentArguments(env),
    bundlePath,
  ], {
    stdio: "pipe",
    env,
  });

  let cleanedUp = false;
  return {
    child,
    stdin,
    stdout,
    stderr,
    cleanup: () => {
      if (cleanedUp) return;
      cleanedUp = true;
      stdin.destroy();
      stdout.destroy();
      stderr.destroy();
      rmSync(ipcDirectory, { recursive: true, force: true });
    },
  };
}
