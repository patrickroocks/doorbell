#pragma once

#include <QtCore>
#include <qlabel.h>

namespace util
{

QString boolToActiveStr(bool val);

QString getJsonStringWithDefault(const QJsonObject& obj, const QString& key, const QString& defaultValue);

void setPictureInLabel(const QString& filePath, QLabel* label);

} // namespace util

// clang-format off
#define Q_ENUM_HELPERS(Cls) \
	public: \
		Q_ENUM(Value) \
		/* Constructors */ \
		Cls(Value v = Value::None): value(v) \
		{} \
		Cls(const Cls& other): value(other.value) \
		{} \
		/* Operators */ \
		Cls& operator=(const Cls& other) \
		{ \
			value = other.value; \
			return *this; \
		} \
		bool operator==(const Cls& other) const \
		{ \
			return value == other.value; \
		} \
		/* Converters */ \
		QString toString() const \
		{ \
			return QVariant::fromValue(value).toString(); \
		} \
		QByteArray toByteArray() const \
		{ \
			return QVariant::fromValue(value).toByteArray(); \
		} \
		Value get() const \
		{ \
			return value; \
		} \
	private: \
		Value value = Value::None; \
	public:
// clang-format on
