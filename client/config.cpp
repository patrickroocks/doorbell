#include "config.h"

#include "util.h"

namespace {

const QString WlanPatternKey = "wlanPattern";
const QString BrokerAddrKey = "brokerAddr";
const QString MqttClientIDKey = "mqttClientID";

const QString DefaultBrokerAddr = "localhost";
const QString DefaultWlanPattern = ".*";

QString getComputerName() { return QSysInfo::machineHostName(); }

QString getDefaultClientId() { return QString("DoorbellClient_%1").arg(getComputerName()); }
}

QJsonObject Config::toJson() const
{
	QJsonObject obj;
	obj[WlanPatternKey] = wlanPattern;
	obj[BrokerAddrKey] = brokerAddr;
	obj[MqttClientIDKey] = mqttClientID;
	return obj;
}

void Config::loadFromJson(const QJsonObject& obj)
{
	wlanPattern = util::getJsonStringWithDefault(obj, WlanPatternKey, DefaultWlanPattern);
	brokerAddr = util::getJsonStringWithDefault(obj, BrokerAddrKey, DefaultBrokerAddr);
	mqttClientID = util::getJsonStringWithDefault(obj, MqttClientIDKey, getDefaultClientId());
}

Config Config::getDefaultConfig()
{
	return Config{.wlanPattern = DefaultWlanPattern, .brokerAddr = DefaultBrokerAddr, .mqttClientID = getDefaultClientId()};
}

bool Config::operator!=(const Config& other) const
{
	return wlanPattern != other.wlanPattern || brokerAddr != other.brokerAddr || mqttClientID != other.mqttClientID;
}
