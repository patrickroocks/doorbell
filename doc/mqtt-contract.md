# Doorbell MQTT Contract

This is the **single source of truth** for the MQTT protocol spoken between the doorbell
device and its controllers. Four independent codebases implement this protocol and must
stay in sync with this document:

| Role                                                | Project                        | Defines its strings in                                                                                                                  |
| --------------------------------------------------- | ------------------------------ | --------------------------------------------------------------------------------------------------------------------------------------- |
| **Device** (publishes events, answers commands)     | `broker-arduino/` (ESP32, C++) | [`mqttHandler.cpp`](../broker-arduino/mqttHandler.cpp)                                                                                  |
| **Controller** (sends commands, shows state)        | `server/` (Rust HTTP server)   | [`src/mqtt/mqttProtocol.rs`](../server/src/mqtt/mqttProtocol.rs)                                                                        |
| **Controller** (desktop tray app)                   | `client/` (Qt/C++)             | [`command.cpp`](../client/command.cpp) / [`command.h`](../client/command.h), topics in [`ringlistener.cpp`](../client/ringlistener.cpp) |
| **Device** (emulator, for testing without hardware) | `emulated-device/` (Rust)      | [`src/mqttProtocol.rs`](../emulated-device/src/mqttProtocol.rs)                                                                         |


## Overview

```
        ┌──────────────────┐         cmd  ─────────────►          ┌─────────────┐
        │   Controllers    │                                      │   Device    │
        │  ───────────     │  ◄───── doorRing / response ─────    │ (ESP32 +    │
        │  Rust server     │                                      │  local MQTT │
        │  Qt client       │         (via MQTT broker)            │  broker)    │
        └──────────────────┘                                      └─────────────┘
```

- The ESP32 **device** runs a local MQTT broker (Mosquitto-style) and is the only party that
  publishes on `doorRing` and `response`. It subscribes to `cmd`.
- Any number of **controllers** (the Rust server, the Qt client) connect to the same broker,
  publish on `cmd`, and subscribe to `doorRing` and `response`.
- The device is considered "connected" by a controller only after a `ping` → `pong`
  round-trip (see [Connection lifecycle](#connection-lifecycle)).

## Topics

| Topic      | Direction            | Purpose                                    |
| ---------- | -------------------- | ------------------------------------------ |
| `cmd`      | controller → device  | Commands sent to the device.               |
| `doorRing` | device → controllers | Ring events and auto-buzz state changes.   |
| `response` | device → controllers | Replies to commands that expect an answer. |

## `cmd` payloads (controller → device)

All payloads are exact, case-sensitive ASCII strings.

| Payload           | Meaning                                                 | Expected reply                                                     |
| ----------------- | ------------------------------------------------------- | ------------------------------------------------------------------ |
| `ping`            | Liveness probe; also used to establish the connection.  | `pong` on `response`                                               |
| `buzz`            | Activate the door buzzer (open the door).               | `buzzAck` on `response` *(see quirks)*                             |
| `autoBuzzOn`      | Enable auto-buzzer.                                     | — (device echoes `autoBuzzOn` on `doorRing` if the state changed)  |
| `autoBuzzOff`     | Disable auto-buzzer.                                    | — (device echoes `autoBuzzOff` on `doorRing` if the state changed) |
| `getAutoBuzz`     | Query current auto-buzz state.                          | `autoBuzzOn` or `autoBuzzOff` on `response`                        |
| `ruleAutoBuzzOn`  | A controller-side rule is active to buzz on a ring.     | — (display only, see below)                                        |
| `ruleAutoBuzzOff` | No controller-side rule is active.                      | — (display only, see below)                                        |
| `testRing`        | Simulate a ring (does not require the physical button). | `testRing <timestamp>` on `doorRing`                               |
| `ackRing`         | Acknowledge the active ring.                            | `ackRing` on `doorRing` (only if a ring is active)                 |
| `getActionLog`    | Fetch the device action log.                            | multi-response on `response`, ends with `endMultiResponse`         |
| `getRawData`      | Fetch raw button-press data.                            | multi-response on `response`, ends with `endMultiResponse`         |
| `getStartTime`    | Query device start timestamp.                           | timestamp string on `response`                                     |
| `reboot`          | Reboot the device.                                      | `ackReboot` on `response`, then the device reboots                 |

### Device vs. rule auto-buzz

`autoBuzzOn`/`autoBuzzOff` control the **device** auto-buzzer: the device opens the door by
itself on the next ring, the state survives a reboot (EEPROM) and it can also be toggled with
switch 1 on the device.

`ruleAutoBuzzOn`/`ruleAutoBuzzOff` only *report* that a **controller-side rule** is armed, so
that the device can show it on its rule LED. The device keeps this state in RAM only, never
acts on it and never echoes it back — when such a rule fires, the controller sends a plain
`buzz`. The two states are independent; a controller must not derive one from the other.

## `doorRing` payloads (device → controllers)

| Payload                       | Meaning                                                                               |
| ----------------------------- | ------------------------------------------------------------------------------------- |
| `ring <timestamp>`            | Real doorbell press while auto-buzz is **off**.                                       |
| `ring auto buzz, <timestamp>` | Real doorbell press while auto-buzz is **on** (the device also buzzes automatically). |
| `testRing <timestamp>`        | Simulated ring (never carries the `auto buzz,` prefix, regardless of state).          |
| `ackRing`                     | The active ring was acknowledged (by a switch on the device or an `ackRing` command). |
| `deviceBuzz`                  | The physical "ack & buzz" button on the device triggered the door buzzer.             |
| `autoBuzzOn`                  | Auto-buzz was just turned **on** (device switch 1 or an `autoBuzzOn` command).        |
| `autoBuzzOff`                 | Auto-buzz was just turned **off**.                                                    |

`<timestamp>` is `YYYY-MM-DD HH:MM:SS`. Before NTP sync the device substitutes
`(No NTP time, seconds since device start: NNNN)`.

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

- **Plain ASCII strings.** No JSON, no binary. Commands are matched by exact string
  equality; `doorRing` ring events carry a trailing ` <timestamp>`
- **QoS 0**, **no retained messages**.
- The device buffers a fixed-size action log / raw-data history; older entries are dropped
  silently.

## Connection lifecycle

1. A controller connects to the broker and subscribes to `doorRing` + `response`.
2. It sends `ping`. Until a `pong` arrives, the controller is only "MQTT connected", not
   "device connected".
3. On `pong`, the controller marks the device connected and immediately requests
   `getAutoBuzz` (to sync the toggle) — the Rust server additionally requests `getStartTime`
   and re-sends the current `ruleAutoBuzzOn`/`ruleAutoBuzzOff` state, which the device does
   not persist.
4. While connected, controllers re-`ping` after ~20 s of inactivity; a missing reply within
   ~5 s drops them back to "not connected".
5. Reboot-on-foreign-broker (Rust server only): if the server finds the device connected
   through a broker whose host is not `localhost`, it issues `reboot` to push the device back onto the expected broker.

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
doorRing => ring 2026-06-04 14:30:11   # device button pressed; controller shows the ring
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
# Operator flips switch 1 on the device:
doorRing => autoBuzzOn

# Operator toggles auto-buzz from a controller:
cmd      <= autoBuzzOff
doorRing => autoBuzzOff                # device echoes the new state to all controllers
```

### A controller-side rule is armed, then fires on a ring
```
cmd      <= ruleAutoBuzzOn             # rule window started; device lights its rule LED
doorRing => ring 2026-06-04 18:02:44   # device auto-buzzer is off, so the device does not open
cmd      <= buzz                       # the controller buzzes because of its rule
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
