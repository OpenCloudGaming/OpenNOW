//! GFN CloudMatch Error Codes
//!
//! Error code mappings extracted from the official GFN web client.
//! These provide user-friendly error messages for session failures.

use once_cell::sync::Lazy;
use std::collections::HashMap;

/// GFN Session Error Codes from official client
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(i64)]
pub enum GfnErrorCode {
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
    RequestForbidden = 3237093634,               // statusCode 2
    ServerInternalTimeout = 3237093635,          // statusCode 3
    ServerInternalError = 3237093636,            // statusCode 4
    ServerInvalidRequest = 3237093637,           // statusCode 5
    ServerInvalidRequestVersion = 3237093638,    // statusCode 6
    SessionListLimitExceeded = 3237093639,       // statusCode 7
    InvalidRequestDataMalformed = 3237093640,    // statusCode 8
    InvalidRequestDataMissing = 3237093641,      // statusCode 9
    RequestLimitExceeded = 3237093642,           // statusCode 10
    SessionLimitExceeded = 3237093643,           // statusCode 11
    InvalidRequestVersionOutOfDate = 3237093644, // statusCode 12
    SessionEntitledTimeExceeded = 3237093645,    // statusCode 13
    AuthFailure = 3237093646,                    // statusCode 14
    InvalidAuthenticationMalformed = 3237093647, // statusCode 15
    InvalidAuthenticationExpired = 3237093648,   // statusCode 16
    InvalidAuthenticationNotFound = 3237093649,  // statusCode 17
    EntitlementFailure = 3237093650,             // statusCode 18
    InvalidAppIdNotAvailable = 3237093651,       // statusCode 19
    InvalidAppIdNotFound = 3237093652,           // statusCode 20
    InvalidSessionIdMalformed = 3237093653,      // statusCode 21
    InvalidSessionIdNotFound = 3237093654,       // statusCode 22
    EulaUnAccepted = 3237093655,                 // statusCode 23
    MaintenanceStatus = 3237093656,              // statusCode 24
    ServiceUnAvailable = 3237093657,             // statusCode 25
    SteamGuardRequired = 3237093658,             // statusCode 26
    SteamLoginRequired = 3237093659,             // statusCode 27
    SteamGuardInvalid = 3237093660,              // statusCode 28
    SteamProfilePrivate = 3237093661,            // statusCode 29
    InvalidCountryCode = 3237093662,             // statusCode 30
    InvalidLanguageCode = 3237093663,            // statusCode 31
    MissingCountryCode = 3237093664,             // statusCode 32
    MissingLanguageCode = 3237093665,            // statusCode 33
    SessionNotPaused = 3237093666,               // statusCode 34
    EmailNotVerified = 3237093667,               // statusCode 35
    InvalidAuthenticationUnsupportedProtocol = 3237093668, // statusCode 36
    InvalidAuthenticationUnknownToken = 3237093669, // statusCode 37
    InvalidAuthenticationCredentials = 3237093670, // statusCode 38
    SessionNotPlaying = 3237093671,              // statusCode 39
    InvalidServiceResponse = 3237093672,         // statusCode 40
    AppPatching = 3237093673,                    // statusCode 41
    GameNotFound = 3237093674,                   // statusCode 42
    NotEnoughCredits = 3237093675,               // statusCode 43
    InvitationOnlyRegistration = 3237093676,     // statusCode 44
    RegionNotSupportedForRegistration = 3237093677, // statusCode 45
    SessionTerminatedByAnotherClient = 3237093678, // statusCode 46
    DeviceIdAlreadyUsed = 3237093679,            // statusCode 47
    ServiceNotExist = 3237093680,                // statusCode 48
    SessionExpired = 3237093681,                 // statusCode 49
    SessionLimitPerDeviceReached = 3237093682,   // statusCode 50
    ForwardingZoneOutOfCapacity = 3237093683,    // statusCode 51
    RegionNotSupportedIndefinitely = 3237093684, // statusCode 52
    RegionBanned = 3237093685,                   // statusCode 53
    RegionOnHoldForFree = 3237093686,            // statusCode 54
    RegionOnHoldForPaid = 3237093687,            // statusCode 55
    AppMaintenanceStatus = 3237093688,           // statusCode 56
    ResourcePoolNotConfigured = 3237093689,      // statusCode 57
    InsufficientVmCapacity = 3237093690,         // statusCode 58
    InsufficientRouteCapacity = 3237093691,      // statusCode 59
    InsufficientScratchSpaceCapacity = 3237093692, // statusCode 60
    RequiredSeatInstanceTypeNotSupported = 3237093693, // statusCode 61
    ServerSessionQueueLengthExceeded = 3237093694, // statusCode 62
    RegionNotSupportedForStreaming = 3237093695, // statusCode 63
    SessionForwardRequestAllocationTimeExpired = 3237093696, // statusCode 64
    SessionForwardGameBinariesNotAvailable = 3237093697, // statusCode 65
    GameBinariesNotAvailableInRegion = 3237093698, // statusCode 66
    UekRetrievalFailed = 3237093699,             // statusCode 67
    EntitlementFailureForResource = 3237093700,  // statusCode 68
    SessionInQueueAbandoned = 3237093701,        // statusCode 69
    MemberTerminated = 3237093702,               // statusCode 70
    SessionRemovedFromQueueMaintenance = 3237093703, // statusCode 71
    ZoneMaintenanceStatus = 3237093704,          // statusCode 72
    GuestModeCampaignDisabled = 3237093705,      // statusCode 73
    RegionNotSupportedAnonymousAccess = 3237093706, // statusCode 74
    InstanceTypeNotSupportedInSingleRegion = 3237093707, // statusCode 75
    InvalidZoneForQueuedSession = 3237093710,    // statusCode 78
    SessionWaitingAdsTimeExpired = 3237093711,   // statusCode 79
    UserCancelledWatchingAds = 3237093712,       // statusCode 80
    StreamingNotAllowedInLimitedMode = 3237093713, // statusCode 81
    ForwardRequestJPMFailed = 3237093714,        // statusCode 82
    MaxSessionNumberLimitExceeded = 3237093715,  // statusCode 83
    GuestModePartnerCapacityDisabled = 3237093716, // statusCode 84
    SessionRejectedNoCapacity = 3237093717,      // statusCode 85
    SessionInsufficientPlayabilityLevel = 3237093718, // statusCode 86
    ForwardRequestLOFNFailed = 3237093719,       // statusCode 87
    InvalidTransportRequest = 3237093720,        // statusCode 88
    UserStorageNotAvailable = 3237093721,        // statusCode 89
    GfnStorageNotAvailable = 3237093722,         // statusCode 90
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

/// User-friendly error messages
static ERROR_MESSAGES: Lazy<HashMap<i64, (&'static str, &'static str)>> = Lazy::new(|| {
    let mut m = HashMap::new();

    // (Title, Description)
    m.insert(15859712, ("Success", "Session started successfully."));

    // Client errors
    m.insert(
        3237085186,
        (
            "Invalid Operation",
            "The requested operation is not valid at this time.",
        ),
    );
    m.insert(
        3237089282,
        (
            "Network Error",
            "A network error occurred. Please check your internet connection.",
        ),
    );
    m.insert(
        3237093377,
        (
            "Authentication Required",
            "Your session has expired. Please log in again.",
        ),
    );
    m.insert(
        3237093379,
        (
            "Server Response Error",
            "Failed to parse server response. Please try again.",
        ),
    );
    m.insert(
        3237093381,
        (
            "Invalid Server Response",
            "The server returned an invalid response.",
        ),
    );
    m.insert(
        3237093384,
        ("Session Error", "An error occurred during session setup."),
    );
    m.insert(
        3237093387,
        (
            "Authentication Timeout",
            "Authentication token update timed out. Please log in again.",
        ),
    );

    // Server errors (most common ones with user-friendly messages)
    m.insert(
        3237093634,
        ("Access Forbidden", "Access to this service is forbidden."),
    );
    m.insert(
        3237093635,
        ("Server Timeout", "The server timed out. Please try again."),
    );
    m.insert(
        3237093636,
        (
            "Server Error",
            "An internal server error occurred. Please try again later.",
        ),
    );
    m.insert(3237093637, ("Invalid Request", "The request was invalid."));
    m.insert(
        3237093639,
        (
            "Too Many Sessions",
            "You have too many active sessions. Please close some sessions and try again.",
        ),
    );
    m.insert(3237093643, ("Session Limit Exceeded", "You have reached your session limit. Another session may already be running on your account."));
    m.insert(
        3237093645,
        (
            "Session Time Exceeded",
            "Your session time has been exceeded.",
        ),
    );
    m.insert(
        3237093646,
        (
            "Authentication Failed",
            "Authentication failed. Please log in again.",
        ),
    );
    m.insert(
        3237093648,
        (
            "Session Expired",
            "Your authentication has expired. Please log in again.",
        ),
    );
    m.insert(
        3237093650,
        (
            "Entitlement Error",
            "You don't have access to this game or service.",
        ),
    );
    m.insert(
        3237093651,
        (
            "Game Not Available",
            "This game is not currently available.",
        ),
    );
    m.insert(
        3237093652,
        ("Game Not Found", "This game was not found in the library."),
    );
    m.insert(
        3237093655,
        (
            "EULA Required",
            "You must accept the End User License Agreement to continue.",
        ),
    );
    m.insert(
        3237093656,
        (
            "Under Maintenance",
            "GeForce NOW is currently under maintenance. Please try again later.",
        ),
    );
    m.insert(
        3237093657,
        (
            "Service Unavailable",
            "The service is temporarily unavailable. Please try again later.",
        ),
    );
    m.insert(
        3237093658,
        (
            "Steam Guard Required",
            "Steam Guard authentication is required. Please complete Steam Guard verification.",
        ),
    );
    m.insert(
        3237093659,
        (
            "Steam Login Required",
            "You need to link your Steam account to play this game.",
        ),
    );
    m.insert(
        3237093660,
        (
            "Steam Guard Invalid",
            "Steam Guard code is invalid. Please try again.",
        ),
    );
    m.insert(
        3237093661,
        (
            "Steam Profile Private",
            "Your Steam profile is private. Please make it public or friends-only.",
        ),
    );
    m.insert(
        3237093667,
        (
            "Email Not Verified",
            "Please verify your email address to continue.",
        ),
    );
    m.insert(
        3237093673,
        (
            "Game Updating",
            "This game is currently being updated. Please try again later.",
        ),
    );
    m.insert(3237093674, ("Game Not Found", "This game was not found."));
    m.insert(
        3237093675,
        (
            "Insufficient Credits",
            "You don't have enough credits for this session.",
        ),
    );
    m.insert(
        3237093678,
        (
            "Session Taken Over",
            "Your session was taken over by another device.",
        ),
    );
    m.insert(3237093681, ("Session Expired", "Your session has expired."));
    m.insert(
        3237093682,
        (
            "Device Limit Reached",
            "You have reached the session limit for this device.",
        ),
    );
    m.insert(
        3237093683,
        (
            "Region At Capacity",
            "Your region is currently at capacity. Please try again later.",
        ),
    );
    m.insert(
        3237093684,
        (
            "Region Not Supported",
            "GeForce NOW is not available in your region.",
        ),
    );
    m.insert(
        3237093685,
        (
            "Region Banned",
            "GeForce NOW is not available in your region.",
        ),
    );
    m.insert(
        3237093686,
        (
            "Free Tier On Hold",
            "Free tier is temporarily unavailable in your region.",
        ),
    );
    m.insert(
        3237093687,
        (
            "Paid Tier On Hold",
            "Paid tier is temporarily unavailable in your region.",
        ),
    );
    m.insert(
        3237093688,
        (
            "Game Maintenance",
            "This game is currently under maintenance.",
        ),
    );
    m.insert(
        3237093690,
        (
            "No Capacity",
            "No gaming rigs are available right now. Please try again later or join the queue.",
        ),
    );
    m.insert(
        3237093694,
        (
            "Queue Full",
            "The queue is currently full. Please try again later.",
        ),
    );
    m.insert(
        3237093695,
        (
            "Region Not Supported",
            "Streaming is not supported in your region.",
        ),
    );
    m.insert(
        3237093698,
        (
            "Game Not Available",
            "This game is not available in your region.",
        ),
    );
    m.insert(
        3237093701,
        ("Queue Abandoned", "Your session in queue was abandoned."),
    );
    m.insert(
        3237093702,
        ("Account Terminated", "Your account has been terminated."),
    );
    m.insert(
        3237093703,
        (
            "Queue Maintenance",
            "The queue was cleared due to maintenance.",
        ),
    );
    m.insert(
        3237093704,
        ("Zone Maintenance", "This server zone is under maintenance."),
    );
    m.insert(3237093711, ("Ads Timeout", "Session expired while waiting for ads. Free tier users must watch ads to play. Please start a new session."));
    m.insert(
        3237093712,
        (
            "Ads Cancelled",
            "Session cancelled because ads were skipped. Free tier users must watch ads to play.",
        ),
    );
    m.insert(
        3237093713,
        ("Limited Mode", "Streaming is not allowed in limited mode."),
    );
    m.insert(
        3237093715,
        ("Session Limit", "Maximum number of sessions reached."),
    );
    m.insert(
        3237093717,
        (
            "No Capacity",
            "No gaming rigs are available. Please try again later.",
        ),
    );
    m.insert(3237093718, ("Playability Level Issue", "Your account's playability level is insufficient. This may mean another session is already running, or there's a subscription issue."));
    m.insert(
        3237093721,
        ("Storage Unavailable", "User storage is not available."),
    );
    m.insert(
        3237093722,
        ("Storage Error", "GFN storage is not available."),
    );

    // Cancellation
    m.insert(
        15867905,
        ("Session Cancelled", "Session setup was cancelled."),
    );
    m.insert(15867906, ("Queue Cancelled", "You left the queue."));
    m.insert(
        15867907,
        ("Request Cancelled", "The request was cancelled."),
    );
    m.insert(
        15867909,
        (
            "System Sleep",
            "Session setup was interrupted by system sleep.",
        ),
    );
    m.insert(
        15868417,
        (
            "No Internet",
            "No internet connection during session setup.",
        ),
    );

    // Network errors
    m.insert(
        3237101580,
        (
            "Socket Error",
            "A socket error occurred. Please check your network.",
        ),
    );
    m.insert(
        3237101581,
        (
            "DNS Error",
            "Failed to resolve server address. Please check your network.",
        ),
    );
    m.insert(
        3237101582,
        (
            "Connection Failed",
            "Failed to connect to the server. Please check your network.",
        ),
    );
    m.insert(
        3237101583,
        ("SSL Error", "A secure connection error occurred."),
    );
    m.insert(
        3237101584,
        (
            "Connection Timeout",
            "Connection timed out. Please check your network.",
        ),
    );
    m.insert(
        3237101585,
        (
            "Receive Timeout",
            "Data receive timed out. Please check your network.",
        ),
    );
    m.insert(
        3237101586,
        ("No Response", "Server not responding. Please try again."),
    );
    m.insert(
        3237101590,
        ("Certificate Error", "Server certificate was rejected."),
    );

    m
});

/// Parsed error information from CloudMatch response
#[derive(Debug, Clone)]
pub struct SessionError {
    /// HTTP status code (e.g., 403)
    pub http_status: u16,
    /// CloudMatch status code from requestStatus.statusCode
    pub status_code: i32,
    /// Status description from requestStatus.statusDescription
    pub status_description: Option<String>,
    /// Unified error code from requestStatus.unifiedErrorCode
    pub unified_error_code: Option<i64>,
    /// Session error code from session.errorCode
    pub session_error_code: Option<i32>,
    /// Computed GFN error code
    pub gfn_error_code: i64,
    /// User-friendly title
    pub title: String,
    /// User-friendly description
    pub description: String,
}

impl SessionError {
    /// Parse error from CloudMatch response JSON
    pub fn from_response(http_status: u16, response_body: &str) -> Self {
        // Try to parse JSON
        let json: serde_json::Value =
            serde_json::from_str(response_body).unwrap_or(serde_json::Value::Null);

        // Extract fields
        let status_code = json["requestStatus"]["statusCode"].as_i64().unwrap_or(0) as i32;

        let status_description = json["requestStatus"]["statusDescription"]
            .as_str()
            .map(|s| s.to_string());

        let unified_error_code = json["requestStatus"]["unifiedErrorCode"].as_i64();

        let session_error_code = json["session"]["errorCode"].as_i64().map(|c| c as i32);

        // Compute GFN error code using official client logic
        let gfn_error_code = Self::compute_error_code(status_code, unified_error_code);

        // Get user-friendly message
        let (title, description) =
            Self::get_error_message(gfn_error_code, &status_description, http_status);

        SessionError {
            http_status,
            status_code,
            status_description,
            unified_error_code,
            session_error_code,
            gfn_error_code,
            title,
            description,
        }
    }

