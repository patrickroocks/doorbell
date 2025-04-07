#include "timer.h"

enum class EventState
{
    Idle,
    Scheduled,
    Running,
};

bool checkRaise(bool newState, bool& stateVar)
{
    bool rv = !stateVar && newState;
    stateVar = newState;
    return rv;
}
