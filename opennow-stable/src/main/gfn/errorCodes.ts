/**
 * GFN CloudMatch Error Codes
 *
 * Error code mappings extracted from the official GFN web client.
 * These provide user-friendly error messages for session failures.
 */

/** GFN Session Error Codes from official client */
export enum GfnErrorCode {
  // Success codes
  Success = 15859712,

  // Client-side errors (3237085xxx - 3237093xxx)
  InvalidOperation = 3237085186,
  NetworkError = 3237089282,
  GetActiveSessionServerError = 3237089283,
  AuthTokenNotUpdated = 3237093377,
  SessionFinishedState = 3237093378,
  ResponseParseFailure = 3237093379,
  InvalidServerResponse = 3237093381,
  PutOrPostInProgress = 3237093382,
  GridServerNotInitialized = 3237093383,
  DOMExceptionInSessionControl = 3237093384,
  InvalidAdStateTransition = 3237093386,
  AuthTokenUpdateTimeout = 3237093387,

  // Server error codes (base 3237093632 + statusCode)
  SessionServerErrorBegin = 3237093632,
  RequestForbidden = 3237093634, // statusCode 2
  ServerInternalTimeout = 3237093635, // statusCode 3
  ServerInternalError = 3237093636, // statusCode 4
  ServerInvalidRequest = 3237093637, // statusCode 5
  ServerInvalidRequestVersion = 3237093638, // statusCode 6
  SessionListLimitExceeded = 3237093639, // statusCode 7
  InvalidRequestDataMalformed = 3237093640, // statusCode 8
  InvalidRequestDataMissing = 3237093641, // statusCode 9
  RequestLimitExceeded = 3237093642, // statusCode 10
  SessionLimitExceeded = 3237093643, // statusCode 11
  InvalidRequestVersionOutOfDate = 3237093644, // statusCode 12
  SessionEntitledTimeExceeded = 3237093645, // statusCode 13
  AuthFailure = 3237093646, // statusCode 14
  InvalidAuthenticationMalformed = 3237093647, // statusCode 15
  InvalidAuthenticationExpired = 3237093648, // statusCode 16
  InvalidAuthenticationNotFound = 3237093649, // statusCode 17
  EntitlementFailure = 3237093650, // statusCode 18
  InvalidAppIdNotAvailable = 3237093651, // statusCode 19
  InvalidAppIdNotFound = 3237093652, // statusCode 20
  InvalidSessionIdMalformed = 3237093653, // statusCode 21
  InvalidSessionIdNotFound = 3237093654, // statusCode 22
  EulaUnAccepted = 3237093655, // statusCode 23
  MaintenanceStatus = 3237093656, // statusCode 24
  ServiceUnAvailable = 3237093657, // statusCode 25
  SteamGuardRequired = 3237093658, // statusCode 26
  SteamLoginRequired = 3237093659, // statusCode 27
  SteamGuardInvalid = 3237093660, // statusCode 28
  SteamProfilePrivate = 3237093661, // statusCode 29
  InvalidCountryCode = 3237093662, // statusCode 30
  InvalidLanguageCode = 3237093663, // statusCode 31
  MissingCountryCode = 3237093664, // statusCode 32
  MissingLanguageCode = 3237093665, // statusCode 33
  SessionNotPaused = 3237093666, // statusCode 34
  EmailNotVerified = 3237093667, // statusCode 35
  InvalidAuthenticationUnsupportedProtocol = 3237093668, // statusCode 36
  InvalidAuthenticationUnknownToken = 3237093669, // statusCode 37
  InvalidAuthenticationCredentials = 3237093670, // statusCode 38
  SessionNotPlaying = 3237093671, // statusCode 39
  InvalidServiceResponse = 3237093672, // statusCode 40
  AppPatching = 3237093673, // statusCode 41
  GameNotFound = 3237093674, // statusCode 42
  NotEnoughCredits = 3237093675, // statusCode 43
  InvitationOnlyRegistration = 3237093676, // statusCode 44
  RegionNotSupportedForRegistration = 3237093677, // statusCode 45
  SessionTerminatedByAnotherClient = 3237093678, // statusCode 46
  DeviceIdAlreadyUsed = 3237093679, // statusCode 47
  ServiceNotExist = 3237093680, // statusCode 48
  SessionExpired = 3237093681, // statusCode 49
  SessionLimitPerDeviceReached = 3237093682, // statusCode 50
  ForwardingZoneOutOfCapacity = 3237093683, // statusCode 51
  RegionNotSupportedIndefinitely = 3237093684, // statusCode 52
  RegionBanned = 3237093685, // statusCode 53
  RegionOnHoldForFree = 3237093686, // statusCode 54
  RegionOnHoldForPaid = 3237093687, // statusCode 55
  AppMaintenanceStatus = 3237093688, // statusCode 56
  ResourcePoolNotConfigured = 3237093689, // statusCode 57
  InsufficientVmCapacity = 3237093690, // statusCode 58
  InsufficientRouteCapacity = 3237093691, // statusCode 59
  InsufficientScratchSpaceCapacity = 3237093692, // statusCode 60
  RequiredSeatInstanceTypeNotSupported = 3237093693, // statusCode 61
  ServerSessionQueueLengthExceeded = 3237093694, // statusCode 62
  RegionNotSupportedForStreaming = 3237093695, // statusCode 63
  SessionForwardRequestAllocationTimeExpired = 3237093696, // statusCode 64
  SessionForwardGameBinariesNotAvailable = 3237093697, // statusCode 65
  GameBinariesNotAvailableInRegion = 3237093698, // statusCode 66
  UekRetrievalFailed = 3237093699, // statusCode 67
  EntitlementFailureForResource = 3237093700, // statusCode 68
  SessionInQueueAbandoned = 3237093701, // statusCode 69
  MemberTerminated = 3237093702, // statusCode 70
  SessionRemovedFromQueueMaintenance = 3237093703, // statusCode 71
  ZoneMaintenanceStatus = 3237093704, // statusCode 72
  GuestModeCampaignDisabled = 3237093705, // statusCode 73
  RegionNotSupportedAnonymousAccess = 3237093706, // statusCode 74
  InstanceTypeNotSupportedInSingleRegion = 3237093707, // statusCode 75
  InvalidZoneForQueuedSession = 3237093710, // statusCode 78
  SessionWaitingAdsTimeExpired = 3237093711, // statusCode 79
  UserCancelledWatchingAds = 3237093712, // statusCode 80
  StreamingNotAllowedInLimitedMode = 3237093713, // statusCode 81
  ForwardRequestJPMFailed = 3237093714, // statusCode 82
  MaxSessionNumberLimitExceeded = 3237093715, // statusCode 83
  GuestModePartnerCapacityDisabled = 3237093716, // statusCode 84
  SessionRejectedNoCapacity = 3237093717, // statusCode 85
  SessionInsufficientPlayabilityLevel = 3237093718, // statusCode 86
  ForwardRequestLOFNFailed = 3237093719, // statusCode 87
  InvalidTransportRequest = 3237093720, // statusCode 88
  UserStorageNotAvailable = 3237093721, // statusCode 89
  GfnStorageNotAvailable = 3237093722, // statusCode 90
  SessionServerErrorEnd = 3237093887,

