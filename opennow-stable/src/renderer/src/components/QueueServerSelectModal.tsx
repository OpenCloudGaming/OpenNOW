import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { JSX } from "react";
import type { GameInfo, PrintedWasteQueueData, PrintedWasteZone } from "@shared/gfn";

// ── Constants / helpers ───────────────────────────────────────────────────────

/**
 * Only include standard NVIDIA zones (NP-*).
 * Alliance-partner zones start with NPA- and have their own routing
 * infrastructure that doesn't follow the cloudmatchbeta.nvidiagrid.net pattern.
 */
function isStandardZone(zoneId: string): boolean {
  return zoneId.startsWith("NP-") && !zoneId.startsWith("NPA-");
}

/**
 * Build the direct cloudmatch URL from a zone ID.
 * "NP-AMS-08" → "https://np-ams-08.cloudmatchbeta.nvidiagrid.net/"
 * This URL is used as streamingBaseUrl in createSession to route the user
 * to that specific zone's load balancer.
 */
function constructZoneUrl(zoneId: string): string {
  return `https://${zoneId.toLowerCase()}.cloudmatchbeta.nvidiagrid.net/`;
}

function formatWait(etaMs: number): string {
  const mins = Math.ceil(etaMs / 60000);
  if (mins < 60) return `~${mins}m`;
  const h = Math.floor(mins / 60);
  const m = mins % 60;
  return m > 0 ? `~${h}h ${m}m` : `~${h}h`;
}

function getPingColor(ms: number | null): string {
  if (ms === null) return "#6b7280";
  if (ms < 30)  return "#58d98a";
  if (ms < 80)  return "#84cc16";
  if (ms < 150) return "#eab308";
  return "#ef4444";
}

