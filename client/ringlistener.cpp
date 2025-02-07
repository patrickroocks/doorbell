#include "ringlistener.h"

#include "ringdialog.h"

namespace {

const QString DoorbellBrokerAddr = "192.168.178.158";

const QMqttTopicName RingTopic{"doorRing"};
const QMqttTopicName CommandTopic{"cmd"};
const QMqttTopicName ResponseTopic{"response"};

const QByteArray MsgBuzz = "buzz";
const QByteArray MsgBuzzAck = "buzzAck";
const QByteArray MsgAckRing = "ackRing";
const QByteArray MsgGetAutoBuzz = "getAutoBuzz";
const QByteArray MsgRing = "ring";
const QByteArray MsgTestRing = "testRing";
const QByteArray MsgAutoBuzzOn = "autoBuzzOn";
const QByteArray MsgAutoBuzzOff = "autoBuzzOff";

const int ReconnectDelay = 5000;

} // namespace

RingListener::RingListener(QApplication * app)
	: app(app)
{
	connect(&client, &QMqttClient::connected, this, &RingListener::onMqttConnected);
	connect(&client, &QMqttClient::messageReceived, this, &RingListener::onMessageReceived);
	connect(&client, &QMqttClient::disconnected, this, &RingListener::onMqttDisconnected);
	connect(&reconnectTimer, &QTimer::timeout, this, &RingListener::mqttConnect);

	setupTray();
	setupDialog();
	mqttConnect();
}

void RingListener::setupDialog()
{
	rd.reset(new RingDialog);

	connect(rd.get(), &RingDialog::doorBuzzer, this, &RingListener::doorBuzzer);
	connect(rd.get(), &RingDialog::dialogClosed, this, &RingListener::dialogClosed);
}

void RingListener::setupTray()
{
	if (!QSystemTrayIcon::isSystemTrayAvailable()) {
		qWarning("System tray is not available on this system.");
		app->quit();
	}

	tray.quitAction.reset(new QAction("Quit", &tray.menu));
	connect(tray.quitAction.get(), &QAction::triggered, app, &QApplication::quit);
	tray.menu.addAction(tray.quitAction.get());
	tray.autoBuzzToggle.reset(new QAction("Auto Buzzer", &tray.menu));
	tray.autoBuzzToggle->setCheckable(true);
	connect(tray.autoBuzzToggle.get(), &QAction::triggered, [this]() {
		// Qt framework gives us the new state
		const bool newAutoBuzzState = tray.autoBuzzToggle->isChecked();
		// Restore old state, wait for MQTT response
		tray.setAutoBuzzChecked(!newAutoBuzzState);

		// Request new state
		if (newAutoBuzzState) client.publish(CommandTopic, MsgAutoBuzzOn);
		else client.publish(CommandTopic, MsgAutoBuzzOff);
	});
	tray.menu.addAction(tray.autoBuzzToggle.get());

	tray.iconOff = QIcon{":/img/bell-grey-off.png"};
	tray.iconDefault = QIcon{":/img/bell-grey.png"};
	tray.iconRing = QIcon{":/img/bell.png"};
	tray.iconAutoBuzz = QIcon{":/img/bell-autobuzz.png"};
	tray.sysTray.setIcon(tray.iconOff);
	tray.sysTray.setContextMenu(&tray.menu);
	tray.sysTray.show();
}

void RingListener::Tray::setAutoBuzzChecked(bool isChecked)
{
	autoBuzzToggle->blockSignals(true);
	autoBuzzToggle->setChecked(isChecked);
	autoBuzzToggle->blockSignals(false);
}

void RingListener::updateIcon()
{
	if (!mqttConnected) tray.sysTray.setIcon(tray.iconOff);
	else if (tray.autoBuzzToggle->isChecked()) tray.sysTray.setIcon(tray.iconAutoBuzz);
	else if (dialogOpen) tray.sysTray.setIcon(tray.iconRing);
	else tray.sysTray.setIcon(tray.iconDefault);
}

void RingListener::ringEvent(bool testRing)
{
	rd->incrementRingCount(testRing);
	dialogOpen = true;
	updateIcon();
}

void RingListener::onMessageReceived(const QByteArray & message, const QMqttTopicName & topic)
{
	if (topic.name() == RingTopic) {

		if (message == MsgRing || message == MsgTestRing) {
			if (message == "ring") ringEvent(false);
			else if (message == "testRing") ringEvent(true);
			waitForBuzzerAck = false;
		} else if (message == MsgAutoBuzzOn) {
			tray.setAutoBuzzChecked(true);
			updateIcon();
		} else if (message == MsgAutoBuzzOff) {
			tray.setAutoBuzzChecked(false);
			updateIcon();
		} else {
			qDebug() << "Unknown message received on ring topic:" << message;
		}

	} else if (topic.name() == ResponseTopic) {
		if (waitForBuzzerAck && message == MsgBuzzAck) {
			waitForBuzzerAck = false;
			rd->closeDialog();
		}
	}
}

void RingListener::mqttConnect()
{
	reconnectTimer.stop();
	qDebug() << "Connecting to MQTT broker...";
	client.setHostname(DoorbellBrokerAddr);
	client.setPort(1883);
	client.setClientId("DoorbellClient");
	client.setKeepAlive(5); // 5 seconds
	client.connectToHost();
}

void RingListener::onMqttConnected()
{
	qDebug() << "Connected to MQTT broker!";
	mqttConnected = true;
	updateIcon();

	auto subscriptions = {RingTopic, ResponseTopic};

	for (const auto & topic : subscriptions) {
		auto subscription = client.subscribe(QMqttTopicFilter{topic.name()});
		if (!subscription) {
			qDebug() << "Failed to subscribe to topic: " << topic.name().toStdString();
			return;
		}
	}

	client.publish(CommandTopic, MsgGetAutoBuzz);
}

void RingListener::onMqttDisconnected()
{
	qDebug() << "Disconnected from MQTT broker. Next reconnect in" << ReconnectDelay << "ms";
	mqttConnected = false;
	updateIcon();
	reconnectTimer.start(ReconnectDelay);
}

void RingListener::doorBuzzer()
{
	if (mqttConnected) {
		client.publish(CommandTopic, MsgBuzz);
		waitForBuzzerAck = true;
	} else {
		qDebug() << "MQTT not connected!";
	}
}

void RingListener::dialogClosed()
{
	if (mqttConnected) {
		client.publish(CommandTopic, MsgAckRing);
	} else {
		qDebug() << "MQTT not connected!";
	}
	dialogOpen = false;
	updateIcon();
}
