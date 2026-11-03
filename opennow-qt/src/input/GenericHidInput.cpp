#include "input/GenericHidInput.h"

#include <QElapsedTimer>
#include <QCryptographicHash>
#include <QMutexLocker>
#include <QTimer>

#include <SDL3/SDL_hidapi.h>

#include <algorithm>
#include <cerrno>
#include <utility>

#ifdef Q_OS_LINUX
#include "input/platform/LinuxHidIdentity.h"
#include <fcntl.h>
#include <linux/hidraw.h>
#include <sys/ioctl.h>
#include <unistd.h>
#endif
#ifdef Q_OS_WIN
#include <windows.h>
#include <setupapi.h>
#include <initguid.h>
#include <devpkey.h>
#include <hidsdi.h>
#endif
#ifdef Q_OS_MACOS
#include <CoreFoundation/CoreFoundation.h>
#include <IOKit/IOKitLib.h>
#include <IOKit/hid/IOHIDDevice.h>
#endif

namespace {
constexpr std::array<std::pair<quint16, quint16>, 18> allowlist{{
    {0x046d, 0xc262}, {0x046d, 0xc24f}, {0x046d, 0xc26e}, {0x046d, 0xc266},
    {0x046d, 0xc272}, {0x046d, 0xc268}, {0x046d, 0xc276},
    {0x044f, 0xb67c}, {0x044f, 0xb68d}, {0x044f, 0xb68f}, {0x044f, 0x0402},
    {0x044f, 0x0404}, {0x044f, 0xb687}, {0x044f, 0xb697},
    {0x06a3, 0x075c}, {0x06a3, 0x0762}, {0x0738, 0x2221}, {0x0738, 0xa221}
}};

struct SupportedDevice {
    const char *name;
    const char *type;
    const char *evidence;
    const char *uncertainty;
};
constexpr const char *usbIds = "https://github.com/usbids/usbids/blob/master/usb.ids";
constexpr const char *logitechIds = "https://github.com/berarma/oversteer/blob/master/data/udev/99-logitech-wheel-perms.rules";
constexpr std::array<SupportedDevice, 18> supported{{
    {"Logitech G920", "wheel", usbIds, ""},
    {"Logitech G29 (PS3 mode)", "wheel", usbIds, ""},
    {"Logitech G923 (Xbox/PC)", "wheel", logitechIds, ""},
    {"Logitech G923 (PlayStation/PC)", "wheel", logitechIds, ""},
    {"Logitech G PRO (Xbox/PC) / RS50 compatibility mode", "wheel", "https://github.com/mescon/logitech-trueforce-linux-driver", "This PID is shared by two wheel models."},
    {"Logitech G PRO (PlayStation/PC)", "wheel", logitechIds, ""},
    {"Logitech RS50", "wheel", "https://github.com/mescon/logitech-trueforce-linux-driver/issues/21", ""},
    {"Thrustmaster T.Flight Hotas 4", "stick", "https://linux-hardware.org/?id=usb:044f-b67c", "PID alone does not confirm PS4 mode."},
    {"Thrustmaster T.Flight Hotas One (PC mode)", "stick", "https://forums.flightsimulator.com/t/thrustmaster-hotas-one-not-working-in-geforce-now/757328", ""},
    {"Thrustmaster TPR Pendular Rudder", "pedals", "https://linux-hardware.org/?id=usb:044f-b68f", ""},
    {"Thrustmaster HOTAS Warthog Joystick", "stick", usbIds, ""},
    {"Thrustmaster HOTAS Warthog Throttle", "throttle", usbIds, ""},
    {"Thrustmaster TWCS Throttle", "throttle", usbIds, ""},
    {"Thrustmaster T80 / T98 Ferrari 296 GTB", "wheel", "https://treexy.com/products/driver-fusion/database/id/usb/vid_044f/pid_b697/", "Model depends on revision; this PID is shared. Mode is not confirmed."},
    {"Saitek X52", "stick", usbIds, ""},
    {"Saitek X52 Pro", "stick", usbIds, ""},
    {"Saitek X56 Stick", "stick", "https://github.com/Chryseus/x56linux", ""},
    {"Saitek X56 Throttle", "throttle", "https://github.com/Chryseus/x56linux", ""}
}};

QString vidPid(quint16 vendor, quint16 product)
{
    return QStringLiteral("%1:%2").arg(vendor, 4, 16, QLatin1Char('0')).arg(product, 4, 16, QLatin1Char('0'));
}

#ifdef Q_OS_MACOS
io_service_t hidService(const QByteArray &path)
{
    if (path.startsWith("DevSrvsID:")) {
        bool valid = false;
        const auto id = path.mid(10).toULongLong(&valid);
        return valid && id ? IOServiceGetMatchingService(kIOMainPortDefault, IORegistryEntryIDMatching(id)) : 0;
    }
    return IORegistryEntryFromPath(kIOMainPortDefault, path.constData());
}
#endif

struct DeviceIdentity {
    QByteArray physicalKey;
    QByteArray instanceKey;
};

DeviceIdentity deviceIdentity(const GenericHidBackend::Device &device)
{
#ifdef Q_OS_LINUX
    const auto identity = LinuxHidIdentity::resolve(device);
    return {identity.physicalKey, identity.instanceKey};
#elif defined(Q_OS_WIN)
    const GUID hidGuid = {0x4d1e55b2, 0xf16f, 0x11cf, {0x88, 0xcb, 0x00, 0x11, 0x11, 0x00, 0x00, 0x30}};
    const auto set = SetupDiGetClassDevsW(&hidGuid, nullptr, nullptr, DIGCF_PRESENT | DIGCF_DEVICEINTERFACE);
    if (set == INVALID_HANDLE_VALUE) return {};
    DeviceIdentity result;
    for (DWORD index = 0; ; ++index) {
        SP_DEVICE_INTERFACE_DATA interfaceData{};
        interfaceData.cbSize = sizeof(interfaceData);
        if (!SetupDiEnumDeviceInterfaces(set, nullptr, &hidGuid, index, &interfaceData)) break;
        DWORD required = 0;
        SetupDiGetDeviceInterfaceDetailW(set, &interfaceData, nullptr, 0, &required, nullptr);
        if (required == 0 || required > 65536) continue;
        QByteArray storage(static_cast<qsizetype>(required), 0);
        auto *detail = reinterpret_cast<SP_DEVICE_INTERFACE_DETAIL_DATA_W *>(storage.data());
        detail->cbSize = sizeof(SP_DEVICE_INTERFACE_DETAIL_DATA_W);
        SP_DEVINFO_DATA info{};
        info.cbSize = sizeof(info);
        if (!SetupDiGetDeviceInterfaceDetailW(set, &interfaceData, detail, required, nullptr, &info)
            || QString::fromWCharArray(detail->DevicePath).compare(QString::fromUtf8(device.path), Qt::CaseInsensitive)) continue;
        GUID container{};
        DEVPROPTYPE type = 0;
        DWORD size = 0;
        if (SetupDiGetDevicePropertyW(set, &info, &DEVPKEY_Device_ContainerId, &type,
                reinterpret_cast<PBYTE>(&container), sizeof(container), &size, 0)
            && type == DEVPROP_TYPE_GUID && size == sizeof(container) && container != GUID{}) {
            FILETIME arrival{};
            if (SetupDiGetDevicePropertyW(set, &info, &DEVPKEY_Device_LastArrivalDate, &type,
                    reinterpret_cast<PBYTE>(&arrival), sizeof(arrival), &size, 0)
                && type == DEVPROP_TYPE_FILETIME && size == sizeof(arrival)
                && (arrival.dwHighDateTime || arrival.dwLowDateTime)) {
                result.physicalKey = QByteArray(reinterpret_cast<const char *>(&container), sizeof(container));
                result.instanceKey = device.path + char(0) + QByteArray(reinterpret_cast<const char *>(&arrival), sizeof(arrival));
            }
        }
        break;
    }
    SetupDiDestroyDeviceInfoList(set);
    return result;
#elif defined(Q_OS_MACOS)
    auto service = hidService(device.path);
    uint64_t instance = 0;
    if (!service) return {};
    if (IORegistryEntryGetRegistryEntryID(service, &instance) != KERN_SUCCESS || !instance) {
        IOObjectRelease(service);
        return {};
    }
    while (service) {
        const auto vendor = IORegistryEntryCreateCFProperty(service, CFSTR("idVendor"), kCFAllocatorDefault, 0);
        const auto product = IORegistryEntryCreateCFProperty(service, CFSTR("idProduct"), kCFAllocatorDefault, 0);
        const auto location = IORegistryEntryCreateCFProperty(service, CFSTR("locationID"), kCFAllocatorDefault, 0);
        qint64 v = 0, p = 0, l = 0;
        const bool valid = vendor && product && location
            && CFGetTypeID(vendor) == CFNumberGetTypeID() && CFGetTypeID(product) == CFNumberGetTypeID()
            && CFGetTypeID(location) == CFNumberGetTypeID()
            && CFNumberGetValue(static_cast<CFNumberRef>(vendor), kCFNumberSInt64Type, &v)
            && CFNumberGetValue(static_cast<CFNumberRef>(product), kCFNumberSInt64Type, &p)
            && CFNumberGetValue(static_cast<CFNumberRef>(location), kCFNumberSInt64Type, &l);
        if (vendor) CFRelease(vendor);
        if (product) CFRelease(product);
        if (location) CFRelease(location);
        if (valid) {
            IOObjectRelease(service);
            if (v != device.vendor || p != device.product || l == 0) return {};
            return {QByteArray::number(quint32(l), 16), QByteArray::number(quint64(instance), 16)};
        }
        io_registry_entry_t parent = 0;
        IORegistryEntryGetParentEntry(service, kIOServicePlane, &parent);
        IOObjectRelease(service);
        service = parent;
    }
#endif
    return {};
}

class SdlHidBackend final : public GenericHidBackend
{
    struct OpenHandle {
        SDL_hid_device *sdl = nullptr;
#ifdef Q_OS_LINUX
        int output = -1;
#elif defined(Q_OS_WIN)
        HANDLE output = INVALID_HANDLE_VALUE;
#elif defined(Q_OS_MACOS)
        IOHIDDeviceRef output = nullptr;
#endif
    };

public:
    bool initialize() override { return SDL_hid_init() == 0; }
    void shutdown() override { SDL_hid_exit(); }
    QList<Device> enumerate() override
    {
        QList<Device> devices;
        for (const auto [vendor, product] : allowlist) {
            auto *head = SDL_hid_enumerate(vendor, product);
            for (auto *entry = head; entry; entry = entry->next) {
                Device device;
                device.path = entry->path;
                device.vendor = entry->vendor_id;
                device.product = entry->product_id;
                device.revision = entry->release_number;
                device.interfaceNumber = entry->interface_number;
                device.usagePage = entry->usage_page;
                device.usage = entry->usage;
                if (entry->serial_number) device.serial = QString::fromWCharArray(entry->serial_number);
                const auto identity = deviceIdentity(device);
                device.physicalKey = identity.physicalKey;
                device.instanceKey = identity.instanceKey;
                devices.append(device);
            }
            SDL_hid_free_enumeration(head);
        }
        return devices;
    }
    Handle open(const Device &device) override
    {
        auto *sdl = SDL_hid_open_path(device.path.constData());
        if (!sdl) return nullptr;
        if (SDL_hid_set_nonblocking(sdl, 1) != 0) {
            SDL_hid_close(sdl);
            return nullptr;
        }
        const auto *info = SDL_hid_get_device_info(sdl);
        if (!info || info->vendor_id != device.vendor || info->product_id != device.product
            || info->release_number != device.revision || info->interface_number != device.interfaceNumber
            || (info->serial_number ? QString::fromWCharArray(info->serial_number) : QString{}) != device.serial) {
            SDL_hid_close(sdl);
            return nullptr;
        }
        auto *entry = new OpenHandle{.sdl = sdl};
#ifdef Q_OS_LINUX
        entry->output = ::open(device.path.constData(), O_RDWR | O_CLOEXEC | O_NONBLOCK);
#elif defined(Q_OS_WIN)
        const auto path = QString::fromUtf8(device.path);
        entry->output = CreateFileW(reinterpret_cast<LPCWSTR>(path.utf16()), GENERIC_READ | GENERIC_WRITE,
            FILE_SHARE_READ | FILE_SHARE_WRITE, nullptr, OPEN_EXISTING, 0, nullptr);
#elif defined(Q_OS_MACOS)
        const auto service = hidService(device.path);
        if (service) {
            entry->output = IOHIDDeviceCreate(kCFAllocatorDefault, service);
            IOObjectRelease(service);
            if (entry->output && IOHIDDeviceOpen(entry->output, kIOHIDOptionsTypeNone) != kIOReturnSuccess) {
                CFRelease(entry->output);
                entry->output = nullptr;
            }
        }
#endif
        const auto identity = deviceIdentity(device);
        if (identity.physicalKey != device.physicalKey || identity.instanceKey != device.instanceKey) {
            close(entry);
            return nullptr;
        }
        return entry;
    }
    bool accessDenied(const Device &device) override
    {
#ifdef Q_OS_LINUX
        return ::access(device.path.constData(), R_OK | W_OK) != 0 && errno == EACCES;
#else
        Q_UNUSED(device);
        return false;
#endif
    }
    void close(Handle handle) override
    {
        auto *entry = static_cast<OpenHandle *>(handle);
#ifdef Q_OS_LINUX
        if (entry->output >= 0) ::close(entry->output);
#elif defined(Q_OS_WIN)
        if (entry->output != INVALID_HANDLE_VALUE) CloseHandle(entry->output);
#elif defined(Q_OS_MACOS)
        if (entry->output) {
            IOHIDDeviceClose(entry->output, kIOHIDOptionsTypeNone);
            CFRelease(entry->output);
        }
#endif
        SDL_hid_close(entry->sdl);
        delete entry;
    }
    QByteArray descriptor(Handle handle) override
    {
        QByteArray bytes(4097, 0);
        const auto length = SDL_hid_get_report_descriptor(static_cast<OpenHandle *>(handle)->sdl,
            reinterpret_cast<unsigned char *>(bytes.data()), bytes.size());
        if (length <= 0) return {};
        if (length > 4096) return bytes;
        bytes.resize(length);
        return bytes;
    }
    int read(Handle handle, unsigned char *bytes, int capacity) override
    {
        return SDL_hid_read_timeout(static_cast<OpenHandle *>(handle)->sdl, bytes, capacity, 0);
    }
    int output(Handle handle, quint8 operation, unsigned char *bytes, int length) override
    {
        auto *entry = static_cast<OpenHandle *>(handle);
        switch (operation) {
        case 0: return SDL_hid_write(entry->sdl, bytes, length);
        case 2: return SDL_hid_get_input_report(entry->sdl, bytes, length);
        case 3: return SDL_hid_get_feature_report(entry->sdl, bytes, length);
        case 4: return SDL_hid_send_feature_report(entry->sdl, bytes, length);
        case 1:
#ifdef Q_OS_LINUX
            if (entry->output < 0) return -1;
            return ioctl(entry->output, HIDIOCSOUTPUT(length), bytes);
#elif defined(Q_OS_WIN)
            return entry->output != INVALID_HANDLE_VALUE && HidD_SetOutputReport(entry->output, bytes, length) ? length : -1;
#elif defined(Q_OS_MACOS)
            if (!entry->output) return -1;
            return IOHIDDeviceSetReport(entry->output, kIOHIDReportTypeOutput, bytes[0],
                bytes[0] == 0 ? bytes + 1 : bytes, bytes[0] == 0 ? length - 1 : length) == kIOReturnSuccess ? length : -1;
#else
            return -1;
#endif
        default: return -1;
        }
    }
};

struct Collection {
    GenericHidBackend::Device device;
    GenericHidBackend::Handle handle = nullptr;
    QByteArray descriptor;
    QSet<quint8> reportIds;
};

bool reportIds(const QByteArray &descriptor, QSet<quint8> &ids)
{
    int depth = 0;
    for (qsizetype offset = 0; offset < descriptor.size();) {
        const auto tag = quint8(descriptor[offset++]);
        if (tag == 0xfe) return false;
        const auto size = (tag & 3) == 3 ? 4 : tag & 3;
        if (offset + size > descriptor.size()) return false;
        if (tag == 0x85) {
            const auto id = quint8(descriptor[offset]);
            if (!id) return false;
            ids.insert(id);
        }
        if (tag == 0xa4 && ++depth > 32) return false;
        if (tag == 0xb4 && --depth < 0) return false;
        offset += size;
    }
    return depth == 0 && ids.size() <= 64;
}

struct Source {
    GenericHidDeviceClaim claim;
    QList<Collection> collections;
    QList<GenericHidBackend::Device> devices;
};

bool sameDevices(const QList<GenericHidBackend::Device> &first, const QList<GenericHidBackend::Device> &second)
{
    if (first.size() != second.size()) return false;
    for (qsizetype i = 0; i < first.size(); ++i) {
        if (first[i].path != second[i].path || first[i].instanceKey != second[i].instanceKey
            || first[i].interfaceNumber != second[i].interfaceNumber || first[i].revision != second[i].revision
            || first[i].serial != second[i].serial || first[i].usagePage != second[i].usagePage
            || first[i].usage != second[i].usage) return false;
    }
    return true;
}
}

