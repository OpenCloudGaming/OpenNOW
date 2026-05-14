#pragma once

#include "OPNStreamTypes.h"
#include "common/OPNGameTypes.h"
#include <string>
#include <vector>
#include <mutex>
#include <unordered_map>

namespace OPN {

class SessionManager {
public:
    static SessionManager &Shared();

    void SetAccessToken(const std::string &token);
    void SetStreamingBaseUrl(const std::string &url);

    void CreateSession(const std::string &appId,
                       const std::string &internalTitle,
                       const StreamSettings &settings,
                       SessionCreateCallback completion);

    void GetActiveSessions(std::function<void(bool, const std::vector<ActiveSessionEntry> &, const std::string &)> completion);

    void ClaimSession(const std::string &sessionId,
                      const std::string &serverIp,
                      const std::string &appId,
                      const StreamSettings &settings,
                      bool recoveryMode,
                      SessionCreateCallback completion);

    void PollSession(const std::string &sessionId,
                     const std::string &serverIp,
                     SessionPollCallback completion);

    void StopSession(const std::string &sessionId,
                      const std::string &serverIp,
                      std::function<void(bool, const std::string &)> completion);

    void ReportSessionAd(const SessionInfo &session,
                         const std::string &adId,
                         const std::string &action,
                         int watchedTimeInMs,
                         int pausedTimeInMs,
                         const std::string &cancelReason,
                         std::function<void(bool, const SessionInfo &, const std::string &)> completion);

    std::string GetStreamingBaseUrl() const;

private:
    SessionManager() = default;

    void pollClaimSession(std::string sessionId,
                             std::string serverIp,
                             std::string deviceId,
                             std::string clientId,
                             NegotiatedStreamProfile initialStreamProfile,
                             SessionCreateCallback completion);
    void MergeAndStoreAdState(SessionInfo &info);

    std::string m_accessToken;
    std::string m_streamingBaseUrl;
    std::mutex m_adStateMutex;
    std::unordered_map<std::string, SessionAdState> m_adStatesBySessionId;
};

}
