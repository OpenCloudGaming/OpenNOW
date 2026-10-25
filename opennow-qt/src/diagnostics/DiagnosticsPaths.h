#pragma once

#include <QDir>
#include <QFileInfo>
#include <QStandardPaths>
#include <QString>

// Match the core's persisted data directory without changing account/settings
// ownership. Qt's AppDataLocation adds the organization name, unlike the core.
inline QString coreDataBaseRoot()
{
    const auto overridePath = qEnvironmentVariable("OPENNOW_DATA_DIR");
    if (qEnvironmentVariableIsSet("OPENNOW_DATA_DIR"))
        return overridePath;
#ifdef Q_OS_WIN
    const auto roaming = qEnvironmentVariable("APPDATA");
    if (qEnvironmentVariableIsSet("APPDATA"))
        return QDir(roaming).filePath(QStringLiteral("OpenNOW"));
#endif
#ifdef Q_OS_MACOS
    if (qEnvironmentVariableIsSet("HOME"))
        return QDir(qEnvironmentVariable("HOME")).filePath(QStringLiteral("Library/Application Support/OpenNOW"));
    const auto base = QStandardPaths::writableLocation(QStandardPaths::GenericDataLocation);
#else
    if (qEnvironmentVariableIsSet("XDG_CONFIG_HOME"))
        return QDir(qEnvironmentVariable("XDG_CONFIG_HOME")).filePath(QStringLiteral("OpenNOW"));
    const auto base = QStandardPaths::writableLocation(QStandardPaths::GenericConfigLocation);
#endif
    return base.isEmpty() ? QString{} : QDir(base).filePath(QStringLiteral("OpenNOW"));
}

inline QString coreDiagnosticsDataRoot()
{
    const auto base = coreDataBaseRoot();
    const auto profile = qEnvironmentVariable("OPENNOW_PROFILE");
    if (base.isEmpty()) return {};
    if (!profile.isEmpty())
        return QDir(base).filePath(QStringLiteral("profiles/") + profile);
#ifdef Q_OS_LINUX
    const auto entries = QDir(base).entryInfoList(QDir::AllEntries | QDir::NoDotAndDotDot | QDir::Hidden | QDir::System);
    const auto onlyProfiles = entries.size() == 1 && entries.first().fileName() == QStringLiteral("profiles")
        && entries.first().isDir() && !entries.first().isSymLink();
    if ((!QFileInfo::exists(base) || onlyProfiles) && !QDir(base).isRoot()) {
        const auto legacy = QFileInfo(QDir::cleanPath(base)).dir().filePath(QStringLiteral("opennow"));
        if (QFileInfo::exists(legacy)) return legacy;
    }
#endif
    return base;
}
