#include "mqttHandler.h"

#include "app.h"
#include "arduinoSecrets.h"
#include "networkHandler.h"
#include "stateGpioHandler.h"

using namespace mqttBrokerName;

// Port for Mqtt Broker (if used)
constexpr int MqttBrokerPort = 1883;

// MQTT protocol contract (topics, messages, examples): see doc/mqtt-contract.md - keep in sync.

// Topics
const char* CommandTopic = "cmd";
const char* RingTopic = "doorRing";
const char* ResponseTopic = "response";

// Commands
const char* CmdBuzz = "buzz";
const char* CmdAutoBuzzOn = "autoBuzzOn";
const char* CmdAutoBuzzOff = "autoBuzzOff";
const char* CmdRuleAutoBuzzOn = "ruleAutoBuzzOn";
const char* CmdRuleAutoBuzzOff = "ruleAutoBuzzOff";
const char* CmdGetActionLog = "getActionLog";
const char* CmdGetAutoBuzz = "getAutoBuzz";
const char* CmdGetStartTime = "getStartTime";
const char* CmdTestRing = "testRing";
const char* CmdPing = "ping";
const char* CmdAckRing = "ackRing";
const char* CmdRawData = "getRawData";
const char* CmdReboot = "reboot";

// Messages
const char* MsgRing = "ring";
const char* MsgAckRing = "ackRing";
const char* MsgDeviceBuzz = "deviceBuzz";
const char* MsgAckReboot = "ackReboot";
const char* MsgTestRing = "testRing";
const char* MsgAutoBuzzOn = "autoBuzzOn";
const char* MsgAutoBuzzOff = "autoBuzzOff";
const char* MsgPong = "pong";
const char* MsgBuzzAck = "buzzAck";
const char* MsgEndMultiResponse = "endMultiResponse";

// Connection
const int connectionTriesPerConfig = 3;

MqttHandler::MqttHandler(App* app)
    : app(app)
    , espClient()
    , client(espClient)
{}

void MqttHandler::reconnectMqttClient()
{
    pickNextMqttConfigStartBrokerIfNeeded();
    tryToConnectToCurrentConfig();
}

void MqttHandler::pickNextMqttConfigStartBrokerIfNeeded()
{
    if (currentMqttConfig.brokerAddress == "localhost") {
        // We are currently running a local broker, stop it before switching configs
        broker->stopBroker();
    }

    ++currentMqttConfigIndex;
    auto mqttConfigs = getMqttConfigs();
    if (currentMqttConfigIndex == mqttConfigs.size()) {
        currentMqttConfigIndex = 0;
    }
    Serial.print("Trying MQTT config: ");
    Serial.print(mqttConfigs.at(currentMqttConfigIndex).brokerAddress);
    Serial.print(":");
    Serial.println(mqttConfigs.at(currentMqttConfigIndex).brokerPort);

    currentMqttConfig = mqttConfigs.at(currentMqttConfigIndex);
    client.setServer(currentMqttConfig.brokerAddress.c_str(), currentMqttConfig.brokerPort);

    if (currentMqttConfig.brokerAddress == "localhost") {
        setupMqttBroker();
    }
}

void MqttHandler::tryToConnectToCurrentConfig()
{
    int currentConnectionTries = 0;

    while (!client.connected() && currentConnectionTries < connectionTriesPerConfig) {
        Serial.print("[MQTT] Connect...");
        if (client.connect("DoorbellBrokerESP32")) {
            Serial.println("[MQTT] connected");
            client.subscribe(CommandTopic);
            Serial.print("[MQTT] subscribed to topic: ");
            Serial.println(CommandTopic);
            break;
        } else {
            Serial.print("[MQTT] error, rc=");
            Serial.println(client.state());
            stateGpioHandler->waitSeconds(3);
            ++currentConnectionTries;
        }
    }
}

