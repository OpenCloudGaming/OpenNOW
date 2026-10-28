#include "input/ControllerInput.h"
#include "input/GenericHidInput.h"
#ifdef Q_OS_LINUX
#include "input/platform/LinuxHidIdentity.h"
#include <QDir>
#include <QFile>
#include <QTemporaryDir>
#endif

#include <QMutexLocker>
#include <QElapsedTimer>
#include <QSemaphore>
#include <QSignalSpy>
#include <QTest>

#include <cstring>

namespace {
#ifdef Q_OS_LINUX
class LinuxSysfs
{
public:
    QTemporaryDir directory;

    QString classRoot() const { return directory.path() + QStringLiteral("/class"); }
    QString usbPath(const QString &port) const { return directory.path() + QStringLiteral("/devices/") + port; }
    QString hidPath(const QString &port) const { return usbPath(port) + QStringLiteral("/interface/hid-arrival"); }

    bool write(const QString &path, const QByteArray &bytes)
    {
        QFile file(path);
        return file.open(QIODevice::WriteOnly) && file.write(bytes) == bytes.size();
    }

    bool add(const QString &node, const QString &port, const QByteArray &hidSerial = "aa-bb-cc-dd",
             const QByteArray &usbSerial = "0000123456789abc")
    {
        const auto link = classRoot() + (node.startsWith(QStringLiteral("event"))
            ? QStringLiteral("/input/") : QStringLiteral("/hidraw/")) + node;
        const auto target = hidPath(port) + (node.startsWith(QStringLiteral("event"))
            ? QStringLiteral("/input/input5") : QString{});
        return directory.isValid() && QDir().mkpath(target) && QDir().mkpath(link)
            && write(usbPath(port) + QStringLiteral("/idVendor"), "046d\n")
            && write(usbPath(port) + QStringLiteral("/idProduct"), "c262\n")
            && write(usbPath(port) + QStringLiteral("/serial"), usbSerial + '\n')
            && write(hidPath(port) + QStringLiteral("/uevent"),
                "HID_ID=0003:0000046D:0000C262\nHID_NAME=Logitech G920\nHID_UNIQ=" + hidSerial + '\n')
            && QFile::link(target, link + QStringLiteral("/device"));
    }

    GenericHidBackend::Device wheel(const char *path = "/dev/hidraw5", const QString &serial = QStringLiteral("aa-bb-cc-dd")) const
    {
        GenericHidBackend::Device result;
        result.path = path;
        result.serial = serial;
        result.vendor = 0x046d;
        result.product = 0xc262;
        result.interfaceNumber = 0;
        const auto identity = LinuxHidIdentity::resolve(result, classRoot());
        result.physicalKey = identity.physicalKey;
        result.instanceKey = identity.instanceKey;
        return result;
    }
};
#endif

QByteArray descriptor(quint8 id = 0)
{
    auto bytes = QByteArray::fromHex("05010904a1010509190129081500250175019508");
    if (id) bytes += QByteArray::fromHex("85") + char(id);
    return bytes + QByteArray::fromHex("8102c0");
}

GenericHidBackend::Device device(const char *path, const char *physical, int interfaceNumber = 0,
                                quint16 product = 0x0402)
{
    return {.path = path, .physicalKey = physical, .instanceKey = QByteArray(path) + ":first",
            .vendor = 0x044f, .product = product,
            .revision = 0x100, .interfaceNumber = interfaceNumber};
}

class FakeBackend final : public GenericHidBackend
{
public:
    struct Open { QByteArray path; };
    struct Output { QByteArray path; quint8 operation; QByteArray bytes; };
    QMutex mutex;
    QList<Device> devices;
    QHash<QByteArray, QByteArray> descriptors;
    QHash<QByteArray, std::deque<QByteArray>> reports;
    QList<Output> outputs;
    QSet<QByteArray> disconnected;
    QSet<QByteArray> denied;
    int opens = 0;
    int closes = 0;
    int scans = 0;
    bool stopped = false;
    bool onGuiThread = false;
    bool blockOutput = false;
    QSemaphore outputEntered;
    QSemaphore outputRelease;

    void checkThread() { onGuiThread |= QThread::currentThread() == QCoreApplication::instance()->thread(); }
    bool initialize() override { QMutexLocker lock(&mutex); checkThread(); return true; }
    void shutdown() override { QMutexLocker lock(&mutex); checkThread(); stopped = true; }
    QList<Device> enumerate() override
    {
        QMutexLocker lock(&mutex);
        checkThread();
        ++scans;
        return devices;
    }
    Handle open(const Device &device) override
    {
        QMutexLocker lock(&mutex);
        checkThread();
        if (denied.contains(device.path)) return nullptr;
        ++opens;
        return new Open{device.path};
    }
    bool accessDenied(const Device &device) override
    {
        QMutexLocker lock(&mutex);
        checkThread();
        return denied.contains(device.path);
    }
    void close(Handle handle) override
    {
        QMutexLocker lock(&mutex);
        checkThread();
        ++closes;
        delete static_cast<Open *>(handle);
    }
    QByteArray descriptor(Handle handle) override
    {
        QMutexLocker lock(&mutex);
        checkThread();
        return descriptors.value(static_cast<Open *>(handle)->path, ::descriptor());
    }
    int read(Handle handle, unsigned char *bytes, int capacity) override
    {
        QMutexLocker lock(&mutex);
        checkThread();
        const auto path = static_cast<Open *>(handle)->path;
        if (disconnected.contains(path)) return -1;
        auto &queue = reports[path];
        if (queue.empty()) return 0;
        auto report = std::move(queue.front());
        queue.pop_front();
        if (report.size() > capacity) return report.size();
        std::memcpy(bytes, report.constData(), report.size());
        return report.size();
    }
    int output(Handle handle, quint8 operation, unsigned char *bytes, int length) override
    {
        bool block = false;
        {
            QMutexLocker lock(&mutex);
            checkThread();
            outputs.append({static_cast<Open *>(handle)->path, operation,
                QByteArray(reinterpret_cast<char *>(bytes), length)});
            block = blockOutput;
        }
        if (block) {
            outputEntered.release();
            outputRelease.tryAcquire(1, 2000);
        }
        if (operation == 2 || operation == 3) {
            for (int i = 1; i < length; ++i) bytes[i] = quint8(i);
        }
        return length;
    }
    void push(const QByteArray &path, int count, QByteArray bytes = QByteArray(1, char(1)))
    {
        QMutexLocker lock(&mutex);
        for (int i = 0; i < count; ++i) reports[path].push_back(bytes);
    }
    int outputCount() { QMutexLocker lock(&mutex); return outputs.size(); }
    int closeCount() { QMutexLocker lock(&mutex); return closes; }
    int scanCount() { QMutexLocker lock(&mutex); return scans; }
};

GenericHidReport output(const GenericHidDeviceClaim &claim, quint8 operation, QByteArray bytes)
{
    return {claim.slot, claim.incarnation, 0, operation, false, bytes};
}

QList<GenericHidReport> receive(GenericHidInput &input, int count)
{
    QList<GenericHidReport> reports;
    QElapsedTimer clock;
    clock.start();
    while (reports.size() < count && clock.elapsed() < 1000) {
        reports.append(input.drain().reports);
        if (reports.size() < count) QTest::qWait(5);
    }
    return reports;
}
}

