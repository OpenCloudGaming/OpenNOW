// GFN Custom Client - Main Entry Point
import { invoke } from "@tauri-apps/api/core";
import {
  initializeStreaming,
  setupInputCapture,
  setInputCaptureMode,
  stopStreaming,
  getStreamingStats,
  isStreamingActive,
  forceInputHandshake,
  isInputReady,
  getInputDebugInfo,
} from "./streaming";

// Types
interface Game {
  id: string;
  title: string;
  publisher?: string;
  developer?: string;
  genres?: string[];
  images: {
    box_art?: string;
    hero?: string;
    thumbnail?: string;
    screenshots?: string[];
  };
  store: {
    store_type: string;
    store_id: string;
    store_url?: string;
  };
  status?: string;
  supported_controls?: string[];
}

interface AuthState {
  is_authenticated: boolean;
  user?: {
    user_id: string;
    display_name: string;
    email?: string;
    avatar_url?: string;
    membership_tier: string;
  };
}

interface Settings {
  quality: string;
  codec: string;
  max_bitrate_mbps: number;
  region?: string;
  discord_rpc: boolean;
  proxy?: string;
  disable_telemetry: boolean;
}

interface ProxyConfig {
  enabled: boolean;
  proxy_type: string;
  host: string;
  port: number;
  username?: string;
  password?: string;
  bypass_local: boolean;
  bypass_list: string[];
}

interface Server {
  id: string;
  name: string;
  region: string;
  country: string;
  ping_ms?: number;
  queue_size?: number;
  status: string;
}

// State
let currentView = "home";
let isAuthenticated = false;
let currentUser: AuthState["user"] | null = null;
let games: Game[] = [];
let discordRpcEnabled = false; // Discord presence toggle
let currentQuality = "auto"; // Current quality preset
let currentCodec = "h264"; // Current video codec
let currentMaxBitrate = 200; // Max bitrate in Mbps (200 = unlimited)

// Helper to get streaming params from quality preset
function getStreamingParams(quality: string): { resolution: string; fps: number } {
  switch (quality) {
    case "low":
      return { resolution: "1280x720", fps: 30 };
    case "medium":
      return { resolution: "1920x1080", fps: 60 };
    case "high":
      return { resolution: "2560x1440", fps: 60 };
    case "ultra":
      return { resolution: "3840x2160", fps: 60 };
    case "high120":
      return { resolution: "1920x1080", fps: 120 };
    case "ultra120":
      return { resolution: "2560x1440", fps: 120 };
    case "competitive":
      return { resolution: "1920x1080", fps: 240 };
    case "extreme":
      return { resolution: "1920x1080", fps: 360 };
    default: // auto
      return { resolution: "1920x1080", fps: 60 };
  }
}

// DOM Elements
const loginBtn = document.getElementById("login-btn")!;
const userMenu = document.getElementById("user-menu")!;
const settingsBtn = document.getElementById("settings-btn")!;
const searchInput = document.getElementById("search-input") as HTMLInputElement;
const navItems = document.querySelectorAll(".nav-item");

// Initialize
document.addEventListener("DOMContentLoaded", async () => {
  console.log("GFN Custom Client initialized");

  // Setup navigation
  setupNavigation();

  // Setup modals
  setupModals();

  // Setup login modal
  setupLoginModal();

  // Setup search
  setupSearch();

  // Load saved settings
  await loadSettings();

  // Check auth status
  await checkAuthStatus();

  // Load initial data
  await loadHomeData();
});

// Load settings from backend and apply to UI
async function loadSettings() {
  try {
    const settings = await invoke<Settings>("get_settings");
    console.log("Loaded settings:", settings);

    // Apply to global state
    currentQuality = settings.quality || "auto";
    currentCodec = settings.codec || "h264";
    currentMaxBitrate = settings.max_bitrate_mbps || 200;
    discordRpcEnabled = settings.discord_rpc || false;

    // Apply to UI elements
    const qualityEl = document.getElementById("quality-setting") as HTMLSelectElement;
    const codecEl = document.getElementById("codec-setting") as HTMLSelectElement;
    const bitrateEl = document.getElementById("bitrate-setting") as HTMLInputElement;
    const bitrateValueEl = document.getElementById("bitrate-value");
    const discordEl = document.getElementById("discord-setting") as HTMLInputElement;
    const telemetryEl = document.getElementById("telemetry-setting") as HTMLInputElement;
    const proxyEl = document.getElementById("proxy-setting") as HTMLInputElement;

    if (qualityEl) qualityEl.value = currentQuality;
    if (codecEl) codecEl.value = currentCodec;
    if (bitrateEl) {
      bitrateEl.value = String(currentMaxBitrate);
      if (bitrateValueEl) {
        bitrateValueEl.textContent = currentMaxBitrate >= 200 ? "Unlimited" : `${currentMaxBitrate} Mbps`;
      }
    }
    if (discordEl) discordEl.checked = discordRpcEnabled;
    if (telemetryEl) telemetryEl.checked = settings.disable_telemetry ?? true;
    if (proxyEl && settings.proxy) proxyEl.value = settings.proxy;

  } catch (error) {
    console.warn("Failed to load settings:", error);
  }
}

// Navigation
function setupNavigation() {
  navItems.forEach((item) => {
    item.addEventListener("click", (e) => {
      e.preventDefault();
      const view = (item as HTMLElement).dataset.view;
      if (view) {
        switchView(view);
      }
    });
  });
}

function switchView(view: string) {
  // Update nav
  navItems.forEach((item) => {
    item.classList.toggle("active", (item as HTMLElement).dataset.view === view);
  });

  // Update views - only toggle active class, don't use hidden for views
  document.querySelectorAll(".view").forEach((v) => {
    const isActive = v.id === `${view}-view`;
    v.classList.toggle("active", isActive);
    // Remove hidden class from views - CSS handles visibility via :not(.active)
    v.classList.remove("hidden");
  });

  currentView = view;

  // Load view-specific data
  if (view === "library") {
    loadLibraryData();
  } else if (view === "store") {
    loadStoreData();
  }
}

