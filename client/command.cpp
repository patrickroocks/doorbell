#include "command.h"

const QByteArray SpecialResponse::BuzzAck = "buzzAck";
const QByteArray SpecialResponse::Pong = "pong";
const QByteArray SpecialResponse::EndMultiResponse = "endMultiResponse";
const QByteArray SpecialResponse::AutoBuzzOn = "autoBuzzOn";
const QByteArray SpecialResponse::AutoBuzzOff = "autoBuzzOff";

const QByteArray RingMessage::Ring = "ring";
const QByteArray RingMessage::TestRing = "testRing";
const QByteArray RingMessage::AckRing = "ackRing";
const QByteArray RingMessage::AutoBuzzOn = "autoBuzzOn";
const QByteArray RingMessage::AutoBuzzOff = "autoBuzzOff";

bool Command::needsResponse() const
{
	switch (value)
	{
	case ping:
	case buzz:
	case getRawData:
	case getStartTime:
	case getActionLog:
	case getAutoBuzz:
		return true;

	case ackRing:
	case autoBuzzOn:
	case autoBuzzOff:
	case testRing:
	default:
		return false;
	}
}

bool Command::isMultiResponse() const
{
	// only action log and raw data are multi-responses
	return value == getActionLog || value == getRawData;
}
