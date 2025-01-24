# Doorbell broker and client

This repository contains the source code and some documentation for a project to connect the doorbell with a PC client.

It contains two projects:
* broker-arduino
  * An Arduino project to get the ring signal from the house intercom und send the door buzzer command to the intercom.
  * The Arduino also runs a MQTT broker.
* client
  * A Qt project to connect to the MQTT broker on the Arduino.
  * Displays a "door ring detected", when a ring message is received via MQTT.
  * Offers a "open door" button to activate the door buzzer.
  * See `client/readme.md` for details.

In `doc/plan.odt` the connections of the hardware are sketched.