// Modals
function setupModals() {
  // Settings modal
  settingsBtn.addEventListener("click", () => {
    showModal("settings-modal");
  });

  // Close buttons
  document.querySelectorAll(".modal-close").forEach((btn) => {
    btn.addEventListener("click", () => {
      hideAllModals();
    });
  });

  // Click outside to close
  document.querySelectorAll(".modal").forEach((modal) => {
    modal.addEventListener("click", (e) => {
      if (e.target === modal) {
        hideAllModals();
      }
    });
  });

  // Save settings
  document.getElementById("save-settings-btn")?.addEventListener("click", saveSettings);

  // Bitrate slider live update
  const bitrateSlider = document.getElementById("bitrate-setting") as HTMLInputElement;
  const bitrateValue = document.getElementById("bitrate-value");
  bitrateSlider?.addEventListener("input", () => {
    const value = parseInt(bitrateSlider.value, 10);
    if (bitrateValue) {
      bitrateValue.textContent = value >= 200 ? "Unlimited" : `${value} Mbps`;
    }
  });

  // Logout button
  document.getElementById("logout-btn")?.addEventListener("click", async () => {
    try {
      await invoke("logout");
      // Reset UI state
      isAuthenticated = false;
      currentUser = null;
      loginBtn.classList.remove("hidden");
      userMenu.classList.add("hidden");
      // Reload the page to reset everything
      window.location.reload();
    } catch (error) {
      console.error("Logout failed:", error);
    }
  });
}

function showModal(modalId: string) {
  document.getElementById(modalId)?.classList.remove("hidden");
}

function hideAllModals() {
  document.querySelectorAll(".modal").forEach((modal) => {
    modal.classList.add("hidden");
  });
}

// Search
function setupSearch() {
  let searchTimeout: number;

  searchInput.addEventListener("input", () => {
    clearTimeout(searchTimeout);
    searchTimeout = setTimeout(() => {
      const query = searchInput.value.trim();
      if (query.length >= 2) {
        searchGames(query);
      }
    }, 300);
  });
}

async function searchGames(query: string) {
  try {
    const results = await invoke<{ games: Game[] }>("search_games", {
      query,
      limit: 20,
    });
    console.log("Search results:", results);
  } catch (error) {
    console.error("Search failed:", error);
  }
}

// Authentication
async function checkAuthStatus() {
  try {
    const status = await invoke<AuthState>("get_auth_status");
    isAuthenticated = status.is_authenticated;
    currentUser = status.user || null;
    updateAuthUI();
  } catch (error) {
    console.error("Failed to check auth status:", error);
  }
}

function updateAuthUI() {
  if (isAuthenticated && currentUser) {
    loginBtn.classList.add("hidden");
    userMenu.classList.remove("hidden");
    const userName = document.getElementById("user-name");
    if (userName) {
      userName.textContent = currentUser.display_name;
    }
    if (currentUser.avatar_url) {
      const avatar = document.getElementById("user-avatar") as HTMLImageElement;
      if (avatar) {
        avatar.src = currentUser.avatar_url;
      }
    }
  } else {
    loginBtn.classList.remove("hidden");
    userMenu.classList.add("hidden");
  }
}

// Show login modal when login button is clicked
loginBtn.addEventListener("click", () => {
  showModal("login-modal");
});

// Setup login modal handlers
function setupLoginModal() {
  const loginModal = document.getElementById("login-modal");
  const nvidiaLoginBtn = document.getElementById("nvidia-login-btn");
  const tokenLoginBtn = document.getElementById("token-login-btn");
  const tokenEntry = document.getElementById("token-entry");
  const loginOptions = loginModal?.querySelector(".login-options");
  const submitTokenBtn = document.getElementById("submit-token-btn");
  const tokenInput = document.getElementById("token-input") as HTMLTextAreaElement;

  // NVIDIA OAuth login
  nvidiaLoginBtn?.addEventListener("click", async () => {
    console.log("Starting NVIDIA OAuth login...");
    nvidiaLoginBtn.textContent = "Signing in...";
    (nvidiaLoginBtn as HTMLButtonElement).disabled = true;

    try {
      const result = await invoke<AuthState>("login_oauth");
      if (result.is_authenticated) {
        isAuthenticated = true;
        currentUser = result.user || null;
        updateAuthUI();
        hideAllModals();
        console.log("NVIDIA OAuth login successful");
      }
    } catch (error) {
      console.error("NVIDIA OAuth login failed:", error);
      alert("Login failed: " + error);
    } finally {
      nvidiaLoginBtn.textContent = "Sign in with NVIDIA";
      (nvidiaLoginBtn as HTMLButtonElement).disabled = false;
    }
  });

  // Show token entry form
  tokenLoginBtn?.addEventListener("click", () => {
    if (loginOptions) (loginOptions as HTMLElement).classList.add("hidden");
    if (tokenEntry) tokenEntry.classList.remove("hidden");
  });

  // Submit token
  submitTokenBtn?.addEventListener("click", async () => {
    const token = tokenInput?.value.trim();
    if (!token) {
      alert("Please enter a token");
      return;
    }

    submitTokenBtn.textContent = "Validating...";
    (submitTokenBtn as HTMLButtonElement).disabled = true;

    try {
      const result = await invoke<AuthState>("set_access_token", { token });
      if (result.is_authenticated) {
        isAuthenticated = true;
        currentUser = result.user || null;
        updateAuthUI();
        hideAllModals();
        // Reset form
        if (tokenInput) tokenInput.value = "";
        if (loginOptions) (loginOptions as HTMLElement).classList.remove("hidden");
        if (tokenEntry) tokenEntry.classList.add("hidden");
        console.log("Token login successful");
      }
    } catch (error) {
      console.error("Token validation failed:", error);
      alert("Invalid token: " + error);
    } finally {
      submitTokenBtn.textContent = "Submit Token";
      (submitTokenBtn as HTMLButtonElement).disabled = false;
    }
  });

  // Reset login modal when closed
  loginModal?.querySelector(".modal-close")?.addEventListener("click", () => {
    if (loginOptions) (loginOptions as HTMLElement).classList.remove("hidden");
    if (tokenEntry) tokenEntry.classList.add("hidden");
    if (tokenInput) tokenInput.value = "";
  });
}