class GenericHidInputTest final : public QObject
{
    Q_OBJECT
private slots:
#ifdef Q_OS_LINUX
    void linuxHidSerialUsesDeviceNamespace()
    {
        LinuxSysfs sysfs;
        QVERIFY(sysfs.add(QStringLiteral("hidraw5"), QStringLiteral("wheel-a")));
        const auto hid = sysfs.wheel();
        QCOMPARE(hid.physicalKey, sysfs.usbPath(QStringLiteral("wheel-a")).toUtf8());
        QVERIFY(!hid.instanceKey.isEmpty());
        const auto usb = sysfs.wheel("/dev/hidraw5", QStringLiteral("0000123456789abc"));
        QCOMPARE(usb.physicalKey, hid.physicalKey);
        QCOMPARE(usb.instanceKey, hid.instanceKey);
        QVERIFY(sysfs.wheel("/dev/hidraw5", QStringLiteral("unrelated-serial")).physicalKey.isEmpty());
        QVERIFY(sysfs.wheel("/dev/hidraw5", QStringLiteral("AA-BB-CC-DD")).physicalKey.isEmpty());
        QVERIFY(sysfs.add(QStringLiteral("hidraw6"), QStringLiteral("wheel-b"), "11-22-33-44", "different-usb-serial"));
        QVERIFY(sysfs.wheel("/dev/hidraw6").physicalKey.isEmpty());
        QVERIFY(!sysfs.wheel("/dev/hidraw6", QStringLiteral("11-22-33-44")).physicalKey.isEmpty());
        auto wrongProduct = hid;
        wrongProduct.product = 0xc261;
        QVERIFY(LinuxHidIdentity::resolve(wrongProduct, sysfs.classRoot()).physicalKey.isEmpty());
        auto wrongVendor = hid;
        wrongVendor.vendor = 0x044f;
        QVERIFY(LinuxHidIdentity::resolve(wrongVendor, sysfs.classRoot()).physicalKey.isEmpty());
    }

    void linuxHidIdentityRejectsUnrelatedPaths()
    {
        LinuxSysfs sysfs;
        QVERIFY(sysfs.add(QStringLiteral("hidraw5"), QStringLiteral("wheel-a")));
        for (const auto *path : {"/dev/hidraw6", "/dev/input/event5", "hidraw5", "/tmp/hidraw5",
                 "/dev/hidraw/../hidraw5", "/dev/hidraw5/../hidraw5", "/dev/input/event/../../hidraw5"}) {
            QVERIFY2(sysfs.wheel(path, QStringLiteral("0000123456789abc")).physicalKey.isEmpty(), path);
        }
        QVERIFY(QFile::remove(sysfs.classRoot() + QStringLiteral("/hidraw/hidraw5/device")));
        QVERIFY(sysfs.wheel().physicalKey.isEmpty());
    }

    void linuxHidSerialKeepsWheelsAndArrivalsDistinct()
    {
        LinuxSysfs sysfs;
        QVERIFY(sysfs.add(QStringLiteral("hidraw5"), QStringLiteral("wheel-a")));
        QVERIFY(sysfs.add(QStringLiteral("hidraw6"), QStringLiteral("wheel-b")));
        QVERIFY(sysfs.add(QStringLiteral("event5"), QStringLiteral("wheel-a")));
        const auto first = sysfs.wheel();
        const auto second = sysfs.wheel("/dev/hidraw6");
        QVERIFY(!first.physicalKey.isEmpty());
        QVERIFY(!second.physicalKey.isEmpty());
        QVERIFY(first.physicalKey != second.physicalKey);
        QVERIFY(first.instanceKey != second.instanceKey);
        QCOMPARE(sysfs.wheel("/dev/input/event5").physicalKey, first.physicalKey);
        QVERIFY(QDir().rename(sysfs.hidPath(QStringLiteral("wheel-a")),
            sysfs.hidPath(QStringLiteral("wheel-a")) + QStringLiteral("-old")));
        QVERIFY(sysfs.wheel().physicalKey.isEmpty());
        QVERIFY(QDir().mkpath(sysfs.hidPath(QStringLiteral("wheel-a"))));
        QVERIFY(QFile::copy(sysfs.hidPath(QStringLiteral("wheel-a")) + QStringLiteral("-old/uevent"),
            sysfs.hidPath(QStringLiteral("wheel-a")) + QStringLiteral("/uevent")));
        const auto replacement = sysfs.wheel();
        QCOMPARE(replacement.physicalKey, first.physicalKey);
        QVERIFY(!replacement.instanceKey.isEmpty());
        QVERIFY(replacement.instanceKey != first.instanceKey);
        QCOMPARE(sysfs.wheel("/dev/hidraw6").instanceKey, second.instanceKey);
    }

    void linuxHidSerialRequiresMatchingEvidence()
    {
        LinuxSysfs sysfs;
        QVERIFY(sysfs.add(QStringLiteral("hidraw5"), QStringLiteral("wheel-a")));
        QVERIFY(QFile::remove(sysfs.hidPath(QStringLiteral("wheel-a")) + QStringLiteral("/uevent")));
        QVERIFY(sysfs.wheel().physicalKey.isEmpty());
        QVERIFY(!sysfs.wheel("/dev/hidraw5", QStringLiteral("0000123456789abc")).physicalKey.isEmpty());
        QVERIFY(sysfs.write(sysfs.hidPath(QStringLiteral("wheel-a")) + QStringLiteral("/uevent"),
            "OTHER_HID_UNIQ=aa-bb-cc-dd\nHID_UNIQ=other-device\n"));
        QVERIFY(sysfs.wheel().physicalKey.isEmpty());
        QVERIFY(sysfs.write(sysfs.hidPath(QStringLiteral("wheel-a")) + QStringLiteral("/uevent"),
            "HID_UNIQ=aa-bb-cc-dd\n"));
        QVERIFY(sysfs.write(sysfs.usbPath(QStringLiteral("wheel-a")) + QStringLiteral("/idProduct"), "c24f\n"));
        auto otherModel = sysfs.wheel();
        QVERIFY(otherModel.physicalKey.isEmpty());
        otherModel.product = 0xc24f;
        QVERIFY(!LinuxHidIdentity::resolve(otherModel, sysfs.classRoot()).physicalKey.isEmpty());
    }

