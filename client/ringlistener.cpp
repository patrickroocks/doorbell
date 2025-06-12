#include "ringlistener.h"

#include "configdiagnosticsdialog.h"
#include "configstore.h"
#include "constants.h"
#include "ringapp.h"
#include "ringdialog.h"
#include "util.h"

namespace {

const QMqttTopicName RingTopic{"doorRing"};
const QMqttTopicName CommandTopic{"cmd"};
const QMqttTopicName ResponseTopic{"response"};

const int MainLoopCycle = 200;
const int ReconnectDelay = 5000;
const int CommandTimeout = 5000;
const int ActivityRefresh = 20000;
const int MainLoopTimeout = 1000;

} // namespace

RingListener::RingListener(RingApp* ringApp)
	: ringApp(ringApp)
	, cfgStore(ringApp->getConfigStore())
{
	const auto now = QDateTime::currentDateTime();
	setupMqtt(now);
	setupTray();
	setupDialog();
	setupMainLoop(now);
}

void RingListener::setupMqtt(const QDateTime& now)
{
	const auto* mqttClient = &conn.mqtt;
	connect(mqttClient, &QMqttClient::connected, this, &RingListener::onMqttConnected);
	connect(mqttClient, &QMqttClient::messageReceived, this, &RingListener::onMessageReceived);
	connect(mqttClient, &QMqttClient::disconnected, this, &RingListener::onMqttDisconnected);

	conn.tsLastConnStateChange = now;
	conn.tsLastActivity = now;
	mqttConnect();
}

void RingListener::setupMainLoop(const QDateTime& now)
{
	// Use a timer and not SingleShot, as SingleShot seems not to be robust across restarts.
	connect(&mainLoop.timer, &QTimer::timeout, this, &RingListener::runMainLoop);

	mainLoop.tsLastRun = now;
	mainLoop.timer.setInterval(MainLoopCycle);
	mainLoop.timer.start();
}

void RingListener::runMainLoop()
{
	const auto now = QDateTime::currentDateTime();
	mainLoopHandleConnection(now);
	mainLoopHandleCommands(now);
}

void RingListener::mainLoopHandleConnection(const QDateTime& now)
{
	if (conn.state == Connection::Disconnected)
	{
		if (conn.tsLastActivity.msecsTo(now) > ReconnectDelay)
			mqttConnect();
	}
	else if (conn.state == Connection::MqttConnected)
	{
		if (conn.tsLastActivity.msecsTo(now) > ReconnectDelay)
			establishConnectionToDevice();
	}
	else if (conn.state == Connection::DeviceConnected && conn.tsLastActivity.msecsTo(now) > ActivityRefresh)
	{
		conn.tsLastActivity = now;
		sendCommand(Command::ping);
	}

	if (conn.state == Connection::DeviceConnected && mainLoop.tsLastRun.msecsTo(now) > MainLoopTimeout)
	{
		// This system was probably in suspend, the auto buzz state is not known anymore
		sendCommand(Command::getAutoBuzz);
	}

	mainLoop.tsLastRun = now;
}

void RingListener::mainLoopHandleCommands(const QDateTime& now)
{
	if (cmdState.sendRecState == CommandState::Idle && !cmdState.queue.isEmpty())
	{
		cmdState.cmdSent = cmdState.queue.takeFirst();
		if (cmdState.cmdSent.needsResponse())
		{
			cmdState.sendRecState = CommandState::WaitForResponse;
			cmdState.multiResponse.clear();
		}
		cmdState.tsCommandSent = now;
		// Require device connection. Only for "ping" we just require the MQTT connection, this command is used to establish the connection.
		if (conn.state == Connection::DeviceConnected || (conn.hasMqttConn() && cmdState.cmdSent == Command::ping))
		{
			conn.mqtt.publish(CommandTopic, cmdState.cmdSent.toByteArray());
		}
		else
		{
			handleCommandResponse(cmdState.cmdSent, ResponseKind::NotConnected);
		}
	}
	else if (cmdState.sendRecState == CommandState::WaitForResponse && cmdState.tsCommandSent.msecsTo(now) >= CommandTimeout)
	{
		handleCommandResponse(cmdState.cmdSent, ResponseKind::Timeout);
	}
}

