import type { MainToRendererSignalingEvent } from "@shared/gfn";

type NativeInputLifecycleEvent = Extract<
  MainToRendererSignalingEvent,
  { type: "native-stream-started" | "native-input-ready" | "native-input-unavailable" | "native-stream-stopped" }
>;

export type NativeInputLifecycleAction =
  | { state: "pending"; activate: false }
  | { state: "ready"; activate: true; protocolVersion: number }
  | { state: "unavailable"; activate: false; reason: string };

export function nativeInputLifecycleAction(
  event: NativeInputLifecycleEvent,
): NativeInputLifecycleAction {
  if (event.type === "native-input-ready") {
    return {
      state: "ready",
      activate: true,
      protocolVersion: event.protocolVersion,
    };
  }
  if (event.type === "native-input-unavailable") {
    return { state: "unavailable", activate: false, reason: event.reason };
  }
  if (event.type === "native-stream-stopped") {
    return {
      state: "unavailable",
      activate: false,
      reason: event.reason ?? "Native streamer stopped",
    };
  }
  return { state: "pending", activate: false };
}
