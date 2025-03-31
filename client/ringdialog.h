#pragma once

#include "commandclient.h"

#include <QMenu>
#include <QProcess>
#include <QTimer>

namespace Ui {
class RingDialog;
}

class RingListener;

class RingDialog : public CommandClientDialog
{
	Q_OBJECT

public:
	explicit RingDialog(RingListener* ringListener);
	void closeEvent(QCloseEvent * event) override;
	~RingDialog();

	void incrementRingCount(bool testRing, const QByteArray& additionalInfo, bool autoBuzzOn);
	void closeDialog();

signals:
	void dialogClosed();

protected:
	void showEvent(QShowEvent * event) override;

	virtual void onReceiveCommandResponse(const Command& cmd, const QByteArray& response) override;

private:
	void playWav();
	void placeTopRight();
	void okOrOpen();

	Ui::RingDialog * ui;
	QProcess wavPlayProcess;
	QTimer wavPlayTimer;
	QByteArray additionalInfos;

	bool canOpen = false;
	int ringCount = 0;
	int testRingCount = 0;
	bool waitForBuzzerAck = false;
};