    /// Compute GFN error code from CloudMatch response (matching official client logic)
    fn compute_error_code(status_code: i32, unified_error_code: Option<i64>) -> i64 {
        // Base error code
        let mut error_code: i64 = 3237093632; // SessionServerErrorBegin

        // Convert statusCode to error code
        if status_code == 1 {
            error_code = 15859712; // Success
        } else if status_code > 0 && status_code < 255 {
            error_code = 3237093632 + status_code as i64;
        }

        // Use unifiedErrorCode if available and error_code is generic
        if let Some(unified) = unified_error_code {
            match error_code {
                3237093632 | 3237093636 | 3237093381 => {
                    error_code = unified;
                }
                _ => {}
            }
        }

        error_code
    }

    /// Get user-friendly error message
    fn get_error_message(
        error_code: i64,
        status_description: &Option<String>,
        http_status: u16,
    ) -> (String, String) {
        // Check for known error code
        if let Some((title, desc)) = ERROR_MESSAGES.get(&error_code) {
            return (title.to_string(), desc.to_string());
        }

        // Parse status description for known patterns
        if let Some(desc) = status_description {
            let desc_upper = desc.to_uppercase();

            if desc_upper.contains("INSUFFICIENT_PLAYABILITY") {
                return (
                    "Session Already Active".to_string(),
                    "Another session is already running on your account. Please close it first or wait for it to timeout.".to_string()
                );
            }

            if desc_upper.contains("SESSION_LIMIT") {
                return (
                    "Session Limit Exceeded".to_string(),
                    "You have reached your maximum number of concurrent sessions.".to_string(),
                );
            }

            if desc_upper.contains("MAINTENANCE") {
                return (
                    "Under Maintenance".to_string(),
                    "The service is currently under maintenance. Please try again later."
                        .to_string(),
                );
            }

            if desc_upper.contains("CAPACITY") || desc_upper.contains("QUEUE") {
                return (
                    "No Capacity Available".to_string(),
                    "All gaming rigs are currently in use. Please try again later.".to_string(),
                );
            }

            if desc_upper.contains("AUTH") || desc_upper.contains("TOKEN") {
                return (
                    "Authentication Error".to_string(),
                    "Please log in again.".to_string(),
                );
            }

            if desc_upper.contains("ENTITLEMENT") {
                return (
                    "Access Denied".to_string(),
                    "You don't have access to this game or service.".to_string(),
                );
            }
        }

        // Fallback based on HTTP status
        match http_status {
            401 => (
                "Unauthorized".to_string(),
                "Please log in again.".to_string(),
            ),
            403 => (
                "Access Denied".to_string(),
                "Access to this resource was denied.".to_string(),
            ),
            404 => (
                "Not Found".to_string(),
                "The requested resource was not found.".to_string(),
            ),
            429 => (
                "Too Many Requests".to_string(),
                "Please wait a moment and try again.".to_string(),
            ),
            500..=599 => (
                "Server Error".to_string(),
                "A server error occurred. Please try again later.".to_string(),
            ),
            _ => (
                "Error".to_string(),
                format!("An error occurred (HTTP {}).", http_status),
            ),
        }
    }

