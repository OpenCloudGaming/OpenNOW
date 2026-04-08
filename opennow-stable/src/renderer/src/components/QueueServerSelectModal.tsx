import { useCallback, useEffect, useMemo, useState } from "react";
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
  if (ms < 30)  return "#22c55e";
  if (ms < 80)  return "#84cc16";
  if (ms < 150) return "#eab308";
  return "#ef4444";
}

function getQueueColor(q: number): string {
  if (q <= 5)  return "#22c55e";
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
  onConfirm: (zoneUrl: string | null) => void;
  onCancel: () => void;
}

export function QueueServerSelectModal({ game, onConfirm, onCancel }: Props): JSX.Element {
  const [queueData,  setQueueData]  = useState<PrintedWasteQueueData | null>(null);
  const [queueLoading, setQueueLoading] = useState(true);
  const [fetchError, setFetchError] = useState<string | null>(null);

  // Ping state — populated after queue data loads
  const [zonePings,  setZonePings]  = useState<Map<string, number | null> | null>(null);
  const [isPinging,  setIsPinging]  = useState(false);

  const [selected, setSelected] = useState<"auto" | "closest" | string>("auto");

  // ── Fetch queue data ──────────────────────────────────────────────────────
  useEffect(() => {
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
  }, []);

  // ── Ping all standard zones once queue data arrives ───────────────────────
  useEffect(() => {
    if (!queueData) return;
    const standardIds = Object.keys(queueData).filter(isStandardZone);
    const regionsToTest = standardIds.map((id) => ({ name: id, url: constructZoneUrl(id) }));
    if (regionsToTest.length === 0) return;

    let cancelled = false;
    setIsPinging(true);
    void (async () => {
      try {
        const results = await window.openNow.pingRegions(regionsToTest);
        if (cancelled) return;
        const map = new Map<string, number | null>();
        for (const r of results) map.set(r.url, r.pingMs);
        setZonePings(map);
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

  const autoIsSameAsClosest =
    autoZone && closestZone && autoZone.zoneId === closestZone.zoneId;

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
      tabIndex={-1}
    >
      <div style={cardStyle}>

        {/* Header */}
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
            <button onClick={onCancel} style={closeBtn} aria-label="Close">✕</button>
          </div>
          <div style={{ height: 1, background: "rgba(255,255,255,0.06)", margin: "16px 0 0" }} />
        </div>

        {/* Scrollable body */}
        <div style={scrollBody}>

          {/* Loading queue */}
          {isLoading && (
            <CenteredNote>
              <Spinner />
              <span style={{ marginTop: 10, display: "block", fontSize: 14, color: "#6b7280" }}>
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
              {/* Recommended */}
              <div style={{ marginBottom: 20 }}>
                <SectionLabel>Recommended</SectionLabel>
                <div style={{
                  display: "grid",
                  gridTemplateColumns:
                    (closestZone && !autoIsSameAsClosest) || isPinging
                      ? "1fr 1fr"
                      : "1fr",
                  gap: 10,
                }}>
                  {/* Auto Selected */}
                  {autoZone && (
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
                      accent="#76b900"
                      onClick={() => setSelected("auto")}
                    />
                  )}

                  {/* Closest / Pinging placeholder */}
                  {(closestZone || isPinging) && !autoIsSameAsClosest && (
                    <RecommendCard
                      label="📍 Closest Server"
                      sublabel={isPinging ? "Measuring latency…" : "Lowest latency to you"}
                      zone={closestZone ?? autoZone!}
                      selected={selected === "closest"}
                      accent="#3b82f6"
                      faded={isPinging || !closestZone}
                      onClick={() => { if (closestZone) setSelected("closest"); }}
                      pinging={isPinging && !closestZone}
                    />
                  )}
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
              <span style={{ fontSize: 13, color: "#6b7280" }}>No server data available.</span>
            </CenteredNote>
          )}
        </div>

        {/* Footer */}
        <div style={footerStyle}>
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
              style={ghostBtn}
              onMouseEnter={(e) => { (e.currentTarget as HTMLButtonElement).style.background = "rgba(255,255,255,0.10)"; }}
              onMouseLeave={(e) => { (e.currentTarget as HTMLButtonElement).style.background = ghostBtn.background as string; }}
            >Cancel</button>
            <button
              onClick={handleConfirm}
              style={launchBtn}
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
    <p style={{ margin: "0 0 10px", fontSize: 11, fontWeight: 600, color: "#4b5563", textTransform: "uppercase", letterSpacing: "0.08em" }}>
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
  zone: ZoneInfo;
  selected: boolean;
  accent: string;
  faded?: boolean;
  pinging?: boolean;
  onClick: () => void;
}

function RecommendCard({ label, sublabel, zone, selected, accent, faded, pinging, onClick }: RecommendCardProps): JSX.Element {
  const [hovered, setHovered] = useState(false);
  const regionMeta = REGION_META[zone.pwRegion] ?? { label: zone.pwRegion, flag: "🌐" };

  return (
    <button
      onClick={onClick}
      onMouseEnter={() => setHovered(true)}
      onMouseLeave={() => setHovered(false)}
      style={{
        background: selected ? `${accent}18` : hovered ? "rgba(255,255,255,0.05)" : "rgba(255,255,255,0.02)",
        border: `1px solid ${selected ? accent : hovered ? "rgba(255,255,255,0.14)" : "rgba(255,255,255,0.07)"}`,
        borderRadius: 10,
        padding: "13px 14px",
        cursor: faded ? "default" : "pointer",
        textAlign: "left",
        width: "100%",
        opacity: faded ? 0.55 : 1,
        transition: "border-color 0.12s, background 0.12s, opacity 0.12s",
      }}
    >
      <div style={{ fontSize: 11, fontWeight: 700, color: selected ? accent : "#6b7280", marginBottom: 1, textTransform: "uppercase", letterSpacing: "0.05em" }}>
        {label}
      </div>
      <div style={{ fontSize: 11, color: "#4b5563", marginBottom: 10, display: "flex", alignItems: "center", gap: 5 }}>
        {pinging && <MiniSpinner color={accent} />}
        {sublabel}
      </div>
      <div style={{ fontSize: 14, fontWeight: 600, color: "#e5e7eb", marginBottom: 8 }}>
        {regionMeta.flag} {pinging ? "—" : zone.zoneId}
      </div>
      {!pinging && (
        <div style={{ display: "flex", gap: 6, flexWrap: "wrap" }}>
          {zone.pingMs !== null && <Chip color={getPingColor(zone.pingMs)}>{zone.pingMs}ms</Chip>}
          <Chip color={getQueueColor(zone.queuePosition)}>Queue: {zone.queuePosition}</Chip>
          {zone.etaMs !== undefined && <Chip color="#6b7280">{formatWait(zone.etaMs)} wait</Chip>}
        </div>
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
        background: selected ? "rgba(118,185,0,0.08)" : hovered ? "rgba(255,255,255,0.04)" : "rgba(255,255,255,0.02)",
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
      {/* Left */}
      <div style={{ display: "flex", alignItems: "center", gap: 7 }}>
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

      {/* Right */}
      <div style={{ display: "flex", gap: 10, alignItems: "center", flexShrink: 0 }}>
        {isPinging ? (
          <span style={{ fontSize: 11, color: "#4b5563", fontStyle: "italic" }}>pinging…</span>
        ) : zone.pingMs !== null ? (
          <span style={{ fontSize: 12, color: getPingColor(zone.pingMs), fontWeight: 600, minWidth: 46, textAlign: "right" }}>
            {zone.pingMs}ms
          </span>
        ) : null}
        <span style={{ fontSize: 12, color: getQueueColor(zone.queuePosition), fontWeight: 700, minWidth: 32, textAlign: "right" }}>
          Q:{zone.queuePosition}
        </span>
        {zone.etaMs !== undefined && (
          <span style={{ fontSize: 11, color: "#6b7280", minWidth: 44, textAlign: "right" }}>
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
        borderTop: "3px solid #76b900",
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
  background: "rgba(0,0,0,0.82)",
  backdropFilter: "blur(8px)",
  WebkitBackdropFilter: "blur(8px)",
};

const cardStyle: React.CSSProperties = {
  background: "linear-gradient(160deg, #111827 0%, #0d1117 100%)",
  border: "1px solid rgba(255,255,255,0.08)",
  borderRadius: 16,
  width: "min(700px, 94vw)",
  maxHeight: "86vh",
  display: "flex",
  flexDirection: "column",
  overflow: "hidden",
  boxShadow: "0 28px 72px rgba(0,0,0,0.65), 0 0 0 1px rgba(255,255,255,0.03)",
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
  borderTop: "1px solid rgba(255,255,255,0.06)",
  display: "flex",
  alignItems: "center",
  justifyContent: "space-between",
  gap: 12,
};

const closeBtn: React.CSSProperties = {
  background: "rgba(255,255,255,0.06)",
  border: "none",
  borderRadius: 8,
  color: "#9ca3af",
  cursor: "pointer",
  fontSize: 16,
  lineHeight: 1,
  padding: "6px 10px",
  flexShrink: 0,
};

const ghostBtn: React.CSSProperties = {
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

const launchBtn: React.CSSProperties = {
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
