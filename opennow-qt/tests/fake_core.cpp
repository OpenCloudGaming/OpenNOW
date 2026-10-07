#include <iostream>
#include <fstream>
#include <string>
#include <chrono>
#include <thread>
#include <unordered_map>
#include <utility>
#include <cstdlib>
#include <iomanip>

namespace {
std::string field(const std::string &json, const std::string &name)
{
    const auto marker = std::string{"\""} + name + "\":\"";
    const auto start = json.find(marker);
    if (start == std::string::npos) return {};
    const auto valueStart = start + marker.size();
    const auto end = json.find('"', valueStart);
    return end == std::string::npos ? std::string{} : json.substr(valueStart, end - valueStart);
}

std::string paramField(const std::string &json, const std::string &name)
{
    const auto params = json.find("\"params\":");
    return params == std::string::npos ? std::string{} : field(json.substr(params), name);
}

bool paramBool(const std::string &json, const std::string &name)
{
    const auto params = json.find("\"params\":");
    return params != std::string::npos
        && json.find("\"" + name + "\":true", params) != std::string::npos;
}

long paramNumber(const std::string &json, const std::string &name, long fallback)
{
    const auto params = json.find("\"params\":");
    if (params == std::string::npos) return fallback;
    const auto marker = "\"" + name + "\":";
    const auto start = json.find(marker, params);
    if (start == std::string::npos) return fallback;
    try {
        return std::stol(json.substr(start + marker.size()));
    } catch (...) {
        return fallback;
    }
}

struct FakePluginRegistry {
    static constexpr const char *GfnId = "org.opennow.geforce-now";
    static constexpr const char *ExampleId = "org.opennow.example.catalog";
    long generation = 1;
    bool installed = false;
    bool enabled = false;
    long sourceGeneration = 0;
    bool restarting = false;
    bool staleSourceSent = false;
    std::string token;

