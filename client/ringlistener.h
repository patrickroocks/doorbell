#pragma once

#include "command.h"

#include <QAction>
#include <QApplication>
#include <QMenu>
#include <QMqttClient>
#include <QObject>
#include <QSystemTrayIcon>
#include <QTimer>

class ConfigDiagnosticsDialog;
class ConfigsSettingsDialog;
class ConfigStore;
class RingApp;
class RingDialog;

class RingListener final : public QObject
{
	Q_OBJECT
public:
	explicit RingListener(RingApp* app);

	// Interface to Settings&Diagnostics
	QString getConnStateStr() const;
	QString getAutoBuzzStateStr() const;
	bool getAutoBuzzState() const;
	void reloadSettings();

public slots:
	// Used by Command client interface.
	void sendCommand(const Command& cmd);
	void reconnect();

signals:
	void receiveCommandResponse(const Command& cmd, const QByteArray& response);
	void connStateChanged(const QString& state);
	void autoBuzzStateChanged(const QString& state);

private:
	void runMainLoop();
	void mainLoopHandleConnection(const QDateTime& now);
	void mainLoopHandleCommands(const QDateTime& now);

	void setupMqtt();
	void setupTray();
	void setupDialog();
	void setupMainLoop();

	enum class ResponseKind
	{
		Normal,
		Timeout,
		NotConnected
	};

	// Connection handling
	void onMqttConnected();
	void onMqttDisconnected();
	void onMessageReceived(const QByteArray& message, const QMqttTopicName& topic);
	void handleCommandResponse(const Command& cmd, ResponseKind responseKind, const QByteArray& response = {});
	void handleInternalStateByResponse(const Command& cmd, ResponseKind responseKind, const QByteArray& response);
	void mqttConnect();
	void establishConnectionToDevice();

	QString getWlanSsid() const;
	bool checkForWifi();

	void updateIcon();
	void setNewAutoBuzzState(bool newState);
	void setMqttDisconnected(const QString& reason);
	void updateConnState();

	QSharedPointer<RingDialog> ringDlg;
	QSharedPointer<ConfigDiagnosticsDialog> cfgDiagDlg;
	const QApplication* const app;
	ConfigStore* const cfgStore;

	struct MainLoop
	{
		QTimer timer;
		QDateTime tsLastRun;
	} mainLoop;

	struct CommandState
	{
		enum SendRecState
		{
			Idle,
			WaitForResponse
		};
		SendRecState sendRecState = SendRecState::Idle;
		Command cmdSent;
		QList<Command> queue;
		QDateTime tsCommandSent;
		QByteArray multiResponse;
	} cmdState;

	struct Connection
	{
		enum State
		{
			Disconnected,
			MqttConnected,
			DeviceConnected
		};
		bool hasMqttConn() const { return state == MqttConnected || state == DeviceConnected; }

		State state;
		QMqttClient mqtt;
		QString currentWifiPattern;
		QString currentBrokerAddr;
		QString connStateDetails;
		QDateTime tsLastActivity;
		QDateTime tsLastConnStateChange;
	} conn;

	struct Tray
	{
		void setAutoBuzzChecked(bool isChecked);

		QIcon iconDefault;
		QIcon iconAutoBuzz;
		QIcon iconRing;
		QIcon iconOff;
		QMenu menu;
		QScopedPointer<QAction> autoBuzzToggle;
		QScopedPointer<QAction> cfgDiagAction;
		QScopedPointer<QAction> quitAction;
		QSystemTrayIcon sysTray;
	} tray;
};
