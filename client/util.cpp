#include "util.h"

#include <QPixmap>

namespace util
{

QString boolToActiveStr(bool val) { return val ? "active" : "inactive"; }

QString getJsonStringWithDefault(const QJsonObject& obj, const QString& key, const QString& defaultValue)
{
	auto it = obj.find(key);
	return it != obj.end() ? it->toString() : defaultValue;
}

void setPictureInLabel(const QString& filePath, QLabel* label)
{
	QPixmap pixmap(filePath);
	label->setPixmap(pixmap);
	label->setScaledContents(true);
	label->setGeometry(label->x(), label->y(), pixmap.width(), pixmap.height());
}

} // namespace util
