#include <iostream>
#include <fstream>
#include <string>
#include <chrono>
#include <thread>

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
    std::string eofMarker;
    bool launchInConsoleMode = false;
    int consoleModeWriteCount = 0;
    if (argc == 3 && std::string(argv[1]) == "--eof-marker") {
        eofMarker = argv[2];
    }
    std::string line;
    while (std::getline(std::cin, line)) {
        const auto id = field(line, "id");
        const auto method = field(line, "method");
        if (method == "core.hello") {
            std::cout << "{\"type\":\"response\",\"id\":\"" << id
                      << "\",\"ok\":true,\"result\":{\"protocolVersion\":1,\"capabilities\":[\"settings\",\"nativeStreamer.v5\",\"nativeStreamer.ownedNvstNegotiation\"]}}\n" << std::flush;
        } else if (method == "settings.get") {
            std::cout << "{\"type\":\"response\",\"id\":\"" << id
                      << "\",\"ok\":true,\"result\":{\"settings\":{\"launchInConsoleMode\":"
                      << (launchInConsoleMode ? "true" : "false")
                      << ",\"reducedMotion\":true,\"appLanguage\":\"system\",\"autoCheckForUpdates\":false}}}\n" << std::flush;
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
                          << (launchInConsoleMode ? "true" : "false") << "}}\n";
                std::cout << "{\"type\":\"event\",\"name\":\"settings.changed\",\"payload\":{\"key\":\"launchInConsoleMode\",\"value\":"
                          << (launchInConsoleMode ? "true" : "false") << "}}\n" << std::flush;
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
                      << "\",\"ok\":true,\"result\":{\"games\":[],\"totalCount\":0,\"source\":\"store-browse\"}}\n" << std::flush;
        } else if (method == "test.streamer-event") {
            std::cout << "{\"type\":\"event\",\"name\":\"streamer.changed\",\"payload\":{\"status\":\"streaming\",\"sessionId\":\"fixture-session\",\"firstFrameLatencyMs\":37,\"mediaBackend\":\"ffmpeg\",\"deviceRecoveryCount\":2,\"queueDropCount\":4}}\n";
            std::cout << "{\"type\":\"response\",\"id\":\"" << id
                      << "\",\"ok\":true,\"result\":{}}\n" << std::flush;
        } else if (method == "test.streamer-nested-event") {
            std::cout << "{\"type\":\"event\",\"name\":\"streamer.changed\",\"payload\":{\"streamer\":{\"status\":\"streaming\",\"sessionId\":\"fixture-session\"}}}\n";
            std::cout << "{\"type\":\"response\",\"id\":\"" << id
                      << "\",\"ok\":true,\"result\":{}}\n" << std::flush;
        } else if (method == "test.echo") {
            std::cout << "{\"type\":\"response\",\"id\":\"" << id
                      << "\",\"ok\":true,\"result\":{\"value\":\"pong\"}}\n" << std::flush;
        } else if (method == "test.event") {
            std::cout << "{\"type\":\"event\",\"name\":\"catalog.changed\",\"payload\":{\"revision\":2}}\n";
            std::cout << "{\"type\":\"response\",\"id\":\"" << id
                      << "\",\"ok\":true,\"result\":{}}\n" << std::flush;
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