    static std::string descriptor(const std::string &id, const std::string &name, const std::string &publisher,
                                  const std::string &description, bool builtin, bool on)
    {
        return "{\"id\":\"" + id + "\",\"name\":\"" + name + "\",\"version\":\"" + (builtin ? "0.9.15" : "1.0.0")
            + "\",\"publisher\":\"" + publisher + "\",\"description\":\"" + description
            + "\",\"builtin\":" + (builtin ? "true" : "false") + ",\"required\":" + (builtin ? "true" : "false")
            + ",\"enabled\":" + (on ? "true" : "false") + ",\"state\":\"" + (on ? "ready" : "disabled")
            + "\",\"capabilities\":[\"catalog.v1\"],\"trust\":\"" + (builtin ? "builtin" : "unsigned-native")
            + "\",\"lastError\":null}";
    }
    std::string example(bool on) const
    {
        return descriptor(ExampleId, "Example Catalog", "OpenNOW example",
                          "Lists sample titles to show how a catalog plugin works.", false, on);
    }
    std::string snapshot() const
    {
        std::string plugins = descriptor(GfnId, "GeForce NOW", "OpenNOW",
                                         "The built-in GeForce NOW catalog and streaming service.", true, true);
        if (std::getenv("OPENNOW_TEST_SOURCES"))
            plugins += "," + descriptor("org.opennow.example.provider", "Example Provider", "OpenNOW example",
                                        "Example Provider fixture.", false, true);
        if (installed) {
            auto entry = example(enabled);
            const auto state = entry.find("\"state\":\"ready\"");
            if (restarting && state != std::string::npos) entry.replace(state, 15, "\"state\":\"starting\"");
            plugins += "," + entry;
        }
        return "{\"generation\":" + std::to_string(generation) + ",\"plugins\":[" + plugins + "]}";
    }
};

void respond(const std::string &id, const std::string &result)
{
    std::cout << "{\"type\":\"response\",\"id\":\"" << id << "\",\"ok\":true,\"result\":" << result << "}\n" << std::flush;
}

void fail(const std::string &id, const std::string &code, const std::string &message)
{
    std::cout << "{\"type\":\"response\",\"id\":\"" << id << "\",\"ok\":false,\"error\":{\"code\":\"" << code
              << "\",\"message\":\"" << message << "\"}}\n" << std::flush;
}

void pluginsChanged(const FakePluginRegistry &registry)
{
    std::cout << "{\"type\":\"event\",\"name\":\"plugins.changed\",\"payload\":{\"generation\":"
              << registry.generation << "}}\n" << std::flush;
}

bool handlePluginRequest(FakePluginRegistry &registry, const std::string &line, const std::string &id,
                         const std::string &method)
{
    if (method == "plugins.list") {
        respond(id, registry.snapshot());
    } else if (method == "plugins.install.inspect") {
        const auto path = paramField(line, "path");
        if (path.rfind("file://", 0) != 0 && path.rfind("/", 0) != 0) {
            fail(id, "invalid_package_path", "Choose a local plugin package file.");
        } else if (registry.installed) {
            fail(id, "plugin_exists", "This plugin is already installed. Remove it before installing another copy.");
        } else {
            registry.token = "fixture-token-" + std::to_string(registry.generation);
            respond(id, "{\"generation\":" + std::to_string(registry.generation) + ",\"inspection\":{\"token\":\""
                + registry.token + "\",\"expiresAt\":\"2026-10-06T23:59:59Z\",\"plugin\":" + registry.example(false)
                + ",\"packageSha256\":\"9f2c4e1b7a6d3c5e8f0a1b2c3d4e5f60718293a4b5c6d7e8f9a0b1c2d3e4f5a6\"}}");
        }
    } else if (method == "plugins.install.commit") {
        if (!paramBool(line, "consent") || paramField(line, "token") != registry.token || registry.token.empty()) {
            fail(id, "consent_required", "Confirm the plugin warning before installing.");
        } else if (paramNumber(line, "expectedGeneration", -1) != registry.generation) {
            fail(id, "stale_generation", "Your plugins changed. Choose the package again.");
        } else {
            registry.token.clear();
            registry.installed = true;
            registry.enabled = false;
            ++registry.generation;
            respond(id, registry.snapshot());
            pluginsChanged(registry);
        }
    } else if (method == "test.plugins.restart") {
        registry.restarting = !registry.restarting;
        if (!registry.restarting) ++registry.sourceGeneration;
        ++registry.generation;
        respond(id, "{}");
        pluginsChanged(registry);
    } else if (method == "plugins.install.cancel") {
        registry.token.clear();
        respond(id, "{\"cancelled\":true}");
    } else if (method == "plugins.setEnabled") {
        const auto target = paramField(line, "id");
        if (target == FakePluginRegistry::GfnId) {
            fail(id, "plugin_required", "GeForce NOW is required and can't be turned off.");
        } else if (target != FakePluginRegistry::ExampleId || !registry.installed) {
            fail(id, "plugin_not_found", "That plugin is not installed.");
        } else if (paramNumber(line, "expectedGeneration", -1) != registry.generation) {
            fail(id, "stale_generation", "Your plugins changed. Try again.");
        } else {
            registry.enabled = paramBool(line, "enabled");
            if (registry.enabled) ++registry.sourceGeneration;
            ++registry.generation;
            respond(id, registry.snapshot());
            pluginsChanged(registry);
        }
    } else if (method == "plugins.uninstall") {
        if (paramField(line, "id") == FakePluginRegistry::GfnId) {
            fail(id, "plugin_required", "GeForce NOW is built in and can't be removed.");
        } else if (!registry.installed || !paramBool(line, "confirmed")) {
            fail(id, "plugin_not_found", "That plugin is not installed.");
        } else {
            registry.installed = false;
            registry.enabled = false;
            ++registry.generation;
            respond(id, registry.snapshot());
            pluginsChanged(registry);
        }
    } else if (method == "sources.catalog.page") {
        const auto source = paramField(line, "sourceId");
        const bool example = source == FakePluginRegistry::ExampleId && registry.installed && registry.enabled
            && !registry.restarting;
        if (source != FakePluginRegistry::GfnId && !example) {
            fail(id, "source_unavailable", "This plugin is not running.");
            return true;
        }
        const auto query = paramField(line, "query");
        if (query == "stale-source" && example && !registry.staleSourceSent) {
            registry.staleSourceSent = true;
            fail(id, "stale_source", "The plugin restarted. Try again.");
            return true;
        }
        if (query == "special-ids") {
            std::string items;
            for (const char *localId : {"__proto__", "constructor", "toString", "hasOwnProperty"}) {
                if (!items.empty()) items += ",";
                items += "{\"id\":{\"sourceId\":\"" + source + "\",\"localId\":\"" + localId
                    + "\"},\"title\":\"Title " + localId + "\"}";
            }
            respond(id, "{\"sourceId\":\"" + source + "\",\"generation\":" + std::to_string(example ? registry.sourceGeneration : 1)
                + ",\"items\":[" + items + "],\"nextCursor\":null,\"coverage\":\"complete\"}");
            return true;
        }
        const auto cursor = paramField(line, "cursor");
        const long limit = paramNumber(line, "limit", 20);
        const int total = example ? 45 : 3;
        int offset = cursor.rfind("page-", 0) == 0 ? std::atoi(cursor.c_str() + 5) : 0;
        std::string items;
        int emitted = 0;
        int index = offset;
        for (; index < total && emitted < limit; ++index) {
            const auto title = (example ? std::string{"Example title "} : std::string{"Fixture game "}) + std::to_string(index + 1);
            if (!query.empty() && query != "stale-source" && title.find(query) == std::string::npos) continue;
            if (!items.empty()) items += ",";
            items += "{\"id\":{\"sourceId\":\"" + source + "\",\"localId\":\"" + (example ? "demo-" : "gfn-")
                + std::to_string(index + 1) + "\"},\"title\":\"" + title + "\"}";
            ++emitted;
        }
        const bool more = index < total && query.empty();
        respond(id, "{\"sourceId\":\"" + source + "\",\"generation\":" + std::to_string(example ? registry.sourceGeneration : 1)
            + ",\"items\":[" + items + "],\"nextCursor\":" + (more ? "\"page-" + std::to_string(index) + "\"" : std::string{"null"})
            + ",\"coverage\":\"" + (example ? (more ? "partial" : "complete") : "unknown") + "\"}");
    } else {
        return false;
    }
    return true;
}
}

