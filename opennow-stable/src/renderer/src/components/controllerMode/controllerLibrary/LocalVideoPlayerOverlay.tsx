import type { JSX } from "react";

export interface LocalVideoPlayerOverlayProps {
  src: string;
  onClose: () => void;
}

export function LocalVideoPlayerOverlay({ src, onClose }: LocalVideoPlayerOverlayProps): JSX.Element {
  return (
    <div className="xmb-local-video-overlay" role="dialog" aria-label="Video playback">
      <button type="button" className="xmb-local-video-backdrop" aria-label="Close video" onClick={onClose} />
      <div className="xmb-local-video-panel">
        <div className="xmb-local-video-frame">
          <video className="xmb-local-video-element" src={src} controls playsInline autoPlay />
        </div>
      </div>
    </div>
  );
}