GenericHidInput::GenericHidInput(std::shared_ptr<GenericHidBackend> backend, QObject *parent)
    : QThread(parent), m_backend(backend ? std::move(backend) : std::make_shared<SdlHidBackend>())
{
    setObjectName(QStringLiteral("genericHidWorker"));
    start();
}

GenericHidInput::~GenericHidInput() { stop(); }

void GenericHidInput::stop()
{
    requestInterruption();
    clearOutputs();
    quit();
    wait();
}

bool GenericHidInput::allowed(quint16 vendor, quint16 product)
{
    return std::find(allowlist.begin(), allowlist.end(), std::pair{vendor, product}) != allowlist.end();
}

QVariantMap GenericHidInput::deviceMetadata(quint16 vendor, quint16 product)
{
    const auto it = std::find(allowlist.begin(), allowlist.end(), std::pair{vendor, product});
    if (it == allowlist.end()) return {};
    const auto &entry = supported[std::distance(allowlist.begin(), it)];
    return {{QStringLiteral("name"), QString::fromUtf8(entry.name)},
        {QStringLiteral("type"), QString::fromUtf8(entry.type)},
        {QStringLiteral("vidPid"), vidPid(vendor, product)},
        {QStringLiteral("evidence"), QString::fromUtf8(entry.evidence)},
        {QStringLiteral("uncertainty"), QString::fromUtf8(entry.uncertainty)}};
}

