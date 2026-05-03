export type ControllerUiSoundKind = "move" | "confirm";

let sharedAudioContext: AudioContext | null = null;

function getAudioContext(): AudioContext {
  const ctx = sharedAudioContext ?? new AudioContext();
  sharedAudioContext = ctx;
  return ctx;
}

/** Shared UI beep for controller surfaces (library, overlay toggle in App). Respects `enabled`. */
export function playControllerUiSound(kind: ControllerUiSoundKind, enabled: boolean): void {
  if (!enabled) return;
  const audioContext = getAudioContext();
  if (audioContext.state === "suspended") void audioContext.resume();

  const now = audioContext.currentTime;
  const oscillator = audioContext.createOscillator();
  const gain = audioContext.createGain();

  const profile: Record<
    ControllerUiSoundKind,
    { start: number; end: number; duration: number; volume: number; type: OscillatorType }
  > = {
    move: { start: 720, end: 680, duration: 0.032, volume: 0.009, type: "triangle" },
    confirm: { start: 640, end: 860, duration: 0.07, volume: 0.016, type: "sine" },
  };

  const active = profile[kind];
  oscillator.type = active.type;
  oscillator.frequency.setValueAtTime(active.start, now);
  oscillator.frequency.exponentialRampToValueAtTime(active.end, now + active.duration);
  gain.gain.setValueAtTime(0.0001, now);
  gain.gain.exponentialRampToValueAtTime(active.volume, now + 0.012);
  gain.gain.exponentialRampToValueAtTime(0.0001, now + active.duration);

  oscillator.connect(gain);
  gain.connect(audioContext.destination);
  oscillator.start(now);
  oscillator.stop(now + active.duration + 0.01);
}
