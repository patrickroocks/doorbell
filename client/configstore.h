#pragma once

#include "config.h"

class RingApp;

class ConfigStore final
{
public:
	explicit ConfigStore(RingApp* app);
	const Config& getCurrentConfig() const { return currentConfig; }
	void setNewConfig(const Config& newConfig);

private:
	static QJsonObject getConfigFromFile(const QString& filePath);
	void saveCurrentConfigToFile() const;

	Config currentConfig;
	QString const configFilePath;
};