QVariantList GenericHidInput::supportedDevices()
{
    QVariantList result;
    for (const auto [vendor, product] : allowlist) result.append(deviceMetadata(vendor, product));
    return result;
}

QList<GenericHidInput::DetectedDevice> GenericHidInput::devices() const
{
    QMutexLocker lock(&m_mutex);
    return m_devices;
}

void GenericHidInput::setForwardingEnabled(bool enabled)
{
    QMutexLocker lock(&m_mutex);
    if (m_forwardingEnabled == enabled) return;
    m_forwardingEnabled = enabled;
    ++m_forwardingEpoch;
    ++m_outputEpoch;
    m_outputs.clear();
    m_outputBytes = 0;
    for (auto &lane : m_lanes) lane = {};
    for (auto &device : m_devices) device.incarnation = 0;
    markInventoryChangedLocked();
}

QByteArray GenericHidInput::groupKey(const GenericHidBackend::Device &device)
{
    if (!allowed(device.vendor, device.product) || device.physicalKey.isEmpty()) return {};
    return QByteArray::number(device.vendor, 16) + ':' + QByteArray::number(device.product, 16) + ':' + device.physicalKey;
}

QByteArray GenericHidInput::physicalDeviceKey(quint16 vendor, quint16 product, const char *path)
{
    if (!path || !allowed(vendor, product)) return {};
    GenericHidBackend::Device device;
    device.path = path;
    device.vendor = vendor;
    device.product = product;
    device.physicalKey = deviceIdentity(device).physicalKey;
    return groupKey(device);
}

