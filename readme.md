# Doorbell services

This repository contains the source code and hardware documentation for connecting a doorbell (house intercom) to doorbell-related services via an Arduino device. At its core is a Rust server that manages auto-buzz rules. Around it are a PC client (C++/Qt) that shows door rings, a web UI for diagnostics and configuration, and an emulated device for testing without hardware. The "delivery2door" service creates auto-buzz rules from delivery notification emails.

* Project documentation is available at [p-roocks.de/doorbell](https://www.p-roocks.de/doorbell).
* The system sketch is given in [doc/system-sketch.md](doc/system-sketch.md).
* In [doc/mqtt-contract.md](doc/mqtt-contract.md) the MQTT protocol between the device and its controllers is specified.

The repository contains five projects:

* `device-arduino`
  * An Arduino project to detect the ring signal from the house intercom and send the door buzzer command to the intercom.
  * See [device-arduino/readme.md](device-arduino/readme.md) for the build environment and the pinned library versions.
* `client`
  * A Qt project to connect to the Arduino device.
  * Displays a "door ring detected" popup when a ring message is received via MQTT.
  * Offers an "Open door" button to activate the door buzzer.
  * Lets you configure the connection and offers some diagnostics.
  * See [client/readme.md](client/readme.md) for details.
* `server`
  * A Rust HTTP server, which connects to the device via MQTT.
  * Offers a web UI to open the door, toggle auto-buzz and view the device and server log.
  * Manages time-based auto-buzz rules, editable via the web UI or a token-protected REST API.
  * Optionally reachable from outside the home network via an ngrok tunnel.
  * See [server/readme.md](server/readme.md) for details.
* `delivery2door`
  * A Rust service that runs next to the server and watches IMAP mail folders for delivery notifications (DHL, Amazon).
  * Creates an auto-buzz rule in the server for the announced delivery window via the server's rule API.
  * Offers its own web UI with a delivery overview and a service log.
  * See [delivery2door/readme.md](delivery2door/readme.md) for details.
* `emulated-device`
  * A Rust console application that emulates the Arduino device, for debugging and demonstrating the system without the real hardware.
  * Supports the same MQTT protocol as the device, via an external MQTT broker (e.g. Mosquitto).
  * Keyboard keys emulate the ring signal and the three switches; commands from the server or the client are answered automatically.
  * See [emulated-device/readme.md](emulated-device/readme.md) for details.

## Checkout
```
git clone https://github.com/patrickroocks/doorbell
```
