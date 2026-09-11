#pragma once

class QWindow;
struct QRhiSwapChainProxyData;

bool resetMetalSdrOutput(QWindow *window, const QRhiSwapChainProxyData &proxy);
