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


private:
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

    // System states
    bool wifiConnected = false;
    bool startupCycleCompleted = false;
    bool validTime = false;
    bool logWhenValidTime = false;
};