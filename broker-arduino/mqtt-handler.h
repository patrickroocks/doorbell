#include "arduino_secrets.h"
#include "state-and-gpio-handler.h"

#include <EmbeddedMqttBroker.h>
#include <NTP.h>
#include <PubSubClient.h>
#include <WiFi.h>
#include <WiFiUdp.h>

#include <deque>

using namespace mqttBrokerName;

#define WIFI_TIMEOUT_CYCLES 10 // 5sec, as a startup cycle is 500ms

// =============================================
//              STATE
// =============================================

bool mqttConnected = false;

std::deque<String> actionLog;
#define MAX_ACTION_LOG_SIZE 50

String startTime;

// =============================================
//              TIME
// =============================================

WiFiUDP wifiUdp;
NTP ntp(wifiUdp);

String getDateTime()
{
    return ntp.formattedTime("%Y-%m-%d %H:%M:%S");
}

#define NTP_UPDATE_INTERVAL 10 // 1sec
EventTimer ntpTimer(NTP_UPDATE_INTERVAL, true);

// =============================================
//              MQTT Topics/Messages
// =============================================

// Topics
const char* commandTopic = "cmd";
const char* ringTopic = "doorRing";
const char* responseTopic = "response";

// Commands
const char* cmdBuzz = "buzz";
const char* cmdAutoBuzzOn = "autoBuzzOn";
const char* cmdAutoBuzzOff = "autoBuzzOff";
const char* cmdGetActionLog = "getActionLog";
const char* cmdGetAutoBuzz = "getAutoBuzz";
const char* cmdGetStartTime = "getStartTime";
const char* cmdTestRing = "testRing";
const char* cmdPing = "ping";
const char* cmdAckRing = "ackRing";
const char* cmdRawData = "getRawData";

// Messages
const char* msgRing = "ring";
const char* msgAckRing = "ackRing";
const char* msgTestRing = "testRing";
const char* msgAutoBuzzOn = "autoBuzzOn";
const char* msgAutoBuzzOff = "autoBuzzOff";
const char* msgPong = "pong";
const char* msgBuzzAck = "buzzAck";
const char* msgEndMultiResponse = "endMultiResponse";

// Mqtt Broker is locally on the Arduino
const char* mqttServerAddr = "localhost";

// =============================================
//              main config
// =============================================

#define MQTT_BROKER_PORT 1883

// *** Connections/Servers

std::vector<WifiConfig> wifiConfigs;
MqttBroker broker(MQTT_BROKER_PORT);
WiFiClient espClient;
PubSubClient client(espClient);

// =============================================
//              EVENTS
// =============================================

void addToActionLog(const String& action)
{
    actionLog.push_back(getDateTime() + " " + action);
    if (actionLog.size() > MAX_ACTION_LOG_SIZE) {
        actionLog.pop_front();
    }
}

void writeRingToMqtt(bool testRing)
{
    if (!wifiConnected) return;

    const String ringStr = testRing ? msgTestRing : msgRing;

    Serial.println(ringStr + " detected");
    addToActionLog(ringStr);

    if (client.connected()) {
        String pubString = ringStr + " ";
        if (autoBuzz && !testRing) pubString += "auto buzz, ";
        pubString += getDateTime();
        client.publish(ringTopic, pubString.c_str());
    } else {
        Serial.println("MQTT not connected, cannot publish ring event");
    }
}

void ringAndWriteToMqtt(bool testRing)
{
    writeRingToMqtt(testRing);

    internalRing();


    if (autoBuzz && !testRing) {
        eventBuzz(true);
    }
}

void buzzAndWriteToLog(bool autoBuzz)
{
    internalBuzz();
    addToActionLog(String{"buzz "} + (autoBuzz ? " (auto)" : "(manual)"));
}

void ackRingAndWriteToMqtt()
{
    internalAck();

    if (client.connected()) {
        client.publish(ringTopic, msgAckRing);
    }
}

void sendAutoBuzzState(bool newAutoBuzzState)
{
    client.publish(ringTopic, newAutoBuzzState ? msgAutoBuzzOn : msgAutoBuzzOff);
}

void showActionLog()
{
    for (const auto& entry : actionLog) {
        client.publish(responseTopic, entry.c_str());
    }

    client.publish(responseTopic, msgEndMultiResponse);
}

void showRawData()
{
    for (const String& str : inputRing.getRawDataStrings()) {
        client.publish(responseTopic, str.c_str());
    }

    client.publish(responseTopic, msgEndMultiResponse);
}

// =============================================
//              CALLBACKS
// =============================================