void RingListener::setupDialog()
{
	ringDlg = QSharedPointer<RingDialog>::create(ringApp, this);
	connect(ringDlg.get(), &RingDialog::dialogClosed, this, &RingListener::updateIcon);

	cfgDiagDlg = QSharedPointer<ConfigDiagnosticsDialog>::create(this, cfgStore);
}

void RingListener::setupTray()
{
	if (!QSystemTrayIcon::isSystemTrayAvailable())
	{
		qWarning("System tray is not available on this system.");
		ringApp->getApplication()->quit();
	}

	// Auto Buzz toggle
	tray.autoBuzzToggle.reset(new QAction("Auto Buzzer", &tray.menu));
	tray.autoBuzzToggle->setCheckable(true);
	connect(tray.autoBuzzToggle.get(),
			&QAction::triggered,
			[this]()
			{
				// Qt framework gives us the new state
				const bool newAutoBuzzState = tray.autoBuzzToggle->isChecked();
				// Restore old state, wait for MQTT response
				tray.setAutoBuzzChecked(!newAutoBuzzState);

				// Request new state
				if (newAutoBuzzState)
					sendCommand(Command::autoBuzzOn);
				else
					sendCommand(Command::autoBuzzOff);
			});
	tray.menu.addAction(tray.autoBuzzToggle.get());

	// Config and Diagnostics Dialog
	tray.cfgDiagAction.reset(new QAction("Config && Diagnostics", &tray.menu));
	connect(tray.cfgDiagAction.get(),
			&QAction::triggered,
			[this]()
			{
				cfgDiagDlg->show();
				cfgDiagDlg->updateSelectedCategory();
			});
	tray.menu.addAction(tray.cfgDiagAction.get());

	// Quit
	tray.quitAction.reset(new QAction("Quit", &tray.menu));
	connect(tray.quitAction.get(), &QAction::triggered, ringApp->getApplication(), &QApplication::quit);
	tray.menu.addAction(tray.quitAction.get());

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
	if (conn.state != Connection::DeviceConnected)
	{
		tray.sysTray.setIcon(tray.iconOff);
		tray.sysTray.setToolTip(conn.connStateDetails);
	}
	else if (tray.autoBuzzToggle->isChecked())
	{
		tray.sysTray.setIcon(tray.iconAutoBuzz);
		tray.sysTray.setToolTip("Auto Buzzer enabled");
	}
	else if (ringDlg->isVisible())
	{
		tray.sysTray.setIcon(tray.iconRing);
		tray.sysTray.setToolTip("Doorbell ring");
	}
	else
	{
		tray.sysTray.setIcon(tray.iconDefault);
		tray.sysTray.setToolTip(conn.connStateDetails);
	}
}

void RingListener::setNewAutoBuzzState(bool newState)
{
	tray.setAutoBuzzChecked(newState);
	updateIcon();
	emit autoBuzzStateChanged(getAutoBuzzStateStr());
}

void RingListener::onMessageReceived(const QByteArray& message, const QMqttTopicName& topic)
{
	const auto raiseRing = [this](bool testRing, const QByteArray& additionalInfo)
	{
		QByteArray fullAdditionalInfo = testRing ? "Test Ring" : "Ring";
		if (!additionalInfo.isEmpty())
			fullAdditionalInfo += " (" + additionalInfo + ")";
		ringDlg->incrementRingCount(testRing, fullAdditionalInfo, getAutoBuzzState());
		conn.tsLastActivity = QDateTime::currentDateTime();
		updateIcon();
	};

	const auto updateAutoBuzz = [this](bool autoBuzz)
	{
		setNewAutoBuzzState(autoBuzz);
		conn.tsLastActivity = QDateTime::currentDateTime();
	};

	if (topic.name() == RingTopic)
	{
		if (message.startsWith(RingMessage::Ring))
		{
			raiseRing(false, message.mid(RingMessage::Ring.size() + 1));
		}
		else if (message.startsWith(RingMessage::TestRing))
		{
			raiseRing(true, message.mid(RingMessage::TestRing.size() + 1));
		}
		else if (message == RingMessage::AutoBuzzOn)
		{
			updateAutoBuzz(true);
		}
		else if (message == RingMessage::AutoBuzzOff)
		{
			updateAutoBuzz(false);
		}
		else if (message == RingMessage::AckRing)
		{
			if (ringDlg->isVisible())
				ringDlg->closeDialog();
		}
		else
		{
			qDebug() << "Unknown message received on ring topic:" << message;
		}
	}
	else if (topic.name() == ResponseTopic)
	{
		handleCommandResponse(cmdState.cmdSent, ResponseKind::Normal, message);
	}
}