    /// Check if this error indicates another session is running
    pub fn is_session_conflict(&self) -> bool {
        matches!(
            self.gfn_error_code,
            3237093643 | // SessionLimitExceeded
            3237093682 | // SessionLimitPerDeviceReached
            3237093715 | // MaxSessionNumberLimitExceeded
            3237093718 // SessionInsufficientPlayabilityLevel
        ) || self
            .status_description
            .as_ref()
            .map(|d| d.to_uppercase().contains("INSUFFICIENT_PLAYABILITY"))
            .unwrap_or(false)
    }

    /// Check if this is a temporary error that might resolve with retry
    pub fn is_retryable(&self) -> bool {
        matches!(
            self.gfn_error_code,
            3237089282 | // NetworkError
            3237093635 | // ServerInternalTimeout
            3237093636 | // ServerInternalError
            3237093683 | // ForwardingZoneOutOfCapacity
            3237093690 | // InsufficientVmCapacity
            3237093717 | // SessionRejectedNoCapacity
            3237101584 | // ConnectionTimeout
            3237101585 | // DataReceiveTimeout
            3237101586 // PeerNoResponse
        )
    }

    /// Check if user needs to log in again
    pub fn needs_reauth(&self) -> bool {
        matches!(
            self.gfn_error_code,
            3237093377 | // AuthTokenNotUpdated
            3237093387 | // AuthTokenUpdateTimeout
            3237093646 | // AuthFailure
            3237093647 | // InvalidAuthenticationMalformed
            3237093648 | // InvalidAuthenticationExpired
            3237093649 | // InvalidAuthenticationNotFound
            3237093668 | // InvalidAuthenticationUnsupportedProtocol
            3237093669 | // InvalidAuthenticationUnknownToken
            3237093670 // InvalidAuthenticationCredentials
        ) || self.http_status == 401
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_insufficient_playability() {
        let response = r#"{"session":{"sessionId":"test","errorCode":1},"requestStatus":{"statusCode":86,"statusDescription":"INSUFFICIENT_PLAYABILITY_LEVEL 8192C105","unifiedErrorCode":-2121088763}}"#;

        let error = SessionError::from_response(403, response);

        assert_eq!(error.status_code, 86);
        assert_eq!(error.gfn_error_code, 3237093718); // 3237093632 + 86
        assert!(error.is_session_conflict());
        assert_eq!(error.title, "Session Already Active");
    }

    #[test]
    fn test_parse_session_limit() {
        let response =
            r#"{"requestStatus":{"statusCode":11,"statusDescription":"SESSION_LIMIT_EXCEEDED"}}"#;

        let error = SessionError::from_response(403, response);

        assert_eq!(error.gfn_error_code, 3237093643); // 3237093632 + 11
        assert!(error.is_session_conflict());
    }
}
