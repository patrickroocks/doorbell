# Doorbell client

## General

This is the PC client to get the bell rings and control the buzzer. The MQTT broker runs on the Arduino.

## Prerequisites

### MQTT

So build QtMqtt, run the following:

```
sudo apt install ninja-build
cd ~/tools/
git clone git://code.qt.io/qt/qtmqtt.git -b 6.8.1
cd qtmqtt
mkdir build && cd build
/opt/Qt/6.8.1/gcc_64/bin/qt-configure-module ..
/opt/Qt/Tools/CMake/bin/cmake --build .
sudo /opt/Qt/Tools/CMake/bin/cmake --install . --verbose
```

Taken from https://stackoverflow.com/questions/68928310/build-specific-modules-in-qt6-i-e-qtmqtt/71984521#71984521.


## Autostart Setup

```
nano ~/.config/autostart/doorbell-client.desktop
```

Contents:

```
[Desktop Entry]
Type=Application
Exec=/home/patrick/code/doorbell/client/build/Desktop_Qt_6_8_1-Release/doorbell-client
Hidden=false
NoDisplay=false
X-GNOME-Autostart-enabled=true
Name[en_US]=doorbell-client
Name=doorbell-client
Comment[en_US]=
Comment=
```

Test:

```
cp doorbell-client.desktop ~/.local/share/applications
gtk-launch doorbell-client.desktop
```