QString RingListener::getWlanSsid() const
{
	QProcess process;
	process.start("iwgetid", {"-r"});
	process.waitForFinished(1000);
	return process.readAllStandardOutput().trimmed();
}

void RingListener::setMqttDisconnected(const QString& reason)
{
	if (conn.hasMqttConn())
	{
		conn.tsLastConnStateChange = QDateTime::currentDateTime();
	}
	conn.state = Connection::Disconnected;
	conn.connStateDetails = reason;
	updateConnState();
}

bool RingListener::checkForWifi()
{
	const auto wlanSsid = getWlanSsid();

	const auto setWifiDisconnected = [this](const QString& reason)
	{
		const QString& additionalReason = ", last check at: " + QDateTime::currentDateTime().toString(DateFormat);
		setMqttDisconnected(reason + additionalReason);
	};

	if (wlanSsid.isEmpty())
	{
		setWifiDisconnected("No WiFi connection found!");
		return false;
	}

	conn.currentWifiPattern = cfgStore->getCurrentConfig().wlanPattern;
	QRegularExpression ssidPattern("^" + conn.currentWifiPattern + "$");
	if (!ssidPattern.match(wlanSsid).hasMatch())
	{
		setWifiDisconnected("WiFi SSID (\"" + wlanSsid + "\") does not match pattern (\"" + conn.currentWifiPattern + "\")");
		return false;
	}

	return true;
}

void RingListener::updateConnState()
{
	emit connStateChanged(getConnStateStr());
	updateIcon();
	qDebug() << "Connection state changed:" << getConnStateStr();
}

void RingListener::reconnect()
{
	if (conn.hasMqttConn())
		conn.mqtt.disconnectFromHost();
	mqttConnect();
}

void RingListener::mqttConnect()
{
	conn.connStateDetails = "Connecting to MQTT broker...";
	conn.state = Connection::Disconnected;
	updateConnState();

	if (!checkForWifi())
		return;

	conn.currentBrokerAddr = cfgStore->getCurrentConfig().brokerAddr;
	conn.tsLastActivity = QDateTime::currentDateTime();
	conn.mqtt.setHostname(conn.currentBrokerAddr);
	conn.mqtt.setPort(1883);
	conn.mqtt.setClientId("DoorbellClient");
	conn.mqtt.connectToHost();
}

void RingListener::onMqttConnected()
{
	qDebug() << "Connected to MQTT broker";

	conn.state = Connection::MqttConnected;
	conn.connStateDetails = "Only connected to MQTT broker, not to the device";
	updateConnState();

	auto subscriptions = {RingTopic, ResponseTopic};

	for (const auto& topic : subscriptions)
	{
		auto subscription = conn.mqtt.subscribe(QMqttTopicFilter{topic.name()});
		if (!subscription)
		{
			qDebug() << "Failed to subscribe to topic: " << topic.name().toStdString();
			return;
		}
	}

	establishConnectionToDevice();
}

void RingListener::establishConnectionToDevice()
{
	conn.tsLastActivity = QDateTime::currentDateTime();
	sendCommand(Command::ping);
}

void RingListener::onMqttDisconnected()
{
	qDebug() << "Disconnected from MQTT broker";
	setMqttDisconnected(QString("No connection to broker (") + conn.currentBrokerAddr + ")");
}

QString RingListener::getConnStateStr() const
{
	QString rv = (conn.state == Connection::DeviceConnected ? "Connected to" : "Disconnected from") + QString{" device since: "}
				 + conn.tsLastConnStateChange.toString(DateFormat) + ".";

	if (!conn.connStateDetails.isEmpty())
		rv += "\nDetails: " + conn.connStateDetails;

	return rv;
}