void callbackMqtt(char* topic, byte* payload, unsigned int length)
{
    if (String(topic) == commandTopic) {
        String payloadStr = "";
        for (unsigned int i = 0; i < length; i++) {
            payloadStr += (char) payload[i];
        }

        if (payloadStr == cmdBuzz) {
            eventBuzz(false);
            client.publish(responseTopic, msgBuzzAck);
        } else if (payloadStr == cmdAutoBuzzOn) {
            setAutoBuzz(true);
        } else if (payloadStr == cmdAutoBuzzOff) {
            setAutoBuzz(false);
        } else if (payloadStr == cmdTestRing) {
            ringAndWriteToMqtt(true);
        } else if (payloadStr == cmdGetActionLog) {
            showActionLog();
        } else if (payloadStr == cmdPing) {
            client.publish(responseTopic, msgPong);
        } else if (payloadStr == cmdGetAutoBuzz) {
            client.publish(responseTopic, autoBuzz ? msgAutoBuzzOn : msgAutoBuzzOff);
        } else if (payloadStr == cmdAckRing) {
            internalAck();
        } else if (payloadStr == cmdRawData) {
            showRawData();
        } else if (payloadStr == cmdGetStartTime) {
            client.publish(responseTopic, startTime.c_str());
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

// =============================================
//              SETUP
// =============================================

void setupWifiConn(const WifiConfig& wifiConfig)
{
    WiFi.begin(wifiConfig.ssid.c_str(), wifiConfig.password.c_str());
    Serial.println("Connecting to WiFi '" + wifiConfig.ssid + "'");
    int numCycles = 0;
    while (WiFi.status() != WL_CONNECTED) {
        waitCycle();
        Serial.print(".");
        if (++numCycles > WIFI_TIMEOUT_CYCLES) {
            Serial.println("WiFi connection timeout for '" + wifiConfig.ssid + "'");
            return;
        }
    }
    Serial.println("WiFi '" + wifiConfig.ssid + "' connected. IP: ");
    Serial.println(WiFi.localIP());
    wifiConnected = true;
}

void setupNTP()
{
    // Timezones
    ntp.ruleDST("CEST", Last, Sun, Mar, 2, 120);
    ntp.ruleSTD("CET", Last, Sun, Oct, 3, 60);
    ntp.begin();
}

void setupWifi()
{
    wifiConnected = false;
    startupCycleCompleted = false;

    for (const auto& wifiConfig : wifiConfigs) {
        setupWifiConn(wifiConfig);
        if (wifiConnected) {
            break;
        }
    }

    if (wifiConnected) {
        setupNTP();
    }
}

void setupMqttBroker()
{
    // Start the mqtt broker.
    // Does not need anything in the "loop" part!
    broker.setMaxNumClients(9); // set according to your system.
    broker.startBroker();
    Serial.println("MQTT broker started");
}

void setupMqttClient()
{
    client.setBufferSize(1024);
    client.setServer(mqttServerAddr, 1883);
    client.setCallback(callbackMqtt);
    Serial.println("MQTT Client started. Destination: ");
    Serial.println(mqttServerAddr);
}

void setupWifiAndMqtt()
{
    // Connection to state-and-gpio-handler
    setRingCommand(ringAndWriteToMqtt);
    setAutoBuzzChangeCommand(sendAutoBuzzState);
    setBuzzCommand(buzzAndWriteToLog);
    setAckRingCommand(ackRingAndWriteToMqtt);
    setTimeStrGetter(getDateTime);

    // from arduiono_secrets.h
    wifiConfigs = getWifiConfigs();

    setupWifi();
    setupMqttBroker();
    setupMqttClient();
}

// =============================================
//              LOOP
// =============================================

void reconnectMqttClient()
{
    while (!client.connected()) {
        Serial.print("[MQTT] Connect...");
        if (client.connect("DoorbellBrokerESP32")) {
            Serial.println("[MQTT] connected");
            client.subscribe(commandTopic);
            Serial.print("[MQTT] subscribed to topic: ");
            Serial.println(commandTopic);
            mqttConnected = true;
        } else {
            Serial.print("[MQTT] error, rc=");
            Serial.println(client.state());
            waitSec(3);
        }
    }
}

void loopClients()
{
    // Reconnect wifi
    if (WiFi.status() != WL_CONNECTED) {
        Serial.println("[WIFI] disconnected, reconnecting...");
        setupWifi();
    }

    // Reconnect/Loop MQTT
    if (!client.connected()) {
        reconnectMqttClient();
    }
    client.loop();

    if (startTime.length() == 0) {
        startTime = getDateTime();
    }

    // LOOP NTP
    if (wifiConnected) {
        if (ntpTimer.checkAndDecrement()) ntp.update();
    }
}