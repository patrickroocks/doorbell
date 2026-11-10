# Doorbell device (ESP32)

Firmware for the device: reads the intercom's ring signal, drives the door buzzer and the
external bell relay, and speaks MQTT with its controllers. See
[../doc/system-sketch.md](../doc/system-sketch.md) for the wiring and
[../doc/mqtt-contract.md](../doc/mqtt-contract.md) for the protocol.

## Build environment

|       |                                                                  |
| ----- | ---------------------------------------------------------------- |
| Board | Arduino Nano ESP32 (ESP32-S3)                                    |
| FQBN  | `arduino:esp32:nano_nora`                                        |
| Core  | `arduino:esp32` **2.0.18-arduino.5** (ESP-IDF 4.4, mbedTLS 2.28) |

Copy [arduinoSecrets.cpp.example](arduinoSecrets.cpp.example) to `arduinoSecrets.cpp` and fill in
the WiFi and MQTT settings before building.

## WiFi connectivity

On start the device tries the WiFi networks in list order, beginning with the one stored in
the EEPROM. The WiFi found is stored, so the next start tries it first. Pressing all three
switches at once reboots the device and makes it start with the next WiFi in the list.

The device registers the hostname `doorbell` via DHCP and mDNS, so it is reachable as
`doorbell.local` without knowing its IP. This is meant for networks with changing IPs (e.g. a
mobile hotspot): there the device falls back to its local broker, and clients use `doorbell.local`
as broker address. On Windows, `.local` names resolve natively; on Linux, Avahi is needed.

## Library versions

These are the versions the firmware is built and tested against. Do not let the Library Manager update them blindly - see the pitfall below.

| Library             | Version         | Used for                                                                  |
| ------------------- | --------------- | ------------------------------------------------------------------------- |
| EmbeddedMqttBroker  | **2.0.12-qos0** | the MQTT broker running on the device itself <br> only used as a fallback |
| WrapperFreeRTOS     | 1.0.2           | dependency of EmbeddedMqttBroker                                          |
| Async TCP           | 3.5.0           | dependency of EmbeddedMqttBroker                                          |
| ESP Async WebServer | 3.12.0          | dependency of EmbeddedMqttBroker                                          |
| PubSubClient        | 2.8             | the MQTT client talking to the Raspi broker                               |
| NTP                 | 1.8.0           | time for the action log and the ring timestamps                           |

### Pitfall: EmbeddedMqttBroker must stay on 2.0.12

Version 2.1.x added TLS/mTLS/WSS transports written against mbedTLS 3.x and ESP-IDF 5.x. The core
above ships mbedTLS 2.28 / ESP-IDF 4.4. The device only ever runs a plain TCP broker on port 1883, so there is nothing to gain from 2.1.x.
