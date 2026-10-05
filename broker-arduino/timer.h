#pragma once

#include <Arduino.h>

// The timers work on absolute deadlines taken from millis(), so a duration does not depend on how
// long a main loop iteration happens to take.
class BaseTimer
{
public:
    BaseTimer(uint32_t durationMs);

    void start();
    void stop();

protected:
    // Whether the deadline has passed. millis() is 32 bit and wraps every ~49 days; the explicit
    // uint32_t/int32_t arithmetic below stays correct across that wrap as long as the duration is
    // well below half of it.
    bool expired() const;

    const uint32_t durationMs;
    uint32_t deadlineMs = 0;
    bool running = false;
};

// --------------------------------------------------------------

// Fires once when its duration has elapsed. An auto restarting timer keeps firing every duration
// and starts itself on the first check, so a periodic timer needs no explicit start().
class EventTimer final : public BaseTimer
{
public:
    EventTimer(uint32_t durationMs, bool autoRestart = false);

    bool checkEvent();

private:
    const bool autoRestart;
};

// --------------------------------------------------------------

// Tells whether its duration is still running.
class DurationTimer final : public BaseTimer
{
public:
    DurationTimer(uint32_t durationMs);

    bool check() const;
};