function getQueueColor(q: number): string {
  if (q <= 5)  return "#58d98a";
  if (q <= 15) return "#84cc16";
  if (q <= 30) return "#eab308";
  return "#ef4444";
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
const PRINTEDWASTE_PING_CACHE_KEY = "opennow.printedwaste-pings.v1";
const MAX_PING_ZONES_PER_OPEN = 12;
const QUEUE_REFRESH_INTERVAL_MS = 2 * 60 * 1000;

interface PingCacheEntry {
  url: string;
  pingMs: number | null;
}

function loadStoredPingResults(): Map<string, number | null> {
  try {
    const raw = window.sessionStorage.getItem(PRINTEDWASTE_PING_CACHE_KEY);
    if (!raw) return new Map();
    const parsed = JSON.parse(raw) as unknown;
    if (!Array.isArray(parsed)) return new Map();
    const results = new Map<string, number | null>();
    for (const entry of parsed as PingCacheEntry[]) {
      results.set(entry.url, entry.pingMs);
    }
    return results;
  } catch {
    return new Map();
  }
}

function saveStoredPingResults(results: Map<string, number | null>): void {
  try {
    const entries: PingCacheEntry[] = [];
    results.forEach((pingMs, url) => {
      entries.push({ url, pingMs });
    });
    window.sessionStorage.setItem(PRINTEDWASTE_PING_CACHE_KEY, JSON.stringify(entries));
  } catch {
    // Ignore storage failures
  }
}

// ── Types ─────────────────────────────────────────────────────────────────────

interface ZoneInfo {
  zoneId: string;
  pwRegion: string;
  queuePosition: number;
  etaMs?: number;
  routingUrl: string; // always set for standard zones
  pingMs: number | null;
}

// ── Component ─────────────────────────────────────────────────────────────────

interface Props {
  game: GameInfo;
  initialQueueData?: PrintedWasteQueueData | null;
  onConfirm: (zoneUrl: string | null) => void;
  onCancel: () => void;
}

export function QueueServerSelectModal({ game, initialQueueData = null, onConfirm, onCancel }: Props): JSX.Element {
  const [queueData,  setQueueData]  = useState<PrintedWasteQueueData | null>(initialQueueData);
  const [queueLoading, setQueueLoading] = useState(initialQueueData === null);
  const [fetchError, setFetchError] = useState<string | null>(null);

  // Ping state — populated after queue data loads
  const [zonePings,  setZonePings]  = useState<Map<string, number | null> | null>(null);
  const [isPinging,  setIsPinging]  = useState(false);

  const [selected, setSelected] = useState<"auto" | "closest" | string>("auto");
  const dialogRef = useRef<HTMLDivElement | null>(null);

  // ── Fetch queue data ──────────────────────────────────────────────────────
  useEffect(() => {
    if (initialQueueData) return;
    let cancelled = false;
    void (async () => {
      try {
        const data = await window.openNow.fetchPrintedWasteQueue();
        if (!cancelled) setQueueData(data);
      } catch {
        if (!cancelled) setFetchError("Could not load queue data. You can still launch with default routing.");
      } finally {
        if (!cancelled) setQueueLoading(false);
      }
    })();
    return () => { cancelled = true; };
  }, [initialQueueData]);

  // Keep queue data fresh while modal is open.
  useEffect(() => {
    let cancelled = false;
    let inFlight = false;

    const refreshQueueData = async (): Promise<void> => {
      if (inFlight) return;
      inFlight = true;
      try {
        const data = await window.openNow.fetchPrintedWasteQueue();
        if (cancelled) return;
        setQueueData(data);
        setFetchError(null);
      } catch {
        // Keep last known queue data if refresh fails.
      } finally {
        inFlight = false;
      }
    };

    const intervalId = window.setInterval(() => {
      void refreshQueueData();
    }, QUEUE_REFRESH_INTERVAL_MS);

    return () => {
      cancelled = true;
      window.clearInterval(intervalId);
    };
  }, []);

  useEffect(() => {
    dialogRef.current?.focus();
  }, []);

  // ── Ping all standard zones once queue data arrives ───────────────────────
  useEffect(() => {
    if (!queueData) return;
    const allStandardZones = Object.entries(queueData)
      .filter(([zoneId]) => isStandardZone(zoneId))
      .map(([zoneId, zone]) => ({
        zoneId,
        pwRegion: zone.Region,
        queuePosition: zone.QueuePosition,
        routingUrl: constructZoneUrl(zoneId),
      }));
    if (allStandardZones.length === 0) return;

    let cancelled = false;
    const cachedPings = loadStoredPingResults();
    const seedMap = new Map<string, number | null>();
    for (const zone of allStandardZones) {
      if (cachedPings.has(zone.routingUrl)) {
        seedMap.set(zone.routingUrl, cachedPings.get(zone.routingUrl) ?? null);
      }
    }
    if (seedMap.size > 0) {
      setZonePings(seedMap);
    }

    // Ping at most one best-queue zone per region first, then fill by queue rank.
    const topPerRegion = new Map<string, (typeof allStandardZones)[number]>();
    for (const zone of allStandardZones) {
      const existing = topPerRegion.get(zone.pwRegion);
      if (!existing || zone.queuePosition < existing.queuePosition) {
        topPerRegion.set(zone.pwRegion, zone);
      }
    }

    const prioritized = [
      ...topPerRegion.values(),
      ...allStandardZones
        .filter((zone) => !topPerRegion.has(zone.pwRegion) || topPerRegion.get(zone.pwRegion)?.zoneId !== zone.zoneId)
        .sort((a, b) => a.queuePosition - b.queuePosition),
    ];

    const zonesToPing = prioritized
      .filter((zone) => !seedMap.has(zone.routingUrl))
      .slice(0, MAX_PING_ZONES_PER_OPEN);

    if (zonesToPing.length === 0) return;
    const regionsToTest = zonesToPing.map((zone) => ({ name: zone.zoneId, url: zone.routingUrl }));

    setIsPinging(true);
    void (async () => {
      try {
        const results = await window.openNow.pingRegions(regionsToTest);
        if (cancelled) return;
        const map = new Map(seedMap);
        for (const r of results) map.set(r.url, r.pingMs);
        setZonePings(map);
        saveStoredPingResults(map);
      } catch {
        // Ping failures are non-fatal
      } finally {
        if (!cancelled) setIsPinging(false);
      }
    })();
    return () => { cancelled = true; };
  }, [queueData]);

  // ── Build enriched zone list (standard zones only) ────────────────────────
  const zones = useMemo<ZoneInfo[]>(() => {
    if (!queueData) return [];
    return Object.entries(queueData)
      .filter(([zoneId]) => isStandardZone(zoneId))
      .map(([zoneId, zone]: [string, PrintedWasteZone]) => {
        const routingUrl = constructZoneUrl(zoneId);
        const pingMs = zonePings?.get(routingUrl) ?? null;
        return {
          zoneId,
          pwRegion: zone.Region,
          queuePosition: zone.QueuePosition,
          etaMs: zone.eta,
          routingUrl,
          pingMs,
        };
      });
  }, [queueData, zonePings]);

  // ── Recommendations ───────────────────────────────────────────────────────

  // Auto: weighted lowest score (40% ping + 60% queue). Falls back to queue-only
  // when ping data isn't in yet.
  const autoZone = useMemo<ZoneInfo | null>(() => {
    if (zones.length === 0) return null;
    const withPing = zones.filter((z) => z.pingMs !== null);
    const pool     = withPing.length > 0 ? withPing : zones;
    const maxPing  = Math.max(...pool.map((z) => z.pingMs ?? 999), 1);
    const maxQueue = Math.max(...pool.map((z) => z.queuePosition), 1);
    return pool.reduce((best, z) => {
      const score = ((z.pingMs ?? maxPing) / maxPing) * 0.4 + (z.queuePosition / maxQueue) * 0.6;
      const bScore = ((best.pingMs ?? maxPing) / maxPing) * 0.4 + (best.queuePosition / maxQueue) * 0.6;
      return score < bScore ? z : best;
    }, pool[0]!);
  }, [zones]);

  // Closest: lowest latency. Only available after pings complete.
  const closestZone = useMemo<ZoneInfo | null>(() => {
    const withPing = zones.filter((z) => z.pingMs !== null);
    if (withPing.length === 0) return null;
    return withPing.reduce((best, z) => (z.pingMs! < best.pingMs! ? z : best));
  }, [zones]);

  // ── Grouped list ──────────────────────────────────────────────────────────
  const groupedZones = useMemo<Record<string, ZoneInfo[]>>(() => {
    const g: Record<string, ZoneInfo[]> = {};
    for (const z of zones) {
      if (!g[z.pwRegion]) g[z.pwRegion] = [];
      g[z.pwRegion].push(z);
    }
    for (const k of Object.keys(g)) {
      g[k].sort((a, b) => a.queuePosition - b.queuePosition);
    }
    return g;
  }, [zones]);

  const regionOrder = useMemo(() => {
    const present = Object.keys(groupedZones);
    return [
      ...REGION_ORDER.filter((r) => present.includes(r)),
      ...present.filter((r) => !REGION_ORDER.includes(r)),
    ];
  }, [groupedZones]);

  // ── Confirm ───────────────────────────────────────────────────────────────
  const handleConfirm = useCallback(() => {
    if (selected === "auto") {
      onConfirm(autoZone?.routingUrl ?? null);
    } else if (selected === "closest") {
      onConfirm(closestZone?.routingUrl ?? null);
    } else {
      const zone = zones.find((z) => z.zoneId === selected);
      onConfirm(zone?.routingUrl ?? null);
    }
  }, [selected, autoZone, closestZone, zones, onConfirm]);

  const handleKeyDown = useCallback((e: React.KeyboardEvent) => {
    if (e.key === "Escape") onCancel();
    if (e.key === "Enter")  handleConfirm();
  }, [onCancel, handleConfirm]);

  const isLoading = queueLoading;

  // ── Render ────────────────────────────────────────────────────────────────
  return (
    <div
      style={overlayStyle}
      onClick={(e) => { if (e.target === e.currentTarget) onCancel(); }}
      onKeyDown={handleKeyDown}
      ref={dialogRef}
      tabIndex={0}
      role="dialog"
      aria-modal="true"
      aria-labelledby="queue-server-select-title"
    >
      <div style={cardStyle}>

        {/* Header */}
        <div style={{ padding: "20px 24px 0", flexShrink: 0 }}>
          <div style={{ display: "flex", alignItems: "flex-start", justifyContent: "space-between", gap: 12 }}>
            <div>
              <h2 id="queue-server-select-title" style={{ margin: 0, fontSize: 18, fontWeight: 700, color: "var(--ink)" }}>
                Select Server
              </h2>
              <p style={{ margin: "3px 0 0", fontSize: 13, color: "var(--ink-muted)" }}>
                {game.title} · Free tier server queue
              </p>
            </div>
            <button onClick={onCancel} style={closeBtn} aria-label="Close">✕</button>
          </div>
          <div style={{ height: 1, background: "var(--panel-border)", margin: "16px 0 0" }} />
        </div>

        {/* Scrollable body */}
        <div style={scrollBody}>

          {/* Loading queue */}
          {isLoading && (
            <CenteredNote>
              <Spinner />
              <span style={{ marginTop: 10, display: "block", fontSize: 14, color: "var(--ink-muted)" }}>
                Fetching live queue data…
              </span>
            </CenteredNote>
          )}

          {/* Error */}
          {!isLoading && fetchError && (
            <div style={{
              background: "rgba(239,68,68,0.08)",
              border: "1px solid rgba(239,68,68,0.18)",
              borderRadius: 10,
              padding: "12px 16px",
              color: "#fca5a5",
              fontSize: 13,
              marginBottom: 16,
            }}>{fetchError}</div>
          )}

          {/* Main content */}
          {!isLoading && zones.length > 0 && (
            <>
              {/* Recommended — always two cards side by side */}
              <div style={{ marginBottom: 20 }}>
                <SectionLabel>Recommended</SectionLabel>
                <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 10 }}>

                  {/* Auto Selected */}
                  <RecommendCard
                    label="⚡ Auto Selected"
                    sublabel={
                      isPinging
                        ? "Lowest queue · pinging…"
                        : zonePings
                        ? "Best ping + queue balance"
                        : "Lowest queue position"
                    }
                    zone={autoZone}
                    selected={selected === "auto"}
                    accent="#58d98a"
                    onClick={() => setSelected("auto")}
                  />

                  {/* Closest Server — always visible; shows spinner while pinging */}
                  <RecommendCard
                    label="📍 Closest Server"
                    sublabel={
                      isPinging
                        ? "Measuring latency…"
                        : closestZone
                        ? "Lowest latency to you"
                        : "Ping unavailable"
                    }
                    zone={closestZone}
                    selected={selected === "closest"}
                    accent="#58d98a"
                    pinging={isPinging}
                    disabled={!closestZone && !isPinging}
                    onClick={() => { if (closestZone) setSelected("closest"); }}
                  />
                </div>
              </div>

              {/* All servers */}
              <div>
                <SectionLabel>All Servers</SectionLabel>
                {regionOrder.map((region) => {
                  const regionZones = groupedZones[region] ?? [];
                  const meta = REGION_META[region] ?? { label: region, flag: "🌐" };
                  return (
                    <div key={region} style={{ marginBottom: 14 }}>
                      <div style={{ display: "flex", alignItems: "center", gap: 6, marginBottom: 6 }}>
                        <span style={{ fontSize: 15 }}>{meta.flag}</span>
                        <span style={{ fontSize: 12, fontWeight: 600, color: "var(--ink-muted)", letterSpacing: "0.03em" }}>
                          {meta.label}
                        </span>
                      </div>
                      <div style={{ display: "flex", flexDirection: "column", gap: 3 }}>
                        {regionZones.map((zone) => (
                          <ZoneRow
                            key={zone.zoneId}
                            zone={zone}
                            isAuto={autoZone?.zoneId === zone.zoneId}
                            isClosest={!!(closestZone && closestZone.zoneId === zone.zoneId)}
                            isPinging={isPinging && zone.pingMs === null}
                            selected={selected === zone.zoneId}
                            onClick={() => setSelected(zone.zoneId)}
                          />
                        ))}
                      </div>
                    </div>
                  );
                })}
              </div>
            </>
          )}

          {!isLoading && !fetchError && zones.length === 0 && (
            <CenteredNote>
              <span style={{ fontSize: 13, color: "var(--ink-muted)" }}>No server data available.</span>
            </CenteredNote>
          )}
        </div>

        {/* Footer */}
        <div style={footerStyle}>
          <a
            href="https://printedwaste.com/gfn"
            target="_blank"
            rel="noopener noreferrer"
            style={{ fontSize: 11, color: "var(--ink-dim)", textDecoration: "none" }}
            onMouseEnter={(e) => { (e.currentTarget as HTMLAnchorElement).style.color = "var(--ink-soft)"; }}
            onMouseLeave={(e) => { (e.currentTarget as HTMLAnchorElement).style.color = "var(--ink-dim)"; }}
          >
            Powered by <strong style={{ color: "inherit" }}>PrintedWaste</strong>
          </a>
          <div style={{ display: "flex", gap: 10 }}>
            <button
              onClick={onCancel}
              style={ghostBtn}
              onMouseEnter={(e) => {
                const btn = e.currentTarget as HTMLButtonElement;
                btn.style.background = "var(--accent-surface)";
                btn.style.borderColor = "var(--accent)";
                btn.style.color = "var(--accent)";
              }}
              onMouseLeave={(e) => {
                const btn = e.currentTarget as HTMLButtonElement;
                btn.style.background = ghostBtn.background as string;
                btn.style.border = ghostBtn.border as string;
                btn.style.color = ghostBtn.color as string;
              }}
            >Cancel</button>
            <button
              onClick={handleConfirm}
              style={launchBtn}
              onMouseEnter={(e) => {
                const btn = e.currentTarget as HTMLButtonElement;
                btn.style.background = "linear-gradient(135deg, var(--accent-hover), var(--accent))";
                btn.style.boxShadow = "0 6px 20px var(--accent-glow)";
              }}
              onMouseLeave={(e) => {
                const btn = e.currentTarget as HTMLButtonElement;
                btn.style.background = launchBtn.background as string;
                btn.style.boxShadow = launchBtn.boxShadow as string;
              }}
            >
              Launch
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
    <p style={{ margin: "0 0 10px", fontSize: 11, fontWeight: 600, color: "var(--ink-dim)", textTransform: "uppercase", letterSpacing: "0.08em" }}>
      {children}
    </p>
  );
}

