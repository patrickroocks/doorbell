#pragma once

#include <QtCore>

class RingApp;

struct Config final
{
	QString wlanPattern;
	QString brokerAddr;

	void loadFromJson(const QJsonObject& obj);
	QJsonObject toJson() const;
};

class ConfigStore final
{
public:
	ConfigStore(RingApp* app);

	const Config& getDefaultConfig() const { return defaultConfig; }
	const Config& getCurrentConfig() const { return currentConfig; }

	void setWlanPattern(const QString& pattern);
	void setBrokerAddr(const QString& addr);

	void saveToFile();

private:
	QJsonObject getConfigFromFile(const QString& filePath);

	Config currentConfig;
	const Config defaultConfig;

	QString const configFilePath;
};
