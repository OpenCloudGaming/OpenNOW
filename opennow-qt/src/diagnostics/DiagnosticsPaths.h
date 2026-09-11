#pragma once

#include <QDir>
#include <QStandardPaths>
#include <QString>

// Match the core's persisted data directory without changing account/settings
// ownership. Qt's AppDataLocation adds the organization name, unlike the core.
inline QString coreDiagnosticsDataRoot()
{
    const auto overridePath = qEnvironmentVariable("OPENNOW_DATA_DIR");
    if (!overridePath.isEmpty())
        return overridePath;
#ifdef Q_OS_WIN
    const auto roaming = qEnvironmentVariable("APPDATA");
    if (!roaming.isEmpty())
        return QDir(roaming).filePath(QStringLiteral("OpenNOW"));
#endif
#ifdef Q_OS_MACOS
    const auto base = QStandardPaths::writableLocation(QStandardPaths::GenericDataLocation);
#else
    const auto base = QStandardPaths::writableLocation(QStandardPaths::GenericConfigLocation);
#endif
    return base.isEmpty() ? QString{} : QDir(base).filePath(QStringLiteral("OpenNOW"));
}
