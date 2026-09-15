#pragma once

#include <QObject>
#include <array>
#include <memory>

class QWindow;

class WaylandHdrOutput final : public QObject
{
    Q_OBJECT
public:
    struct State {
        bool supported = false;
        float whiteNits = 203.0f;
        float minimumNits = 0.0f;
        float maximumNits = 0.0f;
        float targetMinimumNits = 0.0f;
        float targetMaximumNits = 0.0f;
        std::array<double, 8> targetPrimaries{};
    };

    explicit WaylandHdrOutput(QObject *parent = nullptr);
    ~WaylandHdrOutput() override;
    void attach(QWindow *window);
    [[nodiscard]] State state() const;

signals:
    void changed();

protected:
    bool eventFilter(QObject *watched, QEvent *event) override;

private:
    friend class WaylandHdrOutputTest;
    struct Description {
        bool ready = false;
        bool complete = false;
        bool primaries = false;
        bool pq = false;
        bool power = false;
        bool icc = false;
        bool luminances = false;
        bool targetLuminance = false;
        bool targetPrimaries = false;
        double minimum = 0;
        double maximum = 0;
        double white = 0;
        double targetMinimum = 0;
        double targetMaximum = 0;
        std::array<double, 8> primariesValue{};
        std::array<double, 8> targetPrimariesValue{};
    };
    [[nodiscard]] static State stateForDescription(const Description &description);
    struct Private;
    std::unique_ptr<Private> d;
};
