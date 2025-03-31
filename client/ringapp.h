#pragma once

#include <QApplication>

class ConfigStore;
class RingListener;

class RingApp final
{
public:
	RingApp(int argc, char* argv[]);

	QApplication* getApplication() { return &app; }
	const QString& getExecDir() const { return execDir; }
	ConfigStore* getConfigStore() { return configStore.data(); }

	int run();

private:
	void copyAlarmFile();

	QApplication app;
	QSharedPointer<ConfigStore> configStore;
	QSharedPointer<RingListener> listener;

	const QString execDir;
};
