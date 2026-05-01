#pragma once

#include "commandclient.h"

#include <QMenu>
#include <QProcess>
#include <QTimer>

namespace Ui {
class RingDialog;
}

class RingApp;
class RingListener;

class RingDialog final : public CommandClientDialog
{
	Q_OBJECT

public:
	explicit RingDialog(RingApp* app, RingListener* ringListener);
	~RingDialog();

	void incrementRingCount(bool testRing, const QByteArray& additionalInfo, bool autoBuzzOn);
	void closeDialog();

signals:
	void dialogClosed();

protected:
	void closeEvent(QCloseEvent* event) override;
	void showEvent(QShowEvent* event) override;

	virtual void onReceiveCommandResponse(const Command& cmd, const QByteArray& response) override;

private:
	void playWav();
	void placeTopRight();
	void okOrOpen();

	Ui::RingDialog* const ui;
	RingApp* const ringApp;
	QProcess wavPlayProcess;
	QTimer wavPlayTimer;
	QByteArray additionalInfos;
	bool allowedToOpenDoor = false;
	int ringCount = 0;
	int testRingCount = 0;
	bool waitForBuzzerAck = false;
};
