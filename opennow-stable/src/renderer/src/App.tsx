     1	import { useCallback, useEffect, useMemo, useRef, useState } from "react";
     2	import type { CSSProperties, JSX } from "react";
     3	import { createPortal } from "react-dom";
     4	
     5	import type {
     6	  ActiveSessionInfo,
     7	  AuthSession,
     8	  AuthUser,
     9	  CatalogBrowseResult,
    10	  CatalogFilterGroup,
    11	  CatalogSortOption,
    12	  ExistingSessionStrategy,
    13	  GameInfo,
    14	  GameVariant,
    15	  LoginProvider,
    16	  MainToRendererSignalingEvent,
    17	  SessionAdAction,
    18	  SessionAdInfo,
    19	  SessionAdState,
    20	  SessionInfo,
    21	  SessionStopRequest,
    22	  SavedAccount,
    23	  Settings,
    24	  SubscriptionInfo,
    25	  StreamRegion,
    26	  VideoCodec,
    27	  PrintedWasteQueueData,
    28	  PrintedWasteServerMapping,
    29	} from "@shared/gfn";
    30	import {
    31	  DEFAULT_KEYBOARD_LAYOUT,
    32	  getDefaultStreamPreferences,
    33	  USER_FACING_VIDEO_CODEC_OPTIONS,
    34	  getPreferredSessionAdMediaUrl,
    35	  getSessionAdDurationMs,
    36	  getSessionAdItems,
    37	  getSessionAdOpportunity,
    38	  isSessionAdsRequired,
    39	  isSessionQueuePaused,
    40	} from "@shared/gfn";
    41	
    42	import {
    43	  GfnWebRtcClient,
    44	  type StreamDiagnostics,
    45	  type StreamTimeWarning,
    46	} from "./gfn/webrtcClient";
    47	import { formatShortcutForDisplay, isShortcutMatch, normalizeShortcut } from "./shortcuts";
    48	import { useControllerNavigation } from "./controllerNavigation";
    49	import { useElapsedSeconds } from "./utils/useElapsedSeconds";
    50	import { usePlaytime } from "./utils/usePlaytime";
    51	import { createStreamDiagnosticsStore } from "./utils/streamDiagnosticsStore";
    52	import { loadStoredCodecResults, saveStoredCodecResults, testCodecSupport, type CodecTestResult } from "./lib/codecDiagnostics";
    53	
    54	// UI Components
    55	import { LoginScreen } from "./components/LoginScreen";
    56	import { Navbar } from "./components/Navbar";
    57	import { HomePage } from "./components/HomePage";
    58	import { LibraryPage } from "./components/LibraryPage";
    59	import { ControllerLibraryPage } from "./components/ControllerLibraryPage";
    60	import { SettingsPage } from "./components/SettingsPage";
    61	import { StreamLoading } from "./components/StreamLoading";
    62	import { ControllerStreamLoading } from "./components/ControllerStreamLoading";
    63	import type { QueueAdPlaybackEvent, QueueAdPreviewHandle } from "./components/QueueAdPreview";
    64	import { StreamView } from "./components/StreamView";
    65	import { QueueServerSelectModal } from "./components/QueueServerSelectModal";
    66	
    67	const codecOptions: VideoCodec[] = [...USER_FACING_VIDEO_CODEC_OPTIONS];
    68	const DEFAULT_STREAM_PREFERENCES = getDefaultStreamPreferences();
    69	const allResolutionOptions = ["1280x720", "1280x800", "1440x900", "1680x1050", "1920x1080", "1920x1200", "2560x1080", "2560x1440", "2560x1600", "3440x1440", "3840x2160", "3840x2400"];
    70	const fpsOptions = [30, 60, 120, 144, 240];
    71	const aspectRatioOptions = ["16:9", "16:10", "21:9", "32:9"] as const;
    72	
    73	const RESOLUTION_TO_ASPECT_RATIO: Record<string, string> = {
    74	  "1280x720": "16:9",
    75	  "1280x800": "16:10",
    76	  "1440x900": "16:10",
    77	  "1680x1050": "16:10",
    78	  "1920x1080": "16:9",
    79	  "1920x1200": "16:10",
    80	  "2560x1080": "21:9",
    81	  "2560x1440": "16:9",
    82	  "2560x1600": "16:10",
    83	  "3440x1440": "21:9",
    84	  "3840x2160": "16:9",
    85	  "3840x2400": "16:10",
    86	  "5120x1440": "32:9",
    87	};
    88	
    89	const getResolutionsByAspectRatio = (aspectRatio: string): string[] => {
    90	  return allResolutionOptions.filter(res => RESOLUTION_TO_ASPECT_RATIO[res] === aspectRatio);
    91	};
    92	const resolutionOptions = getResolutionsByAspectRatio("16:9");
    93	
    94	type AppStyle = CSSProperties & {
    95	  "--game-poster-scale"?: string;
    96	};
    97	
    98	function getAppStyle(posterSizeScale: number): AppStyle {
    99	  return {
   100	    "--game-poster-scale": String(posterSizeScale),
   101	  };
   102	}
   103	const SESSION_READY_POLL_INTERVAL_MS = 2000;
   104	const SESSION_AD_POLL_INTERVAL_MS = 30000;
   105	const SESSION_AD_PROGRESS_CHECK_INTERVAL_MS = 1000;
   106	const SESSION_AD_START_TIMEOUT_MS = 30000;
   107	const SESSION_AD_FORCE_PLAY_TIMEOUT_MS = 10000;
   108	const SESSION_AD_STUCK_TIMEOUT_MS = 30000;
   109	const VARIANT_SELECTION_LOCALSTORAGE_KEY = "opennow.variantByGameId";
   110	const CATALOG_PREFERENCES_LOCALSTORAGE_KEY = "opennow.catalogPreferences.v1";
   111	const PLAYTIME_RESYNC_INTERVAL_MS = 5 * 60 * 1000;
   112	const FREE_TIER_SESSION_LIMIT_SECONDS = 60 * 60;
   113	const FREE_TIER_30_MIN_WARNING_SECONDS = 30 * 60;
   114	const FREE_TIER_15_MIN_WARNING_SECONDS = 15 * 60;
   115	const FREE_TIER_FINAL_MINUTE_WARNING_SECONDS = 60;
   116	const STREAM_WARNING_VISIBILITY_MS = 15 * 1000;
   117	
   118	interface CatalogPreferences {
   119	  sortId: string;
   120	  filterIds: string[];
   121	}
   122	
   123	function loadCatalogPreferences(): CatalogPreferences {
   124	  try {
   125	    const raw = localStorage.getItem(CATALOG_PREFERENCES_LOCALSTORAGE_KEY);
   126	    if (raw) {
   127	      const parsed = JSON.parse(raw) as Partial<CatalogPreferences>;
   128	      return {
   129	        sortId: typeof parsed.sortId === "string" ? parsed.sortId : "relevance",
   130	        filterIds: Array.isArray(parsed.filterIds) ? parsed.filterIds.filter((id): id is string => typeof id === "string") : [],
   131	      };
   132	    }
   133	  } catch {
   134	    // ignore
   135	  }
   136	  return { sortId: "relevance", filterIds: [] };
   137	}
   138	
   139	function saveCatalogPreferences(prefs: CatalogPreferences): void {
   140	  try {
   141	    localStorage.setItem(CATALOG_PREFERENCES_LOCALSTORAGE_KEY, JSON.stringify(prefs));
   142	  } catch {
   143	    // ignore
   144	  }
   145	}
   146	
   147	type AppPage = "home" | "library" | "settings";
   148	type StreamStatus = "idle" | "queue" | "setup" | "starting" | "connecting" | "streaming";
   149	type StreamLoadingStatus = "queue" | "setup" | "starting" | "connecting";
   150	type ExitPromptState = { open: boolean; gameTitle: string };
   151	type StreamWarningState = {
   152	  code: StreamTimeWarning["code"];
   153	  message: string;
   154	  tone: "warn" | "critical";
   155	  secondsLeft?: number;
   156	};
   157	type LocalSessionTimerWarningState = {
   158	  stage: "free-tier-30m" | "free-tier-15m" | "free-tier-final-minute";
   159	  shownAtMs: number;
   160	};
   161	type LaunchErrorState = {
   162	  stage: StreamLoadingStatus;
   163	  title: string;
   164	  description: string;
   165	  codeLabel?: string;
   166	};
   167	type QueueAdCancelReason = "error" | "other";
   168	type QueueAdErrorInfo = "Ad play timeout" | "Ad video is stuck" | "Error loading url";
   169	type QueueAdMetrics = {
   170	  startedAtMs: number | null;
   171	  wasPausedAtLeastOnce: boolean;
   172	};
   173	type QueueAdReportOptions = {
   174	  cancelReason?: QueueAdCancelReason;
   175	  errorInfo?: QueueAdErrorInfo;
   176	};
   177	
   178	const APP_PAGE_ORDER: AppPage[] = ["home", "library", "settings"];
   179	
   180	const isMac = navigator.platform.toLowerCase().includes("mac");
   181	
   182	function isStandardPrintedWasteZone(zoneId: string): boolean {
   183	  return zoneId.startsWith("NP-") && !zoneId.startsWith("NPA-");
   184	}
   185	
   186	function isAllianceStreamingBaseUrl(streamingBaseUrl: string): boolean {
   187	  if (!streamingBaseUrl.trim()) return false;
   188	  try {
   189	    const { hostname } = new URL(streamingBaseUrl);
   190	    return !hostname.endsWith(".nvidiagrid.net");
   191	  } catch {
   192	    return false;
   193	  }
   194	}
   195	
   196	function hasAnyEligiblePrintedWasteZone(
   197	  queueData: PrintedWasteQueueData,
   198	  mapping: PrintedWasteServerMapping,
   199	): boolean {
   200	  return Object.keys(queueData).some((zoneId) => (
   201	    isStandardPrintedWasteZone(zoneId) && mapping[zoneId]?.nuked !== true
   202	  ));
   203	}
   204	
   205	const DEFAULT_SHORTCUTS = {
   206	  shortcutToggleStats: "F3",
   207	  shortcutTogglePointerLock: "F8",
   208	  shortcutToggleFullscreen: "F10",
   209	  shortcutStopStream: "Ctrl+Shift+Q",
   210	  shortcutToggleAntiAfk: "Ctrl+Shift+K",
   211	  shortcutToggleMicrophone: "Ctrl+Shift+M",
   212	  shortcutScreenshot: "F11",
   213	  shortcutToggleRecording: "F12",
   214	} as const;
   215	
   216	
   217	function sleep(ms: number): Promise<void> {
   218	  return new Promise((resolve) => window.setTimeout(resolve, ms));
   219	}
   220	
   221	async function waitFor(
   222	  predicate: () => boolean,
   223	  { timeout = 1000, interval = 50 }: { timeout?: number; interval?: number } = {},
   224	): Promise<boolean> {
   225	  const start = Date.now();
   226	  while (true) {
   227	    try {
   228	      if (predicate()) return true;
   229	    } catch {
   230	      // ignore predicate errors
   231	    }
   232	    if (Date.now() - start >= timeout) return false;
   233	    // eslint-disable-next-line no-await-in-loop
   234	    await sleep(interval);
   235	  }
   236	}
   237	
   238	function isSessionReadyForConnect(status: number): boolean {
   239	  return status === 2 || status === 3;
   240	}
   241	
   242	function isSessionInQueue(session: SessionInfo): boolean {
   243	  // Official client treats seat setup step 1 as queue state even when queuePosition reaches 1.
   244	  // Fallback to queuePosition-based inference for payloads that do not expose seatSetupStep.
   245	  if (session.seatSetupStep === 1) {
   246	    return true;
   247	  }
   248	  return (session.queuePosition ?? 0) > 1;
   249	}
   250	
   251	function isNumericId(value: string | undefined): value is string {
   252	  if (!value) return false;
   253	  return /^\d+$/.test(value);
   254	}
   255	
   256	function parseNumericId(value: string | undefined): number | null {
   257	  if (!isNumericId(value)) return null;
   258	  const parsed = Number(value);
   259	  return Number.isFinite(parsed) ? parsed : null;
   260	}
   261	
   262	function defaultVariantId(game: GameInfo): string {
   263	  return game.variants[0]?.id ?? game.id;
   264	}
   265	
   266	function getSelectedVariant(game: GameInfo, variantId: string): GameVariant | undefined {
   267	  return game.variants.find((variant) => variant.id === variantId) ?? game.variants[0];
   268	}
   269	
   270	function findSessionContextForAppId(
   271	  catalog: GameInfo[],
   272	  variantByGameId: Record<string, string>,
   273	  appId: number,
   274	): { game: GameInfo; variant?: GameVariant } | null {
   275	  for (const game of catalog) {
   276	    const matchedVariant = game.variants.find((variant) => parseNumericId(variant.id) === appId);
   277	    if (matchedVariant) {
   278	      return { game, variant: matchedVariant };
   279	    }
   280	
   281	    if (parseNumericId(game.launchAppId) === appId) {
   282	      const preferredVariantId = variantByGameId[game.id] ?? defaultVariantId(game);
   283	      return {
   284	        game,
   285	        variant: getSelectedVariant(game, preferredVariantId),
   286	      };
   287	    }
   288	  }
   289	
   290	  return null;
   291	}
   292	
   293	function matchesGameSearch(game: GameInfo, query: string): boolean {
   294	  const normalizedQuery = query.trim().toLowerCase();
   295	  if (!normalizedQuery) return true;
   296	  if (game.searchText?.includes(normalizedQuery)) return true;
   297	  return [
   298	    game.title,
   299	    game.description,
   300	    game.publisherName,
   301	    ...(game.genres ?? []),
   302	    ...(game.featureLabels ?? []),
   303	    ...(game.availableStores ?? []),
   304	  ]
   305	    .filter((value): value is string => typeof value === "string" && value.length > 0)
   306	    .some((value) => value.toLowerCase().includes(normalizedQuery));
   307	}
   308	
   309	function chooseAccountLinked(game: GameInfo, selectedVariant?: GameVariant): boolean {
   310	  if (game.playType === "INSTALL_TO_PLAY") {
   311	    return false;
   312	  }
   313	  if (selectedVariant?.librarySelected) {
   314	    return true;
   315	  }
   316	  if (selectedVariant?.libraryStatus === "IN_LIBRARY") {
   317	    return true;
   318	  }
   319	  if (game.isInLibrary) {
   320	    return true;
   321	  }
   322	  return false;
   323	}
   324	
   325	function areStringArraysEqual(left: string[], right: string[]): boolean {
   326	  if (left.length != right.length) {
   327	    return false;
   328	  }
   329	  return left.every((value, index) => value === right[index]);
   330	}
   331	
   332	function sortLibraryGames(games: GameInfo[], sortId: string): GameInfo[] {
   333	  const copy = [...games];
   334	  const compareTitle = (left: GameInfo, right: GameInfo) => left.title.localeCompare(right.title);
   335	  if (sortId === "z_to_a") {
   336	    return copy.sort((left, right) => right.title.localeCompare(left.title));
   337	  }
   338	  if (sortId === "a_to_z") {
   339	    return copy.sort(compareTitle);
   340	  }
   341	  if (sortId === "last_played") {
   342	    return copy.sort((left, right) => {
   343	      const leftTime = left.lastPlayed ? new Date(left.lastPlayed).getTime() : 0;
   344	      const rightTime = right.lastPlayed ? new Date(right.lastPlayed).getTime() : 0;
   345	      if (leftTime === rightTime) return compareTitle(left, right);
   346	      return rightTime - leftTime;
   347	    });
   348	  }
   349	  if (sortId === "last_added") {
   350	    return copy.sort((left, right) => {
   351	      const leftTime = left.isInLibrary ? new Date(left.lastPlayed ?? 0).getTime() : 0;
   352	      const rightTime = right.isInLibrary ? new Date(right.lastPlayed ?? 0).getTime() : 0;
   353	      if (leftTime === rightTime) return compareTitle(left, right);
   354	      return rightTime - leftTime;
   355	    });
   356	  }
   357	  if (sortId === "most_popular") {
   358	    return copy.sort((left, right) => (right.membershipTierLabel ? 1 : 0) - (left.membershipTierLabel ? 1 : 0) || compareTitle(left, right));
   359	  }
   360	  return copy.sort(compareTitle);
   361	}
   362	
   363	function mergeVariantSelections(
   364	  current: Record<string, string>,
   365	  catalog: GameInfo[],
   366	): Record<string, string> {
   367	  if (catalog.length === 0) {
   368	    return current;
   369	  }
   370	
   371	  const next = { ...current };
   372	  for (const game of catalog) {
   373	    const selectedVariantId = next[game.id];
   374	    const hasSelectedVariant = !!selectedVariantId && game.variants.some((variant) => variant.id === selectedVariantId);
   375	    if (!hasSelectedVariant) {
   376	      next[game.id] = defaultVariantId(game);
   377	    }
   378	  }
   379	  return next;
   380	}
   381	
   382	function defaultDiagnostics(): StreamDiagnostics {
   383	  return {
   384	    connectionState: "closed",
   385	    inputReady: false,
   386	    connectedGamepads: 0,
   387	    resolution: "",
   388	    codec: "",
   389	    isHdr: false,
   390	    bitrateKbps: 0,
   391	    decodeFps: 0,
   392	    renderFps: 0,
   393	    packetsLost: 0,
   394	    packetsReceived: 0,
   395	    packetLossPercent: 0,
   396	    jitterMs: 0,
   397	    rttMs: 0,
   398	    framesReceived: 0,
   399	    framesDecoded: 0,
   400	    framesDropped: 0,
   401	    decodeTimeMs: 0,
   402	    renderTimeMs: 0,
   403	    jitterBufferDelayMs: 0,
   404	    inputQueueBufferedBytes: 0,
   405	    inputQueuePeakBufferedBytes: 0,
   406	    partiallyReliableInputQueueBufferedBytes: 0,
   407	    partiallyReliableInputQueuePeakBufferedBytes: 0,
   408	    inputQueueDropCount: 0,
   409	    inputQueueMaxSchedulingDelayMs: 0,
   410	    partiallyReliableInputOpen: false,
   411	    mouseMoveTransport: "reliable",
   412	    lagReason: "unknown",
   413	    lagReasonDetail: "Waiting for stream stats",
   414	    gpuType: "",
   415	    serverRegion: "",
   416	    decoderPressureActive: false,
   417	    decoderRecoveryAttempts: 0,
   418	    decoderRecoveryAction: "none",
   419	    micState: "uninitialized",
   420	    micEnabled: false,
   421	  };
   422	}
   423	
   424	function isSessionLimitError(error: unknown): boolean {
   425	  if (error && typeof error === "object" && "gfnErrorCode" in error) {
   426	    const candidate = error.gfnErrorCode;
   427	    if (typeof candidate === "number") {
   428	      return candidate === 3237093643 || candidate === 3237093718;
   429	    }
   430	  }
   431	  if (error instanceof Error) {
   432	    const msg = error.message.toUpperCase();
   433	    return msg.includes("SESSION LIMIT") || msg.includes("INSUFFICIENT_PLAYABILITY") || msg.includes("DUPLICATE SESSION");
   434	  }
   435	  return false;
   436	}
   437	
   438	function warningTone(code: StreamTimeWarning["code"]): "warn" | "critical" {
   439	  if (code === 3) {
   440	    return "critical";
   441	  }
   442	  return "warn";
   443	}
   444	
   445	function warningMessage(code: StreamTimeWarning["code"]): string {
   446	  if (code === 1) return "Session time limit approaching";
   447	  if (code === 2) return "Idle timeout approaching";
   448	  return "Maximum session time approaching";
   449	}
   450	
   451	function formatRemainingPlaytimeFromSubscription(
   452	  subscription: SubscriptionInfo | null,
   453	  consumedHours = 0,
   454	): string {
   455	  if (!subscription) {
   456	    return "--";
   457	  }
   458	  if (subscription.isUnlimited) {
   459	    return "Unlimited";
   460	  }
   461	
   462	  const baseHours = Number.isFinite(subscription.remainingHours) ? subscription.remainingHours : 0;
   463	  const safeHours = Math.max(0, baseHours - Math.max(0, consumedHours));
   464	  const totalMinutes = Math.round(safeHours * 60);
   465	  const hours = Math.floor(totalMinutes / 60);
   466	  const minutes = totalMinutes % 60;
   467	
   468	  if (hours > 0) {
   469	    return `${hours}h ${minutes.toString().padStart(2, "0")}m`;
   470	  }
   471	  return `${minutes}m`;
   472	}
   473	
   474	function normalizeMembershipTier(tier: string | null | undefined): string | null {
   475	  if (!tier) {
   476	    return null;
   477	  }
   478	  const normalized = tier.trim().toUpperCase();
   479	  return normalized.length > 0 ? normalized : null;
   480	}
   481	
   482	function shouldShowQueueAdsForMembership(
   483	  subscription: SubscriptionInfo | null,
   484	  authSession: AuthSession | null,
   485	): boolean {
   486	  const effectiveTier = normalizeMembershipTier(subscription?.membershipTier ?? authSession?.user.membershipTier);
   487	  return effectiveTier === null || effectiveTier === "FREE";
   488	}
   489	
   490	function shouldShowFreeTierSessionWarnings(subscription: SubscriptionInfo | null): boolean {
   491	  return normalizeMembershipTier(subscription?.membershipTier) === "FREE";
   492	}
   493	
   494	function hasCrossedWarningThreshold(
   495	  previousSeconds: number | null,
   496	  currentSeconds: number,
   497	  thresholdSeconds: number,
   498	): boolean {
   499	  if (previousSeconds === null) {
   500	    return currentSeconds === thresholdSeconds;
   501	  }
   502	  return previousSeconds > thresholdSeconds && currentSeconds <= thresholdSeconds;
   503	}
   504	
   505	function getLocalSessionTimerWarning(
   506	  stage: LocalSessionTimerWarningState["stage"],
   507	  secondsLeft: number,
   508	): StreamWarningState {
   509	  if (stage === "free-tier-30m") {
   510	    return {
   511	      code: 1,
   512	      message: "30 minutes remaining in this free-tier session",
   513	      tone: "warn",
   514	    };
   515	  }
   516	
   517	  if (stage === "free-tier-15m") {
   518	    return {
   519	      code: 1,
   520	      message: "15 minutes remaining in this free-tier session",
   521	      tone: "warn",
   522	    };
   523	  }
   524	
   525	  return {
   526	    code: 1,
   527	    message: "This free-tier session ends soon",
   528	    tone: "critical",
   529	    secondsLeft: Math.max(0, secondsLeft),
   530	  };
   531	}
   532	
   533	function shouldUseQueueAdPolling(session: SessionInfo, subscription: SubscriptionInfo | null, authSession: AuthSession | null): boolean {
   534	  return (
   535	    shouldShowQueueAdsForMembership(subscription, authSession) &&
   536	    isSessionInQueue(session) &&
   537	    isSessionAdsRequired(session.adState)
   538	  );
   539	}
   540	
   541	function getEffectiveAdState(
   542	  session: SessionInfo | null,
   543	  subscription: SubscriptionInfo | null,
   544	  authSession: AuthSession | null,
   545	): SessionAdState | undefined {
   546	  if (!session) {
   547	    return undefined;
   548	  }
   549	
   550	  if (session.adState) {
   551	    return session.adState;
   552	  }
   553	
   554	  if ((session.status === 1 || session.status === 2 || session.status === 3) && session.queuePosition !== undefined) {
   555	    return {
   556	      isAdsRequired: true,
   557	      sessionAdsRequired: true,
   558	      message: "Free-tier queue ads begin as soon as you enter queue.",
   559	      opportunity: {
   560	        message: "Free-tier queue ads begin as soon as you enter queue.",
   561	      },
   562	      sessionAds: [],
   563	      ads: [],
   564	      serverSentEmptyAds: true,
   565	    };
   566	  }
   567	
   568	  if (!shouldShowQueueAdsForMembership(subscription, authSession)) {
   569	    return undefined;
   570	  }
   571	
   572	  if (!isSessionInQueue(session)) {
   573	    return undefined;
   574	  }
   575	
   576	  return {
   577	    isAdsRequired: true,
   578	    sessionAdsRequired: true,
   579	    message: "Free-tier queue ads begin as soon as you enter queue.",
   580	    opportunity: {
   581	      message: "Free-tier queue ads begin as soon as you enter queue.",
   582	    },
   583	    sessionAds: [
   584	      {
   585	        adId: "queue-ad-placeholder",
   586	        title: "Advertisement in progress",
   587	        description: "Ad media will appear here as soon as it is available.",
   588	      },
   589	    ],
   590	    ads: [
   591	      {
   592	        adId: "queue-ad-placeholder",
   593	        title: "Advertisement in progress",
   594	        description: "Ad media will appear here as soon as it is available.",
   595	      },
   596	    ],
   597	  };
   598	}
   599	
   600	function mergeAdState(
   601	  previous: SessionAdState | undefined,
   602	  next: SessionAdState | undefined,
   603	): SessionAdState | undefined {
   604	  if (!next) {
   605	    return previous;
   606	  }
   607	  // Server only populates sessionAds in the first poll after session creation.
   608	  // Later polls return sessionAdsRequired=true but sessionAds=null (serverSentEmptyAds=true),
   609	  // which produces an empty ads array. Preserve the ad list from the most recent poll that
   610	  // had URLs so the ad player can continue.
   611	  // Do NOT restore when serverSentEmptyAds is false — that signals an explicit client-side
   612	  // clear after a rejected finish action, and we must NOT bring the stale ad back.
   613	  if (
   614	    isSessionAdsRequired(next) &&
   615	    next.serverSentEmptyAds === true &&
   616	    getSessionAdItems(next).length === 0 &&
   617	    previous?.sessionAds &&
   618	    previous.sessionAds.length > 0
   619	  ) {
   620	    return { ...next, sessionAds: previous.sessionAds, ads: previous.ads };
   621	  }
   622	  return next;
   623	}
   624	
   625	function mergePolledSessionState(previous: SessionInfo, next: SessionInfo): SessionInfo {
   626	  if (isSessionReadyForConnect(next.status)) {
   627	    return next;
   628	  }
   629	
   630	  return {
   631	    ...next,
   632	    adState: mergeAdState(previous.adState, next.adState),
   633	    mediaConnectionInfo: next.mediaConnectionInfo ?? previous.mediaConnectionInfo,
   634	  };
   635	}
   636	
   637	function getNextAdReportAction(
   638	  lastAction: SessionAdAction | undefined,
   639	  playbackEvent: "playing" | "paused" | "ended",
   640	): SessionAdAction | null {
   641	  switch (playbackEvent) {
   642	    case "playing":
   643	      if (!lastAction) {
   644	        return "start";
   645	      }
   646	      return lastAction === "pause" ? "resume" : null;
   647	    case "paused":
   648	      return lastAction === "start" || lastAction === "resume" ? "pause" : null;
   649	    case "ended":
   650	      return lastAction === "finish" || lastAction === "cancel" ? null : "finish";
   651	    default:
   652	      return null;
   653	  }
   654	}
   655	
   656	function getActiveQueueAd(
   657	  adState: SessionAdState | undefined,
   658	  activeAdId: string | null,
   659	): SessionAdInfo | undefined {
   660	  const ads = getSessionAdItems(adState);
   661	  if (!ads.length) {
   662	    return undefined;
   663	  }
   664	
   665	  if (activeAdId) {
   666	    const matched = ads.find((ad) => ad.adId === activeAdId);
   667	    if (matched) {
   668	      return matched;
   669	    }
   670	  }
   671	
   672	  return ads[0];
   673	}
   674	
   675	function getNextQueueAd(
   676	  adState: SessionAdState | undefined,
   677	  activeAdId: string,
   678	): SessionAdInfo | undefined {
   679	  const ads = getSessionAdItems(adState);
   680	  if (!ads.length) {
   681	    return undefined;
   682	  }
   683	
   684	  const currentIndex = ads.findIndex((ad) => ad.adId === activeAdId);
   685	  if (currentIndex < 0) {
   686	    return ads[0];
   687	  }
   688	
   689	  return ads[currentIndex + 1];
   690	}
   691	
   692	function toLoadingStatus(status: StreamStatus): StreamLoadingStatus {
   693	  switch (status) {
   694	    case "queue":
   695	    case "setup":
   696	    case "starting":
   697	    case "connecting":
   698	      return status;
   699	    default:
   700	      return "queue";
   701	  }
   702	}
   703	
   704	function toCodeLabel(code: number | undefined): string | undefined {
   705	  if (code === undefined) return undefined;
   706	  if (code === 3237093643) return `SessionLimitExceeded (${code})`;
   707	  if (code === 3237093718) return `SessionInsufficientPlayabilityLevel (${code})`;
   708	  return `GFN Error ${code}`;
   709	}
   710	
   711	function extractLaunchErrorCode(error: unknown): number | undefined {
   712	  if (error && typeof error === "object") {
   713	    if ("gfnErrorCode" in error) {
   714	      const directCode = error.gfnErrorCode;
   715	      if (typeof directCode === "number") return directCode;
   716	    }
   717	    if ("statusCode" in error) {
   718	      const statusCode = error.statusCode;
   719	      if (typeof statusCode === "number" && statusCode > 0 && statusCode < 255) {
   720	        return 3237093632 + statusCode;
   721	      }
   722	    }
   723	  }
   724	  if (error instanceof Error) {
   725	    const match = error.message.match(/\b(3237\d{6,})\b/);
   726	    if (match) {
   727	      const code = Number(match[1]);
   728	      if (Number.isFinite(code)) return code;
   729	    }
   730	  }
   731	  return undefined;
   732	}
   733	
   734	function toLaunchErrorState(error: unknown, stage: StreamLoadingStatus): LaunchErrorState {
   735	  const unknownMessage = "The game could not start. Please try again.";
   736	
   737	  const titleFromError =
   738	    error && typeof error === "object" && "title" in error && typeof error.title === "string"
   739	      ? error.title.trim()
   740	      : "";
   741	  const descriptionFromError =
   742	    error && typeof error === "object" && "description" in error && typeof error.description === "string"
   743	      ? error.description.trim()
   744	      : "";
   745	  const statusDescription =
   746	    error && typeof error === "object" && "statusDescription" in error && typeof error.statusDescription === "string"
   747	      ? error.statusDescription.trim()
   748	      : "";
   749	  const messageFromError = error instanceof Error ? error.message.trim() : "";
   750	  const combined = `${statusDescription} ${messageFromError}`.toUpperCase();
   751	  const code = extractLaunchErrorCode(error);
   752	
   753	  if (
   754	    isSessionLimitError(error) ||
   755	    combined.includes("INSUFFICIENT_PLAYABILITY") ||
   756	    combined.includes("SESSION_LIMIT") ||
   757	    combined.includes("DUPLICATE SESSION")
   758	  ) {
   759	    return {
   760	      stage,
   761	      title: "Duplicate Session Detected",
   762	      description: "Another session is already running on your account. Close it first or wait for it to timeout, then launch again.",
   763	      codeLabel: toCodeLabel(code),
   764	    };
   765	  }
   766	
   767	  return {
   768	    stage,
   769	    title: titleFromError || "Launch Failed",
   770	    description: descriptionFromError || messageFromError || statusDescription || unknownMessage,
   771	    codeLabel: toCodeLabel(code),
   772	  };
   773	}
   774	
   775	export function App(): JSX.Element {
   776	
   777	  // Auth State
   778	  const [authSession, setAuthSession] = useState<AuthSession | null>(null);
   779	  const [savedAccounts, setSavedAccounts] = useState<SavedAccount[]>([]);
   780	  const [providers, setProviders] = useState<LoginProvider[]>([]);
   781	  const [providerIdpId, setProviderIdpId] = useState("");
   782	  const [isLoggingIn, setIsLoggingIn] = useState(false);
   783	  const [loginError, setLoginError] = useState<string | null>(null);
   784	  const [isInitializing, setIsInitializing] = useState(true);
   785	  const [startupStatusMessage, setStartupStatusMessage] = useState("Restoring saved session...");
   786	  const [startupRefreshNotice, setStartupRefreshNotice] = useState<{
   787	    tone: "success" | "warn";
   788	    text: string;
   789	  } | null>(null);
   790	
   791	  // Navigation
   792	  const [currentPage, setCurrentPage] = useState<AppPage>("home");
   793	
   794	  // Games State
   795	  const [games, setGames] = useState<GameInfo[]>([]);
   796	  const [libraryGames, setLibraryGames] = useState<GameInfo[]>([]);
   797	  const [searchQuery, setSearchQuery] = useState("");
   798	  const [selectedGameId, setSelectedGameId] = useState("");
   799	  const [variantByGameId, setVariantByGameId] = useState<Record<string, string>>({});
   800	  const [isLoadingGames, setIsLoadingGames] = useState(false);
   801	  const [catalogFilterGroups, setCatalogFilterGroups] = useState<CatalogFilterGroup[]>([]);
   802	  const [catalogSortOptions, setCatalogSortOptions] = useState<CatalogSortOption[]>([]);
   803	  const [catalogSelectedSortId, setCatalogSelectedSortId] = useState(() => loadCatalogPreferences().sortId);
   804	  const [catalogSelectedFilterIds, setCatalogSelectedFilterIds] = useState<string[]>(() => loadCatalogPreferences().filterIds);
   805	  const [catalogTotalCount, setCatalogTotalCount] = useState(0);
   806	  const [catalogSupportedCount, setCatalogSupportedCount] = useState(0);
   807	  const catalogFilterKey = useMemo(() => catalogSelectedFilterIds.join("|"), [catalogSelectedFilterIds]);
   808	
   809	  // Settings State
   810	  const [settings, setSettings] = useState<Settings>({
   811	    resolution: "1920x1080",
   812	    aspectRatio: "16:9",
   813	    posterSizeScale: 1,
   814	    fps: 60,
   815	    maxBitrateMbps: 75,
   816	    codec: DEFAULT_STREAM_PREFERENCES.codec,
   817	    decoderPreference: "auto",
   818	    encoderPreference: "auto",
   819	    colorQuality: DEFAULT_STREAM_PREFERENCES.colorQuality,
   820	    region: "",
   821	    clipboardPaste: false,
   822	    mouseSensitivity: 1,
   823	    mouseAcceleration: 1,
   824	    shortcutToggleStats: DEFAULT_SHORTCUTS.shortcutToggleStats,
   825	    shortcutTogglePointerLock: DEFAULT_SHORTCUTS.shortcutTogglePointerLock,
   826	    shortcutToggleFullscreen: DEFAULT_SHORTCUTS.shortcutToggleFullscreen,
   827	    shortcutStopStream: DEFAULT_SHORTCUTS.shortcutStopStream,
   828	    shortcutToggleAntiAfk: DEFAULT_SHORTCUTS.shortcutToggleAntiAfk,
   829	    shortcutToggleMicrophone: DEFAULT_SHORTCUTS.shortcutToggleMicrophone,
   830	    shortcutScreenshot: DEFAULT_SHORTCUTS.shortcutScreenshot,
   831	    shortcutToggleRecording: DEFAULT_SHORTCUTS.shortcutToggleRecording,
   832	    microphoneMode: "disabled",
   833	    microphoneDeviceId: "",
   834	    hideStreamButtons: false,
   835	    showAntiAfkIndicator: true,
   836	    showStatsOnLaunch: false,
   837	    controllerMode: false,
   838	    controllerUiSounds: false,
   839	    controllerBackgroundAnimations: false,
   840	    autoLoadControllerLibrary: false,
   841	    autoFullScreen: false,
   842	    favoriteGameIds: [],
   843	    sessionCounterEnabled: false,
   844	    sessionClockShowEveryMinutes: 60,
   845	    sessionClockShowDurationSeconds: 30,
   846	    windowWidth: 1400,
   847	    windowHeight: 900,
   848	    keyboardLayout: DEFAULT_KEYBOARD_LAYOUT,
   849	    gameLanguage: "en_US",
   850	    enableL4S: false,
   851	    enableCloudGsync: false,
   852	    discordRichPresence: false,
   853	    autoCheckForUpdates: true,
   854	  });
   855	  const [settingsLoaded, setSettingsLoaded] = useState(false);
   856	  const [codecResults, setCodecResults] = useState<CodecTestResult[] | null>(() => loadStoredCodecResults());
   857	  const [codecTesting, setCodecTesting] = useState(false);
   858	  const [regions, setRegions] = useState<StreamRegion[]>([]);
   859	  const [subscriptionInfo, setSubscriptionInfo] = useState<SubscriptionInfo | null>(null);
   860	  const diagnosticsStoreRef = useRef<ReturnType<typeof createStreamDiagnosticsStore> | null>(null);
   861	  const diagnosticsStore =
   862	    diagnosticsStoreRef.current ?? (diagnosticsStoreRef.current = createStreamDiagnosticsStore(defaultDiagnostics()));
   863	
   864	  // Stream State
   865	  const [session, setSession] = useState<SessionInfo | null>(null);
   866	  const [streamStatus, setStreamStatus] = useState<StreamStatus>("idle");
   867	  const [showStatsOverlay, setShowStatsOverlay] = useState(false);
   868	  const [antiAfkEnabled, setAntiAfkEnabled] = useState(false);
   869	  const [escHoldReleaseIndicator, setEscHoldReleaseIndicator] = useState<{ visible: boolean; progress: number }>({
   870	    visible: false,
   871	    progress: 0,
   872	  });
   873	  const [exitPrompt, setExitPrompt] = useState<ExitPromptState>({ open: false, gameTitle: "Game" });
   874	  const [streamingGame, setStreamingGame] = useState<GameInfo | null>(null);
   875	  const [streamingStore, setStreamingStore] = useState<string | null>(null);
   876	  const [queuePosition, setQueuePosition] = useState<number | undefined>();
   877	  const [navbarActiveSession, setNavbarActiveSession] = useState<ActiveSessionInfo | null>(null);
   878	  const [isResumingNavbarSession, setIsResumingNavbarSession] = useState(false);
   879	  const [isTerminatingNavbarSession, setIsTerminatingNavbarSession] = useState(false);
   880	  const [accountToRemove, setAccountToRemove] = useState<string | null>(null);
   881	  const [removeAccountConfirmOpen, setRemoveAccountConfirmOpen] = useState(false);
   882	  const [logoutConfirmOpen, setLogoutConfirmOpen] = useState(false);
   883	  const [launchError, setLaunchError] = useState<LaunchErrorState | null>(null);
   884	  const [queueModalGame, setQueueModalGame] = useState<GameInfo | null>(null);
   885	  const [queueModalData, setQueueModalData] = useState<PrintedWasteQueueData | null>(null);
   886	  const [sessionStartedAtMs, setSessionStartedAtMs] = useState<number | null>(null);
   887	  const [remoteStreamWarning, setRemoteStreamWarning] = useState<StreamWarningState | null>(null);
   888	  const [localSessionTimerWarning, setLocalSessionTimerWarning] = useState<LocalSessionTimerWarningState | null>(null);
   889	  const [activeQueueAdId, setActiveQueueAdId] = useState<string | null>(null);
   890	  const previousFreeTierRemainingSecondsRef = useRef<number | null>(null);
   891	
   892	  const { playtime, startSession: startPlaytimeSession, endSession: endPlaytimeSession } = usePlaytime();
   893	  const sessionElapsedSeconds = useElapsedSeconds(sessionStartedAtMs, streamStatus === "streaming");
   894	  const isStreaming = streamStatus === "streaming";
   895	  const freeTierSessionWarningsActive =
   896	    isStreaming && sessionStartedAtMs !== null && shouldShowFreeTierSessionWarnings(subscriptionInfo);
   897	  const freeTierSessionRemainingSeconds = freeTierSessionWarningsActive
   898	    ? Math.max(0, FREE_TIER_SESSION_LIMIT_SECONDS - sessionElapsedSeconds)
   899	    : null;
   900	  const visibleLocalSessionTimerWarning = useMemo(() => {
   901	    if (localSessionTimerWarning === null || freeTierSessionRemainingSeconds === null) {
   902	      return null;
   903	    }
   904	
   905	    return getLocalSessionTimerWarning(localSessionTimerWarning.stage, freeTierSessionRemainingSeconds);
   906	  }, [freeTierSessionRemainingSeconds, localSessionTimerWarning]);
   907	  const streamWarning = useMemo(() => {
   908	    if (visibleLocalSessionTimerWarning?.tone === "critical") {
   909	      return visibleLocalSessionTimerWarning;
   910	    }
   911	    return remoteStreamWarning ?? visibleLocalSessionTimerWarning;
   912	  }, [remoteStreamWarning, visibleLocalSessionTimerWarning]);
   913	
   914	  const controllerOverlayOpenRef = useRef(false);
   915	  const codecTestPromiseRef = useRef<Promise<CodecTestResult[] | null> | null>(null);
   916	  const codecStartupTestAttemptedRef = useRef(false);
   917	  const navbarSessionActionInFlightRef = useRef<"resume" | "terminate" | null>(null);
   918	
   919	  const resetStatsOverlayToPreference = useCallback((): void => {
   920	    setShowStatsOverlay(settings.showStatsOnLaunch);
   921	  }, [settings.showStatsOnLaunch]);
   922	
   923	  const runCodecTest = useCallback(async (): Promise<void> => {
   924	    if (codecTestPromiseRef.current) {
   925	      await codecTestPromiseRef.current;
   926	      return;
   927	    }
   928	
   929	    const testPromise = (async (): Promise<CodecTestResult[] | null> => {
   930	      setCodecTesting(true);
   931	      try {
   932	        const results = await testCodecSupport();
   933	        setCodecResults(results);
   934	        saveStoredCodecResults(results);
   935	        return results;
   936	      } catch (error) {
   937	        console.error("Codec test failed:", error);
   938	        return null;
   939	      } finally {
   940	        setCodecTesting(false);
   941	        codecTestPromiseRef.current = null;
   942	      }
   943	    })();
   944	
   945	    codecTestPromiseRef.current = testPromise;
   946	    await testPromise;
   947	  }, []);
   948	
   949	  const handleControllerPageNavigate = useCallback((direction: "prev" | "next"): void => {
   950	    if (controllerOverlayOpenRef.current) {
   951	      window.dispatchEvent(new CustomEvent("opennow:controller-shoulder", { detail: { direction } }));
   952	      return;
   953	    }
   954	    if (!authSession || streamStatus !== "idle") {
   955	      return;
   956	    }
   957	
   958	    if (settings.controllerMode && currentPage === "library") {
   959	      window.dispatchEvent(new CustomEvent("opennow:controller-shoulder", { detail: { direction } }));
   960	      return;
   961	    }
   962	
   963	    const currentIndex = APP_PAGE_ORDER.indexOf(currentPage);
   964	    const step = direction === "next" ? 1 : -1;
   965	    const nextIndex = (currentIndex + step + APP_PAGE_ORDER.length) % APP_PAGE_ORDER.length;
   966	    setCurrentPage(APP_PAGE_ORDER[nextIndex]);
   967	  }, [authSession, currentPage, settings.controllerMode, streamStatus]);
   968	
   969	  const handleControllerBackAction = useCallback((): boolean => {
   970	    // Prefer to let the controller library handle Back (e.g. closing submenus
   971	    // inside the XMB) before falling back to global navigation.
   972	    const cancelEvent = new CustomEvent("opennow:controller-cancel", { cancelable: true });
   973	    window.dispatchEvent(cancelEvent);
   974	    if (cancelEvent.defaultPrevented) {
   975	      return true;
   976	    }
   977	
   978	    if (controllerOverlayOpenRef.current) {
   979	      setControllerOverlayOpen(false);
   980	      return true;
   981	    }
   982	
   983	    if (!authSession || streamStatus !== "idle") {
   984	      return false;
   985	    }
   986	
   987	    if (settings.controllerMode && currentPage === "settings") {
   988	      setCurrentPage("library");
   989	      return true;
   990	    }
   991	
   992	    if (currentPage !== "home") {
   993	      setCurrentPage("home");
   994	      return true;
   995	    }
   996	    return false;
   997	  }, [authSession, currentPage, settings.controllerMode, streamStatus]);
   998	
   999	  const handleControllerDirectionInput = useCallback((direction: "up" | "down" | "left" | "right"): boolean => {
  1000	    if (controllerOverlayOpenRef.current) {
  1001	      window.dispatchEvent(new CustomEvent("opennow:controller-direction", { detail: { direction } }));
  1002	      return true;
  1003	    }
  1004	    if (!authSession || streamStatus !== "idle") {
  1005	      return false;
  1006	    }
  1007	    if (settings.controllerMode && currentPage === "library") {
  1008	      window.dispatchEvent(new CustomEvent("opennow:controller-direction", { detail: { direction } }));
  1009	      return true;
  1010	    }
  1011	    if (settings.controllerMode && currentPage === "settings") {
  1012	      window.dispatchEvent(new CustomEvent("opennow:controller-direction", { detail: { direction } }));
  1013	      return true;
  1014	    }
  1015	    return false;
  1016	  }, [authSession, currentPage, settings.controllerMode, streamStatus]);
  1017	
  1018	  const handleControllerActivateInput = useCallback((): boolean => {
  1019	    if (controllerOverlayOpenRef.current) {
  1020	      window.dispatchEvent(new CustomEvent("opennow:controller-activate"));
  1021	      return true;
  1022	    }
  1023	    if (!authSession || streamStatus !== "idle") {
  1024	      return false;
  1025	    }
  1026	    if (settings.controllerMode && currentPage === "library") {
  1027	      window.dispatchEvent(new CustomEvent("opennow:controller-activate"));
  1028	      return true;
  1029	    }
  1030	    if (settings.controllerMode && currentPage === "settings") {
  1031	      window.dispatchEvent(new CustomEvent("opennow:controller-activate"));
  1032	      return true;
  1033	    }
  1034	    return false;
  1035	  }, [authSession, currentPage, settings.controllerMode, streamStatus]);
  1036	
  1037	  const handleControllerSecondaryActivateInput = useCallback((): boolean => {
  1038	    if (controllerOverlayOpenRef.current) {
  1039	      window.dispatchEvent(new CustomEvent("opennow:controller-secondary-activate"));
  1040	      return true;
  1041	    }
  1042	    if (!authSession || streamStatus !== "idle") {
  1043	      return false;
  1044	    }
  1045	    if (settings.controllerMode && currentPage === "library") {
  1046	      window.dispatchEvent(new CustomEvent("opennow:controller-secondary-activate"));
  1047	      return true;
  1048	    }
  1049	    if (settings.controllerMode && currentPage === "settings") {
  1050	      window.dispatchEvent(new CustomEvent("opennow:controller-secondary-activate"));
  1051	      return true;
  1052	    }
  1053	    return false;
  1054	  }, [authSession, currentPage, settings.controllerMode, streamStatus]);
  1055	
  1056	  const handleControllerTertiaryActivateInput = useCallback((): boolean => {
  1057	    if (controllerOverlayOpenRef.current) {
  1058	      window.dispatchEvent(new CustomEvent("opennow:controller-tertiary-activate"));
  1059	      return true;
  1060	    }
  1061	    if (!authSession || streamStatus !== "idle") {
  1062	      return false;
  1063	    }
  1064	    if (settings.controllerMode && currentPage === "library") {
  1065	      window.dispatchEvent(new CustomEvent("opennow:controller-tertiary-activate"));
  1066	      return true;
  1067	    }
  1068	    if (settings.controllerMode && currentPage === "settings") {
  1069	      window.dispatchEvent(new CustomEvent("opennow:controller-tertiary-activate"));
  1070	      return true;
  1071	    }
  1072	    return false;
  1073	  }, [authSession, currentPage, settings.controllerMode, streamStatus]);
  1074	
  1075	  const [controllerOverlayOpen, setControllerOverlayOpen] = useState(false);
  1076	  const [isSwitchingGame, setIsSwitchingGame] = useState(false);
  1077	  const [switchingPhase, setSwitchingPhase] = useState<null | "cleaning" | "creating">(null);
  1078	  const [pendingSwitchGameTitle, setPendingSwitchGameTitle] = useState<string | null>(null);
  1079	  const [pendingSwitchGameCover, setPendingSwitchGameCover] = useState<string | null>(null);
  1080	  const [pendingSwitchGameDescription, setPendingSwitchGameDescription] = useState<string | null>(null);
  1081	  const [pendingSwitchGameId, setPendingSwitchGameId] = useState<string | null>(null);
  1082	  const controllerDesktopModeActive = Boolean(authSession)
  1083	    && streamStatus === "idle"
  1084	    && settings.controllerMode
  1085	    && (currentPage === "library" || currentPage === "settings");
  1086	  const controllerUiActive = controllerDesktopModeActive || controllerOverlayOpen;
  1087	
  1088	  const controllerConnected = useControllerNavigation({
  1089	    enabled: controllerUiActive,
  1090	    onNavigatePage: handleControllerPageNavigate,
  1091	    onBackAction: handleControllerBackAction,
  1092	    onDirectionInput: handleControllerDirectionInput,
  1093	    onActivateInput: handleControllerActivateInput,
  1094	    onSecondaryActivateInput: handleControllerSecondaryActivateInput,
  1095	    onTertiaryActivateInput: handleControllerTertiaryActivateInput,
  1096	  });
  1097	  const showControllerHint = controllerUiActive
  1098	    && controllerConnected
  1099	    && !(settings.controllerMode && currentPage === "library");
  1100	
  1101	  useEffect(() => {
  1102	    let raf = 0;
  1103	    const prev = { pressed: false };
  1104	    const tick = () => {
  1105	      try {
  1106	        if (streamStatus !== "streaming") {
  1107	          prev.pressed = false;
  1108	          raf = window.requestAnimationFrame(tick);
  1109	          return;
  1110	        }
  1111	        const pads = navigator.getGamepads ? navigator.getGamepads() : [];
  1112	        const pad = Array.from(pads).find((p) => p && p.connected) ?? null;
  1113	        if (!pad) {
  1114	          prev.pressed = false;
  1115	          raf = window.requestAnimationFrame(tick);
  1116	          return;
  1117	        }
  1118	        // Meta/Home button only: button 16 (standard)
  1119	        const metaPressed = Boolean(pad.buttons[16]?.pressed);
  1120	        if (metaPressed && !prev.pressed) {
  1121	          setControllerOverlayOpen((v) => !v);
  1122	        }
  1123	        prev.pressed = metaPressed;
  1124	      } catch {
  1125	        // ignore
  1126	      }
  1127	      raf = window.requestAnimationFrame(tick);
  1128	    };
  1129	    raf = window.requestAnimationFrame(tick);
  1130	    return () => { if (raf) window.cancelAnimationFrame(raf); };
  1131	  }, [streamStatus]);
  1132	
  1133	  // Refs
  1134	  const videoRef = useRef<HTMLVideoElement | null>(null);
  1135	  const audioRef = useRef<HTMLAudioElement | null>(null);
  1136	  const clientRef = useRef<GfnWebRtcClient | null>(null);
  1137	  const sessionRef = useRef<SessionInfo | null>(null);
  1138	  const hasInitializedRef = useRef(false);
  1139	  const regionsRequestRef = useRef(0);
  1140	  const launchInFlightRef = useRef(false);
  1141	  const launchAbortRef = useRef(false);
  1142	  const streamStatusRef = useRef<StreamStatus>(streamStatus);
  1143	  const exitPromptResolverRef = useRef<((confirmed: boolean) => void) | null>(null);
  1144	  const adReportQueueRef = useRef<Promise<void>>(Promise.resolve());
  1145	  const adReportStateRef = useRef<Record<string, SessionAdAction>>({});
  1146	  const adMetricsRef = useRef<Record<string, QueueAdMetrics>>({});
  1147	  const adMediaUrlCacheRef = useRef<Record<string, string>>({});
  1148	  const queueAdPlaybackRef = useRef<{ adId: string; phase: "playing" | "finishing" } | null>(null);
  1149	  const queueAdPreviewRef = useRef<QueueAdPreviewHandle | null>(null);
  1150	  const activeQueueAdIdRef = useRef<string | null>(null);
  1151	  const adForcePlayTimeoutRef = useRef<number | null>(null);
  1152	  const adStartTimeoutRef = useRef<number | null>(null);
  1153	  const adProgressIntervalRef = useRef<number | null>(null);
  1154	  const adLastProgressTsRef = useRef<number | null>(null);
  1155	  // Stable ref so timer callbacks can call the latest reportQueueAdAction without
  1156	  // capturing a stale closure (auth state can change between scheduling and firing).
  1157	  const reportQueueAdActionRef = useRef<
  1158	    (adId: string, action: SessionAdAction, options?: QueueAdReportOptions) => void
  1159	  >(() => {});
  1160	
  1161	  useEffect(() => {
  1162	    controllerOverlayOpenRef.current = controllerOverlayOpen;
  1163	    if (clientRef.current) {
  1164	      clientRef.current.inputPaused = controllerOverlayOpen;
  1165	    }
  1166	  }, [controllerOverlayOpen]);
  1167	
  1168	  useEffect(() => {
  1169	    if (!controllerOverlayOpen) return;
  1170	    const overlay = document.querySelector(".controller-overlay");
  1171	    if (!overlay) return;
  1172	    const selector = [
  1173	      "button",
  1174	      "a[href]",
  1175	      "input:not([type='hidden'])",
  1176	      "select",
  1177	      "textarea",
  1178	      "[role='button']",
  1179	      "[tabindex]:not([tabindex='-1'])",
  1180	    ].join(",");
  1181	    const candidates = Array.from(overlay.querySelectorAll(selector)) as HTMLElement[];
  1182	    const first = candidates.find((el) => {
  1183	      const style = window.getComputedStyle(el);
  1184	      if (style.visibility === "hidden" || style.display === "none") return false;
  1185	      if ((el as HTMLButtonElement | HTMLInputElement | any).disabled) return false;
  1186	      return el.tabIndex >= 0;
  1187	    });
  1188	    if (first) {
  1189	      document.querySelectorAll<HTMLElement>(".controller-focus").forEach((n) => n.classList.remove("controller-focus"));
  1190	      first.classList.add("controller-focus");
  1191	      try {
  1192	        first.focus({ preventScroll: true });
  1193	      } catch {
  1194	        /* ignore */
  1195	      }
  1196	      first.scrollIntoView({ block: "nearest", inline: "nearest" });
  1197	    }
  1198	  }, [controllerOverlayOpen]);
  1199	
  1200	  const applyVariantSelections = useCallback((catalog: GameInfo[]): void => {
  1201	    setVariantByGameId((prev) => mergeVariantSelections(prev, catalog));
  1202	  }, []);
  1203	
  1204	  const clearQueueAdStartWatchdogs = useCallback((): void => {
  1205	    if (adForcePlayTimeoutRef.current) {
  1206	      clearTimeout(adForcePlayTimeoutRef.current);
  1207	      adForcePlayTimeoutRef.current = null;
  1208	    }
  1209	    if (adStartTimeoutRef.current) {
  1210	      clearTimeout(adStartTimeoutRef.current);
  1211	      adStartTimeoutRef.current = null;
  1212	    }
  1213	  }, []);
  1214	
  1215	  const clearQueueAdProgressWatchdog = useCallback((): void => {
  1216	    if (adProgressIntervalRef.current) {
  1217	      clearInterval(adProgressIntervalRef.current);
  1218	      adProgressIntervalRef.current = null;
  1219	    }
  1220	    adLastProgressTsRef.current = null;
  1221	  }, []);
  1222	
  1223	  const clearQueueAdWatchdogs = useCallback((): void => {
  1224	    clearQueueAdStartWatchdogs();
  1225	    clearQueueAdProgressWatchdog();
  1226	  }, [clearQueueAdProgressWatchdog, clearQueueAdStartWatchdogs]);
  1227	
  1228	  const resetQueueAdMetrics = useCallback((adId: string): void => {
  1229	    adMetricsRef.current[adId] = {
  1230	      startedAtMs: null,
  1231	      wasPausedAtLeastOnce: false,
  1232	    };
  1233	  }, []);
  1234	
  1235	  const markQueueAdStarted = useCallback((adId: string): void => {
  1236	    const current = adMetricsRef.current[adId];
  1237	    adMetricsRef.current[adId] = {
  1238	      startedAtMs: current?.startedAtMs ?? Date.now(),
  1239	      wasPausedAtLeastOnce: current?.wasPausedAtLeastOnce ?? false,
  1240	    };
  1241	  }, []);
  1242	
  1243	  const markQueueAdPaused = useCallback((adId: string): void => {
  1244	    const current = adMetricsRef.current[adId];
  1245	    adMetricsRef.current[adId] = {
  1246	      startedAtMs: current?.startedAtMs ?? null,
  1247	      wasPausedAtLeastOnce: true,
  1248	    };
  1249	  }, []);
  1250	
  1251	  const clearQueueAdMetrics = useCallback((adId: string): void => {
  1252	    delete adMetricsRef.current[adId];
  1253	  }, []);
  1254	
  1255	  const getQueueAdReportPayload = useCallback((adId: string, action: SessionAdAction, options?: QueueAdReportOptions) => {
  1256	    const ad = getSessionAdItems(sessionRef.current?.adState).find((candidate) => candidate.adId === adId);
  1257	    const adLengthMs = getSessionAdDurationMs(ad);
  1258	    const snapshot = queueAdPreviewRef.current?.getSnapshot();
  1259	    const watchedTimeInMs =
  1260	      action === "finish" || action === "cancel"
  1261	        ? Math.max(0, Math.round((snapshot?.currentTime ?? 0) * 1000))
  1262	        : undefined;
  1263	
  1264	    const metrics = adMetricsRef.current[adId];
  1265	    let pausedTimeInMs = 0;
  1266	    if (
  1267	      metrics?.startedAtMs &&
  1268	      metrics.wasPausedAtLeastOnce &&
  1269	      typeof adLengthMs === "number" &&
  1270	      Number.isFinite(adLengthMs) &&
  1271	      adLengthMs > 0
  1272	    ) {
  1273	      const elapsedMs = Date.now() - metrics.startedAtMs;
  1274	      if (elapsedMs > adLengthMs) {
  1275	        pausedTimeInMs = Math.round(elapsedMs - adLengthMs);
  1276	      }
  1277	    }
  1278	
  1279	    return {
  1280	      watchedTimeInMs,
  1281	      pausedTimeInMs,
  1282	      cancelReason: action === "cancel" ? options?.cancelReason : undefined,
  1283	      errorInfo: action === "cancel" ? options?.errorInfo : undefined,
  1284	    };
  1285	  }, []);
  1286	
  1287	  const armQueueAdStartWatchdogs = useCallback((adId: string): void => {
  1288	    clearQueueAdStartWatchdogs();
  1289	    adLastProgressTsRef.current = Date.now();
  1290	
  1291	    adForcePlayTimeoutRef.current = window.setTimeout(() => {
  1292	      if (activeQueueAdIdRef.current !== adId) {
  1293	        return;
  1294	      }
  1295	      const lastAction = adReportStateRef.current[adId];
  1296	      if (lastAction === "start" || lastAction === "resume" || lastAction === "finish" || lastAction === "cancel") {
  1297	        return;
  1298	      }
  1299	      void queueAdPreviewRef.current?.attemptPlayback();
  1300	    }, SESSION_AD_FORCE_PLAY_TIMEOUT_MS);
  1301	
  1302	    adStartTimeoutRef.current = window.setTimeout(() => {
  1303	      if (activeQueueAdIdRef.current !== adId) {
  1304	        return;
  1305	      }
  1306	      const lastAction = adReportStateRef.current[adId];
  1307	      if (lastAction === "start" || lastAction === "resume" || lastAction === "finish" || lastAction === "cancel") {
  1308	        return;
  1309	      }
  1310	
  1311	      clearQueueAdWatchdogs();
  1312	      queueAdPlaybackRef.current = null;
  1313	      adReportStateRef.current[adId] = "cancel";
  1314	      reportQueueAdActionRef.current(adId, "cancel", {
  1315	        cancelReason: "error",
  1316	        errorInfo: "Ad play timeout",
  1317	      });
  1318	    }, SESSION_AD_START_TIMEOUT_MS);
  1319	  }, [clearQueueAdStartWatchdogs, clearQueueAdWatchdogs]);
  1320	
  1321	  const ensureQueueAdProgressWatchdog = useCallback((adId: string): void => {
  1322	    adLastProgressTsRef.current = Date.now();
  1323	    if (adProgressIntervalRef.current) {
  1324	      return;
  1325	    }
  1326	
  1327	    adProgressIntervalRef.current = window.setInterval(() => {
  1328	      if (activeQueueAdIdRef.current !== adId) {
  1329	        return;
  1330	      }
  1331	
  1332	      const lastAction = adReportStateRef.current[adId];
  1333	      if (lastAction !== "start" && lastAction !== "resume") {
  1334	        return;
  1335	      }
  1336	
  1337	      const snapshot = queueAdPreviewRef.current?.getSnapshot();
  1338	      if (!snapshot || snapshot.paused || snapshot.ended || isSessionQueuePaused(sessionRef.current?.adState)) {
  1339	        return;
  1340	      }
  1341	
  1342	      const lastProgressTs = adLastProgressTsRef.current;
  1343	      if (!lastProgressTs || Date.now() - lastProgressTs < SESSION_AD_STUCK_TIMEOUT_MS) {
  1344	        return;
  1345	      }
  1346	
  1347	      clearQueueAdWatchdogs();
  1348	      queueAdPlaybackRef.current = null;
  1349	      adReportStateRef.current[adId] = "cancel";
  1350	      reportQueueAdActionRef.current(adId, "cancel", {
  1351	        cancelReason: "error",
  1352	        errorInfo: "Ad video is stuck",
  1353	      });
  1354	    }, SESSION_AD_PROGRESS_CHECK_INTERVAL_MS);
  1355	  }, [clearQueueAdWatchdogs]);
  1356	
  1357	  const resetLaunchRuntime = useCallback((options?: {
  1358	    keepLaunchError?: boolean;
  1359	    keepStreamingContext?: boolean;
  1360	  }): void => {
  1361	    setSession(null);
  1362	    setStreamStatus("idle");
  1363	    setQueuePosition(undefined);
  1364	    setSessionStartedAtMs(null);
  1365	    setRemoteStreamWarning(null);
  1366	    setLocalSessionTimerWarning(null);
  1367	    setEscHoldReleaseIndicator({ visible: false, progress: 0 });
  1368	    resetStatsOverlayToPreference();
  1369	    diagnosticsStore.set(defaultDiagnostics());
  1370	
  1371	    if (!options?.keepStreamingContext) {
  1372	      setStreamingGame(null);
  1373	      setStreamingStore(null);
  1374	    }
  1375	
  1376	    if (!options?.keepLaunchError) {
  1377	      setLaunchError(null);
  1378	    }
  1379	  }, [diagnosticsStore, resetStatsOverlayToPreference]);
  1380	
  1381	  // Session ref sync
  1382	  useEffect(() => {
  1383	    sessionRef.current = session;
  1384	  }, [session]);
  1385	
  1386	  useEffect(() => {
  1387	    adReportStateRef.current = {};
  1388	    adMetricsRef.current = {};
  1389	    adReportQueueRef.current = Promise.resolve();
  1390	    adMediaUrlCacheRef.current = {};
  1391	    queueAdPlaybackRef.current = null;
  1392	    activeQueueAdIdRef.current = null;
  1393	    setActiveQueueAdId(null);
  1394	    clearQueueAdWatchdogs();
  1395	  }, [clearQueueAdWatchdogs, session?.sessionId]);
  1396	
  1397	  // Keep a ref copy of `streamStatus` so async callbacks can observe latest value
  1398	  useEffect(() => {
  1399	    streamStatusRef.current = streamStatus;
  1400	  }, [streamStatus]);
  1401	
  1402	  // Broadcast minimal session/loading state for UI overlays (controller + other listeners)
  1403	  useEffect(() => {
  1404	    const detail = {
  1405	      status: streamStatus,
  1406	      queuePosition,
  1407	      launchError: launchError ? { title: launchError.title, description: launchError.description, stage: launchError.stage, codeLabel: launchError.codeLabel } : null,
  1408	      gameTitle: streamingGame?.title ?? null,
  1409	      gameCover: streamingGame?.imageUrl ?? null,
  1410	      platformStore: streamingStore ?? null,
  1411	    };
  1412	    try {
  1413	      window.dispatchEvent(new CustomEvent("opennow:session-update", { detail }));
  1414	    } catch {
  1415	      // ignore
  1416	    }
  1417	  }, [streamStatus, queuePosition, launchError, streamingGame, streamingStore]);
  1418	
  1419	  useEffect(() => {
  1420	    document.body.classList.toggle("controller-mode", controllerUiActive);
  1421	    return () => {
  1422	      document.body.classList.remove("controller-mode");
  1423	    };
  1424	  }, [controllerUiActive]);
  1425	
  1426	  useEffect(() => {
  1427	    if (!controllerUiActive || !controllerConnected) {
  1428	      document.body.classList.remove("controller-hide-cursor");
  1429	      return;
  1430	    }
  1431	
  1432	    const IDLE_MS = 1300;
  1433	    let timeoutId: number | null = null;
  1434	
  1435	    const hideCursor = () => {
  1436	      document.body.classList.add("controller-hide-cursor");
  1437	    };
  1438	
  1439	    const showCursor = () => {
  1440	      document.body.classList.remove("controller-hide-cursor");
  1441	      if (timeoutId != null) {
  1442	        window.clearTimeout(timeoutId);
  1443	      }
  1444	      timeoutId = window.setTimeout(hideCursor, IDLE_MS) as unknown as number;
  1445	    };
  1446	
  1447	    const onMouseMove = (): void => showCursor();
  1448	
  1449	    // Start visible then hide after timeout
  1450	    showCursor();
  1451	    document.addEventListener("mousemove", onMouseMove, { passive: true });
  1452	
  1453	    return () => {
  1454	      if (timeoutId != null) {
  1455	        window.clearTimeout(timeoutId);
  1456	      }
  1457	      document.removeEventListener("mousemove", onMouseMove);
  1458	      document.body.classList.remove("controller-hide-cursor");
  1459	    };
  1460	  }, [controllerConnected, controllerUiActive]);
  1461	
  1462	  // Derived state
  1463	  const selectedProvider = useMemo(() => {
  1464	    return providers.find((p) => p.idpId === providerIdpId) ?? authSession?.provider ?? null;
  1465	  }, [providers, providerIdpId, authSession]);
  1466	
  1467	  const effectiveStreamingBaseUrl = useMemo(() => {
  1468	    if (settings.region.trim()) {
  1469	      return settings.region;
  1470	    }
  1471	    return selectedProvider?.streamingServiceUrl ?? "";
  1472	  }, [selectedProvider, settings.region]);
  1473	
  1474	  const reportQueueAdAction = useCallback((adId: string, action: SessionAdAction, options?: QueueAdReportOptions): void => {
  1475	    const currentSession = sessionRef.current;
  1476	    if (!currentSession) {
  1477	      return;
  1478	    }
  1479	
  1480	    const reportPayload = getQueueAdReportPayload(adId, action, options);
  1481	
  1482	    const request = {
  1483	      token: authSession?.tokens.idToken ?? authSession?.tokens.accessToken ?? undefined,
  1484	      streamingBaseUrl: currentSession.streamingBaseUrl ?? effectiveStreamingBaseUrl,
  1485	      serverIp: currentSession.serverIp,
  1486	      zone: currentSession.zone,
  1487	      sessionId: currentSession.sessionId,
  1488	      clientId: currentSession.clientId,
  1489	      deviceId: currentSession.deviceId,
  1490	      adId,
  1491	      action,
  1492	      ...reportPayload,
  1493	    };
  1494	
  1495	    adReportQueueRef.current = adReportQueueRef.current
  1496	      .catch(() => undefined)
  1497	      .then(async () => {
  1498	        if (sessionRef.current?.sessionId !== request.sessionId) {
  1499	          return;
  1500	        }
  1501	
  1502	        try {
  1503	          console.log(
  1504	            `[QueueAds] Sending ad update: action=${action}, adId=${adId}, sessionId=${request.sessionId}, zone=${request.zone}, ` +
  1505	              `serverIp=${request.serverIp}, queuePosition=${sessionRef.current?.queuePosition ?? "n/a"}, ` +
  1506	              `watchedTimeInMs=${request.watchedTimeInMs ?? "n/a"}, pausedTimeInMs=${request.pausedTimeInMs ?? 0}, ` +
  1507	                `cancelReason=${request.cancelReason ?? "n/a"}, errorInfo=${request.errorInfo ?? "n/a"}`,
  1508	          );
  1509	          const updated = await window.openNow.reportSessionAd(request);
  1510	          if (sessionRef.current?.sessionId !== updated.sessionId) {
  1511	            return;
  1512	          }
  1513	
  1514	          console.log(
  1515	            `[QueueAds] Ad update succeeded: action=${action}, adId=${adId}, sessionId=${updated.sessionId}, ` +
  1516	              `status=${updated.status}, queuePosition=${updated.queuePosition ?? "n/a"}`,
  1517	          );
  1518	          console.log(
  1519	            `[QueueAds] Returned ad state: sessionId=${updated.sessionId}, adsRequired=${isSessionAdsRequired(updated.adState)}, ` +
  1520	              `queuePaused=${getSessionAdOpportunity(updated.adState)?.queuePaused ?? false}, gracePeriodSeconds=${getSessionAdOpportunity(updated.adState)?.gracePeriodSeconds ?? "n/a"}, ` +
  1521	              `adCount=${getSessionAdItems(updated.adState).length}`,
  1522	          );
  1523	
  1524	          setSession((previous) => {
  1525	            if (!previous || previous.sessionId !== updated.sessionId) {
  1526	              return previous;
  1527	            }
  1528	            return mergePolledSessionState(previous, updated);
  1529	          });
  1530	          setQueuePosition(updated.queuePosition);
  1531	          const updatedAd = getSessionAdItems(updated.adState).find((candidate) => candidate.adId === adId);
  1532	          const updatedAdMediaUrl = getPreferredSessionAdMediaUrl(updatedAd);
  1533	          if (updatedAdMediaUrl) {
  1534	            adMediaUrlCacheRef.current[adId] = updatedAdMediaUrl;
  1535	          }
  1536	
  1537	          if (action === "finish" || action === "cancel") {
  1538	            clearQueueAdMetrics(adId);
  1539	            if (queueAdPlaybackRef.current?.adId === adId) {
  1540	              queueAdPlaybackRef.current = null;
  1541	            }
  1542	          }
  1543	        } catch (error) {
  1544	          console.warn(`[QueueAds] Failed to report ${action} for ${adId}:`, error);
  1545	
  1546	          if (action === "finish") {
  1547	            // The server rejected the finish (e.g. session was re-queued mid-ad,
  1548	            // invalidating the ad token). Clear the local ad list so the player
  1549	            // falls back to the placeholder card while the server sends fresh
  1550	            // creatives in the next poll. Setting serverSentEmptyAds=false prevents
  1551	            // mergeAdState from restoring the now-stale ad URLs from the previous
  1552	            // state captured in the polling loop's latestSession snapshot.
  1553	            console.log(`[QueueAds] finish rejected — clearing local ad list for adId=${adId}`);
  1554	            delete adReportStateRef.current[adId];
  1555	            clearQueueAdMetrics(adId);
  1556	            if (queueAdPlaybackRef.current?.adId === adId) {
  1557	              queueAdPlaybackRef.current = null;
  1558	            }
  1559	            setSession((previous) => {
  1560	              if (!previous || !previous.adState) {
  1561	                return previous;
  1562	              }
  1563	              return {
  1564	                ...previous,
  1565	                adState: { ...previous.adState, sessionAds: [], ads: [], serverSentEmptyAds: false },
  1566	              };
  1567	            });
  1568	          }
  1569	        }
  1570	      });
  1571	  }, [authSession, clearQueueAdMetrics, effectiveStreamingBaseUrl, getQueueAdReportPayload]);
  1572	
  1573	  const handleQueueAdPlaybackEvent = useCallback((event: QueueAdPlaybackEvent, adId: string): void => {
  1574	    const currentSession = sessionRef.current;
  1575	    const currentAd = getSessionAdItems(currentSession?.adState).find((ad) => ad.adId === adId);
  1576	    if (!currentAd) {
  1577	      return;
  1578	    }
  1579	
  1580	    const lastAction = adReportStateRef.current[adId];
  1581	
  1582	    if (event === "loadstart") {
  1583	      activeQueueAdIdRef.current = adId;
  1584	      setActiveQueueAdId((previous) => (previous === adId ? previous : adId));
  1585	      resetQueueAdMetrics(adId);
  1586	      queueAdPlaybackRef.current = null;
  1587	      clearQueueAdProgressWatchdog();
  1588	      armQueueAdStartWatchdogs(adId);
  1589	      return;
  1590	    }
  1591	
  1592	    if (event === "timeupdate") {
  1593	      adLastProgressTsRef.current = Date.now();
  1594	      return;
  1595	    }
  1596	
  1597	    if (event === "playing") {
  1598	      activeQueueAdIdRef.current = adId;
  1599	      setActiveQueueAdId((previous) => (previous === adId ? previous : adId));
  1600	      clearQueueAdStartWatchdogs();
  1601	      ensureQueueAdProgressWatchdog(adId);
  1602	      queueAdPlaybackRef.current = { adId, phase: "playing" };
  1603	
  1604	      const nextAction = getNextAdReportAction(lastAction, "playing");
  1605	      if (!nextAction) {
  1606	        return;
  1607	      }
  1608	      if (nextAction === "start") {
  1609	        markQueueAdStarted(adId);
  1610	      }
  1611	      adReportStateRef.current[adId] = nextAction;
  1612	      reportQueueAdAction(adId, nextAction);
  1613	      return;
  1614	    }
  1615	
  1616	    if (event === "paused") {
  1617	      if (queueAdPlaybackRef.current?.adId === adId) {
  1618	        queueAdPlaybackRef.current = null;
  1619	      }
  1620	      const nextAction = getNextAdReportAction(lastAction, "paused");
  1621	      if (nextAction) {
  1622	        markQueueAdPaused(adId);
  1623	        adReportStateRef.current[adId] = nextAction;
  1624	        reportQueueAdAction(adId, nextAction);
  1625	      }
  1626	      return;
  1627	    }
  1628	
  1629	    if (event === "ended") {
  1630	      clearQueueAdWatchdogs();
  1631	      if (lastAction === "finish" || lastAction === "cancel") {
  1632	        return;
  1633	      }
  1634	
  1635	      const nextAd = getNextQueueAd(currentSession?.adState, adId);
  1636	      if (nextAd) {
  1637	        activeQueueAdIdRef.current = nextAd.adId;
  1638	        setActiveQueueAdId((previous) => (previous === nextAd.adId ? previous : nextAd.adId));
  1639	      }
  1640	
  1641	      queueAdPlaybackRef.current = { adId, phase: "finishing" };
  1642	      const nextAction = getNextAdReportAction(lastAction, "ended");
  1643	      if (!nextAction) {
  1644	        return;
  1645	      }
  1646	      adReportStateRef.current[adId] = nextAction;
  1647	      reportQueueAdAction(adId, nextAction);
  1648	      return;
  1649	    }
  1650	
  1651	    if (event === "error") {
  1652	      clearQueueAdWatchdogs();
  1653	      if (lastAction === "finish" || lastAction === "cancel") {
  1654	        return;
  1655	      }
  1656	      queueAdPlaybackRef.current = null;
  1657	      adReportStateRef.current[adId] = "cancel";
  1658	      reportQueueAdAction(adId, "cancel", {
  1659	        cancelReason: "error",
  1660	        errorInfo: "Error loading url",
  1661	      });
  1662	    }
  1663	  }, [armQueueAdStartWatchdogs, clearQueueAdProgressWatchdog, clearQueueAdStartWatchdogs, clearQueueAdWatchdogs, ensureQueueAdProgressWatchdog, markQueueAdPaused, markQueueAdStarted, reportQueueAdAction, resetQueueAdMetrics]);
  1664	
  1665	  // Keep the ref in sync so timer callbacks always use the latest auth state.
  1666	  useEffect(() => {
  1667	    reportQueueAdActionRef.current = reportQueueAdAction;
  1668	  }, [reportQueueAdAction]);
  1669	
  1670	  const loadSubscriptionInfo = useCallback(
  1671	    async (session: AuthSession): Promise<void> => {
  1672	      const token = session.tokens.idToken ?? session.tokens.accessToken;
  1673	      const subscription = await window.openNow.fetchSubscription({
  1674	        token,
  1675	        providerStreamingBaseUrl: session.provider.streamingServiceUrl,
  1676	        userId: session.user.userId,
  1677	      });
  1678	      setSubscriptionInfo(subscription);
  1679	    },
  1680	    [],
  1681	  );
  1682	
  1683	  const refreshSavedAccounts = useCallback(async (): Promise<SavedAccount[]> => {
  1684	    const accounts = await window.openNow.getSavedAccounts();
  1685	    setSavedAccounts(accounts);
  1686	    return accounts;
  1687	  }, []);
  1688	
  1689	  const refreshNavbarActiveSession = useCallback(async (sessionOverride?: AuthSession): Promise<void> => {
  1690	    const session = sessionOverride ?? authSession;
  1691	    if (!session) {
  1692	      setNavbarActiveSession(null);
  1693	      return;
  1694	    }
  1695	    const token = session.tokens.idToken ?? session.tokens.accessToken;
  1696	    if (!token || !effectiveStreamingBaseUrl) {
  1697	      setNavbarActiveSession(null);
  1698	      return;
  1699	    }
  1700	    try {
  1701	      const activeSessions = await window.openNow.getActiveSessions(token, effectiveStreamingBaseUrl);
  1702	      const candidate = activeSessions.find((entry) => entry.status === 3 || entry.status === 2) ?? null;
  1703	      setNavbarActiveSession(candidate);
  1704	    } catch (error) {
  1705	      console.warn("Failed to refresh active sessions:", error);
  1706	    }
  1707	  }, [authSession, effectiveStreamingBaseUrl]);
  1708	
  1709	  const allKnownGames = useMemo(() => [...games, ...libraryGames], [games, libraryGames]);
  1710	
  1711	  const gameTitleByAppId = useMemo(() => {
  1712	    const titles = new Map<number, string>();
  1713	
  1714	    for (const game of allKnownGames) {
  1715	      const idsForGame = new Set<number>();
  1716	      const launchId = parseNumericId(game.launchAppId);
  1717	      if (launchId !== null) {
  1718	        idsForGame.add(launchId);
  1719	      }
  1720	      for (const variant of game.variants) {
  1721	        const variantId = parseNumericId(variant.id);
  1722	        if (variantId !== null) {
  1723	          idsForGame.add(variantId);
  1724	        }
  1725	      }
  1726	      for (const appId of idsForGame) {
  1727	        if (!titles.has(appId)) {
  1728	          titles.set(appId, game.title);
  1729	        }
  1730	      }
  1731	    }
  1732	
  1733	    return titles;
  1734	  }, [allKnownGames]);
  1735	
  1736	  const findGameContextForSession = useCallback((activeSession: ActiveSessionInfo) => {
  1737	    return findSessionContextForAppId(allKnownGames, variantByGameId, activeSession.appId);
  1738	  }, [allKnownGames, variantByGameId]);
  1739	
  1740	  const stopSessionByTarget = useCallback(async (
  1741	    target: Pick<SessionStopRequest, "sessionId" | "zone" | "streamingBaseUrl" | "serverIp" | "clientId" | "deviceId"> | null | undefined,
  1742	  ): Promise<boolean> => {
  1743	    if (!target) {
  1744	      return false;
  1745	    }
  1746	    const token = authSession?.tokens.idToken ?? authSession?.tokens.accessToken;
  1747	    if (!token) {
  1748	      console.warn("Skipping session stop: missing auth token");
  1749	      return false;
  1750	    }
  1751	    await window.openNow.stopSession({
  1752	      token,
  1753	      streamingBaseUrl: target.streamingBaseUrl,
  1754	      serverIp: target.serverIp,
  1755	      zone: target.zone,
  1756	      sessionId: target.sessionId,
  1757	      clientId: target.clientId,
  1758	      deviceId: target.deviceId,
  1759	    });
  1760	    return true;
  1761	  }, [authSession]);
  1762	
  1763	  useEffect(() => {
  1764	    if (!startupRefreshNotice) return;
  1765	    const timer = window.setTimeout(() => setStartupRefreshNotice(null), 7000);
  1766	    return () => window.clearTimeout(timer);
  1767	  }, [startupRefreshNotice]);
  1768	
  1769	  useEffect(() => {
  1770	    if (!authSession || streamStatus !== "idle") {
  1771	      return;
  1772	    }
  1773	    void refreshNavbarActiveSession();
  1774	    const timer = window.setInterval(() => {
  1775	      void refreshNavbarActiveSession();
  1776	    }, 10000);
  1777	    return () => window.clearInterval(timer);
  1778	  }, [authSession, refreshNavbarActiveSession, streamStatus]);
  1779	
  1780	  // Initialize app
  1781	  useEffect(() => {
  1782	    if (hasInitializedRef.current) return;
  1783	    hasInitializedRef.current = true;
  1784	
  1785	    const initialize = async () => {
  1786	      try {
  1787	        // Load settings first
  1788	        const loadedSettings = await window.openNow.getSettings();
  1789	        setSettings(loadedSettings);
  1790	        setShowStatsOverlay(loadedSettings.showStatsOnLaunch);
  1791	        setSettingsLoaded(true);
  1792	
  1793	        // Load providers and session (refresh only if token is near expiry)
  1794	        setStartupStatusMessage("Restoring saved session...");
  1795	        const [providerList, sessionResult, accounts] = await Promise.all([
  1796	          window.openNow.getLoginProviders(),
  1797	          window.openNow.getAuthSession(),
  1798	          window.openNow.getSavedAccounts(),
  1799	        ]);
  1800	        const persistedSession = sessionResult.session;
  1801	
  1802	        if (sessionResult.refresh.outcome === "refreshed") {
  1803	          setStartupRefreshNotice({
  1804	            tone: "success",
  1805	            text: "Session restored. Token refreshed.",
  1806	          });
  1807	          setStartupStatusMessage("Token refreshed. Loading your account...");
  1808	        } else if (sessionResult.refresh.outcome === "failed") {
  1809	          setStartupRefreshNotice({
  1810	            tone: "warn",
  1811	            text: "Token refresh failed. Using saved session token.",
  1812	          });
  1813	          setStartupStatusMessage("Token refresh failed. Continuing with saved session...");
  1814	        } else if (sessionResult.refresh.outcome === "missing_refresh_token") {
  1815	          setStartupStatusMessage("Saved session has no refresh token. Continuing...");
  1816	        } else if (persistedSession) {
  1817	          setStartupStatusMessage("Session restored.");
  1818	        } else {
  1819	          setStartupStatusMessage("No saved session found.");
  1820	        }
  1821	
  1822	        // Load persisted variant selections from localStorage before applying defaults
  1823	        try {
  1824	          const raw = localStorage.getItem(VARIANT_SELECTION_LOCALSTORAGE_KEY);
  1825	          if (raw) {
  1826	            const parsed = JSON.parse(raw);
  1827	            if (parsed && typeof parsed === "object") {
  1828	              setVariantByGameId(parsed as Record<string, string>);
  1829	            }
  1830	          }
  1831	        } catch (e) {
  1832	          // ignore parse/storage errors
  1833	        }
  1834	
  1835	        // Update isInitializing FIRST so UI knows we're done loading
  1836	        setIsInitializing(false);
  1837	        setProviders(providerList);
  1838	        setAuthSession(persistedSession);
  1839	        setSavedAccounts(accounts);
  1840	
  1841	        const activeProviderId = persistedSession?.provider?.idpId ?? providerList[0]?.idpId ?? "";
  1842	        setProviderIdpId(activeProviderId);
  1843	
  1844	        if (persistedSession) {
  1845	          await loadSessionRuntimeData(persistedSession);
  1846	        } else {
  1847	          setRegions([]);
  1848	          setGames([]);
  1849	          setLibraryGames([]);
  1850	          setSubscriptionInfo(null);
  1851	          setCatalogFilterGroups([]);
  1852	          setCatalogSortOptions([]);
  1853	          setCatalogTotalCount(0);
  1854	          setCatalogSupportedCount(0);
  1855	        }
  1856	      } catch (error) {
  1857	        console.error("Initialization failed:", error);
  1858	        setStartupStatusMessage("Session restore failed. Please sign in again.");
  1859	        // Always set isInitializing to false even on error
  1860	        setIsInitializing(false);
  1861	      }
  1862	    };
  1863	
  1864	    void initialize();
  1865	  }, [catalogFilterKey, catalogSelectedSortId, loadSessionRuntimeData]);
  1866	
  1867	  useEffect(() => {
  1868	    saveStoredCodecResults(codecResults);
  1869	  }, [codecResults]);
  1870	
  1871	  useEffect(() => {
  1872	    saveCatalogPreferences({ sortId: catalogSelectedSortId, filterIds: catalogSelectedFilterIds });
  1873	  }, [catalogSelectedSortId, catalogSelectedFilterIds]);
  1874	
  1875	  useEffect(() => {
  1876	    if (codecResults || codecTesting || codecStartupTestAttemptedRef.current) {
  1877	      return;
  1878	    }
  1879	    codecStartupTestAttemptedRef.current = true;
  1880	    void runCodecTest();
  1881	  }, [codecResults, codecTesting, runCodecTest]);
  1882	
  1883	  // Auto-load controller library at startup if enabled
  1884	  useEffect(() => {
  1885	    if (isInitializing || !authSession || !settings.controllerMode || !settings.autoLoadControllerLibrary || currentPage !== "home") {
  1886	      return;
  1887	    }
  1888	    setCurrentPage("library");
  1889	  }, [isInitializing, authSession, settings.controllerMode, settings.autoLoadControllerLibrary, currentPage]);
  1890	
  1891	  const shortcuts = useMemo(() => {
  1892	    const parseWithFallback = (value: string, fallback: string) => {
  1893	      const parsed = normalizeShortcut(value);
  1894	      return parsed.valid ? parsed : normalizeShortcut(fallback);
  1895	    };
  1896	    const toggleStats = parseWithFallback(settings.shortcutToggleStats, DEFAULT_SHORTCUTS.shortcutToggleStats);
  1897	    const togglePointerLock = parseWithFallback(settings.shortcutTogglePointerLock, DEFAULT_SHORTCUTS.shortcutTogglePointerLock);
  1898	    const toggleFullscreen = parseWithFallback(settings.shortcutToggleFullscreen, DEFAULT_SHORTCUTS.shortcutToggleFullscreen);
  1899	    const stopStream = parseWithFallback(settings.shortcutStopStream, DEFAULT_SHORTCUTS.shortcutStopStream);
  1900	    const toggleAntiAfk = parseWithFallback(settings.shortcutToggleAntiAfk, DEFAULT_SHORTCUTS.shortcutToggleAntiAfk);
  1901	    const toggleMicrophone = parseWithFallback(settings.shortcutToggleMicrophone, DEFAULT_SHORTCUTS.shortcutToggleMicrophone);
  1902	    const screenshot = parseWithFallback(settings.shortcutScreenshot, DEFAULT_SHORTCUTS.shortcutScreenshot);
  1903	    const recording = parseWithFallback(settings.shortcutToggleRecording, DEFAULT_SHORTCUTS.shortcutToggleRecording);
  1904	    return { toggleStats, togglePointerLock, toggleFullscreen, stopStream, toggleAntiAfk, toggleMicrophone, screenshot, recording };
  1905	  }, [
  1906	    settings.shortcutToggleStats,
  1907	    settings.shortcutTogglePointerLock,
  1908	    settings.shortcutToggleFullscreen,
  1909	    settings.shortcutStopStream,
  1910	    settings.shortcutToggleAntiAfk,
  1911	    settings.shortcutToggleMicrophone,
  1912	    settings.shortcutScreenshot,
  1913	    settings.shortcutToggleRecording,
  1914	  ]);
  1915	
  1916	  const setSessionFullscreen = useCallback(async (nextFullscreen: boolean) => {
  1917	    try {
  1918	      if (nextFullscreen) {
  1919	        if (!document.fullscreenElement) {
  1920	          await document.documentElement.requestFullscreen();
  1921	        }
  1922	      } else if (document.fullscreenElement) {
  1923	        await document.exitFullscreen();
  1924	      }
  1925	    } catch {}
  1926	
  1927	    try {
  1928	      await window.openNow.setFullscreen(nextFullscreen);
  1929	    } catch (error) {
  1930	      console.warn(`Failed to sync native fullscreen state (${nextFullscreen ? "enter" : "exit"}):`, error);
  1931	    }
  1932	  }, []);
  1933	
  1934	  const toggleSessionFullscreen = useCallback(async () => {
  1935	    await setSessionFullscreen(!document.fullscreenElement);
  1936	  }, [setSessionFullscreen]);
  1937	
  1938	  const requestEscLockedPointerCapture = useCallback(async (target: HTMLVideoElement) => {
  1939	    const lockTarget = (target.parentElement as HTMLElement | null) ?? target;
  1940	    const requestPointerLockCompat = async (
  1941	      options?: { unadjustedMovement?: boolean },
  1942	    ): Promise<void> => {
  1943	      const maybePromise = lockTarget.requestPointerLock(options as any) as unknown;
  1944	      if (maybePromise && typeof (maybePromise as Promise<void>).then === "function") {
  1945	        await (maybePromise as Promise<void>);
  1946	      }
  1947	    };
  1948	
  1949	    if (settings.autoFullScreen && !document.fullscreenElement) {
  1950	      await setSessionFullscreen(true);
  1951	    }
  1952	
  1953	    const nav = navigator as any;
  1954	    if (document.fullscreenElement && nav.keyboard?.lock) {
  1955	      await nav.keyboard.lock([
  1956	        "Escape", "F11", "BrowserBack", "BrowserForward", "BrowserRefresh",
  1957	      ]).catch(() => {});
  1958	    }
  1959	
  1960	    await requestPointerLockCompat({ unadjustedMovement: true })
  1961	      .catch((err: DOMException) => {
  1962	        if (err.name === "NotSupportedError") {
  1963	          return requestPointerLockCompat();
  1964	        }
  1965	        throw err;
  1966	      })
  1967	      .catch(() => {});
  1968	  }, [setSessionFullscreen, settings.autoFullScreen]);
  1969	
  1970	  const handleRequestPointerLock = useCallback(() => {
  1971	    if (videoRef.current) {
  1972	      void requestEscLockedPointerCapture(videoRef.current);
  1973	    }
  1974	  }, [requestEscLockedPointerCapture]);
  1975	
  1976	  const resolveExitPrompt = useCallback((confirmed: boolean) => {
  1977	    const resolver = exitPromptResolverRef.current;
  1978	    exitPromptResolverRef.current = null;
  1979	    setExitPrompt((prev) => (prev.open ? { ...prev, open: false } : prev));
  1980	    resolver?.(confirmed);
  1981	  }, []);
  1982	
  1983	  const requestExitPrompt = useCallback((gameTitle: string): Promise<boolean> => {
  1984	    return new Promise((resolve) => {
  1985	      if (exitPromptResolverRef.current) {
  1986	        // Close any previous pending prompt to avoid dangling promises.
  1987	        exitPromptResolverRef.current(false);
  1988	      }
  1989	      exitPromptResolverRef.current = resolve;
  1990	      setExitPrompt({
  1991	        open: true,
  1992	        gameTitle: gameTitle || "this game",
  1993	      });
  1994	    });
  1995	  }, []);
  1996	
  1997	  const handleExitPromptConfirm = useCallback(() => {
  1998	    resolveExitPrompt(true);
  1999	  }, [resolveExitPrompt]);
  2000	
  2001	  const handleExitPromptCancel = useCallback(() => {
  2002	    resolveExitPrompt(false);
  2003	  }, [resolveExitPrompt]);
  2004	
  2005	  useEffect(() => {
  2006	    return () => {
  2007	      if (exitPromptResolverRef.current) {
  2008	        exitPromptResolverRef.current(false);
  2009	        exitPromptResolverRef.current = null;
  2010	      }
  2011	    };
  2012	  }, []);
  2013	
  2014	  // Listen for F11 fullscreen toggle from main process (uses W3C Fullscreen API
  2015	  // so navigator.keyboard.lock() can capture Escape in fullscreen)
  2016	  useEffect(() => {
  2017	    const unsubscribe = window.openNow.onToggleFullscreen(() => {
  2018	      void toggleSessionFullscreen();
  2019	    });
  2020	    return () => unsubscribe();
  2021	  }, [toggleSessionFullscreen]);
  2022	
  2023	  const autoFullscreenRequestedRef = useRef(false);
  2024	
  2025	  useEffect(() => {
  2026	    const isSessionConnecting = streamStatus === "connecting" || streamStatus === "streaming";
  2027	    if (!settings.autoFullScreen || !isSessionConnecting) {
  2028	      autoFullscreenRequestedRef.current = false;
  2029	      return;
  2030	    }
  2031	
  2032	    if (autoFullscreenRequestedRef.current || document.fullscreenElement) {
  2033	      return;
  2034	    }
  2035	
  2036	    autoFullscreenRequestedRef.current = true;
  2037	    void setSessionFullscreen(true);
  2038	  }, [setSessionFullscreen, settings.autoFullScreen, streamStatus]);
  2039	
  2040	  // Anti-AFK interval
  2041	  useEffect(() => {
  2042	    if (!antiAfkEnabled || streamStatus !== "streaming") return;
  2043	
  2044	    const interval = window.setInterval(() => {
  2045	      clientRef.current?.sendAntiAfkPulse();
  2046	    }, 240000); // 4 minutes
  2047	
  2048	    return () => clearInterval(interval);
  2049	  }, [antiAfkEnabled, streamStatus]);
  2050	
  2051	  // Periodically re-sync subscription playtime from backend while streaming.
  2052	  useEffect(() => {
  2053	    if (streamStatus !== "streaming" || !authSession) {
  2054	      return;
  2055	    }
  2056	
  2057	    let cancelled = false;
  2058	
  2059	    const syncPlaytime = async (): Promise<void> => {
  2060	      try {
  2061	        await loadSubscriptionInfo(authSession);
  2062	      } catch (error) {
  2063	        if (!cancelled) {
  2064	          console.warn("Failed to re-sync subscription playtime:", error);
  2065	        }
  2066	      }
  2067	    };
  2068	
  2069	    void syncPlaytime();
  2070	    const timer = window.setInterval(() => {
  2071	      void syncPlaytime();
  2072	    }, PLAYTIME_RESYNC_INTERVAL_MS);
  2073	
  2074	    return () => {
  2075	      cancelled = true;
  2076	      window.clearInterval(timer);
  2077	    };
  2078	  }, [authSession, loadSubscriptionInfo, streamStatus]);
  2079	
  2080	  // Restore focus to video element when navigating away from Settings during streaming
  2081	  useEffect(() => {
  2082	    if (streamStatus === "streaming" && currentPage !== "settings" && videoRef.current) {
  2083	      // Small delay to let React finish rendering the new page
  2084	      const timer = window.setTimeout(() => {
  2085	        if (videoRef.current && document.activeElement !== videoRef.current) {
  2086	          videoRef.current.focus();
  2087	          console.log("[App] Restored focus to video element after leaving Settings");
  2088	        }
  2089	      }, 50);
  2090	      return () => clearTimeout(timer);
  2091	    }
  2092	  }, [currentPage, streamStatus]);
  2093	
  2094	
  2095	  useEffect(() => {
  2096	    if (streamStatus !== "streaming" || sessionStartedAtMs !== null) {
  2097	      return;
  2098	    }
  2099	
  2100	    const evaluate = () => {
  2101	      const snapshot = diagnosticsStore.getSnapshot();
  2102	      const hasLiveFrames =
  2103	        snapshot.framesDecoded > 0 || snapshot.framesReceived > 0 || snapshot.renderFps > 0;
  2104	      if (hasLiveFrames) {
  2105	        setSessionStartedAtMs(Date.now());
  2106	      }
  2107	    };
  2108	
  2109	    evaluate();
  2110	    const unsubscribe = diagnosticsStore.subscribe(evaluate);
  2111	    return unsubscribe;
  2112	  }, [sessionStartedAtMs, streamStatus]);
  2113	
  2114	  useEffect(() => {
  2115	    if (freeTierSessionRemainingSeconds === null) {
  2116	      previousFreeTierRemainingSecondsRef.current = null;
  2117	      setLocalSessionTimerWarning(null);
  2118	      return;
  2119	    }
  2120	
  2121	    const previousSeconds = previousFreeTierRemainingSecondsRef.current;
  2122	
  2123	    if (hasCrossedWarningThreshold(previousSeconds, freeTierSessionRemainingSeconds, FREE_TIER_FINAL_MINUTE_WARNING_SECONDS)) {
  2124	      setLocalSessionTimerWarning({ stage: "free-tier-final-minute", shownAtMs: Date.now() });
  2125	    } else if (hasCrossedWarningThreshold(previousSeconds, freeTierSessionRemainingSeconds, FREE_TIER_15_MIN_WARNING_SECONDS)) {
  2126	      setLocalSessionTimerWarning({ stage: "free-tier-15m", shownAtMs: Date.now() });
  2127	    } else if (hasCrossedWarningThreshold(previousSeconds, freeTierSessionRemainingSeconds, FREE_TIER_30_MIN_WARNING_SECONDS)) {
  2128	      setLocalSessionTimerWarning({ stage: "free-tier-30m", shownAtMs: Date.now() });
  2129	    }
  2130	
  2131	    previousFreeTierRemainingSecondsRef.current = freeTierSessionRemainingSeconds;
  2132	  }, [freeTierSessionRemainingSeconds]);
  2133	
  2134	  useEffect(() => {
  2135	    if (!localSessionTimerWarning) return;
  2136	
  2137	    const warning = localSessionTimerWarning;
  2138	    const remainingMs = Math.max(0, warning.shownAtMs + STREAM_WARNING_VISIBILITY_MS - Date.now());
  2139	    const timer = window.setTimeout(() => {
  2140	      setLocalSessionTimerWarning((current) => (current === warning ? null : current));
  2141	    }, remainingMs);
  2142	    return () => window.clearTimeout(timer);
  2143	  }, [localSessionTimerWarning]);
  2144	
  2145	  useEffect(() => {
  2146	    if (!remoteStreamWarning) return;
  2147	
  2148	    const warning = remoteStreamWarning;
  2149	    const timer = window.setTimeout(() => {
  2150	      setRemoteStreamWarning((current) => (current === warning ? null : current));
  2151	    }, STREAM_WARNING_VISIBILITY_MS);
  2152	    return () => window.clearTimeout(timer);
  2153	  }, [remoteStreamWarning]);
  2154	
  2155	  // Signaling events
  2156	  useEffect(() => {
  2157	    const unsubscribe = window.openNow.onSignalingEvent(async (event: MainToRendererSignalingEvent) => {
  2158	      console.log(`[App] Signaling event: ${event.type}`, event.type === "offer" ? `(SDP ${event.sdp.length} chars)` : "", event.type === "remote-ice" ? event.candidate : "");
  2159	      try {
  2160	        if (event.type === "offer") {
  2161	          const activeSession = sessionRef.current;
  2162	          if (!activeSession) {
  2163	            console.warn("[App] Received offer but no active session in sessionRef!");
  2164	            return;
  2165	          }
  2166	          console.log("[App] Active session for offer:", JSON.stringify({
  2167	            sessionId: activeSession.sessionId,
  2168	            serverIp: activeSession.serverIp,
  2169	            signalingServer: activeSession.signalingServer,
  2170	            mediaConnectionInfo: activeSession.mediaConnectionInfo,
  2171	            iceServersCount: activeSession.iceServers?.length,
  2172	          }));
  2173	
  2174	          if (!clientRef.current && videoRef.current && audioRef.current) {
  2175	            clientRef.current = new GfnWebRtcClient({
  2176	              videoElement: videoRef.current,
  2177	              audioElement: audioRef.current,
  2178	              autoFullScreen: settings.autoFullScreen,
  2179	              microphoneMode: settings.microphoneMode,
  2180	              microphoneDeviceId: settings.microphoneDeviceId || undefined,
  2181	              mouseSensitivity: settings.mouseSensitivity,
  2182	              mouseAcceleration: settings.mouseAcceleration,
  2183	              onLog: (line: string) => console.log(`[WebRTC] ${line}`),
  2184	              onStats: (stats) => diagnosticsStore.set(stats),
  2185	              onEscHoldProgress: (visible, progress) => {
  2186	                setEscHoldReleaseIndicator({ visible, progress });
  2187	              },
  2188	              onTimeWarning: (warning) => {
  2189	                setRemoteStreamWarning({
  2190	                  code: warning.code,
  2191	                  message: warningMessage(warning.code),
  2192	                  tone: warningTone(warning.code),
  2193	                  secondsLeft: warning.secondsLeft,
  2194	                });
  2195	              },
  2196	              onMicStateChange: (state) => {
  2197	                console.log(`[App] Mic state: ${state.state}${state.deviceLabel ? ` (${state.deviceLabel})` : ""}`);
  2198	              },
  2199	            });
  2200	            // Auto-start microphone if mode is enabled
  2201	            if (settings.microphoneMode !== "disabled") {
  2202	              void clientRef.current.startMicrophone();
  2203	            }
  2204	          }
  2205	
  2206	          if (clientRef.current) {
  2207	            await clientRef.current.handleOffer(event.sdp, activeSession, {
  2208	              codec: settings.codec,
  2209	              colorQuality: settings.colorQuality,
  2210	              resolution: settings.resolution,
  2211	              fps: settings.fps,
  2212	              maxBitrateKbps: settings.maxBitrateMbps * 1000,
  2213	            });
  2214	            setLaunchError(null);
  2215	            setStreamStatus("streaming");
  2216	          }
  2217	        } else if (event.type === "remote-ice") {
  2218	          await clientRef.current?.addRemoteCandidate(event.candidate);
  2219	        } else if (event.type === "disconnected") {
  2220	          console.warn("Signaling disconnected:", event.reason);
  2221	          clientRef.current?.dispose();
  2222	          clientRef.current = null;
  2223	          resetLaunchRuntime();
  2224	          launchInFlightRef.current = false;
  2225	        } else if (event.type === "error") {
  2226	          console.error("Signaling error:", event.message);
  2227	        }
  2228	      } catch (error) {
  2229	        console.error("Signaling event error:", error);
  2230	      }
  2231	    });
  2232	
  2233	    return () => unsubscribe();
  2234	  }, [resetLaunchRuntime, settings]);
  2235	
  2236	  // Save settings when changed
  2237	  const updateSetting = useCallback(async <K extends keyof Settings>(key: K, value: Settings[K]) => {
  2238	    setSettings((prev) => ({ ...prev, [key]: value }));
  2239	    if (settingsLoaded) {
  2240	      await window.openNow.setSetting(key, value);
  2241	    }
  2242	    // If a running client exists, push certain settings live
  2243	    if (key === "mouseSensitivity") {
  2244	      try {
  2245	        (clientRef.current as any)?.setMouseSensitivity?.(value as number);
  2246	      } catch {
  2247	        // ignore
  2248	      }
  2249	    }
  2250	    if (key === "mouseAcceleration") {
  2251	      try {
  2252	        (clientRef.current as any)?.setMouseAccelerationPercent?.(value as number);
  2253	      } catch {
  2254	        // ignore
  2255	      }
  2256	    }
  2257	    if (key === "autoFullScreen") {
  2258	      try {
  2259	        (clientRef.current as any)?.setAutoFullScreen?.(value as boolean);
  2260	      } catch {
  2261	        // ignore
  2262	      }
  2263	    }
  2264	    if (key === "maxBitrateMbps") {
  2265	      try {
  2266	        void (clientRef.current as any)?.setMaxBitrateKbps?.((value as number) * 1000);
  2267	      } catch {
  2268	        // ignore
  2269	      }
  2270	    }
  2271	  }, [settingsLoaded]);
  2272	
  2273	  const handleMouseSensitivityChange = useCallback((value: number) => {
  2274	    void updateSetting("mouseSensitivity", value);
  2275	  }, [updateSetting]);
  2276	
  2277	  const handleToggleFavoriteGame = useCallback((gameId: string): void => {
  2278	    const favorites = settings.favoriteGameIds;
  2279	    const exists = favorites.includes(gameId);
  2280	    const next = exists ? favorites.filter((id) => id !== gameId) : [...favorites, gameId];
  2281	    void updateSetting("favoriteGameIds", next);
  2282	  }, [settings.favoriteGameIds, updateSetting]);
  2283	
  2284	  const handleMouseAccelerationChange = useCallback((value: number) => {
  2285	    void updateSetting("mouseAcceleration", value);
  2286	  }, [updateSetting]);
  2287	
  2288	  const handleExitControllerMode = useCallback(() => {
  2289	    setSettings((prev) => ({
  2290	      ...prev,
  2291	      controllerMode: false,
  2292	      autoLoadControllerLibrary: false,
  2293	    }));
  2294	
  2295	    if (settingsLoaded) {
  2296	      void Promise.all([
  2297	        window.openNow.setSetting("controllerMode", false),
  2298	        window.openNow.setSetting("autoLoadControllerLibrary", false),
  2299	      ]).catch((error) => {
  2300	        console.warn("Failed to persist controller mode exit settings:", error);
  2301	      });
  2302	    }
  2303	  }, [settingsLoaded]);
  2304	
  2305	  const handleExitApp = useCallback(() => {
  2306	    void window.openNow.quitApp().catch((error) => {
  2307	      console.warn("Failed to quit application:", error);
  2308	    });
  2309	  }, []);
  2310	
  2311	  const handleMicrophoneModeChange = useCallback((value: import("@shared/gfn").MicrophoneMode) => {
  2312	    // Keep UI responsive while still surfacing persistence failures.
  2313	    void updateSetting("microphoneMode", value).catch((error) => {
  2314	      console.warn("Failed to persist microphone mode setting:", error);
  2315	    });
  2316	  }, [updateSetting]);
  2317	
  2318	  const applyCatalogBrowseResult = useCallback((catalogResult: CatalogBrowseResult): void => {
  2319	    setGames(catalogResult.games);
  2320	    setCatalogFilterGroups(catalogResult.filterGroups);
  2321	    setCatalogSortOptions(catalogResult.sortOptions);
  2322	    setCatalogSelectedSortId((previous) => previous === catalogResult.selectedSortId ? previous : catalogResult.selectedSortId);
  2323	    setCatalogSelectedFilterIds((previous) => areStringArraysEqual(previous, catalogResult.selectedFilterIds) ? previous : catalogResult.selectedFilterIds);
  2324	    setCatalogTotalCount(catalogResult.totalCount);
  2325	    setCatalogSupportedCount(catalogResult.numberSupported);
  2326	    setSelectedGameId((previous) => catalogResult.games.some((game) => game.id === previous) ? previous : (catalogResult.games[0]?.id ?? ""));
  2327	    applyVariantSelections(catalogResult.games);
  2328	  }, [applyVariantSelections]);
  2329	
  2330	  async function loadSessionRuntimeData(session: AuthSession): Promise<void> {
  2331	    const token = session.tokens.idToken ?? session.tokens.accessToken;
  2332	    const discovered = await window.openNow.getRegions({ token });
  2333	    setRegions(discovered);
  2334	
  2335	    try {
  2336	      await loadSubscriptionInfo(session);
  2337	    } catch (error) {
  2338	      console.warn("Failed to load subscription info:", error);
  2339	      setSubscriptionInfo(null);
  2340	    }
  2341	
  2342	    try {
  2343	      const [catalogResult, libGames] = await Promise.all([
  2344	        window.openNow.browseCatalog({
  2345	          token,
  2346	          providerStreamingBaseUrl: session.provider.streamingServiceUrl,
  2347	          searchQuery: "",
  2348	          sortId: catalogSelectedSortId,
  2349	          filterIds: catalogSelectedFilterIds,
  2350	        }),
  2351	        window.openNow.fetchLibraryGames({
  2352	          token,
  2353	          providerStreamingBaseUrl: session.provider.streamingServiceUrl,
  2354	        }),
  2355	      ]);
  2356	      applyCatalogBrowseResult(catalogResult);
  2357	      setLibraryGames(libGames);
  2358	      applyVariantSelections(libGames);
  2359	    } catch (catalogError) {
  2360	      console.error("Initialization games load failed:", catalogError);
  2361	      setGames([]);
  2362	      setLibraryGames([]);
  2363	      setCatalogFilterGroups([]);
  2364	      setCatalogSortOptions([]);
  2365	      setCatalogTotalCount(0);
  2366	      setCatalogSupportedCount(0);
  2367	    }
  2368	  }
  2369	
  2370	  // Login handler
  2371	  const handleLogin = useCallback(async () => {
  2372	    setIsLoggingIn(true);
  2373	    setLoginError(null);
  2374	    try {
  2375	      const session = await window.openNow.login({ providerIdpId: providerIdpId || undefined });
  2376	      setAuthSession(session);
  2377	      setProviderIdpId(session.provider.idpId);
  2378	      await refreshSavedAccounts();
  2379	      await loadSessionRuntimeData(session);
  2380	    } catch (error) {
  2381	      setLoginError(error instanceof Error ? error.message : "Login failed");
  2382	    } finally {
  2383	      setIsLoggingIn(false);
  2384	    }
  2385	  }, [loadSessionRuntimeData, providerIdpId, refreshSavedAccounts]);
  2386	
  2387	  const handleSwitchAccount = useCallback(async (userId: string) => {
  2388	    try {
  2389	      const session = await window.openNow.switchAccount(userId);
  2390	      setAuthSession(session);
  2391	      setProviderIdpId(session.provider.idpId);
  2392	      await refreshSavedAccounts();
  2393	      await loadSessionRuntimeData(session);
  2394	      await refreshNavbarActiveSession(session);
  2395	    } catch (error) {
  2396	      console.warn("Failed to switch account:", error);
  2397	      setLoginError(error instanceof Error ? error.message : "Failed to switch account");
  2398	      await refreshSavedAccounts();
  2399	      const sessionResult = await window.openNow.getAuthSession();
  2400	      setAuthSession(sessionResult.session);
  2401	      if (sessionResult.session) {
  2402	        setProviderIdpId(sessionResult.session.provider.idpId);
  2403	        await loadSessionRuntimeData(sessionResult.session);
  2404	      } else {
  2405	        setRegions([]);
  2406	        setGames([]);
  2407	        setLibraryGames([]);
  2408	        setSubscriptionInfo(null);
  2409	        setCatalogFilterGroups([]);
  2410	        setCatalogSortOptions([]);
  2411	        setCatalogTotalCount(0);
  2412	        setCatalogSupportedCount(0);
  2413	      }
  2414	    }
  2415	  }, [loadSessionRuntimeData, refreshNavbarActiveSession, refreshSavedAccounts]);
  2416	
  2417	  const handleRemoveAccount = useCallback((userId: string) => {
  2418	    setAccountToRemove(userId);
  2419	    setRemoveAccountConfirmOpen(true);
  2420	  }, []);
  2421	
  2422	  const confirmRemoveAccount = useCallback(async () => {
  2423	    if (!accountToRemove) return;
  2424	    const targetUserId = accountToRemove;
  2425	    setRemoveAccountConfirmOpen(false);
  2426	    setAccountToRemove(null);
  2427	
  2428	    await window.openNow.removeAccount(targetUserId);
  2429	    const [accounts, sessionResult] = await Promise.all([
  2430	      window.openNow.getSavedAccounts(),
  2431	      window.openNow.getAuthSession(),
  2432	    ]);
  2433	    setSavedAccounts(accounts);
  2434	    setAuthSession(sessionResult.session);
  2435	    if (sessionResult.session) {
  2436	      setProviderIdpId(sessionResult.session.provider.idpId);
  2437	      await loadSessionRuntimeData(sessionResult.session);
  2438	      return;
  2439	    }
  2440	    setRegions([]);
  2441	    setGames([]);
  2442	    setLibraryGames([]);
  2443	    setSubscriptionInfo(null);
  2444	    setCatalogFilterGroups([]);
  2445	    setCatalogSortOptions([]);
  2446	    setCatalogTotalCount(0);
  2447	    setCatalogSupportedCount(0);
  2448	  }, [accountToRemove, loadSessionRuntimeData]);
  2449	
  2450	  const handleAddAccount = useCallback(() => {
  2451	    setAuthSession(null);
  2452	    setLoginError(null);
  2453	  }, []);
  2454	
  2455	  const confirmLogout = useCallback(async () => {
  2456	    setLogoutConfirmOpen(false);
  2457	    await window.openNow.logout();
  2458	    setAuthSession(null);
  2459	    setSavedAccounts([]);
  2460	    setGames([]);
  2461	    setLibraryGames([]);
  2462	    setVariantByGameId({});
  2463	    resetLaunchRuntime();
  2464	    setNavbarActiveSession(null);
  2465	    setIsResumingNavbarSession(false);
  2466	    setSubscriptionInfo(null);
  2467	    setCurrentPage("home");
  2468	    setCatalogFilterGroups([]);
  2469	    setCatalogSortOptions([]);
  2470	    setCatalogSelectedSortId("relevance");
  2471	    setCatalogSelectedFilterIds([]);
  2472	    setCatalogTotalCount(0);
  2473	    setCatalogSupportedCount(0);
  2474	    setSelectedGameId("");
  2475	  }, [resetLaunchRuntime]);
  2476	
  2477	  // Logout handler
  2478	  const handleLogout = useCallback(() => {
  2479	    setLogoutConfirmOpen(true);
  2480	  }, []);
  2481	
  2482	  // Load games handler
  2483	  const loadGames = useCallback(async (targetSource: "main" | "library") => {
  2484	    setIsLoadingGames(true);
  2485	    try {
  2486	      const token = authSession?.tokens.idToken ?? authSession?.tokens.accessToken;
  2487	      const baseUrl = effectiveStreamingBaseUrl;
  2488	      if (!token) {
  2489	        return;
  2490	      }
  2491	
  2492	      if (targetSource === "main") {
  2493	        const catalogResult = await window.openNow.browseCatalog({
  2494	          token,
  2495	          providerStreamingBaseUrl: baseUrl,
  2496	          searchQuery,
  2497	          sortId: catalogSelectedSortId,
  2498	          filterIds: catalogSelectedFilterIds,
  2499	        });
  2500	        applyCatalogBrowseResult(catalogResult);
  2501	        return;
  2502	      }
  2503	
  2504	      const result = await window.openNow.fetchLibraryGames({ token, providerStreamingBaseUrl: baseUrl });
  2505	      setLibraryGames(result);
  2506	      setSelectedGameId((previous) => result.some((game) => game.id === previous) ? previous : (result[0]?.id ?? ""));
  2507	      applyVariantSelections(result);
  2508	    } catch (error) {
  2509	      console.error("Failed to load games:", error);
  2510	    } finally {
  2511	      setIsLoadingGames(false);
  2512	    }
  2513	  }, [applyCatalogBrowseResult, applyVariantSelections, authSession, effectiveStreamingBaseUrl, searchQuery, catalogFilterKey, catalogSelectedSortId]);
  2514	
  2515	  useEffect(() => {
  2516	    if (!authSession || currentPage !== "home") {
  2517	      return;
  2518	    }
  2519	    const handle = window.setTimeout(() => {
  2520	      void loadGames("main");
  2521	    }, searchQuery.trim() ? 220 : 0);
  2522	    return () => window.clearTimeout(handle);
  2523	  }, [authSession, currentPage, loadGames, searchQuery, catalogFilterKey, catalogSelectedSortId]);
  2524	
  2525	  const handleSelectGameVariant = useCallback((gameId: string, variantId: string): void => {
  2526	    setVariantByGameId((prev) => {
  2527	      if (prev[gameId] === variantId) {
  2528	        return prev;
  2529	      }
  2530	      const next = { ...prev, [gameId]: variantId };
  2531	      try {
  2532	        localStorage.setItem(VARIANT_SELECTION_LOCALSTORAGE_KEY, JSON.stringify(next));
  2533	      } catch (e) {
  2534	        // ignore storage errors
  2535	      }
  2536	      return next;
  2537	    });
  2538	  }, []);
  2539	
  2540	  const claimAndConnectSession = useCallback(async (existingSession: ActiveSessionInfo): Promise<void> => {
  2541	    const token = authSession?.tokens.idToken ?? authSession?.tokens.accessToken;
  2542	    if (!token) {
  2543	      throw new Error("Missing token for session resume");
  2544	    }
  2545	    if (!existingSession.serverIp) {
  2546	      throw new Error("Active session is missing server address. Start the game again to create a new session.");
  2547	    }
  2548	
  2549	    const matchedContext = findGameContextForSession(existingSession);
  2550	    if (matchedContext) {
  2551	      setStreamingGame(matchedContext.game);
  2552	      setStreamingStore(matchedContext.variant?.store ?? null);
  2553	    } else {
  2554	      setStreamingStore(null);
  2555	    }
  2556	
  2557	    const claimed = await window.openNow.claimSession({
  2558	      token,
  2559	      streamingBaseUrl: effectiveStreamingBaseUrl,
  2560	      serverIp: existingSession.serverIp,
  2561	      sessionId: existingSession.sessionId,
  2562	      appId: String(existingSession.appId),
  2563	      settings: {
  2564	        resolution: settings.resolution,
  2565	        fps: settings.fps,
  2566	        maxBitrateMbps: settings.maxBitrateMbps,
  2567	        codec: settings.codec,
  2568	        colorQuality: settings.colorQuality,
  2569	        keyboardLayout: settings.keyboardLayout,
  2570	        gameLanguage: settings.gameLanguage,
  2571	        enableL4S: settings.enableL4S,
  2572	        enableCloudGsync: settings.enableCloudGsync,
  2573	      },
  2574	    });
  2575	
  2576	    console.log("Claimed session:", {
  2577	      sessionId: claimed.sessionId,
  2578	      signalingServer: claimed.signalingServer,
  2579	      signalingUrl: claimed.signalingUrl,
  2580	      status: claimed.status,
  2581	    });
  2582	
  2583	    await sleep(1000);
  2584	
  2585	    setSession(claimed);
  2586	    sessionRef.current = claimed;
  2587	    setQueuePosition(undefined);
  2588	    setStreamStatus("connecting");
  2589	    await window.openNow.connectSignaling({
  2590	      sessionId: claimed.sessionId,
  2591	      signalingServer: claimed.signalingServer,
  2592	      signalingUrl: claimed.signalingUrl,
  2593	    });
  2594	  }, [authSession, effectiveStreamingBaseUrl, findGameContextForSession, resetStatsOverlayToPreference, settings]);
  2595	
  2596	  // Play game handler
  2597	  const handlePlayGame = useCallback(async (game: GameInfo, options?: { bypassGuards?: boolean; streamingBaseUrl?: string }) => {
  2598	    if (!selectedProvider) return;
  2599	
  2600	    console.log("handlePlayGame entry", {
  2601	      title: game.title,
  2602	      launchInFlight: launchInFlightRef.current,
  2603	      streamStatus,
  2604	      bypass: options?.bypassGuards ?? false,
  2605	    });
  2606	
  2607	    if (!options?.bypassGuards && (launchInFlightRef.current || streamStatus !== "idle" || navbarSessionActionInFlightRef.current)) {
  2608	      console.warn("Ignoring play request: launch already in progress or stream not idle", {
  2609	        inFlight: launchInFlightRef.current,
  2610	        streamStatus,
  2611	        navbarSessionAction: navbarSessionActionInFlightRef.current,
  2612	      });
  2613	      return;
  2614	    }
  2615	
  2616	    launchInFlightRef.current = true;
  2617	    launchAbortRef.current = false;
  2618	    let loadingStep: StreamLoadingStatus = "queue";
  2619	    const updateLoadingStep = (next: StreamLoadingStatus): void => {
  2620	      loadingStep = next;
  2621	      setStreamStatus(next);
  2622	    };
  2623	
  2624	    setSessionStartedAtMs(null);
  2625	  setRemoteStreamWarning(null);
  2626	  setLocalSessionTimerWarning(null);
  2627	    setLaunchError(null);
  2628	    resetStatsOverlayToPreference();
  2629	    const selectedVariantId = variantByGameId[game.id] ?? defaultVariantId(game);
  2630	    const selectedVariant = getSelectedVariant(game, selectedVariantId);
  2631	    startPlaytimeSession(game.id);
  2632	    updateLoadingStep("queue");
  2633	    setQueuePosition(undefined);
  2634	
  2635	    try {
  2636	      const token = authSession?.tokens.idToken ?? authSession?.tokens.accessToken;
  2637	
  2638	      // Resolve appId
  2639	      let appId: string | null = null;
  2640	      if (isNumericId(selectedVariantId)) {
  2641	        appId = selectedVariantId;
  2642	      } else if (isNumericId(game.launchAppId)) {
  2643	        appId = game.launchAppId;
  2644	      }
  2645	
  2646	      if (!appId && token) {
  2647	        try {
  2648	          const resolved = await window.openNow.resolveLaunchAppId({
  2649	            token,
  2650	            providerStreamingBaseUrl: effectiveStreamingBaseUrl,
  2651	            appIdOrUuid: game.uuid ?? selectedVariantId,
  2652	          });
  2653	          if (resolved && isNumericId(resolved)) {
  2654	            appId = resolved;
  2655	          }
  2656	        } catch {
  2657	          // Ignore resolution errors
  2658	        }
  2659	      }
  2660	
  2661	      if (!appId) {
  2662	        throw new Error("Could not resolve numeric appId for this game");
  2663	      }
  2664	
  2665	      const numericAppId = Number(appId);
  2666	      const matchedGameContext = findSessionContextForAppId(allKnownGames, variantByGameId, numericAppId) ?? {
  2667	        game,
  2668	        variant: selectedVariant,
  2669	      };
  2670	      setStreamingGame(matchedGameContext.game);
  2671	      setStreamingStore(matchedGameContext.variant?.store ?? null);
  2672	
  2673	      let existingSessionStrategy: ExistingSessionStrategy | undefined;
  2674	
  2675	      // Check for active sessions first
  2676	      if (token) {
  2677	        try {
  2678	          const activeSessions = await window.openNow.getActiveSessions(token, effectiveStreamingBaseUrl);
  2679	          if (activeSessions.length > 0) {
  2680	            // Only claim sessions that are already paused/ready (status 2 or 3).
  2681	            // Status=1 sessions are still in queue/setup; sending a RESUME claim
  2682	            // skips the queue/ad phase entirely. Let them fall through to
  2683	            // createSession so the polling loop handles queue position and ads.
  2684	            const matchingSession = activeSessions.find((entry) => entry.appId === numericAppId && (entry.status === 2 || entry.status === 3)) ?? null;
  2685	            const otherSession = activeSessions.find((s) => s.status === 2 || s.status === 3) ?? null;
  2686	
  2687	            if (matchingSession) {
  2688	              await claimAndConnectSession(matchingSession);
  2689	              setNavbarActiveSession(null);
  2690	              return;
  2691	            }
  2692	
  2693	            if (otherSession) {
  2694	              const choice = await window.openNow.showSessionConflictDialog();
  2695	              if (choice === "cancel") {
  2696	                resetLaunchRuntime();
  2697	                return;
  2698	              }
  2699	              if (choice === "resume") {
  2700	                await claimAndConnectSession(otherSession);
  2701	                setNavbarActiveSession(null);
  2702	                return;
  2703	              }
  2704	              if (choice === "new") {
  2705	                existingSessionStrategy = "force-new";
  2706	              }
  2707	            }
  2708	          }
  2709	        } catch (error) {
  2710	          console.error("Failed to claim/resume session:", error);
  2711	          // Continue to create new session
  2712	        }
  2713	      }
  2714	
  2715	      // Create new session
  2716	      const newSession = await window.openNow.createSession({
  2717	        token: token || undefined,
  2718	        streamingBaseUrl: options?.streamingBaseUrl || effectiveStreamingBaseUrl,
  2719	        appId,
  2720	        internalTitle: game.title,
  2721	        accountLinked: chooseAccountLinked(game, selectedVariant),
  2722	        existingSessionStrategy,
  2723	        zone: "prod",
  2724	        settings: {
  2725	          resolution: settings.resolution,
  2726	          fps: settings.fps,
  2727	          maxBitrateMbps: settings.maxBitrateMbps,
  2728	          codec: settings.codec,
  2729	          colorQuality: settings.colorQuality,
  2730	          keyboardLayout: settings.keyboardLayout,
  2731	          gameLanguage: settings.gameLanguage,
  2732	          enableL4S: settings.enableL4S,
  2733	          enableCloudGsync: settings.enableCloudGsync,
  2734	        },
  2735	      });
  2736	
  2737	      setSession(newSession);
  2738	      setQueuePosition(newSession.queuePosition);
  2739	
  2740	      // Poll for readiness.
  2741	      // Queue and setup/starting modes wait indefinitely until the session becomes ready
  2742	      // or the launch is explicitly aborted. Some rigs take much longer than 180s.
  2743	      let finalSession: SessionInfo | null = null;
  2744	      let latestSession = newSession;
  2745	      let isInQueueMode = isSessionInQueue(newSession);
  2746	      let attempt = 0;
  2747	
  2748	      while (true) {
  2749	        attempt++;
  2750	
  2751	        const pollIntervalMs = shouldUseQueueAdPolling(latestSession, subscriptionInfo, authSession)
  2752	          ? SESSION_AD_POLL_INTERVAL_MS
  2753	          : SESSION_READY_POLL_INTERVAL_MS;
  2754	
  2755	        // Sleep in small ticks during ad-polling intervals so the loop can react
  2756	        // quickly when reportSessionAd clears isAdsRequired (which only updates
  2757	        // sessionRef, not the local latestSession variable).  Standard 2 s intervals
  2758	        // are kept as a single sleep since they're already short.
  2759	        if (pollIntervalMs > SESSION_READY_POLL_INTERVAL_MS) {
  2760	          const tickMs = 500;
  2761	          let elapsed = 0;
  2762	          while (elapsed < pollIntervalMs) {
  2763	            await sleep(tickMs);
  2764	            elapsed += tickMs;
  2765	            if (launchAbortRef.current) return;
  2766	            // Sync ad-action responses from sessionRef into the local tracking variable
  2767	            // so shouldUseQueueAdPolling sees the updated adState immediately.
  2768	            const refSession = sessionRef.current;
  2769	            if (refSession && refSession.sessionId === latestSession.sessionId) {
  2770	              latestSession = mergePolledSessionState(latestSession, refSession);
  2771	            }
  2772	            // Break out of the sleep early when ads are no longer required.
  2773	            if (!shouldUseQueueAdPolling(latestSession, subscriptionInfo, authSession)) {
  2774	              break;
  2775	            }
  2776	          }
  2777	        } else {
  2778	          await sleep(pollIntervalMs);
  2779	        }
  2780	
  2781	        if (shouldUseQueueAdPolling(latestSession, subscriptionInfo, authSession) && queueAdPlaybackRef.current) {
  2782	          const graceDeadline = Date.now() + 5000;
  2783	          while (queueAdPlaybackRef.current && Date.now() < graceDeadline) {
  2784	            await sleep(200);
  2785	            if (launchAbortRef.current) {
  2786	              return;
  2787	            }
  2788	          }
  2789	        }
  2790	
  2791	        if (launchAbortRef.current) {
  2792	          return;
  2793	        }
  2794	
  2795	        if (launchAbortRef.current) {
  2796	          return;
  2797	        }
  2798	
  2799	        const polled = await window.openNow.pollSession({
  2800	          token: token || undefined,
  2801	          streamingBaseUrl: newSession.streamingBaseUrl ?? effectiveStreamingBaseUrl,
  2802	          serverIp: newSession.serverIp,
  2803	          zone: newSession.zone,
  2804	          sessionId: newSession.sessionId,
  2805	          clientId: newSession.clientId,
  2806	          deviceId: newSession.deviceId,
  2807	        });
  2808	
  2809	        if (launchAbortRef.current) {
  2810	          return;
  2811	        }
  2812	
  2813	        const mergedSession = mergePolledSessionState(latestSession, polled);
  2814	        latestSession = mergedSession;
  2815	
  2816	        setSession(mergedSession);
  2817	        setQueuePosition(mergedSession.queuePosition);
  2818	
  2819	        // Check if queue just cleared so the loading UI can transition to setup mode.
  2820	        isInQueueMode = isSessionInQueue(mergedSession);
  2821	
  2822	        console.log(
  2823	          `Poll attempt ${attempt}: status=${mergedSession.status}, seatSetupStep=${mergedSession.seatSetupStep ?? "n/a"}, queuePosition=${mergedSession.queuePosition ?? "n/a"}, serverIp=${mergedSession.serverIp}, queueMode=${isInQueueMode}, adsRequired=${isSessionAdsRequired(mergedSession.adState)}`,
  2824	        );
  2825	
  2826	        if (isSessionReadyForConnect(mergedSession.status)) {
  2827	          finalSession = mergedSession;
  2828	          break;
  2829	        }
  2830	
  2831	        // Update status based on session state
  2832	        if (isInQueueMode) {
  2833	          updateLoadingStep("queue");
  2834	        } else if (mergedSession.status === 1) {
  2835	          updateLoadingStep("setup");
  2836	        }
  2837	
  2838	      }
  2839	
  2840	      // finalSession is guaranteed to be set here (we only exit the loop via break when session is ready)
  2841	
  2842	      setQueuePosition(undefined);
  2843	      updateLoadingStep("connecting");
  2844	
  2845	      // Use finalSession (the status=2 poll result) as the authoritative source for
  2846	      // signaling coordinates — it carries the real server IP resolved at the moment
  2847	      // the rig became ready. sessionRef.current may still hold stale zone-LB data
  2848	      // from a prior React render cycle.
  2849	      const sessionToConnect = finalSession ?? sessionRef.current ?? newSession;
  2850	      console.log("Connecting signaling with:", {
  2851	        sessionId: sessionToConnect.sessionId,
  2852	        signalingServer: sessionToConnect.signalingServer,
  2853	        signalingUrl: sessionToConnect.signalingUrl,
  2854	        status: sessionToConnect.status,
  2855	      });
  2856	
  2857	      await window.openNow.connectSignaling({
  2858	        sessionId: sessionToConnect.sessionId,
  2859	        signalingServer: sessionToConnect.signalingServer,
  2860	        signalingUrl: sessionToConnect.signalingUrl,
  2861	      });
  2862	    } catch (error) {
  2863	      if (launchAbortRef.current) {
  2864	        return;
  2865	      }
  2866	      console.error("Launch failed:", error);
  2867	      setLaunchError(toLaunchErrorState(error, loadingStep));
  2868	      await window.openNow.disconnectSignaling().catch(() => {});
  2869	      clientRef.current?.dispose();
  2870	      clientRef.current = null;
  2871	      resetLaunchRuntime({ keepLaunchError: true, keepStreamingContext: true });
  2872	      void refreshNavbarActiveSession();
  2873	    } finally {
  2874	      launchInFlightRef.current = false;
  2875	    }
  2876	  }, [
  2877	    authSession,
  2878	    allKnownGames,
  2879	    claimAndConnectSession,
  2880	    effectiveStreamingBaseUrl,
  2881	    refreshNavbarActiveSession,
  2882	    resetLaunchRuntime,
  2883	    resetStatsOverlayToPreference,
  2884	    selectedProvider,
  2885	    settings,
  2886	    streamStatus,
  2887	    variantByGameId,
  2888	  ]);
  2889	
  2890	  // Gate handler: shows queue server modal for FREE-tier users before launching
  2891	  const handleInitiatePlay = useCallback(async (game: GameInfo) => {
  2892	    const effectiveTier = normalizeMembershipTier(
  2893	      subscriptionInfo?.membershipTier ?? authSession?.user.membershipTier,
  2894	    );
  2895	    const isFreeUser = effectiveTier === "FREE";
  2896	    const isAllianceServer = isAllianceStreamingBaseUrl(effectiveStreamingBaseUrl);
  2897	    if (isAllianceServer) {
  2898	      setQueueModalData(null);
  2899	      void handlePlayGame(game);
  2900	      return;
  2901	    }
  2902	    if (isFreeUser && streamStatus === "idle" && !launchInFlightRef.current) {
  2903	      try {
  2904	        const [queueResult, mappingResult] = await Promise.allSettled([
  2905	          window.openNow.fetchPrintedWasteQueue(),
  2906	          window.openNow.fetchPrintedWasteServerMapping(),
  2907	        ]);
  2908	
  2909	        if (queueResult.status !== "fulfilled" || mappingResult.status !== "fulfilled") {
  2910	          console.warn(
  2911	            "[QueueServerSelect] PrintedWaste unavailable, skipping queue checks and launching with default routing.",
  2912	            {
  2913	              queueStatus: queueResult.status,
  2914	              mappingStatus: mappingResult.status,
  2915	            },
  2916	          );
  2917	          setQueueModalData(null);
  2918	          void handlePlayGame(game);
  2919	          return;
  2920	        }
  2921	
  2922	        const queueData = queueResult.value;
  2923	        if (!queueData || Object.keys(queueData).length === 0) {
  2924	          setQueueModalData(null);
  2925	          void handlePlayGame(game);
  2926	          return;
  2927	        }
  2928	
  2929	        if (!hasAnyEligiblePrintedWasteZone(queueData, mappingResult.value)) {
  2930	          console.warn(
  2931	            "[QueueServerSelect] No eligible non-nuked PrintedWaste zones available, skipping queue checks.",
  2932	          );
  2933	          setQueueModalData(null);
  2934	          void handlePlayGame(game);
  2935	          return;
  2936	        }
  2937	
  2938	        setQueueModalData(queueData);
  2939	        setQueueModalGame(game);
  2940	      } catch (error) {
  2941	        console.warn("[QueueServerSelect] PrintedWaste queue checks failed, launching without modal.", error);
  2942	        setQueueModalData(null);
  2943	        void handlePlayGame(game);
  2944	      }
  2945	      return;
  2946	    }
  2947	    void handlePlayGame(game);
  2948	  }, [subscriptionInfo, authSession, streamStatus, handlePlayGame, effectiveStreamingBaseUrl]);
  2949	
  2950	  const handleQueueModalConfirm = useCallback((zoneUrl: string | null) => {
  2951	    const game = queueModalGame;
  2952	    setQueueModalGame(null);
  2953	    setQueueModalData(null);
  2954	    if (game) {
  2955	      void handlePlayGame(game, { streamingBaseUrl: zoneUrl ?? undefined });
  2956	    }
  2957	  }, [queueModalGame, handlePlayGame]);
  2958	
  2959	  const handleQueueModalCancel = useCallback(() => {
  2960	    setQueueModalGame(null);
  2961	    setQueueModalData(null);
  2962	  }, []);
  2963	
  2964	  useEffect(() => {
  2965	    if (!logoutConfirmOpen && !removeAccountConfirmOpen) return;
  2966	
  2967	    const handleKeyDown = (event: KeyboardEvent) => {
  2968	      if (event.key === "Escape") {
  2969	        if (removeAccountConfirmOpen) {
  2970	          setRemoveAccountConfirmOpen(false);
  2971	          setAccountToRemove(null);
  2972	        } else if (logoutConfirmOpen) {
  2973	          setLogoutConfirmOpen(false);
  2974	        }
  2975	      }
  2976	      if (event.key === "Enter") {
  2977	        event.preventDefault();
  2978	        if (removeAccountConfirmOpen) {
  2979	          void confirmRemoveAccount();
  2980	        } else if (logoutConfirmOpen) {
  2981	          void confirmLogout();
  2982	        }
  2983	      }
  2984	    };
  2985	
  2986	    window.addEventListener("keydown", handleKeyDown);
  2987	    const previousOverflow = document.body.style.overflow;
  2988	    document.body.style.overflow = "hidden";
  2989	
  2990	    return () => {
  2991	      window.removeEventListener("keydown", handleKeyDown);
  2992	      document.body.style.overflow = previousOverflow;
  2993	    };
  2994	  }, [confirmLogout, confirmRemoveAccount, logoutConfirmOpen, removeAccountConfirmOpen]);
  2995	
  2996	  const accountToRemoveDisplayName = useMemo(() => (
  2997	    savedAccounts.find((account) => account.userId === accountToRemove)?.displayName ?? "this account"
  2998	  ), [accountToRemove, savedAccounts]);
  2999	
  3000	  const logoutConfirmModal = logoutConfirmOpen && typeof document !== "undefined"
  3001	    ? createPortal(
  3002	        <div className="logout-confirm" role="dialog" aria-modal="true" aria-label="Log out confirmation">
  3003	          <button
  3004	            type="button"
  3005	            className="logout-confirm-backdrop"
  3006	            onClick={() => setLogoutConfirmOpen(false)}
  3007	            aria-label="Cancel log out"
  3008	          />
  3009	          <div className="logout-confirm-card">
  3010	            <div className="logout-confirm-kicker">Session</div>
  3011	            <h3 className="logout-confirm-title">Log out of OpenNOW?</h3>
  3012	            <p className="logout-confirm-text">
  3013	              You&apos;re about to sign out of this device and return to guest mode.
  3014	            </p>
  3015	            <p className="logout-confirm-subtext">
  3016	              Your cloud session data stays on the service. This just clears your local app session.
  3017	            </p>
  3018	            <div className="logout-confirm-actions">
  3019	              <button
  3020	                type="button"
  3021	                className="logout-confirm-btn logout-confirm-btn-cancel"
  3022	                onClick={() => setLogoutConfirmOpen(false)}
  3023	              >
  3024	                Stay Signed In
  3025	              </button>
  3026	              <button
  3027	                type="button"
  3028	                className="logout-confirm-btn logout-confirm-btn-confirm"
  3029	                onClick={() => {
  3030	                  void confirmLogout();
  3031	                }}
  3032	              >
  3033	                Log Out
  3034	              </button>
  3035	            </div>
  3036	            <div className="logout-confirm-hint">
  3037	              <kbd>Enter</kbd> confirm · <kbd>Esc</kbd> cancel
  3038	            </div>
  3039	          </div>
  3040	        </div>,
  3041	        document.body,
  3042	      )
  3043	    : null;
  3044	
  3045	  const removeAccountConfirmModal = removeAccountConfirmOpen && typeof document !== "undefined"
  3046	    ? createPortal(
  3047	        <div className="logout-confirm" role="dialog" aria-modal="true" aria-label="Remove account confirmation">
  3048	          <button
  3049	            type="button"
  3050	            className="logout-confirm-backdrop"
  3051	            onClick={() => {
  3052	              setRemoveAccountConfirmOpen(false);
  3053	              setAccountToRemove(null);
  3054	            }}
  3055	            aria-label="Cancel account removal"
  3056	          />
  3057	          <div className="logout-confirm-card">
  3058	            <div className="logout-confirm-kicker">Accounts</div>
  3059	            <h3 className="logout-confirm-title">Remove account?</h3>
  3060	            <p className="logout-confirm-text">
  3061	              Are you sure you want to remove {accountToRemoveDisplayName} from saved accounts?
  3062	            </p>
  3063	            <p className="logout-confirm-subtext">
  3064	              You can add this account again by signing in.
  3065	            </p>
  3066	            <div className="logout-confirm-actions">
  3067	              <button
  3068	                type="button"
  3069	                className="logout-confirm-btn logout-confirm-btn-cancel"
  3070	                onClick={() => {
  3071	                  setRemoveAccountConfirmOpen(false);
  3072	                  setAccountToRemove(null);
  3073	                }}
  3074	              >
  3075	                Cancel
  3076	              </button>
  3077	              <button
  3078	                type="button"
  3079	                className="logout-confirm-btn logout-confirm-btn-confirm"
  3080	                onClick={() => {
  3081	                  void confirmRemoveAccount();
  3082	                }}
  3083	              >
  3084	                Remove
  3085	              </button>
  3086	            </div>
  3087	            <div className="logout-confirm-hint">
  3088	              <kbd>Enter</kbd> confirm · <kbd>Esc</kbd> cancel
  3089	            </div>
  3090	          </div>
  3091	        </div>,
  3092	        document.body,
  3093	      )
  3094	    : null;
  3095	
  3096	  const handleResumeFromNavbar = useCallback(async () => {
  3097	    if (
  3098	      !selectedProvider
  3099	      || !navbarActiveSession
  3100	      || isResumingNavbarSession
  3101	      || isTerminatingNavbarSession
  3102	      || navbarSessionActionInFlightRef.current
  3103	    ) {
  3104	      return;
  3105	    }
  3106	    if (launchInFlightRef.current || streamStatus !== "idle") {
  3107	      return;
  3108	    }
  3109	
  3110	    navbarSessionActionInFlightRef.current = "resume";
  3111	    launchInFlightRef.current = true;
  3112	    setIsResumingNavbarSession(true);
  3113	    let loadingStep: StreamLoadingStatus = "setup";
  3114	    const updateLoadingStep = (next: StreamLoadingStatus): void => {
  3115	      loadingStep = next;
  3116	      setStreamStatus(next);
  3117	    };
  3118	
  3119	    setLaunchError(null);
  3120	    setQueuePosition(undefined);
  3121	    setSessionStartedAtMs(null);
  3122	    setRemoteStreamWarning(null);
  3123	    setLocalSessionTimerWarning(null);
  3124	    resetStatsOverlayToPreference();
  3125	    const matchedContext = findGameContextForSession(navbarActiveSession);
  3126	    if (matchedContext) {
  3127	      setStreamingGame(matchedContext.game);
  3128	      setStreamingStore(matchedContext.variant?.store ?? null);
  3129	    } else {
  3130	      setStreamingStore(null);
  3131	    }
  3132	    updateLoadingStep("setup");
  3133	
  3134	    try {
  3135	      await claimAndConnectSession(navbarActiveSession);
  3136	      setNavbarActiveSession(null);
  3137	    } catch (error) {
  3138	      console.error("Navbar resume failed:", error);
  3139	      setLaunchError(toLaunchErrorState(error, loadingStep));
  3140	      await window.openNow.disconnectSignaling().catch(() => {});
  3141	      clientRef.current?.dispose();
  3142	      clientRef.current = null;
  3143	      resetLaunchRuntime({ keepLaunchError: true });
  3144	      void refreshNavbarActiveSession();
  3145	    } finally {
  3146	      navbarSessionActionInFlightRef.current = null;
  3147	      launchInFlightRef.current = false;
  3148	      setIsResumingNavbarSession(false);
  3149	    }
  3150	  }, [
  3151	    claimAndConnectSession,
  3152	    isTerminatingNavbarSession,
  3153	    isResumingNavbarSession,
  3154	    navbarActiveSession,
  3155	    findGameContextForSession,
  3156	    refreshNavbarActiveSession,
  3157	    resetLaunchRuntime,
  3158	    resetStatsOverlayToPreference,
  3159	    selectedProvider,
  3160	    streamStatus,
  3161	  ]);
  3162	
  3163	  const handleTerminateNavbarSession = useCallback(async () => {
  3164	    if (
  3165	      !navbarActiveSession
  3166	      || isResumingNavbarSession
  3167	      || isTerminatingNavbarSession
  3168	      || navbarSessionActionInFlightRef.current
  3169	    ) {
  3170	      return;
  3171	    }
  3172	    if (launchInFlightRef.current || streamStatus !== "idle") {
  3173	      return;
  3174	    }
  3175	
  3176	    const activeSessionTitle = gameTitleByAppId.get(navbarActiveSession.appId)?.trim() || "this session";
  3177	    if (!window.confirm(`Terminate ${activeSessionTitle}? This will end the active cloud session immediately.`)) {
  3178	      return;
  3179	    }
  3180	
  3181	    navbarSessionActionInFlightRef.current = "terminate";
  3182	    setIsTerminatingNavbarSession(true);
  3183	    try {
  3184	      await stopSessionByTarget({
  3185	        sessionId: navbarActiveSession.sessionId,
  3186	        zone: "",
  3187	        streamingBaseUrl: navbarActiveSession.streamingBaseUrl ?? (effectiveStreamingBaseUrl || undefined),
  3188	        serverIp: navbarActiveSession.serverIp,
  3189	      });
  3190	      setNavbarActiveSession(null);
  3191	    } catch (error) {
  3192	      console.error("Navbar terminate failed:", error);
  3193	    } finally {
  3194	      navbarSessionActionInFlightRef.current = null;
  3195	      setIsTerminatingNavbarSession(false);
  3196	      void refreshNavbarActiveSession();
  3197	    }
  3198	  }, [
  3199	    effectiveStreamingBaseUrl,
  3200	    gameTitleByAppId,
  3201	    isResumingNavbarSession,
  3202	    isTerminatingNavbarSession,
  3203	    navbarActiveSession,
  3204	    refreshNavbarActiveSession,
  3205	    stopSessionByTarget,
  3206	    streamStatus,
  3207	  ]);
  3208	
  3209	  // Stop stream handler
  3210	  const handleStopStream = useCallback(async () => {
  3211	    try {
  3212	      resolveExitPrompt(false);
  3213	      const status = streamStatusRef.current;
  3214	      if (status !== "idle" && status !== "streaming") {
  3215	        launchAbortRef.current = true;
  3216	      }
  3217	      await window.openNow.disconnectSignaling();
  3218	
  3219	      const current = sessionRef.current;
  3220	      if (current) {
  3221	        await stopSessionByTarget({
  3222	          streamingBaseUrl: current.streamingBaseUrl,
  3223	          serverIp: current.serverIp,
  3224	          zone: current.zone,
  3225	          sessionId: current.sessionId,
  3226	          clientId: current.clientId,
  3227	          deviceId: current.deviceId,
  3228	        });
  3229	      }
  3230	
  3231	      clientRef.current?.dispose();
  3232	      clientRef.current = null;
  3233	      setNavbarActiveSession(null);
  3234	      if (streamingGame) endPlaytimeSession(streamingGame.id);
  3235	      resetLaunchRuntime();
  3236	      void refreshNavbarActiveSession();
  3237	    } catch (error) {
  3238	      console.error("Stop failed:", error);
  3239	    }
  3240	  }, [endPlaytimeSession, refreshNavbarActiveSession, resetLaunchRuntime, resolveExitPrompt, stopSessionByTarget, streamingGame]);
  3241	
  3242	  const handleSwitchGame = useCallback(async (game: GameInfo) => {
  3243	    setControllerOverlayOpen(false);
  3244	    setPendingSwitchGameTitle(game.title ?? null);
  3245	    setPendingSwitchGameCover(game.imageUrl ?? null);
  3246	    setPendingSwitchGameDescription(game.description ?? null);
  3247	    setPendingSwitchGameId(game.id ?? null);
  3248	    setIsSwitchingGame(true);
  3249	    setSwitchingPhase("cleaning");
  3250	    // allow overlay to paint
  3251	    await new Promise<void>((resolve) => window.setTimeout(resolve, 140));
  3252	    try {
  3253	      await handleStopStream();
  3254	    } catch (e) {
  3255	      console.error("Error while cleaning up stream during switch:", e);
  3256	    }
  3257	    setSwitchingPhase("creating");
  3258	    // ensure runtime flags/state reflect the stopped session before launching again
  3259	    launchInFlightRef.current = false;
  3260	    setStreamStatus("idle");
  3261	    // give the render loop a frame so React state updates propagate
  3262	    await new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));
  3263	    // small pause so user sees transition
  3264	    await new Promise<void>((resolve) => window.setTimeout(resolve, 120));
  3265	
  3266	    // Wait until the play guard conditions are satisfied (defensive against races)
  3267	    const ready = await waitFor(() => !launchInFlightRef.current && streamStatusRef.current === "idle", { timeout: 1000, interval: 50 });
  3268	    if (!ready) {
  3269	      console.warn("Switch flow: runtime not ready for new launch after cleanup", {
  3270	        launchInFlight: launchInFlightRef.current,
  3271	        streamStatus: streamStatusRef.current,
  3272	      });
  3273	      // Do a small additional delay before aborting the automatic start to avoid nav-to-dashboard
  3274	      await sleep(250);
  3275	    }
  3276	
  3277	    try {
  3278	      await handlePlayGame(game, { bypassGuards: true });
  3279	    } catch (e) {
  3280	      console.error("Error while starting new stream during switch:", e);
  3281	    }
  3282	    setIsSwitchingGame(false);
  3283	    setSwitchingPhase(null);
  3284	    setPendingSwitchGameTitle(null);
  3285	    setPendingSwitchGameCover(null);
  3286	    setPendingSwitchGameDescription(null);
  3287	    setPendingSwitchGameId(null);
  3288	  }, [handleStopStream, handlePlayGame]);
  3289	
  3290	  const handleDismissLaunchError = useCallback(async () => {
  3291	    await window.openNow.disconnectSignaling().catch(() => {});
  3292	    clientRef.current?.dispose();
  3293	    clientRef.current = null;
  3294	    resetLaunchRuntime();
  3295	    void refreshNavbarActiveSession();
  3296	  }, [refreshNavbarActiveSession, resetLaunchRuntime]);
  3297	
  3298	  const releasePointerLockIfNeeded = useCallback(async () => {
  3299	    if (document.pointerLockElement) {
  3300	      // Tell the client to suppress synthetic Escape/reactive re-acquisition
  3301	      try {
  3302	        // clientRef is a mutable ref to the GfnWebRtcClient instance; access runtime property
  3303	        (clientRef.current as any).suppressNextSyntheticEscape = true;
  3304	      } catch (e) {
  3305	        // ignore
  3306	      }
  3307	      document.exitPointerLock();
  3308	      setEscHoldReleaseIndicator({ visible: false, progress: 0 });
  3309	      await sleep(75);
  3310	    }
  3311	  }, []);
  3312	
  3313	  const handlePromptedStopStream = useCallback(async () => {
  3314	    if (streamStatus === "idle") {
  3315	      return;
  3316	    }
  3317	
  3318	    await releasePointerLockIfNeeded();
  3319	
  3320	    const loadingPhases: StreamStatus[] = ["queue", "setup", "starting", "connecting"];
  3321	    if (loadingPhases.includes(streamStatus)) {
  3322	      launchAbortRef.current = true;
  3323	      await handleStopStream();
  3324	      return;
  3325	    }
  3326	
  3327	    const gameName = (streamingGame?.title || "this game").trim();
  3328	    const shouldExit = await requestExitPrompt(gameName);
  3329	    if (!shouldExit) {
  3330	      return;
  3331	    }
  3332	
  3333	    await handleStopStream();
  3334	  }, [handleStopStream, releasePointerLockIfNeeded, requestExitPrompt, streamStatus, streamingGame?.title]);
  3335	
  3336	  // Keyboard shortcuts
  3337	  useEffect(() => {
  3338	    const handleKeyDown = (e: KeyboardEvent) => {
  3339	      const target = e.target as HTMLElement | null;
  3340	      const isTyping = !!target && (
  3341	        target.tagName === "INPUT" ||
  3342	        target.tagName === "TEXTAREA" ||
  3343	        target.isContentEditable
  3344	      );
  3345	      if (isTyping) {
  3346	        return;
  3347	      }
  3348	
  3349	      if (exitPrompt.open) {
  3350	        if (e.key === "Escape") {
  3351	          e.preventDefault();
  3352	          e.stopPropagation();
  3353	          e.stopImmediatePropagation();
  3354	          handleExitPromptCancel();
  3355	        } else if (e.key === "Enter") {
  3356	          e.preventDefault();
  3357	          e.stopPropagation();
  3358	          e.stopImmediatePropagation();
  3359	          handleExitPromptConfirm();
  3360	        }
  3361	        return;
  3362	      }
  3363	
  3364	      const isPasteShortcut = e.key.toLowerCase() === "v" && !e.altKey && (isMac ? e.metaKey : e.ctrlKey);
  3365	      if (streamStatus === "streaming" && isPasteShortcut) {
  3366	        // Always stop local/browser paste behavior while streaming.
  3367	        // If clipboard paste is enabled, send clipboard text into the stream.
  3368	        e.preventDefault();
  3369	        e.stopPropagation();
  3370	        e.stopImmediatePropagation();
  3371	
  3372	        if (settings.clipboardPaste) {
  3373	          void (async () => {
  3374	            const client = clientRef.current;
  3375	            if (!client) return;
  3376	
  3377	            try {
  3378	              const text = await navigator.clipboard.readText();
  3379	              if (text && client.sendText(text) > 0) {
  3380	                return;
  3381	              }
  3382	            } catch (error) {
  3383	              console.warn("Clipboard read failed, falling back to paste shortcut:", error);
  3384	            }
  3385	
  3386	            client.sendPasteShortcut(isMac);
  3387	          })();
  3388	        }
  3389	        return;
  3390	      }
  3391	
  3392	      if (isShortcutMatch(e, shortcuts.toggleStats)) {
  3393	        e.preventDefault();
  3394	        e.stopPropagation();
  3395	        e.stopImmediatePropagation();
  3396	        setShowStatsOverlay((prev) => !prev);
  3397	        return;
  3398	      }
  3399	
  3400	      if (isShortcutMatch(e, shortcuts.togglePointerLock)) {
  3401	        e.preventDefault();
  3402	        e.stopPropagation();
  3403	        e.stopImmediatePropagation();
  3404	        if (streamStatus === "streaming" && videoRef.current) {
  3405	          if (document.pointerLockElement === videoRef.current) {
  3406	            try {
  3407	              (clientRef.current as any).suppressNextSyntheticEscape = true;
  3408	            } catch {
  3409	              // best-effort — client may not be initialised
  3410	            }
  3411	            document.exitPointerLock();
  3412	            setEscHoldReleaseIndicator({ visible: false, progress: 0 });
  3413	          } else {
  3414	            void requestEscLockedPointerCapture(videoRef.current);
  3415	          }
  3416	        }
  3417	        return;
  3418	      }
  3419	
  3420	      if (isShortcutMatch(e, shortcuts.toggleFullscreen)) {
  3421	        e.preventDefault();
  3422	        e.stopPropagation();
  3423	        e.stopImmediatePropagation();
  3424	        if (streamStatus === "connecting" || streamStatus === "streaming") {
  3425	          void toggleSessionFullscreen();
  3426	        }
  3427	        return;
  3428	      }
  3429	
  3430	      if (isShortcutMatch(e, shortcuts.stopStream)) {
  3431	        e.preventDefault();
  3432	        e.stopPropagation();
  3433	        e.stopImmediatePropagation();
  3434	        void handlePromptedStopStream();
  3435	        return;
  3436	      }
  3437	
  3438	      if (isShortcutMatch(e, shortcuts.toggleAntiAfk)) {
  3439	        e.preventDefault();
  3440	        e.stopPropagation();
  3441	        e.stopImmediatePropagation();
  3442	        if (streamStatus === "streaming") {
  3443	          setAntiAfkEnabled((prev) => !prev);
  3444	        }
  3445	        return;
  3446	      }
  3447	
  3448	      if (isShortcutMatch(e, shortcuts.toggleMicrophone)) {
  3449	        e.preventDefault();
  3450	        e.stopPropagation();
  3451	        e.stopImmediatePropagation();
  3452	        if (streamStatus === "streaming") {
  3453	          clientRef.current?.toggleMicrophone();
  3454	        }
  3455	      }
  3456	    };
  3457	
  3458	    // Use capture phase so app shortcuts run before stream input capture listeners.
  3459	    window.addEventListener("keydown", handleKeyDown, true);
  3460	    return () => window.removeEventListener("keydown", handleKeyDown, true);
  3461	  }, [
  3462	    exitPrompt.open,
  3463	    handleExitPromptCancel,
  3464	    handleExitPromptConfirm,
  3465	    handlePromptedStopStream,
  3466	    requestEscLockedPointerCapture,
  3467	    settings.clipboardPaste,
  3468	    shortcuts,
  3469	    streamStatus,
  3470	    toggleSessionFullscreen,
  3471	  ]);
  3472	
  3473	  const filteredGames = games;
  3474	
  3475	  const filteredLibraryGames = useMemo(() => {
  3476	    const query = searchQuery.trim();
  3477	    const searched = query ? libraryGames.filter((game) => matchesGameSearch(game, query)) : libraryGames;
  3478	    return sortLibraryGames(searched, catalogSelectedSortId === "relevance" ? "last_played" : catalogSelectedSortId);
  3479	  }, [libraryGames, searchQuery, catalogSelectedSortId]);
  3480	
  3481	  const activeSessionGameTitle = useMemo(() => {
  3482	    if (!navbarActiveSession) return null;
  3483	    const mappedTitle = gameTitleByAppId.get(navbarActiveSession.appId);
  3484	    if (mappedTitle) {
  3485	      return mappedTitle;
  3486	    }
  3487	    if (session?.sessionId === navbarActiveSession.sessionId && streamingGame?.title) {
  3488	      return streamingGame.title;
  3489	    }
  3490	    return null;
  3491	  }, [gameTitleByAppId, navbarActiveSession, session?.sessionId, streamingGame?.title]);
  3492	
  3493	  const effectiveAdState = getEffectiveAdState(session, subscriptionInfo, authSession);
  3494	  const activeQueueAd = useMemo(
  3495	    () => getActiveQueueAd(effectiveAdState, activeQueueAdId),
  3496	    [activeQueueAdId, effectiveAdState],
  3497	  );
  3498	  const activeQueueAdMediaUrl = getPreferredSessionAdMediaUrl(activeQueueAd) ?? (activeQueueAd ? adMediaUrlCacheRef.current[activeQueueAd.adId] : undefined);
  3499	
  3500	  useEffect(() => {
  3501	    const nextActiveAd = getActiveQueueAd(effectiveAdState, activeQueueAdIdRef.current);
  3502	    const nextActiveAdId = nextActiveAd?.adId ?? null;
  3503	    activeQueueAdIdRef.current = nextActiveAdId;
  3504	    setActiveQueueAdId((previous) => (previous === nextActiveAdId ? previous : nextActiveAdId));
  3505	
  3506	    if (!nextActiveAdId) {
  3507	      clearQueueAdWatchdogs();
  3508	    }
  3509	  }, [clearQueueAdWatchdogs, effectiveAdState]);
  3510	
  3511	  useEffect(() => {
  3512	    if (!activeQueueAd) {
  3513	      return;
  3514	    }
  3515	
  3516	    const syncQueueAdVisibility = (): void => {
  3517	      const preview = queueAdPreviewRef.current;
  3518	      if (!preview) {
  3519	        return;
  3520	      }
  3521	
  3522	      if (document.hidden || isSessionQueuePaused(effectiveAdState)) {
  3523	        preview.pause();
  3524	        return;
  3525	      }
  3526	
  3527	      void preview.resume();
  3528	    };
  3529	
  3530	    syncQueueAdVisibility();
  3531	    document.addEventListener("visibilitychange", syncQueueAdVisibility);
  3532	    return () => {
  3533	      document.removeEventListener("visibilitychange", syncQueueAdVisibility);
  3534	    };
  3535	  }, [activeQueueAd, effectiveAdState]);
  3536	
  3537	  // Show login screen if not authenticated
  3538	  if (!authSession) {
  3539	    return (
  3540	      <>
  3541	        <LoginScreen
  3542	          providers={providers}
  3543	          selectedProviderId={providerIdpId}
  3544	          onProviderChange={setProviderIdpId}
  3545	          onLogin={handleLogin}
  3546	          isLoading={isLoggingIn}
  3547	          error={loginError}
  3548	          isInitializing={isInitializing}
  3549	          statusMessage={startupStatusMessage}
  3550	        />
  3551	      </>
  3552	    );
  3553	  }
  3554	
  3555	  const showLaunchOverlay = streamStatus !== "idle" || launchError !== null || isSwitchingGame;
  3556	  const hasActiveStreamView = streamStatus !== "idle";
  3557	  const showLaunchErrorOverlay = launchError !== null;
  3558	  const showControllerLaunchLoading =
  3559	    !isSwitchingGame
  3560	    && settings.controllerMode
  3561	    && (showLaunchErrorOverlay || (streamStatus !== "idle" && streamStatus !== "streaming" && streamStatus !== "connecting"));
  3562	  const showDesktopLaunchLoading =
  3563	    !isSwitchingGame
  3564	    && !settings.controllerMode
  3565	    && (showLaunchErrorOverlay || (streamStatus !== "idle" && streamStatus !== "streaming"));
  3566	  const consumedHours =
  3567	    streamStatus === "streaming"
  3568	      ? Math.floor(sessionElapsedSeconds / 60) / 60
  3569	      : 0;
  3570	  const remainingPlaytimeText = formatRemainingPlaytimeFromSubscription(subscriptionInfo, consumedHours);
  3571	
  3572	  // Show stream lifecycle (waiting/connecting/streaming/failure)
  3573	  if (showLaunchOverlay) {
  3574	    const loadingStatus = launchError ? launchError.stage : toLoadingStatus(streamStatus);
  3575	    return (
  3576	      <>
  3577	        {hasActiveStreamView && (
  3578	          <StreamView
  3579	            className={isSwitchingGame ? "sv--switching" : undefined}
  3580	            videoRef={videoRef}
  3581	            audioRef={audioRef}
  3582	            diagnosticsStore={diagnosticsStore}
  3583	            showStats={showStatsOverlay}
  3584	            shortcuts={{
  3585	              toggleStats: formatShortcutForDisplay(settings.shortcutToggleStats, isMac),
  3586	              togglePointerLock: formatShortcutForDisplay(settings.shortcutTogglePointerLock, isMac),
  3587	              toggleFullscreen: formatShortcutForDisplay(settings.shortcutToggleFullscreen, isMac),
  3588	              stopStream: formatShortcutForDisplay(settings.shortcutStopStream, isMac),
  3589	              toggleAntiAfk: shortcuts.toggleAntiAfk.canonical,
  3590	              toggleMicrophone: formatShortcutForDisplay(settings.shortcutToggleMicrophone, isMac),
  3591	              screenshot: shortcuts.screenshot.canonical,
  3592	              recording: shortcuts.recording.canonical,
  3593	            }}
  3594	            hideStreamButtons={settings.hideStreamButtons}
  3595	            serverRegion={session?.serverIp}
  3596	            antiAfkEnabled={antiAfkEnabled}
  3597	            showAntiAfkIndicator={settings.showAntiAfkIndicator}
  3598	            escHoldReleaseIndicator={escHoldReleaseIndicator}
  3599	            exitPrompt={exitPrompt}
  3600	            sessionStartedAtMs={sessionStartedAtMs}
  3601	            sessionCounterEnabled={settings.sessionCounterEnabled}
  3602	            sessionClockShowEveryMinutes={settings.sessionClockShowEveryMinutes}
  3603	            sessionClockShowDurationSeconds={settings.sessionClockShowDurationSeconds}
  3604	            streamWarning={streamWarning}
  3605	            isConnecting={streamStatus === "connecting"}
  3606	            isStreaming={isStreaming}
  3607	            gameTitle={streamingGame?.title ?? "Game"}
  3608	            platformStore={streamingStore ?? undefined}
  3609	            onToggleFullscreen={() => {
  3610	              void toggleSessionFullscreen();
  3611	            }}
  3612	            onConfirmExit={handleExitPromptConfirm}
  3613	            onCancelExit={handleExitPromptCancel}
  3614	            onEndSession={() => {
  3615	              void handlePromptedStopStream();
  3616	            }}
  3617	            onToggleMicrophone={() => {
  3618	              clientRef.current?.toggleMicrophone();
  3619	            }}
  3620	            mouseSensitivity={settings.mouseSensitivity}
  3621	            onMouseSensitivityChange={handleMouseSensitivityChange}
  3622	            mouseAcceleration={settings.mouseAcceleration}
  3623	            onMouseAccelerationChange={handleMouseAccelerationChange}
  3624	            microphoneMode={settings.microphoneMode}
  3625	            onMicrophoneModeChange={handleMicrophoneModeChange}
  3626	            onScreenshotShortcutChange={(value) => {
  3627	              void updateSetting("shortcutScreenshot", value);
  3628	            }}
  3629	            onRecordingShortcutChange={(value) => {
  3630	              void updateSetting("shortcutToggleRecording", value);
  3631	            }}
  3632	            subscriptionInfo={subscriptionInfo}
  3633	            micTrack={clientRef.current?.getMicTrack() ?? null}
  3634	            onRequestPointerLock={handleRequestPointerLock}
  3635	            onReleasePointerLock={() => {
  3636	              void releasePointerLockIfNeeded();
  3637	            }}
  3638	          />
  3639	        )}
  3640	        {isSwitchingGame && settings.controllerMode && streamStatus !== "connecting" && (
  3641	          <ControllerStreamLoading
  3642	            gameTitle={pendingSwitchGameTitle ?? streamingGame?.title ?? "Game"}
  3643	            gamePoster={pendingSwitchGameCover ?? streamingGame?.imageUrl}
  3644	            gameDescription={pendingSwitchGameDescription ?? streamingGame?.description}
  3645	            status={switchingPhase === "cleaning" ? "setup" : "starting"}
  3646	            queuePosition={queuePosition}
  3647	            adState={effectiveAdState}
  3648	            activeAd={activeQueueAd}
  3649	            activeAdMediaUrl={activeQueueAdMediaUrl}
  3650	            error={
  3651	              launchError
  3652	                ? {
  3653	                    title: launchError.title,
  3654	                    description: launchError.description,
  3655	                    code: launchError.codeLabel,
  3656	                  }
  3657	                : undefined
  3658	            }
  3659	            onAdPlaybackEvent={handleQueueAdPlaybackEvent}
  3660	            adPreviewRef={queueAdPreviewRef}
  3661	            playtimeData={playtime}
  3662	            gameId={pendingSwitchGameId ?? streamingGame?.id}
  3663	            enableBackgroundAnimations={settings.controllerBackgroundAnimations}
  3664	          />
  3665	        )}
  3666	        {isSwitchingGame && !settings.controllerMode && (
  3667	          <StreamLoading
  3668	            gameTitle={pendingSwitchGameTitle ?? streamingGame?.title ?? "Game"}
  3669	            gameCover={pendingSwitchGameCover ?? streamingGame?.imageUrl}
  3670	            platformStore={streamingStore ?? undefined}
  3671	            status={switchingPhase === "cleaning" ? "setup" : "starting"}
  3672	            queuePosition={queuePosition}
  3673	            adState={effectiveAdState}
  3674	            activeAd={activeQueueAd}
  3675	            activeAdMediaUrl={activeQueueAdMediaUrl}
  3676	            onAdPlaybackEvent={handleQueueAdPlaybackEvent}
  3677	            adPreviewRef={queueAdPreviewRef}
  3678	            error={
  3679	              launchError
  3680	                ? {
  3681	                    title: launchError.title,
  3682	                    description: launchError.description,
  3683	                    code: launchError.codeLabel,
  3684	                  }
  3685	                : undefined
  3686	            }
  3687	            onCancel={() => {
  3688	              if (launchError) {
  3689	                void handleDismissLaunchError();
  3690	                return;
  3691	              }
  3692	              void handlePromptedStopStream();
  3693	            }}
  3694	          />
  3695	        )}
  3696	        {streamStatus === "streaming" && controllerOverlayOpen && (
  3697	          <div className="controller-overlay">
  3698	            <ControllerLibraryPage
  3699	              games={filteredLibraryGames}
  3700	              onPlayGame={handleSwitchGame}
  3701	              isLoading={isLoadingGames}
  3702	              selectedGameId={selectedGameId}
  3703	              onSelectGame={setSelectedGameId}
  3704	              uiSoundsEnabled={settings.controllerUiSounds}
  3705	              selectedVariantByGameId={variantByGameId}
  3706	              onSelectGameVariant={handleSelectGameVariant}
  3707	              favoriteGameIds={settings.favoriteGameIds}
  3708	              onToggleFavoriteGame={handleToggleFavoriteGame}
  3709	              onOpenSettings={() => setCurrentPage("settings")}
  3710	              currentStreamingGame={streamingGame}
  3711	              onResumeGame={() => setControllerOverlayOpen(false)}
  3712	              onCloseGame={async () => {
  3713	                setControllerOverlayOpen(false);
  3714	                // allow overlay close animation to play
  3715	                await sleep(300);
  3716	                await releasePointerLockIfNeeded();
  3717	                await handleStopStream();
  3718	              }}
  3719	              pendingSwitchGameCover={pendingSwitchGameCover}
  3720	              userName={authSession?.user.displayName}
  3721	              userAvatarUrl={authSession?.user.avatarUrl}
  3722	              subscriptionInfo={subscriptionInfo}
  3723	              playtimeData={playtime}
  3724	              sessionStartedAtMs={sessionStartedAtMs}
  3725	              isStreaming={isStreaming}
  3726	              sessionCounterEnabled={settings.sessionCounterEnabled}
  3727	              settings={{
  3728	                resolution: settings.resolution,
  3729	                fps: settings.fps,
  3730	                codec: settings.codec,
  3731	                enableL4S: settings.enableL4S,
  3732	                enableCloudGsync: settings.enableCloudGsync,
  3733	                microphoneDeviceId: settings.microphoneDeviceId,
  3734	                controllerUiSounds: settings.controllerUiSounds,
  3735	                controllerBackgroundAnimations: settings.controllerBackgroundAnimations,
  3736	                autoLoadControllerLibrary: settings.autoLoadControllerLibrary,
  3737	                autoFullScreen: settings.autoFullScreen,
  3738	                aspectRatio: settings.aspectRatio,
  3739	                posterSizeScale: settings.posterSizeScale,
  3740	                maxBitrateMbps: settings.maxBitrateMbps,
  3741	              }}
  3742	              resolutionOptions={getResolutionsByAspectRatio(settings.aspectRatio)}
  3743	              fpsOptions={fpsOptions}
  3744	              codecOptions={codecOptions}
  3745	              aspectRatioOptions={aspectRatioOptions as unknown as string[]}
  3746	              onSettingChange={updateSetting}
  3747	            />
  3748	          </div>
  3749	        )}
  3750	        {showControllerLaunchLoading && (
  3751	          <ControllerStreamLoading
  3752	            gameTitle={streamingGame?.title ?? "Game"}
  3753	            gamePoster={streamingGame?.imageUrl}
  3754	            gameDescription={streamingGame?.description}
  3755	            status={loadingStatus}
  3756	            queuePosition={queuePosition}
  3757	            adState={effectiveAdState}
  3758	            activeAd={activeQueueAd}
  3759	            activeAdMediaUrl={activeQueueAdMediaUrl}
  3760	            error={
  3761	              launchError
  3762	                ? {
  3763	                    title: launchError.title,
  3764	                    description: launchError.description,
  3765	                    code: launchError.codeLabel,
  3766	                  }
  3767	                : undefined
  3768	            }
  3769	            onAdPlaybackEvent={handleQueueAdPlaybackEvent}
  3770	            adPreviewRef={queueAdPreviewRef}
  3771	            playtimeData={playtime}
  3772	            gameId={streamingGame?.id}
  3773	            enableBackgroundAnimations={settings.controllerBackgroundAnimations}
  3774	          />
  3775	        )}
  3776	        {showDesktopLaunchLoading && (
  3777	          <StreamLoading
  3778	            gameTitle={streamingGame?.title ?? "Game"}
  3779	            gameCover={streamingGame?.imageUrl}
  3780	            platformStore={streamingStore ?? undefined}
  3781	            status={loadingStatus}
  3782	            queuePosition={queuePosition}
  3783	            adState={effectiveAdState}
  3784	            activeAd={activeQueueAd}
  3785	            activeAdMediaUrl={activeQueueAdMediaUrl}
  3786	            onAdPlaybackEvent={handleQueueAdPlaybackEvent}
  3787	            adPreviewRef={queueAdPreviewRef}
  3788	            error={
  3789	              launchError
  3790	                ? {
  3791	                    title: launchError.title,
  3792	                    description: launchError.description,
  3793	                    code: launchError.codeLabel,
  3794	                  }
  3795	                : undefined
  3796	            }
  3797	            onCancel={() => {
  3798	              if (launchError) {
  3799	                void handleDismissLaunchError();
  3800	                return;
  3801	              }
  3802	              void handlePromptedStopStream();
  3803	            }}
  3804	          />
  3805	        )}
  3806	        {showControllerHint && streamStatus !== "streaming" && (
  3807	          <div className="controller-hint controller-hint--overlay">
  3808	            <span>D-pad Navigate</span>
  3809	            <span>A Select</span>
  3810	            <span>B Back</span>
  3811	          </div>
  3812	        )}
  3813	      </>
  3814	    );
  3815	  }
  3816	
  3817	  // Main app layout
  3818	  return (
  3819	    <div className="app-container" style={getAppStyle(settings.posterSizeScale)}>
  3820	      {startupRefreshNotice && (
  3821	        <div className={`auth-refresh-notice auth-refresh-notice--${startupRefreshNotice.tone}`}>
  3822	          {startupRefreshNotice.text}
  3823	        </div>
  3824	      )}
  3825	      {!(settings.controllerMode && currentPage === "library") && (
  3826	        <Navbar
  3827	          currentPage={currentPage}
  3828	          onNavigate={setCurrentPage}
  3829	          user={authSession.user}
  3830	          subscription={subscriptionInfo}
  3831	          activeSession={navbarActiveSession}
  3832	          activeSessionGameTitle={activeSessionGameTitle}
  3833	          isResumingSession={isResumingNavbarSession}
  3834	          isTerminatingSession={isTerminatingNavbarSession}
  3835	          onResumeSession={() => {
  3836	            void handleResumeFromNavbar();
  3837	          }}
  3838	          onTerminateSession={() => {
  3839	            void handleTerminateNavbarSession();
  3840	          }}
  3841	          savedAccounts={savedAccounts}
  3842	          onSwitchAccount={handleSwitchAccount}
  3843	          onRemoveAccount={(userId) => {
  3844	            void handleRemoveAccount(userId);
  3845	          }}
  3846	          onAddAccount={handleAddAccount}
  3847	          onLogout={handleLogout}
  3848	        />
  3849	      )}
  3850	
  3851	      <main className="main-content">
  3852	        {currentPage === "home" && (
  3853	          <HomePage
  3854	            games={filteredGames}
  3855	            searchQuery={searchQuery}
  3856	            onSearchChange={setSearchQuery}
  3857	            onPlayGame={handleInitiatePlay}
  3858	            isLoading={isLoadingGames}
  3859	            selectedGameId={selectedGameId}
  3860	            onSelectGame={setSelectedGameId}
  3861	            selectedVariantByGameId={variantByGameId}
  3862	            onSelectGameVariant={handleSelectGameVariant}
  3863	            filterGroups={catalogFilterGroups}
  3864	            selectedFilterIds={catalogSelectedFilterIds}
  3865	            onToggleFilter={(filterId) => {
  3866	              setCatalogSelectedFilterIds((previous) => previous.includes(filterId) ? previous.filter((value) => value !== filterId) : [...previous, filterId]);
  3867	            }}
  3868	            sortOptions={catalogSortOptions}
  3869	            selectedSortId={catalogSelectedSortId}
  3870	            onSortChange={setCatalogSelectedSortId}
  3871	            totalCount={catalogTotalCount}
  3872	            supportedCount={catalogSupportedCount}
  3873	          />
  3874	        )}
  3875	
  3876	        {currentPage === "library" && (
  3877	          settings.controllerMode ? (
  3878	            <ControllerLibraryPage
  3879	              games={filteredLibraryGames}
  3880	              onPlayGame={handleInitiatePlay}
  3881	              isLoading={isLoadingGames}
  3882	              selectedGameId={selectedGameId}
  3883	              onSelectGame={setSelectedGameId}
  3884	              uiSoundsEnabled={settings.controllerUiSounds}
  3885	              selectedVariantByGameId={variantByGameId}
  3886	              onSelectGameVariant={handleSelectGameVariant}
  3887	              favoriteGameIds={settings.favoriteGameIds}
  3888	              onToggleFavoriteGame={handleToggleFavoriteGame}
  3889	              onOpenSettings={() => setCurrentPage("settings")}
  3890	              currentStreamingGame={streamingGame}
  3891	              onResumeGame={handlePlayGame}
  3892	              onCloseGame={handlePromptedStopStream}
  3893	              pendingSwitchGameCover={pendingSwitchGameCover}
  3894	              userName={authSession?.user.displayName}
  3895	              userAvatarUrl={authSession?.user.avatarUrl}
  3896	              subscriptionInfo={subscriptionInfo}
  3897	              playtimeData={playtime}
  3898	              sessionStartedAtMs={sessionStartedAtMs}
  3899	              isStreaming={isStreaming}
  3900	              sessionCounterEnabled={settings.sessionCounterEnabled}
  3901	              settings={{
  3902	                resolution: settings.resolution,
  3903	                fps: settings.fps,
  3904	                codec: settings.codec,
  3905	                enableL4S: settings.enableL4S,
  3906	                enableCloudGsync: settings.enableCloudGsync,
  3907	                microphoneDeviceId: settings.microphoneDeviceId,
  3908	                controllerUiSounds: settings.controllerUiSounds,
  3909	                controllerBackgroundAnimations: settings.controllerBackgroundAnimations,
  3910	                autoLoadControllerLibrary: settings.autoLoadControllerLibrary,
  3911	                autoFullScreen: settings.autoFullScreen,
  3912	                aspectRatio: settings.aspectRatio,
  3913	                posterSizeScale: settings.posterSizeScale,
  3914	                maxBitrateMbps: settings.maxBitrateMbps,
  3915	              }}
  3916	              resolutionOptions={getResolutionsByAspectRatio(settings.aspectRatio)}
  3917	              fpsOptions={fpsOptions}
  3918	              codecOptions={codecOptions}
  3919	              aspectRatioOptions={aspectRatioOptions as unknown as string[]}
  3920	              onSettingChange={updateSetting}
  3921	              onExitControllerMode={handleExitControllerMode}
  3922	              onExitApp={handleExitApp}
  3923	            />
  3924	          ) : (
  3925	            <LibraryPage
  3926	              games={filteredLibraryGames}
  3927	              searchQuery={searchQuery}
  3928	              onSearchChange={setSearchQuery}
  3929	              onPlayGame={handleInitiatePlay}
  3930	              isLoading={isLoadingGames}
  3931	              selectedGameId={selectedGameId}
  3932	              onSelectGame={setSelectedGameId}
  3933	              selectedVariantByGameId={variantByGameId}
  3934	              onSelectGameVariant={handleSelectGameVariant}
  3935	              libraryCount={libraryGames.length}
  3936	              sortOptions={catalogSortOptions.filter((option) => option.id !== "relevance")}
  3937	              selectedSortId={catalogSelectedSortId === "relevance" ? "last_played" : catalogSelectedSortId}
  3938	              onSortChange={setCatalogSelectedSortId}
  3939	            />
  3940	          )
  3941	        )}
  3942	
  3943	        {currentPage === "settings" && (
  3944	          <SettingsPage
  3945	            settings={settings}
  3946	            regions={regions}
  3947	            codecResults={codecResults}
  3948	            codecTesting={codecTesting}
  3949	            onRunCodecTest={runCodecTest}
  3950	            onSettingChange={updateSetting}
  3951	          />
  3952	        )}
  3953	      </main>
  3954	      {showControllerHint && (
  3955	        <div className="controller-hint">
  3956	          <span>D-pad Navigate</span>
  3957	          <span>A Select</span>
  3958	          <span>B Back</span>
  3959	          <span>LB/RB Tabs</span>
  3960	        </div>
  3961	      )}
  3962	
  3963	      {logoutConfirmModal}
  3964	      {removeAccountConfirmModal}
  3965	      {queueModalGame && streamStatus === "idle" && (
  3966	        <QueueServerSelectModal
  3967	          game={queueModalGame}
  3968	          initialQueueData={queueModalData}
  3969	          onConfirm={handleQueueModalConfirm}
  3970	          onCancel={handleQueueModalCancel}
  3971	        />
  3972	      )}
  3973	    </div>
  3974	  );
  3975	}
  3976	