#pragma once

#include <QtCore>

struct Config final
{
	QString wlanPattern;
	QString brokerAddr;
	QString mqttClientID;

	void loadFromJson(const QJsonObject& obj);
	QJsonObject toJson() const;
	static Config getDefaultConfig();

	bool operator!=(const Config& other) const;
};