  // Session setup cancelled
  SessionSetupCancelled = 15867905,
  SessionSetupCancelledDuringQueuing = 15867906,
  RequestCancelled = 15867907,
  SystemSleepDuringSessionSetup = 15867909,
  NoInternetDuringSessionSetup = 15868417,

  // Network errors (3237101xxx)
  SocketError = 3237101580,
  AddressResolveFailed = 3237101581,
  ConnectFailed = 3237101582,
  SslError = 3237101583,
  ConnectionTimeout = 3237101584,
  DataReceiveTimeout = 3237101585,
  PeerNoResponse = 3237101586,
  UnexpectedHttpRedirect = 3237101587,
  DataSendFailure = 3237101588,
  DataReceiveFailure = 3237101589,
  CertificateRejected = 3237101590,
  DataNotAllowed = 3237101591,
  NetworkErrorUnknown = 3237101592,
}

/** Error message entry with title and description */
interface ErrorMessageEntry {
  title: string;
  description: string;
}

/** User-friendly error messages map */
export const ERROR_MESSAGES: Map<number, ErrorMessageEntry> = new Map([
  // Success
  [15859712, { title: "Success", description: "Session started successfully." }],

  // Client errors
  [
    3237085186,
    {
      title: "Invalid Operation",
      description: "The requested operation is not valid at this time.",
    },
  ],
  [
    3237089282,
    {
      title: "Network Error",
      description: "A network error occurred. Please check your internet connection.",
    },
  ],
  [
    3237093377,
    {
      title: "Authentication Required",
      description: "Your session has expired. Please log in again.",
    },
  ],
  [
    3237093379,
    {
      title: "Server Response Error",
      description: "Failed to parse server response. Please try again.",
    },
  ],
  [
    3237093381,
    {
      title: "Invalid Server Response",
      description: "The server returned an invalid response.",
    },
  ],
  [
    3237093384,
    {
      title: "Session Error",
      description: "An error occurred during session setup.",
    },
  ],
  [
    3237093387,
    {
      title: "Authentication Timeout",
      description: "Authentication token update timed out. Please log in again.",
    },
  ],

  // Server errors
  [
    3237093634,
    {
      title: "Access Forbidden",
      description: "Access to this service is forbidden.",
    },
  ],
  [
    3237093635,
    {
      title: "Server Timeout",
      description: "The server timed out. Please try again.",
    },
  ],
  [
    3237093636,
    {
      title: "Server Error",
      description: "An internal server error occurred. Please try again later.",
    },
  ],
  [
    3237093637,
    {
      title: "Invalid Request",
      description: "The request was invalid.",
    },
  ],
  [
    3237093639,
    {
      title: "Too Many Sessions",
      description: "You have too many active sessions. Please close some sessions and try again.",
    },
  ],
  [
    3237093643,
    {
      title: "Session Limit Exceeded",
      description: "You have reached your session limit. Another session may already be running on your account.",
    },
  ],
  [
    3237093645,
    {
      title: "Session Time Exceeded",
      description: "Your session time has been exceeded.",
    },
  ],
  [
    3237093646,
    {
      title: "Authentication Failed",
      description: "Authentication failed. Please log in again.",
    },
  ],
  [
    3237093648,
    {
      title: "Session Expired",
      description: "Your authentication has expired. Please log in again.",
    },
  ],
  [
    3237093650,
    {
      title: "Entitlement Error",
      description: "You don't have access to this game or service.",
    },
  ],
  [
    3237093651,
    {
      title: "Game Not Available",
      description: "This game is not currently available.",
    },
  ],
  [
    3237093652,
    {
      title: "Game Not Found",
      description: "This game was not found in the library.",
    },
  ],
  [
    3237093655,
    {
      title: "EULA Required",
      description: "You must accept the End User License Agreement to continue.",
    },
  ],
  [
    3237093656,
    {
      title: "Under Maintenance",
      description: "GeForce NOW is currently under maintenance. Please try again later.",
    },
  ],
  [
    3237093657,
    {
      title: "Service Unavailable",
      description: "The service is temporarily unavailable. Please try again later.",
    },
  ],
  [
    3237093658,
    {
      title: "Steam Guard Required",
      description: "Steam Guard authentication is required. Please complete Steam Guard verification.",
    },
  ],
  [
    3237093659,
    {
      title: "Steam Login Required",
      description: "You need to link your Steam account to play this game.",
    },
  ],
  [
    3237093660,
    {
      title: "Steam Guard Invalid",
      description: "Steam Guard code is invalid. Please try again.",
    },
  ],
  [
    3237093661,
    {
      title: "Steam Profile Private",
      description: "Your Steam profile is private. Please make it public or friends-only.",
    },
  ],
  [
    3237093667,
    {
      title: "Email Not Verified",
      description: "Please verify your email address to continue.",
    },
  ],
  [
    3237093673,
    {
      title: "Game Updating",
      description: "This game is currently being updated. Please try again later.",
    },
  ],
  [
    3237093674,
    {
      title: "Game Not Found",
      description: "This game was not found.",
    },
  ],
  [
    3237093675,
    {
      title: "Insufficient Credits",
      description: "You don't have enough credits for this session.",
    },
  ],
  [
    3237093678,
    {
      title: "Session Taken Over",
      description: "Your session was taken over by another device.",
    },
  ],
  [
    3237093681,
    {
      title: "Session Expired",
      description: "Your session has expired.",
    },
  ],
  [
    3237093682,
    {
      title: "Device Limit Reached",
      description: "You have reached the session limit for this device.",
    },
  ],
  [
    3237093683,
    {
      title: "Region At Capacity",
      description: "Your region is currently at capacity. Please try again later.",
    },
  ],
  [
    3237093684,
    {
      title: "Region Not Supported",
      description: "GeForce NOW is not available in your region.",
    },
  ],
  [
    3237093685,
    {
      title: "Region Banned",
      description: "GeForce NOW is not available in your region.",
    },
  ],
  [
    3237093686,
    {
      title: "Free Tier On Hold",
      description: "Free tier is temporarily unavailable in your region.",
    },
  ],
  [
    3237093687,
    {
      title: "Paid Tier On Hold",
      description: "Paid tier is temporarily unavailable in your region.",
    },
  ],
  [
    3237093688,
    {
      title: "Game Maintenance",
      description: "This game is currently under maintenance.",
    },
  ],
  [
    3237093690,
    {
      title: "No Capacity",
      description: "No gaming rigs are available right now. Please try again later or join the queue.",
    },
  ],
  [
    3237093694,
    {
      title: "Queue Full",
      description: "The queue is currently full. Please try again later.",
    },
  ],
  [
    3237093695,
    {
      title: "Region Not Supported",
      description: "Streaming is not supported in your region.",
    },
  ],
  [
    3237093698,
    {
      title: "Game Not Available",
      description: "This game is not available in your region.",
    },
  ],
  [
    3237093701,
    {
      title: "Queue Abandoned",
      description: "Your session in queue was abandoned.",
    },
  ],
  [
    3237093702,
    {
      title: "Account Terminated",
      description: "Your account has been terminated.",
    },
  ],
  [
    3237093703,
    {
      title: "Queue Maintenance",
      description: "The queue was cleared due to maintenance.",
    },
  ],
  [
    3237093704,
    {
      title: "Zone Maintenance",
      description: "This server zone is under maintenance.",
    },
  ],
  [
    3237093711,
    {
      title: "Ads Timeout",
      description: "Session expired while waiting for ads. Free tier users must watch ads to play. Please start a new session.",
    },
  ],
  [
    3237093712,
    {
      title: "Ads Cancelled",
      description: "Session cancelled because ads were skipped. Free tier users must watch ads to play.",
    },
  ],
  [
    3237093713,
    {
      title: "Limited Mode",
      description: "Streaming is not allowed in limited mode.",
    },
  ],
  [
    3237093715,
    {
      title: "Session Limit",
      description: "Maximum number of sessions reached.",
    },
  ],
  [
    3237093717,
    {
      title: "No Capacity",
      description: "No gaming rigs are available. Please try again later.",
    },
  ],
  [
    3237093718,
    {
      title: "Playability Level Issue",
      description: "Your account's playability level is insufficient. This may mean another session is already running, or there's a subscription issue.",
    },
  ],
  [
    3237093721,
    {
      title: "Storage Unavailable",
      description: "User storage is not available.",
    },
  ],
  [
    3237093722,
    {
      title: "Storage Error",
      description: "GFN storage is not available.",
    },
  ],

  // Cancellation
  [
    15867905,
    {
      title: "Session Cancelled",
      description: "Session setup was cancelled.",
    },
  ],
  [
    15867906,
    {
      title: "Queue Cancelled",
      description: "You left the queue.",
    },
  ],
  [
    15867907,
    {
      title: "Request Cancelled",
      description: "The request was cancelled.",
    },
  ],
  [
    15867909,
    {
      title: "System Sleep",
      description: "Session setup was interrupted by system sleep.",
    },
  ],
  [
    15868417,
    {
      title: "No Internet",
      description: "No internet connection during session setup.",
    },
  ],

  // Network errors
  [
    3237101580,
    {
      title: "Socket Error",
      description: "A socket error occurred. Please check your network.",
    },
  ],
  [
    3237101581,
    {
      title: "DNS Error",
      description: "Failed to resolve server address. Please check your network.",
    },
  ],
  [
    3237101582,
    {
      title: "Connection Failed",
      description: "Failed to connect to the server. Please check your network.",
    },
  ],
  [
    3237101583,
    {
      title: "SSL Error",
      description: "A secure connection error occurred.",
    },
  ],
  [
    3237101584,
    {
      title: "Connection Timeout",
      description: "Connection timed out. Please check your network.",
    },
  ],
  [
    3237101585,
    {
      title: "Receive Timeout",
      description: "Data receive timed out. Please check your network.",
    },
  ],
  [
    3237101586,
    {
      title: "No Response",
      description: "Server not responding. Please try again.",
    },
  ],
  [
    3237101590,
    {
      title: "Certificate Error",
      description: "Server certificate was rejected.",
    },
  ],
]);