struct FakeSourceRegistry {
    static constexpr const char *GfnId = "org.opennow.geforce-now";
    static constexpr const char *ProviderId = "org.opennow.example.provider";
    static constexpr const char *AnonymousId = "org.opennow.example.anonymous";
    long generation = 1;
    long providerGeneration = 1;
    std::string selected = GfnId;
    bool gfnEnabled = true;
    int authPolls = 0;
    bool signedIn = false;
    std::string attempt;
    std::string attemptKind;
    long accountRevision = 0;
    long settingsRevision = 1;
    bool hdrPreferred = false;
    std::string region = "eu";
    long streamRevision = 1;
    std::string streamCodec = "auto";
    int sessionPolls = 0;
    bool sessionActive = false;
    int releases = 0;
    int releaseAttempts = 0;
    bool failNextRelease = false;
    int reconciles = 0;
    long mediaRevision = 7;

    static std::string row(const std::string &id, const std::string &name, bool builtin, bool enabled,
                           const std::string &capabilities, const std::string &authKinds, const std::string &playback)
    {
        return "{\"id\":\"" + id + "\",\"name\":\"" + name + "\",\"version\":\"1.0.0\",\"publisher\":\"OpenNOW example\","
            "\"description\":\"" + name + " fixture.\",\"builtin\":" + (builtin ? "true" : "false")
            + ",\"required\":false,\"enabled\":" + (enabled ? "true" : "false") + ",\"state\":\"" + (enabled ? "ready" : "disabled")
            + "\",\"capabilities\":[\"catalog.v1\"],\"trust\":\"" + (builtin ? "builtin" : "unsigned-native")
            + "\",\"lastError\":null,\"protocolVersion\":2,\"providerCapabilities\":[" + capabilities + "],\"authKinds\":["
            + authKinds + "],\"playback\":" + playback + "}";
    }
    std::string snapshot() const
    {
        return "{\"generation\":" + std::to_string(generation) + ",\"selectedSourceId\":\"" + selected + "\",\"sources\":["
            + row(GfnId, "GeForce NOW", true, gfnEnabled,
                  "\"auth.deviceCode.v2\",\"accounts.v2\",\"catalog.library.v2\",\"catalog.details.v2\",\"launch.v2\",\"sessions.v2\"",
                  "\"device-code\"", "\"gfn-native\"") + ","
            + row(ProviderId, "Example Provider", false, true,
                  "\"auth.deviceCode.v2\",\"auth.browser.v2\",\"accounts.v2\",\"catalog.public.v2\",\"catalog.library.v2\",\"catalog.details.v2\",\"launch.v2\",\"sessions.v2\",\"settings.v2\",\"media.worker.v1\"",
                  "\"device-code\",\"browser\"", "\"media-worker-v1\"") + ","
            + row(AnonymousId, "Example Anonymous", false, true,
                  "\"auth.anonymous.v2\",\"catalog.public.v2\",\"catalog.details.v2\"", "\"anonymous\"", "null")
            + "]}";
    }
    std::string account() const
    {
        return "{\"key\":{\"authority\":\"example\",\"account\":\"player-1\"},\"name\":\"Example player\","
               "\"persistence\":\"durable\",\"reauthenticationRequired\":false,\"pinLocked\":false}";
    }
    std::string authState() const
    {
        if (signedIn)
            return "{\"state\":\"signed-in\",\"account\":" + account() + ",\"revision\":" + std::to_string(accountRevision) + "}";
        if (!attempt.empty() && authPolls >= 2)
            return "{\"state\":\"authorized\",\"attempt\":\"" + attempt + "\"}";
        if (!attempt.empty() && attemptKind == "browser")
            return "{\"state\":\"pending\",\"challenge\":{\"kind\":\"browser\",\"attempt\":\"" + attempt
                + "\",\"openHandle\":\"open-handle-1\",\"expiresAtMs\":4102444800000,\"pollAfterMs\":300}}";
        if (!attempt.empty())
            return "{\"state\":\"pending\",\"challenge\":{\"kind\":\"device-code\",\"attempt\":\"" + attempt
                + "\",\"userCode\":\"WXYZ-1234\",\"verificationUri\":\"https://example.invalid/link\","
                  "\"expiresAtMs\":4102444800000,\"pollAfterMs\":300}}";
        return "{\"state\":\"signed-out\"}";
    }
    std::string streamSettingsView() const
    {
        return "{\"revision\":" + std::to_string(streamRevision) + ",\"settings\":["
            "{\"key\":\"stream.codec\",\"label\":\"Requested codec\",\"control\":{\"kind\":\"choice\",\"choices\":["
            "{\"value\":\"auto\",\"label\":\"auto\"},{\"value\":\"h264\",\"label\":\"h264\"}]},"
            "\"value\":{\"kind\":\"choice\",\"value\":\"" + streamCodec + "\"}}]}";
    }
    std::string sessionView() const
    {
        return std::string("{\"key\":{\"account\":null,\"remoteId\":\"s1\"},\"target\":{\"game\":\"game-1\",\"variant\":\"default\"},"
            "\"state\":") + (!sessionActive ? "{\"state\":\"finished\",\"reason\":\"user_stopped\"}"
                : sessionPolls < 1 ? "{\"state\":\"queued\",\"position\":2,\"waitSeconds\":30}" : "{\"state\":\"ready\"}") + "}";
    }
    std::string settingsView() const
    {
        return "{\"revision\":" + std::to_string(settingsRevision) + ",\"settings\":["
            "{\"key\":\"hdr\",\"label\":\"Prefer HDR\",\"control\":{\"kind\":\"boolean\"},\"value\":{\"kind\":\"boolean\",\"value\":"
            + (hdrPreferred ? "true" : "false") + "}},"
            "{\"key\":\"region\",\"label\":\"Region\",\"control\":{\"kind\":\"choice\",\"choices\":[{\"value\":\"eu\",\"label\":\"Europe\"},"
            "{\"value\":\"us\",\"label\":\"United States\"}]},\"value\":{\"kind\":\"choice\",\"value\":\"" + region + "\"}}]}";
    }
};

