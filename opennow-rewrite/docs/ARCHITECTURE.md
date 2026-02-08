# Rewrite Architecture (Initial)

The rewrite is split into strict layers to keep platform-specific code isolated.

## Layers

1. `core/` (portable)
   - business logic
   - session state machine
   - configuration and feature flags
2. `media/` (mostly portable)
   - GStreamer/WebRTC abstractions
3. `platform/<os>/` (OS-specific)
   - input hooks
   - decode/backend probing
   - packaging hooks
4. `apps/desktop/` (UI shell)
   - app startup
   - shell window and settings UX

## Rule

Only `platform/<os>/` may contain direct OS API calls.
