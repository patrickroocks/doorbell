# Doorbell MQTT Contract

This is the single source of truth for the MQTT protocol used between the doorbell
device and its controllers. Four independent codebases implement this protocol and must
stay in sync with this document:

| Role                                        | Project                        | Protocol implementation                                                     |
| ------------------------------------------- | ------------------------------ | --------------------------------------------------------------------------- |
| Device (publishes events, answers commands) | `device-arduino/` (ESP32, C++) | [`mqttHandler.cpp`](../device-arduino/mqttHandler.cpp)                      |
| Doorbell server                             | `server/` (Rust HTTP server)   | [`src/mqtt/mqttProtocol.rs`](../server/src/mqtt/mqttProtocol.rs)            |
| Qt desktop client                           | `client/` (Qt/C++)             | [`command.cpp`](../client/command.cpp) / [`command.h`](../client/command.h) |
| Emulated device                             | `emulated-device/` (Rust)      | [`src/mqttProtocol.rs`](../emulated-device/src/mqttProtocol.rs)             |


## Overview

```
        ┌──────────────────┐         cmd  ─────────────►          ┌─────────────┐
        │   Controllers    │                                      │   Device    │
        │  ───────────     │  ◄───── doorRing / response ─────    │ (ESP32 +    │
        │  Doorbell server │                                      │  fallback   │
        │  Qt client       │         (via MQTT broker)            │  broker)    │
        └──────────────────┘                                      └─────────────┘
```

