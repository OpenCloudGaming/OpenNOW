import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { JSX } from "react";
import type { GameInfo, PrintedWasteQueueData, PrintedWasteZone, StreamRegion } from "@shared/gfn";

const PING_RESULTS_STORAGE_KEY = "opennow.ping-results.v1";

interface PingCacheEntry {
  url: string;
  pingMs: number | null;
}

interface ZoneInfo {
  zoneId: string;
  pwRegion: string;
  queuePosition: number;
  etaMs?: number;
  lastUpdated: number;
  pingMs: number | null;
  gfnRegion: StreamRegion | null;
}

const REGION_META: Record<string, { label: string; flag: string }> = {
  US:   { label: "North America",  flag: "🇺🇸" },
  EU:   { label: "Europe",         flag: "🇪🇺" },
  JP:   { label: "Japan",          flag: "🇯🇵" },
  KR:   { label: "South Korea",    flag: "🇰🇷" },
  CA:   { label: "Canada",         flag: "🇨🇦" },
  THAI: { label: "Southeast Asia", flag: "🇹🇭" },
  MY:   { label: "Malaysia",       flag: "🇲🇾" },
};

const REGION_ORDER = ["US", "CA", "EU", "JP", "KR", "THAI", "MY"];

// ── Helpers ───────────────────────────────────────────────────────────────────

/**
 * Extract the uppercase zone ID from a GFN streaming URL.
 * e.g. "https://np-ams-08.cloudmatchbeta.nvidiagrid.net/" → "NP-AMS-08"
 * This is the canonical way to match GFN regions to PrintedWaste zone IDs
 * because the metadata key name from serverInfo is not guaranteed to match.
 */
function zoneIdFromUrl(url: string): string | null {
  try {
    const hostname = new URL(url).hostname; // "np-ams-08.cloudmatchbeta.nvidiagrid.net"
    const subdomain = hostname.split(".")[0]; // "np-ams-08"
    return subdomain ? subdomain.toUpperCase() : null; // "NP-AMS-08"
  } catch {
    return null;
  }
}

function matchGfnRegion(zoneId: string, regions: StreamRegion[]): StreamRegion | null {
  const upper = zoneId.toUpperCase();
  // 1. Exact name match (case-insensitive)
  const byName = regions.find((r) => r.name.toUpperCase() === upper);
  if (byName) return byName;
  // 2. URL-subdomain match — the reliable fallback
  const byUrl = regions.find((r) => zoneIdFromUrl(r.url) === upper);
  return byUrl ?? null;
}

function formatWait(etaMs: number): string {
  const mins = Math.ceil(etaMs / 60000);
  if (mins < 60) return `~${mins}m`;
  const h = Math.floor(mins / 60);
  const m = mins % 60;
  return m > 0 ? `~${h}h ${m}m` : `~${h}h`;
}

function getPingColor(pingMs: number | null): string {
  if (pingMs === null) return "#6b7280";
  if (pingMs < 30)  return "#22c55e";
  if (pingMs < 80)  return "#84cc16";
  if (pingMs < 150) return "#eab308";
  return "#ef4444";
}

function getQueueColor(pos: number): string {
  if (pos <= 5)  return "#22c55e";
  if (pos <= 15) return "#84cc16";
  if (pos <= 30) return "#eab308";
  return "#ef4444";
}

function loadStoredPingResults(): Map<string, number | null> {
  try {
    const raw = window.sessionStorage.getItem(PING_RESULTS_STORAGE_KEY);
    if (!raw) return new Map();
    const parsed = JSON.parse(raw) as PingCacheEntry[];
    if (!Array.isArray(parsed)) return new Map();
    const map = new Map<string, number | null>();
    for (const entry of parsed) map.set(entry.url, entry.pingMs);
    return map;
  } catch {
    return new Map();
  }
}

// ── Component ─────────────────────────────────────────────────────────────────

interface Props {
  game: GameInfo;
  regions: StreamRegion[];
  onConfirm: (zoneUrl: string | null) => void;
  onCancel: () => void;
}