    void linuxHidArrivalsRetireOnlyReplacedWheel()
    {
        LinuxSysfs sysfs;
        QVERIFY(sysfs.add(QStringLiteral("hidraw5"), QStringLiteral("wheel-a")));
        QVERIFY(sysfs.add(QStringLiteral("hidraw6"), QStringLiteral("wheel-b")));
        const auto first = sysfs.wheel();
        const auto second = sysfs.wheel("/dev/hidraw6");
        QVERIFY(!first.physicalKey.isEmpty());
        QVERIFY(!second.physicalKey.isEmpty());
        auto backend = std::make_shared<FakeBackend>();
        backend->devices = {first, second};
        GenericHidInput input(backend);
        QTRY_COMPARE(input.claims().size(), 2);
        const auto claims = input.claims();
        const auto firstKey = GenericHidInput::groupKey(first);
        const auto devices = input.devices();
        const auto found = std::find_if(devices.cbegin(), devices.cend(),
            [&](const auto &entry) { return entry.key == firstKey; });
        QVERIFY(found != devices.cend());
        const auto oldClaim = std::find_if(claims.cbegin(), claims.cend(),
            [&](const auto &claim) { return claim.incarnation == found->incarnation; });
        QVERIFY(oldClaim != claims.cend());
        const auto old = *oldClaim;
        const auto unaffectedClaim = std::find_if(claims.cbegin(), claims.cend(),
            [&](const auto &claim) { return claim.incarnation != old.incarnation; });
        QVERIFY(unaffectedClaim != claims.cend());
        const auto unaffected = *unaffectedClaim;
        QVERIFY(QDir().rename(sysfs.hidPath(QStringLiteral("wheel-a")),
            sysfs.hidPath(QStringLiteral("wheel-a")) + QStringLiteral("-old")));
        QVERIFY(QDir().mkpath(sysfs.hidPath(QStringLiteral("wheel-a"))));
        QVERIFY(QFile::copy(sysfs.hidPath(QStringLiteral("wheel-a")) + QStringLiteral("-old/uevent"),
            sysfs.hidPath(QStringLiteral("wheel-a")) + QStringLiteral("/uevent")));
        {
            QMutexLocker lock(&backend->mutex);
            backend->devices[0] = sysfs.wheel();
        }
        QTRY_COMPARE(backend->closeCount(), 1);
        QTRY_COMPARE(input.claims().size(), 2);
        const auto replacements = input.claims();
        const auto replacementClaim = std::find_if(replacements.cbegin(), replacements.cend(),
            [&](const auto &claim) { return claim.slot == old.slot; });
        QVERIFY(replacementClaim != replacements.cend());
        const auto replacement = *replacementClaim;
        QVERIFY(replacement.incarnation != old.incarnation);
        QVERIFY(std::any_of(replacements.cbegin(), replacements.cend(),
            [&](const auto &claim) { return claim.incarnation == unaffected.incarnation; }));
        input.submitOutput(output(old, 0, QByteArray(1, char(1))));
        input.submitOutput(output(replacement, 0, QByteArray(1, char(2))));
        input.submitOutput(output(unaffected, 0, QByteArray(1, char(3))));
        QTRY_COMPARE(backend->outputCount(), 2);
        input.stop();
        for (const auto &entry : backend->outputs) {
            QCOMPARE(entry.bytes, entry.path == "/dev/hidraw5" ? QByteArray::fromHex("0002") : QByteArray::fromHex("0003"));
        }
        QCOMPARE(backend->opens, backend->closes);
    }
#endif

    void supportedDeviceMetadataMatchesAllowlist()
    {
        ControllerInput input;
        const auto supported = input.supportedFlightDevices();
        QCOMPARE(supported.size(), 18);
        QSet<QString> ids;
        const QSet<QString> allowedTypes{QStringLiteral("stick"), QStringLiteral("throttle"),
            QStringLiteral("pedals"), QStringLiteral("wheel")};
        QSet<QString> types;
        int wheels = 0;
        for (const auto &value : supported) {
            const auto entry = value.toMap();
            const auto id = entry.value(QStringLiteral("vidPid")).toString();
            const auto parts = id.split(QLatin1Char(':'));
            QCOMPARE(parts.size(), 2);
            QVERIFY(GenericHidInput::allowed(parts[0].toUShort(nullptr, 16), parts[1].toUShort(nullptr, 16)));
            QVERIFY(!entry.value(QStringLiteral("name")).toString().isEmpty());
            const auto type = entry.value(QStringLiteral("type")).toString();
            QVERIFY(allowedTypes.contains(type));
            types.insert(type);
            QVERIFY(entry.value(QStringLiteral("evidence")).toString().startsWith(QStringLiteral("https://")));
            if (entry.value(QStringLiteral("type")).toString() == QStringLiteral("wheel")) ++wheels;
            ids.insert(id);
        }
        QCOMPARE(ids.size(), 18);
        QCOMPARE(types, allowedTypes);
        QCOMPARE(wheels, 8);
        QVERIFY(!GenericHidInput::deviceMetadata(0x044f, 0xb67c).value(QStringLiteral("uncertainty")).toString().isEmpty());
        QVERIFY(!GenericHidInput::deviceMetadata(0x044f, 0xb697).value(QStringLiteral("uncertainty")).toString().isEmpty());
    }

