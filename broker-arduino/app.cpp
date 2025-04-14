#include "app.h"

#include "mqttHandler.h"
#include "networkHandler.h"
#include "stateGpioHandler.h"
#include "timing.h"

NetworkHandler& createNetworkHandler(App* app)
{
    static NetworkHandler networkHandler(app);
    return networkHandler;
}

MqttHandler& createMqttHandler(App* app)
{
    static MqttHandler mqttHandler(app);
    return mqttHandler;
}

StateGpioHandler& createStateGpioHandler(App* app)
{
    static StateGpioHandler stateGpioHandler(app);
    return stateGpioHandler;
}

App::App()
{}

NetworkHandler* App::getNetworkHandler()
{
    return networkHandler;
}

MqttHandler* App::getMqttHandler()
{
    return mqttHandler;
}

StateGpioHandler* App::getStateGpioHandler()
{
    return stateGpioHandler;
}

void App::setup()
{
    networkHandler = &createNetworkHandler(this);
    mqttHandler = &createMqttHandler(this);
    stateGpioHandler = &createStateGpioHandler(this);

    Serial.begin(115200);
    Serial.println("");
    Serial.println("Starting doorbell broker...");

    // LEDs are already needed for WIFI setup (blinking!)
    stateGpioHandler->setup();
    networkHandler->setup();
    mqttHandler->setup();
}

void App::loop()
{
    delay(MainLoopSampleTimeMs);
    networkHandler->loop();
    mqttHandler->loop();
    stateGpioHandler->loop();
}