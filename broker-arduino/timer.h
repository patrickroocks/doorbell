#pragma once

class BaseTimer
{
public:
    BaseTimer(int cycles);

    void start();
    void stop();
    void decrement();

protected:
    const int cycles;
    int count = 0;
};

// --------------------------------------------------------------

class EventTimer final : public BaseTimer
{
public:
    EventTimer(int cycles, bool autoRestart = false);

    bool checkAndDecrement();

private:
    bool autoRestart = false;
};

// --------------------------------------------------------------

class DurationTimer final : public BaseTimer
{
public:
    DurationTimer(int cycles);

    bool check();
};