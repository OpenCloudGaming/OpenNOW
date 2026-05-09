export function sanitizeGenreName(raw: string): string {
  return raw
    .replace(/_/g, " ")
    .replace(/\b\w/g, (ch) => ch.toUpperCase());
}
