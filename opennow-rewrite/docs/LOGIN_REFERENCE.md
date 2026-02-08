# Login Reference Notes (from current Rust implementation)

This file tracks what has been mirrored from `opennow-streamer/src/auth/mod.rs` into the rewrite bootstrap.

## Mirrored into rewrite base

- `LoginProvider` model + NVIDIA default provider
- PKCE generation with SHA-256 + base64url challenge (`make_pkce_challenge`)
- Callback port probing by binding localhost against known redirect ports
- OAuth authorize URL construction for `login.nvidia.com/authorize`
- Authorization-code extraction from callback query (`code=`), including URL decoding

## Intentionally simplified for bootstrap

- callback listener currently uses synchronous sockets in core auth service
  - move this into a platform/network adapter for cleaner separation
- no token exchange endpoint client yet (`/token`)
- no userinfo decode/fetch yet

## Next auth milestones

1. Add HTTP client abstraction and implement `/token` exchange.
2. Add refresh-token flow and expiry handling.
3. Add secure token persistence abstraction.
4. Add JWT decode for `sub`, `preferred_username`, `email`, `gfn_tier`.
5. Add `/userinfo` fallback.
