#pragma once

#include "debouncedSwitch.h"
#include "timer.h"

class App;
class MqttHandler;
class NetworkHandler;

class StateGpioHandler
{
public:
    StateGpioHandler(App* app);

    void setup();
    void loop();

    // Waiting / system state from external
    void loopWaitCycle();
    void waitSeconds(int sec);
    void reboot();

    // State
    const CircularArray<String, MaxRawDataStrings>& getArchivedRawDataStrings() const;
    String getCurrentRawDataStr() const;
    bool getAutoBuzzState() const;

    // Events
    void ring(bool testRing);
    void buzz();
    void setAutoBuzzState(bool newAutoBuzzState);
    void ackRing();

private:
    // Events
    void ackRingButton();
    void ackRingAndBuzzButton();

    // Setup / EEPROM
    void setupPins();
    void readEeprom();
    void writeEeprom();

    // Loop functions
    void updateBlinkState();
    void readSwitches();
    void readInputs();
    void handleRelays();
    void writeLedsInNormalLoop();
    void decrementTimers();
    void checkForReboot();

    enum class EventState
    {
        Idle,
        Scheduled,
        Running,
    };

    void scheduleEvent(EventState& state);

    // Connection to other components
    App* const app;
    MqttHandler* mqttHandler = nullptr;
    NetworkHandler* networkHandler = nullptr;

    // GPIO inputs
    DebouncedSwitch switchBuzzMode;
    DebouncedSwitch switchAckBuzz;
    DebouncedSwitch switchAck;
    DebouncedSwitch inputRing;

    // Timers
    DurationTimer timerDoorBuzzer;
    DurationTimer timerExtBell;
    DurationTimer timerBellBlink;
    DurationTimer timerAckLedOn;
    DurationTimer timerErrorLedOn;
    DurationTimer timerReboot;

    // System states
    bool ringActive = false;
    bool autoBuzz = false;
    bool wantToReboot = false;

    // LED state
    uint8_t blinkState = 0;
    uint8_t ledSeq = 0;

    // States for Buzzer and Ext Bell relays
    EventState stateDoorBuzzer = EventState::Idle;
    EventState stateExtBell = EventState::Idle;
};