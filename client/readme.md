# Doorbell client/broker

Project description: https://www.p-roocks.de/doorbell/

Have a look at `doc/system-sketch.pdf` to get a technical overview over the project.

# Doorbell broker

The broker is an Arduino project in the subfolder `broker`. This reads the signals from the intercom and controls the relays. The MQTT broker runs also on the Arduino.

# Doorbell client

The client is a Qt6 project in the sub-folder `client`.

## General

This is the PC client to get the bell rings and control the buzzer. 

## Prerequisites

### MQTT

The doorbell client needs QtMqtt to connect to the broker. To build QtMqtt:
* Replace QtVersion in the following snippet (here 6.8.1).
* Run the following snippet:

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

To setup autostart for the doorbell broker within Ubuntu, do the following:

```
touch ~/.config/autostart/doorbell-client.desktop
nano ~/.config/autostart/doorbell-client.desktop
```

Fill in the following contents:

```
[Desktop Entry]
Type=Application
Exec=~/code/doorbell/client/build/Desktop_Qt_6_8_1-Release/doorbell-client
Hidden=false
NoDisplay=false
X-GNOME-Autostart-enabled=true
Name[en_US]=doorbell-client
Name=doorbell-client
Comment[en_US]=
Comment=
```

To test if it works, run:

```
cp doorbell-client.desktop ~/.local/share/applications
gtk-launch doorbell-client.desktop
```

## Disable Wayland for Ring Popup

Placing the ring popup top right on the screen and in the foreground only works if wayland is disabled.

* Set "WaylandEnable=false" in GNOME config:
* Run `sudo nano /etc/gdm3/custom.conf`
	* Set the following:
```
[daemon]
WaylandEnable=false
```
* reboot the system
