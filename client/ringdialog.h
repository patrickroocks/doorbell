#pragma once

#include <QDialog>
#include <QMenu>
#include <QProcess>
#include <QTimer>

namespace Ui {
class RingDialog;
}

class RingDialog : public QDialog
{
	Q_OBJECT

public:
	explicit RingDialog(QWidget * parent = nullptr);
	void closeEvent(QCloseEvent * event) override;
	~RingDialog();

	void incrementRingCount(bool testRing);
	void closeDialog();

signals:
	void doorBuzzer();
	void dialogClosed();

protected:
	void showEvent(QShowEvent * event) override;

private:
	void playWav();
	void placeTopRight();
	void doorBuzzerButtonClick();

	Ui::RingDialog * ui;
	QProcess wavPlayProcess;
	QTimer wavPlayTimer;

	int ringCount = 0;
	int testRingCount = 0;
};