std::string wrap(const std::string &source, long generation, const std::string &result)
{
    return "{\"sourceId\":\"" + source + "\",\"generation\":" + std::to_string(generation) + ",\"result\":" + result + "}";
}

void sourcesChanged(const std::string &source)
{
    std::cout << "{\"type\":\"event\",\"name\":\"sources.changed\",\"payload\":{\"sourceId\":\"" << source << "\"}}\n" << std::flush;
}

std::string gamePage(const std::string &source, const std::string &scope, const std::string &query, const std::string &cursor, long limit)
{
    const int total = source == FakeSourceRegistry::AnonymousId ? 4 : 30;
    const int offset = cursor.rfind("page-", 0) == 0 ? std::atoi(cursor.c_str() + 5) : 0;
    std::string items;
    int index = offset, emitted = 0;
    for (; index < total && emitted < limit; ++index) {
        const auto title = std::string(source == FakeSourceRegistry::AnonymousId ? "Free title " : "Provider game ") + std::to_string(index + 1);
        if (!query.empty() && title.find(query) == std::string::npos) continue;
        if (!items.empty()) items += ",";
        items += "{\"id\":\"game-" + std::to_string(index + 1) + "\",\"title\":\"" + title + "\",\"artwork\":null,"
            "\"subtitle\":\"Fixture\",\"badges\":[\"<b>Plain</b>\"],\"availability\":\""
            + (index == 2 ? std::string("maintenance") : std::string("available")) + "\"}";
        ++emitted;
    }
    const bool more = index < total && query.empty();
    return "{\"items\":[" + items + "],\"nextCursor\":" + (more ? "\"page-" + std::to_string(index) + "\"" : std::string("null"))
        + ",\"coverage\":\"" + (more ? "partial" : "complete") + "\",\"revision\":\"rev-1\",\"scope\":" + scope + "}";
}