    void forwardingGateKeepsInventoryAndReincarnatesClaims()
    {
        auto backend = std::make_shared<FakeBackend>();
        backend->devices = {device("a", "parent-a"), device("b", "parent-a", 1),
            device("unresolved", "")};
        GenericHidInput input(backend);
        QTRY_COMPARE(input.claims().size(), 1);
        QTRY_COMPARE(input.devices().size(), 2);
        const auto previous = input.claims().first();
        const auto ids = input.devices();
        backend->push("a", 1);
        QTest::qWait(20);
        input.setForwardingEnabled(false);
        QVERIFY(input.claims().isEmpty());
        QVERIFY(input.drain().reports.isEmpty());
        input.submitOutput(output(previous, 1, QByteArray::fromHex("0102")));
        QTRY_COMPARE(backend->closeCount(), 2);
        const auto scans = backend->scanCount();
        QTRY_VERIFY(backend->scanCount() > scans);
        QCOMPARE(backend->opens, 2);
        QCOMPARE(backend->outputCount(), 0);
        QCOMPARE(input.devices().size(), 2);
        for (int i = 0; i < ids.size(); ++i) {
            QCOMPARE(input.devices()[i].deviceId, ids[i].deviceId);
            QCOMPARE(input.devices()[i].incarnation, quint64(0));
        }
        input.setForwardingEnabled(true);
        QTRY_COMPARE(input.claims().size(), 1);
        QVERIFY(input.claims().first().incarnation > previous.incarnation);
        input.stop();
        QCOMPARE(backend->opens, backend->closes);
        QVERIFY(!backend->onGuiThread);
    }

    void controllerInventoryUsesServerAdmissionAndResets()
    {
        auto backend = std::make_shared<FakeBackend>();
        backend->devices = {device("a", "parent-a"), device("b", "parent-a", 1),
            device("unresolved", "")};
        ControllerInput input(nullptr, backend);
        QSignalSpy changes(&input, &ControllerInput::flightDevicesChanged);
        QTRY_COMPARE(input.genericHidClaims().size(), 1);
        QTRY_COMPARE(input.flightDevices().size(), 2);
        QVERIFY(input.forwardFlightControls());
        for (const auto &value : input.flightDevices()) {
            const auto entry = value.toMap();
            QCOMPARE(entry.size(), 6);
            QCOMPARE(entry.value(QStringLiteral("status")).toString(), QStringLiteral("ready"));
            QCOMPARE(entry.value(QStringLiteral("player")).toInt(), 0);
        }
        const auto previous = input.genericHidClaims().first();
        const auto statusCount = [&](const QString &status) {
            int count = 0;
            for (const auto &value : input.flightDevices())
                if (value.toMap().value(QStringLiteral("status")).toString() == status) ++count;
            return count;
        };
        input.setGenericHidAdmission(previous.slot, previous.incarnation + 1, GenericHidAdmission::Admitted);
        QCOMPARE(statusCount(QStringLiteral("active")), 0);
        input.setGenericHidAdmission(previous.slot, previous.incarnation, GenericHidAdmission::Admitted);
        QCOMPARE(statusCount(QStringLiteral("active")), 1);
        input.setGenericHidAdmission(previous.slot, previous.incarnation, GenericHidAdmission::Declined);
        QCOMPARE(statusCount(QStringLiteral("declined")), 1);
        input.resetGenericHidAdmissions();
        QCOMPARE(statusCount(QStringLiteral("ready")), 2);
        input.setGenericHidAdmission(previous.slot, previous.incarnation, GenericHidAdmission::Admitted);
        input.setGenericHidAdmission(previous.slot, previous.incarnation, GenericHidAdmission::Pending);
        QCOMPARE(statusCount(QStringLiteral("ready")), 2);
        input.setGenericHidAdmission(previous.slot, previous.incarnation, GenericHidAdmission::Admitted);
        QSignalSpy reports(&input, &ControllerInput::genericHidReport);
        input.setForwardFlightControls(false);
        QVERIFY(input.genericHidClaims().isEmpty());
        QCOMPARE(statusCount(QStringLiteral("ignored")), 2);
        backend->push("a", 1);
        input.handleGenericHidOutput(output(previous, 1, QByteArray::fromHex("0102")));
        QTest::qWait(350);
        QCOMPARE(reports.size(), 0);
        QCOMPARE(backend->outputCount(), 0);
        input.setForwardFlightControls(true);
        QTRY_COMPARE(input.genericHidClaims().size(), 1);
        QVERIFY(input.genericHidClaims().first().incarnation > previous.incarnation);
        QCOMPARE(statusCount(QStringLiteral("ready")), 2);
        input.setGenericHidAdmission(previous.slot, previous.incarnation, GenericHidAdmission::Admitted);
        QCOMPARE(statusCount(QStringLiteral("active")), 0);
        const auto current = input.genericHidClaims().first();
        input.setGenericHidAdmission(current.slot, current.incarnation, GenericHidAdmission::Declined);
        {
            QMutexLocker lock(&backend->mutex);
            backend->devices.clear();
        }
        QTRY_VERIFY(input.flightDevices().isEmpty());
        QVERIFY(!changes.isEmpty());
    }

    void deniedAccessReportsNoAccessAndRetriesWhenGranted()
    {
        auto backend = std::make_shared<FakeBackend>();
        backend->devices = {device("a", "parent")};
        backend->denied = {"a"};
        ControllerInput input(nullptr, backend);
        QSignalSpy diagnostics(&input, &ControllerInput::inputDiagnostic);
        const auto status = [&] {
            const auto devices = input.flightDevices();
            return devices.size() == 1 ? devices.first().toMap().value(QStringLiteral("status")).toString() : QString{};
        };
        QTRY_COMPARE(status(), QStringLiteral("no-access"));
        QVERIFY(input.genericHidClaims().isEmpty());
        QTRY_VERIFY(std::any_of(diagnostics.cbegin(), diagnostics.cend(), [](const auto &arguments) {
            return arguments.first().toString().contains(QStringLiteral("044f:0402 refused: no read/write access to a"));
        }));
        {
            QMutexLocker lock(&backend->mutex);
            backend->denied.clear();
        }
        QTRY_COMPARE(input.genericHidClaims().size(), 1);
        QTRY_COMPARE(status(), QStringLiteral("ready"));
        const auto claim = input.genericHidClaims().first();
        diagnostics.clear();
        input.setGenericHidAdmission(claim.slot, claim.incarnation, GenericHidAdmission::Declined);
        QCOMPARE(diagnostics.size(), 1);
        QCOMPARE(diagnostics.first().first().toString(), QStringLiteral("generic-hid 044f:0402 slot=0 declined by server"));
        input.setForwardFlightControls(false);
        QCOMPARE(status(), QStringLiteral("ignored"));
    }

