# Changelog

### Device version 0.3.1, 2026-11-10

- Reboot via three buttons tries next WiFi in list on next start

### Server version 0.3.1, 2026-11-10

- Dashboard on start page (rule schedule, last ring)
- Improved creation of rules (short-cut, implicit delete-at-end flag)
- Reboot on connect only if enabled in the MQTT server config

### Server version 0.3.0, 2026-10-18

- Initial published and documented version of the the server
- Web UI to open the door, toggle the auto buzzer, and view settings, diagnostics and the device log
- Connects to a list of MQTT brokers
- Rule management for auto-buzz rules
- Rule API for other services like Delivery2Door
- Server log in SQLite, accessible via the web UI
- `/serviceState` endpoint for monitoring without credentials
- Optional ngrok tunnel for access from outside the home network

### Device version 0.3.0, 2026-10-18

- Configurable list of MQTT brokers, e.g. Mosquitto on the Raspi and the local broker on the ESP32
- LED 3 shows that a rule on the server is active to buzz on a ring
- Outgoing MQTT messages are queued and sent from the main loop; long responses (action log, raw data) are spread over several cycles
- Reconnect to WiFi instead of rebooting
- Show firmware version in device log

### Emulated device version 0.1.0, 2026-10-18

- Console application that emulates the device's MQTT protocol, for debugging without the real hardware

### Delivery2Door version 0.1.0, 2026-10-18

- Crawls mail for delivery announcements and creates auto-buzz rules in the relevant time span
- This service is entirely vibe-coded

### Qt Client version 0.2.2, 2026-10-18

- Add MQTT client ID setting
- Fix that settings are applied correctly

### Client version 0.2.1, 2026-06-12

- Fix main loop occasionally getting stuck

### Arduino device version 0.2.0, 2025-04-17

- Persist auto buzzer state in EEPROM
- Detect a stuck MQTT broker
- Scheduler to switch only one relay at a time
- Log for device start, ring, buzz, and auto buzzer state

### Qt client version 0.2.0, 2025-01-04

- Config & diagnostics dialog
- Show ring timestamps in the ring window

### Arduino device and Qt client version 0.1.0, 2025-01-04

- Initial version
- First working connection between intercom and PC
