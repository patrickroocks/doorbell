#pragma once

#include <QDialog>
#include <QtCore/QtCore>

class RingListener;
class Command;

// Interface for a class that can send commands and receive responses.
class CommandClientDialog : public QDialog
{
	Q_OBJECT
public:
	explicit CommandClientDialog(RingListener* ringListener);

protected slots:
	virtual void onReceiveCommandResponse(const Command& cmd, const QByteArray& response) {}

protected:
	void sendCommand(const Command& cmd);

private:
	RingListener* const ringListener;
};

