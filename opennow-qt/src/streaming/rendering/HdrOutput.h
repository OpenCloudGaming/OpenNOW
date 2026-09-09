#pragma once

#include <QObject>
#include <QPointer>
#include <QTimer>
#include <atomic>
#include <memory>

class QQuickWindow;
class HdrOutputPass;
class WaylandHdrOutput;

class HdrOutput final : public QObject
{
    Q_OBJECT
    Q_PROPERTY(bool supported READ supported NOTIFY changed)
    Q_PROPERTY(int outputMode READ outputMode NOTIFY changed)
    Q_PROPERTY(QString status READ status NOTIFY changed)
    Q_PROPERTY(bool chromeRequired READ chromeRequired NOTIFY changed)

public:
    enum OutputMode {
        Sdr = 0,
        LinearScRgb = 1,
        Hdr10 = 2,
        LinearDisplayReferred = 3
    };

    struct State {
        int mode = 0;
        float whiteNits = 203.0f;
        bool supported = false;
        int outputMode = 0;
    };

    explicit HdrOutput(QObject *parent = nullptr);
    ~HdrOutput() override;
    void attach(QQuickWindow *window);
    bool supported() const { return m_supported; }
    int outputMode() const { return m_mode; }
    bool chromeRequired() const { return m_chromeRequired; }
    QString status() const;
    static State renderState();

signals:
    void changed();

private:
    void updateOutput();
    void publish(State state);
    void requestChrome(bool required);
    QPointer<QQuickWindow> m_window;
    QTimer m_probeTimer;
    std::atomic<bool> m_probeRequested{true};
    std::atomic<bool> m_chromeGuiReady{false};
    bool m_chromeSynchronized = false;
    bool m_chromeRequired = false;
    std::unique_ptr<HdrOutputPass> m_outputPass;
    std::unique_ptr<WaylandHdrOutput> m_waylandOutput;
    bool m_supported = false;
    int m_mode = 0;
    static std::atomic<int> s_mode;
    static std::atomic<float> s_whiteNits;
    static std::atomic<bool> s_supported;
};
