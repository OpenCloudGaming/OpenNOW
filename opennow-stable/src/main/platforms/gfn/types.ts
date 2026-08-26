import type { SessionError } from "./errorCodes";
import type { SessionErrorInfo } from "@shared/sessionError";

export interface CloudMatchRequest {
  sessionRequestData: {
    appId: string | number;
    internalTitle: string | null;
    availableSupportedControllers: number[];
    preferredController?: number;
    requestedAudioFormat?: number;
    externalAppId?: string | null;
    transport?: null;
    networkTestSessionId: string | null;
    parentSessionId: string | null;
    clientIdentification: string;
    deviceHashId: string;
    clientVersion: string;
    sdkVersion: string;
    streamerVersion: number | string;
    clientPlatformName: string;
    clientRequestMonitorSettings: Array<{
      monitorId?: number;
      positionX?: number;
      positionY?: number;
      widthInPixels: number;
      heightInPixels: number;
      framesPerSecond: number;
      sdrHdrMode: number;
      displayData: {
        displayPrimaryX0?: number;
        displayPrimaryY0?: number;
        displayPrimaryX1?: number;
        displayPrimaryY1?: number;
        displayPrimaryX2?: number;
        displayPrimaryY2?: number;
        displayWhitePointX?: number;
        displayWhitePointY?: number;
        desiredContentMaxLuminance?: number;
        desiredContentMinLuminance?: number;
        desiredContentMaxFrameAverageLuminance?: number;
      } | null;
      hdr10PlusGamingData?: unknown;
      dpi: number;
    }>;
    useOps: boolean;
    audioMode: number;
    metaData: Array<{ key: string; value: string }>;
    sdrHdrMode: number;
    clientDisplayHdrCapabilities: {
      version: number;
      hdrEdrSupportedFlagsInUint32: number;
      staticMetadataDescriptorId?: number;
      static_metadata_descriptor_id?: number;
      display_data?: Record<string, number>;
    } | null;
    surroundAudioInfo: number;
    remoteControllersBitmap: number;
    clientTimezoneOffset: number;
    enhancedStreamMode: number;
    appLaunchMode: number;
    secureRTSPSupported: boolean;
    partnerCustomData: string | null;
    accountLinked: boolean;
    enablePersistingInGameSettings: boolean;
    userAge: number;
    requestedStreamingFeatures: {
      reflex: boolean;
      bitDepth: number;
      cloudGsync: boolean;
      enabledL4S: boolean;
      trueHdr?: boolean;
      mouseMovementFlags?: number;
      supportedHidDevices?: number;
      profile?: number;
      fallbackToLogicalResolution?: boolean;
      hidDevices?: string | null;
      chromaFormat: number;
      prefilterMode?: number;
      prefilterSharpness?: number;
      prefilterNoiseReduction?: number;
      hudStreamingMode?: number;
      qosPolicy?: number;
      touchSupport?: boolean;
      sdrColorSpace?: number;
      hdrColorSpace?: number;
      maxBitrateKbps?: number;
      codec?: number;
      vsync?: boolean;
      dynamicStreamingMode?: number;
      audioChannelCount?: number;
    };
  };
}

