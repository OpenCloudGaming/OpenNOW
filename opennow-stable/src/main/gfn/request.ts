import { SessionError } from "./errorCodes";

interface CloudMatchResponseReadOptions {
  onText?: (text: string) => void;
  onErrorText?: (text: string) => void;
}

export async function readCloudMatchResponseText(
  response: Response,
  options: CloudMatchResponseReadOptions = {},
): Promise<string> {
  const text = await response.text();
  options.onText?.(text);

  if (!response.ok) {
    options.onErrorText?.(text);
    throw SessionError.fromResponse(response.status, text);
  }

  return text;
}

export async function readCloudMatchJson<T>(
  response: Response,
  options: CloudMatchResponseReadOptions = {},
): Promise<{ text: string; payload: T }> {
  const text = await readCloudMatchResponseText(response, options);
  return {
    text,
    payload: JSON.parse(text) as T,
  };
}

export async function throwIfCloudMatchResponseError(response: Response): Promise<void> {
  if (response.ok) {
    return;
  }

  const text = await response.text();
  throw SessionError.fromResponse(response.status, text);
}