// Data Loading
async function loadHomeData() {
  console.log("Loading home data...");

  // Create placeholder games initially
  const placeholderGames = createPlaceholderGames();
  renderGamesGrid("featured-games", placeholderGames.slice(0, 6));
  renderGamesGrid("recent-games", placeholderGames.slice(6, 12));
  renderGamesGrid("free-games", placeholderGames.slice(12, 18));

  // Try to load library data (requires authentication)
  if (isAuthenticated) {
    console.log("User is authenticated, trying fetch_library...");
    try {
      const accessToken = await invoke<string>("get_access_token");
      console.log("Got access token, calling fetch_library...");
      const response = await invoke<{ games: Game[] }>("fetch_library", {
        accessToken,
        vpcId: null, // Use default (Amsterdam)
      });
      console.log("fetch_library response:", response);
      if (response.games.length > 0) {
        games = response.games;
        console.log("Loaded", games.length, "games from library with images");
        console.log("First game:", games[0]);
        renderGamesGrid("featured-games", games.slice(0, 6));
        renderGamesGrid("recent-games", games.slice(6, 12));
        renderGamesGrid("free-games", games.slice(12, 18));
      } else {
        console.log("Library returned 0 games, trying fetch_main_games...");
        throw new Error("Empty library");
      }
    } catch (error) {
      console.error("Failed to load library:", error);
      // Fall back to main games panel
      console.log("Falling back to fetch_main_games...");
      try {
        const accessToken = await invoke<string>("get_access_token").catch(() => null);
        const response = await invoke<{ games: Game[] }>("fetch_main_games", {
          accessToken,
          vpcId: null,
        });
        console.log("fetch_main_games response:", response);
        if (response.games.length > 0) {
          games = response.games;
          console.log("Loaded", games.length, "games from main panel");
          console.log("First game:", games[0]);
          renderGamesGrid("featured-games", games.slice(0, 6));
          renderGamesGrid("recent-games", games.slice(6, 12));
          renderGamesGrid("free-games", games.slice(12, 18));
        }
      } catch (e) {
        console.error("Failed to load main games:", e);
        // Final fallback to static games
        console.log("Falling back to fetch_games (no images)...");
        try {
          const response = await invoke<{ games: Game[] }>("fetch_games", {
            limit: 50,
            offset: 0,
          });
          if (response.games.length > 0) {
            games = response.games;
            renderGamesGrid("featured-games", games.slice(0, 6));
          }
        } catch (e2) {
          console.error("All game loading failed:", e2);
        }
      }
    }
  } else {
    // Not authenticated - load public main games panel (has proper images)
    console.log("Not authenticated, trying fetch_main_games...");
    try {
      const response = await invoke<{ games: Game[] }>("fetch_main_games", {
        accessToken: null,
        vpcId: null,
      });
      console.log("fetch_main_games response:", response);
      if (response.games.length > 0) {
        games = response.games;
        console.log("Loaded", games.length, "public games with images");
        console.log("First game images:", games[0]?.images);
        renderGamesGrid("featured-games", games.slice(0, 6));
        renderGamesGrid("recent-games", games.slice(6, 12));
        renderGamesGrid("free-games", games.slice(12, 18));
      }
    } catch (error) {
      console.error("Failed to load public games from fetch_main_games:", error);
      // Final fallback to static list (no images)
      console.log("Falling back to fetch_games...");
      try {
        const response = await invoke<{ games: Game[] }>("fetch_games", {
          limit: 50,
          offset: 0,
        });
        console.log("fetch_games response, games count:", response.games.length);
        if (response.games.length > 0) {
          games = response.games;
          renderGamesGrid("featured-games", games.slice(0, 6));
        }
      } catch (e) {
        console.error("Failed to load static games:", e);
      }
    }
  }
}

async function loadLibraryData() {
  console.log("Loading library data...");
}

async function loadStoreData() {
  console.log("Loading store data...");
  const placeholderGames = createPlaceholderGames();
  renderGamesGrid("all-games", placeholderGames);
}

