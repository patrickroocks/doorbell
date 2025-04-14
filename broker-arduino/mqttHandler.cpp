#include "mqttHandler.h"

#include "app.h"
#include "networkHandler.h"
#include "stateGpioHandler.h"

using namespace mqttBrokerName;

// Mqtt Broker is locally on the Arduino
const char* MqttServerAddr = "localhost";
constexpr int MqttBrokerPort = 1883;

// Topics
const char* CommandTopic = "cmd";
const char* RingTopic = "doorRing";
const char* ResponseTopic = "response";

// Commands
const char* CmdBuzz = "buzz";
const char* CmdAutoBuzzOn = "autoBuzzOn";
const char* CmdAutoBuzzOff = "autoBuzzOff";
const char* CmdGetActionLog = "getActionLog";
const char* CmdGetAutoBuzz = "getAutoBuzz";
const char* CmdGetStartTime = "getStartTime";
const char* CmdTestRing = "testRing";
const char* CmdPing = "ping";
const char* CmdAckRing = "ackRing";
const char* CmdRawData = "getRawData";

// Messages
const char* MsgRing = "ring";
const char* MsgAckRing = "ackRing";
const char* MsgTestRing = "testRing";
const char* MsgAutoBuzzOn = "autoBuzzOn";
const char* MsgAutoBuzzOff = "autoBuzzOff";
const char* MsgPong = "pong";
const char* MsgBuzzAck = "buzzAck";
const char* MsgEndMultiResponse = "endMultiResponse";

MqttHandler::MqttHandler(App* app)
    : app(app)
    , broker(MqttBrokerPort)
    , espClient()
    , client(espClient)
{}

void MqttHandler::reconnectMqttClient()
{
    while (!client.connected()) {
        Serial.print("[MQTT] Connect...");
        if (client.connect("DoorbellBrokerESP32")) {
            Serial.println("[MQTT] connected");
            client.subscribe(CommandTopic);
            Serial.print("[MQTT] subscribed to topic: ");
            Serial.println(CommandTopic);
        } else {
            Serial.print("[MQTT] error, rc=");
            Serial.println(client.state());
            stateGpioHandler->waitSeconds(3);
        }
    }
}

void MqttHandler::setupMqttBroker()
{
    // Start the mqtt broker.
    // Does not need anything in the "loop" part!
    broker.setMaxNumClients(9); // set according to your system.
    broker.startBroker();
    Serial.println("MQTT broker started");
}

void MqttHandler::setupMqttClient()
{
    client.setBufferSize(1024);
    client.setServer(MqttServerAddr, 1883);
    client.setCallback([this](char* topic, byte* payload, unsigned int length) { callbackMqtt(topic, payload, length); });
    Serial.println("MQTT Client started. Destination: ");
    Serial.println(MqttServerAddr);
}

void MqttHandler::setup()
{
    Serial.println("Setup MqttHandler");
    stateGpioHandler = app->getStateGpioHandler();
    networkHandler = app->getNetworkHandler();

    startTime = networkHandler->getDateTime();
    addToActionLog("Device started");

    setupMqttBroker();
    setupMqttClient();
}

void MqttHandler::loop()
{
    // Reconnect/Loop MQTT
    if (!client.connected()) {
        if (mqttConnected) {
            mqttConnected = false;
            // We were alread connected: MQTT Server probably crashed, reboot the Arduino
            Serial.println("MQTT connection lost, reboot...");
            stateGpioHandler->reboot();
            return;
        }
        reconnectMqttClient();
    } else {
        mqttConnected = true;
        client.loop();
    }
}

void MqttHandler::callbackMqtt(char* topic, byte* payload, unsigned int length)
{
    if (String(topic) == CommandTopic) {
        String payloadStr = "";
        for (unsigned int i = 0; i < length; i++) {
            payloadStr += (char) payload[i];
        }

        if (payloadStr == CmdBuzz) {
            stateGpioHandler->buzz();
            client.publish(ResponseTopic, MsgBuzzAck);
        } else if (payloadStr == CmdAutoBuzzOn) {
            stateGpioHandler->setAutoBuzzState(true);
        } else if (payloadStr == CmdAutoBuzzOff) {
            stateGpioHandler->setAutoBuzzState(false);
        } else if (payloadStr == CmdTestRing) {
            stateGpioHandler->ring(true);
        } else if (payloadStr == CmdGetActionLog) {
            showActionLog();
        } else if (payloadStr == CmdPing) {
            client.publish(ResponseTopic, MsgPong);
        } else if (payloadStr == CmdGetAutoBuzz) {
            client.publish(ResponseTopic, stateGpioHandler->getAutoBuzzState() ? MsgAutoBuzzOn : MsgAutoBuzzOff);
        } else if (payloadStr == CmdAckRing) {
            stateGpioHandler->ackRing();
        } else if (payloadStr == CmdRawData) {
            showRawData();
        } else if (payloadStr == CmdGetStartTime) {
            client.publish(ResponseTopic, startTime.c_str());
        } else {
            Serial.print("[MQTT] received unknown command: ");
            Serial.println(payloadStr);
        }
    } else {
        // should not happen, we are not subscribed to other topics
        Serial.print("[MQTT] received data on unexpected topic: ");
        Serial.println(topic);
    }
}

void MqttHandler::addToActionLog(const String& action)
{
    actionLog.push(networkHandler->getDateTime() + " " + action);
}

void MqttHandler::showRawData()
{
    const auto& rawDataStrings = stateGpioHandler->getArchivedRawDataStrings();

    for (int i = 0; i < rawDataStrings.size(); ++i) {
        client.publish(ResponseTopic, rawDataStrings.at(i).c_str());
    }

    const String lastString = stateGpioHandler->getCurrentRawDataStr();
    if (lastString.length() > 0) {
        client.publish(ResponseTopic, lastString.c_str());
    }

    client.publish(ResponseTopic, MsgEndMultiResponse);
}

void MqttHandler::writeAutoBuzzStateToMqtt(bool newAutoBuzzState)
{
    client.publish(RingTopic, newAutoBuzzState ? MsgAutoBuzzOn : MsgAutoBuzzOff);
}

void MqttHandler::writeAckRingToMqtt()
{
    if (client.connected()) {
        client.publish(RingTopic, MsgAckRing);
    }
}

void MqttHandler::writeRingToMqttAndLog(bool testRing)
{
    const String ringStr = testRing ? MsgTestRing : MsgRing;

    Serial.println(ringStr + " detected");
    addToActionLog(ringStr);

    if (client.connected()) {
        String pubString = ringStr + " ";
        if (stateGpioHandler->getAutoBuzzState() && !testRing) pubString += "auto buzz, ";
        pubString += networkHandler->getDateTime();
        client.publish(RingTopic, pubString.c_str());
    } else {
        Serial.println("MQTT not connected, cannot publish ring event");
    }
}

void MqttHandler::writeBuzzToLog(bool autoBuzz)
{
    addToActionLog(String{"buzz "} + (autoBuzz ? " (auto)" : "(manual)"));
}

void MqttHandler::showActionLog()
{
    for (const auto& entry : actionLog) {
        client.publish(ResponseTopic, entry.c_str());
    }

    client.publish(ResponseTopic, MsgEndMultiResponse);
}

bool MqttHandler::getMqttConnected() const
{
    return mqttConnected;
}