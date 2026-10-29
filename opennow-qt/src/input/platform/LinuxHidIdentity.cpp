#include "input/platform/LinuxHidIdentity.h"

#ifdef Q_OS_LINUX
#include <QDir>
#include <QFile>
#include <QFileInfo>

#include <algorithm>
#include <sys/stat.h>

LinuxHidIdentity::Result LinuxHidIdentity::resolve(const GenericHidBackend::Device &device,
                                                 const QString &sysfsClassRoot)
{
    const auto prefix = device.path.startsWith("/dev/hidraw") ? QByteArray("/dev/hidraw")
        : device.path.startsWith("/dev/input/event") ? QByteArray("/dev/input/event") : QByteArray{};
    if (prefix.isEmpty()) return {};
    const auto number = device.path.sliced(prefix.size());
    if (number.isEmpty() || !std::all_of(number.cbegin(), number.cend(),
            [](char digit) { return digit >= '0' && digit <= '9'; })) return {};
    const auto deviceClass = prefix == "/dev/hidraw" ? QStringLiteral("hidraw") : QStringLiteral("input");
    auto current = QFileInfo(QStringLiteral("%1/%2/%3/device").arg(sysfsClassRoot, deviceClass)
                                .arg(QFileInfo(QString::fromLocal8Bit(device.path)).fileName())).canonicalFilePath();
    if (current.isEmpty()) return {};
    const auto instancePath = current.toUtf8();
    struct stat instance{};
    if (::stat(instancePath.constData(), &instance) != 0 || !S_ISDIR(instance.st_mode) || !instance.st_ino) return {};
    const auto instanceKey = instancePath + char(0) + QByteArray::number(quint64(instance.st_dev), 16)
        + ':' + QByteArray::number(quint64(instance.st_ino), 16);
    QString hidSerial;
    while (!current.isEmpty() && current != QStringLiteral("/")) {
        QFile vendor(current + QStringLiteral("/idVendor"));
        QFile product(current + QStringLiteral("/idProduct"));
        if (vendor.open(QIODevice::ReadOnly) && product.open(QIODevice::ReadOnly)) {
            if (vendor.readAll().trimmed().toUShort(nullptr, 16) != device.vendor
                || product.readAll().trimmed().toUShort(nullptr, 16) != device.product) return {};
            if (!device.serial.isEmpty()) {
                QFile serial(current + QStringLiteral("/serial"));
                if (!serial.open(QIODevice::ReadOnly)
                    || (QString::fromUtf8(serial.readAll().trimmed()) != device.serial
                        && hidSerial != device.serial)) return {};
            }
            return {current.toUtf8(), instanceKey};
        }
        if (hidSerial.isNull()) {
            QFile uevent(current + QStringLiteral("/uevent"));
            if (uevent.open(QIODevice::ReadOnly)) {
                for (const auto &line : uevent.readAll().split('\n')) {
                    if (line.startsWith("HID_UNIQ=")) {
                        hidSerial = QString::fromUtf8(line.sliced(9));
                        break;
                    }
                }
            }
        }
        current = QFileInfo(current).dir().absolutePath();
    }
    return {};
}
#endif
