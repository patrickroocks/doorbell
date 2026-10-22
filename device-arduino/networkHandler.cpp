#include "networkHandler.h"

#include "app.h"
#include "arduinoSecrets.h"
#include "mqttHandler.h"
#include "stateGpioHandler.h"
#include "timing.h"

NetworkHandler::NetworkHandler(App* app)
    : app(app)
    , ntp(wifiUdp)
    , ntpTimer(NtpUpdateIntervalMs, true)
    , wifiReconnectTimer(WifiReconnectWaitTimeMs)
{}

String NetworkHandler::getDateTime()
{
    if (validTime) {
        return ntp.formattedTime("%Y-%m-%d %H:%M:%S");
    } else {
        // return seconds since device start:
        return "(No NTP time, seconds since device start: " + String(millis() / 1000) + ")";
    }
}

void NetworkHandler::setup()
{
    Serial.println("Setup NetworkHandler");

    stateGpioHandler = app->getStateGpioHandler();
    mqttHandler = app->getMqttHandler();

    wifiConnected = false;

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
    if (!handleWifiConnection()) return;

    // Loop NTP, regularly update time.
    if (ntpTimer.checkEvent()) ntp.update();
    if (!validTime) {
        validTime = ntp.year() != 1970;
        if (validTime && logWhenValidTime) {
            mqttHandler->addToActionLog("Received first NTP time");
            logWhenValidTime = false;
        }
    }
}

// Keeps track of the WiFi link.
bool NetworkHandler::handleWifiConnection()
{
    if (WiFi.status() == WL_CONNECTED) {
        if (wifiReconnectPending) {
            Serial.println("WiFi reconnected.");
            mqttHandler->addToActionLog("WiFi reconnected");
            wifiReconnectPending = false;
        }
        wifiConnected = true;
        return true;
    }

    wifiConnected = false;

    if (!wifiReconnectPending) {
        Serial.println("WiFi disconnected, trying to reconnect...");
        wifiReconnectPending = true;
        wifiReconnectTimer.start();
        WiFi.reconnect();
        return false;
    }

    if (wifiReconnectTimer.checkEvent()) {
        // Fail over to next WiFI config.
        Serial.println("WiFi reconnect failed, reboot...");
        stateGpioHandler->reboot();
    }
    return false;
}

void NetworkHandler::setupWifiConn(const WifiConfig& wifiConfig)
{
    WiFi.begin(wifiConfig.ssid.c_str(), wifiConfig.password.c_str());
    Serial.println("Connecting to WiFi '" + wifiConfig.ssid + "'");
    const uint32_t deadlineMs = millis() + WifiConnectTimeoutMs;
    while (WiFi.status() != WL_CONNECTED) {
        stateGpioHandler->loopWaitCycle();
        Serial.print(".");
        if ((int32_t) (millis() - deadlineMs) >= 0) {
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
    validTime = ntp.year() != 1970;
}

bool NetworkHandler::getWifiConnected() const
{
    return wifiConnected;
}

bool NetworkHandler::hasValidTime() const
{
    return validTime;
}

void NetworkHandler::setRequestLogWhenValidTime()
{
    // Not yet a valid NTP time, log when we get a valid time.
    logWhenValidTime = true;
}