export function QueueServerSelectModal({ game, regions, onConfirm, onCancel }: Props): JSX.Element {
  const [queueData, setQueueData] = useState<PrintedWasteQueueData | null>(null);
  const [loading, setLoading]     = useState(true);
  const [fetchError, setFetchError] = useState<string | null>(null);
  const [selected, setSelected]   = useState<"auto" | "closest" | string>("auto");
  const listRef = useRef<HTMLDivElement>(null);

  // Cached ping results from SettingsPage tests
  const pingResults = useMemo(() => loadStoredPingResults(), []);
  const hasPingData = pingResults.size > 0;

  useEffect(() => {
    let cancelled = false;
    void (async () => {
      try {
        const data = await window.openNow.fetchPrintedWasteQueue();
        if (!cancelled) setQueueData(data);
      } catch {
        if (!cancelled) setFetchError("Could not load queue data. You can still launch with default routing.");
      } finally {
        if (!cancelled) setLoading(false);
      }
    })();
    return () => { cancelled = true; };
  }, []);

  // Build enriched zone list — match every PrintedWaste zone to a GFN region via URL subdomain
  const zones = useMemo<ZoneInfo[]>(() => {
    if (!queueData) return [];
    return Object.entries(queueData).map(([zoneId, zone]: [string, PrintedWasteZone]) => {
      const gfnRegion = matchGfnRegion(zoneId, regions);
      const pingMs = gfnRegion ? (pingResults.get(gfnRegion.url) ?? null) : null;
      return {
        zoneId,
        pwRegion: zone.Region,
        queuePosition: zone.QueuePosition,
        etaMs: zone.eta,
        lastUpdated: zone["Last Updated"],
        pingMs,
        gfnRegion,
      };
    });
  }, [queueData, regions, pingResults]);

  // ── Auto-selected: lowest weighted score (40% ping + 60% queue) ──────────
  const autoZone = useMemo<ZoneInfo | null>(() => {
    // Prefer zones we can actually route to
    const routable = zones.filter((z) => z.gfnRegion !== null);
    const pool = routable.length > 0 ? routable : zones; // fallback to all if no GFN mappings
    if (pool.length === 0) return null;

    // Separate into "has ping" and "no ping"
    const withPing = pool.filter((z) => z.pingMs !== null);
    const scoring  = withPing.length > 0 ? withPing : pool;

    const maxPing  = Math.max(...scoring.map((z) => z.pingMs ?? 999), 1);
    const maxQueue = Math.max(...scoring.map((z) => z.queuePosition), 1);

    return scoring.reduce((best, z) => {
      const np = (z.pingMs ?? maxPing) / maxPing;
      const nq = z.queuePosition / maxQueue;
      const bp = (best.pingMs ?? maxPing) / maxPing;
      const bq = best.queuePosition / maxQueue;
      return (np * 0.4 + nq * 0.6) < (bp * 0.4 + bq * 0.6) ? z : best;
    }, scoring[0]!);
  }, [zones]);

  // ── Closest: lowest ping (only if ping data available) ───────────────────
  const closestZone = useMemo<ZoneInfo | null>(() => {
    if (!hasPingData) return null;
    const candidates = zones.filter((z) => z.pingMs !== null && z.gfnRegion !== null);
    if (candidates.length === 0) return null;
    return candidates.reduce((best, z) => (z.pingMs! < best.pingMs! ? z : best));
  }, [zones, hasPingData]);

  const autoIsSameAsClosest =
    autoZone && closestZone && autoZone.zoneId === closestZone.zoneId;

  // ── Group remaining zones by region ─────────────────────────────────────
  const groupedZones = useMemo<Record<string, ZoneInfo[]>>(() => {
    const groups: Record<string, ZoneInfo[]> = {};
    for (const z of zones) {
      if (!groups[z.pwRegion]) groups[z.pwRegion] = [];
      groups[z.pwRegion].push(z);
    }
    for (const key of Object.keys(groups)) {
      groups[key].sort((a, b) => a.queuePosition - b.queuePosition);
    }
    return groups;
  }, [zones]);

  const regionOrder = useMemo(() => {
    const present = Object.keys(groupedZones);
    return [
      ...REGION_ORDER.filter((r) => present.includes(r)),
      ...present.filter((r) => !REGION_ORDER.includes(r)),
    ];
  }, [groupedZones]);

  // ── Confirm ──────────────────────────────────────────────────────────────
  const handleConfirm = useCallback(() => {
    if (selected === "auto") {
      onConfirm(autoZone?.gfnRegion?.url ?? null);
    } else if (selected === "closest") {
      onConfirm(closestZone?.gfnRegion?.url ?? null);
    } else {
      const zone = zones.find((z) => z.zoneId === selected);
      onConfirm(zone?.gfnRegion?.url ?? null);
    }
  }, [selected, autoZone, closestZone, zones, onConfirm]);

  const handleKeyDown = useCallback((e: React.KeyboardEvent) => {
    if (e.key === "Escape") onCancel();
    if (e.key === "Enter") handleConfirm();
  }, [onCancel, handleConfirm]);

  const showRecommended = !loading && !fetchError && (autoZone !== null || closestZone !== null);

  // ── Render ───────────────────────────────────────────────────────────────
  return (
    <div
      style={{
        position: "fixed",
        inset: 0,
        zIndex: 1000,
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        background: "rgba(0,0,0,0.82)",
        backdropFilter: "blur(8px)",
        WebkitBackdropFilter: "blur(8px)",
      }}
      onClick={(e) => { if (e.target === e.currentTarget) onCancel(); }}
      onKeyDown={handleKeyDown}
      tabIndex={-1}
    >
      <div style={{
        background: "linear-gradient(160deg, #111827 0%, #0d1117 100%)",
        border: "1px solid rgba(255,255,255,0.08)",
        borderRadius: 16,
        width: "min(700px, 94vw)",
        maxHeight: "86vh",
        display: "flex",
        flexDirection: "column",
        overflow: "hidden",
        boxShadow: "0 28px 72px rgba(0,0,0,0.65), 0 0 0 1px rgba(255,255,255,0.03)",
      }}>

        {/* ── Header ── */}
        <div style={{ padding: "20px 24px 0", flexShrink: 0 }}>
          <div style={{ display: "flex", alignItems: "flex-start", justifyContent: "space-between", gap: 12 }}>
            <div>
              <h2 style={{ margin: 0, fontSize: 18, fontWeight: 700, color: "#f9fafb" }}>
                Select Server
              </h2>
              <p style={{ margin: "3px 0 0", fontSize: 13, color: "#6b7280" }}>
                {game.title} · Free tier server queue
              </p>
            </div>
            <button
              onClick={onCancel}
              aria-label="Close"
              style={{
                background: "rgba(255,255,255,0.06)",
                border: "none",
                borderRadius: 8,
                color: "#9ca3af",
                cursor: "pointer",
                fontSize: 16,
                lineHeight: 1,
                padding: "6px 10px",
                flexShrink: 0,
              }}
            >✕</button>
          </div>
          <div style={{ height: 1, background: "rgba(255,255,255,0.06)", margin: "16px 0 0" }} />
        </div>

        {/* ── Scrollable body ── */}
        <div
          ref={listRef}
          style={{
            flex: 1,
            overflowY: "auto",
            padding: "16px 24px",
            scrollbarWidth: "thin",
            scrollbarColor: "rgba(255,255,255,0.08) transparent",
          }}
        >
          {/* Loading */}
          {loading && (
            <div style={{ textAlign: "center", padding: "36px 0", color: "#6b7280" }}>
              <Spinner />
              <p style={{ margin: "10px 0 0", fontSize: 14 }}>Fetching live queue data…</p>
            </div>
          )}

          {/* Error banner */}
          {!loading && fetchError && (
            <div style={{
              background: "rgba(239,68,68,0.08)",
              border: "1px solid rgba(239,68,68,0.18)",
              borderRadius: 10,
              padding: "12px 16px",
              color: "#fca5a5",
              fontSize: 13,
              marginBottom: 16,
            }}>
              {fetchError}
            </div>
          )}

          {/* Recommended */}
          {showRecommended && (
            <div style={{ marginBottom: 20 }}>
              <SectionLabel>Recommended</SectionLabel>
              <div style={{
                display: "grid",
                gridTemplateColumns: closestZone && !autoIsSameAsClosest ? "1fr 1fr" : "1fr",
                gap: 10,
              }}>
                {autoZone && (
                  <RecommendCard
                    label="⚡ Auto Selected"
                    sublabel={hasPingData ? "Best ping + queue balance" : "Lowest queue position"}
                    zone={autoZone}
                    selected={selected === "auto"}
                    accent="#76b900"
                    onClick={() => setSelected("auto")}
                  />
                )}
                {closestZone && !autoIsSameAsClosest && (
                  <RecommendCard
                    label="📍 Closest Server"
                    sublabel="Lowest latency to you"
                    zone={closestZone}
                    selected={selected === "closest"}
                    accent="#3b82f6"
                    onClick={() => setSelected("closest")}
                  />
                )}
              </div>
            </div>
          )}

          {/* No ping notice */}
          {!loading && !fetchError && zones.length > 0 && !hasPingData && (
            <div style={{
              background: "rgba(251,191,36,0.06)",
              border: "1px solid rgba(251,191,36,0.15)",
              borderRadius: 8,
              padding: "9px 13px",
              fontSize: 12,
              color: "#fde68a",
              marginBottom: 16,
            }}>
              💡 Run a ping test in Settings to see latency per server and get a Closest Server recommendation.
            </div>
          )}

          {/* All servers */}
          {!loading && zones.length > 0 && (
            <div>
              <SectionLabel>All Servers</SectionLabel>
              {regionOrder.map((region) => {
                const regionZones = groupedZones[region] ?? [];
                const meta = REGION_META[region] ?? { label: region, flag: "🌐" };
                return (
                  <div key={region} style={{ marginBottom: 14 }}>
                    <div style={{ display: "flex", alignItems: "center", gap: 6, marginBottom: 6 }}>
                      <span style={{ fontSize: 15 }}>{meta.flag}</span>
                      <span style={{ fontSize: 12, fontWeight: 600, color: "#6b7280", letterSpacing: "0.03em" }}>
                        {meta.label}
                      </span>
                    </div>
                    <div style={{ display: "flex", flexDirection: "column", gap: 3 }}>
                      {regionZones.map((zone) => (
                        <ZoneRow
                          key={zone.zoneId}
                          zone={zone}
                          isAuto={autoZone?.zoneId === zone.zoneId}
                          isClosest={closestZone?.zoneId === zone.zoneId}
                          selected={selected === zone.zoneId}
                          onClick={() => setSelected(zone.zoneId)}
                        />
                      ))}
                    </div>
                  </div>
                );
              })}
            </div>
          )}

          {!loading && !fetchError && zones.length === 0 && (
            <p style={{ color: "#6b7280", fontSize: 13, textAlign: "center", padding: "24px 0" }}>
              No server data available.
            </p>
          )}
        </div>

        {/* ── Footer ── */}
        <div style={{
          padding: "12px 24px 20px",
          flexShrink: 0,
          borderTop: "1px solid rgba(255,255,255,0.06)",
          display: "flex",
          alignItems: "center",
          justifyContent: "space-between",
          gap: 12,
        }}>
          <a
            href="https://printedwaste.com/gfn"
            target="_blank"
            rel="noopener noreferrer"
            style={{ fontSize: 11, color: "#4b5563", textDecoration: "none" }}
            onMouseEnter={(e) => { (e.currentTarget as HTMLAnchorElement).style.color = "#9ca3af"; }}
            onMouseLeave={(e) => { (e.currentTarget as HTMLAnchorElement).style.color = "#4b5563"; }}
          >
            Powered by <strong style={{ color: "inherit" }}>PrintedWaste</strong>
          </a>

          <div style={{ display: "flex", gap: 10 }}>
            <button
              onClick={onCancel}
              style={ghostButtonStyle}
              onMouseEnter={(e) => { (e.currentTarget as HTMLButtonElement).style.background = "rgba(255,255,255,0.10)"; }}
              onMouseLeave={(e) => { (e.currentTarget as HTMLButtonElement).style.background = "rgba(255,255,255,0.06)"; }}
            >
              Cancel
            </button>
            <button
              onClick={handleConfirm}
              style={launchButtonStyle}
              onMouseEnter={(e) => { (e.currentTarget as HTMLButtonElement).style.opacity = "0.88"; }}
              onMouseLeave={(e) => { (e.currentTarget as HTMLButtonElement).style.opacity = "1"; }}
            >
              Launch <span style={{ marginLeft: 4 }}>→</span>
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}

// ── Sub-components ────────────────────────────────────────────────────────────

function SectionLabel({ children }: { children: React.ReactNode }): JSX.Element {
  return (
    <p style={{
      margin: "0 0 10px",
      fontSize: 11,
      fontWeight: 600,
      color: "#4b5563",
      textTransform: "uppercase",
      letterSpacing: "0.08em",
    }}>
      {children}
    </p>
  );
}

interface RecommendCardProps {
  label: string;
  sublabel: string;
  zone: ZoneInfo;
  selected: boolean;
  accent: string;
  onClick: () => void;
}

function RecommendCard({ label, sublabel, zone, selected, accent, onClick }: RecommendCardProps): JSX.Element {
  const regionMeta = REGION_META[zone.pwRegion] ?? { label: zone.pwRegion, flag: "🌐" };
  const [hovered, setHovered] = useState(false);

  return (
    <button
      onClick={onClick}
      onMouseEnter={() => setHovered(true)}
      onMouseLeave={() => setHovered(false)}
      style={{
        background: selected
          ? `${accent}1a`
          : hovered ? "rgba(255,255,255,0.05)" : "rgba(255,255,255,0.02)",
        border: `1px solid ${selected ? accent : hovered ? "rgba(255,255,255,0.14)" : "rgba(255,255,255,0.08)"}`,
        borderRadius: 10,
        padding: "13px 15px",
        cursor: "pointer",
        textAlign: "left",
        width: "100%",
        transition: "border-color 0.12s, background 0.12s",
      }}
    >
      <div style={{ fontSize: 11, fontWeight: 700, color: selected ? accent : "#6b7280", marginBottom: 1, letterSpacing: "0.04em", textTransform: "uppercase" }}>
        {label}
      </div>
      <div style={{ fontSize: 11, color: "#4b5563", marginBottom: 10 }}>{sublabel}</div>
      <div style={{ fontSize: 14, fontWeight: 600, color: "#e5e7eb", marginBottom: 8 }}>
        {regionMeta.flag} {zone.zoneId}
      </div>
      <div style={{ display: "flex", gap: 6, flexWrap: "wrap" }}>
        {zone.pingMs !== null && (
          <Chip color={getPingColor(zone.pingMs)}>{zone.pingMs}ms</Chip>
        )}
        <Chip color={getQueueColor(zone.queuePosition)}>Queue: {zone.queuePosition}</Chip>
        {zone.etaMs !== undefined && (
          <Chip color="#6b7280">{formatWait(zone.etaMs)} wait</Chip>
        )}
        {zone.gfnRegion === null && (
          <Chip color="#4b5563">no route</Chip>
        )}
      </div>
    </button>
  );
}

interface ZoneRowProps {
  zone: ZoneInfo;
  isAuto: boolean;
  isClosest: boolean;
  selected: boolean;
  onClick: () => void;
}

function ZoneRow({ zone, isAuto, isClosest, selected, onClick }: ZoneRowProps): JSX.Element {
  const [hovered, setHovered] = useState(false);

  return (
    <button
      onClick={onClick}
      onMouseEnter={() => setHovered(true)}
      onMouseLeave={() => setHovered(false)}
      style={{
        background: selected
          ? "rgba(118,185,0,0.08)"
          : hovered ? "rgba(255,255,255,0.04)" : "rgba(255,255,255,0.02)",
        border: `1px solid ${selected ? "rgba(118,185,0,0.38)" : hovered ? "rgba(255,255,255,0.09)" : "rgba(255,255,255,0.05)"}`,
        borderRadius: 7,
        padding: "7px 11px",
        cursor: "pointer",
        textAlign: "left",
        display: "flex",
        alignItems: "center",
        justifyContent: "space-between",
        gap: 10,
        width: "100%",
        transition: "border-color 0.1s, background 0.1s",
      }}
    >
      {/* Left: zone ID + badges */}
      <div style={{ display: "flex", alignItems: "center", gap: 7, minWidth: 0 }}>
        <span style={{
          fontSize: 12,
          fontWeight: 600,
          color: selected ? "#d1fae5" : "#d1d5db",
          fontFamily: "'Roboto Mono', 'Courier New', monospace",
          letterSpacing: "0.02em",
        }}>
          {zone.zoneId}
        </span>
        {isAuto && (
          <span style={{ fontSize: 10, background: "rgba(118,185,0,0.15)", color: "#76b900", borderRadius: 4, padding: "1px 5px", fontWeight: 700 }}>
            AUTO
          </span>
        )}
        {isClosest && !isAuto && (
          <span style={{ fontSize: 10, background: "rgba(59,130,246,0.15)", color: "#60a5fa", borderRadius: 4, padding: "1px 5px", fontWeight: 700 }}>
            NEAREST
          </span>
        )}
      </div>

      {/* Right: stats */}
      <div style={{ display: "flex", gap: 10, alignItems: "center", flexShrink: 0 }}>
        {zone.pingMs !== null && (
          <span style={{ fontSize: 12, color: getPingColor(zone.pingMs), fontWeight: 600, minWidth: 46, textAlign: "right" }}>
            {zone.pingMs}ms
          </span>
        )}
        <span style={{ fontSize: 12, color: getQueueColor(zone.queuePosition), fontWeight: 700, minWidth: 32, textAlign: "right" }}>
          Q:{zone.queuePosition}
        </span>
        {zone.etaMs !== undefined && (
          <span style={{ fontSize: 11, color: "#6b7280", minWidth: 44, textAlign: "right" }}>
            {formatWait(zone.etaMs)}
          </span>
        )}
        {zone.gfnRegion === null && (
          <span style={{ fontSize: 10, color: "#374151", fontStyle: "italic" }}>no route</span>
        )}
      </div>
    </button>
  );
}

function Chip({ color, children }: { color: string; children: React.ReactNode }): JSX.Element {
  return (
    <span style={{
      display: "inline-flex",
      alignItems: "center",
      fontSize: 11,
      fontWeight: 600,
      color,
      background: `${color}18`,
      borderRadius: 4,
      padding: "2px 7px",
    }}>
      {children}
    </span>
  );
}

function Spinner(): JSX.Element {
  return (
    <>
      <style>{`@keyframes on-spin { to { transform: rotate(360deg); } }`}</style>
      <div style={{
        display: "inline-block",
        width: 26,
        height: 26,
        border: "3px solid rgba(255,255,255,0.08)",
        borderTop: "3px solid #76b900",
        borderRadius: "50%",
        animation: "on-spin 0.75s linear infinite",
      }} />
    </>
  );
}

// ── Shared button styles ──────────────────────────────────────────────────────

const ghostButtonStyle: React.CSSProperties = {
  background: "rgba(255,255,255,0.06)",
  border: "1px solid rgba(255,255,255,0.08)",
  borderRadius: 8,
  color: "#9ca3af",
  cursor: "pointer",
  fontSize: 13,
  fontWeight: 500,
  padding: "8px 18px",
  transition: "background 0.12s",
};

const launchButtonStyle: React.CSSProperties = {
  background: "linear-gradient(135deg, #76b900 0%, #4d7a00 100%)",
  border: "none",
  borderRadius: 8,
  color: "#fff",
  cursor: "pointer",
  fontSize: 13,
  fontWeight: 700,
  padding: "8px 22px",
  display: "flex",
  alignItems: "center",
  transition: "opacity 0.12s",
  letterSpacing: "0.02em",
};
