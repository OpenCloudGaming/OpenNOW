import { Directory, Encoding, Filesystem } from "@capacitor/filesystem";
import { Preferences } from "@capacitor/preferences";

const BASE_DIR = "opennow";

function normalizePath(path: string): string {
  return path.startsWith(`${BASE_DIR}/`) ? path : `${BASE_DIR}/${path}`;
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

export async function ensureDir(path: string): Promise<void> {
  try {
    await Filesystem.mkdir({ path, directory: Directory.Data, recursive: true });
  } catch {
    return;
  }
}

export async function writeFile(path: string, data: string): Promise<void> {
  const normalized = normalizePath(path);
  const parent = normalized.split("/").slice(0, -1).join("/");
  if (parent) await ensureDir(parent);
  await Filesystem.writeFile({ path: normalized, data, directory: Directory.Data, encoding: Encoding.UTF8 });
}

export async function appendFile(path: string, data: string): Promise<void> {
  const normalized = normalizePath(path);
  const parent = normalized.split("/").slice(0, -1).join("/");
  if (parent) await ensureDir(parent);
  await Filesystem.appendFile({ path: normalized, data, directory: Directory.Data, encoding: Encoding.UTF8 });
}

export async function readFile(path: string): Promise<string> {
  const normalized = normalizePath(path);
  const result = await Filesystem.readFile({ path: normalized, directory: Directory.Data, encoding: Encoding.UTF8 });
  return typeof result.data === "string" ? result.data : "";
}

export async function readFileBase64(path: string): Promise<string> {
  const normalized = normalizePath(path);
  const result = await Filesystem.readFile({ path: normalized, directory: Directory.Data });
  return typeof result.data === "string" ? result.data : "";
}

export async function deleteFile(path: string): Promise<void> {
  const normalized = normalizePath(path);
  try {
    await Filesystem.deleteFile({ path: normalized, directory: Directory.Data });
  } catch {
    return;
  }
}

export async function readDir(path: string): Promise<string[]> {
  const normalized = normalizePath(path);
  try {
    const result = await Filesystem.readdir({ path: normalized, directory: Directory.Data });
    return result.files.map((file) => file.name);
  } catch {
    return [];
  }
}

export async function clearDirectory(path: string): Promise<void> {
  const names = await readDir(path);
  await Promise.all(names.map(async (name) => {
    const childPath = `${path}/${name}`;
    try {
      await Filesystem.deleteFile({ path: normalizePath(childPath), directory: Directory.Data });
      return;
    } catch {
      await clearDirectory(childPath);
      await Filesystem.rmdir({ path: normalizePath(childPath), directory: Directory.Data }).catch(() => undefined);
    }
  }));
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
