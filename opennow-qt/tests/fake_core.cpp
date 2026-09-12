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
    std::unordered_map<std::string, int> busyAttempts;
    if (argc == 3 && std::string(argv[1]) == "--eof-marker") {
        eofMarker = argv[2];
    }
    std::string line;
    while (std::getline(std::cin, line)) {
        const auto id = field(line, "id");
        const auto method = field(line, "method");
        if (method == "core.hello") {
            std::cout << "{\"type\":\"response\",\"id\":\"" << id
                      << "\",\"ok\":true,\"result\":{\"protocolVersion\":1,\"capabilities\":[\"settings\",\"nativeStreamer.v7\",\"nativeStreamer.ownedNvstNegotiation\"]}}\n" << std::flush;
        } else if (method == "updater.startup.ack") {
            ++startupAcknowledgements;
            std::cout << "{\"type\":\"response\",\"id\":\"" << id
                      << "\",\"ok\":true,\"result\":{\"acknowledged\":true}}\n" << std::flush;
        } else if (method == "test.app-context") {
            const auto executable = std::getenv("OPENNOW_APP_EXECUTABLE");
            const auto pid = std::getenv("OPENNOW_APP_PID");
            std::cout << "{\"type\":\"response\",\"id\":\"" << id
                      << "\",\"ok\":true,\"result\":{\"executable\":" << std::quoted(executable ? executable : "")
                      << ",\"pid\":" << std::quoted(pid ? pid : "")
                      << ",\"startupAcknowledgements\":" << startupAcknowledgements
                      << ",\"hasUpdateEnvironment\":" << (std::getenv("OPENNOW_UPDATE_PLAN") && std::getenv("OPENNOW_UPDATE_NONCE") ? "true" : "false")
                      << "}}\n" << std::flush;
        } else if (method == "session.create" || method == "streamer.prepare") {
            std::cout << "{\"type\":\"response\",\"id\":\"" << id
                      << "\",\"ok\":true,\"result\":" << line << "}\n" << std::flush;
        } else if (method == "settings.get") {
            std::cout << "{\"type\":\"response\",\"id\":\"" << id
                      << "\",\"ok\":true,\"result\":{\"settings\":{\"launchInConsoleMode\":"
                      << (launchInConsoleMode ? "true" : "false")
                      << ",\"switchToConsoleOnPad\":false,\"reducedMotion\":true,\"appLanguage\":\"system\",\"autoCheckForUpdates\":false}}}\n" << std::flush;
        } else if (method == "settings.set") {
            const auto consoleModeWrite = line.find("\"key\":\"launchInConsoleMode\"")
                != std::string::npos;
            if (consoleModeWrite && consoleModeWriteCount++ == 5) {
                std::cout << "{\"type\":\"response\",\"id\":\"" << id
                          << "\",\"ok\":false,\"error\":{\"code\":\"settings_write_failed\",\"message\":\"Fixture denied settings persistence\"}}\n" << std::flush;
            } else if (consoleModeWrite) {
                launchInConsoleMode = line.find("\"value\":true") != std::string::npos;
                std::cout << "{\"type\":\"response\",\"id\":\"" << id
                          << "\",\"ok\":true,\"result\":{\"key\":\"launchInConsoleMode\",\"value\":"
                          << (launchInConsoleMode ? "true" : "false")
                          << (launchInConsoleMode ? "" : ",\"changes\":{\"switchToConsoleOnPad\":false}") << "}}\n";
                std::cout << "{\"type\":\"event\",\"name\":\"settings.changed\",\"payload\":{\"key\":\"launchInConsoleMode\",\"value\":"
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
