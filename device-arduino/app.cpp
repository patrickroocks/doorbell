#include "app.h"

#include "mqttHandler.h"
#include "networkHandler.h"
#include "stateGpioHandler.h"
#include "timing.h"
#include "version.h"

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
    Serial.println(String("Starting doorbell device, version ") + DeviceVersion + "...");

    // LEDs are already needed for WIFI setup (blinking!)
    stateGpioHandler->setup();
    stateGpioHandler->waitSeconds(1); // ensures we see all LEDs at least once
    networkHandler->setup();
    mqttHandler->setup();
}

void App::loop()
{
    waitForNextCycle();
    networkHandler->loop();
    mqttHandler->loop();
    stateGpioHandler->loop();
}

void App::waitForNextCycle()
{
    const uint32_t now = millis();
    const int32_t remainingMs = (int32_t) (nextCycleMs - now);
    if (remainingMs > 0) {
        delay(remainingMs);
    } else {
        nextCycleMs = now;
    }
    nextCycleMs += MainLoopSampleTimeMs;
}