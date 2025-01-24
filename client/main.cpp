#include "ringlistener.h"

#include <QApplication>
#include <QFile>

#include "constants.h"

void copyAlarmFile()
{
	QString filePath = "alarm.wav";
	QFile resourceFile(":/audio/alarm.wav");
	if (resourceFile.open(QIODevice::ReadOnly)) {
		QFile outFile(TmpAlarmFile);
		if (outFile.open(QIODevice::WriteOnly)) {
			outFile.write(resourceFile.readAll());
			outFile.close();
		} else {
			qDebug() << "Failed to open output file:" << TmpAlarmFile;
		}
		resourceFile.close();
	}
}

int main(int argc, char * argv[])
{
	copyAlarmFile();

	QApplication a(argc, argv);
	QApplication::setQuitOnLastWindowClosed(false);

	RingListener rl{&a};
	return a.exec();
}
