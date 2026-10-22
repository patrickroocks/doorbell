#pragma once

#include "circularArray.h"
#include "configStructs.h"

#include <Arduino.h>
#include <EmbeddedMqttBroker.h>
#include <PubSubClient.h>

constexpr int ActionLogSize = 20;
constexpr int OutboxSize = 8;
constexpr int MultiResponseLinesPerLoop = 10;

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
    void writeDeviceBuzzToMqtt();
    void addToActionLog(const String& action);

private:
    void setupMqttBroker();
    void setupMqttClient();

    void reconnectMqttClient();
    void pickNextMqttConfigStartBrokerIfNeeded();
    void tryToConnectToCurrentConfig();

    void callbackMqtt(char* topic, byte* payload, unsigned int length);

    // A message to be published.
    struct OutboundMessage final
    {
        OutboundMessage() = default;
        OutboundMessage(const char* topic, const String& payload)
            : topic(topic)
            , payload(payload)
        {}

        const char* topic = nullptr;
        String payload;
    };

    // A multi line response that is currently being sent out, line by line.
    enum class MultiResponse
    {
        None,
        ActionLog,
        RawData,
    };

    // Publishing.
    void enqueueMessage(const char* topic, const String& payload);
    void flushOutbox();
    void startMultiResponse(MultiResponse kind);
    void sendPendingMultiResponse();
    bool publishNextMultiResponseLine();
    void discardPendingMessages();

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

    // Outbox for important messages (like ring) which is entirely published in every loop.
    CircularArray<OutboundMessage, OutboxSize> outbox;

    // Queue for multi-line reponses (e.g., log, raw data), limited per loop to avoid blocking.
    MultiResponse pendingMultiResponse = MultiResponse::None;
    CircularArraySequenceCounter multiResponseSeqNr;
};
