#include "util.h"

namespace util
{

QString boolToActiveStr(bool val) { return val ? "active" : "inactive"; }

QString getJsonStringWithDefault(const QJsonObject& obj, const QString& key, const QString& defaultValue)
{
	auto it = obj.find(key);
	return it != obj.end() ? it->toString() : defaultValue;
}

} // namespace util