void MqttHandler::setupMqttBroker()
{
    // Start the mqtt broker.
    // Does not need anything in the "loop" part!
    if (!broker) broker = MqttBrokerFactory::createTcpBroker(MqttBrokerPort);
    broker->setMaxNumClients(9); // set according to your system.
    broker->startBroker();
    Serial.println("MQTT broker started");
}

void MqttHandler::setupMqttClient()
{
    client.setBufferSize(1024);
    client.setCallback([this](char* topic, byte* payload, unsigned int length) { callbackMqtt(topic, payload, length); });
    Serial.println("MQTT Client started.");
}

void MqttHandler::setup()
{
    Serial.println("Setup MqttHandler");
    stateGpioHandler = app->getStateGpioHandler();
    networkHandler = app->getNetworkHandler();

    startTime = networkHandler->getDateTime();
    // NTP Time is already written to the log, if we have a valid time.
    String ntpHint;
    if (!networkHandler->hasValidTime()) {
        networkHandler->setRequestLogWhenValidTime();
        ntpHint = " (NTP time not yet valid)";
    }
    addToActionLog(String("Device started, initial autoBuzz is ") + (stateGpioHandler->getAutoBuzzState() ? "on" : "off") + ntpHint);

    setupMqttClient();
}

void MqttHandler::loop()
{
    // Wait for WiFi connection?
    if (!networkHandler->getWifiConnected()) return;

    // Reconnect/Loop MQTT
    if (!client.connected()) {
        if (mqttConnected) {
            mqttConnected = false;
            // We were alread connected: MQTT Server probably crashed or Raspi is gone, reboot the Arduino
            Serial.println("MQTT connection lost, reboot...");
            stateGpioHandler->reboot();
            return;
        }
        discardPendingMessages();
        reconnectMqttClient();
    } else {
        if (!mqttConnected) {
            mqttConnected = true;
            stateGpioHandler->initLedsAfterStartUp();
        }

        // Get incoming messages.
        client.loop();
        // Send outgoing messages.
        flushOutbox();
        sendPendingMultiResponse();
    }
}