QList<GenericHidDeviceClaim> GenericHidInput::claims() const
{
    QMutexLocker lock(&m_mutex);
    QList<GenericHidDeviceClaim> claims;
    for (const auto &lane : m_lanes) if (lane.claim.incarnation) claims.append(lane.claim);
    return claims;
}

GenericHidInput::Batch GenericHidInput::drain(int limit)
{
    QMutexLocker lock(&m_mutex);
    Batch batch;
    batch.inventoryChanged = std::exchange(m_inventoryChanged, false);
    batch.diagnostics = std::exchange(m_diagnostics, {});
    batch.scanGeneration = m_scanGeneration;
    if (m_droppedDiagnostics)
        batch.diagnostics.append(QStringLiteral("generic-hid %1 diagnostics dropped").arg(std::exchange(m_droppedDiagnostics, 0)));
    for (const auto &lane : m_lanes) if (lane.claim.incarnation) batch.claims.append(lane.claim);
    limit = std::clamp(limit, 0, 32);
    int empty = 0;
    while (batch.reports.size() < limit && empty < maxSources) {
        auto &lane = m_lanes[m_drainSlot];
        m_drainSlot = (m_drainSlot + 1) % maxSources;
        if (lane.reports.empty()) {
            ++empty;
            continue;
        }
        empty = 0;
        lane.bytes -= lane.reports.front().bytes.size();
        batch.reports.append(std::move(lane.reports.front()));
        lane.reports.pop_front();
    }
    return batch;
}

