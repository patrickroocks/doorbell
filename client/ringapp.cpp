#include "ringapp.h"

#include "configstore.h"
#include "constants.h"
#include "ringlistener.h"

#include <QApplication>
#include <QFile>

RingApp::RingApp(int argc, char* argv[])
	: app(argc, argv)
	, execDir(QCoreApplication::applicationDirPath() + "/")
{
	copyAlarmFile();
	QApplication::setQuitOnLastWindowClosed(false);

	configStore = QSharedPointer<ConfigStore>::create(this);
	listener = QSharedPointer<RingListener>::create(this);
}

QString RingApp::getWavPath() const { return execDir + TmpAlarmFile; }

void RingApp::copyAlarmFile()
{
	QFile resourceFile(":/audio/alarm.wav");
	if (resourceFile.open(QIODevice::ReadOnly)) {
		QFile outFile(getWavPath());
		if (outFile.open(QIODevice::WriteOnly)) {
			outFile.write(resourceFile.readAll());
			outFile.close();
		} else {
			qDebug() << "Failed to open output file:" << outFile.fileName();
		}
		resourceFile.close();
	}
}

int RingApp::run() { return app.exec(); }