void MqttHandler::callbackMqtt(char* topic, byte* payload, unsigned int length)
{
    // cf. doc/mqtt-contract.md.
    // We don't publish from here (while receiving), but write into the outbox.
    if (String(topic) == CommandTopic) {
        String payloadStr = "";
        for (unsigned int i = 0; i < length; i++) {
            payloadStr += (char) payload[i];
        }

        if (payloadStr == CmdBuzz) {
            stateGpioHandler->buzz();
            enqueueMessage(ResponseTopic, MsgBuzzAck);
        } else if (payloadStr == CmdAutoBuzzOn) {
            stateGpioHandler->setAutoBuzzState(true);
        } else if (payloadStr == CmdAutoBuzzOff) {
            stateGpioHandler->setAutoBuzzState(false);
        } else if (payloadStr == CmdRuleAutoBuzzOn) {
            stateGpioHandler->setRuleAutoBuzzState(true);
        } else if (payloadStr == CmdRuleAutoBuzzOff) {
            stateGpioHandler->setRuleAutoBuzzState(false);
        } else if (payloadStr == CmdTestRing) {
            stateGpioHandler->ring(true);
        } else if (payloadStr == CmdGetActionLog) {
            startMultiResponse(MultiResponse::ActionLog);
        } else if (payloadStr == CmdPing) {
            enqueueMessage(ResponseTopic, MsgPong);
        } else if (payloadStr == CmdGetAutoBuzz) {
            enqueueMessage(ResponseTopic, stateGpioHandler->getAutoBuzzState() ? MsgAutoBuzzOn : MsgAutoBuzzOff);
        } else if (payloadStr == CmdAckRing) {
            stateGpioHandler->ackRing();
        } else if (payloadStr == CmdRawData) {
            startMultiResponse(MultiResponse::RawData);
        } else if (payloadStr == CmdGetStartTime) {
            enqueueMessage(ResponseTopic, startTime);
        } else if (payloadStr == CmdReboot) {
            enqueueMessage(ResponseTopic, MsgAckReboot);
            stateGpioHandler->reboot();
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

void MqttHandler::enqueueMessage(const char* topic, const String& payload)
{
    if (!client.connected()) {
        Serial.println("[MQTT] not connected, dropping message: " + payload);
        return;
    }
    if (outbox.full()) {
        // Dropping the newest keeps the order of what is already queued intact.
        Serial.println("[MQTT] outbox full, dropping message: " + payload);
        return;
    }
    outbox.push({topic, payload});
}

void MqttHandler::flushOutbox()
{
    while (!outbox.empty()) {
        const OutboundMessage& message = outbox.front();
        if (!client.publish(message.topic, message.payload.c_str())) {
            Serial.println("[MQTT] publish failed on topic: " + String(message.topic));
        }
        outbox.popFront();
    }
}

void MqttHandler::discardPendingMessages()
{
    outbox.clear();
    pendingMultiResponse = MultiResponse::None;
    multiResponseSeqNr = {};
}

void MqttHandler::startMultiResponse(MultiResponse kind)
{
    pendingMultiResponse = kind;
    multiResponseSeqNr = {};
}

void MqttHandler::sendPendingMultiResponse()
{
    if (pendingMultiResponse == MultiResponse::None) return;

    for (int line = 0; line < MultiResponseLinesPerLoop; ++line) {
        if (!publishNextMultiResponseLine()) {
            pendingMultiResponse = MultiResponse::None;
            multiResponseSeqNr = {};
            return;
        }
    }
}

bool MqttHandler::publishNextMultiResponseLine()
{
    // Publishes the line at multiResponseSeqNr. Returns false when the response is complete.
    // We assume that no new `MultiResponseLinesPerLoop` lines are added to raw data / action log while we are sending the response.

    if (pendingMultiResponse == MultiResponse::ActionLog) {
        if (auto elem = actionLog.iterateBySequence(multiResponseSeqNr)) {
            client.publish(ResponseTopic, elem->c_str());
            return true;
        }
    } else if (pendingMultiResponse == MultiResponse::RawData) {
        const auto& rawDataStrings = stateGpioHandler->getArchivedRawDataStrings();

        if (auto elem = rawDataStrings.iterateBySequence(multiResponseSeqNr)) {
            client.publish(ResponseTopic, elem->c_str());
            return true;
        }
        // The still incomplete current recording is appended after the archived ones.
        if (rawDataStrings.isLastSeqNr(multiResponseSeqNr)) {
            ++multiResponseSeqNr.seqNr;
            const String lastString = stateGpioHandler->getCurrentRawDataStr();
            if (lastString.length() > 0) {
                client.publish(ResponseTopic, lastString.c_str());
                return true;
            }
        }
    }

    client.publish(ResponseTopic, MsgEndMultiResponse);
    return false;
}

void MqttHandler::writeAutoBuzzStateToLogAndMqtt(bool newAutoBuzzState)
{
    enqueueMessage(RingTopic, newAutoBuzzState ? MsgAutoBuzzOn : MsgAutoBuzzOff);
    actionLog.push(networkHandler->getDateTime() + " autoBuzz " + (newAutoBuzzState ? "on" : "off"));
}

void MqttHandler::writeAckRingToMqtt()
{
    enqueueMessage(RingTopic, MsgAckRing);
}

void MqttHandler::writeDeviceBuzzToMqtt()
{
    enqueueMessage(RingTopic, MsgDeviceBuzz);
}

void MqttHandler::writeRingToMqttAndLog(bool testRing)
{
    const String ringStr = testRing ? MsgTestRing : MsgRing;

    addToActionLog(ringStr);

    String pubString = ringStr + " ";
    if (stateGpioHandler->getAutoBuzzState() && !testRing) pubString += "auto buzz, ";
    pubString += networkHandler->getDateTime();
    enqueueMessage(RingTopic, pubString);
}

void MqttHandler::writeBuzzToLog(bool autoBuzz)
{
    addToActionLog(String{"buzz "} + (autoBuzz ? "(auto)" : "(manual)"));
}