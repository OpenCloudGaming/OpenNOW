#pragma once

#include <QHash>
#include <QPointF>

#include <functional>

// Turns touchscreen contacts into mouse input for the stream.
//
// GeForce NOW has no touch input channel, so touch has to become a mouse, in
// one of two ways (the same split as Moonlight, Steam Link and GFN's own
// mobile "virtual mouse"):
//
// Direct:   the remote cursor goes where the finger is (absolute positions).
//           Tap = left click there, hold = right click, drag = left-button
//           drag. Works where the game uses the OS cursor (Steam, launchers,
//           desktop apps).
// Trackpad: the finger moves the cursor by relative motion, like a laptop
//           touchpad. Tap = left click where the cursor is, two-finger tap =
//           right click, hold then move = left-button drag. Needed for games
//           that hide the OS cursor and move their own from raw mouse input:
//           they ignore absolute positions, and the server does not report
//           where their cursor is.
//
// Both modes: two-finger drag = scroll wheel, three-finger tap = switch between
// the modes (the owner decides what that means). The mode is fixed for a
// gesture (first finger down until the last finger up).
//
// The class holds no timer and no Qt event types so it can be driven and
// tested with plain points and times; the owner calls holdTimeout() after
// holdDelayMs() of a pending hold.
class StreamTouchGestures
{
public:
    enum class Mode { Direct, Trackpad };

    struct Sink {
        std::function<void(QPointF)> moveTo;         // item coordinates
        std::function<void(QPointF)> moveBy;         // item coordinates
        std::function<void(int button, bool pressed)> button;  // 1 left, 3 right
        std::function<void(int x, int y)> wheel;     // 120 = one notch, Qt angle sign
        std::function<void(bool armed)> holdTimer;   // start/stop the hold timer
        std::function<void()> switchMode;            // three-finger tap
    };

    static constexpr qreal kSlop = 14.0;              // px before a touch counts as a move
    static constexpr int kHoldDelayMs = 500;
    static constexpr int kTwoFingerTapMs = 300;
    static constexpr qreal kWheelPerPixel = 3.0;      // 40 px of finger travel = one notch

    explicit StreamTouchGestures(Sink sink) : m_sink(std::move(sink)) {}

    // Contacts are reported by id; the mode is read once per gesture.
    void pressed(int id, QPointF position, qint64 timeMs, Mode mode);
    void moved(int id, QPointF position, qint64 timeMs);
    void released(int id, QPointF position, qint64 timeMs);
    void holdTimeout();
    // Drop the gesture; releases a held button so the rig is not left dragging.
    void cancel();

    [[nodiscard]] bool active() const { return !m_points.isEmpty(); }
    [[nodiscard]] Mode mode() const { return m_mode; }

private:
    struct Point {
        QPointF start;
        QPointF last;
        QPointF reported;  // trackpad: position already sent as relative motion
    };

    void setLeftHeld(bool held);
    QPointF centroid() const;
    void reset();

    Sink m_sink;
    Mode m_mode = Mode::Direct;
    QHash<int, Point> m_points;
    qint64 m_startMs = 0;
    int m_maxPoints = 0;
    bool m_moved = false;
    bool m_holdFired = false;
    bool m_leftHeld = false;
    bool m_scrolled = false;
    QPointF m_lastCentroid;
    QPointF m_wheelRemainder;
};
