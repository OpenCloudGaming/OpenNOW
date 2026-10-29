const COLOURS = { green: "#59bf40", yellow: "#f3c44b", blue: "#1a9fff", grey: "#8b929a" };

export function StatusDot({ colour }: { colour: keyof typeof COLOURS }) {
  return (
    <span style={{ display: "inline-block", width: 8, height: 8, borderRadius: 4, background: COLOURS[colour], marginRight: 12, flexShrink: 0 }} />
  );
}

export function Labelled({ dot, children }: { dot: keyof typeof COLOURS; children: string }) {
  return (
    <span style={{ display: "flex", alignItems: "center" }}>
      <StatusDot colour={dot} />
      {children}
    </span>
  );
}
