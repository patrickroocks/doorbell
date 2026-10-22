#include "timer.h"

BaseTimer::BaseTimer(uint32_t durationMs)
    : durationMs(durationMs)
{}

void BaseTimer::start()
{
    deadlineMs = millis() + durationMs;
    running = true;
}

void BaseTimer::stop()
{
    running = false;
}

bool BaseTimer::expired() const
{
    return (int32_t) (millis() - deadlineMs) >= 0;
}

// --------------------------------------------------------------

EventTimer::EventTimer(uint32_t durationMs, bool autoRestart)
    : BaseTimer(durationMs)
    , autoRestart(autoRestart)
{}

bool EventTimer::checkEvent()
{
    if (!running) {
        // An auto restarting timer fires on its very first check and runs on schedule from there.
        // A one shot timer only reacts to an explicit start().
        if (!autoRestart) return false;
        start();
        return true;
    }

    if (!expired()) return false;

    if (autoRestart) {
        start();
    } else {
        stop();
    }
    return true;
}

// --------------------------------------------------------------

DurationTimer::DurationTimer(uint32_t durationMs)
    : BaseTimer(durationMs)
{}

bool DurationTimer::check() const
{
    return running && !expired();
}
