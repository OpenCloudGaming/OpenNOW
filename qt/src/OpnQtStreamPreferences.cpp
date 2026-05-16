#include "OpnQtStreamPreferences.h"

#include <QtCore/QSettings>
#include <QtCore/QVariantList>
#include <QtCore/QVariantMap>

namespace OpnQt {
namespace {

constexpr const char *kSelectedRegionUrlKey = "OpenNOW.Stream.RegionUrl";
constexpr const char *kCachedRegionsKey = "OpenNOW.Stream.CachedRegions";
constexpr const char *kDefaultStreamingBaseUrl = "https://prod.cloudmatchbeta.nvidiagrid.net/";

QString normalizedWithTrailingSlash(QString url) {
    url = url.trimmed();
    if (url.isEmpty()) return QString::fromLatin1(kDefaultStreamingBaseUrl);
    return url.endsWith(QLatin1Char('/')) ? url : url + QLatin1Char('/');
}

} // namespace

QString defaultStreamingBaseUrl() {
    return QString::fromLatin1(kDefaultStreamingBaseUrl);
}

QString normalizedStreamingBaseUrl(const QString &url) {
    return normalizedWithTrailingSlash(url);
}

QString loadSelectedStreamingBaseUrl() {
    QSettings settings;
    const QString selected = settings.value(QString::fromLatin1(kSelectedRegionUrlKey)).toString().trimmed();
    if (!selected.isEmpty()) return normalizedWithTrailingSlash(selected);

    const QVariantList regions = settings.value(QString::fromLatin1(kCachedRegionsKey)).toList();
    for (const QVariant &rawRegion : regions) {
        const QVariantMap region = rawRegion.toMap();
        const QString url = region.value(QStringLiteral("url")).toString().trimmed();
        const int latencyMs = region.value(QStringLiteral("latencyMs"), -1).toInt();
        if (!url.isEmpty() && latencyMs >= 0) return normalizedWithTrailingSlash(url);
    }

    return defaultStreamingBaseUrl();
}

} // namespace OpnQt
