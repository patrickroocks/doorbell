#pragma once

#include "circularArray.h"
#include "configStructs.h"

#include <Arduino.h>
#include <EmbeddedMqttBroker.h>
#include <PubSubClient.h>

constexpr int ActionLogSize = 50;

class App;
class StateGpioHandler;
class NetworkHandler;

class MqttHandler final
{
public:
    MqttHandler(App* app);

    void loop();
    void setup();

    void writeRingToMqttAndLog(bool testRing);
    void writeBuzzToLog(bool autoBuzz);
    void writeAutoBuzzStateToLogAndMqtt(bool newAutoBuzzState);
    void writeAckRingToMqtt();
    bool getMqttConnected() const;
    void addToActionLog(const String& action);

private:
    void setupMqttBroker();
    void setupMqttClient();

    void reconnectMqttClient();
    void pickNextMqttConfigStartBrokerIfNeeded();
    void tryToConnectToCurrentConfig();

    void callbackMqtt(char* topic, byte* payload, unsigned int length);
    void showActionLog();
    void showRawData();

    // Connection to other components
    App* const app;
    StateGpioHandler* stateGpioHandler = nullptr;
    NetworkHandler* networkHandler = nullptr;

    // MQTT stack
    mqttBrokerName::MqttBroker* broker = nullptr;
    WiFiClient espClient;
    PubSubClient client;

    // State
    MqttClientConfig currentMqttConfig;
    int currentMqttConfigIndex = -1;
    bool mqttConnected = false;
    CircularArray<String, ActionLogSize> actionLog;
    String startTime;
};