/** Parsed error information from CloudMatch response */
export interface SessionErrorInfo {
  /** HTTP status code (e.g., 403) */
  httpStatus: number;
  /** CloudMatch status code from requestStatus.statusCode */
  statusCode: number;
  /** Status description from requestStatus.statusDescription */
  statusDescription?: string;
  /** Unified error code from requestStatus.unifiedErrorCode */
  unifiedErrorCode?: number;
  /** Session error code from session.errorCode */
  sessionErrorCode?: number;
  /** Computed GFN error code */
  gfnErrorCode: number;
  /** User-friendly title */
  title: string;
  /** User-friendly description */
  description: string;
}

/** CloudMatch error response structure */
interface CloudMatchErrorResponse {
  requestStatus?: {
    statusCode?: number;
    statusDescription?: string;
    unifiedErrorCode?: number;
  };
  session?: {
    sessionId?: string;
    errorCode?: number;
  };
}

/** Session error class for parsing and handling CloudMatch errors */
export class SessionError extends Error {
  /** HTTP status code */
  public readonly httpStatus: number;
  /** CloudMatch status code from requestStatus.statusCode */
  public readonly statusCode: number;
  /** Status description from requestStatus.statusDescription */
  public readonly statusDescription?: string;
  /** Unified error code from requestStatus.unifiedErrorCode */
  public readonly unifiedErrorCode?: number;
  /** Session error code from session.errorCode */
  public readonly sessionErrorCode?: number;
  /** Computed GFN error code */
  public readonly gfnErrorCode: number;
  /** User-friendly title */
  public readonly title: string;