void GenericHidInput::removeLocked(quint8 slot, quint64 incarnation)
{
    if (slot >= maxSources || m_lanes[slot].claim.incarnation != incarnation) return;
    m_lanes[slot] = {};
    for (auto &device : m_devices) if (device.incarnation == incarnation) device.incarnation = 0;
    markInventoryChangedLocked();
    m_failedSources.insert(incarnation);
    for (auto it = m_outputs.begin(); it != m_outputs.end();) {
        if (it->incarnation == incarnation) {
            m_outputBytes -= it->bytes.size();
            it = m_outputs.erase(it);
        } else ++it;
    }
}

void GenericHidInput::markInventoryChangedLocked()
{
    if (m_inventoryChanged) return;
    m_inventoryChanged = true;
    emit inventoryChanged();
}

bool GenericHidInput::append(const GenericHidReport &report)
{
    QMutexLocker lock(&m_mutex);
    auto &lane = m_lanes[report.slot];
    if (!m_forwardingEnabled || lane.claim.incarnation != report.incarnation) return false;
    if (lane.reports.size() >= 64 || lane.bytes + report.bytes.size() > 16 * 1024) {
        qWarning("Generic HID input overflow; removing source %u", report.slot);
        removeLocked(report.slot, report.incarnation);
        return false;
    }
    lane.bytes += report.bytes.size();
    lane.reports.push_back(report);
    return true;
}

void GenericHidInput::diagnose(const QString &message)
{
    QMutexLocker lock(&m_mutex);
    if (m_diagnostics.size() >= 32) {
        ++m_droppedDiagnostics;
        return;
    }
    m_diagnostics.append(message);
}

void GenericHidInput::submitOutput(const GenericHidReport &report)
{
    QMutexLocker lock(&m_mutex);
    if (!m_forwardingEnabled || report.slot >= maxSources || !report.incarnation || report.error
        || m_lanes[report.slot].claim.incarnation != report.incarnation) return;
    if (m_outputs.size() >= 32 || m_outputBytes + report.bytes.size() > 8 * 1024
        || report.bytes.isEmpty() || report.bytes.size() > maxReportBytes || report.operation > 4) {
        qWarning("Generic HID output overflow or invalid output; removing source %u", report.slot);
        removeLocked(report.slot, report.incarnation);
        return;
    }
    m_outputBytes += report.bytes.size();
    m_outputs.push_back(report);
}

void GenericHidInput::clearOutputs()
{
    QMutexLocker lock(&m_mutex);
    m_outputs.clear();
    m_outputBytes = 0;
    ++m_outputEpoch;
}