function CenteredNote({ children }: { children: React.ReactNode }): JSX.Element {
  return <div style={{ textAlign: "center", padding: "36px 0" }}>{children}</div>;
}

interface RecommendCardProps {
  label: string;
  sublabel: string;
  zone: ZoneInfo | null;
  selected: boolean;
  accent: string;
  pinging?: boolean;
  disabled?: boolean;
  onClick: () => void;
}

function RecommendCard({ label, sublabel, zone, selected, accent, pinging, disabled, onClick }: RecommendCardProps): JSX.Element {
  const [hovered, setHovered] = useState(false);
  const regionMeta = zone ? (REGION_META[zone.pwRegion] ?? { label: zone.pwRegion, flag: "🌐" }) : null;

  const isInteractive = !disabled && !pinging;

  return (
    <button
      onClick={onClick}
      onMouseEnter={() => setHovered(true)}
      onMouseLeave={() => setHovered(false)}
      style={{
        background: selected
          ? "var(--accent-surface-strong)"
          : hovered && isInteractive
          ? "var(--accent-surface)"
          : "var(--bg-c)",
        border: `1px solid ${
          selected ? "var(--accent)"
          : hovered && isInteractive ? "var(--panel-border-solid)"
          : "var(--panel-border)"
        }`,
        borderRadius: 10,
        padding: "13px 14px",
        cursor: isInteractive ? "pointer" : "default",
        textAlign: "left",
        width: "100%",
        opacity: disabled ? 0.4 : 1,
        transition: "border-color 0.12s, background 0.12s, opacity 0.12s",
        minHeight: 110,
      }}
    >
      <div style={{ fontSize: 11, fontWeight: 700, color: selected ? "var(--accent)" : "var(--ink-muted)", marginBottom: 1, textTransform: "uppercase", letterSpacing: "0.05em" }}>
        {label}
      </div>
      <div style={{ fontSize: 11, color: "var(--ink-dim)", marginBottom: 10, display: "flex", alignItems: "center", gap: 5 }}>
        {pinging && <MiniSpinner color={accent} />}
        {sublabel}
      </div>

      {pinging ? (
        <>
          <div style={{ fontSize: 14, fontWeight: 600, color: "var(--ink-muted)", marginBottom: 8 }}>—</div>
          <div style={{ fontSize: 11, color: "var(--ink-dim)" }}>Pinging servers…</div>
        </>
      ) : zone ? (
        <>
          <div style={{ fontSize: 14, fontWeight: 600, color: "var(--ink)", marginBottom: 8 }}>
            {regionMeta?.flag} {zone.zoneId}
          </div>
          <div style={{ display: "flex", gap: 6, flexWrap: "wrap" }}>
            {zone.pingMs !== null && <Chip color={getPingColor(zone.pingMs)}>{zone.pingMs}ms</Chip>}
            <Chip color={getQueueColor(zone.queuePosition)}>Queue: {zone.queuePosition}</Chip>
            {zone.etaMs !== undefined && <Chip color="#6b7280">{formatWait(zone.etaMs)} wait</Chip>}
          </div>
        </>
      ) : (
        <div style={{ fontSize: 12, color: "var(--ink-dim)" }}>No ping data available</div>
      )}
    </button>
  );
}