bool handleSourceRequest(FakeSourceRegistry &registry, const std::string &line, const std::string &id, const std::string &method)
{
    if (method == "streamer.source.policy") {
        respond(id, "{\"localPolicy\":{\"videoBackend\":\"auto\",\"audioOutputDevice\":\"\",\"maxBitrateMbps\":75,"
            "\"replayBufferEnabled\":false,\"replayBufferSeconds\":30,\"replayBufferMemoryMiB\":256,\"shortcuts\":{}}}");
        return true;
    }
    if (method == "streamer.source.prepare") {
        if (paramField(line, "sessionHandle") == "pending") {
            std::cout << "{\"type\":\"event\",\"name\":\"test.source-prepare-pending\",\"payload\":{}}\n" << std::flush;
            return true;
        }
        if (paramField(line, "sessionHandle") != "op-1" || line.find("\"offer\":{") == std::string::npos) {
            fail(id, "session_owner_mismatch", "The private source handle is unavailable or no longer owned");
            return true;
        }
        respond(id, std::string("{\"version\":1,\"leaseId\":\"lease-1\",\"offerId\":\"offer-1\",\"runtimeEpoch\":1,\"sourceId\":\"")
            + FakeSourceRegistry::ProviderId + "\",\"session\":{\"account\":null,\"remoteId\":\"s1\"},\"attemptId\":\"attempt-1\","
            "\"expiresAtMs\":4102444800000,\"media\":{\"kind\":\"worker\",\"package\":{\"versionRoot\":\"/private/root\"},"
            "\"prepared\":{\"accepted\":{\"offerId\":\"offer-1\",\"runtimeEpoch\":1,\"video\":{\"encoding\":\"h264-annex-b\","
            "\"width\":320,\"height\":240,\"fps\":50,\"bitDepth\":8,\"chroma\":\"yuv420\",\"color\":{\"range\":\"limited\","
            "\"primaries\":\"bt709\",\"transfer\":\"bt709\",\"matrix\":\"bt709\",\"chromaLocation\":\"left\",\"note\":\"<b>x</b>\"},"
            "\"secretTag\":\"private-video-tag\"},\"audio\":null,\"input\":{\"keyboard\":true}},"
            "\"bootstrap\":\"private-bootstrap-secret\"}}}");
        return true;
    }
    if (method == "test.fail-next-release") {
        registry.failNextRelease = true;
        respond(id, "{\"armed\":1}");
        return true;
    }
    if (method == "streamer.source.release") {
        ++registry.releaseAttempts;
        if (std::exchange(registry.failNextRelease, false)) {
            fail(id, "session_journal_unavailable", "The session journal is unavailable");
            return true;
        }
        if (paramField(line, "leaseId") == "lease-1" && line.find("\"remoteId\":\"s1\"") != std::string::npos)
            ++registry.releases;
        respond(id, "{\"released\":true}");
        return true;
    }
    if (method == "streamer.source.observe") {
        respond(id, "{\"mediaRevision\":" + std::to_string(registry.mediaRevision) + "}");
        return true;
    }
    if (method == "streamer.source.reconcile") {
        if (paramNumber(line, "mediaRevision", -1) != registry.mediaRevision) {
            fail(id, "stale_media_observation", "Observe the media state again");
            return true;
        }
        if (line.find("\"status\":{") != std::string::npos && line.find("\"nativeIdle\":") != std::string::npos
                && line.find("\"type\":\"media-status\"") == std::string::npos)
            ++registry.reconciles;
        respond(id, "{\"reconciled\":true}");
        return true;
    }
    if (method == "test.source-releases") {
        respond(id, "{\"releases\":" + std::to_string(registry.releases) + ",\"reconciles\":"
            + std::to_string(registry.reconciles) + ",\"releaseAttempts\":" + std::to_string(registry.releaseAttempts) + "}");
        return true;
    }
    if (method == "sources.auth.open") {
        if (paramField(line, "openHandle") != "open-handle-1") {
            fail(id, "session_owner_mismatch", "The private source handle is unavailable or no longer owned");
            return true;
        }
        respond(id, "{\"url\":\"https://example.invalid/authorize?secret=private-browser-url\",\"attempt\":\"" + registry.attempt + "\"}");
        return true;
    }
    if (method == "sources.session.current") {
        respond(id, "{\"session\":null}");
        return true;
    }
    if (method == "sources.list") {
        respond(id, registry.snapshot());
        return true;
    }
    if (method == "sources.select") {
        const auto source = paramField(line, "sourceId");
        if (source != FakeSourceRegistry::GfnId && source != FakeSourceRegistry::ProviderId && source != FakeSourceRegistry::AnonymousId) {
            fail(id, "source_unavailable", "That service isn't available.");
            return true;
        }
        registry.selected = source;
        ++registry.generation;
        respond(id, registry.snapshot());
        return true;
    }
    if (method == "sources.session.create") {
        if (paramField(line, "sourceId") == "pending") {
            std::cout << "{\"type\":\"event\",\"name\":\"test.source-create-pending\",\"payload\":{}}\n" << std::flush;
        } else if (paramField(line, "sourceId") == "invalid") {
            respond(id, "{\"sourceId\":\"other\",\"generation\":1,\"result\":{}}");
        } else if (line.find("\"offer\":{") == std::string::npos) {
            fail(id, "invalid_params", "A native media offer is required");
        } else {
            registry.sessionActive = true;
            registry.sessionPolls = 0;
            respond(id, wrap(paramField(line, "sourceId"), 1, "{\"session\":" + registry.sessionView() + ",\"sessionHandle\":\"op-1\"}"));
        }
        return true;
    }
    if (method.rfind("sources.", 0) != 0)
        return false;
    const auto source = paramField(line, "sourceId");
    const bool provider = source == FakeSourceRegistry::ProviderId;
    const bool anonymous = source == FakeSourceRegistry::AnonymousId;
    if (!provider && !anonymous) {
        fail(id, "source_unavailable", "That service isn't available.");
        return true;
    }
    const long generation = anonymous ? 1 : registry.providerGeneration;
    if (method == "sources.auth.state") {
        respond(id, wrap(source, generation, anonymous ? "{\"state\":\"not-required\"}" : registry.authState()));
    } else if (method == "sources.auth.start" && provider) {
        registry.attempt = "attempt-" + std::to_string(++registry.accountRevision);
        registry.attemptKind = paramField(line, "kind");
        registry.authPolls = 0;
        respond(id, wrap(source, generation, registry.authState()));
    } else if (method == "sources.auth.poll" && provider) {
        if (paramField(line, "attempt") != registry.attempt) {
            fail(id, "stale_source", "This sign-in attempt is no longer current.");
            return true;
        }
        ++registry.authPolls;
        respond(id, wrap(source, generation, registry.authState()));
    } else if (method == "sources.auth.complete" && provider) {
        if (registry.authPolls < 2 || paramField(line, "attempt") != registry.attempt) {
            fail(id, "auth_required", "Approve the sign-in first.");
            return true;
        }
        registry.attempt.clear();
        registry.signedIn = true;
        respond(id, wrap(source, generation, registry.authState()));
        sourcesChanged(source);
    } else if (method == "sources.auth.cancel" && provider) {
        registry.attempt.clear();
        respond(id, wrap(source, generation, "{}"));
    } else if (method == "sources.auth.logout" && provider) {
        registry.signedIn = false;
        respond(id, wrap(source, generation, registry.authState()));
        sourcesChanged(source);
    } else if (method == "sources.accounts.list" && provider) {
        respond(id, wrap(source, generation, std::string("{\"accounts\":[") + (registry.signedIn ? registry.account() : "")
            + "],\"selected\":" + (registry.signedIn ? "{\"authority\":\"example\",\"account\":\"player-1\"}" : "null")
            + ",\"revision\":" + std::to_string(registry.accountRevision) + "}"));
    } else if (method == "sources.public.page" || method == "sources.library.page") {
        const bool library = method == "sources.library.page";
        if (library && (!provider || !registry.signedIn)) {
            fail(id, "auth_required", "Sign in to see your library.");
            return true;
        }
        const auto scope = library ? "{\"kind\":\"account\",\"scope\":{\"account\":{\"authority\":\"example\",\"account\":\"player-1\"},\"revision\":"
            + std::to_string(registry.accountRevision) + "}}" : std::string("{\"kind\":\"public\"}");
        respond(id, wrap(source, generation, gamePage(source, scope, paramField(line, "query"), paramField(line, "cursor"),
            paramNumber(line, "limit", 20))));
    } else if (method == "sources.game.get") {
        const auto game = paramField(line, "game");
        respond(id, wrap(source, generation, "{\"game\":{\"id\":\"" + game + "\",\"title\":\"Detail " + game
            + "\",\"artwork\":null,\"subtitle\":null,\"badges\":[],\"availability\":\"available\"},\"description\":\"A <i>plain</i> description.\","
              "\"variants\":[{\"id\":\"default\",\"label\":\"Standard\",\"availability\":\"available\"}],\"revision\":\"rev-1\",\"scope\":{\"kind\":\"public\"}}"));
    } else if (method == "sources.launch.inspect") {
        respond(id, wrap(source, generation, "{\"state\":\"ready\",\"target\":{\"game\":\"" + paramField(line, "game")
            + "\",\"variant\":\"default\"},\"revision\":\"rev-1\"}"));
    } else if (method == "sources.session.poll" && provider) {
        ++registry.sessionPolls;
        respond(id, wrap(source, generation, registry.sessionView()));
    } else if (method == "sources.session.stop" && provider) {
        registry.sessionActive = false;
        respond(id, wrap(source, generation, "{\"state\":\"resolved\"}"));
    } else if (method == "sources.settings.get" && provider) {
        respond(id, wrap(source, generation, registry.streamSettingsView()));
    } else if (method == "sources.settings.set" && provider) {
        if (paramNumber(line, "expectedRevision", -1) != registry.streamRevision) {
            fail(id, "stale_settings", "Source settings changed before this request");
            return true;
        }
        registry.streamCodec = line.find("\"value\":\"h264\"") != std::string::npos ? "h264" : "auto";
        ++registry.streamRevision;
        respond(id, wrap(source, generation, registry.streamSettingsView()));
    } else if (method == "sources.providerSettings.get" && provider) {
        respond(id, wrap(source, generation, registry.settingsView()));
    } else if (method == "sources.providerSettings.set" && provider) {
        if (paramNumber(line, "expectedRevision", -1) != registry.settingsRevision) {
            fail(id, "stale_source", "Settings changed. Try again.");
            return true;
        }
        const auto key = paramField(line, "key");
        if (key == "hdr") registry.hdrPreferred = line.find("\"value\":true") != std::string::npos;
        else if (key == "region") {
            const auto marker = std::string{"\"kind\":\"choice\",\"value\":\""};
            const auto at = line.find(marker);
            if (at != std::string::npos) registry.region = line.substr(at + marker.size(), line.find('"', at + marker.size()) - at - marker.size());
        }
        ++registry.settingsRevision;
        respond(id, wrap(source, generation, registry.settingsView()));
    } else {
        fail(id, "unsupported_feature", "This service doesn't support that.");
    }
    return true;
}

