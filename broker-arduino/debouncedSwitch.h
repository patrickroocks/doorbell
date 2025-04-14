#pragma once

#include <Arduino.h>

#include "circularArray.h"

constexpr int MaxRawDataLength = 100; // 10sec
constexpr int MaxRawDataStrings = 20;

class App;
class NetworkHandler;

class DebouncedSwitch final
{
public:
    DebouncedSwitch(int pin, int debounceCycles, App* app, bool rawDataLoggingEnabled = false);
    void setup();

    bool checkRaise();

    const CircularArray<String, MaxRawDataStrings>& getArchivedRawDataStrings() const;
    String getCurrentRawDataStr() const;

private:
    void collectRawData(bool isPressed);
    String getRawDataStr() const;
    void readState();

private:
    App* const app;
    NetworkHandler* networkHandler = nullptr;

    const int pin;
    const int debounceCycles;
    const bool rawDataLoggingEnabled;
    bool lastState = false;
    bool lastDebounceState = false;
    int debounceCounter = 0;

    bool currentlyCollectingRawData = false;

    CircularArray<int, MaxRawDataLength> rawData;
    CircularArray<String, MaxRawDataStrings> rawDataStrings;
};
