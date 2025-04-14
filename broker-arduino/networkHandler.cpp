#include "networkHandler.h"

#include "app.h"
#include "arduinoSecrets.h"
#include "stateGpioHandler.h"
#include "timing.h"

constexpr int NtpUpdateIntervalCycles = 10; // 1sec

NetworkHandler::NetworkHandler(App* app)
    : app(app)
    , ntp(wifiUdp)
    , ntpTimer(NtpUpdateIntervalCycles, true)
{}

String NetworkHandler::getDateTime()
{
    return ntp.formattedTime("%Y-%m-%d %H:%M:%S");
}

void NetworkHandler::setup()
{
    Serial.println("Setup NetworkHandler");

    stateGpioHandler = app->getStateGpioHandler();

    wifiConnected = false;
    startupCycleCompleted = false;

    auto wifiConfigs = getWifiConfigs();

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


void NetworkHandler::loop()
{
    // Reconnect wifi
    if (WiFi.status() != WL_CONNECTED) {
        wifiConnected = false;
        Serial.println("Wifi disconnected, reboot...");
        stateGpioHandler->reboot();
        return;
    }

    // Loop NTP
    if (wifiConnected) {
        if (ntpTimer.checkAndDecrement()) ntp.update();
    }
}

void NetworkHandler::setupWifiConn(const WifiConfig& wifiConfig)
{
    WiFi.begin(wifiConfig.ssid.c_str(), wifiConfig.password.c_str());
    Serial.println("Connecting to WiFi '" + wifiConfig.ssid + "'");
    int numCycles = 0;
    while (WiFi.status() != WL_CONNECTED) {
        stateGpioHandler->loopWaitCycle();
        Serial.print(".");
        if (++numCycles > WifiTimeoutCycles) {
            Serial.println("WiFi connection timeout for '" + wifiConfig.ssid + "'");
            return;
        }
    }
    Serial.println("WiFi '" + wifiConfig.ssid + "' connected. IP: ");
    Serial.println(WiFi.localIP());
    wifiConnected = true;
}

void NetworkHandler::setupNTP()
{
    // Timezones
    ntp.ruleDST("CEST", Last, Sun, Mar, 2, 120);
    ntp.ruleSTD("CET", Last, Sun, Oct, 3, 60);
    ntp.begin();
}

bool NetworkHandler::getWifiConnected() const
{
    return wifiConnected;
}