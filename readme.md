# Doorbell services

This repository contains the source code for a project to connect the doorbell (house intercom) with a PC client and a web UI. 

* A small documentation of the project is available at: https://www.p-roocks.de/doorbell
* The system sketch is given in [doc/system-sketch.md](doc/system-sketch.md).
* In [doc/mqtt-contract.md](doc/mqtt-contract.md) the MQTT protocol between the device and its controllers is specified.

It contains five projects:
* device-arduino
  * An Arduino project to get the ring signal from the house intercom und send the door buzzer command to the intercom.
  * See [device-arduino/readme.md](device-arduino/readme.md) for the build environment and the pinned library versions.
* client
  * A Qt project to connect to the Arduino device.
  * Displays a "door ring detected" popup, when a ring message is received via MQTT.
  * Offers a "open door" button to activate the door buzzer.
  * Allows to configure the connection and offers some diagnostics.
  * See [client/readme.md](client/readme.md) for details.
* server
  * A Rust HTTP server, which connects to the device via MQTT.
  * Offers a web UI to open the door, toggle the auto buzzer and view the device and server log.
  * Manages time-based auto-buzz rules, editable via the web UI or a token-protected REST API.
  * Optionally reachable from outside the home network via an ngrok tunnel.
  * See [server/readme.md](server/readme.md) for details.
* delivery2door
  * A Rust service, running next to the server, which watches mail folders (IMAP) for delivery announcements (DHL, Amazon).
  * Creates an auto-buzz rule in the server for the announced delivery window, via the server's rule API.
  * Offers its own web UI with a delivery overview and a service log.
  * See [delivery2door/readme.md](delivery2door/readme.md) for details.
* emulated-device
  * A Rust console application that emulates the Arduino device, for debugging and demonstrating the system without the real hardware.
  * Supports the same MQTT protocol as the device, via an external MQTT broker (e.g. Mosquitto).
  * Keys emulate the ring signal and the three switches; commands from the server or the client are answered automatically.
  * See [emulated-device/readme.md](emulated-device/readme.md) for details.

## Checkout
```
git clone https://github.com/patrickroocks/doorbell doorbell
```