// Generate fallback placeholder SVG
function getFallbackPlaceholder(title: string): string {
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="180" height="240" viewBox="0 0 180 240">
    <rect fill="#252525" width="180" height="240"/>
    <rect fill="#76b900" x="20" y="80" width="140" height="80" rx="8"/>
    <text x="90" y="128" font-family="Arial,sans-serif" font-size="12" fill="white" text-anchor="middle">${title.substring(0, 15)}</text>
  </svg>`;
  return `data:image/svg+xml,${encodeURIComponent(svg)}`;
}

// Safe DOM element creation
function createGameCard(game: Game): HTMLElement {
  const card = document.createElement("div");
  card.className = "game-card";
  card.dataset.gameId = game.id;

  const img = document.createElement("img");
  img.className = "game-card-image";
  img.alt = game.title;
  img.loading = "lazy";
  img.referrerPolicy = "no-referrer"; // Bypass referrer check for NVIDIA CDN
  img.crossOrigin = "anonymous"; // Allow cross-origin loading

  // Use fallback if no image provided
  const imageSrc = game.images.box_art || game.images.thumbnail;
  if (imageSrc) {
    img.src = imageSrc;
    img.onerror = () => {
      img.src = getFallbackPlaceholder(game.title);
      img.onerror = null; // Prevent infinite loop
    };
  } else {
    img.src = getFallbackPlaceholder(game.title);
  }

  const info = document.createElement("div");
  info.className = "game-card-info";

  const title = document.createElement("div");
  title.className = "game-card-title";
  title.textContent = game.title;

  const store = document.createElement("div");
  store.className = "game-card-store";
  store.textContent = game.store.store_type;

  info.appendChild(title);
  info.appendChild(store);
  card.appendChild(img);
  card.appendChild(info);

  card.addEventListener("click", () => {
    showGameDetail(game.id);
  });

  return card;
}

function renderGamesGrid(containerId: string, gamesList: Game[]) {
  const container = document.getElementById(containerId);
  if (!container) return;

  // Clear existing content
  container.replaceChildren();

  // Add game cards using safe DOM methods
  gamesList.forEach((game) => {
    container.appendChild(createGameCard(game));
  });
}

function createGameDetailElement(game: Game): HTMLElement {
  const wrapper = document.createElement("div");
  wrapper.className = "game-detail-wrapper";

  // Hero section with gradient overlay
  const hero = document.createElement("div");
  hero.className = "game-detail-hero";
  hero.style.backgroundImage = `linear-gradient(to bottom, transparent 0%, rgba(26,26,46,0.8) 60%, rgba(26,26,46,1) 100%), url('${game.images.hero || game.images.box_art || ""}')`;

  // Content container
  const content = document.createElement("div");
  content.className = "game-detail-content";

  // Box art
  const boxArt = document.createElement("img");
  boxArt.className = "game-detail-boxart";
  boxArt.src = game.images.box_art || game.images.thumbnail || getFallbackPlaceholder(game.title);
  boxArt.alt = game.title;
  boxArt.onerror = () => { boxArt.src = getFallbackPlaceholder(game.title); };

  // Info section
  const info = document.createElement("div");
  info.className = "game-detail-info";

  const titleEl = document.createElement("h1");
  titleEl.className = "game-detail-title";
  titleEl.textContent = game.title;

  const meta = document.createElement("div");
  meta.className = "game-detail-meta";

  // Publisher/Developer
  if (game.publisher || game.developer) {
    const pubDev = document.createElement("span");
    pubDev.textContent = game.developer
      ? `${game.developer}${game.publisher && game.publisher !== game.developer ? ` / ${game.publisher}` : ""}`
      : game.publisher || "";
    meta.appendChild(pubDev);
  }

  // Store badge
  const storeBadge = document.createElement("span");
  storeBadge.className = `store-badge store-${game.store.store_type.toLowerCase()}`;
  storeBadge.textContent = game.store.store_type;
  meta.appendChild(storeBadge);

  // Status indicator
  if (game.status) {
    const statusBadge = document.createElement("span");
    statusBadge.className = `status-badge status-${game.status.toLowerCase()}`;
    statusBadge.textContent = game.status === "Available" ? "Ready to Play" : game.status;
    meta.appendChild(statusBadge);
  }

  // Genres
  if (game.genres && game.genres.length > 0) {
    const genres = document.createElement("div");
    genres.className = "game-detail-genres";
    game.genres.slice(0, 4).forEach((genre) => {
      const genreTag = document.createElement("span");
      genreTag.className = "genre-tag";
      genreTag.textContent = genre;
      genres.appendChild(genreTag);
    });
    info.appendChild(genres);
  }

  // Controls supported
  if (game.supported_controls && game.supported_controls.length > 0) {
    const controls = document.createElement("div");
    controls.className = "game-detail-controls";
    const controlsLabel = document.createElement("span");
    controlsLabel.className = "controls-label";
    controlsLabel.textContent = "Supported Controls: ";
    controls.appendChild(controlsLabel);
    controls.appendChild(document.createTextNode(game.supported_controls.join(", ")));
    info.appendChild(controls);
  }

  const desc = document.createElement("div");
  desc.className = "game-detail-description";
  desc.textContent = "Experience this game through GeForce NOW cloud gaming. Stream instantly without downloads.";

  // Actions
  const actions = document.createElement("div");
  actions.className = "game-detail-actions";

  const playBtn = document.createElement("button");
  playBtn.className = "btn btn-primary btn-large";
  playBtn.textContent = "Play Now";
  playBtn.addEventListener("click", () => launchGame(game));

  const favBtn = document.createElement("button");
  favBtn.className = "btn btn-secondary";
  favBtn.textContent = "♡ Add to Library";
  favBtn.addEventListener("click", async () => {
    favBtn.textContent = "♥ Added";
    favBtn.classList.add("favorited");
    // TODO: Call add_favorite API
  });

  const storeBtn = document.createElement("button");
  storeBtn.className = "btn btn-secondary";
  storeBtn.textContent = `View on ${game.store.store_type}`;
  storeBtn.addEventListener("click", () => {
    if (game.store.store_url) {
      window.open(game.store.store_url, "_blank");
    }
  });

  actions.appendChild(playBtn);
  actions.appendChild(favBtn);
  if (game.store.store_url) {
    actions.appendChild(storeBtn);
  }

  info.appendChild(titleEl);
  info.appendChild(meta);
  info.appendChild(desc);
  info.appendChild(actions);

  content.appendChild(boxArt);
  content.appendChild(info);

  wrapper.appendChild(hero);
  wrapper.appendChild(content);

  // Screenshots section
  if (game.images.screenshots && game.images.screenshots.length > 0) {
    const screenshotsSection = document.createElement("div");
    screenshotsSection.className = "game-detail-screenshots";

    const screenshotsTitle = document.createElement("h3");
    screenshotsTitle.textContent = "Screenshots";
    screenshotsSection.appendChild(screenshotsTitle);

    const screenshotsGrid = document.createElement("div");
    screenshotsGrid.className = "screenshots-grid";

    game.images.screenshots.slice(0, 4).forEach((url) => {
      const screenshot = document.createElement("img");
      screenshot.className = "screenshot";
      screenshot.src = url;
      screenshot.alt = "Screenshot";
      screenshot.addEventListener("click", () => {
        // TODO: Lightbox
        window.open(url, "_blank");
      });
      screenshotsGrid.appendChild(screenshot);
    });

    screenshotsSection.appendChild(screenshotsGrid);
    wrapper.appendChild(screenshotsSection);
  }

  return wrapper;
}

async function showGameDetail(gameId: string) {
  const game = games.find((g) => g.id === gameId) || createPlaceholderGames().find((g) => g.id === gameId);
  if (!game) return;

  const detailContainer = document.getElementById("game-detail");
  if (!detailContainer) return;

  // Clear and append new content safely
  detailContainer.replaceChildren();
  detailContainer.appendChild(createGameDetailElement(game));

  showModal("game-modal");
}

// Streaming state
interface StreamingUIState {
  active: boolean;
  sessionId: string | null;
  gameName: string | null;
  phase: string;
  gpuType: string | null;
  serverIp: string | null;
  inputCleanup: (() => void) | null;
  statsInterval: number | null;
  escCleanup: (() => void) | null;
}

let streamingUIState: StreamingUIState = {
  active: false,
  sessionId: null,
  gameName: null,
  phase: "idle",
  gpuType: null,
  serverIp: null,
  inputCleanup: null,
  statsInterval: null,
  escCleanup: null,
};

async function launchGame(game: Game) {
  console.log("Launching game:", game.title);
  hideAllModals();

  // Get the access token first
  let accessToken: string;
  try {
    accessToken = await invoke<string>("get_access_token");
  } catch (e) {
    console.error("Not authenticated:", e);
    alert("Please login first to launch games.");
    return;
  }

  // Show streaming overlay
  showStreamingOverlay(game.title, "Requesting session...");

  // Update Discord presence to show in queue (if enabled)
  if (discordRpcEnabled) {
    try {
      await invoke("set_queue_presence", {
        gameName: game.title,
        queuePosition: null,
        etaSeconds: null,
      });
    } catch (e) {
      console.warn("Discord presence update failed:", e);
    }
  }

  try {
    // Phase 1: Start session
    console.log("Starting session with game ID:", game.id);
    updateStreamingStatus("Creating session...");

    const streamParams = getStreamingParams(currentQuality);
    console.log("Using streaming params:", streamParams, "quality:", currentQuality);

    const sessionResult = await invoke<{
      session_id: string;
      server: { ip: string; id: string };
    }>("start_session", {
      request: {
        game_id: game.id,
        store_type: game.store.store_type,
        store_id: game.store.store_id,
        quality_preset: currentQuality,
        resolution: streamParams.resolution,
        fps: streamParams.fps,
        codec: currentCodec,
        max_bitrate_mbps: currentMaxBitrate,
      },
      accessToken: accessToken,
    });

    console.log("Session created:", sessionResult);
    streamingUIState.sessionId = sessionResult.session_id;
    streamingUIState.gameName = game.title;
    streamingUIState.active = true;

    // Phase 2: Poll until ready and start streaming
    updateStreamingStatus("Waiting for server...");

    const streamingResult = await invoke<{
      session_id: string;
      phase: string;
      server_ip: string | null;
      signaling_url: string | null;
      gpu_type: string | null;
      connection_info: {
        control_ip: string;
        control_port: number;
        stream_ip: string | null;
        stream_port: number;
        resource_path: string;
      } | null;
      error: string | null;
    }>("start_streaming_flow", {
      sessionId: sessionResult.session_id,
      accessToken: accessToken,
    });

    console.log("Streaming ready:", streamingResult);
    streamingUIState.phase = streamingResult.phase;
    streamingUIState.gpuType = streamingResult.gpu_type;
    streamingUIState.serverIp = streamingResult.server_ip;

    // Update overlay with success
    updateStreamingStatus(`Connected to ${streamingResult.gpu_type || "GPU"}`);

    // Update Discord presence to show playing (if enabled)
    if (discordRpcEnabled) {
      try {
        await invoke("set_game_presence", {
          gameName: game.title,
          gameId: game.id,
        });
      } catch (e) {
        console.warn("Discord presence update failed:", e);
      }
    }

    // Show streaming info
    showStreamingInfo(streamingResult);

    // Phase 3: Initialize WebRTC video streaming
    updateStreamingStatus("Starting video stream...");

    // Create fullscreen streaming container
    const streamContainer = createStreamingContainer(game.title);

    try {
      // Initialize WebRTC streaming
      await initializeStreaming(streamingResult, accessToken, streamContainer);

      // Set up input capture
      const videoElement = document.getElementById("gfn-stream-video") as HTMLVideoElement;
      if (videoElement) {
        streamingUIState.inputCleanup = setupInputCapture(videoElement);
      }

      // Start stats monitoring
      streamingUIState.statsInterval = window.setInterval(async () => {
        if (isStreamingActive()) {
          const stats = await getStreamingStats();
          if (stats) {
            updateStreamingStatsDisplay(stats);
          }
        }
      }, 1000);

      console.log("Video streaming initialized");
    } catch (streamError) {
      console.error("Failed to initialize video stream:", streamError);
      updateStreamingStatus(`Video error: ${streamError}`);
    }

  } catch (error) {
    console.error("Failed to launch game:", error);
    streamingUIState.active = false;

    // Hide overlay and show error
    hideStreamingOverlay();

    // Reset Discord presence (if enabled)
    if (discordRpcEnabled) {
      try {
        await invoke("set_browsing_presence");
      } catch (e) {
        // Ignore
      }
    }

    alert(`Failed to launch game: ${error}`);
  }
}

// Create fullscreen streaming container
function createStreamingContainer(gameName: string): HTMLElement {
  // Remove existing container if any
  const existing = document.getElementById("streaming-container");
  if (existing) existing.remove();

  const container = document.createElement("div");
  container.id = "streaming-container";
  container.innerHTML = `
    <div class="stream-video-wrapper">
      <!-- Video element will be inserted here by streaming.ts -->
    </div>
    <div class="stream-overlay">
      <div class="stream-header">
        <span class="stream-game-name">${gameName}</span>
        <div class="stream-controls">
          <button class="stream-btn" id="stream-fullscreen-btn" title="Fullscreen">⛶</button>
          <button class="stream-btn" id="stream-settings-btn" title="Settings">⚙</button>
          <button class="stream-btn stream-btn-danger" id="stream-exit-btn" title="Exit">✕</button>
        </div>
      </div>
      <div class="stream-stats" id="stream-stats">
        <span id="stats-fps">-- FPS</span>
        <span id="stats-latency">-- ms</span>
        <span id="stats-resolution">----x----</span>
        <span id="stats-codec">----</span>
        <span id="stats-bitrate">-- Mbps</span>
      </div>
    </div>
    <div class="stream-settings-panel" id="stream-settings-panel">
      <div class="settings-panel-header">
        <span>Stream Settings</span>
        <button class="settings-close-btn" id="settings-close-btn">✕</button>
      </div>
      <div class="settings-panel-content">
        <div class="settings-section">
          <h4>Stream Info</h4>
          <div class="settings-info-grid">
            <div class="info-item">
              <span class="info-label">Resolution</span>
              <span class="info-value" id="info-resolution">--</span>
            </div>
            <div class="info-item">
              <span class="info-label">FPS</span>
              <span class="info-value" id="info-fps">--</span>
            </div>
            <div class="info-item">
              <span class="info-label">Codec</span>
              <span class="info-value" id="info-codec">--</span>
            </div>
            <div class="info-item">
              <span class="info-label">Bitrate</span>
              <span class="info-value" id="info-bitrate">--</span>
            </div>
            <div class="info-item">
              <span class="info-label">Latency</span>
              <span class="info-value" id="info-latency">--</span>
            </div>
            <div class="info-item">
              <span class="info-label">Packet Loss</span>
              <span class="info-value" id="info-packet-loss">--</span>
            </div>
          </div>
        </div>
        <div class="settings-section">
          <h4>Display</h4>
          <div class="settings-option">
            <label>Show Stats Overlay</label>
            <input type="checkbox" id="setting-show-stats" checked>
          </div>
        </div>
      </div>
    </div>
  `;

  // Add styles
  const style = document.createElement("style");
  style.id = "streaming-container-style";
  style.textContent = `
    #streaming-container {
      position: fixed;
      top: 0;
      left: 0;
      right: 0;
      bottom: 0;
      background: #000;
      z-index: 10001;
      display: flex;
      align-items: center;
      justify-content: center;
    }
    .stream-video-wrapper {
      width: 100%;
      height: 100%;
      display: flex;
      align-items: center;
      justify-content: center;
    }
    #gfn-stream-video {
      width: 100%;
      height: 100%;
      object-fit: contain;
    }
    .stream-overlay {
      position: absolute;
      top: 0;
      left: 0;
      right: 0;
      padding: 10px 20px;
      background: linear-gradient(to bottom, rgba(0,0,0,0.7) 0%, transparent 100%);
      opacity: 0;
      transition: opacity 0.3s;
      pointer-events: none;
    }
    #streaming-container:hover .stream-overlay {
      opacity: 1;
      pointer-events: auto;
    }
    .stream-header {
      display: flex;
      justify-content: space-between;
      align-items: center;
    }
    .stream-game-name {
      font-size: 18px;
      font-weight: bold;
      color: #76b900;
    }
    .stream-controls {
      display: flex;
      gap: 8px;
    }
    .stream-btn {
      background: rgba(255,255,255,0.1);
      border: none;
      color: white;
      padding: 8px 12px;
      border-radius: 4px;
      cursor: pointer;
      font-size: 16px;
      transition: background 0.2s;
    }
    .stream-btn:hover {
      background: rgba(255,255,255,0.2);
    }
    .stream-btn-danger:hover {
      background: rgba(255,0,0,0.5);
    }
    .stream-stats {
      position: absolute;
      bottom: 10px;
      left: 20px;
      display: flex;
      gap: 15px;
      font-size: 12px;
      color: #aaa;
      background: rgba(0,0,0,0.5);
      padding: 5px 10px;
      border-radius: 4px;
    }
    .stream-stats span {
      font-family: monospace;
    }
    .stream-settings-panel {
      position: absolute;
      top: 60px;
      right: 20px;
      width: 320px;
      background: rgba(20, 20, 20, 0.95);
      border: 1px solid rgba(255,255,255,0.1);
      border-radius: 8px;
      display: none;
      z-index: 10002;
      box-shadow: 0 4px 20px rgba(0,0,0,0.5);
    }
    .stream-settings-panel.visible {
      display: block;
    }
    .settings-panel-header {
      display: flex;
      justify-content: space-between;
      align-items: center;
      padding: 12px 16px;
      border-bottom: 1px solid rgba(255,255,255,0.1);
      color: #76b900;
      font-weight: bold;
    }
    .settings-close-btn {
      background: none;
      border: none;
      color: #aaa;
      font-size: 16px;
      cursor: pointer;
      padding: 4px 8px;
    }
    .settings-close-btn:hover {
      color: #fff;
    }
    .settings-panel-content {
      padding: 16px;
      max-height: 400px;
      overflow-y: auto;
    }
    .settings-section {
      margin-bottom: 20px;
    }
    .settings-section:last-child {
      margin-bottom: 0;
    }
    .settings-section h4 {
      margin: 0 0 12px 0;
      color: #fff;
      font-size: 13px;
      text-transform: uppercase;
      letter-spacing: 0.5px;
    }
    .settings-info-grid {
      display: grid;
      grid-template-columns: 1fr 1fr;
      gap: 10px;
    }
    .info-item {
      display: flex;
      flex-direction: column;
      gap: 4px;
    }
    .info-label {
      font-size: 11px;
      color: #888;
      text-transform: uppercase;
    }
    .info-value {
      font-size: 14px;
      color: #fff;
      font-family: monospace;
    }
    .settings-option {
      display: flex;
      justify-content: space-between;
      align-items: center;
      padding: 8px 0;
    }
    .settings-option label {
      color: #ddd;
      font-size: 13px;
    }
    .settings-option input[type="checkbox"] {
      width: 18px;
      height: 18px;
      accent-color: #76b900;
    }
  `;

  document.head.appendChild(style);
  document.body.appendChild(container);

  // Find the video wrapper to return
  const videoWrapper = container.querySelector(".stream-video-wrapper") as HTMLElement;

  // Set up button handlers
  document.getElementById("stream-exit-btn")?.addEventListener("click", () => {
    exitStreaming();
  });

  document.getElementById("stream-fullscreen-btn")?.addEventListener("click", () => {
    if (document.fullscreenElement) {
      document.exitFullscreen();
    } else {
      container.requestFullscreen();
    }
  });

  // Settings panel toggle
  const settingsPanel = document.getElementById("stream-settings-panel");
  const settingsBtn = document.getElementById("stream-settings-btn");
  const closeSettingsBtn = document.getElementById("settings-close-btn");
  const showStatsCheckbox = document.getElementById("setting-show-stats") as HTMLInputElement;
  const statsOverlay = document.getElementById("stream-stats");

  settingsBtn?.addEventListener("click", () => {
    settingsPanel?.classList.toggle("visible");
  });

  closeSettingsBtn?.addEventListener("click", () => {
    settingsPanel?.classList.remove("visible");
  });

  // Toggle stats overlay visibility
  showStatsCheckbox?.addEventListener("change", () => {
    if (statsOverlay) {
      statsOverlay.style.display = showStatsCheckbox.checked ? "flex" : "none";
    }
  });

  // Hold ESC to exit fullscreen (1 second hold required)
  let escHoldStart = 0;
  let escHoldTimer: number | null = null;

  const escKeyDownHandler = (e: KeyboardEvent) => {
    if (e.key === "Escape" && document.fullscreenElement) {
      // Prevent browser's default behavior of exiting fullscreen on ESC
      e.preventDefault();

      // Only start the hold timer if not already started
      if (escHoldStart === 0) {
        escHoldStart = Date.now();
        escHoldTimer = window.setTimeout(() => {
          if (escHoldStart > 0) {
            document.exitFullscreen().catch(() => {});
            escHoldStart = 0;
          }
        }, 1000); // 1 second hold
      }
    }
  };

  const escKeyUpHandler = (e: KeyboardEvent) => {
    if (e.key === "Escape") {
      escHoldStart = 0;
      if (escHoldTimer) {
        clearTimeout(escHoldTimer);
        escHoldTimer = null;
      }
    }
  };

  document.addEventListener("keydown", escKeyDownHandler);
  document.addEventListener("keyup", escKeyUpHandler);

  // Store cleanup for ESC handlers
  streamingUIState.escCleanup = () => {
    document.removeEventListener("keydown", escKeyDownHandler);
    document.removeEventListener("keyup", escKeyUpHandler);
    if (escHoldTimer) {
      clearTimeout(escHoldTimer);
    }
  };

  return videoWrapper;
}

// Update streaming stats display
function updateStreamingStatsDisplay(stats: {
  fps: number;
  latency_ms: number;
  bitrate_kbps: number;
  packet_loss: number;
  resolution: string;
  codec: string;
}): void {
  // Update overlay stats
  const fpsEl = document.getElementById("stats-fps");
  const latencyEl = document.getElementById("stats-latency");
  const resEl = document.getElementById("stats-resolution");
  const codecEl = document.getElementById("stats-codec");
  const bitrateEl = document.getElementById("stats-bitrate");

  const bitrateFormatted = stats.bitrate_kbps >= 1000
    ? `${(stats.bitrate_kbps / 1000).toFixed(1)} Mbps`
    : `${stats.bitrate_kbps} kbps`;

  if (fpsEl) fpsEl.textContent = `${Math.round(stats.fps)} FPS`;
  if (latencyEl) latencyEl.textContent = `${stats.latency_ms} ms`;
  if (resEl) resEl.textContent = stats.resolution || "----x----";
  if (codecEl) codecEl.textContent = stats.codec || "----";
  if (bitrateEl) bitrateEl.textContent = bitrateFormatted;

  // Update settings panel info
  const infoResEl = document.getElementById("info-resolution");
  const infoFpsEl = document.getElementById("info-fps");
  const infoCodecEl = document.getElementById("info-codec");
  const infoBitrateEl = document.getElementById("info-bitrate");
  const infoLatencyEl = document.getElementById("info-latency");
  const infoPacketLossEl = document.getElementById("info-packet-loss");

  if (infoResEl) infoResEl.textContent = stats.resolution || "--";
  if (infoFpsEl) infoFpsEl.textContent = `${Math.round(stats.fps)}`;
  if (infoCodecEl) infoCodecEl.textContent = stats.codec || "--";
  if (infoBitrateEl) infoBitrateEl.textContent = bitrateFormatted;
  if (infoLatencyEl) infoLatencyEl.textContent = `${stats.latency_ms} ms`;
  if (infoPacketLossEl) infoPacketLossEl.textContent = `${(stats.packet_loss * 100).toFixed(2)}%`;
}

// Exit streaming and cleanup
async function exitStreaming(): Promise<void> {
  console.log("Exiting streaming...");

  // Stop input capture
  if (streamingUIState.inputCleanup) {
    streamingUIState.inputCleanup();
    streamingUIState.inputCleanup = null;
  }

  // Stop ESC handlers
  if (streamingUIState.escCleanup) {
    streamingUIState.escCleanup();
    streamingUIState.escCleanup = null;
  }

  // Stop stats monitoring
  if (streamingUIState.statsInterval) {
    clearInterval(streamingUIState.statsInterval);
    streamingUIState.statsInterval = null;
  }

  // Stop WebRTC streaming
  stopStreaming();

  // Stop backend session
  if (streamingUIState.sessionId) {
    try {
      const accessToken = await invoke<string>("get_access_token");
      await invoke("stop_streaming_flow", {
        sessionId: streamingUIState.sessionId,
        accessToken: accessToken,
      });
    } catch (e) {
      console.warn("Error stopping session:", e);
    }
  }

  // Remove streaming container
  const container = document.getElementById("streaming-container");
  const style = document.getElementById("streaming-container-style");
  if (container) container.remove();
  if (style) style.remove();

  // Hide streaming overlay
  hideStreamingOverlay();

  // Reset state
  streamingUIState = {
    active: false,
    sessionId: null,
    gameName: null,
    phase: "idle",
    gpuType: null,
    serverIp: null,
    inputCleanup: null,
    statsInterval: null,
    escCleanup: null,
  };

  // Reset Discord presence (if enabled)
  if (discordRpcEnabled) {
    try {
      await invoke("set_browsing_presence");
    } catch (e) {
      // Ignore
    }
  }

  console.log("Streaming exited");
}

// Streaming overlay functions
function showStreamingOverlay(gameName: string, status: string) {
  // Remove existing overlay if any
  const existing = document.getElementById("streaming-overlay");
  if (existing) existing.remove();

  const overlay = document.createElement("div");
  overlay.id = "streaming-overlay";
  overlay.innerHTML = `
    <div class="streaming-overlay-content">
      <div class="streaming-spinner"></div>
      <h2 id="streaming-game-name">${gameName}</h2>
      <p id="streaming-status">${status}</p>
      <div id="streaming-info" style="display: none;">
        <div class="streaming-stat"><span>GPU:</span> <span id="streaming-gpu">-</span></div>
        <div class="streaming-stat"><span>Server:</span> <span id="streaming-server">-</span></div>
        <div class="streaming-stat"><span>Status:</span> <span id="streaming-phase">-</span></div>
      </div>
      <button id="streaming-cancel-btn" class="btn btn-secondary">Cancel</button>
    </div>
  `;

  // Add styles
  const style = document.createElement("style");
  style.id = "streaming-overlay-style";
  style.textContent = `
    #streaming-overlay {
      position: fixed;
      top: 0;
      left: 0;
      right: 0;
      bottom: 0;
      background: rgba(0, 0, 0, 0.9);
      display: flex;
      align-items: center;
      justify-content: center;
      z-index: 10000;
    }
    .streaming-overlay-content {
      text-align: center;
      color: white;
      max-width: 400px;
      padding: 40px;
    }
    .streaming-spinner {
      width: 60px;
      height: 60px;
      border: 4px solid rgba(118, 185, 0, 0.3);
      border-top-color: #76b900;
      border-radius: 50%;
      animation: spin 1s linear infinite;
      margin: 0 auto 20px;
    }
    @keyframes spin {
      to { transform: rotate(360deg); }
    }
    #streaming-game-name {
      font-size: 24px;
      margin-bottom: 10px;
      color: #76b900;
    }
    #streaming-status {
      font-size: 16px;
      color: #aaa;
      margin-bottom: 20px;
    }
    #streaming-info {
      background: rgba(255, 255, 255, 0.1);
      border-radius: 8px;
      padding: 15px;
      margin-bottom: 20px;
      text-align: left;
    }
    .streaming-stat {
      display: flex;
      justify-content: space-between;
      padding: 5px 0;
      border-bottom: 1px solid rgba(255, 255, 255, 0.1);
    }
    .streaming-stat:last-child {
      border-bottom: none;
    }
    #streaming-cancel-btn {
      margin-top: 20px;
    }
  `;

  document.head.appendChild(style);
  document.body.appendChild(overlay);

  // Add cancel handler
  document.getElementById("streaming-cancel-btn")?.addEventListener("click", cancelStreaming);
}

function updateStreamingStatus(status: string) {
  const statusEl = document.getElementById("streaming-status");
  if (statusEl) {
    statusEl.textContent = status;
  }
}

function showStreamingInfo(info: {
  gpu_type: string | null;
  server_ip: string | null;
  phase: string;
}) {
  const infoEl = document.getElementById("streaming-info");
  const gpuEl = document.getElementById("streaming-gpu");
  const serverEl = document.getElementById("streaming-server");
  const phaseEl = document.getElementById("streaming-phase");

  if (infoEl) infoEl.style.display = "block";
  if (gpuEl) gpuEl.textContent = info.gpu_type || "Unknown";
  if (serverEl) serverEl.textContent = info.server_ip || "Unknown";
  if (phaseEl) phaseEl.textContent = info.phase;

  // Hide spinner when ready
  const spinner = document.querySelector(".streaming-spinner") as HTMLElement;
  if (spinner && info.phase === "Ready") {
    spinner.style.borderTopColor = "#76b900";
    spinner.style.animation = "none";
    spinner.innerHTML = "✓";
    spinner.style.display = "flex";
    spinner.style.alignItems = "center";
    spinner.style.justifyContent = "center";
    spinner.style.fontSize = "30px";
    spinner.style.color = "#76b900";
  }
}

function hideStreamingOverlay() {
  const overlay = document.getElementById("streaming-overlay");
  const style = document.getElementById("streaming-overlay-style");
  if (overlay) overlay.remove();
  if (style) style.remove();
}

async function cancelStreaming() {
  console.log("Cancelling streaming...");

  try {
    // Cancel polling if active
    await invoke("cancel_polling");
  } catch (e) {
    console.warn("Error cancelling polling:", e);
  }

  // Use the full exit streaming function to clean up everything
  await exitStreaming();
}

// Settings
async function saveSettings() {
  const qualityEl = document.getElementById("quality-setting") as HTMLSelectElement;
  const codecEl = document.getElementById("codec-setting") as HTMLSelectElement;
  const bitrateEl = document.getElementById("bitrate-setting") as HTMLInputElement;
  const regionEl = document.getElementById("region-setting") as HTMLSelectElement;
  const proxyEl = document.getElementById("proxy-setting") as HTMLInputElement;
  const telemetryEl = document.getElementById("telemetry-setting") as HTMLInputElement;
  const discordEl = document.getElementById("discord-setting") as HTMLInputElement;

  // Update global state
  discordRpcEnabled = discordEl?.checked || false;
  currentQuality = qualityEl?.value || "auto";
  currentCodec = codecEl?.value || "h264";
  currentMaxBitrate = parseInt(bitrateEl?.value || "200", 10);

  const settings: Settings = {
    quality: qualityEl?.value || "auto",
    codec: codecEl?.value || "h264",
    max_bitrate_mbps: currentMaxBitrate,
    region: regionEl?.value || undefined,
    discord_rpc: discordRpcEnabled,
    proxy: proxyEl?.value || undefined,
    disable_telemetry: telemetryEl?.checked || true,
  };

  try {
    await invoke("save_settings", { settings });
    hideAllModals();
    console.log("Settings saved");
  } catch (error) {
    console.error("Failed to save settings:", error);
  }
}

// Placeholder Data
function createPlaceholderGames(): Game[] {
  const titles = [
    "Cyberpunk 2077",
    "The Witcher 3",
    "Fortnite",
    "Apex Legends",
    "League of Legends",
    "Valorant",
    "Destiny 2",
    "Warframe",
    "Path of Exile",
    "Lost Ark",
    "Counter-Strike 2",
    "Dota 2",
    "Rocket League",
    "Fall Guys",
    "Among Us",
    "Minecraft",
    "Roblox",
    "GTA V",
  ];

  // Generate placeholder images using data URLs for reliability
  const generatePlaceholder = (title: string, index: number): string => {
    // Create a simple colored placeholder using SVG data URL
    const colors = ["#76b900", "#8dd100", "#5a9400", "#4a7d00", "#3d6600"];
    const color = colors[index % colors.length];
    const shortTitle = title.substring(0, 12);
    const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="180" height="240" viewBox="0 0 180 240">
      <rect fill="#252525" width="180" height="240"/>
      <rect fill="${color}" x="20" y="80" width="140" height="80" rx="8"/>
      <text x="90" y="128" font-family="Arial,sans-serif" font-size="14" fill="white" text-anchor="middle">${shortTitle}</text>
    </svg>`;
    return `data:image/svg+xml,${encodeURIComponent(svg)}`;
  };

  return titles.map((title, i) => ({
    id: `game-${i}`,
    title,
    publisher: "Publisher",
    images: {
      box_art: generatePlaceholder(title, i),
      thumbnail: generatePlaceholder(title, i),
    },
    store: {
      store_type: i % 3 === 0 ? "Steam" : i % 3 === 1 ? "Epic" : "Free",
      store_id: `${i}`,
    },
  }));
}

// Export for window access
(window as any).gfnClient = {
  switchView,
  searchGames,
  showGameDetail,
  // Streaming controls
  exitStreaming,
  // Input debugging
  forceInputHandshake,
  isInputReady,
  getInputDebugInfo,
  setInputCaptureMode,
  // Get streaming state
  getStreamingState: () => streamingUIState,
};