interface ZoneRowProps {
  zone: ZoneInfo;
  isAuto: boolean;
  isClosest: boolean;
  isPinging: boolean;
  selected: boolean;
  onClick: () => void;
}

function ZoneRow({ zone, isAuto, isClosest, isPinging, selected, onClick }: ZoneRowProps): JSX.Element {
  const [hovered, setHovered] = useState(false);

  return (
    <button
      onClick={onClick}
      onMouseEnter={() => setHovered(true)}
      onMouseLeave={() => setHovered(false)}
      style={{
        background: selected ? "var(--accent-surface)" : hovered ? "var(--card-hover)" : "var(--bg-c)",
        border: `1px solid ${
          selected
            ? "color-mix(in srgb, var(--accent) 45%, var(--panel-border))"
            : hovered
            ? "var(--panel-border-solid)"
            : "var(--panel-border)"
        }`,
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
      {/* Left */}
      <div style={{ display: "flex", alignItems: "center", gap: 7 }}>
        <span style={{
          fontSize: 12,
          fontWeight: 600,
          color: selected ? "var(--accent)" : "var(--ink-soft)",
          fontFamily: "'Roboto Mono', 'Courier New', monospace",
          letterSpacing: "0.02em",
        }}>
          {zone.zoneId}
        </span>
        {isAuto && (
          <span style={{ fontSize: 10, background: "var(--accent-surface-strong)", color: "var(--accent)", borderRadius: 4, padding: "1px 5px", fontWeight: 700 }}>
            AUTO
          </span>
        )}
        {isClosest && !isAuto && (
          <span style={{ fontSize: 10, background: "var(--accent-surface)", color: "var(--accent)", borderRadius: 4, padding: "1px 5px", fontWeight: 700 }}>
            NEAREST
          </span>
        )}
      </div>

      {/* Right */}
      <div style={{ display: "flex", gap: 10, alignItems: "center", flexShrink: 0 }}>
        {isPinging ? (
          <span style={{ fontSize: 11, color: "var(--ink-dim)", fontStyle: "italic" }}>pinging…</span>
        ) : zone.pingMs !== null ? (
          <span style={{ fontSize: 12, color: getPingColor(zone.pingMs), fontWeight: 600, minWidth: 46, textAlign: "right" }}>
            {zone.pingMs}ms
          </span>
        ) : null}
        <span style={{ fontSize: 12, color: getQueueColor(zone.queuePosition), fontWeight: 700, minWidth: 32, textAlign: "right" }}>
          Q:{zone.queuePosition}
        </span>
        {zone.etaMs !== undefined && (
          <span style={{ fontSize: 11, color: "var(--ink-muted)", minWidth: 44, textAlign: "right" }}>
            {formatWait(zone.etaMs)}
          </span>
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
      background: `${color}1a`,
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
      <style>{`@keyframes on-spin{to{transform:rotate(360deg)}}`}</style>
      <div style={{
        display: "inline-block",
        width: 26,
        height: 26,
        border: "3px solid rgba(255,255,255,0.08)",
        borderTop: "3px solid var(--accent)",
        borderRadius: "50%",
        animation: "on-spin 0.75s linear infinite",
      }} />
    </>
  );
}

function MiniSpinner({ color }: { color: string }): JSX.Element {
  return (
    <div style={{
      width: 9,
      height: 9,
      border: `2px solid ${color}33`,
      borderTop: `2px solid ${color}`,
      borderRadius: "50%",
      animation: "on-spin 0.75s linear infinite",
      flexShrink: 0,
    }} />
  );
}

// ── Static styles ─────────────────────────────────────────────────────────────

const overlayStyle: React.CSSProperties = {
  position: "fixed",
  inset: 0,
  zIndex: 1000,
  display: "flex",
  alignItems: "center",
  justifyContent: "center",
  background: "rgba(4, 6, 10, 0.78)",
  backdropFilter: "blur(8px)",
  WebkitBackdropFilter: "blur(8px)",
};

const cardStyle: React.CSSProperties = {
  background: "linear-gradient(180deg, var(--bg-a), var(--bg-b))",
  border: "1px solid var(--panel-border-solid)",
  borderRadius: 16,
  width: "min(700px, 94vw)",
  maxHeight: "86vh",
  display: "flex",
  flexDirection: "column",
  overflow: "hidden",
  boxShadow: "0 24px 64px rgba(0,0,0,0.6)",
};

const scrollBody: React.CSSProperties = {
  flex: 1,
  overflowY: "auto",
  padding: "16px 24px",
  scrollbarWidth: "thin",
  scrollbarColor: "rgba(255,255,255,0.08) transparent",
};

const footerStyle: React.CSSProperties = {
  padding: "12px 24px 20px",
  flexShrink: 0,
  borderTop: "1px solid var(--panel-border)",
  display: "flex",
  alignItems: "center",
  justifyContent: "space-between",
  gap: 12,
};

const closeBtn: React.CSSProperties = {
  background: "var(--bg-c)",
  border: "1px solid var(--panel-border)",
  borderRadius: 8,
  color: "var(--ink-muted)",
  cursor: "pointer",
  fontSize: 16,
  lineHeight: 1,
  padding: "6px 10px",
  flexShrink: 0,
};

const ghostBtn: React.CSSProperties = {
  background: "var(--bg-c)",
  border: "1px solid var(--panel-border-solid)",
  borderRadius: 8,
  color: "var(--ink-soft)",
  cursor: "pointer",
  fontSize: 13,
  fontWeight: 500,
  padding: "8px 18px",
  transition: "background 0.12s",
};

const launchBtn: React.CSSProperties = {
  background: "linear-gradient(135deg, var(--accent), var(--accent-press))",
  border: "none",
  borderRadius: 8,
  color: "var(--accent-on)",
  cursor: "pointer",
  fontSize: 13,
  fontWeight: 700,
  padding: "8px 22px",
  display: "flex",
  alignItems: "center",
  transition: "opacity 0.12s, transform 0.12s, box-shadow 0.12s",
  letterSpacing: "0.02em",
  boxShadow: "0 4px 16px var(--accent-glow)",
};
