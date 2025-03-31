#include "configstore.h"

#include "constants.h"
#include "ringapp.h"
#include "util.h"

namespace {

const QString WlanPatternKey = "wlanPattern";
const QString BrokerAddrKey = "brokerAddr";

const QString DefaultBrokerAddr = "localhost";
const QString DefaultWlanPattern = ".*";
}

QJsonObject Config::toJson() const
{
	QJsonObject obj;
	obj[WlanPatternKey] = wlanPattern;
	obj[BrokerAddrKey] = brokerAddr;
	return obj;
}

void Config::loadFromJson(const QJsonObject& obj)
{
	wlanPattern = util::getJsonStringWithDefault(obj, WlanPatternKey, DefaultWlanPattern);
	brokerAddr = util::getJsonStringWithDefault(obj, BrokerAddrKey, DefaultBrokerAddr);
}

// ----------------------------------------------------------------------------

ConfigStore::ConfigStore(RingApp* app)
	: defaultConfig{Config{.wlanPattern = DefaultWlanPattern, .brokerAddr = DefaultBrokerAddr}}
	, configFilePath(app->getExecDir() + ConfigFileName)
{
	currentConfig.loadFromJson(getConfigFromFile(configFilePath));
	saveToFile();
}

QJsonObject ConfigStore::getConfigFromFile(const QString & filePath)
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

void ConfigStore::saveToFile()
{
	QJsonObject obj = currentConfig.toJson();
	QJsonDocument doc(obj);

	QFile file(configFilePath);
	file.open(QFile::WriteOnly);
	file.write(doc.toJson());
}

void ConfigStore::setWlanPattern(const QString & pattern)
{
	currentConfig.wlanPattern = pattern;
	saveToFile();
}

void ConfigStore::setBrokerAddr(const QString & addr)
{
	currentConfig.brokerAddr = addr;
	saveToFile();
}