  constructor(info: SessionErrorInfo) {
    super(info.description);
    this.name = "SessionError";
    this.httpStatus = info.httpStatus;
    this.statusCode = info.statusCode;
    this.statusDescription = info.statusDescription;
    this.unifiedErrorCode = info.unifiedErrorCode;
    this.sessionErrorCode = info.sessionErrorCode;
    this.gfnErrorCode = info.gfnErrorCode;
    this.title = info.title;
  }

  /** Get error type as a string (e.g., "SessionLimitExceeded") */
  get errorType(): string {
    // Try to find the enum name from the error code
    const entry = Object.entries(GfnErrorCode).find(([, value]) => value === this.gfnErrorCode);
    if (entry) {
      return entry[0];
    }
    // Fallback to status code based naming
    if (this.statusCode > 0) {
      return `StatusCode${this.statusCode}`;
    }
    return "UnknownError";
  }

  /** Get user-friendly error message */
  get errorDescription(): string {
    return this.message;
  }

  /**
   * Parse error from CloudMatch response JSON
   */
  static fromResponse(httpStatus: number, responseBody: string): SessionError {
    let json: CloudMatchErrorResponse = {};

    try {
      json = JSON.parse(responseBody) as CloudMatchErrorResponse;
    } catch {
      // Parsing failed, use empty object
    }

    // Extract fields
    const statusCode = json.requestStatus?.statusCode ?? 0;
    const statusDescription = json.requestStatus?.statusDescription;
    const unifiedErrorCode = json.requestStatus?.unifiedErrorCode;
    const sessionErrorCode = json.session?.errorCode;

    // Compute GFN error code using official client logic
    const gfnErrorCode = SessionError.computeErrorCode(statusCode, unifiedErrorCode);

    // Get user-friendly message
    const { title, description } = SessionError.getErrorMessage(
      gfnErrorCode,
      statusDescription,
      httpStatus,
    );

    return new SessionError({
      httpStatus,
      statusCode,
      statusDescription,
      unifiedErrorCode,
      sessionErrorCode,
      gfnErrorCode,
      title,
      description,
    });
  }

