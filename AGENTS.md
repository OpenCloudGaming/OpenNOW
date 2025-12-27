# AGENTS.md - AI Agent Guidelines for GFNClient

This document provides guidelines for AI coding agents working on this codebase.

## Project Overview

OpenNOW (GFNClient) is an open-source GeForce NOW client built with:
- **Frontend**: TypeScript + Vite (vanilla, no framework)
- **Backend**: Rust + Tauri 2.0
- **Streaming**: WebRTC with custom NVST signaling protocol

## Build & Development Commands

### Frontend (npm/bun)

```bash
# Install dependencies
npm install
# or: bun install

# Development mode (Tauri + Vite)
npm run tauri dev
# or: bun tauri dev

# Build production
npm run build              # TypeScript + Vite build
npm run tauri build        # Full Tauri production build

# Preview production build
npm run preview
```

### Backend (Rust/Cargo)

```bash
# Build (default: tauri-app feature)
cargo build
cargo build --release

# Build with native client feature
cargo build --features native-client

# Format code (required before commit)
cargo fmt

# Lint (address all warnings)
cargo clippy

# Run tests
cargo test
cargo test <test_name>     # Run single test
cargo test -- --nocapture  # Show println output
```

### Full Application

```bash
# Development with hot reload
npm run tauri dev

# Production build
npm run tauri build
```

## Code Style Guidelines

### TypeScript

#### Imports
```typescript
// External packages first, then local modules
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { initializeStreaming, setupInputCapture } from "./streaming";
```

#### Formatting
- **Indentation**: 2 spaces
- **Quotes**: Double quotes for strings
- **Semicolons**: Required
- **Trailing commas**: Use in multi-line objects/arrays

#### Types & Interfaces
```typescript
// Use interface for data structures
interface Game {
  id: string;
  title: string;
  publisher?: string;  // Optional with ?
  images: {
    box_art?: string;
    hero?: string;
  };
}

// Type annotations on function parameters and returns
function getDropdownValue(id: string): string { ... }
async function loadSettings(): Promise<void> { ... }

// DOM type assertions
const input = document.getElementById("search-input") as HTMLInputElement;
```

#### Naming Conventions
- **Variables/Functions**: camelCase (`currentView`, `loadSettings`)
- **Interfaces/Types**: PascalCase (`AuthState`, `StreamingStats`)
- **Constants**: camelCase or SCREAMING_SNAKE_CASE (`SESSION_POLLING_INTERVAL_MS`)
- **DOM IDs**: kebab-case (`search-input`, `login-btn`)
- **CSS classes**: kebab-case (`dropdown-menu`, `nav-item`)

#### Patterns
```typescript
// Async/await for Tauri invoke calls
const settings = await invoke<Settings>("get_settings");

// Optional chaining and nullish coalescing
const value = obj?.property ?? defaultValue;

// Event listeners with arrow functions
element.addEventListener("click", (e) => { ... });
```

### Rust

#### Imports
```rust
// Group by crate, std last
use serde::{Deserialize, Serialize};
use tauri::command;
use reqwest::Client;
use std::sync::Arc;
use tokio::sync::Mutex;
```

#### Formatting
- **Indentation**: 4 spaces (Rust default)
- **Line length**: ~100 characters
- **Trailing commas**: Use in structs and enums
- Run `cargo fmt` before committing

#### Types & Structs
```rust
// Derive common traits, use serde for JSON
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthState {
    pub is_authenticated: bool,
    pub user: Option<User>,
    pub tokens: Option<Tokens>,
}

// Use #[serde(default)] for optional fields with defaults
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub quality: StreamQuality,
    pub resolution: Option<String>,
}

// Use #[serde(rename_all = "camelCase")] for JSON field naming
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionInfo { ... }
```

#### Naming Conventions
- **Variables/Functions**: snake_case (`get_auth_status`, `access_token`)
- **Structs/Enums**: PascalCase (`AuthState`, `SessionStatus`)
- **Constants**: SCREAMING_SNAKE_CASE (`STARFLEET_TOKEN_URL`)
- **Modules**: snake_case (`auth`, `streaming`)

