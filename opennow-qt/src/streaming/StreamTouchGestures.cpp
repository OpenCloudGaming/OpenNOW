#include "streaming/StreamTouchGestures.h"

#include <QLineF>

#include <cmath>

void StreamTouchGestures::pressed(int id, QPointF position, qint64 timeMs, Mode mode)
{
    if (m_points.isEmpty()) {
        reset();
        m_mode = mode;
        m_startMs = timeMs;
    }
    m_points.insert(id, Point{position, position, position});
    m_maxPoints = std::max(m_maxPoints, int(m_points.size()));
    if (m_points.size() == 1) {
        if (m_mode == Mode::Direct) m_sink.moveTo(position);
        m_sink.holdTimer(true);
        return;
    }
    // A second finger turns the gesture into scroll / two-finger tap.
    m_sink.holdTimer(false);
    setLeftHeld(false);
    m_lastCentroid = centroid();
}

void StreamTouchGestures::moved(int id, QPointF position, qint64 timeMs)
{
    Q_UNUSED(timeMs);
    auto it = m_points.find(id);
    if (it == m_points.end()) return;
    it->last = position;
    if (QLineF(it->start, position).length() > kSlop && !m_moved) {
        m_moved = true;
        m_sink.holdTimer(false);
    }

    if (m_points.size() >= 2) {
        const QPointF now = centroid();
        const QPointF delta = now - m_lastCentroid;
        m_lastCentroid = now;
        if (!m_moved) return;
        m_wheelRemainder += delta * kWheelPerPixel;
        const int x = int(std::trunc(m_wheelRemainder.x()));
        const int y = int(std::trunc(m_wheelRemainder.y()));
        if (x != 0 || y != 0) {
            m_wheelRemainder -= QPointF(x, y);
            m_scrolled = true;
            m_sink.wheel(x, y);
        }
        return;
    }
    // A finger left over after a two-finger gesture does nothing.
    if (!m_moved || m_maxPoints >= 2) {
        if (m_maxPoints >= 2) it->reported = position;
        return;
    }

    if (m_mode == Mode::Direct) {
        // Dragging from the touch point: the button goes down once the finger
        // leaves the slop, unless a hold already produced a right click.
        if (!m_holdFired && !m_leftHeld) setLeftHeld(true);
        m_sink.moveTo(position);
        return;
    }
    // Trackpad: `reported` starts at the touch-down point, so the first move
    // past the slop carries everything since then and short moves are not lost.
    m_sink.moveBy(position - it->reported);
    it->reported = position;
}

void StreamTouchGestures::released(int id, QPointF position, qint64 timeMs)
{
    // A release can carry movement of its own: apply it first, so a fast swipe
    // does not end as a tap and a drag does not lose its last segment.
    auto it = m_points.find(id);
    if (it == m_points.end()) return;
    if (it->last != position) moved(id, position, timeMs);
    m_points.erase(it);
    if (!m_points.isEmpty()) {
        m_lastCentroid = centroid();
        return;
    }
    m_sink.holdTimer(false);
    if (m_maxPoints >= 3) {
        if (!m_scrolled && !m_moved && timeMs - m_startMs <= kTwoFingerTapMs && m_sink.switchMode)
            m_sink.switchMode();
    } else if (m_maxPoints >= 2) {
        if (!m_scrolled && !m_moved && timeMs - m_startMs <= kTwoFingerTapMs) {
            m_sink.button(3, true);
            m_sink.button(3, false);
        }
    } else if (m_leftHeld) {
        setLeftHeld(false);
    } else if (!m_moved && !m_holdFired) {
        if (m_mode == Mode::Direct) m_sink.moveTo(position);
        m_sink.button(1, true);
        m_sink.button(1, false);
    }
    reset();
}

void StreamTouchGestures::holdTimeout()
{
    if (m_points.size() != 1 || m_moved || m_holdFired) return;
    m_holdFired = true;
    if (m_mode == Mode::Direct) {
        // Hold in place = right click at the finger.
        m_sink.button(3, true);
        m_sink.button(3, false);
    } else {
        // Hold, then move = drag with the left button.
        setLeftHeld(true);
    }
}

void StreamTouchGestures::cancel()
{
    m_sink.holdTimer(false);
    setLeftHeld(false);
    m_points.clear();
    reset();
}

void StreamTouchGestures::setLeftHeld(bool held)
{
    if (m_leftHeld == held) return;
    m_leftHeld = held;
    m_sink.button(1, held);
}

QPointF StreamTouchGestures::centroid() const
{
    QPointF sum;
    for (const auto &point : m_points) sum += point.last;
    return m_points.isEmpty() ? sum : sum / qreal(m_points.size());
}

void StreamTouchGestures::reset()
{
    m_maxPoints = int(m_points.size());
    m_moved = false;
    m_holdFired = false;
    m_scrolled = false;
    m_wheelRemainder = {};
}
