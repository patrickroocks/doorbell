#pragma once

#include "timer.h"

#include <Arduino.h>

#include <NTP.h>
#include <PubSubClient.h>
#include <WiFi.h>
#include <WiFiUdp.h>

class App;
class MqttHandler;
class StateGpioHandler;
class WifiConfig;

class NetworkHandler final
{
public:
    NetworkHandler(App* app);
    String getDateTime();
    void setup();
    void loop();
    bool getWifiConnected() const;
    bool hasValidTime() const;
    void setRequestLogWhenValidTime();
    void preferNextWifiOnNextStart();


private:
    bool handleWifiConnection();
    uint8_t getStartWifiIndex() const;
    void setupWifiConn(const WifiConfig& wifiConfig);
    void setupNTP();

    // Connection to other components
    App* const app;
    StateGpioHandler* stateGpioHandler = nullptr;
    MqttHandler* mqttHandler = nullptr;

    // Connection stack
    WiFiUDP wifiUdp;
    NTP ntp;
    EventTimer ntpTimer;
    EventTimer wifiReconnectTimer; // one shot, no auto restart

    // Number of entries in getWifiConfigs()
    uint8_t wifiCount = 0;

    // System states
    bool wifiConnected = false;
    bool validTime = false;
    bool logWhenValidTime = false;
    bool wifiReconnectPending = false;
};