#pragma once

#include <Arduino.h>

class StateGpioHandler;
class MqttHandler;
class NetworkHandler;

class App
{
public:
    App();

    void setup();
    void loop();

    NetworkHandler* getNetworkHandler();
    MqttHandler* getMqttHandler();
    StateGpioHandler* getStateGpioHandler();

private:
    NetworkHandler* networkHandler;
    MqttHandler* mqttHandler;
    StateGpioHandler* stateGpioHandler;

    bool startupCycleCompleted = false;
};