  /**
   * Compute GFN error code from CloudMatch response (matching official client logic)
   */
  private static computeErrorCode(statusCode: number, unifiedErrorCode?: number): number {
    // Base error code
    let errorCode: number = 3237093632; // SessionServerErrorBegin

    // Convert statusCode to error code
    if (statusCode === 1) {
      errorCode = 15859712; // Success
    } else if (statusCode > 0 && statusCode < 255) {
      errorCode = 3237093632 + statusCode;
    }

    // Use unifiedErrorCode if available and error_code is generic
    if (unifiedErrorCode !== undefined) {
      switch (errorCode) {
        case 3237093632: // SessionServerErrorBegin
        case 3237093636: // ServerInternalError
        case 3237093381: // InvalidServerResponse
          errorCode = unifiedErrorCode;
          break;
      }
    }

    return errorCode;
  }

  /**
   * Get user-friendly error message
   */
  private static getErrorMessage(
    errorCode: number,
    statusDescription: string | undefined,
    httpStatus: number,
  ): { title: string; description: string } {
    // Check for known error code
    const knownError = ERROR_MESSAGES.get(errorCode);
    if (knownError) {
      return knownError;
    }

    // Parse status description for known patterns
    if (statusDescription) {
      const descUpper = statusDescription.toUpperCase();

      if (descUpper.includes("INSUFFICIENT_PLAYABILITY")) {
        return {
          title: "Session Already Active",
          description:
            "Another session is already running on your account. Please close it first or wait for it to timeout.",
        };
      }

      if (descUpper.includes("SESSION_LIMIT")) {
        return {
          title: "Session Limit Exceeded",
          description: "You have reached your maximum number of concurrent sessions.",
        };
      }

      if (descUpper.includes("MAINTENANCE")) {
        return {
          title: "Under Maintenance",
          description: "The service is currently under maintenance. Please try again later.",
        };
      }

      if (descUpper.includes("CAPACITY") || descUpper.includes("QUEUE")) {
        return {
          title: "No Capacity Available",
          description: "All gaming rigs are currently in use. Please try again later.",
        };
      }

      if (descUpper.includes("AUTH") || descUpper.includes("TOKEN")) {
        return {
          title: "Authentication Error",
          description: "Please log in again.",
        };
      }

      if (descUpper.includes("ENTITLEMENT")) {
        return {
          title: "Access Denied",
          description: "You don't have access to this game or service.",
        };
      }
    }

    // Fallback based on HTTP status
    switch (httpStatus) {
      case 401:
        return {
          title: "Unauthorized",
          description: "Please log in again.",
        };
      case 403:
        return {
          title: "Access Denied",
          description: "Access to this resource was denied.",
        };
      case 404:
        return {
          title: "Not Found",
          description: "The requested resource was not found.",
        };
      case 429:
        return {
          title: "Too Many Requests",
          description: "Please wait a moment and try again.",
        };
    }

    if (httpStatus >= 500 && httpStatus < 600) {
      return {
        title: "Server Error",
        description: "A server error occurred. Please try again later.",
      };
    }

    return {
      title: "Error",
      description: `An error occurred (HTTP ${httpStatus}).`,
    };
  }

