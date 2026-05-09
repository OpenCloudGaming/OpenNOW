import type { RefObject } from "react";
import { useEffect, useRef } from "react";

/**
 * Draws a segmented RMS level meter for a live microphone track (time-domain).
 * Shared by stream sidebar and controller in-stream mic level row.
 */
export function useMicMeter(
  canvasRef: RefObject<HTMLCanvasElement | null>,
  track: MediaStreamTrack | null,
  active: boolean,
): void {
  const pendingCloseRef = useRef<Promise<void> | null>(null);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!active || !track || !canvas) return;

    const ctx2d = canvas.getContext("2d");
    if (!ctx2d) return;

    const dpr = window.devicePixelRatio || 1;
    canvas.width = Math.round(canvas.clientWidth * dpr);
    canvas.height = Math.round(canvas.clientHeight * dpr);
    const W = canvas.width;
    const H = canvas.height;
    if (W <= 0 || H <= 0) {
      return;
    }

    let audioCtx: AudioContext | null = null;
    let source: MediaStreamAudioSourceNode | null = null;
    let analyser: AnalyserNode | null = null;
    let tickTimer: number | null = null;
    let dead = false;

    const start = async () => {
      if (pendingCloseRef.current) {
        try {
          await pendingCloseRef.current;
        } catch {
          // Ignore close errors from previous contexts.
        }
      }
      if (dead) {
        return;
      }

      try {
        audioCtx = new AudioContext();
        await audioCtx.resume().catch(() => undefined);
        if (dead) {
          return;
        }

        analyser = audioCtx.createAnalyser();
        analyser.fftSize = 256;
        analyser.smoothingTimeConstant = 0.65;
        source = audioCtx.createMediaStreamSource(new MediaStream([track]));
        source.connect(analyser);

        const buf = new Uint8Array(analyser.frequencyBinCount);
        const SEG = 20;
        const GAP = Math.round(2 * dpr);
        const bw = (W - GAP * (SEG - 1)) / SEG;
        const radius = Math.min(3 * dpr, bw / 2);
        const frameIntervalMs = 33;

        const frame = () => {
          if (dead || !analyser) return;
          tickTimer = window.setTimeout(frame, frameIntervalMs);
          analyser.getByteTimeDomainData(buf);

          let sum = 0;
          for (let i = 0; i < buf.length; i++) {
            const v = ((buf[i] ?? 128) - 128) / 128;
            sum += v * v;
          }
          const rms = Math.sqrt(sum / buf.length);
          const level = Math.min(1, rms * 5.5);
          const filled = Math.round(level * SEG);

          ctx2d.clearRect(0, 0, W, H);
          for (let i = 0; i < SEG; i++) {
            const x = i * (bw + GAP);
            if (i < filled) {
              ctx2d.fillStyle =
                i < SEG * 0.7 ? "#58d98a" : i < SEG * 0.9 ? "#fbbf24" : "#f87171";
            } else {
              ctx2d.fillStyle = "rgba(255,255,255,0.07)";
            }
            ctx2d.beginPath();
            ctx2d.roundRect(x, 0, Math.max(1, bw), H, radius);
            ctx2d.fill();
          }
        };

        frame();
      } catch (e) {
        console.warn("[MicMeter]", e);
      }
    };

    void start();

    return () => {
      dead = true;
      if (tickTimer !== null) {
        window.clearTimeout(tickTimer);
      }
      source?.disconnect();
      analyser?.disconnect();
      if (audioCtx && audioCtx.state !== "closed") {
        pendingCloseRef.current = audioCtx
          .close()
          .catch(() => undefined)
          .then(() => undefined);
      }
    };
  }, [track, active, canvasRef]);
}