    void allowlistedSdlControllerUsesNormalPathWhenForwardingDisabled()
    {
        auto backend = std::make_shared<FakeBackend>();
        ControllerInput input(nullptr, backend);
        SDL_VirtualJoystickDesc virtualDevice{};
        SDL_INIT_INTERFACE(&virtualDevice);
        virtualDevice.type = SDL_JOYSTICK_TYPE_GAMEPAD;
        virtualDevice.naxes = SDL_GAMEPAD_AXIS_COUNT;
        virtualDevice.nbuttons = SDL_GAMEPAD_BUTTON_COUNT;
        virtualDevice.axis_mask = (1u << SDL_GAMEPAD_AXIS_COUNT) - 1;
        virtualDevice.button_mask = (1u << SDL_GAMEPAD_BUTTON_COUNT) - 1;
        virtualDevice.name = "OpenNOW forwarding toggle test";
        virtualDevice.vendor_id = 0x044f;
        virtualDevice.product_id = 0x0402;
        const auto id = SDL_AttachVirtualJoystick(&virtualDevice);
        QVERIFY(id && SDL_IsGamepad(id));
        struct Detach {
            SDL_JoystickID id;
            ~Detach() { SDL_DetachVirtualJoystick(id); }
        } detach{id};
        QTRY_COMPARE(input.flightDevices().size(), 1);
        QCOMPARE(input.controllerCount(), 0);
        QCOMPARE(input.flightDevices().first().toMap().value(QStringLiteral("status")).toString(), QStringLiteral("ready"));
        input.setForwardFlightControls(false);
        QTRY_COMPARE(input.controllerCount(), 1);
        QCOMPARE(input.flightDevices().first().toMap().value(QStringLiteral("status")).toString(), QStringLiteral("controller"));
        QCOMPARE(input.flightDevices().first().toMap().value(QStringLiteral("player")).toInt(), 1);
        QCOMPARE(input.deviceClaims().first().product, quint16(0x0402));
        QVERIFY(input.genericHidClaims().isEmpty());
        input.setShellCaptureEnabled(false);
        QSignalSpy snapshots(&input, &ControllerInput::gamepadSnapshot);
        auto *joystick = SDL_GetJoystickFromID(id);
        QVERIFY(joystick);
        QVERIFY(SDL_SetJoystickVirtualButton(joystick, SDL_GAMEPAD_BUTTON_SOUTH, true));
        QTRY_VERIFY(!snapshots.isEmpty() && snapshots.last()[2].toUInt() == 0x1000);
        input.setForwardFlightControls(true);
        QCOMPARE(input.controllerCount(), 0);
        QCOMPARE(input.flightDevices().first().toMap().value(QStringLiteral("status")).toString(), QStringLiteral("ready"));
        QCOMPARE(input.flightDevices().first().toMap().value(QStringLiteral("player")).toInt(), 0);
        input.setForwardFlightControls(false);
        QCOMPARE(input.controllerCount(), 1);
    }

    void allowlistAndPhysicalIdentity()
    {
        const QList<QPair<quint16, quint16>> pairs{{0x046d, 0xc262}, {0x046d, 0xc24f},
            {0x046d, 0xc26e}, {0x046d, 0xc266}, {0x046d, 0xc272}, {0x046d, 0xc268}, {0x046d, 0xc276},
            {0x044f, 0xb67c}, {0x044f, 0xb68d}, {0x044f, 0xb68f}, {0x044f, 0x0402}, {0x044f, 0x0404},
            {0x044f, 0xb687}, {0x044f, 0xb697}, {0x06a3, 0x075c}, {0x06a3, 0x0762},
            {0x0738, 0x2221}, {0x0738, 0xa221}};
        for (const auto &pair : pairs) QVERIFY(GenericHidInput::allowed(pair.first, pair.second));
        QVERIFY(!GenericHidInput::allowed(0x054c, 0x0ce6));
        QVERIFY(!GenericHidInput::allowed(0x044f, 0x9999));
        auto first = device("a", "parent-a");
        auto second = device("b", "parent-a", 1);
        QCOMPARE(GenericHidInput::groupKey(first), GenericHidInput::groupKey(second));
        second.product = 0x0404;
        QVERIFY(GenericHidInput::groupKey(first) != GenericHidInput::groupKey(second));
        second.product = first.product;
        second.physicalKey = "parent-b";
        QVERIFY(GenericHidInput::groupKey(first) != GenericHidInput::groupKey(second));
        second.physicalKey.clear();
        QVERIFY(GenericHidInput::groupKey(second).isEmpty());
    }

    void keyboardPointerAndDigitizerCollectionsRemainLocal()
    {
        auto backend = std::make_shared<FakeBackend>();
        auto stick = device("stick", "parent");
        stick.usagePage = 1;
        stick.usage = 4;
        backend->devices.append(stick);
        for (const auto page : {quint16(1), quint16(0x0d)}) {
            const auto usages = page == 1 ? QList<quint16>{1, 2, 6, 7} : QList<quint16>{1, 2, 4, 5};
            for (const auto usage : usages) {
                auto reserved = device("reserved", "parent");
                reserved.path += QByteArray::number(page) + ':' + QByteArray::number(usage);
                reserved.usagePage = page;
                reserved.usage = usage;
                backend->devices.append(reserved);
            }
        }
        GenericHidInput input(backend);
        QTRY_COMPARE(input.claims().size(), 1);
        QMutexLocker lock(&backend->mutex);
        QCOMPARE(backend->opens, 1);
    }

    void compositeGroupingAndUnknownInterface()
    {
        auto backend = std::make_shared<FakeBackend>();
        backend->devices = {device("a", "parent-a"), device("b", "parent-a", 1),
            device("c", "parent-a", 0, 0x0404), device("d", "parent-b"),
            device("unknown", "parent-c", -1), device("unproven", ""),
            device("unsupported", "parent-d", 0, 0x9999)};
        GenericHidInput input(backend);
        QTRY_COMPARE(input.claims().size(), 3);
        const auto claims = input.claims();
        int interfaces = 0;
        for (const auto &claim : claims) interfaces += claim.interfaces.size();
        QCOMPARE(interfaces, 4);
        for (const auto &claim : claims) for (const auto &hidInterface : claim.interfaces) QVERIFY(hidInterface.number != 0xff);
        input.stop();
        QCOMPARE(backend->opens, backend->closes);
        QVERIFY(backend->stopped);
        QVERIFY(!backend->onGuiThread);
    }

