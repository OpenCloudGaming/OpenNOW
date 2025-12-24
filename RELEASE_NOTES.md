## GFN Client v0.0.1 - Development Build

### Features
- **NVIDIA OAuth Login** - Sign in with your NVIDIA account using secure OAuth 2.0 + PKCE flow
- **Subscription Tier Display** - Shows your membership tier (Free/Priority/Ultimate) with colored badge
- **Playtime Tracking** - Displays remaining hours in the navbar (e.g., 64h / 105h)
- **Game Library** - Browse your GFN game library with cover art
- **Discord Rich Presence** - Show what you're playing on Discord

### Technical Details
- Built with Tauri 2.x + Rust backend
- Uses native GFN API endpoints
- Secure token handling with id_token JWT authentication

### Known Bugs
- **Session cleanup** - You need to close the session via the official GFN client (our client doesn't terminate sessions yet)
- **ESC key broken** - ESC key doesn't work properly during streaming (will be fixed)

### Known Limitations
- This is an early development build
- Streaming functionality is work in progress
- Some features may not work as expected

### Downloads
- `gfn-client.exe` - Standalone Windows executable (no installation required)

---
*This is a development preview. Use at your own risk.*
