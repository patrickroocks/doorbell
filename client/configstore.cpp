#include "configstore.h"

#include "constants.h"
#include "ringapp.h"

ConfigStore::ConfigStore(RingApp* app)
	: configFilePath(app->getExecDir() + ConfigFileName)
{
	currentConfig.loadFromJson(getConfigFromFile(configFilePath));
	saveCurrentConfigToFile();
}

QJsonObject ConfigStore::getConfigFromFile(const QString& filePath)
{
	QFile file(filePath);
	if (!file.exists()) {
		return {};
	}
	file.open(QFile::ReadOnly);
	const QByteArray content = file.readAll();

	QJsonParseError err;
	QJsonDocument doc = QJsonDocument::fromJson(content, &err);

	if (err.error != QJsonParseError::NoError) {
		return {};
	}

	return doc.object();
}

void ConfigStore::saveCurrentConfigToFile() const
{
	QJsonObject obj = currentConfig.toJson();
	QJsonDocument doc(obj);

	QFile file(configFilePath);
	file.open(QFile::WriteOnly);
	file.write(doc.toJson());
}

void ConfigStore::setNewConfig(const Config& newConfig)
{
	currentConfig = newConfig;
	saveCurrentConfigToFile();
}