int main(int argc, char **argv)
{
    if (argc == 2 && std::string(argv[1]) == "--graphics-preferences") {
        const auto *payload = std::getenv("OPENNOW_TEST_GPU_BOOTSTRAP");
        if (payload && std::string(payload) == "delay") {
            std::this_thread::sleep_for(std::chrono::seconds(5));
            return 0;
        }
        std::cout << (payload ? payload : "{\"version\":1,\"windowsGpuDeviceId\":\"fixture-gpu\"}") << '\n';
        return 0;
    }
    std::string eofMarker;
    bool launchInConsoleMode = false;
    int consoleModeWriteCount = 0;
    int startupAcknowledgements = 0;
    int createReceipts = 0;
    std::unordered_map<std::string, int> busyAttempts;
    if (argc == 3 && std::string(argv[1]) == "--eof-marker") {
        eofMarker = argv[2];
    }
    FakePluginRegistry pluginRegistry;
    const bool pluginsEnabled = std::getenv("OPENNOW_TEST_PLUGINS") != nullptr;
    const bool sourcesEnabled = std::getenv("OPENNOW_TEST_SOURCES") != nullptr;
    const bool sourcesOnly = std::getenv("OPENNOW_TEST_SOURCES_ONLY") != nullptr;
    FakeSourceRegistry sourceRegistry;
    std::string line;
    while (std::getline(std::cin, line)) {
        const auto id = field(line, "id");
        const auto method = field(line, "method");
        if (field(line, "type") == "ack") {
            ++createReceipts;
        } else if (method == "test.create-receipts") {
            std::cout << "{\"type\":\"response\",\"id\":\"" << id
                      << "\",\"ok\":true,\"result\":{\"receipts\":" << createReceipts << "}}\n" << std::flush;
        } else if (field(line, "type") == "cancel") {
            continue;
        } else if (method == "core.hello") {
            const auto protocolVersion = std::getenv("OPENNOW_TEST_OLD_CORE") ? 4 : 5;
            std::cout << "{\"type\":\"response\",\"id\":\"" << id
                      << "\",\"ok\":true,\"result\":{\"protocolVersion\":" << protocolVersion
                      << (sourcesOnly ? ",\"capabilities\":[\"settings\",\"sources.v2\""
                          : ",\"capabilities\":[\"settings\",\"catalog.libraryPages.v1\",\"catalog.metadata.v1\",\"account.syncObservation.v1\",\"catalog.languages.v1\",\"nativeStreamer.v7\",\"nativeStreamer.ownedNvstNegotiation\"")
                      << (sourcesOnly || std::getenv("OPENNOW_TEST_NO_QUEUE_CAPABILITY") ? "" : ",\"queue.servers.v1\"")
                      << (sourcesEnabled && !sourcesOnly ? ",\"sources.v2\"" : "")
                      << (pluginsEnabled ? ",\"plugins.v1\",\"sources.catalog.v1\"" : "")
                      << [] {
                             if (!std::getenv("OPENNOW_TEST_CAPABILITY_FLOOD")) return std::string{};
                             std::string extra = ",\"settings\",\"" + std::string(200, 'x') + "\",42";
                             for (int index = 0; index < 200; ++index) extra += ",\"flood." + std::to_string(index) + "\"";
                             return extra;
                         }()
                      << "]}}\n" << std::flush;
        } else if (pluginsEnabled && handlePluginRequest(pluginRegistry, line, id, method)) {
            continue;
        } else if ((sourcesEnabled || sourcesOnly) && handleSourceRequest(sourceRegistry, line, id, method)) {
            continue;
        } else if (method == "updater.startup.ack") {
            ++startupAcknowledgements;
            std::cout << "{\"type\":\"response\",\"id\":\"" << id
                      << "\",\"ok\":true,\"result\":{\"acknowledged\":true}}\n" << std::flush;
        } else if (method == "test.app-context") {
            const auto executable = std::getenv("OPENNOW_APP_EXECUTABLE");
            const auto pid = std::getenv("OPENNOW_APP_PID");
            const auto pictures = std::getenv("OPENNOW_PICTURES_DIR");
            std::cout << "{\"type\":\"response\",\"id\":\"" << id
                      << "\",\"ok\":true,\"result\":{\"executable\":" << std::quoted(executable ? executable : "")
                      << ",\"pid\":" << std::quoted(pid ? pid : "")
                      << ",\"picturesDirectory\":" << std::quoted(pictures ? pictures : "")
                      << ",\"hasPicturesDirectory\":" << (pictures ? "true" : "false")
                      << ",\"startupAcknowledgements\":" << startupAcknowledgements
                      << ",\"hasUpdateEnvironment\":" << (std::getenv("OPENNOW_UPDATE_PLAN") && std::getenv("OPENNOW_UPDATE_NONCE") ? "true" : "false")
                      << "}}\n" << std::flush;
        } else if (method == "session.create" || method == "streamer.prepare" || method == "settings.choices.get") {
            if (line.find("\"delayReceipt\":true") != std::string::npos)
                std::this_thread::sleep_for(std::chrono::milliseconds(150));
            std::cout << "{\"type\":\"response\",\"id\":\"" << id
                      << "\",\"ok\":true,\"result\":" << line << "}\n" << std::flush;
        } else if (method == "settings.get") {
            std::cout << "{\"type\":\"response\",\"id\":\"" << id
                      << "\",\"ok\":true,\"result\":{\"settings\":{\"launchInConsoleMode\":"
                      << (launchInConsoleMode ? "true" : "false")
                      << ",\"switchToConsoleOnPad\":false,\"reducedMotion\":true,\"appLanguage\":\"system\",\"autoCheckForUpdates\":false,\"webrtcCompatibilityMode\":\"auto\"}}}\n" << std::flush;
        } else if (method == "settings.set") {
            const auto consoleModeWrite = line.find("\"key\":\"launchInConsoleMode\"")
                != std::string::npos;
            if (consoleModeWrite && consoleModeWriteCount++ == 5) {
                std::cout << "{\"type\":\"response\",\"id\":\"" << id
                          << "\",\"ok\":false,\"error\":{\"code\":\"settings_write_failed\",\"message\":\"Fixture denied settings persistence\"}}\n" << std::flush;
            } else if (consoleModeWrite) {
                launchInConsoleMode = line.find("\"value\":true") != std::string::npos;
                std::cout << "{\"type\":\"event\",\"name\":\"settings.changed\",\"payload\":{\"key\":\"launchInConsoleMode\",\"value\":"
                          << (launchInConsoleMode ? "true" : "false")
                          << (launchInConsoleMode ? "" : ",\"changes\":{\"switchToConsoleOnPad\":false}") << "}}\n";
                std::cout << "{\"type\":\"response\",\"id\":\"" << id
                          << "\",\"ok\":true,\"result\":{\"key\":\"launchInConsoleMode\",\"value\":"
                          << (launchInConsoleMode ? "true" : "false")
                          << (launchInConsoleMode ? "" : ",\"changes\":{\"switchToConsoleOnPad\":false}") << "}}\n" << std::flush;
            } else {
                std::cout << "{\"type\":\"response\",\"id\":\"" << id
                          << "\",\"ok\":true,\"result\":{}}\n" << std::flush;
            }
        } else if (method == "auth.providers.list") {
            std::cout << "{\"type\":\"response\",\"id\":\"" << id
                      << "\",\"ok\":true,\"result\":{\"providers\":[]}}\n" << std::flush;
        } else if (method == "auth.session.get" || method == "session.active.get") {
            std::cout << "{\"type\":\"response\",\"id\":\"" << id
                      << "\",\"ok\":true,\"result\":{\"session\":null}}\n" << std::flush;
        } else if (method == "catalog.public.list") {
            std::cout << "{\"type\":\"response\",\"id\":\"" << id
                      << "\",\"ok\":true,\"result\":{\"games\":[],\"totalCount\":0}}\n" << std::flush;
        } else if (method == "catalog.store.list") {
            std::cout << "{\"type\":\"response\",\"id\":\"" << id
                      << "\",\"ok\":true,\"result\":{\"games\":[],\"totalCount\":0,\"source\":\"store-browse\",\"hasNextPage\":false,\"nextCursor\":\"\"}}\n" << std::flush;
        } else if (method == "catalog.store.presentation") {
            std::cout << "{\"type\":\"response\",\"id\":\"" << id
                      << "\",\"ok\":true,\"result\":{\"section\":\"" << field(line, "section")
                      << "\",\"items\":[]}}\n" << std::flush;
        } else if (method == "test.streamer-event") {
            std::cout << "{\"type\":\"event\",\"name\":\"streamer.changed\",\"payload\":{\"status\":\"streaming\",\"sessionId\":\"fixture-session\",\"firstFrameLatencyMs\":37,\"mediaBackend\":\"ffmpeg\",\"deviceRecoveryCount\":2,\"queueDropCount\":4}}\n";
            std::cout << "{\"type\":\"response\",\"id\":\"" << id
                      << "\",\"ok\":true,\"result\":{}}\n" << std::flush;
        } else if (method == "test.streamer-nested-event") {
            std::cout << "{\"type\":\"event\",\"name\":\"streamer.changed\",\"payload\":{\"streamer\":{\"status\":\"streaming\",\"sessionId\":\"fixture-session\"}}}\n";
            std::cout << "{\"type\":\"response\",\"id\":\"" << id
                      << "\",\"ok\":true,\"result\":{}}\n" << std::flush;
        } else if (method == "test.busy" || method == "test.busy-forever") {
            const auto attempt = ++busyAttempts[id];
            if (method == "test.busy-forever" || attempt <= 2) {
                std::cout << "{\"type\":\"response\",\"id\":\"" << id
                          << "\",\"ok\":false,\"error\":{\"code\":\"busy\",\"message\":\"Core request limit reached\"}}\n";
            } else {
                std::cout << "{\"type\":\"response\",\"id\":\"" << id
                          << "\",\"ok\":true,\"result\":" << line << "}\n";
            }
            std::cout << "{\"type\":\"event\",\"name\":\"test.busy-attempt\",\"payload\":{\"attempt\":"
                      << attempt << "}}\n" << std::flush;
        } else if (method == "test.echo") {
            std::cout << "{\"type\":\"response\",\"id\":\"" << id
                      << "\",\"ok\":true,\"result\":{\"value\":\"pong\"}}\n" << std::flush;
        } else if (method == "test.event") {
            std::cout << "{\"type\":\"event\",\"name\":\"catalog.changed\",\"payload\":{\"revision\":2}}\n";
            std::cout << "{\"type\":\"response\",\"id\":\"" << id
                      << "\",\"ok\":true,\"result\":{}}\n" << std::flush;
        } else if (method == "test.malformed-event-batch") {
            std::cout << "{invalid json}\n"
                      << "{\"type\":\"event\",\"name\":\"session.changed\",\"payload\":{\"status\":\"streaming\"}}\n"
                      << std::flush;
        } else if (method == "test.error") {
            std::cout << "{\"type\":\"response\",\"id\":\"" << id
                      << "\",\"ok\":false,\"error\":{\"code\":\"expected\",\"message\":\"Expected failure\"}}\n" << std::flush;
        } else if (method == "test.hang") {
            continue;
        } else if (method == "test.partial") {
            std::cout << "{\"type\":\"response\",\"id\":\"" << id << std::flush;
            std::this_thread::sleep_for(std::chrono::milliseconds(25));
            std::cout << "\",\"ok\":true,\"result\":{\"fragmented\":true}}\n" << std::flush;
        } else if (method == "test.stderr") {
            std::cerr << "native-streamer: decoder diagnostic\n" << std::flush;
            std::cout << "{\"type\":\"response\",\"id\":\"" << id
                      << "\",\"ok\":true,\"result\":{}}\n" << std::flush;
        } else if (method == "test.exit") {
            return 23;
        } else {
            std::cout << "{\"type\":\"response\",\"id\":\"" << id
                      << "\",\"ok\":true,\"result\":{}}\n" << std::flush;
        }
    }
    if (!eofMarker.empty()) {
        std::ofstream marker(eofMarker, std::ios::binary | std::ios::trunc);
        marker << "graceful";
        marker.flush();
    }
    return 0;
}