QString RingListener::getAutoBuzzStateStr() const
{
	if (conn.state != Connection::DeviceConnected)
	{
		return "Unknown (not connected)";
	}
	else
	{
		return util::boolToActiveStr(tray.autoBuzzToggle->isChecked());
	}
}

bool RingListener::getAutoBuzzState() const { return tray.autoBuzzToggle->isChecked(); }

void RingListener::sendCommand(const Command& command) { cmdState.queue.append(command); }

void RingListener::handleCommandResponse(const Command& cmd, ResponseKind responseKind, const QByteArray& response)
{
	handleInternalStateByResponse(cmd, responseKind, response);

	// Handle non-timeout multi-responses.
	if (cmd.isMultiResponse() && responseKind == ResponseKind::Normal)
	{
		if (response == SpecialResponse::EndMultiResponse)
		{
			emit receiveCommandResponse(cmd, cmdState.multiResponse);
			cmdState.multiResponse.clear();
			cmdState.sendRecState = CommandState::Idle;
		}
		else
		{
			// Avoid running into timeout
			cmdState.tsCommandSent = QDateTime::currentDateTime();
			if (!cmdState.multiResponse.isEmpty())
				cmdState.multiResponse += "\n";

			cmdState.multiResponse += response;
		}
		return;
	}

	cmdState.sendRecState = CommandState::Idle;

	// Forward to other listeners.
	switch (responseKind)
	{
	case ResponseKind::Normal:
		emit receiveCommandResponse(cmd, response);
		break;
	case ResponseKind::Timeout:
		emit receiveCommandResponse(cmd, "[Timeout while sending command '" + cmd.toByteArray() + "']");
		break;
	case ResponseKind::NotConnected:
		emit receiveCommandResponse(cmd, "[Not connected]");
		break;
	}
}

void RingListener::handleInternalStateByResponse(const Command& cmd, ResponseKind responseKind, const QByteArray& response)
{
	const auto now = QDateTime::currentDateTime();

	if (responseKind == ResponseKind::Normal)
	{
		conn.tsLastActivity = now;

		// Check for auto buzzer response
		if (cmd == Command::getAutoBuzz)
		{
			if (response == SpecialResponse::AutoBuzzOn)
				setNewAutoBuzzState(true);
			else if (response == SpecialResponse::AutoBuzzOff)
				setNewAutoBuzzState(false);
		}

		// Check if a connection is established (ping -> pong).
		if (cmd == Command::ping && response == SpecialResponse::Pong)
		{
			const bool establishedConnectionNow = (conn.state == Connection::MqttConnected);

			if (establishedConnectionNow)
			{
				conn.state = Connection::DeviceConnected;
				emit autoBuzzStateChanged(getAutoBuzzStateStr());
				conn.tsLastConnStateChange = now;
				conn.connStateDetails = "";
				sendCommand(Command::getAutoBuzz);
				updateConnState();
			}
		}
	}

	// Check if the connection to the device is lost (timeout for ANY command or unexpected response to ping).
	const bool timeout = responseKind == ResponseKind::Timeout;
	const bool pingWithoutPong = (cmd == Command::ping && response != SpecialResponse::Pong);

	if (timeout || pingWithoutPong)
	{
		if (timeout)
			conn.connStateDetails = "Connected to MQTT broker, device not responding";
		else
			conn.connStateDetails = "Connected to MQTT broker, unexpected response from device: \"" + response + "\"";

		conn.connStateDetails += ", last try at: " + cmdState.tsCommandSent.toString(DateFormat);
		updateConnState();

		if (conn.state == Connection::DeviceConnected)
		{
			conn.state = Connection::MqttConnected;
			conn.tsLastConnStateChange = QDateTime::currentDateTime();
			emit autoBuzzStateChanged(getAutoBuzzStateStr());
			updateConnState();
		}
	}
}

void RingListener::reloadSettings()
{
	const auto newConfig = cfgStore->getCurrentConfig();

	if (newConfig.brokerAddr != conn.currentBrokerAddr || newConfig.wlanPattern != conn.currentWifiPattern)
	{
		conn.mqtt.disconnectFromHost();
		mqttConnect();
	}
}
