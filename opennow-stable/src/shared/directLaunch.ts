export interface DirectLaunchArgs {
  appId?: string;
  title?: string;
}

const APP_ID_FLAGS = new Set(["--launch-app-id", "--app-id"]);
const TITLE_FLAGS = new Set(["--launch-title", "--launch-game", "--game-title", "--game"]);

function normalizeFlagValue(value: string | undefined): string | undefined {
  const trimmed = value?.trim();
  if (!trimmed) return undefined;
  if (
    (trimmed.startsWith("\"") && trimmed.endsWith("\"")) ||
    (trimmed.startsWith("'") && trimmed.endsWith("'"))
  ) {
    const unquoted = trimmed.slice(1, -1).trim();
    return unquoted || undefined;
  }
  return trimmed;
}

function readFlagValue(args: readonly string[], index: number, inlineValue?: string): string | undefined {
  const normalizedInline = normalizeFlagValue(inlineValue);
  if (normalizedInline) return normalizedInline;

  const next = args[index + 1];
  if (!next || next.startsWith("--")) return undefined;
  return normalizeFlagValue(next);
}

function splitFlag(arg: string): { flag: string; inlineValue?: string } {
  const separatorIndex = arg.indexOf("=");
  if (separatorIndex === -1) {
    return { flag: arg };
  }
  return {
    flag: arg.slice(0, separatorIndex),
    inlineValue: arg.slice(separatorIndex + 1),
  };
}

export function parseDirectLaunchArgs(argv: readonly string[]): DirectLaunchArgs | null {
  let appId: string | undefined;
  let title: string | undefined;

  for (let index = 0; index < argv.length; index++) {
    const { flag, inlineValue } = splitFlag(argv[index] ?? "");
    if (APP_ID_FLAGS.has(flag)) {
      const value = readFlagValue(argv, index, inlineValue);
      if (value && /^\d+$/.test(value)) {
        appId = value;
      }
      continue;
    }

    if (TITLE_FLAGS.has(flag)) {
      const value = readFlagValue(argv, index, inlineValue);
      if (value) {
        title = value;
      }
    }
  }

  if (!appId && !title) return null;
  return { appId, title };
}