  /**
   * Check if this error indicates another session is running
   */
  isSessionConflict(): boolean {
    const sessionConflictCodes = [
      GfnErrorCode.SessionLimitExceeded, // 3237093643
      GfnErrorCode.SessionLimitPerDeviceReached, // 3237093682
      GfnErrorCode.MaxSessionNumberLimitExceeded, // 3237093715
      GfnErrorCode.SessionInsufficientPlayabilityLevel, // 3237093718
    ];

    if (sessionConflictCodes.includes(this.gfnErrorCode)) {
      return true;
    }

    if (this.statusDescription?.toUpperCase().includes("INSUFFICIENT_PLAYABILITY")) {
      return true;
    }

    return false;
  }

  /**
   * Check if this is a temporary error that might resolve with retry
   */
  isRetryable(): boolean {
    const retryableCodes = [
      GfnErrorCode.NetworkError, // 3237089282
      GfnErrorCode.ServerInternalTimeout, // 3237093635
      GfnErrorCode.ServerInternalError, // 3237093636
      GfnErrorCode.ForwardingZoneOutOfCapacity, // 3237093683
      GfnErrorCode.InsufficientVmCapacity, // 3237093690
      GfnErrorCode.SessionRejectedNoCapacity, // 3237093717
      GfnErrorCode.ConnectionTimeout, // 3237101584
      GfnErrorCode.DataReceiveTimeout, // 3237101585
      GfnErrorCode.PeerNoResponse, // 3237101586
    ];

    return retryableCodes.includes(this.gfnErrorCode);
  }

