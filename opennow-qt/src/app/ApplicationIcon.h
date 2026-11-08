#pragma once

#include <QIcon>
#include <QSize>
#include <QString>

inline QString applicationIconResource(int size)
{
#if defined(Q_OS_MACOS)
    return QStringLiteral(":/icons/opennow-macos-%1.png").arg(size);
#else
    return QStringLiteral(":/icons/opennow-%1.png").arg(size);
#endif
}

inline QIcon applicationWindowIcon()
{
    QIcon icon;
    for (const int size : {16, 24, 32, 48, 64, 128, 256, 512, 1024})
        icon.addFile(applicationIconResource(size), QSize(size, size));
    return icon;
}
