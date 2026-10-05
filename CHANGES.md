# Server version 0.3.0, 2026-10-05

- Initial published and documented version of the the server
- Web UI to open the door, toggle the auto buzzer, and view settings, diagnostics and the device log
- Connects to a list of MQTT brokers
- Rule management for auto-buzz rules
- Rule API for other services
- Server log in SQLite, accessible via the web UI
- `/serviceState` endpoint for monitoring without credentials
- Optional ngrok tunnel for access from outside the home network

# Broker version 0.3.0, 2026-10-05

- Configurable list of MQTT brokers, e.g. Mosquitto on the Raspi and the local broker on the ESP32
- LED 3 shows that a rule on the server is active to buzz on a ring (`ruleAutoBuzzOn`/`ruleAutoBuzzOff`)
- Outgoing MQTT messages are queued and sent from the main loop; long responses (action log, raw data) are spread over several cycles
- Reconnect to WiFi instead of rebooting

# Emulated device version 0.1.0, 2026-10-05

- Console application that emulates the device's MQTT protocol, for debugging without the real hardware

# Delivery2Door version 0.1.0, 2026-10-05

- Crawls mail for delivery announcements and creates auto-buzz rules in the relevant time span

# Client version 0.2.2, 2026-10-05

- Add MQTT client ID setting
- Fix that settings are applied correctly

# Client version 0.2.1, 2025-06-12

- Fix main loop occasionally getting stuck

# Arduino broker and QT client version 0.2.0, 2025-04-17

- Broker:
  - Persist auto buzzer state in EEPROM
  - Detect a stuck MQTT broker
  - Scheduler to switch only one relay at a time
  - Log for device start, ring, buzz, and auto buzzer state

- Client:
  - Config & diagnostics dialog
  - Show ring timestamps in the ring window

# Arduino broker and QT client version 0.1.0, 2025-01-04

- Initial version
- First working connection between intercom and PC