- The MQTT broker runs on the Raspberry Pi, while the Arduino has just a fallback broker, if the Raspi is unavailable.
- The device is considered "connected" by a controller only after a `ping` → `pong`
  round-trip (see [Connection lifecycle](#connection-lifecycle)).

## Topics

| Topic      | Direction            | Purpose                                    |
| ---------- | -------------------- | ------------------------------------------ |
| `cmd`      | controller → device  | Commands sent to the device.               |
| `doorRing` | device → controllers | Ring events and auto-buzz state changes.   |
| `response` | device → controllers | Replies to commands that expect an answer. |

## Command payloads (controller → device)

All payloads are exact, case-sensitive ASCII strings.

| Payload           | Meaning                                           | Expected reply                                 |
| ----------------- | ------------------------------------------------- | ---------------------------------------------- |
| `ping`            | Liveness probe.                                   | `pong` on `response`                           |
| `buzz`            | Activate the door buzzer (open the door).         | `buzzAck` on `response`                        |
| `autoBuzzOn`      | Enable auto-buzz.                                 | `autoBuzzOn` on `doorRing` (if state changed)  |
| `autoBuzzOff`     | Disable auto-buzz.                                | `autoBuzzOff` on `doorRing` (if state changed) |
| `getAutoBuzz`     | Query current auto-buzz state.                    | `autoBuzz(On/Off)` on `response`               |
| `ruleAutoBuzzOn`  | A rule on the server is active to buzz on a ring. | none                                           |
| `ruleAutoBuzzOff` | No rule on the server is active.                  | none                                           |
| `testRing`        | Simulate a ring (no real doorbell press needed).  | `testRing <timestamp>` on `doorRing`           |
| `ackRing`         | Acknowledge the active ring.                      | `ackRing` on `doorRing`                        |
| `getActionLog`    | Fetch the device action log.                      | multi-response on `response`                   |
| `getRawData`      | Fetch raw samples of the bell input.              | multi-response on `response`                   |
| `getStartTime`    | Query device start timestamp.                     | timestamp string on `response`                 |
| `reboot`          | Reboot the device.                                | `ackReboot` on `response`                      |

### Device vs. rule auto-buzz

* `autoBuzzOn`/`autoBuzzOff` control the device auto-buzz: the device opens the door by itself on every ring, the state survives a reboot (EEPROM) and it can also be toggled with switch 1 on the device.
* `ruleAutoBuzzOn`/`ruleAutoBuzzOff` only report that a rule on the server is active, so that the device can show it on its rule LED. The device keeps this state in RAM only. When such a rule fires, the server sends a plain `buzz`. 

## `doorRing` payloads (device → controllers)

| Payload                       | Meaning                                                                               |
| ----------------------------- | ------------------------------------------------------------------------------------- |
| `ring <timestamp>`            | Real doorbell press while auto-buzz is off.                                           |
| `ring auto buzz, <timestamp>` | Real doorbell press while auto-buzz is on (the device also buzzes automatically).     |
| `testRing <timestamp>`        | Simulated ring (never carries the `auto buzz,` prefix, regardless of state).          |
| `ackRing`                     | The active ring was acknowledged (by a switch on the device or an `ackRing` command). |
| `deviceBuzz`                  | The physical "ack & buzz" button on the device triggered the door buzzer.             |
| `autoBuzzOn`                  | Auto-buzz was just turned on (device switch 1 or an `autoBuzzOn` command).            |
| `autoBuzzOff`                 | Auto-buzz was just turned off.                                                        |

`<timestamp>` is `YYYY-MM-DD HH:MM:SS`.

## `response` payloads (device → controllers)

| Payload                           | Meaning                                                                                         |
| --------------------------------- | ----------------------------------------------------------------------------------------------- |
| `pong`                            | Reply to `ping`.                                                                                |
| `buzzAck`                         | Reply to `buzz`.                                                                                |
| `autoBuzzOn` / `autoBuzzOff`      | Reply to `getAutoBuzz`.                                                                         |
| `<timestamp string>`              | Reply to `getStartTime`.                                                                        |
| `ackReboot`                       | Reply to `reboot`.                                                                              |
| `<log line>` … `endMultiResponse` | Reply to `getActionLog` / `getRawData`: one line per message, terminated by `endMultiResponse`. |

## Payload conventions

* Plain ASCII strings. Commands are matched by exact string equality; `doorRing` ring events carry a trailing ` <timestamp>`.
* QoS 0, no retained messages.
* Multi-response messages (e.g. action log) are terminated by `endMultiResponse`.

## Connection lifecycle

1. A controller connects to the broker and subscribes to `doorRing` + `response`.
2. It sends `ping`. Until a `pong` arrives, the controller is only "MQTT connected", not
   "device connected".
3. On `pong`, the controller marks the device connected and immediately requests
   `getAutoBuzz` (to sync the toggle).
4. The Rust server additionally requests `getStartTime` and re-sends the current `ruleAutoBuzzOn`/`ruleAutoBuzzOff` state, which the device does not persist.
5. While connected, controllers re-`ping` to keep track of the connection status.
6. Reboot on foreign broker (Rust server only): if the server finds the device connected through a broker whose host is not `localhost`, it issues `reboot` to push the device back onto the expected broker.

## Example communication

Notation: `topic <= payload` is published by a controller; `topic => payload` is published
by the device.

### Connect handshake
```
cmd      <= ping
response => pong                       # device is now "connected"
cmd      <= getAutoBuzz
response => autoBuzzOff
cmd      <= getStartTime               # Rust server only
response => 2026-06-04 09:12:03
cmd      <= ruleAutoBuzzOff            # Rust server only, no reply; re-sent on every connect
```

### Ring with auto-buzz OFF, then acknowledge
```
doorRing => ring 2026-06-04 14:30:11   # doorbell pressed at the entrance; controller shows the ring
cmd      <= ackRing                    # operator acknowledges from a controller
doorRing => ackRing                    # device confirms; controllers close the ring dialog
```

### Ring with auto-buzz ON (device opens the door itself)
```
doorRing => ring auto buzz, 2026-06-04 14:31:42
                                       # no buzz command needed; the device already buzzed
```

### Toggle auto-buzz — from the device vs. from a controller
```
# Operator presses switch 1 on the device:
doorRing => autoBuzzOn

# Operator toggles auto-buzz from a controller:
cmd      <= autoBuzzOff
doorRing => autoBuzzOff                # device echoes the new state to all controllers
```

### A rule on the server is active, then fires on a ring
```
cmd      <= ruleAutoBuzzOn             # rule window started; device lights its rule LED
doorRing => ring 2026-06-04 18:02:44   # device auto-buzz is off, so the device does not open
cmd      <= buzz                       # the server buzzes because of its rule
response => buzzAck
cmd      <= ruleAutoBuzzOff            # rule window ended (or the rule was deleted)
```

### Manual buzz from a controller
```
cmd      <= buzz
response => buzzAck                    # door buzzer activated
```

### Simulated ring from a controller
```
cmd      <= testRing
doorRing => testRing 2026-06-04 14:35:00
```

### Fetch the action log (multi-response)
```
cmd      <= getActionLog
response => 2026-06-04 14:30:11 ring
response => 2026-06-04 14:30:14 buzz (manual)
response => 2026-06-04 14:31:42 ring
response => endMultiResponse           # terminator
```
