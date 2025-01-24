

class BaseTimer
{
public:
    BaseTimer(int cycles)
        : cycles(cycles)
    {}

    void start()
    {
        count = cycles;
    }

    void stop()
    {
        count = -1;
    }

    void decrement()
    {
        if (count >= -1) --count;
    }

protected:
    const int cycles = 0;
    int count = 0;
};

class EventTimer : public BaseTimer
{
public:
    EventTimer(int cycles, bool autoRestart = false)
        : BaseTimer(cycles)
        , autoRestart(autoRestart)
    {}

    bool checkAndDecrement()
    {
        bool eventRaise = count == 0;
        decrement();
        if (eventRaise && autoRestart) start();
        return eventRaise;
    }

private:
    bool autoRestart = false;
};

class DurationTimer : public BaseTimer
{
public:
    DurationTimer(int cycles)
        : BaseTimer(cycles)
    {}

    bool check()
    {
        return count > 0;
    }
};