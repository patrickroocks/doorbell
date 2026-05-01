#pragma once

#include <Arduino.h>

struct WifiConfig
{
    String ssid;
    String password;
};

struct MqttClientConfig
{
    String brokerAddress;
    int brokerPort;
};