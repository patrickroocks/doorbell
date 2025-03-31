#pragma once

#include <QtCore/QtCore>

#include "util.h"

class Command final : public QObject
{
	Q_OBJECT
public:
	enum Value
	{
		ping,
		buzz,
		ackRing,
		autoBuzzOn,
		autoBuzzOff,
		getAutoBuzz,
		getRawData,
		getStartTime,
		getActionLog,
		testRing,
		None = ping
	};
	Q_ENUM_HELPERS(Command)

	bool needsResponse() const;
	bool isMultiResponse() const;
};

class SpecialResponse final
{
public:
	static const QByteArray BuzzAck;
	static const QByteArray Pong;
	static const QByteArray EndMultiResponse;
	static const QByteArray AutoBuzzOn;
	static const QByteArray AutoBuzzOff;
};

class RingMessage final
{
public:
	static const QByteArray Ring;
	static const QByteArray TestRing;
	static const QByteArray AckRing;
	static const QByteArray AutoBuzzOn;
	static const QByteArray AutoBuzzOff;
};
