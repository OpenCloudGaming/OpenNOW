#pragma once

#include "input/GenericHidInput.h"

namespace LinuxHidIdentity {
struct Result {
    QByteArray physicalKey;
    QByteArray instanceKey;
};

Result resolve(const GenericHidBackend::Device &device,
               const QString &sysfsClassRoot = QStringLiteral("/sys/class"));
}
