#include <iostream>
#include <fstream>
#include <string>
#include <chrono>
#include <thread>
#include <unordered_map>
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
                      << ",\"capabilities\":[\"settings\",\"catalog.libraryPages.v1\",\"catalog.metadata.v1\",\"account.syncObservation.v1\",\"catalog.languages.v1\",\"nativeStreamer.v7\",\"nativeStreamer.ownedNvstNegotiation\""
                      << (std::getenv("OPENNOW_TEST_NO_QUEUE_CAPABILITY") ? "" : ",\"queue.servers.v1\"")
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
