
#include "debouncedSwitch.h"
#include "timer.h"
#include "utils.h"

// =============================================
//              main config
// =============================================

#define DOOR_OPEN_TIME  50  // 5 sec
#define EXT_BELL_TIME   30  // 3 sec
#define BELL_BLINK_TIME 600 // 60 sec

#define STARTUP_CYCLE_TIME 500 // 500ms

// =============================================
//              SETUP
// =============================================

#define LED_ON             2
#define LED_AUTOBUZZ       3
#define LED_EXTBELL_BUZZER 4
#define LED_ERROR          5
#define LED_DOORBELL       6

#define FIRST_LED LED_ON
#define LAST_LED  LED_DOORBELL

#define SWITCH_BUZZMODE 7
#define SWITCH_ACK_BUZZ 8
#define SWITCH_ACK      9

#define INP_RING       10
#define RELAY_BUZZER   11
#define RELAY_EXT_BELL 12

#define SWITCH_DEBOUNCE_CYCLES 2
#define INPUT_DEBOUNCE_CYCLES  5

DebouncedSwitch switchBuzzMode(SWITCH_BUZZMODE, SWITCH_DEBOUNCE_CYCLES);
DebouncedSwitch switchAckBuzz(SWITCH_ACK_BUZZ, SWITCH_DEBOUNCE_CYCLES);
DebouncedSwitch switchAck(SWITCH_ACK, SWITCH_DEBOUNCE_CYCLES);
DebouncedSwitch inputRing(INP_RING, INPUT_DEBOUNCE_CYCLES, true);

// =============================================
//              global Vars
// =============================================

// LED state
uint8_t blinkState = 0;
uint8_t ledSeq = FIRST_LED;

// Timers
DurationTimer timerDoorBuzzer(DOOR_OPEN_TIME);
DurationTimer timerExtBell(EXT_BELL_TIME);
DurationTimer timerBellBlink(BELL_BLINK_TIME);

// System states
bool autoBuzz = false;
bool wifiConnected = false;
bool startupCycleCompleted = false;

// =============================================
//              Interface
// =============================================

void (*callbackRing)(bool) = nullptr;
void (*callbackAutoBuzzChange)(bool) = nullptr;
void (*callbackBuzz)(bool) = nullptr;
void (*callbackAckRing)() = nullptr;

void setRingCommand(void (*callback)(bool))
{
    callbackRing = callback;
}

void setAutoBuzzChangeCommand(void (*callback)(bool))
{
    callbackAutoBuzzChange = callback;
}

void setBuzzCommand(void (*callback)(bool))
{
    callbackBuzz = callback;
}

void setAckRingCommand(void (*callback)())
{
    callbackAckRing = callback;
}

// =============================================
//              SETUP
// =============================================

void setupLeds()
{
    // LEDs
    pinMode(LED_ON, OUTPUT);
    pinMode(LED_AUTOBUZZ, OUTPUT);
    pinMode(LED_EXTBELL_BUZZER, OUTPUT);
    pinMode(LED_ERROR, OUTPUT);
    pinMode(LED_DOORBELL, OUTPUT);

    // Switches (input)
    // Use PULLUPS, all switches are grounded
    pinMode(SWITCH_BUZZMODE, INPUT_PULLUP);
    pinMode(SWITCH_ACK_BUZZ, INPUT_PULLUP);
    pinMode(SWITCH_ACK, INPUT_PULLUP);

    // ring input (also grounded, has its own pull up)
    pinMode(INP_RING, INPUT);

    // Relays
    pinMode(RELAY_BUZZER, OUTPUT);
    pinMode(RELAY_EXT_BELL, OUTPUT);

    // build-in LED for output
    pinMode(LED_BUILTIN, OUTPUT);
}

// =============================================
//              EVENTS
// =============================================

void eventBuzz(bool autoBuzz)
{
    if (callbackBuzz) {
        callbackBuzz(autoBuzz);
    }
}

void eventAckRing()
{
    if (callbackAckRing) {
        callbackAckRing();
    }
}

void eventRing(bool testRing)
{
    if (callbackRing) {
        callbackRing(testRing);
    }
}

void setAutoBuzz(bool newState)
{
    autoBuzz = newState;
    if (callbackAutoBuzzChange) {
        callbackAutoBuzzChange(autoBuzz);
    }
}

// =============================================
//              LOOP NORMAL
// =============================================

void updateBlinkState()
{
    blinkState = 1 - blinkState;
}

void writeLedsInNormalLoop()
{
    updateBlinkState();
    digitalWrite(LED_BUILTIN, HIGH);
    digitalWrite(LED_ON, HIGH);
    digitalWrite(LED_AUTOBUZZ, autoBuzz ? blinkState : LOW);
    digitalWrite(LED_EXTBELL_BUZZER, timerDoorBuzzer.check() ? blinkState : (timerExtBell.check() ? HIGH : LOW));
    digitalWrite(LED_ERROR, !wifiConnected);
    digitalWrite(LED_DOORBELL, timerBellBlink.check() ? blinkState : LOW);
}

void readSwitches()
{
    if (switchBuzzMode.checkRaise()) {
        setAutoBuzz(!autoBuzz);
    }

    if (switchAckBuzz.checkRaise()) {
        if (timerBellBlink.check()) {
            eventAckRing();
            if (!autoBuzz) eventBuzz(false);
        }
    }

    if (switchAck.checkRaise()) {
        eventAckRing();
    }
}

void readInputs()
{
    if (inputRing.checkRaise()) {
        eventRing(false);
    }
}


void handleRelays()
{
    digitalWrite(RELAY_BUZZER, timerDoorBuzzer.check() ? HIGH : LOW);
    digitalWrite(RELAY_EXT_BELL, timerExtBell.check() ? HIGH : LOW);
}

void decrementTimers()
{
    timerDoorBuzzer.decrement();
    timerExtBell.decrement();
    timerBellBlink.decrement();
}

void loopState()
{
    readSwitches();
    readInputs();
    handleRelays();
    writeLedsInNormalLoop();
    decrementTimers();
}

// =============================================
//              LOOP STARTUP
// =============================================


void writeLedsStartUpSequence()
{
    updateBlinkState();

    // loop blink (shows that we didn't stuck)
    digitalWrite(LED_BUILTIN, blinkState);

    // write LOW to all LEDS, except the ledSeq LED:
    for (uint8_t i = FIRST_LED; i <= LAST_LED; ++i) {
        digitalWrite(i, i == ledSeq ? HIGH : LOW);
    }

    if (++ledSeq > LAST_LED) {
        ledSeq = FIRST_LED;
        startupCycleCompleted = true;
    }
}

void waitCycle()
{
    delay(STARTUP_CYCLE_TIME);
    writeLedsStartUpSequence();
}

void waitSec(int sec)
{
    int cycles = sec * 1000 / STARTUP_CYCLE_TIME;
    for (int i = 0; i < cycles; ++i) {
        waitCycle();
    }
}