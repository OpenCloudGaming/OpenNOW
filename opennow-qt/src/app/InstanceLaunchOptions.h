#pragma once

#include <QRegularExpression>
#include <QStringList>

struct InstanceLaunchOptions
{
    QString profile;
    quint32 controllerId = 0;
    QString error;

    static InstanceLaunchOptions parse(const QStringList &arguments, const QString &inheritedProfile = {})
    {
        InstanceLaunchOptions options;
        options.profile = inheritedProfile;
        bool profileSeen = false;
        bool controllerSeen = false;
        for (qsizetype index = 1; index < arguments.size(); ++index) {
            const auto argument = arguments.at(index);
            const bool isProfile = argument == QStringLiteral("--profile")
                || argument.startsWith(QStringLiteral("--profile="));
            const bool isController = argument == QStringLiteral("--controller")
                || argument.startsWith(QStringLiteral("--controller="));
            if (!isProfile && !isController) continue;
            auto &seen = isProfile ? profileSeen : controllerSeen;
            if (seen) {
                options.error = QStringLiteral("Profile and controller options may only be specified once");
                return options;
            }
            seen = true;
            QString value;
            if (argument.contains(QLatin1Char('=')))
                value = argument.mid(argument.indexOf(QLatin1Char('=')) + 1);
            else if (index + 1 < arguments.size() && !arguments.at(index + 1).startsWith(QLatin1Char('-')))
                value = arguments.at(++index);
            if (value.isEmpty()) {
                options.error = QStringLiteral("%1 requires a value").arg(argument);
                return options;
            }
            if (isProfile) {
                options.profile = value;
            } else {
                bool valid = false;
                options.controllerId = value.toUInt(&valid);
                if (!valid || options.controllerId == 0
                        || !QRegularExpression(QStringLiteral("\\A[0-9]+\\z")).match(value).hasMatch()) {
                    options.error = QStringLiteral("--controller requires a nonzero SDL controller instance ID");
                    return options;
                }
            }
        }
        if (!options.profile.isEmpty()
                && !QRegularExpression(QStringLiteral("\\A[A-Za-z0-9][A-Za-z0-9_-]{0,63}\\z"))
                        .match(options.profile).hasMatch())
            options.error = QStringLiteral("--profile requires 1 to 64 ASCII letters, digits, underscores or hyphens, starting with a letter or digit");
        if (options.controllerId != 0 && options.profile.isEmpty())
            options.error = QStringLiteral("--controller requires an isolated --profile");
        return options;
    }

    QStringList restartArguments() const
    {
        if (profile.isEmpty()) return {};
        return {QStringLiteral("--profile"), profile};
    }
};
