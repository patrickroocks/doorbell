#include "stateGpioHandler.h"

#include "app.h"
#include "debouncedSwitch.h"
#include "gpioConfig.h"
#include "mqttHandler.h"
#include "networkHandler.h"
#include "timing.h"

#include <EEPROM.h>

constexpr int EepromSize = 1;
constexpr int EepromAddressAutoBuzz = 0;

StateGpioHandler::StateGpioHandler(App* app)
    : app(app)
    , switchBuzzMode(SwitchBuzzmode, SwitchDebounceCycles, app)
    , switchAckBuzz(SwitchAckBuzz, SwitchDebounceCycles, app)
    , switchAck(SwitchAck, SwitchDebounceCycles, app)
    , inputRing(InputRing, InputDebounceCycles, app, true)
    , timerDoorBuzzer(DoorOpenCycles)
    , timerExtBell(ExtBellCycles)
    , timerBellBlink(BellBlinkCycles)
    , timerAckLedOn(AckLedCycles)
    , timerErrorLedOn(ErrorLedCycles)
    , timerReboot(RebootWaitCycles)
{
    ledSeq = FirstLed;
}

void StateGpioHandler::updateBlinkState()
{
    blinkState = 1 - blinkState;
}

void StateGpioHandler::loop()
{
    updateBlinkState();
    readSwitches();
    readInputs();
    handleRelays();
    writeLedsInNormalLoop();
    decrementTimers();
    checkForReboot();
}

void StateGpioHandler::checkForReboot()
{
    if (wantToReboot && !timerReboot.check()) {
        Serial.println("Rebooting...");
        ESP.restart();
    }
}

void StateGpioHandler::waitSeconds(int sec)
{
    int cycles = sec * 1000 / StartupCycleTimeMs;
    for (int i = 0; i < cycles; ++i) {
        loopWaitCycle();
    }
}

void StateGpioHandler::loopWaitCycle()
{
    updateBlinkState();

    delay(StartupCycleTimeMs);

    // loop blink (shows that we didn't stuck)
    digitalWrite(LED_BUILTIN, blinkState);

    // write LOW to all LEDS, except the ledSeq LED:
    for (uint8_t i = FirstLed; i <= LastLed; ++i) {
        digitalWrite(i, i == ledSeq ? HIGH : LOW);
    }

    if (++ledSeq > LastLed) {
        ledSeq = FirstLed;
    }
}

void StateGpioHandler::setup()
{
    Serial.println("Setup StateGpioHandler");

    mqttHandler = app->getMqttHandler();
    networkHandler = app->getNetworkHandler();
    inputRing.setup();

    readEeprom();
    setupPins();
}

void StateGpioHandler::setupPins()
{
    // LEDs
    for (int i = FirstLed; i <= LastLed; ++i) {
        pinMode(i, OUTPUT);
    }

    // Switches
    pinMode(SwitchBuzzmode, INPUT_PULLUP);
    pinMode(SwitchAckBuzz, INPUT_PULLUP);
    pinMode(SwitchAck, INPUT_PULLUP);

    // Ring input (also grounded, has its own pull up)
    pinMode(InputRing, INPUT);

    // Relays
    pinMode(RelayBuzzer, OUTPUT);
    pinMode(RelayExtBell, OUTPUT);

    // Build-in LED for output
    pinMode(LED_BUILTIN, OUTPUT);
}


void StateGpioHandler::readEeprom()
{
    if (!EEPROM.begin(EepromSize)) {
        Serial.println("Failed to initialize EEPROM");
        return;
    }

    Serial.println(EEPROM.read(EepromAddressAutoBuzz));
    autoBuzz = EEPROM.read(EepromAddressAutoBuzz) == 1;
}

void StateGpioHandler::writeEeprom()
{
    EEPROM.write(EepromAddressAutoBuzz, autoBuzz ? 1 : 0);
    EEPROM.commit();
}

void StateGpioHandler::scheduleEvent(EventState& state)
{
    if (state == EventState::Idle) {
        state = EventState::Scheduled;
    }
}

void StateGpioHandler::ring(bool testRing)
{
    ringActive = true;
    mqttHandler->writeRingToMqttAndLog(testRing);
    scheduleEvent(stateExtBell);
    timerBellBlink.start();

    if (autoBuzz) {
        scheduleEvent(stateDoorBuzzer);
    }
}