void GenericHidInput::run()
{
    if (!m_backend->initialize()) {
        qWarning("Generic HID initialization failed; allowlisted devices remain excluded from gamepad emulation");
        diagnose(QStringLiteral("generic-hid initialization failed; allowlisted devices are not forwarded"));
        return;
    }
    QHash<QByteArray, Source> sources;
    QHash<QByteArray, QList<GenericHidBackend::Device>> suppressed;
    QSet<QByteArray> noAccess;
    QSet<QByteArray> rejectedPaths;
    quint64 nextIncarnation = 1;
    QElapsedTimer discoveryClock;
    discoveryClock.start();
    bool firstScan = true;
    quint64 workerEpoch = 0;
    QTimer timer;
    auto closeSource = [&](const QByteArray &key, bool suppress) {
        auto it = sources.find(key);
        if (it == sources.end()) return;
        {
            QMutexLocker lock(&m_mutex);
            removeLocked(it->claim.slot, it->claim.incarnation);
        }
        if (suppress) {
            suppressed.insert(key, it->devices);
            diagnose(QStringLiteral("generic-hid %1 slot=%2 removed after a read, output, or queue failure")
                .arg(vidPid(it->claim.vendor, it->claim.product)).arg(it->claim.slot));
        }
        for (const auto &collection : it->collections) m_backend->close(collection.handle);
        sources.erase(it);
    };
    auto tick = [&] {
        if (isInterruptionRequested()) {
            quit();
            return;
        }
        QSet<quint64> failed;
        bool forwarding = false;
        quint64 forwardingEpoch = 0;
        {
            QMutexLocker lock(&m_mutex);
            failed.swap(m_failedSources);
            forwarding = m_forwardingEnabled;
            forwardingEpoch = m_forwardingEpoch;
        }
        if (workerEpoch != forwardingEpoch || !forwarding) {
            for (const auto &key : sources.keys()) closeSource(key, false);
            suppressed.clear();
            noAccess.clear();
            firstScan = true;
            workerEpoch = forwardingEpoch;
        }
        for (const auto &key : sources.keys()) {
            if (failed.contains(sources[key].claim.incarnation)) closeSource(key, true);
        }
        if (firstScan || discoveryClock.elapsed() >= 300) {
            firstScan = false;
            discoveryClock.restart();
            QHash<QByteArray, QList<GenericHidBackend::Device>> groups;
            QHash<QByteArray, DetectedDevice> detected;
            QSet<QByteArray> unresolved;
            for (const auto &device : m_backend->enumerate()) {
                if (!allowed(device.vendor, device.product)) continue;
                if ((device.usagePage == 1 && (device.usage == 1 || device.usage == 2
                        || device.usage == 6 || device.usage == 7))
                    || (device.usagePage == 0x0d && (device.usage == 1 || device.usage == 2
                        || device.usage == 4 || device.usage == 5))) continue;
                const auto key = groupKey(device);
                const auto inventoryKey = key.isEmpty() ? device.path : key;
                if (!inventoryKey.isEmpty() && (detected.contains(inventoryKey) || detected.size() < 64)) {
                    detected.insert(inventoryKey, {
                        QString::fromLatin1(QCryptographicHash::hash(inventoryKey, QCryptographicHash::Sha256).toHex()),
                        key, device.serial, device.vendor, device.product});
                }
                if (key.isEmpty() || device.instanceKey.isEmpty()) {
                    if (!rejectedPaths.contains(device.path)) {
                        qWarning("Generic HID physical or arrival identity unresolved; refusing device");
                        diagnose(QStringLiteral("generic-hid %1 refused: physical or arrival identity unresolved for %2")
                            .arg(vidPid(device.vendor, device.product), QString::fromLocal8Bit(device.path)));
                    }
                    unresolved.insert(device.path);
                    continue;
                }
                auto &group = groups[key];
                const auto previous = std::find_if(group.cbegin(), group.cend(),
                    [&](const auto &entry) { return entry.path == device.path; });
                if (previous != group.cend() && previous->interfaceNumber == device.interfaceNumber
                    && previous->revision == device.revision && previous->serial == device.serial
                    && previous->instanceKey == device.instanceKey) continue;
                group.append(device);
            }
            rejectedPaths = unresolved;
            for (auto it = groups.begin(); it != groups.end(); ++it)
                std::sort(it->begin(), it->end(), [](const auto &a, const auto &b) { return a.path < b.path; });
            for (const auto &key : sources.keys()) {
                if (!groups.contains(key) || !sameDevices(sources[key].devices, groups[key])) closeSource(key, false);
            }
            for (const auto &key : suppressed.keys()) {
                const auto changed = !groups.contains(key) || !sameDevices(suppressed[key], groups[key]);
                const auto accessRestored = !changed && noAccess.contains(key)
                    && std::none_of(groups[key].cbegin(), groups[key].cend(),
                        [&](const auto &device) { return m_backend->accessDenied(device); });
                if (accessRestored) {
                    const auto &device = groups[key].first();
                    diagnose(QStringLiteral("generic-hid %1 access granted; retrying").arg(vidPid(device.vendor, device.product)));
                }
                if (changed || accessRestored) {
                    suppressed.remove(key);
                    noAccess.remove(key);
                }
            }
            for (auto it = groups.cbegin(); it != groups.cend(); ++it) {
                if (!forwarding) break;
                if (sources.contains(it.key()) || suppressed.contains(it.key()) || sources.size() >= maxSources) continue;
                Source source;
                source.devices = it.value();
                const auto &device = source.devices.first();
                source.claim.vendor = device.vendor;
                source.claim.product = device.product;
                source.claim.revision = device.revision;
                source.claim.incarnation = nextIncarnation++;
                std::array<bool, maxSources> used{};
                for (const auto &entry : std::as_const(sources)) used[entry.claim.slot] = true;
                source.claim.slot = std::distance(used.begin(), std::find(used.begin(), used.end(), false));
                bool valid = source.devices.size() <= 16;
                QByteArray deniedPath;
                for (const auto &candidate : source.devices) {
                    if (!valid || candidate.path.isEmpty() || candidate.interfaceNumber < 0 || candidate.interfaceNumber >= 255
                        || candidate.revision != source.claim.revision) {
                        valid = false;
                        break;
                    }
                    if (std::any_of(source.collections.cbegin(), source.collections.cend(),
                            [&](const auto &collection) { return collection.device.path == candidate.path; })) {
                        valid = false;
                        break;
                    }
                    auto *handle = m_backend->open(candidate);
                    if (!handle) {
                        if (m_backend->accessDenied(candidate)) deniedPath = candidate.path;
                        valid = false;
                        break;
                    }
                    Collection collection{.device = candidate, .handle = handle, .descriptor = m_backend->descriptor(handle)};
                    valid = collection.descriptor.size() <= 4096;
                    if (valid && !reportIds(collection.descriptor, collection.reportIds)) {
                        collection.descriptor.clear();
                        collection.reportIds.clear();
                        qWarning("Generic HID descriptor cannot be parsed; using capture-detach fallback");
                        diagnose(QStringLiteral("generic-hid %1 descriptor cannot be parsed on interface %2; using capture-detach fallback")
                            .arg(vidPid(candidate.vendor, candidate.product)).arg(candidate.interfaceNumber));
                    }
                    source.collections.append(collection);
                    if (!valid) break;
                }
                QHash<int, QList<const Collection *>> interfaces;
                for (const auto &collection : std::as_const(source.collections)) interfaces[collection.device.interfaceNumber].append(&collection);
                for (auto group = interfaces.cbegin(); valid && group != interfaces.cend(); ++group) {
                    QByteArray descriptor;
                    QSet<quint8> ids;
                    for (const auto *collection : group.value()) {
                        if (group.value().size() > 1 && (collection->descriptor.isEmpty() || collection->reportIds.isEmpty())) {
                            valid = false;
                            break;
                        }
                        for (const auto id : collection->reportIds) if (ids.contains(id)) valid = false;
                        ids.unite(collection->reportIds);
                        if (group.value().size() == 1) descriptor = collection->descriptor;
                        else descriptor += QByteArray(1, char(0xa4)) + collection->descriptor + QByteArray(1, char(0xb4));
                    }
                    if (descriptor.size() > 4096) valid = false;
                    source.claim.interfaces.append({quint8(group.key()), descriptor});
                }
                if (!valid) {
                    for (const auto &collection : source.collections) m_backend->close(collection.handle);
                    suppressed.insert(it.key(), it.value());
                    if (!deniedPath.isEmpty()) {
                        noAccess.insert(it.key());
                        qWarning("Generic HID device has no read/write access; refusing device");
                        diagnose(QStringLiteral("generic-hid %1 refused: no read/write access to %2; grant it with a udev rule")
                            .arg(vidPid(device.vendor, device.product), QString::fromLocal8Bit(deniedPath)));
                    } else {
                        qWarning("Generic HID interface, descriptor, or collection routing unavailable; refusing device");
                        diagnose(QStringLiteral("generic-hid %1 refused: interface, descriptor, or collection routing unavailable")
                            .arg(vidPid(device.vendor, device.product)));
                    }
                    continue;
                }
                std::sort(source.claim.interfaces.begin(), source.claim.interfaces.end(), [](const auto &a, const auto &b) { return a.number < b.number; });
                {
                    QMutexLocker lock(&m_mutex);
                    if (!m_forwardingEnabled || m_forwardingEpoch != forwardingEpoch) {
                        lock.unlock();
                        for (const auto &collection : source.collections) m_backend->close(collection.handle);
                        continue;
                    }
                    m_lanes[source.claim.slot].claim = source.claim;
                    markInventoryChangedLocked();
                }
                diagnose(QStringLiteral("generic-hid %1 opened slot=%2 interfaces=%3 collections=%4")
                    .arg(vidPid(source.claim.vendor, source.claim.product)).arg(source.claim.slot)
                    .arg(source.claim.interfaces.size()).arg(source.collections.size()));
                sources.insert(it.key(), source);
            }
            QList<DetectedDevice> inventory;
            for (auto entry : std::as_const(detected)) {
                entry.noAccess = noAccess.contains(entry.key);
                if (sources.contains(entry.key)) {
                    entry.slot = sources[entry.key].claim.slot;
                    entry.incarnation = sources[entry.key].claim.incarnation;
                }
                inventory.append(entry);
            }
            std::sort(inventory.begin(), inventory.end(), [](const auto &a, const auto &b) { return a.deviceId < b.deviceId; });
            QMutexLocker lock(&m_mutex);
            if (m_forwardingEpoch != forwardingEpoch) for (auto &entry : inventory) entry.incarnation = 0;
            ++m_scanGeneration;
            if (inventory != m_devices) {
                m_devices = std::move(inventory);
                markInventoryChangedLocked();
            }
        }
        for (int i = 0; i < 32; ++i) {
            if (isInterruptionRequested()) break;
            GenericHidReport request;
            quint64 outputEpoch = 0;
            {
                QMutexLocker lock(&m_mutex);
                if (m_outputs.empty()) break;
                request = std::move(m_outputs.front());
                m_outputs.pop_front();
                m_outputBytes -= request.bytes.size();
                outputEpoch = m_outputEpoch;
            }
            Source *source = nullptr;
            for (auto &entry : sources) if (entry.claim.slot == request.slot && entry.claim.incarnation == request.incarnation) source = &entry;
            if (!source) continue;
            {
                QMutexLocker lock(&m_mutex);
                if (m_lanes[request.slot].claim.incarnation != request.incarnation || m_outputEpoch != outputEpoch) continue;
            }
            GenericHidReport reply{request.slot, request.incarnation, request.interfaceNumber, request.operation, true, {}};
            const bool get = request.operation == 2 || request.operation == 3;
            Collection *selected = nullptr;
            for (auto &collection : source->collections) {
                if (collection.device.interfaceNumber != request.interfaceNumber || collection.descriptor.isEmpty()) continue;
                if (!collection.reportIds.isEmpty() && !collection.reportIds.contains(quint8(request.bytes[0]))) continue;
                if (selected) { selected = nullptr; break; }
                selected = &collection;
            }
            if (!selected) {
                append(reply);
                continue;
            }
            QByteArray bytes = request.bytes;
            int prefix = 0;
            if (get) {
                if (bytes.size() != 3) { append(reply); continue; }
                const auto length = quint8(bytes[1]) | (quint16(quint8(bytes[2])) << 8);
                if (!length || length > maxReportBytes || (selected->reportIds.isEmpty() && bytes[0] != 0)) {
                    append(reply);
                    continue;
                }
                const auto id = bytes[0];
                prefix = id == 0 ? 1 : 0;
                bytes = QByteArray(length + prefix, 0);
                bytes[0] = id;
            } else if (selected->reportIds.isEmpty()) bytes.prepend(char(0));
            {
                QMutexLocker lock(&m_mutex);
                if (m_lanes[request.slot].claim.incarnation != request.incarnation || m_outputEpoch != outputEpoch) continue;
            }
            const auto count = m_backend->output(selected->handle, request.operation,
                reinterpret_cast<unsigned char *>(bytes.data()), bytes.size());
            {
                QMutexLocker lock(&m_mutex);
                if (m_outputEpoch != outputEpoch) continue;
            }
            if (count < 0 || count > bytes.size() || (get && count <= prefix) || (!get && count != bytes.size())) {
                append(reply);
            } else if (get) {
                reply.error = false;
                reply.bytes = bytes.mid(prefix, count - prefix);
                append(reply);
            }
        }
        for (const auto &key : sources.keys()) {
            auto &source = sources[key];
            bool healthy = true;
            for (const auto &collection : std::as_const(source.collections)) {
                for (int i = 0; i < 8 && healthy; ++i) {
                    std::array<unsigned char, maxReportBytes + 1> bytes{};
                    const auto length = m_backend->read(collection.handle, bytes.data(), bytes.size());
                    if (length == 0) break;
                    if (length < 0 || length > maxReportBytes) { healthy = false; break; }
                    GenericHidReport report{source.claim.slot, source.claim.incarnation,
                        quint8(collection.device.interfaceNumber), 0, false,
                        QByteArray(reinterpret_cast<const char *>(bytes.data()), length)};
                    healthy = append(report);
                }
                if (!healthy) break;
            }
            if (!healthy) closeSource(key, true);
        }
        const auto interval = sources.isEmpty() ? std::max<qint64>(1, 300 - discoveryClock.elapsed()) : 4;
        if (timer.interval() != interval) timer.setInterval(static_cast<int>(interval));
    };
    timer.setTimerType(Qt::PreciseTimer);
    timer.setInterval(300);
    connect(&timer, &QTimer::timeout, &timer, tick, Qt::DirectConnection);
    timer.start();
    if (!isInterruptionRequested()) {
        tick();
        exec();
    }
    timer.stop();
    for (const auto &key : sources.keys()) closeSource(key, false);
    {
        QMutexLocker lock(&m_mutex);
        m_outputs.clear();
        m_outputBytes = 0;
        m_failedSources.clear();
    }
    m_backend->shutdown();
}
