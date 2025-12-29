// GFN Custom Client - Main Entry Point
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { getVersion } from "@tauri-apps/api/app";
import { fetch as tauriFetch } from "@tauri-apps/plugin-http";
import {
  initializeStreaming,
  setupInputCapture,
  setInputCaptureMode,
  suspendCursorCapture,
  resumeCursorCapture,
  stopStreaming,
  getStreamingStats,
  isStreamingActive,
  forceInputHandshake,
  isInputReady,
  getInputDebugInfo,
  StreamingOptions,
} from "./streaming";
import { initLogging, exportLogs, clearLogs } from "./logging";

// ============================================
// Custom Dropdown Component
// ============================================

interface DropdownChangeCallback {
  (value: string, text: string): void;
}

const dropdownCallbacks: Map<string, DropdownChangeCallback[]> = new Map();

function initializeDropdowns() {
  const dropdowns = document.querySelectorAll('.custom-dropdown');

  dropdowns.forEach(dropdown => {
    const trigger = dropdown.querySelector('.dropdown-trigger') as HTMLElement;
    const menu = dropdown.querySelector('.dropdown-menu') as HTMLElement;
    const options = dropdown.querySelectorAll('.dropdown-option');

    if (!trigger || !menu) return;

    // Toggle dropdown on click
    trigger.addEventListener('click', (e) => {
      e.stopPropagation();
      const isOpen = dropdown.classList.contains('open');

      // Close all other dropdowns
      document.querySelectorAll('.custom-dropdown.open').forEach(d => {
        if (d !== dropdown) d.classList.remove('open');
      });

      dropdown.classList.toggle('open', !isOpen);
    });

    // Keyboard navigation
    trigger.addEventListener('keydown', (e) => {
      if (e.key === 'Enter' || e.key === ' ') {
        e.preventDefault();
        trigger.click();
      } else if (e.key === 'Escape') {
        dropdown.classList.remove('open');
      } else if (e.key === 'ArrowDown' && dropdown.classList.contains('open')) {
        e.preventDefault();
        const selected = menu.querySelector('.dropdown-option.selected') as HTMLElement;
        const next = selected?.nextElementSibling as HTMLElement || menu.querySelector('.dropdown-option') as HTMLElement;
        next?.click();
      } else if (e.key === 'ArrowUp' && dropdown.classList.contains('open')) {
        e.preventDefault();
        const selected = menu.querySelector('.dropdown-option.selected') as HTMLElement;
        const prev = selected?.previousElementSibling as HTMLElement || menu.querySelector('.dropdown-option:last-child') as HTMLElement;
        prev?.click();
      }
    });

    // Option selection
    options.forEach(option => {
      option.addEventListener('click', (e) => {
        e.stopPropagation();
        const value = (option as HTMLElement).dataset.value || '';
        const text = option.textContent || '';

        // Update selected state
        options.forEach(o => o.classList.remove('selected'));
        option.classList.add('selected');

        // Update trigger text
        const triggerText = trigger.querySelector('.dropdown-text');
        if (triggerText) triggerText.textContent = text;

        // Close dropdown
        dropdown.classList.remove('open');

        // Fire change callbacks
        const dropdownId = (dropdown as HTMLElement).dataset.dropdown;
        if (dropdownId) {
          const callbacks = dropdownCallbacks.get(dropdownId) || [];
          callbacks.forEach(cb => cb(value, text));
        }
      });
    });
  });

  // Close dropdowns when clicking outside
  document.addEventListener('click', () => {
    document.querySelectorAll('.custom-dropdown.open').forEach(d => {
      d.classList.remove('open');
    });
  });
}

// Get dropdown value
function getDropdownValue(id: string): string {
  const dropdown = document.querySelector(`[data-dropdown="${id}"]`);
  if (!dropdown) return '';
  const selected = dropdown.querySelector('.dropdown-option.selected') as HTMLElement;
  return selected?.dataset.value || '';
}

// Set dropdown value
function setDropdownValue(id: string, value: string): void {
  const dropdown = document.querySelector(`[data-dropdown="${id}"]`);
  if (!dropdown) return;

  const options = dropdown.querySelectorAll('.dropdown-option');
  const trigger = dropdown.querySelector('.dropdown-trigger');
  const triggerText = trigger?.querySelector('.dropdown-text');

  options.forEach(option => {
    const optionEl = option as HTMLElement;
    if (optionEl.dataset.value === value) {
      options.forEach(o => o.classList.remove('selected'));
      optionEl.classList.add('selected');
      if (triggerText) triggerText.textContent = optionEl.textContent || '';
    }
  });
}

// Add change listener to dropdown
function onDropdownChange(id: string, callback: DropdownChangeCallback): void {
  if (!dropdownCallbacks.has(id)) {
    dropdownCallbacks.set(id, []);
  }
  dropdownCallbacks.get(id)!.push(callback);
}

// Set dropdown options dynamically
function setDropdownOptions(id: string, options: { value: string; text: string; selected?: boolean; className?: string }[]): void {
  const dropdown = document.querySelector(`[data-dropdown="${id}"]`);
  if (!dropdown) {
    console.warn(`Dropdown not found: ${id}`);
    return;
  }

  const menu = dropdown.querySelector('.dropdown-menu');
  const trigger = dropdown.querySelector('.dropdown-trigger');
  const triggerText = trigger?.querySelector('.dropdown-text');

  if (!menu) {
    console.warn(`Dropdown menu not found for: ${id}`);
    return;
  }

  // Clear existing options
  menu.innerHTML = '';

  // Add new options
  options.forEach(opt => {
    const optionEl = document.createElement('div');
    let className = 'dropdown-option';
    if (opt.selected) className += ' selected';
    if (opt.className) className += ' ' + opt.className;
    optionEl.className = className;
    optionEl.dataset.value = opt.value;
    optionEl.textContent = opt.text;

    // Store the option data for the click handler
    const optValue = opt.value;
    const optText = opt.text;
    const optClassName = opt.className;

    // Add click handler
    optionEl.addEventListener('click', (e) => {
      e.stopPropagation();

      // Update selected state (preserve custom classes)
      menu.querySelectorAll('.dropdown-option').forEach(o => o.classList.remove('selected'));
      optionEl.classList.add('selected');

      // Update trigger text and color
      if (triggerText) {
        triggerText.textContent = optText;
        // Apply color class to trigger if option has one
        const triggerEl = dropdown.querySelector('.dropdown-trigger');
        if (triggerEl) {
          triggerEl.classList.remove('latency-excellent', 'latency-good', 'latency-fair', 'latency-poor', 'latency-bad');
          if (optClassName) triggerEl.classList.add(optClassName);
        }
      }

      // Close dropdown
      dropdown.classList.remove('open');

      // Fire change callbacks
      const callbacks = dropdownCallbacks.get(id) || [];
      callbacks.forEach(cb => cb(optValue, optText));
    });

    menu.appendChild(optionEl);

    // Update trigger text if this is the selected option
    if (opt.selected && triggerText) {
      triggerText.textContent = opt.text;
      // Apply color class to trigger
      const triggerEl = dropdown.querySelector('.dropdown-trigger');
      if (triggerEl && opt.className) {
        triggerEl.classList.add(opt.className);
      }
    }
  });

  console.log(`Set ${options.length} options for dropdown: ${id}`);
}

// ============================================
// Types
// ============================================

// Types
interface GameVariant {
  id: string;
  store_type: string;
  supported_controls?: string[];
}

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
  variants?: GameVariant[];
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
  provider?: LoginProvider;
}

// Login provider for multi-region support (camelCase to match Rust serde)
interface LoginProvider {
  idpId: string;
  loginProviderCode: string;
  loginProviderDisplayName: string;
  loginProvider: string;
  streamingServiceUrl: string;
  loginProviderPriority: number;
}

interface ResolutionOption {
  heightInPixels: number;
  widthInPixels: number;
  framesPerSecond: number;
  isEntitled: boolean;
}

interface FeatureOption {
  key?: string;
  textValue?: string;
  setValue?: string[];
  booleanValue?: boolean;
}

interface SubscriptionFeatures {
  resolutions: ResolutionOption[];
  features: FeatureOption[];
}

interface BitrateConfig {
  bitrateOption: boolean;
  bitrateValue: number;
  minBitrateValue: number;
  maxBitrateValue: number;
}

interface ResolutionConfig {
  heightInPixels: number;
  widthInPixels: number;
  framesPerSecond: number;
}

interface StreamingQualityProfile {
  clientStreamingQualityMode?: string;
  maxBitRate?: BitrateConfig;
  resolution?: ResolutionConfig;
  features?: FeatureOption[];
}

interface AddonAttribute {
  key?: string;
  textValue?: string;
}

interface SubscriptionAddon {
  uri?: string;
  id?: string;
  type?: string;
  subType?: string;
  autoPayEnabled?: boolean;
  attributes?: AddonAttribute[];
  status?: string;
}

interface SubscriptionInfo {
  membershipTier: string;
  remainingTimeInMinutes?: number;
  totalTimeInMinutes?: number;
  renewalDateTime?: string;
  type?: string;
  subType?: string;
  features?: SubscriptionFeatures;
  streamingQualities?: StreamingQualityProfile[];
  addons?: SubscriptionAddon[];
}

