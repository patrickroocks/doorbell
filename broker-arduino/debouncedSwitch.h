#include <deque>
#include <vector>

#define MAX_RAW_DATA_LENGTH  100 // 10sec
#define MAX_RAW_DATA_STRINGS 20

// =============================================
//          Interface for time strings
// =============================================

String (*callbackTimeStr)() = nullptr;

void setTimeStrGetter(String (*callback)())
{
    callbackTimeStr = callback;
}

// =============================================
//         Class for debouncing a switch
// =============================================

class DebouncedSwitch
{
public:
    DebouncedSwitch(int pin, int debounceCycles, bool rawDataLoggingEnabled = false)
        : pin(pin)
        , debounceCycles(debounceCycles)
        , rawDataLoggingEnabled(rawDataLoggingEnabled){};

    bool checkRaise()
    {
        readState();
        const bool raise = lastState && !lastDebounceState;
        lastDebounceState = lastState;
        return raise;
    }

    std::deque<String> getRawDataStrings() const
    {
        std::deque<String> rv = rawDataStrings;
        if (currentlyCollectingRawData) rv.push_back(getRawDataStr());
        return rv;
    }

private:
    void collectRawData(bool isPressed)
    {
        if (!rawDataLoggingEnabled) return;

        // If pressed, start new raw data monitoring.
        if (isPressed) {
            currentlyCollectingRawData = true;
            rawData.reserve(MAX_RAW_DATA_LENGTH);
        }

        // Collect raw data if started, until max length is reached.
        if (currentlyCollectingRawData) {
            rawData.push_back(isPressed);
            if (rawData.size() >= MAX_RAW_DATA_LENGTH) {
                generateRawDataString();
                rawData.clear();
                currentlyCollectingRawData = false;
            }
        }
    }

    String getRawDataStr() const
    {
        String rawDataStr;
        if (callbackTimeStr) {
            rawDataStr = callbackTimeStr() + " ";
        }
        for (auto& data : rawData) {
            rawDataStr += data ? "1" : "0";
        }
        return rawDataStr;
    }

    void generateRawDataString()
    {
        rawDataStrings.push_back(getRawDataStr());
        if (rawDataStrings.size() > MAX_RAW_DATA_STRINGS) {
            rawDataStrings.pop_front();
        }
    }

    void readState()
    {
        // All switches/inputs are grounded, so HIGH means not pressed.
        bool isPressed = digitalRead(pin) == LOW;

        collectRawData(isPressed);

        if (isPressed != lastState) {
            debounceCounter++;
            if (debounceCounter >= debounceCycles) {
                lastState = isPressed;
                debounceCounter = 0;
            }
        } else {
            debounceCounter = 0;
        }
    }

private:
    const int pin;
    const int debounceCycles;
    const bool rawDataLoggingEnabled;
    bool lastState = false;
    bool lastDebounceState = false;
    int debounceCounter = 0;

    bool currentlyCollectingRawData = false;
    std::vector<int> rawData;
    std::deque<String> rawDataStrings;
};
