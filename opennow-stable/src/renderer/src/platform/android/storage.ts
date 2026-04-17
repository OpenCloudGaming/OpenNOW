import { Directory, Encoding, Filesystem } from "@capacitor/filesystem";
import { Preferences } from "@capacitor/preferences";

const BASE_DIR = "opennow";

interface StoragePathOptions {
  relativeToBaseDir?: boolean;
}

function normalizePath(path: string, options: StoragePathOptions = {}): string {
  if (options.relativeToBaseDir === false) {
    return path;
  }
  return path.startsWith(`${BASE_DIR}/`) ? path : `${BASE_DIR}/${path}`;
}

function filesystemErrorMessage(error: unknown): string {
  if (error instanceof Error) {
    return error.message;
  }
  return String(error);
}

function isMissingFilesystemError(error: unknown): boolean {
  const message = filesystemErrorMessage(error).toLowerCase();
  return message.includes("does not exist")
    || message.includes("not found")
    || message.includes("no such file")
    || message.includes("no such directory")
    || message.includes("file_notfound")
    || message.includes("file does not exist");
}

function isAlreadyExistsFilesystemError(error: unknown): boolean {
  const message = filesystemErrorMessage(error).toLowerCase();
  return message.includes("already exists") || message.includes("file exists");
}

export async function getPreferenceJson<T>(key: string, fallback: T): Promise<T> {
  const { value } = await Preferences.get({ key });
  if (!value) return fallback;
  try {
    return JSON.parse(value) as T;
  } catch {
    return fallback;
  }
}

export async function setPreferenceJson(key: string, value: unknown): Promise<void> {
  await Preferences.set({ key, value: JSON.stringify(value) });
}

export async function removePreference(key: string): Promise<void> {
  await Preferences.remove({ key });
}

export async function ensureDir(path: string, options: StoragePathOptions = {}): Promise<void> {
  try {
    await Filesystem.mkdir({ path: normalizePath(path, options), directory: Directory.Data, recursive: true });
  } catch (error) {
    if (isAlreadyExistsFilesystemError(error)) {
      return;
    }
    throw error;
  }
}

export async function writeFile(path: string, data: string, options: StoragePathOptions = {}): Promise<void> {
  const normalized = normalizePath(path, options);
  const parent = normalized.split("/").slice(0, -1).join("/");
  if (parent) await ensureDir(parent, { relativeToBaseDir: false });
  await Filesystem.writeFile({ path: normalized, data, directory: Directory.Data, encoding: Encoding.UTF8 });
}

export async function appendFile(path: string, data: string, options: StoragePathOptions = {}): Promise<void> {
  const normalized = normalizePath(path, options);
  const parent = normalized.split("/").slice(0, -1).join("/");
  if (parent) await ensureDir(parent, { relativeToBaseDir: false });
  await Filesystem.appendFile({ path: normalized, data, directory: Directory.Data, encoding: Encoding.UTF8 });
}

export async function readFile(path: string, options: StoragePathOptions = {}): Promise<string> {
  const normalized = normalizePath(path, options);
  const result = await Filesystem.readFile({ path: normalized, directory: Directory.Data, encoding: Encoding.UTF8 });
  return typeof result.data === "string" ? result.data : "";
}

export async function readFileBase64(path: string, options: StoragePathOptions = {}): Promise<string> {
  const normalized = normalizePath(path, options);
  const result = await Filesystem.readFile({ path: normalized, directory: Directory.Data });
  return typeof result.data === "string" ? result.data : "";
}

export async function deleteFile(path: string, options: StoragePathOptions = {}): Promise<void> {
  const normalized = normalizePath(path, options);
  try {
    await Filesystem.deleteFile({ path: normalized, directory: Directory.Data });
  } catch (error) {
    if (isMissingFilesystemError(error)) {
      return;
    }
    throw error;
  }
}

export async function readDir(path: string, options: StoragePathOptions = {}): Promise<string[]> {
  const normalized = normalizePath(path, options);
  try {
    const result = await Filesystem.readdir({ path: normalized, directory: Directory.Data });
    return result.files.map((file) => file.name);
  } catch (error) {
    if (isMissingFilesystemError(error)) {
      return [];
    }
    throw error;
  }
}

async function clearDirectoryNormalized(path: string): Promise<void> {
  const entries = await Filesystem.readdir({ path, directory: Directory.Data }).catch((error) => {
    if (isMissingFilesystemError(error)) {
      return null;
    }
    throw error;
  });
  if (!entries) {
    return;
  }

  await Promise.all(entries.files.map(async (entry) => {
    const childPath = `${path}/${entry.name}`;
    if (entry.type === "directory") {
      await clearDirectoryNormalized(childPath);
      try {
        await Filesystem.rmdir({ path: childPath, directory: Directory.Data });
      } catch (error) {
        if (isMissingFilesystemError(error)) {
          return;
        }
        throw error;
      }
      return;
    }

    try {
      await Filesystem.deleteFile({ path: childPath, directory: Directory.Data });
      return;
    } catch (error) {
      if (isMissingFilesystemError(error)) {
        return;
      }
      throw error;
    }
  }));
}

export async function clearDirectory(path: string, options: StoragePathOptions = {}): Promise<void> {
  await clearDirectoryNormalized(normalizePath(path, options));
}

export function toBase64DataUrl(mimeType: string, data: string): string {
  return `data:${mimeType};base64,${data}`;
}

export function dataUrlToBase64(dataUrl: string): { mimeType: string; base64: string } {
  const match = /^data:([^;]+);base64,(.+)$/i.exec(dataUrl);
  if (!match || !match[1] || !match[2]) {
    throw new Error("Invalid data URL");
  }
  return { mimeType: match[1], base64: match[2] };
}

export function base64FromArrayBuffer(buffer: ArrayBuffer): string {
  let binary = "";
  const bytes = new Uint8Array(buffer);
  const chunkSize = 0x8000;
  for (let index = 0; index < bytes.length; index += chunkSize) {
    binary += String.fromCharCode(...bytes.subarray(index, index + chunkSize));
  }
  return btoa(binary);
}

export { BASE_DIR };