interface Settings {
  quality: string;
  resolution?: string;
  fps?: number;
  codec: string;
  audio_codec?: string;
  max_bitrate_mbps: number;
  region?: string;
  discord_rpc: boolean;
  discord_show_stats?: boolean;
  proxy?: string;
  disable_telemetry: boolean;
  reflex?: boolean; // NVIDIA Reflex low-latency mode
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

// PrintedWaste API types for queue times
interface PrintedWasteServerMapping {
  title: string;
  region: string;
  is4080Server: boolean;
  is5080Server: boolean;
  nuked: boolean;
}

interface PrintedWasteQueueData {
  QueuePosition: number;
  "Last Updated": number;
  Region: string;
  eta?: number; // ETA in milliseconds
}

interface PrintedWasteQueueResponse {
  status: boolean;
  error: boolean;
  data: { [serverId: string]: PrintedWasteQueueData };
}

interface PrintedWasteMappingResponse {
  status: boolean;
  error: boolean;
  data: { [serverId: string]: PrintedWasteServerMapping };
}

// Combined server info for queue selection
interface QueueServerInfo {
  serverId: string;
  displayName: string;  // e.g., "Oregon"
  region: string;       // e.g., "US Northwest"
  ping_ms?: number;
  queuePosition: number;
  etaSeconds?: number;
  is4080Server: boolean;
  is5080Server: boolean;
}

interface ActiveSession {
  sessionId: string;
  appId: number;
  gpuType: string | null;
  status: number;
  serverIp: string | null;
  signalingUrl: string | null;
  resolution: string | null;
  fps: number | null;
}

// Active session state
let detectedActiveSessions: ActiveSession[] = [];
let pendingGameLaunch: Game | null = null;
let sessionPollingInterval: number | null = null;
const SESSION_POLLING_INTERVAL_MS = 10000; // Check every 10 seconds

// State
let currentView = "home";
let isAuthenticated = false;
let currentUser: AuthState["user"] | null = null;
let currentSubscription: SubscriptionInfo | null = null;
let games: Game[] = [];
let discordRpcEnabled = true; // Discord presence toggle (enabled by default)
let discordShowStats = false; // Show resolution/fps/ms in Discord (default off)
let currentQuality = "auto"; // Current quality preset (legacy/fallback)
let currentResolution = "1920x1080"; // Current resolution (WxH format)
let currentFps = 60; // Current FPS
let currentCodec = "h264"; // Current video codec
let currentAudioCodec = "opus"; // Current audio codec
let currentMaxBitrate = 200; // Max bitrate in Mbps (200 = unlimited)
let availableResolutions: string[] = []; // Available resolutions from subscription
let availableFpsOptions: number[] = []; // Available FPS options from subscription
let currentRegion = "auto"; // Preferred region (auto = lowest ping)
let cachedServers: Server[] = []; // Cached server latency data
let isTestingLatency = false; // Flag to prevent concurrent latency tests
let reflexEnabled = true; // NVIDIA Reflex low-latency mode (auto-enabled for 120+ FPS)

// PrintedWaste queue data cache
let cachedQueueData: PrintedWasteQueueResponse | null = null;
let cachedServerMapping: PrintedWasteMappingResponse | null = null;
let lastQueueFetch = 0;
const QUEUE_CACHE_TTL_MS = 30000; // 30 second cache for queue data
let selectedQueueServer: string | null = null; // User's selected server for queue
let queueCountdownInterval: number | null = null; // Countdown timer interval
let queueStartEta: number = 0; // Initial ETA when queue started (in seconds)
let queueStartTime: number = 0; // When the queue started (timestamp)

// Helper to get streaming params - uses direct resolution and fps values
function getStreamingParams(): { resolution: string; fps: number } {
  return { resolution: currentResolution, fps: currentFps };
}

// Check if user is on free tier
function isFreeTier(subscription: SubscriptionInfo | null): boolean {
  if (!subscription) return true; // Assume free if no subscription data
  const tier = subscription.membershipTier?.toUpperCase() || "FREE";
  return tier === "FREE" || tier === "FOUNDER"; // FOUNDER is also a free tier variant
}

// Check if using an Alliance Partner (non-NVIDIA provider)
// PrintedWaste queue data is only available for NVIDIA global servers
function isAlliancePartner(): boolean {
  if (!selectedLoginProvider) return false;
  return selectedLoginProvider.loginProviderCode !== "NVIDIA";
}

// ============================================
// PrintedWaste Queue API Integration
// ============================================

// Get the current app version for user agent
async function getAppVersion(): Promise<string> {
  try {
    return await getVersion();
  } catch {
    return "0.0.12"; // Fallback version
  }
}

// Fetch server mapping from PrintedWaste
async function fetchServerMapping(): Promise<PrintedWasteMappingResponse | null> {
  if (cachedServerMapping) {
    return cachedServerMapping;
  }

  try {
    const version = await getAppVersion();
    const response = await tauriFetch("https://remote.printedwaste.com/config/GFN_SERVERID_TO_REGION_MAPPING", {
      headers: {
        "User-Agent": `OpenNOW/${version}`
      }
    });

    if (!response.ok) {
      console.error("Failed to fetch server mapping:", response.status);
      return null;
    }

    cachedServerMapping = await response.json();
    return cachedServerMapping;
  } catch (error) {
    console.error("Error fetching server mapping:", error);
    return null;
  }
}

// Fetch queue data from PrintedWaste
async function fetchQueueData(): Promise<PrintedWasteQueueResponse | null> {
  const now = Date.now();

  // Return cached data if still valid
  if (cachedQueueData && (now - lastQueueFetch) < QUEUE_CACHE_TTL_MS) {
    return cachedQueueData;
  }

  try {
    const version = await getAppVersion();
    const response = await tauriFetch("https://api.printedwaste.com/gfn/queue/", {
      headers: {
        "User-Agent": `OpenNOW/${version}`
      }
    });

    if (!response.ok) {
      console.error("Failed to fetch queue data:", response.status);
      return cachedQueueData; // Return stale cache on error
    }

    cachedQueueData = await response.json();
    lastQueueFetch = now;
    return cachedQueueData;
  } catch (error) {
    console.error("Error fetching queue data:", error);
    return cachedQueueData; // Return stale cache on error
  }
}

// Get combined queue server info with ping data
async function getQueueServersInfo(): Promise<QueueServerInfo[]> {
  // Skip PrintedWaste queue for Alliance Partners - they have their own infrastructure
  if (isAlliancePartner()) {
    console.log("Skipping PrintedWaste queue - using Alliance Partner servers");
    return [];
  }

  const [mapping, queueData] = await Promise.all([
    fetchServerMapping(),
    fetchQueueData()
  ]);

  if (!mapping || !queueData) {
    console.error("Failed to fetch queue server info");
    return [];
  }

  const servers: QueueServerInfo[] = [];

  for (const [serverId, serverData] of Object.entries(mapping.data)) {
    // Skip nuked servers
    if (serverData.nuked) continue;

    // Only include RTX 4080 or 5080 servers
    if (!serverData.is4080Server && !serverData.is5080Server) continue;

    const queueInfo = queueData.data[serverId];
    if (!queueInfo) continue; // Skip if no queue data

    // Find ping from cached servers (match by region/name)
    const cachedServer = cachedServers.find(s =>
      s.name.toLowerCase().includes(serverData.title.toLowerCase()) ||
      serverData.title.toLowerCase().includes(s.name.toLowerCase()) ||
      s.id === serverId
    );

    servers.push({
      serverId,
      displayName: serverData.title,
      region: serverData.region,
      ping_ms: cachedServer?.ping_ms,
      queuePosition: queueInfo.QueuePosition,
      etaSeconds: queueInfo.eta ? Math.floor(queueInfo.eta / 1000) : undefined, // Convert ms to seconds
      is4080Server: serverData.is4080Server,
      is5080Server: serverData.is5080Server
    });
  }

  // Sort by ping (best first), undefined pings go to end
  servers.sort((a, b) => {
    if (a.ping_ms === undefined && b.ping_ms === undefined) return 0;
    if (a.ping_ms === undefined) return 1;
    if (b.ping_ms === undefined) return -1;
    return a.ping_ms - b.ping_ms;
  });

  return servers;
}

// Format ETA in a human-readable format
function formatQueueEta(etaSeconds: number | undefined): string {
  if (!etaSeconds || etaSeconds <= 0) return "Unknown";

  if (etaSeconds < 60) {
    return `${etaSeconds}s`;
  } else if (etaSeconds < 3600) {
    const minutes = Math.floor(etaSeconds / 60);
    return `${minutes}m`;
  } else if (etaSeconds < 86400) {
    const hours = Math.floor(etaSeconds / 3600);
    const minutes = Math.floor((etaSeconds % 3600) / 60);
    return minutes > 0 ? `${hours}h ${minutes}m` : `${hours}h`;
  } else {
    const days = Math.floor(etaSeconds / 86400);
    const hours = Math.floor((etaSeconds % 86400) / 3600);
    return hours > 0 ? `${days}d ${hours}h` : `${days}d`;
  }
}

// Calculate auto-selected server based on ping and queue time
function getAutoSelectedServer(servers: QueueServerInfo[]): QueueServerInfo | null {
  if (servers.length === 0) return null;

  // Score each server: lower is better
  // We balance ping (important for gameplay) with queue time
  // Ping weight: 1.0, Queue ETA weight: 0.1 (per minute)
  const scored = servers.map(server => {
    const pingScore = server.ping_ms ?? 500; // High penalty for unknown ping
    const etaMinutes = (server.etaSeconds ?? 0) / 60;
    // Cap ETA penalty to prevent extremely long queues from dominating
    const etaScore = Math.min(etaMinutes * 0.1, 50);
    return {
      server,
      score: pingScore + etaScore
    };
  });

  scored.sort((a, b) => a.score - b.score);
  return scored[0]?.server ?? null;
}

// Show the queue server selection modal (for free tier users)
async function showQueueSelectionModal(game: Game): Promise<string | null> {
  return new Promise(async (resolve) => {
    // Fetch queue data
    const servers = await getQueueServersInfo();

    if (servers.length === 0) {
      // No queue data available, proceed with normal flow
      resolve(null);
      return;
    }

    const autoServer = getAutoSelectedServer(servers);

    // Remove existing modal if any
    const existing = document.getElementById("queue-selection-modal");
    if (existing) existing.remove();

    const modal = document.createElement("div");
    modal.id = "queue-selection-modal";
    modal.className = "modal";
    modal.innerHTML = `
      <div class="modal-content queue-modal-content">
        <button class="modal-close">&times;</button>
        <h2>Select Server</h2>
        <p class="queue-modal-subtitle">Choose a server to queue on based on your ping and current wait times.</p>

        <div class="queue-server-list" id="queue-server-list">
          <!-- Auto option -->
          <div class="queue-server-item selected" data-server-id="auto" data-eta="${autoServer?.etaSeconds || 0}">
            <div class="queue-server-info">
              <span class="queue-server-name">Auto (Recommended)</span>
              <span class="queue-server-detail">${autoServer ? `${autoServer.displayName} - Best balance of ping & wait` : 'Best available'}</span>
            </div>
            <div class="queue-server-stats">
              <span class="queue-ping ${autoServer ? getLatencyClassName(autoServer.ping_ms) : ''}">${autoServer?.ping_ms ? `${autoServer.ping_ms}ms` : '--'}</span>
              <span class="queue-wait">~${formatQueueEta(autoServer?.etaSeconds)}</span>
            </div>
          </div>

          ${servers.map(server => `
            <div class="queue-server-item" data-server-id="${server.serverId}" data-eta="${server.etaSeconds || 0}">
              <div class="queue-server-info">
                <span class="queue-server-name">${server.displayName}</span>
                <span class="queue-server-detail">${server.is5080Server ? 'RTX 5080' : 'RTX 4080'}</span>
              </div>
              <div class="queue-server-stats">
                <span class="queue-ping ${getLatencyClassName(server.ping_ms)}">${server.ping_ms ? `${server.ping_ms}ms` : '--'}</span>
                <span class="queue-wait">~${formatQueueEta(server.etaSeconds)}</span>
              </div>
            </div>
          `).join('')}
        </div>

        <div class="queue-modal-actions">
          <button id="queue-start-btn" class="btn btn-primary btn-large">Start Queue</button>
          <button id="queue-cancel-btn" class="btn btn-secondary">Cancel</button>
        </div>

        <div class="queue-attribution">
          <span>Powered by <a href="https://printedwaste.com/gfn/" target="_blank" rel="noopener noreferrer">PrintedWaste</a></span>
        </div>
      </div>
    `;

    // Add modal styles
    const style = document.createElement("style");
    style.id = "queue-modal-style";
    style.textContent = `
      .queue-modal-content {
        max-width: 500px;
        max-height: 80vh;
        overflow: hidden;
        display: flex;
        flex-direction: column;
      }
      .queue-modal-subtitle {
        color: var(--text-secondary);
        font-size: 14px;
        margin-bottom: 16px;
      }
      .queue-server-list {
        flex: 1;
        overflow-y: auto;
        max-height: 350px;
        margin-bottom: 16px;
        border: 1px solid var(--border-color);
        border-radius: var(--radius);
      }
      .queue-server-item {
        display: flex;
        justify-content: space-between;
        align-items: center;
        padding: 12px 16px;
        cursor: pointer;
        transition: background 0.15s;
        border-bottom: 1px solid var(--border-color);
      }
      .queue-server-item:last-child {
        border-bottom: none;
      }
      .queue-server-item:hover {
        background: var(--bg-hover);
      }
      .queue-server-item.selected {
        background: rgba(118, 185, 0, 0.15);
        border-left: 3px solid var(--accent-green);
      }
      .queue-server-info {
        display: flex;
        flex-direction: column;
        gap: 2px;
      }
      .queue-server-name {
        font-weight: 500;
        color: var(--text-primary);
      }
      .queue-server-detail {
        font-size: 12px;
        color: var(--text-muted);
      }
      .queue-server-stats {
        display: flex;
        gap: 16px;
        align-items: center;
      }
      .queue-ping {
        font-family: monospace;
        font-size: 14px;
        min-width: 50px;
        text-align: right;
      }
      .queue-wait {
        font-size: 13px;
        color: var(--text-secondary);
        min-width: 70px;
        text-align: right;
      }
      .queue-modal-actions {
        display: flex;
        gap: 12px;
        margin-bottom: 12px;
      }
      .queue-modal-actions .btn {
        flex: 1;
      }
      .queue-attribution {
        text-align: center;
        font-size: 11px;
        color: var(--text-muted);
      }
      .queue-attribution a {
        color: var(--accent-green);
        text-decoration: none;
      }
      .queue-attribution a:hover {
        text-decoration: underline;
      }
    `;

    document.head.appendChild(style);
    document.body.appendChild(modal);

    // Initialize Lucide icons if available
    if (typeof lucide !== 'undefined') {
      lucide.createIcons();
    }

    let selectedServerId = "auto";
    let selectedEta = autoServer?.etaSeconds || 0;

    // Handle server selection
    const serverItems = modal.querySelectorAll('.queue-server-item');
    serverItems.forEach(item => {
      item.addEventListener('click', () => {
        serverItems.forEach(i => i.classList.remove('selected'));
        item.classList.add('selected');
        selectedServerId = (item as HTMLElement).dataset.serverId || "auto";
        selectedEta = parseInt((item as HTMLElement).dataset.eta || "0", 10);
      });
    });

    // Handle start button
    modal.querySelector('#queue-start-btn')?.addEventListener('click', () => {
      selectedQueueServer = selectedServerId === "auto" ? (autoServer?.serverId || null) : selectedServerId;
      queueStartEta = selectedEta;
      queueStartTime = Date.now();
      modal.remove();
      style.remove();
      resolve(selectedQueueServer);
    });

    // Handle cancel button
    modal.querySelector('#queue-cancel-btn')?.addEventListener('click', () => {
      modal.remove();
      style.remove();
      resolve(null);
    });

    // Handle close button
    modal.querySelector('.modal-close')?.addEventListener('click', () => {
      modal.remove();
      style.remove();
      resolve(null);
    });

    // Close on background click
    modal.addEventListener('click', (e) => {
      if (e.target === modal) {
        modal.remove();
        style.remove();
        resolve(null);
      }
    });
  });
}

// Start countdown timer for queue ETA
function startQueueCountdown() {
  if (queueCountdownInterval) {
    clearInterval(queueCountdownInterval);
  }

  const updateCountdown = () => {
    const queueEtaEl = document.getElementById("queue-eta");

    if (!queueEtaEl || queueStartEta <= 0) return;

    // Calculate remaining time
    const elapsed = Math.floor((Date.now() - queueStartTime) / 1000);
    const remaining = Math.max(0, queueStartEta - elapsed);

    queueEtaEl.textContent = formatQueueEta(remaining);
  };

  // Update immediately and then every second
  updateCountdown();
  queueCountdownInterval = window.setInterval(updateCountdown, 1000);
}

// Stop countdown timer
function stopQueueCountdown() {
  if (queueCountdownInterval) {
    clearInterval(queueCountdownInterval);
    queueCountdownInterval = null;
  }

  queueStartEta = 0;
  queueStartTime = 0;
}

// Show queue times page (can be accessed from UI)
async function showQueueTimesPage(): Promise<void> {
  // Queue times are only available for NVIDIA global servers
  if (isAlliancePartner()) {
    alert("Queue times are only available for NVIDIA global servers. Alliance Partner servers have their own queue system.");
    return;
  }

  const servers = await getQueueServersInfo();

  if (servers.length === 0) {
    alert("Unable to fetch queue times. Please try again later.");
    return;
  }

  // Remove existing modal if any
  const existing = document.getElementById("queue-times-modal");
  if (existing) existing.remove();

  const modal = document.createElement("div");
  modal.id = "queue-times-modal";
  modal.className = "modal";
  modal.innerHTML = `
    <div class="modal-content queue-times-content">
      <button class="modal-close">&times;</button>
      <h2>Queue Times</h2>
      <p class="queue-times-subtitle">Current wait times for RTX 4080/5080 servers, sorted by your ping.</p>

      <div class="queue-times-list" id="queue-times-list">
        ${servers.map(server => `
          <div class="queue-times-item">
            <div class="queue-times-info">
              <span class="queue-times-name">${server.displayName}</span>
              <span class="queue-times-gpu">${server.is5080Server ? 'RTX 5080' : 'RTX 4080'}</span>
            </div>
            <div class="queue-times-stats">
              <span class="queue-times-ping ${getLatencyClassName(server.ping_ms)}">${server.ping_ms ? `${server.ping_ms}ms` : '--'}</span>
              <span class="queue-times-wait">~${formatQueueEta(server.etaSeconds)}</span>
              <span class="queue-times-position">#${server.queuePosition}</span>
            </div>
          </div>
        `).join('')}
      </div>

      <div class="queue-attribution">
        <span>Powered by <a href="https://printedwaste.com/gfn/" target="_blank" rel="noopener noreferrer">PrintedWaste</a></span>
      </div>
    </div>
  `;

  // Add modal styles
  const style = document.createElement("style");
  style.id = "queue-times-modal-style";
  style.textContent = `
    .queue-times-content {
      max-width: 550px;
      max-height: 80vh;
      overflow: hidden;
      display: flex;
      flex-direction: column;
    }
    .queue-times-subtitle {
      color: var(--text-secondary);
      font-size: 14px;
      margin-bottom: 16px;
    }
    .queue-times-list {
      flex: 1;
      overflow-y: auto;
      max-height: 400px;
      margin-bottom: 16px;
      border: 1px solid var(--border-color);
      border-radius: var(--radius);
    }
    .queue-times-item {
      display: flex;
      justify-content: space-between;
      align-items: center;
      padding: 12px 16px;
      border-bottom: 1px solid var(--border-color);
    }
    .queue-times-item:last-child {
      border-bottom: none;
    }
    .queue-times-item:nth-child(odd) {
      background: var(--bg-tertiary);
    }
    .queue-times-info {
      display: flex;
      flex-direction: column;
      gap: 2px;
    }
    .queue-times-name {
      font-weight: 500;
      color: var(--text-primary);
    }
    .queue-times-gpu {
      font-size: 12px;
      color: var(--text-muted);
    }
    .queue-times-stats {
      display: flex;
      gap: 16px;
      align-items: center;
    }
    .queue-times-ping {
      font-family: monospace;
      font-size: 14px;
      min-width: 55px;
      text-align: right;
    }
    .queue-times-wait {
      font-size: 13px;
      color: var(--text-secondary);
      min-width: 70px;
      text-align: right;
    }
    .queue-times-position {
      font-size: 12px;
      color: var(--text-muted);
      min-width: 40px;
      text-align: right;
    }
    .queue-attribution {
      text-align: center;
      font-size: 11px;
      color: var(--text-muted);
    }
    .queue-attribution a {
      color: var(--accent-green);
      text-decoration: none;
    }
    .queue-attribution a:hover {
      text-decoration: underline;
    }
  `;

  document.head.appendChild(style);
  document.body.appendChild(modal);

  // Handle close button
  modal.querySelector('.modal-close')?.addEventListener('click', () => {
    modal.remove();
    style.remove();
  });

  // Close on background click
  modal.addEventListener('click', (e) => {
    if (e.target === modal) {
      modal.remove();
      style.remove();
    }
  });
}

// Check if resolution is above 1080p (considering any aspect ratio)
function isResolutionAbove1080p(resolution: string): boolean {
  const parts = resolution.split('x');
  if (parts.length !== 2) return false;
  const height = parseInt(parts[1], 10);
  // 1080p max height is 1080, anything above is considered premium
  // For ultrawide 1080p (2560x1080), height is still 1080 so it's allowed
  return height > 1080;
}

// Populate resolution and FPS dropdowns from subscription data
function populateStreamingOptions(subscription: SubscriptionInfo | null): void {
  // Default options if no subscription data
  const defaultResolutions = [
    { width: 1280, height: 720 },
    { width: 1920, height: 1080 },
    { width: 2560, height: 1440 },
    { width: 3840, height: 2160 },
  ];
  const defaultFps = [30, 60, 120, 240];

  // Check if user is on free tier
  const isFree = isFreeTier(subscription);
  console.log(`User tier: ${subscription?.membershipTier || "FREE"}, isFree: ${isFree}`);

  // Helper to get friendly resolution label
  const getResolutionLabel = (res: string, disabled: boolean): string => {
    const labels: { [key: string]: string } = {
      '1280x720': '1280x720 (720p)',
      '1920x1080': '1920x1080 (1080p)',
      '2560x1440': '2560x1440 (1440p)',
      '3840x2160': '3840x2160 (4K)',
      '5120x2880': '5120x2880 (5K)',
      '2560x1080': '2560x1080 (UW 1080p)',
      '3440x1440': '3440x1440 (UW 1440p)',
      '1920x800': '1920x800 (21:9)',
      '2560x1600': '2560x1600 (16:10)',
      '1680x1050': '1680x1050 (16:10)',
    };
    const label = labels[res] || res;
    return disabled ? `${label} (Priority/Ultimate)` : label;
  };

  if (subscription?.features?.resolutions && subscription.features.resolutions.length > 0) {
    // Extract unique resolutions and FPS from subscription (ignore isEntitled - show all options)
    const resolutionSet = new Set<string>();
    const fpsSet = new Set<number>();

    for (const res of subscription.features.resolutions) {
      // Show all resolutions/FPS regardless of entitlement
      resolutionSet.add(`${res.widthInPixels}x${res.heightInPixels}`);
      fpsSet.add(res.framesPerSecond);
    }

    // Convert to sorted arrays
    availableResolutions = Array.from(resolutionSet).sort((a, b) => {
      const [aW] = a.split('x').map(Number);
      const [bW] = b.split('x').map(Number);
      return aW - bW;
    });

    // Always include high FPS options even if not in API response (for paid tiers)
    if (!isFree) {
      fpsSet.add(120);
      fpsSet.add(240);
    }

    availableFpsOptions = Array.from(fpsSet).sort((a, b) => a - b);

    console.log(`Populated ${availableResolutions.length} resolutions and ${availableFpsOptions.length} FPS options from subscription`);
  } else {
    // Use defaults
    availableResolutions = defaultResolutions.map(r => `${r.width}x${r.height}`);
    availableFpsOptions = defaultFps;
    console.log("Using default resolution/FPS options (no subscription data)");
  }

  // For free tier, filter out premium options
  if (isFree) {
    // Filter resolutions: keep only those at or below 1080p height
    availableResolutions = availableResolutions.filter(res => !isResolutionAbove1080p(res));
    
    // Filter FPS: remove 240 and 360 fps options
    availableFpsOptions = availableFpsOptions.filter(fps => fps < 240);
    
    console.log(`Free tier: Filtered to ${availableResolutions.length} resolutions and ${availableFpsOptions.length} FPS options`);
  }

  // Build resolution options for custom dropdown
  const resolutionOptions = availableResolutions.map(res => ({
    value: res,
    text: getResolutionLabel(res, false),
    selected: res === currentResolution
  }));

  // If current resolution not in list, select highest available
  if (!resolutionOptions.some(o => o.selected) && resolutionOptions.length > 0) {
    // Select 1080p if available, otherwise the highest
    const preferred = resolutionOptions.find(o => o.value === "1920x1080") || resolutionOptions[resolutionOptions.length - 1];
    preferred.selected = true;
    currentResolution = preferred.value;
  }

  setDropdownOptions("resolution-setting", resolutionOptions);

  // Build FPS options for custom dropdown
  const fpsOptions = availableFpsOptions.map(fps => ({
    value: String(fps),
    text: `${fps} FPS`,
    selected: fps === currentFps
  }));

  // If current FPS not in list, select highest available
  if (!fpsOptions.some(o => o.selected) && fpsOptions.length > 0) {
    // Select 60 FPS if available, otherwise the highest
    const preferred = fpsOptions.find(o => o.value === "60") || fpsOptions[fpsOptions.length - 1];
    preferred.selected = true;
    currentFps = parseInt(preferred.value, 10);
  }

  setDropdownOptions("fps-setting", fpsOptions);
}

// Get latency class for color coding based on ping value
function getLatencyClass(pingMs: number | undefined): string {
  if (pingMs === undefined) return "latency-offline";
  if (pingMs < 20) return "latency-excellent";
  if (pingMs < 40) return "latency-good";
  if (pingMs < 80) return "latency-fair";
  if (pingMs < 120) return "latency-poor";
  return "latency-bad";
}

// Format latency display text
function formatLatency(pingMs: number | undefined, status: string): string {
  if (status !== "Online") return status.toLowerCase();
  if (pingMs === undefined) return "---";
  return `${pingMs}ms`;
}

// Number of latency test rounds to average (ICMP ping is accurate, fewer rounds needed)
const LATENCY_TEST_ROUNDS = 3;

// Test latency to all regions with multiple rounds for accuracy
async function testLatency(): Promise<Server[]> {
  if (isTestingLatency) {
    console.log("Latency test already in progress");
    return cachedServers;
  }

  isTestingLatency = true;
  console.log(`Starting latency test (${LATENCY_TEST_ROUNDS} rounds)...`);

  // Update UI to show testing state
  updateLatencyTestingUI(true, 0, LATENCY_TEST_ROUNDS);

  try {
    // Store results from all rounds: Map<serverId, pingValues[]>
    const allResults: Map<string, number[]> = new Map();
    let baseServers: Server[] = [];

    // Run multiple rounds
    for (let round = 0; round < LATENCY_TEST_ROUNDS; round++) {
      console.log(`Latency test round ${round + 1}/${LATENCY_TEST_ROUNDS}...`);
      updateLatencyTestingUI(true, round + 1, LATENCY_TEST_ROUNDS);

      const servers = await invoke<Server[]>("get_servers", { accessToken: null });

      if (round === 0) {
        baseServers = servers;
      }

      // Collect ping values
      for (const server of servers) {
        if (server.status === "Online" && server.ping_ms !== undefined) {
          if (!allResults.has(server.id)) {
            allResults.set(server.id, []);
          }
          allResults.get(server.id)!.push(server.ping_ms);
        }
      }

      // Small delay between rounds to avoid hammering servers
      if (round < LATENCY_TEST_ROUNDS - 1) {
        await new Promise(resolve => setTimeout(resolve, 500));
      }
    }

    // Calculate averaged results
    const averagedServers: Server[] = baseServers.map(server => {
      const pings = allResults.get(server.id);
      if (pings && pings.length > 0) {
        // Calculate average, excluding outliers (highest and lowest if we have enough samples)
        let avgPing: number;
        if (pings.length >= 3) {
          // Remove highest and lowest, then average the rest
          const sorted = [...pings].sort((a, b) => a - b);
          const trimmed = sorted.slice(1, -1);
          avgPing = Math.round(trimmed.reduce((a, b) => a + b, 0) / trimmed.length);
        } else {
          avgPing = Math.round(pings.reduce((a, b) => a + b, 0) / pings.length);
        }
        return { ...server, ping_ms: avgPing };
      }
      return server;
    });

    // Sort by ping
    averagedServers.sort((a, b) => {
      if (a.status === "Online" && b.status === "Online") {
        return (a.ping_ms || 9999) - (b.ping_ms || 9999);
      }
      if (a.status === "Online") return -1;
      if (b.status === "Online") return 1;
      return 0;
    });

    cachedServers = averagedServers;

    // Log summary
    console.log(`Latency test complete (${LATENCY_TEST_ROUNDS} rounds averaged):`);
    console.table(averagedServers.map(s => ({
      Region: s.name,
      "Avg Ping (ms)": s.ping_ms || "offline",
      Status: s.status,
      Samples: allResults.get(s.id)?.length || 0
    })));

    // Update the region dropdown with averaged latency data
    populateRegionDropdown(averagedServers);

    // Update status bar
    updateStatusBarLatency();

    return averagedServers;
  } catch (error) {
    console.error("Latency test failed:", error);
    return cachedServers;
  } finally {
    isTestingLatency = false;
    updateLatencyTestingUI(false, 0, LATENCY_TEST_ROUNDS);
  }
}

// Update UI to show latency testing in progress
function updateLatencyTestingUI(testing: boolean, currentRound: number = 0, totalRounds: number = 1): void {
  const pingInfo = document.getElementById("ping-info");
  if (pingInfo) {
    if (testing) {
      // Clear existing content
      while (pingInfo.firstChild) {
        pingInfo.removeChild(pingInfo.firstChild);
      }
      // Add spinner
      const spinner = document.createElement("span");
      spinner.className = "region-loading-spinner";
      pingInfo.appendChild(spinner);
      // Show progress
      const progressText = currentRound > 0
        ? ` Testing ${currentRound}/${totalRounds}...`
        : " Testing...";
      pingInfo.appendChild(document.createTextNode(progressText));
      pingInfo.className = "";
    }
  }
}

// Get latency class name for dropdown coloring
function getLatencyClassName(pingMs: number | undefined): string {
  if (pingMs === undefined) return '';
  if (pingMs < 20) return 'latency-excellent';
  if (pingMs < 40) return 'latency-good';
  if (pingMs < 80) return 'latency-fair';
  if (pingMs < 120) return 'latency-poor';
  return 'latency-bad';
}

// Populate region dropdown with latency data
function populateRegionDropdown(servers: Server[]): void {
  // Use the saved currentRegion (from settings) as the source of truth
  // Only fall back to dropdown value if currentRegion is not set
  // This ensures the saved region persists across app restarts
  const currentValue = currentRegion || getDropdownValue("region-setting") || "auto";

  // Build options array
  const options: { value: string; text: string; selected?: boolean; className?: string }[] = [];

  // Add Auto option first
  const bestServer = servers.find(s => s.status === "Online");
  const autoText = bestServer && bestServer.ping_ms
    ? `Auto (${bestServer.name} - ${bestServer.ping_ms}ms)`
    : "Auto (Lowest Ping)";
  options.push({
    value: "auto",
    text: autoText,
    selected: currentValue === "auto",
    className: bestServer ? getLatencyClassName(bestServer.ping_ms) : ''
  });

  // Group servers by region and add them
  const regions: { [key: string]: Server[] } = {};
  for (const server of servers) {
    if (!regions[server.region]) {
      regions[server.region] = [];
    }
    regions[server.region].push(server);
  }

  // Add servers grouped by region
  for (const [regionName, regionServers] of Object.entries(regions)) {
    for (const server of regionServers) {
      if (server.status !== "Online") continue; // Skip offline servers

      const latencyText = formatLatency(server.ping_ms, server.status);
      const text = server.ping_ms
        ? `${regionName} - ${server.name} (${latencyText})`
        : `${regionName} - ${server.name}`;

      options.push({
        value: server.id,
        text: text,
        selected: currentValue === server.id,
        className: getLatencyClassName(server.ping_ms)
      });
    }
  }

  // Update the dropdown
  setDropdownOptions("region-setting", options);

  // Ensure the saved region is selected in the dropdown
  // Don't overwrite currentRegion - it should only change when user explicitly selects a new region
  if (currentValue && currentValue !== "auto") {
    // Check if the saved region exists in the options
    const regionExists = options.some(o => o.value === currentValue);
    if (regionExists) {
      setDropdownValue("region-setting", currentValue);
    } else {
      // Region no longer exists (server removed), fall back to auto
      console.warn(`Saved region "${currentValue}" not found in server list, falling back to auto`);
      setDropdownValue("region-setting", "auto");
      currentRegion = "auto";
    }
  }
}

// Get CSS color for latency value
function getLatencyColor(pingMs: number | undefined): string {
  if (pingMs === undefined) return "#666666";
  if (pingMs < 20) return "#00c853";
  if (pingMs < 40) return "#76b900";
  if (pingMs < 80) return "#ffc107";
  if (pingMs < 120) return "#ff9800";
  return "#f44336";
}

// Update status bar with current region and ping
function updateStatusBarLatency(): void {
  const serverInfo = document.getElementById("server-info");
  const pingInfo = document.getElementById("ping-info");

  if (!serverInfo || !pingInfo) return;

  let displayServer: Server | undefined;

  if (currentRegion === "auto") {
    // Find best server (first online one, already sorted by ping)
    displayServer = cachedServers.find(s => s.status === "Online");
    serverInfo.textContent = displayServer ? `Server: Auto (${displayServer.name})` : "Server: Auto";
  } else {
    // Find selected server
    displayServer = cachedServers.find(s => s.id === currentRegion);
    serverInfo.textContent = displayServer ? `Server: ${displayServer.name}` : `Server: ${currentRegion}`;
  }

  if (displayServer && displayServer.ping_ms !== undefined) {
    pingInfo.textContent = `Ping: ${displayServer.ping_ms}ms`;
    pingInfo.className = getLatencyClass(displayServer.ping_ms);
  } else {
    pingInfo.textContent = "Ping: --ms";
    pingInfo.className = "";
  }
}

// Get the server ID to use for session launch
function getPreferredServerForSession(): string | undefined {
  // If a queue server was selected (free tier users), use that
  if (selectedQueueServer) {
    const server = selectedQueueServer;
    // Reset for next launch
    selectedQueueServer = null;
    return server;
  }

  if (currentRegion === "auto") {
    // Use the best (lowest ping) online server
    const bestServer = cachedServers.find(s => s.status === "Online");
    return bestServer?.id;
  }
  return currentRegion;
}

// DOM Elements
const loginBtn = document.getElementById("login-btn")!;
const userMenu = document.getElementById("user-menu")!;
const settingsBtn = document.getElementById("settings-btn")!;
const searchInput = document.getElementById("search-input") as HTMLInputElement;
const navItems = document.querySelectorAll(".nav-item");

// Declare Lucide global (loaded via CDN)
declare const lucide: { createIcons: () => void };

// Update checker
interface GitHubRelease {
  tag_name: string;
  name: string;
  body: string;
  html_url: string;
  prerelease: boolean;
}

async function checkForUpdates(): Promise<void> {
  try {
    // Use releases list instead of /latest to avoid 404 when no releases exist
    const response = await fetch(
      "https://api.github.com/repos/zortos293/GFNClient/releases?per_page=1"
    );

    if (!response.ok) {
      console.log("Could not check for updates (API error)");
      return;
    }

    const releases = await response.json();

    if (!Array.isArray(releases) || releases.length === 0) {
      // No releases published yet - this is expected for new projects
      console.log("No releases found - skipping update check");
      return;
    }

    // Use the first (most recent) release
    await handleReleaseCheck(releases[0]);
  } catch (error) {
    // Network errors, etc - silently ignore
    console.log("Update check skipped:", error instanceof Error ? error.message : "network error");
  }
}

async function handleReleaseCheck(release: GitHubRelease): Promise<void> {
  const latestVersion = release.tag_name.replace(/^v/, "");
  const currentVersion = await getVersion();

  // First check if latest is actually newer than current
  if (!isNewerVersion(latestVersion, currentVersion)) {
    console.log("App is up to date:", currentVersion);
    // Clear any skipped version since we're now at or past it
    localStorage.removeItem("skippedVersion");
    return;
  }

  // Latest is newer - check if user explicitly skipped this version
  const skippedVersion = localStorage.getItem("skippedVersion");
  if (skippedVersion === latestVersion) {
    console.log("User skipped version", latestVersion);
    return;
  }

  // Show update modal
  console.log("Update available:", latestVersion);
  showUpdateModal(release, latestVersion);
}

function isNewerVersion(latest: string, current: string): boolean {
  const latestParts = latest.split(".").map(Number);
  const currentParts = current.split(".").map(Number);

  for (let i = 0; i < Math.max(latestParts.length, currentParts.length); i++) {
    const l = latestParts[i] || 0;
    const c = currentParts[i] || 0;
    if (l > c) return true;
    if (l < c) return false;
  }
  return false;
}

function showUpdateModal(release: GitHubRelease, version: string): void {
  const modal = document.getElementById("update-modal");
  const versionSpan = document.getElementById("update-version");
  const changelogDiv = document.getElementById("update-changelog-content");
  const downloadBtn = document.getElementById("update-download-btn") as HTMLAnchorElement;
  const skipBtn = document.getElementById("update-skip-btn");
  const laterBtn = document.getElementById("update-later-btn");

  if (!modal || !versionSpan || !changelogDiv || !downloadBtn) return;

  versionSpan.textContent = `v${version}`;

  // Parse changelog from release body
  const changelog = release.body || "No changelog available.";
  changelogDiv.innerHTML = formatChangelog(changelog);

  // Set download link
  downloadBtn.href = release.html_url;

  // Show modal
  modal.classList.remove("hidden");

  // Reinitialize Lucide icons
  if (typeof lucide !== 'undefined') {
    lucide.createIcons();
  }

  // Skip button - remember to skip this version
  skipBtn?.addEventListener("click", () => {
    localStorage.setItem("skippedVersion", version);
    modal.classList.add("hidden");
  });

  // Later button - just close
  laterBtn?.addEventListener("click", () => {
    modal.classList.add("hidden");
  });

  // Close button
  const closeBtn = modal.querySelector(".modal-close");
  closeBtn?.addEventListener("click", () => {
    modal.classList.add("hidden");
  });
}

function formatChangelog(body: string): string {
  // Convert markdown-style changelog to HTML
  let html = body
    // Convert headers
    .replace(/^### (.+)$/gm, "<strong>$1</strong>")
    .replace(/^## (.+)$/gm, "<strong>$1</strong>")
    // Convert bullet points
    .replace(/^[*-] (.+)$/gm, "<li>$1</li>")
    // Convert newlines
    .replace(/\n\n/g, "<br><br>")
    .replace(/\n/g, " ");

  // Wrap lists
  if (html.includes("<li>")) {
    html = html.replace(/(<li>.*<\/li>)/g, "<ul>$1</ul>");
    // Clean up consecutive ul tags
    html = html.replace(/<\/ul>\s*<ul>/g, "");
  }

  return html;
}

// Initialize
document.addEventListener("DOMContentLoaded", async () => {
  // Initialize frontend logging first (intercepts console.*)
  initLogging();

  console.log("OpenNOW initialized");

  // Initialize Lucide icons
  if (typeof lucide !== 'undefined') {
    lucide.createIcons();
  }

  // Initialize custom dropdowns
  initializeDropdowns();

  // Setup navigation
  setupNavigation();

  // Setup modals
  setupModals();

  // Setup login modal
  setupLoginModal();

  // Setup session modals
  setupSessionModals();

  // Setup queue times nav click handler
  const queueTimesNav = document.getElementById("queue-times-nav");
  if (queueTimesNav) {
    queueTimesNav.addEventListener("click", (e) => {
      e.preventDefault();
      showQueueTimesPage();
    });
  }

  // Setup search
  setupSearch();

  // Load saved settings
  await loadSettings();

  // Check auth status
  await checkAuthStatus();

  // Load initial data
  await loadHomeData();

  // Run latency test in background on startup
  testLatency().catch(err => console.error("Initial latency test failed:", err));

  // Check for updates
  checkForUpdates();

  // Check for active sessions after auth (if authenticated)
  if (isAuthenticated) {
    const sessions = await checkActiveSessions();
    if (sessions.length > 0) {
      // Show navbar indicator and modal
      updateNavbarSessionIndicator(sessions[0]);
      showActiveSessionModal(sessions[0]);
    }
    // Start polling for active sessions every 10 seconds
    startSessionPolling();
  }

  // Setup region dropdown change handler
  onDropdownChange("region-setting", (value) => {
    currentRegion = value;
    updateStatusBarLatency();
  });
});

// Detect platform
const isMacOS = navigator.platform.toUpperCase().includes("MAC") ||
  navigator.userAgent.toUpperCase().includes("MAC");
const isWindows = navigator.platform.toUpperCase().includes("WIN") ||
  navigator.userAgent.toUpperCase().includes("WIN");

// Load settings from backend and apply to UI
async function loadSettings() {
  try {
    const settings = await invoke<Settings>("get_settings");
    console.log("Loaded settings:", settings);

    // Apply to global state
    currentQuality = settings.quality || "auto";
    currentResolution = settings.resolution || "1920x1080";
    currentFps = settings.fps || 60;
    currentCodec = settings.codec || "h264";
    currentAudioCodec = settings.audio_codec || "opus";
    currentMaxBitrate = settings.max_bitrate_mbps || 200;
    discordRpcEnabled = settings.discord_rpc !== false; // Default to true
    discordShowStats = settings.discord_show_stats === true; // Default to false
    currentRegion = settings.region || "auto";
    reflexEnabled = settings.reflex !== false; // Default to true

    // Apply to UI elements (non-dropdown)
    const bitrateEl = document.getElementById("bitrate-setting") as HTMLInputElement;
    const bitrateValueEl = document.getElementById("bitrate-value");
    const discordEl = document.getElementById("discord-setting") as HTMLInputElement;
    const discordStatsEl = document.getElementById("discord-stats-setting") as HTMLInputElement;
    const telemetryEl = document.getElementById("telemetry-setting") as HTMLInputElement;
    const proxyEl = document.getElementById("proxy-setting") as HTMLInputElement;

    // Update codec dropdown options
    // H.265/HEVC only supported on macOS (Windows browser doesn't support HEVC decoding)
    // AV1 requires RTX 30+/RX 6000+ for hardware decoding
    const codecOptions: { value: string; text: string; selected?: boolean; disabled?: boolean }[] = [
      { value: "h264", text: "H.264 (Best Compatibility)", selected: currentCodec === "h264" },
    ];
    
    if (isMacOS) {
      codecOptions.push({ value: "h265", text: "H.265/HEVC (Lower Latency)", selected: currentCodec === "h265" });
    }
    
    codecOptions.push({ value: "av1", text: "AV1 (Requires AV1 Decoder - RTX 30+/RX 6000+)", selected: currentCodec === "av1" });
    
    // If user had H.265 selected on Windows, fall back to H.264
    if (isWindows && currentCodec === "h265") {
      currentCodec = "h264";
      codecOptions[0].selected = true;
    }
    
    setDropdownOptions("codec-setting", codecOptions);

    // Update audio codec dropdown options based on platform
    const audioCodecOptions = [
      { value: "opus", text: "Opus (Default)", selected: currentAudioCodec === "opus" },
    ];
    if (isMacOS) {
      audioCodecOptions.push({
        value: "opus-stereo",
        text: "Opus Stereo (Better Audio)",
        selected: currentAudioCodec === "opus-stereo"
      });
    } else if (currentAudioCodec === "opus-stereo") {
      // Fall back to Opus if not on macOS
      currentAudioCodec = "opus";
      audioCodecOptions[0].selected = true;
    }
    setDropdownOptions("audio-codec-setting", audioCodecOptions);

    // Apply dropdown values
    setDropdownValue("resolution-setting", currentResolution);
    setDropdownValue("fps-setting", String(currentFps));
    setDropdownValue("region-setting", currentRegion);

    // Apply non-dropdown values
    if (bitrateEl) {
      bitrateEl.value = String(currentMaxBitrate);
      if (bitrateValueEl) {
        bitrateValueEl.textContent = currentMaxBitrate >= 200 ? "Unlimited" : `${currentMaxBitrate} Mbps`;
      }
    }
    if (discordEl) discordEl.checked = discordRpcEnabled;
    if (discordStatsEl) discordStatsEl.checked = discordShowStats;
    if (telemetryEl) telemetryEl.checked = settings.disable_telemetry ?? true;
    if (proxyEl && settings.proxy) proxyEl.value = settings.proxy;

    const reflexEl = document.getElementById("reflex-setting") as HTMLInputElement;
    if (reflexEl) reflexEl.checked = reflexEnabled;

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

  // Clear search input and hide search view when navigating away
  const searchInput = document.getElementById("search-input") as HTMLInputElement;
  if (searchInput) {
    searchInput.value = "";
  }
  hideSearchDropdown();

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

  // Export logs button
  document.getElementById("export-logs-btn")?.addEventListener("click", async () => {
    const btn = document.getElementById("export-logs-btn") as HTMLButtonElement;
    const originalText = btn.textContent;
    btn.textContent = "Exporting...";
    btn.disabled = true;

    try {
      const savedPath = await exportLogs();
      console.log("Logs exported to:", savedPath);
      btn.textContent = "Exported!";
      setTimeout(() => {
        btn.textContent = originalText;
        btn.disabled = false;
      }, 2000);
    } catch (error) {
      console.error("Failed to export logs:", error);
      btn.textContent = originalText;
      btn.disabled = false;
      // Don't show error for cancelled export
      if (error !== "Export cancelled") {
        alert("Failed to export logs: " + error);
      }
    }
  });

  // Clear logs button
  document.getElementById("clear-logs-btn")?.addEventListener("click", async () => {
    const btn = document.getElementById("clear-logs-btn") as HTMLButtonElement;
    const originalText = btn.textContent;

    try {
      await clearLogs();
      console.log("Logs cleared");
      btn.textContent = "Cleared!";
      setTimeout(() => {
        btn.textContent = originalText;
      }, 2000);
    } catch (error) {
      console.error("Failed to clear logs:", error);
      alert("Failed to clear logs: " + error);
    }
  });

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

// ============================================================================
// SESSION DETECTION
// ============================================================================

// Check for active sessions on startup or before launching a game
async function checkActiveSessions(): Promise<ActiveSession[]> {
  try {
    console.log("Checking for active sessions...");
    const accessToken = await invoke<string>("get_gfn_jwt");
    console.log("Got JWT token, calling get_active_sessions...");
    const sessions = await invoke<ActiveSession[]>("get_active_sessions", {
      accessToken,
    });
    detectedActiveSessions = sessions;
    console.log("Active sessions response:", sessions, `(${sessions.length})`);
    if (sessions.length > 0) {
      console.log("First session details:", JSON.stringify(sessions[0], null, 2));
    }
    return sessions;
  } catch (error) {
    console.error("Failed to check active sessions:", error);
    return [];
  }
}

// Start polling for active sessions (when not streaming)
function startSessionPolling() {
  // Don't start if already polling or currently streaming
  if (sessionPollingInterval !== null) {
    console.log("Session polling already active");
    return;
  }

  if (isStreamingActive()) {
    console.log("Not starting session polling - currently streaming");
    return;
  }

  if (!isAuthenticated) {
    console.log("Not starting session polling - not authenticated");
    return;
  }

  console.log("Starting session polling (every 10 seconds)");

  sessionPollingInterval = window.setInterval(async () => {
    // Stop polling if we started streaming
    if (isStreamingActive()) {
      console.log("Stopping session polling - streaming started");
      stopSessionPolling();
      return;
    }

    // Don't poll if not authenticated
    if (!isAuthenticated) {
      console.log("Stopping session polling - no longer authenticated");
      stopSessionPolling();
      return;
    }

    const sessions = await checkActiveSessions();
    if (sessions.length > 0) {
      // Update navbar indicator if not already showing
      const existingIndicator = document.getElementById("active-session-indicator");
      if (!existingIndicator) {
        console.log("Active session detected via polling:", sessions[0].sessionId);
        updateNavbarSessionIndicator(sessions[0]);
        showActiveSessionModal(sessions[0]);
      }
    } else {
      // No active sessions - hide indicator if showing
      hideNavbarSessionIndicator();
    }
  }, SESSION_POLLING_INTERVAL_MS);
}

// Stop polling for active sessions
function stopSessionPolling() {
  if (sessionPollingInterval !== null) {
    console.log("Stopping session polling");
    window.clearInterval(sessionPollingInterval);
    sessionPollingInterval = null;
  }
}

// Find game title by app ID
function getGameTitleByAppId(appId: number | undefined): string {
  if (!appId) return "Unknown Game";
  const game = games.find((g) => g.id === String(appId));
  return game?.title || `Game ID: ${appId}`;
}

// Show the active session modal with session info
function showActiveSessionModal(session: ActiveSession) {
  const gameEl = document.getElementById("active-session-game");
  const gpuEl = document.getElementById("active-session-gpu");
  const resolutionEl = document.getElementById("active-session-resolution");
  const serverEl = document.getElementById("active-session-server");

  if (gameEl) gameEl.textContent = getGameTitleByAppId(session.appId);
  if (gpuEl) gpuEl.textContent = session.gpuType || "Unknown GPU";
  if (resolutionEl) {
    const res = session.resolution || "Unknown";
    const fps = session.fps ? `@ ${session.fps} FPS` : "";
    resolutionEl.textContent = `${res} ${fps}`.trim();
  }
  if (serverEl) serverEl.textContent = session.serverIp || "Unknown";

  // Also update navbar indicator
  updateNavbarSessionIndicator(session);

  showModal("active-session-modal");
}

// Show the session conflict modal when trying to launch a new game
function showSessionConflictModal(existingSession: ActiveSession, newGame: Game) {
  const gameEl = document.getElementById("conflict-session-game");
  const gpuEl = document.getElementById("conflict-session-gpu");

  if (gameEl) gameEl.textContent = getGameTitleByAppId(existingSession.appId);
  if (gpuEl) gpuEl.textContent = existingSession.gpuType || "Unknown GPU";

  pendingGameLaunch = newGame;
  showModal("session-conflict-modal");
}

// Store the game for retry
let regionErrorGame: Game | null = null;
let sessionLimitGame: Game | null = null;

// Show region error modal
function showRegionErrorModal(errorMessage: string, game: Game) {
  const errorEl = document.getElementById("region-error-message");
  if (errorEl) {
    // Extract the status description from the error message
    const match = errorMessage.match(/REGION_NOT_SUPPORTED[_A-Z]*\s*[A-F0-9]*/i);
    errorEl.textContent = match ? match[0] : "Region not supported";
  }
  regionErrorGame = game;
  showModal("region-error-modal");
}

// Show session limit exceeded modal
function showSessionLimitModal(errorMessage: string, game: Game) {
  const errorEl = document.getElementById("session-limit-error-message");
  if (errorEl) {
    // Extract the status description from the error message
    const match = errorMessage.match(/SESSION_LIMIT[_A-Z]*\s*[A-F0-9]*/i);
    errorEl.textContent = match ? match[0] : "Session limit exceeded";
  }
  sessionLimitGame = game;
  showModal("session-limit-modal");
}

// Update navbar with active session indicator
function updateNavbarSessionIndicator(session: ActiveSession | null) {
  let indicator = document.getElementById("active-session-indicator");

  if (!session) {
    // Remove indicator if no session
    indicator?.remove();
    return;
  }

  // Create indicator if it doesn't exist
  if (!indicator) {
    indicator = document.createElement("div");
    indicator.id = "active-session-indicator";
    indicator.className = "active-session-indicator";

    // Insert after nav items
    const nav = document.querySelector(".main-nav");
    if (nav) {
      nav.appendChild(indicator);
    }
  }

  // Clear existing content
  indicator.replaceChildren();

  const gameName = getGameTitleByAppId(session.appId);
  const shortName = gameName.length > 20 ? gameName.substring(0, 20) + "..." : gameName;

  // Create elements safely
  const dot = document.createElement("span");
  dot.className = "session-indicator-dot";

  const text = document.createElement("span");
  text.className = "session-indicator-text";
  text.textContent = shortName;

  const gpu = document.createElement("span");
  gpu.className = "session-indicator-gpu";
  gpu.textContent = session.gpuType || "GPU";

  indicator.appendChild(dot);
  indicator.appendChild(text);
  indicator.appendChild(gpu);

  // Click to show modal
  indicator.onclick = () => showActiveSessionModal(session);
}

// Hide navbar session indicator
function hideNavbarSessionIndicator() {
  updateNavbarSessionIndicator(null);
}

// Update navbar with storage indicator
function updateNavbarStorageIndicator(subscription: SubscriptionInfo | null) {
  let indicator = document.getElementById("storage-indicator");

  // Find permanent storage addon
  const storageAddon = subscription?.addons?.find(
    (addon) => addon.subType === "PERMANENT_STORAGE"
  );

  if (!storageAddon) {
    // Remove indicator if no storage addon
    indicator?.remove();
    return;
  }

  // Extract storage info from attributes
  const totalAttr = storageAddon.attributes?.find(a => a.key === "TOTAL_STORAGE_SIZE_IN_GB");
  const usedAttr = storageAddon.attributes?.find(a => a.key === "USED_STORAGE_SIZE_IN_GB");
  const regionAttr = storageAddon.attributes?.find(a => a.key === "STORAGE_METRO_REGION_NAME");

  const totalGB = parseInt(totalAttr?.textValue || "0", 10);
  const usedGB = parseInt(usedAttr?.textValue || "0", 10);
  const region = regionAttr?.textValue || "Unknown";

  if (totalGB === 0) {
    indicator?.remove();
    return;
  }

  // Create indicator if it doesn't exist
  if (!indicator) {
    indicator = document.createElement("div");
    indicator.id = "storage-indicator";
    indicator.className = "storage-indicator";

    // Insert in the status bar left section
    const statusLeft = document.querySelector(".status-left");
    if (statusLeft) {
      statusLeft.appendChild(indicator);
    }
  }

  // Clear existing content
  indicator.replaceChildren();

  // Calculate percentage for coloring
  const percentage = Math.round((usedGB / totalGB) * 100);

  // Determine color based on usage
  let color = "#76b900"; // green
  if (percentage >= 90) {
    color = "#f44336"; // red
  } else if (percentage >= 75) {
    color = "#ffc107"; // yellow
  }

  // Create elements with inline color
  const icon = document.createElement("i");
  icon.setAttribute("data-lucide", "hard-drive");
  icon.style.width = "12px";
  icon.style.height = "12px";
  icon.style.color = color;

  const text = document.createElement("span");
  text.textContent = `${usedGB} / ${totalGB} GB`;
  text.style.color = color;
  text.style.fontSize = "12px";

  indicator.style.color = color;
  indicator.appendChild(icon);
  indicator.appendChild(text);
  indicator.title = `Cloud Storage: ${usedGB} GB used of ${totalGB} GB\nLocation: ${region}`;

  // Re-init Lucide icons for the new icon
  if (typeof lucide !== 'undefined') {
    lucide.createIcons();
  }

  // Apply color to SVG after Lucide creates it
  setTimeout(() => {
    const svg = indicator.querySelector("svg");
    if (svg) {
      svg.style.color = color;
      svg.style.stroke = color;
      svg.style.width = "12px";
      svg.style.height = "12px";
    }
  }, 10);
}

// Update status bar with session time remaining
function updateStatusBarSessionTime(subscription: SubscriptionInfo | null) {
  let indicator = document.getElementById("session-time-indicator");

  if (!subscription || !subscription.remainingTimeInMinutes) {
    indicator?.remove();
    return;
  }

  const remaining = subscription.remainingTimeInMinutes;
  const total = subscription.totalTimeInMinutes || 0;
  const remainingHrs = Math.floor(remaining / 60);
  const totalHrs = Math.floor(total / 60);
  const percentRemaining = total > 0 ? Math.round((remaining / total) * 100) : 100;

  // Create indicator if it doesn't exist
  if (!indicator) {
    indicator = document.createElement("div");
    indicator.id = "session-time-indicator";
    indicator.className = "session-time-indicator";

    // Insert in the status bar left section
    const statusLeft = document.querySelector(".status-left");
    if (statusLeft) {
      statusLeft.appendChild(indicator);
    }
  }

  // Clear existing content
  indicator.replaceChildren();

  // Determine color based on remaining time
  let color = "#76b900"; // green
  if (percentRemaining <= 10) {
    color = "#f44336"; // red
  } else if (percentRemaining <= 25) {
    color = "#ffc107"; // yellow
  }

  // Create elements with inline color
  const icon = document.createElement("i");
  icon.setAttribute("data-lucide", "clock");
  icon.style.width = "12px";
  icon.style.height = "12px";
  icon.style.color = color;

  const text = document.createElement("span");
  text.textContent = `${remainingHrs}h / ${totalHrs}h`;
  text.style.color = color;
  text.style.fontSize = "12px";

  indicator.style.color = color;
  indicator.appendChild(icon);
  indicator.appendChild(text);
  indicator.title = `Session time: ${remainingHrs} hours remaining of ${totalHrs} hours total`;

  // Re-init Lucide icons
  if (typeof lucide !== 'undefined') {
    lucide.createIcons();
  }

  // Apply color to SVG after Lucide creates it
  setTimeout(() => {
    const svg = indicator.querySelector("svg");
    if (svg) {
      svg.style.color = color;
      svg.style.stroke = color;
      svg.style.width = "12px";
      svg.style.height = "12px";
    }
  }, 10);
}

// Update queue times nav visibility (only for free tier users)
function updateQueueTimesLinkVisibility(subscription: SubscriptionInfo | null) {
  const navItem = document.getElementById("queue-times-nav");
  if (!navItem) return;

  // Show nav item for free tier users who are authenticated
  if (isAuthenticated && isFreeTier(subscription)) {
    navItem.classList.remove("hidden");
  } else {
    navItem.classList.add("hidden");
  }
}

// Setup session modal handlers
function setupSessionModals() {
  // Active session modal handlers
  const connectBtn = document.getElementById("connect-session-btn");
  const terminateBtn = document.getElementById("terminate-session-btn");
  const dismissBtn = document.getElementById("dismiss-session-btn");

  connectBtn?.addEventListener("click", async () => {
    if (detectedActiveSessions.length > 0) {
      hideAllModals();
      await connectToExistingSession(detectedActiveSessions[0]);
    }
  });

  terminateBtn?.addEventListener("click", async () => {
    if (detectedActiveSessions.length > 0) {
      try {
        const accessToken = await invoke<string>("get_gfn_jwt");
        await invoke("terminate_session", {
          sessionId: detectedActiveSessions[0].sessionId,
          accessToken,
        });
        console.log("Session terminated");
        detectedActiveSessions = [];
        hideNavbarSessionIndicator();
        hideAllModals();
      } catch (error) {
        console.error("Failed to terminate session:", error);
      }
    }
  });

  dismissBtn?.addEventListener("click", () => {
    hideAllModals();
  });

  // Session conflict modal handlers
  const terminateAndLaunchBtn = document.getElementById("terminate-and-launch-btn");
  const cancelLaunchBtn = document.getElementById("cancel-launch-btn");

  terminateAndLaunchBtn?.addEventListener("click", async () => {
    if (detectedActiveSessions.length > 0 && pendingGameLaunch) {
      try {
        const accessToken = await invoke<string>("get_gfn_jwt");
        await invoke("terminate_session", {
          sessionId: detectedActiveSessions[0].sessionId,
          accessToken,
        });
        console.log("Session terminated, launching new game");
        detectedActiveSessions = [];
        hideNavbarSessionIndicator();
        hideAllModals();
        // Launch the pending game
        const gameToLaunch = pendingGameLaunch;
        pendingGameLaunch = null;
        await launchGame(gameToLaunch);
      } catch (error) {
        console.error("Failed to terminate session:", error);
      }
    }
  });

  cancelLaunchBtn?.addEventListener("click", () => {
    pendingGameLaunch = null;
    hideAllModals();
  });

  // Region error modal handlers
  const regionRetryBtn = document.getElementById("region-error-retry-btn");
  const regionCloseBtn = document.getElementById("region-error-close-btn");

  regionRetryBtn?.addEventListener("click", async () => {
    hideAllModals();
    if (regionErrorGame) {
      const gameToRetry = regionErrorGame;
      regionErrorGame = null;
      await launchGame(gameToRetry);
    }
  });

  regionCloseBtn?.addEventListener("click", () => {
    regionErrorGame = null;
    hideAllModals();
  });

  // Session limit modal handlers
  const sessionLimitTerminateBtn = document.getElementById("session-limit-terminate-btn");
  const sessionLimitCloseBtn = document.getElementById("session-limit-close-btn");

  sessionLimitTerminateBtn?.addEventListener("click", async () => {
    hideAllModals();
    // Try to terminate any active sessions and retry
    try {
      const accessToken = await invoke<string>("get_gfn_jwt");
      // Check for active sessions
      const activeSessions = await invoke<ActiveSession[]>("get_active_sessions", { accessToken });

      if (activeSessions.length > 0) {
        // Terminate the first active session
        await invoke("terminate_session", {
          sessionId: activeSessions[0].sessionId,
          accessToken,
        });
        console.log("Terminated existing session:", activeSessions[0].sessionId);
        detectedActiveSessions = [];
        hideNavbarSessionIndicator();
      }

      // Retry launching the game
      if (sessionLimitGame) {
        const gameToRetry = sessionLimitGame;
        sessionLimitGame = null;
        await launchGame(gameToRetry);
      }
    } catch (error) {
      console.error("Failed to terminate session:", error);
      alert(`Failed to terminate session: ${error}`);
    }
  });

  sessionLimitCloseBtn?.addEventListener("click", () => {
    sessionLimitGame = null;
    hideAllModals();
  });
}

// Connect to an existing session
async function connectToExistingSession(session: ActiveSession) {
  console.log("Connecting to existing session:", session.sessionId);

  // Stop session polling while we're reconnecting/streaming
  stopSessionPolling();

  // Get the GFN JWT token
  let accessToken: string;
  try {
    accessToken = await invoke<string>("get_gfn_jwt");
  } catch (e) {
    console.error("Not authenticated:", e);
    startSessionPolling(); // Resume polling since we're not connecting
    return;
  }

  // Find the game for this session
  const game = games.find((g) => g.id === String(session.appId));
  const gameName = game?.title || `Game (${session.appId})`;

  // Show streaming overlay
  showStreamingOverlay(gameName, "Connecting to session...");

  // Update Discord presence (if enabled)
  if (discordRpcEnabled) {
    try {
      streamingUIState.gameStartTime = Math.floor(Date.now() / 1000);
      await invoke("set_game_presence", {
        gameName: gameName,
        region: null,
        resolution: discordShowStats ? session.resolution : null,
        fps: discordShowStats ? session.fps : null,
        latencyMs: null,
      });
    } catch (e) {
      console.warn("Discord presence update failed:", e);
    }
  }

  try {
    // Set up streaming state
    streamingUIState.sessionId = session.sessionId;
    streamingUIState.gameName = gameName;
    streamingUIState.active = true;
    streamingUIState.gpuType = session.gpuType;
    streamingUIState.serverIp = session.serverIp;

    // Extract stream IP from signaling URL or connection info
    // signalingUrl format: "wss://66-22-147-39.cloudmatchbeta.nvidiagrid.net:443/nvst/"
    let streamIp: string | null = session.serverIp;
    if (session.signalingUrl) {
      const match = session.signalingUrl.match(/wss:\/\/([^:\/]+)/);
      if (match) {
        streamIp = match[1];
      }
    }

    if (!streamIp || !session.signalingUrl) {
      throw new Error("Missing stream IP or signaling URL for reconnection");
    }

    // IMPORTANT: Claim the session first with a PUT request
    // This is required by the GFN server to "activate" the session for streaming
    // Without this, the WebRTC connection will timeout
    updateStreamingStatus("Claiming session...");
    console.log("Claiming session with PUT request...");

    interface ClaimSessionResponse {
      sessionId: string;
      status: number;
      gpuType: string | null;
      signalingUrl: string | null;
      serverIp: string | null;
      connectionInfo: Array<{ ip: string | null; port: number | null; usage: number }> | null;
    }

    const claimResult = await invoke<ClaimSessionResponse>("claim_session", {
      sessionId: session.sessionId,
      serverIp: streamIp,
      accessToken: accessToken,
      appId: String(session.appId), // Must be string like "106466949"
      resolution: session.resolution || "1920x1080",
      fps: session.fps || 60,
    });

    console.log("Session claimed successfully:", claimResult);
    console.log("Claim result details - signalingUrl:", claimResult.signalingUrl, "serverIp:", claimResult.serverIp);

    // Update streaming state with claimed values
    if (claimResult.gpuType) {
      streamingUIState.gpuType = claimResult.gpuType;
    }
    if (claimResult.serverIp) {
      streamingUIState.serverIp = claimResult.serverIp;
    }

    // Use the signaling URL from the claim response (which is now from the polled GET when status is 2)
    // The backend polls until the session transitions from status 6 to status 2/3, then returns
    // the correct connectionInfo with the signaling URL.
    // Fall back to original if claim response doesn't have one.
    const actualSignalingUrl = claimResult.signalingUrl || session.signalingUrl;
    console.log("Using signaling URL from claim (polled until ready):", actualSignalingUrl);
    console.log("Original session signalingUrl:", session.signalingUrl);
    console.log("Claim result status:", claimResult.status);

    // Extract the stream IP from the signaling URL
    let actualStreamIp = streamIp;
    if (actualSignalingUrl) {
      const match = actualSignalingUrl.match(/wss:\/\/([^:\/]+)/);
      if (match) {
        actualStreamIp = match[1];
        console.log("Extracted stream IP from signaling URL:", actualStreamIp);
      }
    }
    console.log("Final stream IP to use:", actualStreamIp);

    // Set up the backend session storage for reconnection
    // This is required for get_webrtc_config and other backend functions to work
    // Pass connectionInfo for proper ICE candidate construction with real media ports
    await invoke("setup_reconnect_session", {
      sessionId: session.sessionId,
      serverIp: actualStreamIp,
      signalingUrl: actualSignalingUrl,
      gpuType: claimResult.gpuType || session.gpuType,
      connectionInfo: claimResult.connectionInfo || null,
    });

    console.log("Reconnect session setup complete");

    // Build the streaming result object to pass to initializeStreaming
    // Use type assertion since we're constructing a compatible object
    // Use claimed session values which may be updated after the PUT request
    //
    // connectionInfo contains multiple entries with different usage types:
    //   - usage=2:  Primary media path (UDP) - preferred for streaming
    //   - usage=17: Alternative media path - used by some Alliance Partners (e.g., Zain)
    //               when primary media entry is not available
    //   - usage=14: Signaling (WSS) - MUST NOT be used for media traffic
    //
    // We prefer usage=2 and fall back to usage=17 for Alliance Partner compatibility
    const mediaConn = claimResult.connectionInfo?.find(c => c.usage === 2)
      || claimResult.connectionInfo?.find(c => c.usage === 17);
    const realMediaPort = mediaConn?.port || 443;
    const realMediaIp = mediaConn?.ip || actualStreamIp;
    console.log("Using media connection info for reconnect - IP:", realMediaIp, "Port:", realMediaPort, "Usage:", mediaConn?.usage);

    const streamingResult = {
      sessionId: session.sessionId,
      phase: "Ready" as const,
      serverIp: claimResult.serverIp || actualStreamIp,
      signalingUrl: actualSignalingUrl,
      gpuType: claimResult.gpuType || session.gpuType,
      connectionInfo: (actualStreamIp && session.serverIp) ? {
        controlIp: (claimResult.serverIp || session.serverIp) as string,
        controlPort: 443,
        streamIp: realMediaIp,
        streamPort: realMediaPort,
        resourcePath: "/nvst/",
      } : null,
      error: null as string | null,
    };

    console.log("Streaming result for reconnect:", streamingResult);

    updateStreamingStatus(`Connected to ${claimResult.gpuType || session.gpuType || "GPU"}`);
    showStreamingInfo(streamingResult);

    // Create fullscreen streaming container
    const streamContainer = createStreamingContainer(gameName);

    // Initialize WebRTC streaming
    const streamingOptions: StreamingOptions = {
      resolution: session.resolution || currentResolution,
      fps: session.fps || currentFps
    };
    await initializeStreaming(streamingResult, accessToken, streamContainer, streamingOptions);

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

    console.log("Connected to existing session successfully");
  } catch (error) {
    console.error("Failed to connect to session:", error);
    streamingUIState.active = false;
    hideStreamingOverlay();

    // Show a helpful error message
    const errorMsg = String(error);
    if (errorMsg.includes("timeout") || errorMsg.includes("Timeout")) {
      showSessionReconnectError(
        "Connection Timeout",
        "Could not connect to the session. This usually happens when the session is already streaming to another client (like a browser tab).\n\nPlease close any other GFN clients or browser tabs running this session, then try again."
      );
    } else {
      showSessionReconnectError("Connection Failed", errorMsg);
    }

    if (discordRpcEnabled) {
      try {
        await invoke("set_browsing_presence");
      } catch (e) {
        // Ignore
      }
    }

    // Resume session polling since reconnection failed
    startSessionPolling();
  }
}

// Show reconnect error message
function showSessionReconnectError(title: string, message: string) {
  // Update the active session modal to show error
  const gameEl = document.getElementById("active-session-game");
  const gpuEl = document.getElementById("active-session-gpu");
  const resolutionEl = document.getElementById("active-session-resolution");
  const serverEl = document.getElementById("active-session-server");

  // Create error display
  const modal = document.getElementById("active-session-modal");
  if (modal) {
    const content = modal.querySelector(".session-modal-content");
    if (content) {
      const header = content.querySelector(".session-modal-header h2");
      if (header) header.textContent = title;

      const desc = content.querySelector(".session-modal-description");
      if (desc) desc.textContent = message;

      // Change icon to warning
      const icon = content.querySelector(".session-icon");
      if (icon) {
        icon.classList.add("warning");
        icon.textContent = "\u26A0"; // Warning symbol
      }
    }
  }

  showModal("active-session-modal");
}

// Search
let currentSearchQuery = "";
let searchResultsCache: Game[] = [];

function setupSearch() {
  let searchTimeout: number;
  const searchDropdown = document.getElementById("search-dropdown")!;

  searchInput.addEventListener("input", () => {
    clearTimeout(searchTimeout);
    const query = searchInput.value.trim();

    if (query.length < 2) {
      hideSearchDropdown();
      currentSearchQuery = "";
      return;
    }

    searchTimeout = setTimeout(async () => {
      currentSearchQuery = query;
      await searchGamesForSuggestions(query);
    }, 300);
  });

  // Handle Enter key for full search results
  searchInput.addEventListener("keydown", async (e) => {
    if (e.key === "Enter") {
      e.preventDefault();
      const query = searchInput.value.trim();
      if (query.length >= 2) {
        hideSearchDropdown();
        await showFullSearchResults(query);
      }
    } else if (e.key === "Escape") {
      hideSearchDropdown();
    }
  });

  // Close dropdown when clicking outside
  document.addEventListener("click", (e) => {
    if (!searchInput.contains(e.target as Node) && !searchDropdown.contains(e.target as Node)) {
      hideSearchDropdown();
    }
  });
}

function hideSearchDropdown() {
  const searchDropdown = document.getElementById("search-dropdown");
  if (searchDropdown) {
    searchDropdown.classList.add("hidden");
  }
}

function showSearchDropdown(games: Game[]) {
  const searchDropdown = document.getElementById("search-dropdown")!;
  searchDropdown.replaceChildren();

  if (games.length === 0) {
    const noResults = document.createElement("div");
    noResults.className = "search-dropdown-empty";
    noResults.textContent = "No games found";
    searchDropdown.appendChild(noResults);
  } else {
    games.forEach((game) => {
      const item = document.createElement("div");
      item.className = "search-dropdown-item";
      item.dataset.gameId = game.id;

      const img = document.createElement("img");
      img.className = "search-dropdown-image";
      img.src = game.images.box_art || game.images.thumbnail || getFallbackPlaceholder(game.title);
      img.alt = game.title;
      img.referrerPolicy = "no-referrer";
      img.onerror = () => { img.src = getFallbackPlaceholder(game.title); };

      const info = document.createElement("div");
      info.className = "search-dropdown-info";

      const title = document.createElement("div");
      title.className = "search-dropdown-title";
      title.textContent = game.title;

      const store = document.createElement("div");
      store.className = "search-dropdown-store";
      store.textContent = game.store.store_type;

      info.appendChild(title);
      info.appendChild(store);
      item.appendChild(img);
      item.appendChild(info);

      item.addEventListener("click", () => {
        hideSearchDropdown();
        showGameDetail(game.id);
      });

      searchDropdown.appendChild(item);
    });

    // Add "View all results" link if there are more results
    if (searchResultsCache.length >= 5) {
      const viewAll = document.createElement("div");
      viewAll.className = "search-dropdown-viewall";
      viewAll.textContent = `View all results for "${currentSearchQuery}"`;
      viewAll.addEventListener("click", async () => {
        hideSearchDropdown();
        await showFullSearchResults(currentSearchQuery);
      });
      searchDropdown.appendChild(viewAll);
    }
  }

  searchDropdown.classList.remove("hidden");
}

async function searchGamesForSuggestions(query: string) {
  try {
    const token = isAuthenticated ? await invoke<string>("get_gfn_jwt").catch(() => null) : null;
    const results = await invoke<{ games: Game[] }>("search_games_graphql", {
      query,
      limit: 5,
      accessToken: token,
      vpcId: null,
    });
    searchResultsCache = results.games;
    showSearchDropdown(results.games);
  } catch (error) {
    console.error("Search failed:", error);
    showSearchDropdown([]);
  }
}

async function showFullSearchResults(query: string) {
  try {
    const token = isAuthenticated ? await invoke<string>("get_gfn_jwt").catch(() => null) : null;
    const results = await invoke<{ games: Game[]; total_count: number }>("search_games_graphql", {
      query,
      limit: 50,
      accessToken: token,
      vpcId: null,
    });

    // Show search results in main content area
    currentView = "search";

    // Deselect all nav items since search is not a nav view
    navItems.forEach((item) => item.classList.remove("active"));

    // Hide all other views
    document.querySelectorAll(".view").forEach((v) => {
      v.classList.remove("active");
    });

    // Get or create search results view
    let searchView = document.getElementById("search-view");
    if (!searchView) {
      searchView = document.createElement("section");
      searchView.id = "search-view";
      searchView.className = "view";
      document.getElementById("main-content")!.appendChild(searchView);
    }

    // Clear and populate search view
    searchView.innerHTML = "";
    searchView.classList.add("active");

    // Create search results header
    const header = document.createElement("div");
    header.className = "search-results-header";
    header.innerHTML = `
      <h2>Search results for "${query}"</h2>
      <span class="search-results-count">${results.total_count} games found</span>
    `;
    searchView.appendChild(header);

    // Create games grid
    const grid = document.createElement("div");
    grid.className = "games-grid";
    grid.id = "search-results-grid";
    searchView.appendChild(grid);

    // Store results in cache for showGameDetail
    searchResultsCache = results.games;

    // Render games
    results.games.forEach((game) => {
      grid.appendChild(createGameCard(game));
    });

  } catch (error) {
    console.error("Full search failed:", error);
  }
}

async function searchGames(query: string) {
  // Keep legacy function for compatibility
  await searchGamesForSuggestions(query);
}

// Authentication
async function checkAuthStatus() {
  let providerVpcId: string | null = null;

  try {
    const status = await invoke<AuthState>("get_auth_status");
    isAuthenticated = status.is_authenticated;
    currentUser = status.user || null;

    // Restore the login provider from saved auth state
    if (status.provider) {
      selectedLoginProvider = status.provider;
      console.log("Restored login provider:", status.provider.loginProviderDisplayName);
      
      // Also set it in the backend memory (in case it wasn't restored there)
      await invoke("set_login_provider", { provider: status.provider });
      
      // Fetch server info for the provider (discovers VPC ID and regions)
      try {
        const token = await invoke<string>("get_gfn_jwt");
        const serverInfo = await invoke<{ vpcId: string | null; regions: [string, string][]; baseUrl: string | null }>("fetch_server_info", { accessToken: token });
        console.log("Server info fetched for restored provider:", serverInfo);
        providerVpcId = serverInfo.vpcId;
      } catch (e) {
        console.warn("Failed to fetch server info for restored provider:", e);
      }
    }

    // Fetch real subscription tier from API if authenticated
    if (isAuthenticated && currentUser) {
      try {
        const token = await invoke<string>("get_gfn_jwt");
        const subscription = await invoke<SubscriptionInfo>("fetch_subscription", {
          accessToken: token,
          userId: currentUser.user_id,
          vpcId: providerVpcId,
        });
        // Store subscription and update user's membership tier
        currentSubscription = subscription;
        currentUser.membership_tier = subscription.membershipTier;
        console.log("Subscription:", subscription);

        // Populate resolution and FPS dropdowns from subscription data
        populateStreamingOptions(subscription);

        // Update status bar indicators
        console.log("Subscription addons:", subscription.addons);
        updateNavbarStorageIndicator(subscription);
        updateStatusBarSessionTime(subscription);

        // Show queue times link for free tier users
        updateQueueTimesLinkVisibility(subscription);
      } catch (subError) {
        console.warn("Failed to fetch subscription, using default tier:", subError);
        currentSubscription = null;
        // Use default streaming options
        populateStreamingOptions(null);
        // Show queue times link (assume free tier on error)
        updateQueueTimesLinkVisibility(null);
      }
    } else {
      // Not authenticated - use default streaming options
      populateStreamingOptions(null);
      // Hide queue times link when not authenticated
      updateQueueTimesLinkVisibility(null);
    }

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
    const userTier = document.getElementById("user-tier");
    if (userTier && currentUser.membership_tier) {
      const tier = currentUser.membership_tier.toUpperCase();
      userTier.textContent = tier;
      userTier.className = `user-tier tier-${tier.toLowerCase()}`;
    }
    // Hide user-time from top bar (now shown in status bar)
    const userTime = document.getElementById("user-time");
    if (userTime) {
      userTime.style.display = "none";
    }
  } else {
    loginBtn.classList.remove("hidden");
    userMenu.classList.add("hidden");
  }
}

// Cached login providers
let cachedLoginProviders: LoginProvider[] = [];
let selectedLoginProvider: LoginProvider | null = null;

// Fetch and populate login providers dropdown
async function fetchAndPopulateLoginProviders(): Promise<void> {
  try {
    console.log("Fetching login providers...");
    const providers = await invoke<LoginProvider[]>("fetch_login_providers");
    cachedLoginProviders = providers;
    console.log(`Fetched ${providers.length} login providers:`, providers.map(p => p.loginProviderDisplayName));

    // Build dropdown options
    const options = providers.map(provider => ({
      value: provider.loginProviderCode,
      text: provider.loginProviderDisplayName === "NVIDIA"
        ? "NVIDIA (Global)"
        : provider.loginProviderDisplayName,
      selected: provider.loginProviderCode === "NVIDIA"
    }));

    console.log("Setting dropdown options:", options);
    setDropdownOptions("login-provider", options);

    // Set default provider (NVIDIA) and update button text
    const nvidiaProvider = providers.find(p => p.loginProviderCode === "NVIDIA");
    if (nvidiaProvider) {
      selectedLoginProvider = nvidiaProvider;
      updateLoginButtonText(nvidiaProvider.loginProviderDisplayName);
      console.log("Default provider set to:", nvidiaProvider.loginProviderDisplayName);
    } else if (providers.length > 0) {
      // Fallback to first provider if NVIDIA not found
      selectedLoginProvider = providers[0];
      updateLoginButtonText(providers[0].loginProviderDisplayName);
      console.log("Fallback provider set to:", providers[0].loginProviderDisplayName);
    }
  } catch (error) {
    console.error("Failed to fetch login providers:", error);
    // Keep default NVIDIA option and set button text
    updateLoginButtonText("NVIDIA");
  }
}

// Update login button text based on selected provider
function updateLoginButtonText(providerName: string): void {
  const loginBtnText = document.getElementById("login-btn-text");
  if (loginBtnText) {
    loginBtnText.textContent = `Sign in with ${providerName}`;
  }
}

// Setup login modal handlers
function setupLoginModal() {
  const loginModal = document.getElementById("login-modal");
  const nvidiaLoginBtn = document.getElementById("nvidia-login-btn");
  const tokenLoginBtn = document.getElementById("token-login-btn");
  const tokenEntry = document.getElementById("token-entry");
  const loginOptions = loginModal?.querySelector(".login-options");
  const submitTokenBtn = document.getElementById("submit-token-btn");
  const tokenInput = document.getElementById("token-input") as HTMLTextAreaElement;

  // Handle provider dropdown change
  onDropdownChange("login-provider", async (value, text) => {
    console.log(`Login provider changed to: ${value} (${text})`);
    const provider = cachedLoginProviders.find(p => p.loginProviderCode === value);
    if (provider) {
      selectedLoginProvider = provider;
      await invoke("set_login_provider", { provider });
      updateLoginButtonText(provider.loginProviderDisplayName);
    }
  });

  // OAuth login with selected provider
  nvidiaLoginBtn?.addEventListener("click", async () => {
    const providerName = selectedLoginProvider?.loginProviderDisplayName || "NVIDIA";
    console.log(`Starting OAuth login with provider: ${providerName}...`);

    const loginBtnText = document.getElementById("login-btn-text");
    if (loginBtnText) loginBtnText.textContent = "Signing in...";
    (nvidiaLoginBtn as HTMLButtonElement).disabled = true;

    try {
      const result = await invoke<AuthState>("login_oauth");
      if (result.is_authenticated) {
        isAuthenticated = true;
        currentUser = result.user || null;
        hideAllModals();
        console.log("OAuth login successful");
        
        // Fetch server info for the selected provider (discovers VPC ID and regions)
        try {
          const token = await invoke<string>("get_gfn_jwt");
          console.log("Fetching server info for provider...");
          const serverInfo = await invoke<{ vpcId: string | null; regions: [string, string][]; baseUrl: string | null }>("fetch_server_info", { accessToken: token });
          console.log("Server info fetched:", serverInfo);
          if (serverInfo.vpcId) {
            console.log(`Using VPC ID: ${serverInfo.vpcId}`);
          }
          if (serverInfo.regions.length > 0) {
            console.log(`Provider has ${serverInfo.regions.length} regions:`, serverInfo.regions.map(r => r[0]));
          }
        } catch (serverInfoError) {
          console.warn("Failed to fetch server info (will use defaults):", serverInfoError);
        }
        
        // Refresh subscription info and reload games
        await checkAuthStatus();
        await loadHomeData();
        // Re-run latency test with provider-specific servers
        testLatency().catch(err => console.error("Latency test after login failed:", err));
        // Start session polling
        startSessionPolling();
      }
    } catch (error) {
      console.error("OAuth login failed:", error);
      alert("Login failed: " + error);
    } finally {
      updateLoginButtonText(providerName);
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
        hideAllModals();
        // Reset form
        if (tokenInput) tokenInput.value = "";
        if (loginOptions) (loginOptions as HTMLElement).classList.remove("hidden");
        if (tokenEntry) tokenEntry.classList.add("hidden");
        console.log("Token login successful");
        
        // Fetch server info for the selected provider (discovers VPC ID and regions)
        try {
          const jwtToken = await invoke<string>("get_gfn_jwt");
          console.log("Fetching server info for provider...");
          const serverInfo = await invoke<{ vpcId: string | null; regions: [string, string][]; baseUrl: string | null }>("fetch_server_info", { accessToken: jwtToken });
          console.log("Server info fetched:", serverInfo);
        } catch (serverInfoError) {
          console.warn("Failed to fetch server info (will use defaults):", serverInfoError);
        }
        
        // Refresh subscription info and reload games
        await checkAuthStatus();
        await loadHomeData();
        // Re-run latency test with provider-specific servers
        testLatency().catch(err => console.error("Latency test after login failed:", err));
        // Start session polling
        startSessionPolling();
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

  // Fetch providers when login button is clicked (to show modal)
  loginBtn?.addEventListener("click", async () => {
    showModal("login-modal");
    // Fetch providers if not already cached
    if (cachedLoginProviders.length === 0) {
      await fetchAndPopulateLoginProviders();
    }
    // Reinitialize Lucide icons for the modal
    if (typeof lucide !== 'undefined') {
      lucide.createIcons();
    }
  });
}

// Data Loading
async function loadHomeData() {
  console.log("Loading home data...");

  // Show login prompt if not authenticated
  if (!isAuthenticated) {
    const featuredGames = document.getElementById("featured-games");
    const recentGames = document.getElementById("recent-games");
    const freeGames = document.getElementById("free-games");

    const loginPrompt = `
      <div class="login-prompt">
        <i data-lucide="log-in" class="login-prompt-icon"></i>
        <p>Please sign in to browse games</p>
        <button class="btn btn-primary" onclick="document.getElementById('login-btn')?.click()">Sign In</button>
      </div>
    `;

    if (featuredGames) featuredGames.innerHTML = loginPrompt;
    if (recentGames) recentGames.innerHTML = '';
    if (freeGames) freeGames.innerHTML = '';

    // Hide the other sections when not logged in
    const sections = document.querySelectorAll('#home-view .content-section');
    sections.forEach((section, index) => {
      if (index > 0) (section as HTMLElement).style.display = 'none';
    });

    // Reinitialize Lucide icons
    if (typeof lucide !== 'undefined') {
      lucide.createIcons();
    }
    return;
  }

  // Show all sections when logged in
  const sections = document.querySelectorAll('#home-view .content-section');
  sections.forEach(section => {
    (section as HTMLElement).style.display = '';
  });

  // Show loading spinners initially
  showGridLoading("featured-games");
  showGridLoading("recent-games");
  showGridLoading("free-games");

  // Try to load library data (requires authentication)
  if (isAuthenticated) {
    console.log("User is authenticated, trying fetch_main_games...");
    try {
      const accessToken = await invoke<string>("get_gfn_jwt");
      console.log("Got GFN JWT token, calling fetch_main_games...");
      const response = await invoke<{ games: Game[] }>("fetch_main_games", {
        accessToken,
        vpcId: null, // Use default (Amsterdam)
      });
      console.log("fetch_main_games response:", response);
      if (response.games.length > 0) {
        games = response.games;
        console.log("Loaded", games.length, "games from main panel with images");
        console.log("First game:", games[0]);
        renderGamesGrid("featured-games", games.slice(0, 6));
        renderGamesGrid("recent-games", games.slice(6, 12));
        renderGamesGrid("free-games", games.slice(12, 18));
      } else {
        console.log("Main games returned 0 games, trying fetch_library...");
        throw new Error("Empty main games");
      }
    } catch (error) {
      console.error("Failed to load main games:", error);
      // Fall back to library
      console.log("Falling back to fetch_library...");
      try {
        const accessToken = await invoke<string>("get_gfn_jwt").catch(() => null);
        const response = await invoke<{ games: Game[] }>("fetch_library", {
          accessToken,
          vpcId: null,
        });
        console.log("fetch_library response:", response);
        if (response.games.length > 0) {
          games = response.games;
          console.log("Loaded", games.length, "games from library");
          console.log("First game:", games[0]);
          renderGamesGrid("featured-games", games.slice(0, 6));
          renderGamesGrid("recent-games", games.slice(6, 12));
          renderGamesGrid("free-games", games.slice(12, 18));
        }
      } catch (e) {
        console.error("Failed to load library:", e);
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
  }
}

async function loadLibraryData() {
  console.log("Loading library data...");

  // Show loading spinners while loading
  showGridLoading("recently-played");
  showGridLoading("my-games");

  try {
    const accessToken = await invoke<string>("get_gfn_jwt");
    console.log("Got GFN JWT token, calling fetch_library for library view...");
    const response = await invoke<{ games: Game[] }>("fetch_library", {
      accessToken,
      vpcId: null,
    });
    console.log("fetch_library response for library view:", response);

    if (response.games.length > 0) {
      const libraryGames = response.games;
      console.log("Loaded", libraryGames.length, "games for library view");

      // Recently played: show first 6 games (API returns most recent first)
      renderGamesGrid("recently-played", libraryGames.slice(0, 6));

      // My games: show all library games
      renderGamesGrid("my-games", libraryGames);
    } else {
      console.log("Library returned 0 games");
      // Clear placeholders if no games
      renderGamesGrid("recently-played", []);
      renderGamesGrid("my-games", []);
    }
  } catch (error) {
    console.error("Failed to load library data:", error);
  }
}

async function loadStoreData() {
  console.log("Loading store data...");
  showGridLoading("all-games");
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

function getStoreIcon(storeType: string): string {
  const iconMap: Record<string, string> = {
    steam: "cloud",
    epic: "gamepad-2",
    ubisoft: "shield",
    gog: "disc",
    ea: "zap",
    origin: "zap",
  };
  return iconMap[storeType] || "store";
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

// Show loading spinner in a grid container
function showGridLoading(containerId: string) {
  const container = document.getElementById(containerId);
  if (!container) return;

  container.replaceChildren();

  const loadingDiv = document.createElement("div");
  loadingDiv.className = "grid-loading";

  const spinner = document.createElement("div");
  spinner.className = "grid-loading-spinner";

  const text = document.createElement("span");
  text.textContent = "Loading...";

  loadingDiv.appendChild(spinner);
  loadingDiv.appendChild(text);
  container.appendChild(loadingDiv);
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
  hero.style.backgroundImage = `linear-gradient(to bottom, transparent 0%, rgba(26,26,46,0.7) 50%, rgba(26,26,46,1) 100%), url('${game.images.hero || game.images.box_art || ""}')`;

  // Content container (side by side: box art + info)
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

  // Store badge with icon
  const storeBadge = document.createElement("span");
  const storeType = game.store.store_type.toLowerCase();
  storeBadge.className = `store-badge store-${storeType}`;
  storeBadge.innerHTML = `<i data-lucide="${getStoreIcon(storeType)}"></i>${game.store.store_type}`;
  meta.appendChild(storeBadge);

  // Status indicator with icon
  if (game.status) {
    const statusBadge = document.createElement("span");
    statusBadge.className = `status-badge status-${game.status.toLowerCase()}`;
    const statusIcon = game.status === "Available" ? "circle-check" : "clock";
    const statusText = game.status === "Available" ? "Ready to Play" : game.status;
    statusBadge.innerHTML = `<i data-lucide="${statusIcon}"></i>${statusText}`;
    meta.appendChild(statusBadge);
  }

  info.appendChild(titleEl);
  info.appendChild(meta);

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

  // Controls supported with icons (deduplicated)
  if (game.supported_controls && game.supported_controls.length > 0) {
    const controls = document.createElement("div");
    controls.className = "game-detail-controls";

    const controlsLabel = document.createElement("span");
    controlsLabel.className = "controls-label";
    controlsLabel.textContent = "Controls";
    controls.appendChild(controlsLabel);

    const controlIcons = document.createElement("div");
    controlIcons.className = "control-icons";

    // Deduplicate controls
    const controlsLower = game.supported_controls.map(c => c.toLowerCase());
    const hasKeyboard = controlsLower.some(c => c.includes("keyboard") || c.includes("mouse"));
    const hasGamepad = controlsLower.some(c => c.includes("gamepad") || c.includes("controller"));
    const hasTouch = controlsLower.some(c => c.includes("touch"));

    if (hasKeyboard) {
      const icon = document.createElement("span");
      icon.className = "control-icon";
      icon.innerHTML = `<i data-lucide="keyboard"></i>Keyboard & Mouse`;
      controlIcons.appendChild(icon);
    }
    if (hasGamepad) {
      const icon = document.createElement("span");
      icon.className = "control-icon";
      icon.innerHTML = `<i data-lucide="gamepad-2"></i>Controller`;
      controlIcons.appendChild(icon);
    }
    if (hasTouch) {
      const icon = document.createElement("span");
      icon.className = "control-icon";
      icon.innerHTML = `<i data-lucide="hand"></i>Touch`;
      controlIcons.appendChild(icon);
    }

    controls.appendChild(controlIcons);
    info.appendChild(controls);
  }

  const desc = document.createElement("div");
  desc.className = "game-detail-description";
  desc.textContent = "Experience this game through GeForce NOW cloud gaming. Stream instantly without downloads.";
  info.appendChild(desc);

  // Actions
  const actions = document.createElement("div");
  actions.className = "game-detail-actions";

  // Track selected variant
  let selectedVariantId = game.id;

  // Store selector if multiple variants
  if (game.variants && game.variants.length > 1) {
    const storeSelector = document.createElement("div");
    storeSelector.className = "store-selector";

    const selectorLabel = document.createElement("span");
    selectorLabel.className = "store-selector-label";
    selectorLabel.textContent = "Play on:";
    storeSelector.appendChild(selectorLabel);

    const selectorBtns = document.createElement("div");
    selectorBtns.className = "store-selector-buttons";

    game.variants.forEach((variant, index) => {
      const btn = document.createElement("button");
      btn.className = `store-selector-btn${index === 0 ? " active" : ""}`;
      btn.dataset.variantId = variant.id;
      btn.textContent = variant.store_type;
      btn.addEventListener("click", () => {
        selectorBtns.querySelectorAll(".store-selector-btn").forEach(b => b.classList.remove("active"));
        btn.classList.add("active");
        selectedVariantId = variant.id;
      });
      selectorBtns.appendChild(btn);
    });

    storeSelector.appendChild(selectorBtns);
    info.appendChild(storeSelector);
  }

  const playBtn = document.createElement("button");
  playBtn.className = "btn btn-primary btn-large";
  playBtn.innerHTML = `<i data-lucide="play"></i> Play Now`;
  playBtn.addEventListener("click", () => {
    // Use selected variant ID
    const gameToLaunch = { ...game, id: selectedVariantId };
    launchGame(gameToLaunch);
  });

  const favBtn = document.createElement("button");
  favBtn.className = "btn btn-secondary";
  favBtn.innerHTML = `<i data-lucide="heart"></i> Add to Library`;
  favBtn.addEventListener("click", async () => {
    favBtn.innerHTML = `<i data-lucide="heart"></i> Added`;
    favBtn.classList.add("favorited");
    lucide.createIcons();
  });

  const storeBtn = document.createElement("button");
  storeBtn.className = "btn btn-secondary";
  storeBtn.innerHTML = `<i data-lucide="external-link"></i> View on ${game.store.store_type}`;
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
  const game = games.find((g) => g.id === gameId)
    || searchResultsCache.find((g) => g.id === gameId)
    || createPlaceholderGames().find((g) => g.id === gameId);
  if (!game) return;

  const detailContainer = document.getElementById("game-detail");
  if (!detailContainer) return;

  // Clear and append new content safely
  detailContainer.replaceChildren();
  detailContainer.appendChild(createGameDetailElement(game));

  // Render Lucide icons in the new content
  lucide.createIcons();

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
  region: string | null;
  inputCleanup: (() => void) | null;
  statsInterval: number | null;
  escCleanup: (() => void) | null;
  lastDiscordUpdate: number;
  gameStartTime: number;
}

let streamingUIState: StreamingUIState = {
  active: false,
  sessionId: null,
  gameName: null,
  phase: "idle",
  gpuType: null,
  serverIp: null,
  region: null,
  inputCleanup: null,
  statsInterval: null,
  escCleanup: null,
  lastDiscordUpdate: 0,
  gameStartTime: 0,
};

async function launchGame(game: Game) {
  console.log("Launching game:", game.title);
  hideAllModals();

  // Stop session polling while we're launching/streaming
  stopSessionPolling();

  // Get the GFN JWT token first (required for API authentication)
  let accessToken: string;
  try {
    accessToken = await invoke<string>("get_gfn_jwt");
  } catch (e) {
    console.error("Not authenticated:", e);
    alert("Please login first to launch games.");
    startSessionPolling(); // Resume polling since we're not launching
    return;
  }

  // Check for active sessions before launching
  const activeSessions = await checkActiveSessions();
  if (activeSessions.length > 0) {
    // Show the conflict modal instead of launching
    showSessionConflictModal(activeSessions[0], game);
    startSessionPolling(); // Resume polling since we're not launching
    return;
  }

  // For free tier users on NVIDIA servers, show server selection modal with queue times
  // Skip for Alliance Partners as they have their own queue system
  if (isFreeTier(currentSubscription) && !isAlliancePartner()) {
    const selectedServer = await showQueueSelectionModal(game);
    if (selectedServer === null && selectedQueueServer === null) {
      // User cancelled
      startSessionPolling();
      return;
    }
    // If user selected a server, selectedQueueServer is already set
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

    const streamParams = getStreamingParams();
    console.log("Using streaming params:", streamParams, "resolution:", currentResolution, "fps:", currentFps);

    // Get preferred server based on region setting
    const preferredServer = getPreferredServerForSession();
    console.log("Using preferred server:", preferredServer || "default");

    const sessionResult = await invoke<{
      sessionId: string;
      signalingUrl: string | null;
      server: { ip: string; id: string };
    }>("start_session", {
      request: {
        game_id: game.id,
        store_type: game.store.store_type,
        store_id: game.store.store_id,
        preferred_server: preferredServer,
        quality_preset: currentQuality,
        resolution: streamParams.resolution,
        fps: streamParams.fps,
        codec: currentCodec,
        max_bitrate_mbps: currentMaxBitrate,
        reflex: reflexEnabled, // NVIDIA Reflex low-latency mode
      },
      accessToken: accessToken,
    });

    console.log("Session created:", sessionResult);
    streamingUIState.sessionId = sessionResult.sessionId;
    streamingUIState.gameName = game.title;
    streamingUIState.active = true;

    // Phase 2: Poll until ready and start streaming
    updateStreamingStatus("Waiting for server...");

    console.log("Calling start_streaming_flow for session:", sessionResult.sessionId);
    let streamingResult;
    try {
      streamingResult = await invoke<{
        sessionId: string;
        phase: string;
        serverIp: string | null;
        signalingUrl: string | null;
        gpuType: string | null;
        connectionInfo: {
          controlIp: string;
          controlPort: number;
          streamIp: string | null;
          streamPort: number;
          resourcePath: string;
        } | null;
        error: string | null;
      }>("start_streaming_flow", {
        sessionId: sessionResult.sessionId,
        accessToken: accessToken,
      });
    } catch (e) {
      console.error("start_streaming_flow failed:", e);
      throw e;
    }

    console.log("Streaming ready:", streamingResult);
    console.log("  - sessionId:", streamingResult.sessionId);
    console.log("  - phase:", streamingResult.phase);
    console.log("  - serverIp:", streamingResult.serverIp);
    console.log("  - signalingUrl:", streamingResult.signalingUrl);
    console.log("  - connectionInfo:", streamingResult.connectionInfo);
    console.log("  - gpuType:", streamingResult.gpuType);
    streamingUIState.phase = streamingResult.phase;
    streamingUIState.gpuType = streamingResult.gpuType;
    streamingUIState.serverIp = streamingResult.serverIp;

    // Determine the region name for display
    const currentServer = cachedServers.find(s => s.id === currentRegion) ||
      (currentRegion === "auto" ? cachedServers.find(s => s.status === "Online") : null);
    streamingUIState.region = currentServer?.name || currentRegion;

    // Update overlay with success
    updateStreamingStatus(`Connected to ${streamingResult.gpuType || "GPU"}`);

    // Update Discord presence to show playing (if enabled)
    if (discordRpcEnabled) {
      try {
        // Store start time in seconds for Discord elapsed time
        streamingUIState.gameStartTime = Math.floor(Date.now() / 1000);
        await invoke("set_game_presence", {
          gameName: game.title,
          region: streamingUIState.region,
          resolution: discordShowStats ? currentResolution : null,
          fps: discordShowStats ? currentFps : null,
          latencyMs: null,
        });
        streamingUIState.lastDiscordUpdate = Date.now();
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
      // Initialize WebRTC streaming with user's selected resolution/fps
      const streamingOptions: StreamingOptions = {
        resolution: currentResolution,
        fps: currentFps
      };
      await initializeStreaming(streamingResult, accessToken, streamContainer, streamingOptions);

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

            // Update Discord presence every 15 seconds with current stats
            if (discordRpcEnabled && streamingUIState.gameName) {
              const now = Date.now();
              if (now - streamingUIState.lastDiscordUpdate >= 15000) {
                try {
                  await invoke("update_game_stats", {
                    gameName: streamingUIState.gameName,
                    region: streamingUIState.region,
                    resolution: discordShowStats ? (stats.resolution || currentResolution) : null,
                    fps: discordShowStats ? (stats.fps || null) : null,
                    latencyMs: discordShowStats ? (stats.latency_ms || null) : null,
                    startTime: streamingUIState.gameStartTime,
                  });
                  streamingUIState.lastDiscordUpdate = now;
                } catch (e) {
                  // Silently ignore Discord update failures
                }
              }
            }
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

    // Resume session polling since launch failed
    startSessionPolling();

    // Check for specific errors and show appropriate modals
    const errorStr = String(error);
    if (errorStr.includes("REGION_NOT_SUPPORTED")) {
      showRegionErrorModal(errorStr, game);
    } else if (errorStr.includes("SESSION_LIMIT")) {
      showSessionLimitModal(errorStr, game);
    } else {
      alert(`Failed to launch game: ${error}`);
    }
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
          <button class="stream-btn" id="stream-fullscreen-btn" title="Fullscreen"><i data-lucide="maximize"></i></button>
          <button class="stream-btn" id="stream-settings-btn" title="Settings"><i data-lucide="settings"></i></button>
          <button class="stream-btn stream-btn-danger" id="stream-exit-btn" title="Exit"><i data-lucide="x"></i></button>
        </div>
      </div>
    </div>
    <div class="stream-stats" id="stream-stats">
      <span id="stats-region">Region: --</span>
      <span id="stats-fps">-- FPS</span>
      <span id="stats-latency">-- ms</span>
      <span id="stats-resolution">----x----</span>
      <span id="stats-codec">----</span>
      <span id="stats-bitrate">-- Mbps</span>
    </div>
    <div class="stream-exit-overlay" id="stream-exit-overlay">
      <div class="exit-overlay-content">
        <svg class="exit-progress-ring" viewBox="0 0 100 100">
          <circle class="exit-progress-bg" cx="50" cy="50" r="45" />
          <circle class="exit-progress-bar" cx="50" cy="50" r="45" />
        </svg>
        <span class="exit-overlay-text">Hold ESC to exit</span>
      </div>
    </div>
    <div class="stream-settings-panel" id="stream-settings-panel">
      <div class="settings-panel-header">
        <span>Stream Settings</span>
        <button class="settings-close-btn" id="settings-close-btn"><i data-lucide="x"></i></button>
      </div>
      <div class="settings-panel-content">
        <div class="settings-section">
          <h4>Stream Info</h4>
          <div class="settings-info-grid">
            <div class="info-item">
              <span class="info-label">Region</span>
              <span class="info-value" id="info-region">--</span>
            </div>
            <div class="info-item">
              <span class="info-label">GPU</span>
              <span class="info-value" id="info-gpu">--</span>
            </div>
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
      display: flex;
      align-items: center;
      justify-content: center;
      background: rgba(255,255,255,0.1);
      border: none;
      color: white;
      width: 36px;
      height: 36px;
      border-radius: 6px;
      cursor: pointer;
      transition: background 0.2s;
    }
    .stream-btn svg {
      width: 18px;
      height: 18px;
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
      z-index: 10003;
    }
    #streaming-container:fullscreen .stream-stats,
    #streaming-container:-webkit-full-screen .stream-stats {
      position: fixed;
      bottom: 20px;
      left: 20px;
    }
    .stream-stats span {
      font-family: monospace;
    }
    /* Latency color coding for stats */
    .stream-stats .latency-excellent,
    .info-value.latency-excellent { color: #00c853 !important; }
    .stream-stats .latency-good,
    .info-value.latency-good { color: #76b900 !important; }
    .stream-stats .latency-fair,
    .info-value.latency-fair { color: #ffc107 !important; }
    .stream-stats .latency-poor,
    .info-value.latency-poor { color: #ff9800 !important; }
    .stream-stats .latency-bad,
    .info-value.latency-bad { color: #f44336 !important; }
    #stats-region {
      color: #76b900;
      font-weight: 500;
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
    /* Hide top bar (header with game name and buttons) in fullscreen mode */
    #streaming-container:fullscreen .stream-header,
    #streaming-container:-webkit-full-screen .stream-header,
    #streaming-container:-moz-full-screen .stream-header,
    #streaming-container:-ms-fullscreen .stream-header,
    #streaming-container.is-fullscreen .stream-header {
      display: none !important;
    }
    /* Also hide settings panel in fullscreen mode */
    #streaming-container:fullscreen .stream-settings-panel,
    #streaming-container:-webkit-full-screen .stream-settings-panel,
    #streaming-container:-moz-full-screen .stream-settings-panel,
    #streaming-container:-ms-fullscreen .stream-settings-panel,
    #streaming-container.is-fullscreen .stream-settings-panel {
      display: none !important;
    }
    /* ESC exit overlay styles */
    .stream-exit-overlay {
      position: fixed;
      top: 0;
      left: 0;
      width: 100%;
      height: 100%;
      display: flex;
      align-items: center;
      justify-content: center;
      background: rgba(0, 0, 0, 0.7);
      opacity: 0;
      visibility: hidden;
      transition: opacity 0.15s ease, visibility 0.15s ease;
      z-index: 10000;
      pointer-events: none;
    }
    .stream-exit-overlay.active {
      opacity: 1;
      visibility: visible;
    }
    .exit-overlay-content {
      display: flex;
      flex-direction: column;
      align-items: center;
      gap: 16px;
    }
    .exit-progress-ring {
      width: 100px;
      height: 100px;
      transform: rotate(-90deg);
    }
    .exit-progress-bg {
      fill: none;
      stroke: rgba(255, 255, 255, 0.2);
      stroke-width: 6;
    }
    .exit-progress-bar {
      fill: none;
      stroke: #76b900;
      stroke-width: 6;
      stroke-linecap: round;
      stroke-dasharray: 283;
      stroke-dashoffset: 283;
      transition: stroke-dashoffset 1s linear;
    }
    .stream-exit-overlay.active .exit-progress-bar {
      stroke-dashoffset: 0;
    }
    .exit-overlay-text {
      color: #fff;
      font-size: 16px;
      font-weight: 500;
      text-shadow: 0 2px 4px rgba(0, 0, 0, 0.5);
    }
  `;

  document.head.appendChild(style);
  document.body.appendChild(container);

  // Reinitialize Lucide icons for dynamically added content
  if (typeof lucide !== 'undefined') {
    lucide.createIcons();
  }

  // Find the video wrapper to return
  const videoWrapper = container.querySelector(".stream-video-wrapper") as HTMLElement;

  // Set up button handlers
  document.getElementById("stream-exit-btn")?.addEventListener("click", () => {
    exitStreaming();
  });

  document.getElementById("stream-fullscreen-btn")?.addEventListener("click", async () => {
    console.log("Fullscreen button clicked");

    // Try Tauri's window API first (works properly on macOS)
    let tauriSuccess = false;
    let enteringFullscreen = false;
    try {
      const appWindow = getCurrentWindow();
      console.log("Got Tauri window:", appWindow);
      const isFullscreen = await appWindow.isFullscreen();
      console.log("Current fullscreen state:", isFullscreen);
      enteringFullscreen = !isFullscreen;
      await appWindow.setFullscreen(enteringFullscreen);
      console.log("Fullscreen toggled to:", enteringFullscreen);
      tauriSuccess = true;

      // Toggle is-fullscreen class on container for CSS rules
      const streamContainer = document.getElementById("streaming-container");
      if (streamContainer) {
        if (enteringFullscreen) {
          streamContainer.classList.add("is-fullscreen");
        } else {
          streamContainer.classList.remove("is-fullscreen");
        }
      }

      // Manually handle cursor hiding since browser fullscreenchange event won't fire for Tauri fullscreen
      const video = document.getElementById("gfn-stream-video") as HTMLVideoElement;
      if (enteringFullscreen) {
        // Entering fullscreen - hide cursor, use absolute mode
        console.log("Entering fullscreen - hiding cursor");
        await setInputCaptureMode('pointerlock'); // This just hides cursor now
        
        // Also request browser fullscreen on the streaming container for proper fullscreen behavior
        const container = document.getElementById("streaming-container");
        if (container && !document.fullscreenElement) {
          try {
            await container.requestFullscreen();
            console.log("Browser fullscreen requested on container");
          } catch (e) {
            console.warn("Browser fullscreen failed:", e);
          }
        }
      } else {
        // Exiting fullscreen - show cursor
        console.log("Exiting fullscreen - showing cursor");
        await setInputCaptureMode('absolute');
        
        // Exit browser fullscreen if active
        if (document.fullscreenElement) {
          try {
            await document.exitFullscreen();
            console.log("Browser fullscreen exited");
          } catch (e) {
            console.warn("Failed to exit browser fullscreen:", e);
          }
        }
      }
    } catch (e) {
      console.error("Tauri fullscreen API error:", e);
    }

    // If Tauri failed, try browser API
    if (!tauriSuccess) {
      console.log("Falling back to browser fullscreen API");
      const fullscreenElement = document.fullscreenElement ||
        (document as any).webkitFullscreenElement ||
        (document as any).mozFullScreenElement ||
        (document as any).msFullscreenElement;

      if (fullscreenElement) {
        console.log("Exiting fullscreen via browser API");
        if (document.exitFullscreen) {
          document.exitFullscreen().catch(err => console.error("exitFullscreen error:", err));
        } else if ((document as any).webkitExitFullscreen) {
          (document as any).webkitExitFullscreen();
        } else if ((document as any).mozCancelFullScreen) {
          (document as any).mozCancelFullScreen();
        } else if ((document as any).msExitFullscreen) {
          (document as any).msExitFullscreen();
        }
      } else {
        console.log("Entering fullscreen via browser API on container:", container);
        try {
          if (container.requestFullscreen) {
            await container.requestFullscreen();
          } else if ((container as any).webkitRequestFullscreen) {
            (container as any).webkitRequestFullscreen();
          } else if ((container as any).mozRequestFullScreen) {
            (container as any).mozRequestFullScreen();
          } else if ((container as any).msRequestFullscreen) {
            (container as any).msRequestFullscreen();
          }
        } catch (err) {
          console.error("Browser fullscreen error:", err);
        }
      }
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
  let tauriFullscreenState = false; // Track Tauri fullscreen state

  // Helper to check if in fullscreen (cross-browser)
  const isBrowserFullscreen = () => document.fullscreenElement ||
    (document as any).webkitFullscreenElement ||
    (document as any).mozFullScreenElement ||
    (document as any).msFullscreenElement;

  // Helper to exit fullscreen using Tauri API (macOS) with browser fallback
  const exitFullscreenAsync = async () => {
    let exitedViaTauri = false;
    try {
      const appWindow = getCurrentWindow();
      const isFullscreen = await appWindow.isFullscreen();
      if (isFullscreen) {
        await appWindow.setFullscreen(false);
        exitedViaTauri = true;
      }
    } catch (e) {
      // Fall through to browser API
    }

    // Browser API fallback
    if (!exitedViaTauri) {
      if (document.exitFullscreen) {
        document.exitFullscreen().catch(() => {});
      } else if ((document as any).webkitExitFullscreen) {
        (document as any).webkitExitFullscreen();
      } else if ((document as any).mozCancelFullScreen) {
        (document as any).mozCancelFullScreen();
      } else if ((document as any).msExitFullscreen) {
        (document as any).msExitFullscreen();
      }
    }

    // Switch back to absolute mode and exit pointer lock
    console.log("ESC exit: Switching to absolute mode");
    await setInputCaptureMode('absolute');
    if (document.pointerLockElement) {
      document.exitPointerLock();
    }
  };

  // Periodically check Tauri fullscreen state for ESC handler
  const updateTauriFullscreenState = async () => {
    try {
      const appWindow = getCurrentWindow();
      tauriFullscreenState = await appWindow.isFullscreen();
    } catch {
      tauriFullscreenState = false;
    }
  };
  const fullscreenCheckInterval = setInterval(updateTauriFullscreenState, 500);

  const escKeyDownHandler = (e: KeyboardEvent) => {
    const isFullscreen = isBrowserFullscreen() || tauriFullscreenState;
    if (e.key === "Escape" && isFullscreen) {
      // Prevent browser's default behavior of exiting fullscreen on ESC
      e.preventDefault();

      // Only start the hold timer if not already started
      if (escHoldStart === 0) {
        escHoldStart = Date.now();

        // Show the exit overlay with animation
        const exitOverlay = document.getElementById("stream-exit-overlay");
        if (exitOverlay) {
          exitOverlay.classList.add("active");
        }

        escHoldTimer = window.setTimeout(() => {
          if (escHoldStart > 0) {
            // Hide overlay before exiting
            if (exitOverlay) {
              exitOverlay.classList.remove("active");
            }
            // Remove is-fullscreen class
            const streamContainer = document.getElementById("streaming-container");
            if (streamContainer) {
              streamContainer.classList.remove("is-fullscreen");
            }
            exitFullscreenAsync();
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
      // Hide the exit overlay
      const exitOverlay = document.getElementById("stream-exit-overlay");
      if (exitOverlay) {
        exitOverlay.classList.remove("active");
      }
    }
  };

  document.addEventListener("keydown", escKeyDownHandler);
  document.addEventListener("keyup", escKeyUpHandler);

  // Window focus/blur handlers for macOS cursor capture
  // When switching windows (Cmd+Tab), we need to release and recapture the cursor
  const handleWindowBlur = () => {
    console.log("Window blur - suspending cursor capture");
    suspendCursorCapture();
  };

  const handleWindowFocus = () => {
    console.log("Window focus - resuming cursor capture");
    resumeCursorCapture();
  };

  window.addEventListener("blur", handleWindowBlur);
  window.addEventListener("focus", handleWindowFocus);

  // Store cleanup for ESC handlers, fullscreen check interval, and focus handlers
  streamingUIState.escCleanup = () => {
    document.removeEventListener("keydown", escKeyDownHandler);
    document.removeEventListener("keyup", escKeyUpHandler);
    window.removeEventListener("blur", handleWindowBlur);
    window.removeEventListener("focus", handleWindowFocus);
    if (escHoldTimer) {
      clearTimeout(escHoldTimer);
    }
    clearInterval(fullscreenCheckInterval);
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
  const regionEl = document.getElementById("stats-region");
  const fpsEl = document.getElementById("stats-fps");
  const latencyEl = document.getElementById("stats-latency");
  const resEl = document.getElementById("stats-resolution");
  const codecEl = document.getElementById("stats-codec");
  const bitrateEl = document.getElementById("stats-bitrate");
  const bitrateFormatted = stats.bitrate_kbps >= 1000
    ? `${(stats.bitrate_kbps / 1000).toFixed(1)} Mbps`
    : `${stats.bitrate_kbps} kbps`;

  // Get current region info
  const currentServer = cachedServers.find(s => s.id === currentRegion) ||
    (currentRegion === "auto" ? cachedServers.find(s => s.status === "Online") : null);

  if (regionEl) {
    regionEl.textContent = currentServer ? currentServer.name : (currentRegion === "auto" ? "Auto" : currentRegion);
  }

  if (fpsEl) fpsEl.textContent = `${Math.round(stats.fps)} FPS`;

  // Color code the latency
  if (latencyEl) {
    latencyEl.textContent = `${stats.latency_ms} ms`;
    // Remove all latency classes first
    latencyEl.classList.remove("latency-excellent", "latency-good", "latency-fair", "latency-poor", "latency-bad");
    // Add appropriate class based on latency
    latencyEl.classList.add(getLatencyClass(stats.latency_ms));
  }

  if (resEl) resEl.textContent = stats.resolution || "----x----";
  if (codecEl) codecEl.textContent = stats.codec || "----";
  if (bitrateEl) bitrateEl.textContent = bitrateFormatted;

  // Update settings panel info
  const infoRegionEl = document.getElementById("info-region");
  const infoGpuEl = document.getElementById("info-gpu");
  const infoResEl = document.getElementById("info-resolution");
  const infoFpsEl = document.getElementById("info-fps");
  const infoCodecEl = document.getElementById("info-codec");
  const infoBitrateEl = document.getElementById("info-bitrate");
  const infoLatencyEl = document.getElementById("info-latency");
  const infoPacketLossEl = document.getElementById("info-packet-loss");

  if (infoRegionEl) {
    infoRegionEl.textContent = currentServer ? currentServer.name : (currentRegion === "auto" ? "Auto" : currentRegion);
  }
  if (infoGpuEl) {
    infoGpuEl.textContent = streamingUIState.gpuType || "--";
  }
  if (infoResEl) infoResEl.textContent = stats.resolution || "--";
  if (infoFpsEl) infoFpsEl.textContent = `${Math.round(stats.fps)}`;
  if (infoCodecEl) infoCodecEl.textContent = stats.codec || "--";
  if (infoBitrateEl) infoBitrateEl.textContent = bitrateFormatted;
  if (infoLatencyEl) {
    infoLatencyEl.textContent = `${stats.latency_ms} ms`;
    infoLatencyEl.classList.remove("latency-excellent", "latency-good", "latency-fair", "latency-poor", "latency-bad");
    infoLatencyEl.classList.add(getLatencyClass(stats.latency_ms));
  }
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
      const accessToken = await invoke<string>("get_gfn_jwt");
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
    region: null,
    inputCleanup: null,
    statsInterval: null,
    escCleanup: null,
    lastDiscordUpdate: 0,
    gameStartTime: 0,
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

  // Resume session polling now that we're not streaming
  startSessionPolling();
}

// Queue status polling interval
let queueStatusInterval: number | null = null;

// Queue status interface
interface QueueStatus {
  session_status: number;
  queue_position: number;
  eta_ms: number;
  is_in_queue: boolean;
}

// Start polling for queue status updates
function startQueueStatusPolling() {
  // Clear any existing interval
  if (queueStatusInterval !== null) {
    clearInterval(queueStatusInterval);
  }

  // Start countdown timer if we have an ETA (free tier users)
  if (queueStartEta > 0) {
    startQueueCountdown();
  }

  // Poll every 2 seconds for queue status
  queueStatusInterval = window.setInterval(async () => {
    try {
      const status = await invoke<QueueStatus>("get_queue_status");

      if (status.is_in_queue && status.queue_position > 0) {
        // Update the overlay to show queue position
        updateQueueDisplay(status.queue_position, status.eta_ms);
      } else if (status.session_status === 2) {
        // Session is ready, stop polling queue status
        stopQueueStatusPolling();
      }
    } catch (e) {
      // Silently ignore errors during queue polling
      console.debug("Queue status poll error:", e);
    }
  }, 2000);
}

// Stop polling for queue status
function stopQueueStatusPolling() {
  if (queueStatusInterval !== null) {
    clearInterval(queueStatusInterval);
    queueStatusInterval = null;
  }
  // Also stop the countdown timer
  stopQueueCountdown();
}

// Update the queue display in the overlay
function updateQueueDisplay(position: number, etaMs: number) {
  const statusEl = document.getElementById("streaming-status");
  const queueInfoEl = document.getElementById("queue-info");
  
  if (statusEl) {
    statusEl.textContent = `Queue position: ${position}`;
  }
  
  // Show/update the queue info section
  if (queueInfoEl) {
    queueInfoEl.style.display = "block";
    const positionEl = document.getElementById("queue-position");
    if (positionEl) {
      positionEl.textContent = String(position);
    }
  }
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
      <div id="queue-info" style="display: none;">
        <div class="queue-stats-row">
          <div class="queue-display">
            <span class="queue-label">Position in Queue</span>
            <span class="queue-position" id="queue-position">-</span>
          </div>
          <div class="queue-display">
            <span class="queue-label">Estimated Wait</span>
            <span class="queue-eta" id="queue-eta">--</span>
          </div>
        </div>
        <p class="queue-hint">Free tier users may experience longer wait times during peak hours.</p>
      </div>
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
    #queue-info {
      background: rgba(118, 185, 0, 0.1);
      border: 1px solid rgba(118, 185, 0, 0.3);
      border-radius: 12px;
      padding: 20px;
      margin-bottom: 20px;
    }
    .queue-stats-row {
      display: flex;
      justify-content: center;
      gap: 40px;
    }
    .queue-display {
      display: flex;
      flex-direction: column;
      align-items: center;
      gap: 8px;
    }
    .queue-label {
      font-size: 14px;
      color: #888;
      text-transform: uppercase;
      letter-spacing: 1px;
    }
    .queue-position {
      font-size: 48px;
      font-weight: bold;
      color: #76b900;
      line-height: 1;
    }
    .queue-eta {
      font-size: 48px;
      font-weight: bold;
      color: #76b900;
      line-height: 1;
    }
    .queue-hint {
      font-size: 12px;
      color: #666;
      margin-top: 12px;
      margin-bottom: 0;
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
  
  // Start polling for queue status
  startQueueStatusPolling();
}

function updateStreamingStatus(status: string) {
  const statusEl = document.getElementById("streaming-status");
  if (statusEl) {
    statusEl.textContent = status;
  }
}

function showStreamingInfo(info: {
  gpuType: string | null;
  serverIp: string | null;
  phase: string;
}) {
  const infoEl = document.getElementById("streaming-info");
  const gpuEl = document.getElementById("streaming-gpu");
  const serverEl = document.getElementById("streaming-server");
  const phaseEl = document.getElementById("streaming-phase");
  const queueInfoEl = document.getElementById("queue-info");

  // Hide queue info when session is ready
  if (queueInfoEl && info.phase === "Ready") {
    queueInfoEl.style.display = "none";
  }
  
  // Stop queue polling when ready
  if (info.phase === "Ready") {
    stopQueueStatusPolling();
  }

  if (infoEl) infoEl.style.display = "block";
  if (gpuEl) gpuEl.textContent = info.gpuType || "Unknown";
  if (serverEl) serverEl.textContent = info.serverIp || "Unknown";
  if (phaseEl) phaseEl.textContent = info.phase;

  // Hide spinner when ready
  const spinner = document.querySelector(".streaming-spinner") as HTMLElement;
  if (spinner && info.phase === "Ready") {
    spinner.style.borderTopColor = "#76b900";
    spinner.style.animation = "none";
    spinner.innerHTML = '<svg xmlns="http://www.w3.org/2000/svg" width="30" height="30" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="20 6 9 17 4 12"></polyline></svg>';
    spinner.style.display = "flex";
    spinner.style.alignItems = "center";
    spinner.style.justifyContent = "center";
    spinner.style.color = "#76b900";
  }
}

function hideStreamingOverlay() {
  // Stop queue status polling
  stopQueueStatusPolling();
  
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
  const bitrateEl = document.getElementById("bitrate-setting") as HTMLInputElement;
  const proxyEl = document.getElementById("proxy-setting") as HTMLInputElement;
  const telemetryEl = document.getElementById("telemetry-setting") as HTMLInputElement;
  const discordEl = document.getElementById("discord-setting") as HTMLInputElement;
  const discordStatsEl = document.getElementById("discord-stats-setting") as HTMLInputElement;
  const reflexEl = document.getElementById("reflex-setting") as HTMLInputElement;

  // Get dropdown values
  const resolution = getDropdownValue("resolution-setting") || "1920x1080";
  const fps = getDropdownValue("fps-setting") || "60";
  const codec = getDropdownValue("codec-setting") || "h264";
  const audioCodec = getDropdownValue("audio-codec-setting") || "opus";
  const region = getDropdownValue("region-setting") || "auto";

  // Update global state
  discordRpcEnabled = discordEl?.checked || false;
  discordShowStats = discordStatsEl?.checked || false;
  reflexEnabled = reflexEl?.checked ?? true;
  currentResolution = resolution;
  currentFps = parseInt(fps, 10);
  currentCodec = codec;
  currentAudioCodec = audioCodec;
  currentMaxBitrate = parseInt(bitrateEl?.value || "200", 10);
  currentRegion = region;

  // Update status bar with new region selection
  updateStatusBarLatency();

  const settings: Settings = {
    quality: "custom", // Mark as custom since we use explicit resolution/fps
    resolution: currentResolution,
    fps: currentFps,
    codec: codec,
    audio_codec: audioCodec,
    max_bitrate_mbps: currentMaxBitrate,
    region: region || undefined,
    discord_rpc: discordRpcEnabled,
    discord_show_stats: discordShowStats,
    proxy: proxyEl?.value || undefined,
    disable_telemetry: telemetryEl?.checked || true,
    reflex: reflexEnabled,
  };

  try {
    await invoke("save_settings", { settings });
    hideAllModals();
    console.log("Settings saved:", settings);
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
  // Queue times
  showQueueTimesPage,
};
