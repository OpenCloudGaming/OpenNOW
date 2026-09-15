#pragma once

#include <QDir>
#include <QFileInfo>
#include <QStandardPaths>
#include <QString>

inline QString mediaPicturesRoot()
{
    if (!qEnvironmentVariableIsSet("OPENNOW_PICTURES_DIR"))
        return QStandardPaths::writableLocation(QStandardPaths::PicturesLocation);
    const auto overridePath = qEnvironmentVariable("OPENNOW_PICTURES_DIR");
    if (overridePath.isEmpty()) return {};
    return QDir::cleanPath(QFileInfo(overridePath).absoluteFilePath());
}

inline QString mediaScreenshotsDirectory()
{
    const auto root = mediaPicturesRoot();
    return root.isEmpty() ? QString{} : QDir(root).filePath(QStringLiteral("OpenNOW/Screenshots"));
}

inline QString mediaRecordingsDirectory()
{
    const auto root = mediaPicturesRoot();
    return root.isEmpty() ? QString{} : QDir(root).filePath(QStringLiteral("OpenNOW/Recordings"));
}
