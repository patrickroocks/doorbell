#include "ringdialog.h"
#include "ui_ringdialog.h"

#include <QCloseEvent>
#include <QPixmap>
#include <QProcess>
#include <QScreen>
#include <QShowEvent>

#include "constants.h"

RingDialog::RingDialog(QWidget * parent)
	: QDialog(parent)
	, ui(new Ui::RingDialog)
	, wavPlayProcess(this)
{
	ui->setupUi(this);

	// Ensure that window is always on top.
	// If this does not work, disable wayland! (cf. placeTopMid())
	setWindowFlags(windowFlags() | Qt::WindowStaysOnTopHint);

	// Setup bell image
	QPixmap pixmap(":/img/bell.png");
	ui->lblBell->setPixmap(pixmap);
	ui->lblBell->setScaledContents(true);
	ui->lblBell->setGeometry(ui->lblBell->x(), ui->lblBell->y(), pixmap.width(), pixmap.height());

	// Setup connections
	connect(&wavPlayTimer, &QTimer::timeout, this, &RingDialog::playWav);
	connect(ui->cmdOpenDoor, &QPushButton::clicked, this, &RingDialog::doorBuzzerButtonClick);
}

void RingDialog::incrementRingCount(bool testRing)
{
	if (testRing) testRingCount++;
	else ringCount++;

	if (!wavPlayTimer.isActive() && ringCount > 0) {
		playWav();
		wavPlayTimer.start(WavPlayIntervalMs);
	}

	if (testRingCount == 0) {
		if (ringCount == 1) ui->lblMessage->setText("Door ring detected!");
		else ui->lblMessage->setText(QString("Door rings detected (total: %1)").arg(ringCount));
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
	testRingCount = 0;
	ringCount = 0;
	wavPlayTimer.stop();

	this->hide();

	emit dialogClosed();
}

void RingDialog::playWav() { wavPlayProcess.start("aplay", {TmpAlarmFile}); }

void RingDialog::doorBuzzerButtonClick()
{
	ui->lblMessage->setText(QString("Waiting for door buzzer..."));
	emit doorBuzzer();
}

RingDialog::~RingDialog() { delete ui; }

void RingDialog::placeTopRight()
{
	/* If placement of the window and keeping it in foreground does not work, disable wayland!
	
	* Set "WaylandEnable=false" in GNOME config:
		 sudo nano /etc/gdm3/custom.conf
		```
		[daemon]
		WaylandEnable=false
		```
	* reboot system
	*/

	QScreen * screen = QGuiApplication::primaryScreen();
	QRect screenGeometry = screen->geometry();
	int x = screenGeometry.topRight().x() - this->width();
	int y = screenGeometry.topRight().y();
	this->setGeometry(x, y, this->width(), this->height());
}

void RingDialog::showEvent(QShowEvent * event) { placeTopRight(); }
