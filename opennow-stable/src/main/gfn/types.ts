import type { SessionError, SessionErrorInfo } from "./errorCodes";

export interface CloudMatchRequest {
  sessionRequestData: {
    appId: string;
    internalTitle: string | null;
    availableSupportedControllers: number[];
    networkTestSessionId: string | null;
    parentSessionId: string | null;
    clientIdentification: string;
    deviceHashId: string;
    clientVersion: string;
    sdkVersion: string;
    streamerVersion: number;
    clientPlatformName: string;
    clientRequestMonitorSettings: Array<{
      widthInPixels: number;
      heightInPixels: number;
      framesPerSecond: number;
      sdrHdrMode: number;
      displayData: {
        desiredContentMaxLuminance: number;
        desiredContentMinLuminance: number;
        desiredContentMaxFrameAverageLuminance: number;
      };
      dpi: number;
    }>;
    useOps: boolean;
    audioMode: number;
    metaData: Array<{ key: string; value: string }>;
    sdrHdrMode: number;
    clientDisplayHdrCapabilities: {
      version: number;
      hdrEdrSupportedFlagsInUint32: number;
      staticMetadataDescriptorId: number;
    } | null;
    surroundAudioInfo: number;
    remoteControllersBitmap: number;
    clientTimezoneOffset: number;
    enhancedStreamMode: number;
    appLaunchMode: number;
    secureRTSPSupported: boolean;
    partnerCustomData: string;
    accountLinked: boolean;
    enablePersistingInGameSettings: boolean;
    userAge: number;
    requestedStreamingFeatures: {
      reflex: boolean;
      bitDepth: number;
      cloudGsync: boolean;
      enabledL4S: boolean;
      mouseMovementFlags: number;
      trueHdr: boolean;
      supportedHidDevices: number;
      profile: number;
      fallbackToLogicalResolution: boolean;
      hidDevices: string | null;
      chromaFormat: number;
      prefilterMode: number;
      prefilterSharpness: number;
      prefilterNoiseReduction: number;
      hudStreamingMode: number;
      sdrColorSpace: number;
      hdrColorSpace: number;
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
    status: number;
    errorCode?: number;
    gpuType?: string;
    connectionInfo?: Array<{
      ip?: string;
      port: number;
      usage: number;
      protocol?: number;
      resourcePath?: string;
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
  };
}

/** Session in the get sessions response */
export interface SessionEntry {
  sessionId: string;
  status: number;
  gpuType?: string;
  sessionRequestData?: {
    appId?: string;
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
