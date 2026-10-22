#include "debouncedSwitch.h"

#include "app.h"
#include "networkHandler.h"

DebouncedSwitch::DebouncedSwitch(int pin, int debounceCycles, App* app, bool rawDataLoggingEnabled)
    : app(app)
    , pin(pin)
    , debounceCycles(debounceCycles)
    , rawDataLoggingEnabled(rawDataLoggingEnabled)
{}

void DebouncedSwitch::setup()
{
    Serial.println("Setup DebouncedSwitch");
    networkHandler = app->getNetworkHandler();
}

bool DebouncedSwitch::checkRaise()
{
    readState();
    const bool raise = lastState && !lastDebounceState;
    lastDebounceState = lastState;
    return raise;
}

bool DebouncedSwitch::isCurrentlyPressed() const
{
    return isPressed;
}

void DebouncedSwitch::collectRawData()
{
    if (!rawDataLoggingEnabled) return;

    // If pressed, start new raw data monitoring.
    if (isPressed) {
        currentlyCollectingRawData = true;
    }

    // Collect raw data if started, until max length is reached.
    if (currentlyCollectingRawData) {
        rawData.push(isPressed);
        if (rawData.full()) {
            rawDataStrings.push(getRawDataStr());
            rawData.clear();
            currentlyCollectingRawData = false;
        }
    }
}

String DebouncedSwitch::getRawDataStr() const
{
    if (!currentlyCollectingRawData) return "";

    String rawDataStr;
    if (networkHandler) rawDataStr = networkHandler->getDateTime() + " ";
    for (auto& data : rawData) {
        rawDataStr += data ? "1" : "0";
    }
    return rawDataStr;
}

void DebouncedSwitch::readState()
{
    // All switches/inputs are grounded, so HIGH means not pressed.
    isPressed = digitalRead(pin) == LOW;

    collectRawData();

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

const CircularArray<String, MaxRawDataStrings>& DebouncedSwitch::getArchivedRawDataStrings() const
{
    return rawDataStrings;
}

String DebouncedSwitch::getCurrentRawDataStr() const
{
    if (rawDataLoggingEnabled) {
        return getRawDataStr();
    } else {
        return "";
    }
}