type TranslateFunction = typeof import("../i18n").t;

export function formatCatalogLastPlayed(t: TranslateFunction, date?: string): string {
  if (!date) return t("library.lastPlayed.never");

  const lastPlayed = new Date(date);
  const now = new Date();
  const diffMs = now.getTime() - lastPlayed.getTime();
  const diffMins = Math.floor(diffMs / 60000);
  const diffHours = Math.floor(diffMs / 3600000);
  const diffDays = Math.floor(diffMs / 86400000);

  if (diffMins < 1) return t("library.lastPlayed.justNow");
  if (diffMins < 60) return t("library.lastPlayed.minutesAgo", { count: diffMins });
  if (diffHours < 24) return t("library.lastPlayed.hoursAgo", { count: diffHours });
  if (diffDays < 7) return t("library.lastPlayed.daysAgo", { count: diffDays });
  if (diffDays < 30) return t("library.lastPlayed.weeksAgo", { count: Math.floor(diffDays / 7) });

  return lastPlayed.toLocaleDateString();
}

export function formatPlaytimeLastPlayed(isoString: string | null): string {
  if (!isoString) return "Never";
  const then = new Date(isoString);
  const now = new Date();

  const thenDay = new Date(then.getFullYear(), then.getMonth(), then.getDate()).getTime();
  const todayDay = new Date(now.getFullYear(), now.getMonth(), now.getDate()).getTime();

  const diffDays = Math.round((todayDay - thenDay) / 86_400_000);

  if (diffDays === 0) return "Today";
  if (diffDays === 1) return "Yesterday";
  if (diffDays < 7) return `${diffDays} days ago`;
  if (diffDays < 30) return `${Math.floor(diffDays / 7)} wk ago`;
  if (diffDays < 365) return `${Math.floor(diffDays / 30)} mo ago`;
  return `${Math.floor(diffDays / 365)} yr ago`;
}
