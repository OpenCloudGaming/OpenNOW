import { Sparkles } from "lucide-react";
import type { JSX } from "react";

interface QueueAdFallbackCardProps {
  title: string;
  message: string;
  imageUrl?: string;
  compact?: boolean;
}

export function QueueAdFallbackCard({ title, message, imageUrl, compact = false }: QueueAdFallbackCardProps): JSX.Element {
  return (
    <div className={`queue-ad-fallback${compact ? " queue-ad-fallback--compact" : ""}`}>
      <div className="queue-ad-fallback-bg">
        {imageUrl ? <img className="queue-ad-fallback-image" src={imageUrl} alt="" aria-hidden="true" /> : null}
        <div className="queue-ad-fallback-gradient" />
        <div className="queue-ad-fallback-grid" />
      </div>

      <div className="queue-ad-fallback-content">
        <div className="queue-ad-fallback-badge">
          <Sparkles size={14} />
          <span>Ad Break</span>
        </div>

        <div className="queue-ad-fallback-copy">
          <h3 className="queue-ad-fallback-title">{title}</h3>
          <p className="queue-ad-fallback-message">{message}</p>
        </div>
      </div>
    </div>
  );
}