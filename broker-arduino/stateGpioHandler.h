#pragma once

#include "debouncedSwitch.h"
#include "timer.h"

class App;
class MqttHandler;
class NetworkHandler;

class StateGpioHandler final
{
public:
    StateGpioHandler(App* app);

    void setup();
    void loop();

    // Waiting / system state from external
    void loopWaitCycle();
    void initLedsAfterStartUp();
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
    void setRuleAutoBuzzState(bool newRuleAutoBuzzState);
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
    // A rule on the server is armed to buzz on a ring. Shown on LedRuleAutobuzz only - the server
    // does the buzzing itself. Not persisted in the EEPROM, the server re-sends it after a connect.
    bool ruleAutoBuzz = false;
    bool wantToReboot = false;

    // LED state
    uint8_t blinkState = 0;
    uint8_t ledSeq = 0;

    // States for Buzzer and Ext Bell relays
    EventState stateDoorBuzzer = EventState::Idle;
    EventState stateExtBell = EventState::Idle;
};