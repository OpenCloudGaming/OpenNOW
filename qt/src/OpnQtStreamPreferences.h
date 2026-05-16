#pragma once

#include <QtCore/QString>

namespace OpnQt {

QString defaultStreamingBaseUrl();
QString loadSelectedStreamingBaseUrl();
QString normalizedStreamingBaseUrl(const QString &url);

} // namespace OpnQt