export interface CloudMatchResponse {
  requestStatus: {
    statusCode: number;
    statusDescription?: string;
    unifiedErrorCode?: number;
  };
  session: {
    sessionId: string;
    subSessionId?: string;
    status: number;
    queuePosition?: number;
    seatSetupInfo?: {
      seatSetupStep?: number;
      queuePosition?: number;
      seatSetupEta?: number;
    };
    sessionAdsRequired?: boolean;
    isAdsRequired?: boolean;
    sessionAds?: Array<{
      adId?: string;
      adState?: number;
      /** Top-level direct URL (server field name confirmed from live capture) */
      adUrl?: string;
      /** Legacy field names retained for forward-compat */
      mediaUrl?: string;
      videoUrl?: string;
      url?: string;
      /** Multi-format media source list (mp4deinterlaced720p, hlsadaptive, webm) */
      adMediaFiles?: Array<{
        mediaFileUrl?: string;
        encodingProfile?: string;
      }>;
      clickThroughUrl?: string;
      /** Duration in seconds (live field name: adLengthInSeconds) */
      adLengthInSeconds?: number;
      /** Legacy duration field names */
      durationMs?: number;
      durationInMs?: number;
      title?: string;
      description?: string;
    }>;
    opportunity?: {
      state?: string;
      queuePaused?: boolean;
      gracePeriodSeconds?: number;
      message?: string;
      title?: string;
      description?: string;
    };
    progressState?: number;
    eta?: number;
    sessionProgress?: {
      queuePosition?: number;
      progressState?: number;
      eta?: number;
      isAdsRequired?: boolean;
    };
    progressInfo?: {
      queuePosition?: number;
      progressState?: number;
      eta?: number;
      isAdsRequired?: boolean;
    };
    errorCode?: number;
    gpuType?: string;
    serverLocation?: string;
    connectionInfo?: Array<{
      ip?: string;
      port: number;
      usage: number;
      protocol?: number;
      appLevelProtocol?: number;
      resourcePath?: string;
      [key: string]: unknown;
    }>;
    sessionControlInfo?: {
      ip?: string;
    };
    iceServerConfiguration?: {
      iceServers?: Array<{
        urls: string[] | string;
        username?: string;
        credential?: string;
      }>;
    };
    sessionRequestData?: {
      appId?: string;
      appLaunchMode?: number;
      enablePersistingInGameSettings?: boolean;
      clientRequestMonitorSettings?: Array<{
        widthInPixels?: number;
        heightInPixels?: number;
        framesPerSecond?: number;
      }>;
      requestedStreamingFeatures?: {
        reflex?: boolean;
        bitDepth?: number;
        cloudGsync?: boolean;
        chromaFormat?: number;
        enabledL4S?: boolean;
        trueHdr?: boolean;
        codec?: number;
      };
    };
    finalizedStreamingFeatures?: {
      reflex?: boolean;
      bitDepth?: number;
      cloudGsync?: boolean;
      chromaFormat?: number;
      enabledL4S?: boolean;
      trueHdr?: boolean;
      codec?: number;
    };
    monitorSettings?: Array<{
      widthInPixels?: number;
      heightInPixels?: number;
      framesPerSecond?: number;
    }>;
  };
}

/** Session in the get sessions response */
export interface SessionEntry {
  sessionId: string;
  subSessionId?: string;
  status: number;
  queuePosition?: number;
  seatSetupInfo?: {
    seatSetupStep?: number;
    queuePosition?: number;
  };
  sessionProgress?: {
    queuePosition?: number;
  };
  progressInfo?: {
    queuePosition?: number;
  };
  gpuType?: string;
  sessionRequestData?: {
    appId?: string;
    appLaunchMode?: number;
    [key: string]: unknown;
  };
  sessionControlInfo?: {
    ip?: string;
  };
  connectionInfo?: Array<{
    ip?: string;
    port: number;
    usage: number;
    protocol?: number;
    appLevelProtocol?: number;
    resourcePath?: string;
    [key: string]: unknown;
  }>;
  monitorSettings?: Array<{
    widthInPixels?: number;
    heightInPixels?: number;
    framesPerSecond?: number;
  }>;
}

/** Response from GET /v2/session (list of sessions) */
export interface GetSessionsResponse {
  requestStatus: {
    statusCode: number;
    statusDescription?: string;
    unifiedErrorCode?: number;
  };
  sessions: SessionEntry[];
}

// Re-export error types for convenience
export type { SessionError, SessionErrorInfo };

/** Result type for CloudMatch operations that may fail with a SessionError */
export type CloudMatchResult<T> =
  | { success: true; data: T }
  | { success: false; error: SessionError };

/** Error response structure from CloudMatch API */
export interface CloudMatchErrorResponse {
  requestStatus: {
    statusCode: number;
    statusDescription?: string;
    unifiedErrorCode?: number;
  };
  session?: {
    sessionId?: string;
    errorCode?: number;
  };
}

/** Entitled resolution from subscription features */
export interface EntitledResolution {
  width: number;
  height: number;
  fps: number;
}

/** Storage addon info */
export interface StorageAddon {
  type: "PERMANENT_STORAGE";
  sizeGb?: number;
  usedGb?: number;
  regionName?: string;
  regionCode?: string;
}

/** Subscription info from MES API */
export interface SubscriptionInfo {
  membershipTier: string;
  subscriptionType?: string;
  subscriptionSubType?: string;
  allottedHours: number;
  purchasedHours: number;
  rolledOverHours: number;
  usedHours: number;
  remainingHours: number;
  totalHours: number;
  firstEntitlementStartDateTime?: string;
  serverRegionId?: string;
  currentSpanStartDateTime?: string;
  currentSpanEndDateTime?: string;
  notifyUserWhenTimeRemainingInMinutes?: number;
  notifyUserOnSessionWhenRemainingTimeInMinutes?: number;
  state?: string;
  isGamePlayAllowed?: boolean;
  isUnlimited: boolean;
  storageAddon?: StorageAddon;
  entitledResolutions: EntitledResolution[];
}
