#include "arduino_secrets.h"
#include "state-and-gpio-handler.h"

#include <EmbeddedMqttBroker.h>
#include <NTP.h>
#include <PubSubClient.h>
#include <WiFi.h>
#include <WiFiUdp.h>

#include <deque>

using namespace mqttBrokerName;

#define WIFI_TIMEOUT_CYCLES 20 // 10sec, as a startup cycle is 500ms

// =============================================
//              STATE
// =============================================

bool mqttConnected = false;

std::deque<String> ringArchive;
#define MAX_RING_ARCHIVE 20

String startUpTime;

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
const char* cmdShowRingArchive = "showRingArchive";
const char* cmdGetAutoBuzz = "getAutoBuzz";
const char* cmdGetStartUpTime = "getStartUpTime";
const char* cmdTestExtRing = "test_ext_ring";
const char* cmdPing = "ping";
const char* cmdAckRing = "ackRing";
const char* cmdRawData = "getRawData";

// Messages
const char* msgRing = "ring";
const char* msgTestRing = "testRing";
const char* msgAutoBuzzOn = "autoBuzzOn";
const char* msgAutoBuzzOff = "autoBuzzOff";
const char* msgPong = "pong";
const char* msgBuzzAck = "buzzAck";

const char* mqttServerAddr = "localhost";

// =============================================
//              main config
// =============================================

#define MQTT_BROKER_PORT 1883

// *** Connections/Servers

MqttBroker broker(MQTT_BROKER_PORT);
WiFiClient espClient;
PubSubClient client(espClient);

// =============================================
//              EVENTS
// =============================================

void writeRingToMqtt(bool testRing)
{
    if (!wifiConnected) return;

    Serial.println((testRing ? String() : "test ") + "ring detected at " + getDateTime());

    ringArchive.push_back(getDateTime() + (testRing ? " (t)" : String()));

    if (ringArchive.size() > MAX_RING_ARCHIVE) {
        ringArchive.pop_front();
    }

    if (client.connected()) {
        client.publish(ringTopic, testRing ? msgTestRing : msgRing);
    } else {
        Serial.println("MQTT not connected, cannot publish ring event");
    }
}

void internalRing()
{
    timerExtBell.start();
    timerBellBlink.start();

    if (autoBuzz) {
        eventBuzz();
    }
}

void ringAndWriteToMqtt(bool testRing)
{
    writeRingToMqtt(testRing);
    internalRing();
}

void sendAutoBuzzState(bool newAutoBuzzState)
{
    client.publish(ringTopic, newAutoBuzzState ? msgAutoBuzzOn : msgAutoBuzzOff);
}

void showArchive()
{
    String archive = "";
    for (auto& ring : ringArchive) {
        archive += ring;
        if (&ring != &ringArchive.back()) {
            archive += ", ";
        }
    }

    client.publish(responseTopic, archive.c_str());
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
            eventBuzz();
            client.publish(responseTopic, msgBuzzAck);
        } else if (payloadStr == cmdAutoBuzzOn) {
            setAutoBuzz(true);
        } else if (payloadStr == cmdAutoBuzzOff) {
            setAutoBuzz(false);
        } else if (payloadStr == cmdTestExtRing) {
            ringAndWriteToMqtt(true);
        } else if (payloadStr == cmdShowRingArchive) {
            showArchive();
        } else if (payloadStr == cmdPing) {
            client.publish(responseTopic, msgPong);
        } else if (payloadStr == cmdGetAutoBuzz) {
            client.publish(responseTopic, autoBuzz ? "on" : "off");
        } else if (payloadStr == cmdAckRing) {
            timerBellBlink.stop();
        } else if (payloadStr == cmdRawData) {
            for (const String& str : inputRing.getRawDataStrings()) {
                client.publish(responseTopic, str.c_str());
            }
        } else if (payloadStr == cmdGetStartUpTime) {
            client.publish(responseTopic, startUpTime.c_str());
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

void setupWifi()
{
    // from the ardunio_secrets.h (SSID/password)
    const char* ssid = SECRET_SSID;
    const char* password = SECRET_PASS;

    WiFi.begin(ssid, password);
    Serial.println("Connecting to WiFi ");
    int numCycles = 0;
    while (WiFi.status() != WL_CONNECTED) {
        waitCycle();
        Serial.print(".");
        if (++numCycles > WIFI_TIMEOUT_CYCLES) {
            Serial.println("WiFi connection timeout");
            return;
        }
    }
    Serial.println("WiFi connected. IP: ");
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
    setRingCommand(ringAndWriteToMqtt);
    setAutoBuzzChangeCommand(sendAutoBuzzState);

    setupWifi();
    setupNTP();
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
    // Loop MQTT
    if (!client.connected()) {
        reconnectMqttClient();
    }
    client.loop();

    if (startUpTime.length() == 0) {
        startUpTime = getDateTime();
    }

    // LOOP NTP
    if (wifiConnected) {
        if (ntpTimer.checkAndDecrement()) ntp.update();
    }
}