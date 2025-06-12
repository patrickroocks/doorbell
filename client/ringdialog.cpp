#include "ringdialog.h"
#include "ui_ringdialog.h"

#include "command.h"
#include "constants.h"
#include "util.h"

#include <QCloseEvent>
#include <QProcess>
#include <QScreen>
#include <QShowEvent>

#include "ringapp.h"

RingDialog::RingDialog(RingApp* app, RingListener* ringListener)
	: CommandClientDialog(ringListener)
	, ui(new Ui::RingDialog)
	, ringApp(app)
	, wavPlayProcess(this)
{
	ui->setupUi(this);

	// Ensure that window is always on top.
	// If this does not work, disable wayland! (cf. placeTopMid())
	setWindowFlags(windowFlags() | Qt::WindowStaysOnTopHint);

	// Do not allow to resize the dialog:
	this->setFixedSize(this->size());

	// Setup bell image
	util::setPictureInLabel(":/img/bell.png", ui->lblBell);

	// Setup connections
	connect(&wavPlayTimer, &QTimer::timeout, this, &RingDialog::playWav);
	connect(ui->cmdOkOrOpen, &QPushButton::clicked, this, &RingDialog::okOrOpen);
}

void RingDialog::incrementRingCount(bool testRing, const QByteArray& additionalInfo, bool autoBuzzOn)
{
	if (testRing) testRingCount++;
	else ringCount++;

	if (!testRing && !autoBuzzOn)
	{
		canOpen = true;
		ui->cmdOkOrOpen->setText("Open house door");
	}
	else
	{
		ui->cmdOkOrOpen->setText("Ok");
	}

	if (!additionalInfos.isEmpty())
		additionalInfos.append('\n');
	additionalInfos += additionalInfo;
	ui->txtAdditionalInfos->setPlainText(additionalInfos);

	const int totalRingCount = ringCount + testRingCount;
	if (!wavPlayTimer.isActive() && totalRingCount > 0)
	{
		playWav();
		wavPlayTimer.start(WavPlayIntervalMs);
	}

	if (testRingCount == 0) {
		if (ringCount == 1)
			ui->lblMessage->setText("Door ring detected!");
		else
			ui->lblMessage->setText(QString("Door rings detected (total: %1)").arg(ringCount));
	} else {
		ui->lblMessage->setText(QString("Door rings detected (normal: %1, test: %2)").arg(ringCount).arg(testRingCount));
	}

	show();
}

void RingDialog::closeEvent(QCloseEvent * event)
{
	closeDialog();
	event->ignore();
}

void RingDialog::closeDialog()
{
	sendCommand(Command::ackRing);
	testRingCount = 0;
	ringCount = 0;
	canOpen = false;
	additionalInfos.clear();
	wavPlayTimer.stop();

	this->hide();
	emit dialogClosed();
}

void RingDialog::playWav() { wavPlayProcess.start("aplay", {ringApp->getWavPath()}); }

void RingDialog::okOrOpen()
{
	if (canOpen)
	{
		ui->lblMessage->setText(QString("Waiting for door buzzer..."));
		waitForBuzzerAck = true;
		sendCommand(Command::buzz);
	}
	else
	{
		closeDialog();
	}
}

RingDialog::~RingDialog() { delete ui; }

void RingDialog::placeTopRight()
{
	// Disable Wayland, cf. readme.md
	QScreen * screen = QGuiApplication::primaryScreen();
	QRect screenGeometry = screen->geometry();
	const int x = screenGeometry.topRight().x() - this->width();
	const int y = screenGeometry.topRight().y();
	this->setGeometry(x, y, this->width(), this->height());
}

void RingDialog::showEvent(QShowEvent * event) { placeTopRight(); }

void RingDialog::onReceiveCommandResponse(const Command& cmd, const QByteArray& response)
{
	if (cmd == Command::buzz && response == SpecialResponse::BuzzAck)
	{
		waitForBuzzerAck = false;
		closeDialog();
	}
}