void StateGpioHandler::buzz()
{
    scheduleEvent(stateDoorBuzzer);
    mqttHandler->writeBuzzToLog(autoBuzz);
}

void StateGpioHandler::setAutoBuzzState(bool newAutoBuzzState)
{
    const bool valueChanged = newAutoBuzzState != autoBuzz;
    autoBuzz = newAutoBuzzState;
    mqttHandler->writeAutoBuzzStateToMqtt(newAutoBuzzState);
    if (valueChanged) writeEeprom();
}

bool StateGpioHandler::getAutoBuzzState() const
{
    return autoBuzz;
}

void StateGpioHandler::ackRing()
{
    ringActive = false;
    mqttHandler->writeAckRingToMqtt();
    timerAckLedOn.start();
    timerBellBlink.stop();
}

void StateGpioHandler::ackRingButton()
{
    if (!ringActive) {
        timerErrorLedOn.start();
        return;
    }
    ackRing();
}

void StateGpioHandler::ackRingAndBuzzButton()
{
    if (!ringActive) {
        timerErrorLedOn.start();
        return;
    }

    ackRing();
    if (!autoBuzz) {
        buzz();
    }
}


void StateGpioHandler::writeLedsInNormalLoop()
{
    digitalWrite(LED_BUILTIN, HIGH);
    digitalWrite(LedOn, HIGH);
    if (timerAckLedOn.check()) {
        digitalWrite(LedAckAutobuzz, HIGH);
    } else {
        digitalWrite(LedAckAutobuzz, autoBuzz ? blinkState : LOW);
    }
    if (wantToReboot) {
        digitalWrite(LedError, blinkState);
    } else {
        digitalWrite(LedError, timerErrorLedOn.check() ? HIGH : LOW);
    }
    // Blick in opposite state of the auto buzzer LED
    digitalWrite(LedDoorbell, timerBellBlink.check() ? !blinkState : static_cast<uint8_t>(ringActive));
}

void StateGpioHandler::readSwitches()
{
    if (switchBuzzMode.checkRaise()) {
        setAutoBuzzState(!autoBuzz);
    }

    if (switchAckBuzz.checkRaise()) {
        ackRingAndBuzzButton();
    }

    if (switchAck.checkRaise()) {
        ackRingButton();
    }
}

void StateGpioHandler::readInputs()
{
    if (inputRing.checkRaise()) {
        ring(false);
    }
}

void StateGpioHandler::handleRelays()
{
    // Never turn of both relays at the same time,
    // the power consumption is probably too high.

    // Scheduled -> Running - only if no other relay is running
    if (stateExtBell == EventState::Scheduled && stateDoorBuzzer != EventState::Running) {
        stateExtBell = EventState::Running;
        timerExtBell.start();
    } else if (stateDoorBuzzer == EventState::Scheduled && stateExtBell != EventState::Running) {
        stateDoorBuzzer = EventState::Running;
        timerDoorBuzzer.start();
    }

    // Running -> Idle
    if (stateExtBell == EventState::Running && !timerExtBell.check()) {
        stateExtBell = EventState::Idle;
    }
    if (stateDoorBuzzer == EventState::Running && !timerDoorBuzzer.check()) {
        stateDoorBuzzer = EventState::Idle;
    }

    digitalWrite(RelayBuzzer, stateDoorBuzzer == EventState::Running ? HIGH : LOW);
    digitalWrite(RelayExtBell, stateExtBell == EventState::Running ? HIGH : LOW);
}

void StateGpioHandler::decrementTimers()
{
    timerAckLedOn.decrement();
    timerErrorLedOn.decrement();
    timerDoorBuzzer.decrement();
    timerExtBell.decrement();
    timerBellBlink.decrement();
    timerErrorLedOn.decrement();
    timerReboot.decrement();
}

const CircularArray<String, MaxRawDataStrings>& StateGpioHandler::getArchivedRawDataStrings() const
{
    return inputRing.getArchivedRawDataStrings();
}

String StateGpioHandler::getCurrentRawDataStr() const
{
    return inputRing.getCurrentRawDataStr();
}

void StateGpioHandler::reboot()
{
    if (!wantToReboot) {
        wantToReboot = true;
        timerReboot.start();
    }
}