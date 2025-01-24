#pragma once

#include <QAction>
#include <QApplication>
#include <QMenu>
#include <QMqttClient>
#include <QObject>
#include <QSystemTrayIcon>
#include <QTimer>

class RingDialog;

class RingListener : public QObject
{
	Q_OBJECT
public:
	RingListener(QApplication * app);

	QMqttClient client;
	QSharedPointer<RingDialog> rd;

private:
	void setupTray();
	void setupDialog();
	void runTimer();
	void onMqttConnected();
	void onMqttDisconnected();
	void onMessageReceived(const QByteArray & message, const QMqttTopicName & topic);
	void mqttConnect();
	void doorBuzzer();
	void dialogClosed();
	void ringEvent(bool testRing);
	void updateIcon();

	bool mqttConnected = false;
	bool waitForBuzzerAck = false;
	bool dialogOpen = false;
	QTimer reconnectTimer;

	QApplication const * app;

	struct Tray
	{
		void setAutoBuzzChecked(bool isChecked);

		QIcon iconDefault;
		QIcon iconAutoBuzz;
		QIcon iconRing;
		QIcon iconOff;
		QMenu menu;
		QScopedPointer<QAction> quitAction;
		QScopedPointer<QAction> autoBuzzToggle;
		QSystemTrayIcon sysTray;
	} tray;
};
