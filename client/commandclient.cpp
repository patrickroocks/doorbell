#include "commandclient.h"

#include "ringlistener.h"

CommandClientDialog::CommandClientDialog(RingListener* ringListener)
	: QDialog(nullptr)
	, ringListener(ringListener)
{
	connect(ringListener, &RingListener::receiveCommandResponse, this, &CommandClientDialog::onReceiveCommandResponse);
}

void CommandClientDialog::sendCommand(const Command& cmd) { ringListener->sendCommand(cmd); }