  /**
   * Check if user needs to log in again
   */
  needsReauth(): boolean {
    const reauthCodes = [
      GfnErrorCode.AuthTokenNotUpdated, // 3237093377
      GfnErrorCode.AuthTokenUpdateTimeout, // 3237093387
      GfnErrorCode.AuthFailure, // 3237093646
      GfnErrorCode.InvalidAuthenticationMalformed, // 3237093647
      GfnErrorCode.InvalidAuthenticationExpired, // 3237093648
      GfnErrorCode.InvalidAuthenticationNotFound, // 3237093649
      GfnErrorCode.InvalidAuthenticationUnsupportedProtocol, // 3237093668
      GfnErrorCode.InvalidAuthenticationUnknownToken, // 3237093669
      GfnErrorCode.InvalidAuthenticationCredentials, // 3237093670
    ];

    if (reauthCodes.includes(this.gfnErrorCode)) {
      return true;
    }

    if (this.httpStatus === 401) {
      return true;
    }

    return false;
  }

  /**
   * Convert to a plain object for serialization
   */
  toJSON(): SessionErrorInfo {
    return {
      httpStatus: this.httpStatus,
      statusCode: this.statusCode,
      statusDescription: this.statusDescription,
      unifiedErrorCode: this.unifiedErrorCode,
      sessionErrorCode: this.sessionErrorCode,
      gfnErrorCode: this.gfnErrorCode,
      title: this.title,
      description: this.message,
    };
  }
}

/** Helper function to check if an error is a SessionError */
export function isSessionError(error: unknown): error is SessionError {
  return error instanceof SessionError;
}

/** Helper function to parse error from CloudMatch response */
export function parseCloudMatchError(httpStatus: number, responseBody: string): SessionError {
  return SessionError.fromResponse(httpStatus, responseBody);
}
