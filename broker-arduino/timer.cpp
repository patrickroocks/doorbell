#include "timer.h"

BaseTimer::BaseTimer(int cycles)
    : cycles(cycles)
{}

void BaseTimer::start()
{
    count = cycles;
}

void BaseTimer::stop()
{
    count = -1;
}

void BaseTimer::decrement()
{
    if (count >= -1) --count;
}

// --------------------------------------------------------------

EventTimer::EventTimer(int cycles, bool autoRestart)
    : BaseTimer(cycles)
    , autoRestart(autoRestart)
{}

bool EventTimer::checkAndDecrement()
{
    bool eventRaise = count == 0;
    decrement();
    if (eventRaise && autoRestart) start();
    return eventRaise;
}

// --------------------------------------------------------------

DurationTimer::DurationTimer(int cycles)
    : BaseTimer(cycles)
{}

bool DurationTimer::check()
{
    return count > 0;
}