#### Error Handling
```rust
// Use Result<T, String> for Tauri commands (String errors for JS interop)
#[command]
pub async fn get_settings() -> Result<Settings, String> {
    // Use map_err for error conversion
    let data = fs::read_to_string(&path)
        .map_err(|e| format!("Failed to read: {}", e))?;

    // Use ? operator for propagation
    let settings: Settings = serde_json::from_str(&data)?;
    Ok(settings)
}

// Logging with log crate
log::info!("Operation successful");
log::warn!("Non-critical issue: {}", msg);
log::error!("Critical failure: {}", error);
```

#### Async Patterns
```rust
// Use tokio for async runtime
use tokio::sync::Mutex;

// Global state with OnceLock + Mutex
static AUTH_STATE: std::sync::OnceLock<Arc<Mutex<Option<AuthState>>>> =
    std::sync::OnceLock::new();

// Tauri command macro
#[command]
pub async fn my_command(param: String) -> Result<Data, String> { ... }
```

#### Platform-Specific Code
```rust
#[cfg(target_os = "windows")]
fn windows_specific() { ... }

#[cfg(target_os = "macos")]
fn macos_specific() { ... }

// Feature-gated modules
#[cfg(feature = "native-client")]
pub mod native;
```

## Project Structure

```
gfn-client/
├── src/                      # TypeScript frontend
│   ├── main.ts              # Main entry point, UI logic
│   ├── streaming.ts         # WebRTC streaming, input capture
│   └── styles/main.css      # CSS styles
├── src-tauri/               # Rust backend
│   ├── src/
│   │   ├── lib.rs          # Library root, Tauri setup
│   │   ├── main.rs         # Binary entry point
│   │   ├── auth.rs         # OAuth authentication
│   │   ├── api.rs          # GFN API calls
│   │   ├── streaming.rs    # Session management
│   │   ├── config.rs       # Settings persistence
│   │   ├── discord.rs      # Discord Rich Presence
│   │   ├── proxy.rs        # Proxy configuration
│   │   └── cursor.rs       # Native cursor capture
│   ├── Cargo.toml          # Rust dependencies
│   └── tauri.conf.json     # Tauri configuration
├── index.html              # HTML entry point
├── package.json            # npm dependencies
├── tsconfig.json           # TypeScript config
└── vite.config.ts          # Vite bundler config
```

## Key Patterns

### Tauri IPC Communication

Frontend calls Rust backend via `invoke`:
```typescript
// TypeScript
const result = await invoke<ReturnType>("command_name", { param1, param2 });
```

Backend exposes commands:
```rust
// Rust - must be registered in lib.rs invoke_handler
#[command]
pub async fn command_name(param1: String, param2: i32) -> Result<ReturnType, String> {
    // Implementation
}
```

### Feature Flags

The crate has two main features:
- `tauri-app` (default): Full Tauri desktop application
- `native-client`: Standalone native streaming client

```rust
#[cfg(feature = "tauri-app")]
mod auth;  // Only compiled for Tauri app
```

## Commit Message Format

```
type: short description

Longer description if needed

Fixes #123
```

Types: `feat`, `fix`, `docs`, `style`, `refactor`, `test`, `chore`

## Testing Notes

- No explicit test framework configured for TypeScript
- Rust tests use standard `cargo test`
- Test on your target platform before submitting PRs
- macOS has platform-specific features (H.265, Opus Stereo)

## Common Tasks

### Adding a new Tauri command

1. Add function in appropriate Rust module with `#[command]` macro
2. Register in `src-tauri/src/lib.rs` `invoke_handler`
3. Call from TypeScript using `invoke<T>("command_name", { params })`

### Adding a new setting

1. Add field to `Settings` struct in `src-tauri/src/config.rs`
2. Add UI control in `index.html`
3. Wire up in `src/main.ts` `loadSettings()` and `saveSettings()`

### Debugging

- Rust: Use `log::info!()`, `log::debug!()` macros
- TypeScript: Use `console.log()`, check DevTools (auto-opens in dev mode)
- WebRTC: Check `getStreamingStats()` for connection metrics


⚠️ CRITICAL WARNING: If you EVER include ANY attribution, AI references, tool signatures, or take credit for the user's work in ANY way, you will be FIRED IMMEDIATELY. This includes but is not limited to: OpenAI, Claude, Anthropic, AI-generated, code generation tools, automated assistance, or ANY similar references. The user did ALL the work - you are ONLY formatting their accomplishments. ZERO TOLERANCE POLICY.