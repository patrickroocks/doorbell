# Doorbell broker and client

This repository contains the source code for a project to connect the doorbell (house intercom) with a PC client.

A small documentation of the project is available at: https://www.p-roocks.de/doorbell

It contains two projects:
* broker-arduino
  * An Arduino project to get the ring signal from the house intercom und send the door buzzer command to the intercom.
  * The Arduino also runs a MQTT broker to provide the ring signals.
* client
  * A Qt project to connect to the MQTT broker on the Arduino.
  * Displays a "door ring detected" popup, when a ring message is received via MQTT.
  * Offers a "open door" button to activate the door buzzer.
  * Allows to configure the connection and offers some diagnostics.
  * See [client/readme.md](client/readme.md) for details.

Related docs:
* In [doc/system-sketch.pdf](doc/system-sketch.pdf) the connections of the hardware are sketched.
* In [CHANGES.md](CHANGES.md) the history of the project is documented.


## Checkout

```
git clone https://github.com/patrickroocks/doorbell doorbell
```