    void idleDiscoveryStaysPacedAndHotplugResumesPolling()
    {
        auto backend = std::make_shared<FakeBackend>();
        GenericHidInput input(backend);
        QTRY_COMPARE(backend->scanCount(), 1);
        QTest::qWait(100);
        QCOMPARE(backend->scanCount(), 1);
        {
            QMutexLocker lock(&backend->mutex);
            backend->devices = {device("a", "parent")};
        }
        QTRY_COMPARE_WITH_TIMEOUT(input.claims().size(), 1, 350);
        backend->push("a", 1);
        const auto reports = receive(input, 1);
        QCOMPARE(reports.size(), 1);
        QVERIFY(backend->scanCount() <= 2);
    }

    void missingArrivalIdentityFailsClosed()
    {
        auto backend = std::make_shared<FakeBackend>();
        auto unproven = device("a", "parent");
        unproven.instanceKey.clear();
        backend->devices = {unproven};
        GenericHidInput input(backend);
        QTRY_COMPARE(backend->scanCount(), 1);
        QVERIFY(input.claims().isEmpty());
        input.stop();
        QCOMPARE(backend->opens, 0);
    }

    void sameInterfaceCollectionsRouteByReportId()
    {
        auto backend = std::make_shared<FakeBackend>();
        backend->devices = {device("a", "parent"), device("b", "parent")};
        backend->descriptors["a"] = descriptor(1);
        backend->descriptors["b"] = descriptor(2);
        GenericHidInput input(backend);
        QTRY_COMPARE(input.claims().size(), 1);
        const auto claim = input.claims().first();
        QCOMPARE(claim.interfaces.size(), 1);
        QCOMPARE(claim.interfaces.first().descriptor, QByteArray(1, char(0xa4)) + descriptor(1)
            + QByteArray::fromHex("b4a4") + descriptor(2) + QByteArray(1, char(0xb4)));
        input.submitOutput(output(claim, 0, QByteArray::fromHex("0212")));
        QTRY_COMPARE(backend->outputCount(), 1);
        {
            QMutexLocker lock(&backend->mutex);
            QCOMPARE(backend->outputs.first().path, QByteArray("b"));
            QCOMPARE(backend->outputs.first().bytes, QByteArray::fromHex("0212"));
        }
        input.submitOutput(output(claim, 4, QByteArray::fromHex("0312")));
        const auto replies = receive(input, 1);
        QCOMPARE(replies.size(), 1);
        QVERIFY(replies.first().error);
        QCOMPARE(replies.first().operation, quint8(4));
        QCOMPARE(backend->outputCount(), 1);
    }

    void ambiguousCollectionsFailClosed()
    {
        auto backend = std::make_shared<FakeBackend>();
        backend->devices = {device("a", "parent"), device("b", "parent")};
        GenericHidInput input(backend);
        QTRY_COMPARE(backend->closeCount(), 2);
        QVERIFY(input.claims().isEmpty());
    }

    void overlappingCollectionReportIdsFailClosed()
    {
        auto backend = std::make_shared<FakeBackend>();
        backend->devices = {device("a", "parent"), device("b", "parent")};
        backend->descriptors["a"] = descriptor(1);
        backend->descriptors["b"] = descriptor(1);
        GenericHidInput input(backend);
        QTRY_COMPARE(backend->closeCount(), 2);
        QVERIFY(input.claims().isEmpty());
    }

    void duplicateEnumerationOfSameProvenHandleIsDeduplicated()
    {
        auto backend = std::make_shared<FakeBackend>();
        backend->devices = {device("a", "parent"), device("a", "parent")};
        GenericHidInput input(backend);
        QTRY_COMPARE(input.claims().size(), 1);
        QCOMPARE(input.claims().first().interfaces.size(), 1);
        input.stop();
        QCOMPARE(backend->opens, 1);
        QCOMPARE(backend->closes, 1);
    }

    void inventoryAndInterfaceBounds()
    {
        auto backend = std::make_shared<FakeBackend>();
        for (int i = 0; i < 9; ++i) {
            auto entry = device("a", "parent");
            entry.path = QByteArray::number(i);
            entry.physicalKey = entry.path;
            backend->devices.append(entry);
        }
        GenericHidInput input(backend);
        QTRY_COMPARE(input.claims().size(), 8);
        for (const auto &claim : input.claims()) QVERIFY(claim.slot < 8);
        input.stop();
        QCOMPARE(backend->opens, 8);
        QCOMPARE(backend->closes, 8);
        auto oversized = std::make_shared<FakeBackend>();
        for (int i = 0; i < 17; ++i) {
            auto entry = device("a", "parent", i);
            entry.path = QByteArray::number(i);
            oversized->devices.append(entry);
        }
        GenericHidInput rejected(oversized);
        QTest::qWait(20);
        QVERIFY(rejected.claims().isEmpty());
        rejected.stop();
        QCOMPARE(oversized->opens, 0);
    }

    void numberedGetKeepsReportId()
    {
        auto backend = std::make_shared<FakeBackend>();
        backend->devices = {device("a", "parent", 2)};
        backend->descriptors["a"] = descriptor(7);
        GenericHidInput input(backend);
        QTRY_COMPARE(input.claims().size(), 1);
        auto request = output(input.claims().first(), 3, QByteArray::fromHex("070300"));
        request.interfaceNumber = 2;
        input.submitOutput(request);
        const auto replies = receive(input, 1);
        QCOMPARE(replies.size(), 1);
        QCOMPARE(replies.first().bytes, QByteArray::fromHex("070102"));
        QVERIFY(!replies.first().error);
        QCOMPARE(replies.first().interfaceNumber, quint8(2));
        QMutexLocker lock(&backend->mutex);
        QCOMPARE(backend->outputs.first().bytes.size(), 3);
        QCOMPARE(backend->outputs.first().bytes[0], char(7));
    }

