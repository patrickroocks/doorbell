

bool checkRaise(bool newState, bool& stateVar)
{
    bool rv = !stateVar && newState;
    stateVar = newState;
    return rv;
}