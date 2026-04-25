import { Check, Copy } from "lucide-react";
import { useState } from "react";
import type { JSX } from "react";

export function CopyErrorButton({
  text,
  className,
}: {
  text: string;
  className: string;
}): JSX.Element {
  const [copyState, setCopyState] = useState<"idle" | "copied" | "failed">("idle");

  const handleCopy = async (): Promise<void> => {
    try {
      await navigator.clipboard.writeText(text);
      setCopyState("copied");
      window.setTimeout(() => setCopyState("idle"), 1600);
    } catch {
      setCopyState("failed");
      window.setTimeout(() => setCopyState("idle"), 2200);
    }
  };

  return (
    <button
      type="button"
      className={className}
      onClick={() => {
        void handleCopy();
      }}
      aria-label="Copy launch error"
    >
      {copyState === "copied" ? <Check size={14} /> : <Copy size={14} />}
      <span>{copyState === "copied" ? "Copied" : copyState === "failed" ? "Copy Failed" : "Copy Error"}</span>
    </button>
  );
}