    void unnumberedOutputAndGetFraming()
    {
        auto backend = std::make_shared<FakeBackend>();
        backend->devices = {device("a", "parent")};
        GenericHidInput input(backend);
        QTRY_COMPARE(input.claims().size(), 1);
        const auto claim = input.claims().first();
        input.submitOutput(output(claim, 0, QByteArray::fromHex("1234")));
        input.submitOutput(output(claim, 1, QByteArray::fromHex("5678")));
        input.submitOutput(output(claim, 4, QByteArray::fromHex("9abc")));
        input.submitOutput(output(claim, 2, QByteArray::fromHex("000300")));
        input.submitOutput(output(claim, 3, QByteArray::fromHex("000200")));
        QTRY_COMPARE(backend->outputCount(), 5);
        auto replies = receive(input, 2);
        QCOMPARE(replies.size(), 2);
        QCOMPARE(replies[0].bytes, QByteArray::fromHex("010203"));
        QCOMPARE(replies[1].bytes, QByteArray::fromHex("0102"));
        QCOMPARE(replies[0].operation, quint8(2));
        QCOMPARE(replies[1].operation, quint8(3));
        QVERIFY(!replies[0].error && !replies[1].error);
        {
            QMutexLocker lock(&backend->mutex);
            QCOMPARE(backend->outputs[0].bytes, QByteArray::fromHex("001234"));
            QCOMPARE(backend->outputs[1].bytes, QByteArray::fromHex("005678"));
            QCOMPARE(backend->outputs[1].operation, quint8(1));
            QCOMPARE(backend->outputs[2].bytes, QByteArray::fromHex("009abc"));
            QCOMPARE(backend->outputs[3].bytes.size(), 4);
        }
        input.submitOutput(output(claim, 2, QByteArray::fromHex("0003")));
        replies = receive(input, 1);
        QCOMPARE(replies.size(), 1);
        QVERIFY(replies.first().error && replies.first().bytes.isEmpty());
        QCOMPARE(backend->outputCount(), 5);
    }

    void emptyDescriptorStillPollsButCannotRouteOutput()
    {
        auto backend = std::make_shared<FakeBackend>();
        backend->devices = {device("a", "parent")};
        backend->descriptors["a"] = {};
        GenericHidInput input(backend);
        QTRY_COMPARE(input.claims().size(), 1);
        QVERIFY(input.claims().first().interfaces.first().descriptor.isEmpty());
        backend->push("a", 1);
        auto reports = receive(input, 1);
        QCOMPARE(reports.size(), 1);
        QCOMPARE(reports.first().bytes, QByteArray(1, char(1)));
        input.submitOutput(output(input.claims().first(), 0, QByteArray(1, char(2))));
        reports = receive(input, 1);
        QCOMPARE(reports.size(), 1);
        QVERIFY(reports.first().error);
        QCOMPARE(backend->outputCount(), 0);
    }

    void malformedSingleCollectionDescriptorUsesFallback()
    {
        auto backend = std::make_shared<FakeBackend>();
        backend->devices = {device("a", "parent", 2)};
        backend->descriptors["a"] = QByteArray::fromHex("85");
        GenericHidInput input(backend);
        QTRY_COMPARE(input.claims().size(), 1);
        const auto claim = input.claims().first();
        QVERIFY(claim.interfaces.first().descriptor.isEmpty());
        backend->push("a", 1, QByteArray::fromHex("010203"));
        auto reports = receive(input, 1);
        QCOMPARE(reports.size(), 1);
        QCOMPARE(reports.first().bytes, QByteArray::fromHex("010203"));
        QCOMPARE(reports.first().interfaceNumber, quint8(2));
        auto request = output(claim, 0, QByteArray::fromHex("0123"));
        request.interfaceNumber = 2;
        input.submitOutput(request);
        reports = receive(input, 1);
        QCOMPARE(reports.size(), 1);
        QVERIFY(reports.first().error && reports.first().bytes.isEmpty());
        QCOMPARE(reports.first().interfaceNumber, quint8(2));
        QCOMPARE(backend->outputCount(), 0);
    }

    void mailboxBoundAndOverflowRemoval()
    {
        auto backend = std::make_shared<FakeBackend>();
        backend->devices = {device("a", "parent")};
        GenericHidInput input(backend);
        QTRY_COMPARE(input.claims().size(), 1);
        backend->push("a", 40);
        QTest::qWait(40);
        auto first = input.drain(1000);
        QCOMPARE(first.reports.size(), 32);
        QCOMPARE(input.drain().reports.size(), 8);
        backend->push("a", 65, QByteArray(256, char(1)));
        QTRY_VERIFY(input.claims().isEmpty());
        QTRY_COMPARE(backend->closeCount(), 1);
        auto removed = input.drain();
        QVERIFY(removed.inventoryChanged && removed.claims.isEmpty() && removed.reports.isEmpty());
        QTest::qWait(350);
        QVERIFY(input.claims().isEmpty());
    }

    void outputOverflowAndStaleIncarnation()
    {
        auto backend = std::make_shared<FakeBackend>();
        backend->devices = {device("a", "parent")};
        backend->blockOutput = true;
        GenericHidInput input(backend);
        QTRY_COMPARE(input.claims().size(), 1);
        const auto claim = input.claims().first();
        auto stale = output(claim, 0, QByteArray(1, char(1)));
        ++stale.incarnation;
        input.submitOutput(stale);
        QTest::qWait(20);
        QCOMPARE(backend->outputCount(), 0);
        input.submitOutput(output(claim, 0, QByteArray(1, char(1))));
        QVERIFY(backend->outputEntered.tryAcquire(1, 1000));
        for (int i = 0; i < 33; ++i) input.submitOutput(output(claim, 0, QByteArray(256, char(1))));
        QVERIFY(input.claims().isEmpty());
        backend->outputRelease.release();
        QTRY_COMPARE(backend->closeCount(), 1);
        QCOMPARE(backend->outputCount(), 1);
        QVERIFY(input.drain().reports.isEmpty());
    }

    void oversizedPhysicalReportRemovesInventory()
    {
        auto backend = std::make_shared<FakeBackend>();
        backend->devices = {device("a", "parent")};
        GenericHidInput input(backend);
        QTRY_COMPARE(input.claims().size(), 1);
        backend->push("a", 1, QByteArray(257, char(1)));
        QTRY_VERIFY(input.claims().isEmpty());
        QTRY_COMPARE(backend->closeCount(), 1);
        QVERIFY(input.drain().reports.isEmpty());
    }

    void oversizedDescriptorIsNotAcceptedAsTruncatedPrefix()
    {
        auto backend = std::make_shared<FakeBackend>();
        backend->devices = {device("a", "parent")};
        backend->descriptors["a"] = QByteArray(4097, char(0));
        GenericHidInput input(backend);
        QTRY_COMPARE(backend->closeCount(), 1);
        QVERIFY(input.claims().isEmpty());
        QVERIFY(input.drain().reports.isEmpty());
    }

