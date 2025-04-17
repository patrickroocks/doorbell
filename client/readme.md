# Doorbell client

This is the PC client to get the bell rings and control the buzzer. The client is a Qt6 project in the sub-folder `client`. Have a look at [../readme.md](../readme.md) for an overview of the entire project (broker & client).

## Build

The doorbell client needs Qt6, and we recommend Qt 6.8.3.

### Build MQTT dependency

The doorbell client needs QtMqtt to connect to the broker. To build QtMqtt:
* Replace QtVersion in the following snippet (here 6.8.3).
* Run the following snippet:

```
sudo apt install ninja-build
cd ~/tools/
git clone git://code.qt.io/qt/qtmqtt.git -b 6.8.3
cd qtmqtt
mkdir build && cd build
/opt/Qt/6.8.3/gcc_64/bin/qt-configure-module ..
/opt/Qt/Tools/CMake/bin/cmake --build .
sudo /opt/Qt/Tools/CMake/bin/cmake --install . --verbose
```

Taken from https://stackoverflow.com/questions/68928310/build-specific-modules-in-qt6-i-e-qtmqtt/71984521#71984521.

### Build the client

After the dependencies have been installed, build the doorbell client:

```
cd client
mkdir build && cd build
cmake .. -DCMAKE_PREFIX_PATH=/opt/Qt/6.8.3/gcc_64
make -j8
```

### Build and deploy

There is a script [client/build-and-deploy.sh](client/build-and-deploy.sh) doing the following steps:
* Clean the build
* Build
* Stop running doorbell-client
* Move the build file to `../client-release/doorbell-client`
* Start the new doorbell client

## System configuration

### Autostart setup

To setup autostart for the doorbell broker within Ubuntu, do the following:

```
touch ~/.config/autostart/doorbell-client.desktop
nano ~/.config/autostart/doorbell-client.desktop
```

Fill in the following contents:

```
[Desktop Entry]
Type=Application
Exec=~/code/doorbell/client-release/doorbell-client
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

### Disable Wayland for Ring Popup

Placing the ring popup top right on the screen and in the foreground only works if Wayland is disabled.

* Set "WaylandEnable=false" in GNOME config:
* Run `sudo nano /etc/gdm3/custom.conf`
	* Set the following:
```
[daemon]
WaylandEnable=false
```
* Reboot the system
