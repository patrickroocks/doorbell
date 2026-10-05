#pragma once

#include <Arduino.h>

class StateGpioHandler;
class MqttHandler;
class NetworkHandler;

class App final
{
public:
    App();

    void setup();
    void loop();

    NetworkHandler* getNetworkHandler();
    MqttHandler* getMqttHandler();
    StateGpioHandler* getStateGpioHandler();

private:
    void waitForNextCycle();

    NetworkHandler* networkHandler = nullptr;
    MqttHandler* mqttHandler = nullptr;
    StateGpioHandler* stateGpioHandler = nullptr;

    // Start of the next main loop cycle.
    uint32_t nextCycleMs = 0;
};