    void physicalRemovalAndReconnectRejectOldOutput()
    {
        auto backend = std::make_shared<FakeBackend>();
        backend->devices = {device("a", "parent")};
        GenericHidInput input(backend);
        QTRY_COMPARE(input.claims().size(), 1);
        const auto old = input.claims().first();
        {
            QMutexLocker lock(&backend->mutex);
            backend->devices.clear();
            backend->disconnected.insert("a");
        }
        QTRY_VERIFY(input.claims().isEmpty());
        QTest::qWait(350);
        {
            QMutexLocker lock(&backend->mutex);
            backend->disconnected.clear();
            backend->devices = {device("a", "parent")};
        }
        QTRY_COMPARE(input.claims().size(), 1);
        QVERIFY(old.incarnation != input.claims().first().incarnation);
        input.submitOutput(output(old, 0, QByteArray(1, char(1))));
        QTest::qWait(20);
        QCOMPARE(backend->outputCount(), 0);
        input.stop();
        QVERIFY(input.claims().isEmpty());
        QCOMPARE(backend->opens, backend->closes);
        QVERIFY(backend->stopped && !backend->onGuiThread);
    }

    void rapidSamePathReplugClearsOnlyOldInstanceSuppression()
    {
        auto backend = std::make_shared<FakeBackend>();
        backend->devices = {device("a", "parent")};
        GenericHidInput input(backend);
        QTRY_COMPARE(input.claims().size(), 1);
        const auto old = input.claims().first();
        {
            QMutexLocker lock(&backend->mutex);
            backend->disconnected.insert("a");
        }
        QTRY_VERIFY(input.claims().isEmpty());
        QTRY_COMPARE(backend->closeCount(), 1);
        {
            QMutexLocker lock(&backend->mutex);
            backend->devices.first().instanceKey = "arrival-2";
            backend->disconnected.clear();
        }
        backend->push("a", 1, QByteArray(1, char(3)));
        QTRY_COMPARE(input.claims().size(), 1);
        const auto current = input.claims().first();
        QVERIFY(current.incarnation != old.incarnation);
        const auto reports = receive(input, 1);
        QCOMPARE(reports.size(), 1);
        QCOMPARE(reports.first().incarnation, current.incarnation);
        QCOMPARE(reports.first().bytes, QByteArray(1, char(3)));
        input.submitOutput(output(old, 0, QByteArray(1, char(1))));
        input.submitOutput(output(current, 0, QByteArray(1, char(2))));
        QTRY_COMPARE(backend->outputCount(), 1);
        input.stop();
        QCOMPARE(backend->opens, 2);
        QCOMPARE(backend->closes, 2);
        QCOMPARE(backend->outputs.first().bytes, QByteArray::fromHex("0002"));
    }

    void arrivalChangeWithoutAbsentEnumerationGetsFreshIncarnation()
    {
        auto backend = std::make_shared<FakeBackend>();
        backend->devices = {device("a", "parent")};
        GenericHidInput input(backend);
        QTRY_COMPARE(input.claims().size(), 1);
        const auto old = input.claims().first();
        {
            QMutexLocker lock(&backend->mutex);
            backend->devices.first().instanceKey = "arrival-2";
        }
        QTRY_VERIFY(input.claims().size() == 1 && input.claims().first().incarnation != old.incarnation);
        input.stop();
        QCOMPARE(backend->opens, 2);
        QCOMPARE(backend->closes, 2);
    }

    void pauseClearsPendingOutputsButKeepsInputPolling()
    {
        auto backend = std::make_shared<FakeBackend>();
        backend->devices = {device("a", "parent")};
        backend->blockOutput = true;
        GenericHidInput input(backend);
        QTRY_COMPARE(input.claims().size(), 1);
        const auto claim = input.claims().first();
        input.submitOutput(output(claim, 0, QByteArray(1, char(1))));
        QVERIFY(backend->outputEntered.tryAcquire(1, 1000));
        for (int i = 0; i < 10; ++i) input.submitOutput(output(claim, 1, QByteArray(1, char(2))));
        input.clearOutputs();
        {
            QMutexLocker lock(&backend->mutex);
            backend->blockOutput = false;
        }
        backend->outputRelease.release();
        backend->push("a", 1);
        const auto reports = receive(input, 1);
        QCOMPARE(reports.size(), 1);
        QCOMPARE(reports.first().operation, quint8(0));
        QCOMPARE(backend->outputCount(), 1);
        QCOMPARE(input.claims().size(), 1);
        input.submitOutput(output(claim, 4, QByteArray(1, char(3))));
        QTRY_COMPARE(backend->outputCount(), 2);
        QMutexLocker lock(&backend->mutex);
        QCOMPARE(backend->outputs.last().operation, quint8(4));
    }

    void officialDevicesExcludedWithoutSessionOtherGamepadsRetained()
    {
        ControllerInput input;
        SDL_VirtualJoystickDesc virtualDevice{};
        SDL_INIT_INTERFACE(&virtualDevice);
        virtualDevice.type = SDL_JOYSTICK_TYPE_GAMEPAD;
        virtualDevice.naxes = SDL_GAMEPAD_AXIS_COUNT;
        virtualDevice.nbuttons = SDL_GAMEPAD_BUTTON_COUNT;
        virtualDevice.axis_mask = (1u << SDL_GAMEPAD_AXIS_COUNT) - 1;
        virtualDevice.button_mask = (1u << SDL_GAMEPAD_BUTTON_COUNT) - 1;
        virtualDevice.name = "OpenNOW generic-HID ownership test";
        virtualDevice.vendor_id = 0x044f;
        virtualDevice.product_id = 0x0402;
        const auto excluded = SDL_AttachVirtualJoystick(&virtualDevice);
        QVERIFY(excluded && SDL_IsGamepad(excluded));
        virtualDevice.product_id = 0x9999;
        const auto retained = SDL_AttachVirtualJoystick(&virtualDevice);
        QVERIFY(retained && SDL_IsGamepad(retained));
        QTRY_COMPARE(input.controllerCount(), 1);
        QCOMPARE(input.deviceClaims().first().product, quint16(0x9999));
        input.setShellCaptureEnabled(false);
        input.setInputSuspended(true);
        QTest::qWait(20);
        QCOMPARE(input.controllerCount(), 1);
        QVERIFY(SDL_DetachVirtualJoystick(excluded));
        QVERIFY(SDL_DetachVirtualJoystick(retained));
        QTRY_COMPARE(input.controllerCount(), 0);
    }
};

QTEST_MAIN(GenericHidInputTest)
#include "tst_generichidinput.moc"
