# Doorbell MQTT Contract

This is the **single source of truth** for the MQTT protocol spoken between the doorbell
device and its controllers. Three independent codebases implement this protocol and must
stay in sync with this document:

| Role                                            | Project                        | Defines its strings in                                                                                                                  |
| ----------------------------------------------- | ------------------------------ | --------------------------------------------------------------------------------------------------------------------------------------- |
| **Device** (publishes events, answers commands) | `broker-arduino/` (ESP32, C++) | [`mqttHandler.cpp`](../broker-arduino/mqttHandler.cpp)                                                                                  |
| **Controller** (sends commands, shows state)    | `server/` (Rust HTTP server)   | [`src/mqtt/mqttProtocol.rs`](../server/src/mqtt/mqttProtocol.rs)                                                                        |
| **Controller** (desktop tray app)               | `client/` (Qt/C++)             | [`command.cpp`](../client/command.cpp) / [`command.h`](../client/command.h), topics in [`ringlistener.cpp`](../client/ringlistener.cpp) |


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

| Payload        | Meaning                                                 | Expected reply                                                     |
| -------------- | ------------------------------------------------------- | ------------------------------------------------------------------ |
| `ping`         | Liveness probe; also used to establish the connection.  | `pong` on `response`                                               |
| `buzz`         | Activate the door buzzer (open the door).               | `buzzAck` on `response` *(see quirks)*                             |
| `autoBuzzOn`   | Enable auto-buzzer.                                     | — (device echoes `autoBuzzOn` on `doorRing` if the state changed)  |
| `autoBuzzOff`  | Disable auto-buzzer.                                    | — (device echoes `autoBuzzOff` on `doorRing` if the state changed) |
| `getAutoBuzz`  | Query current auto-buzz state.                          | `autoBuzzOn` or `autoBuzzOff` on `response`                        |
| `testRing`     | Simulate a ring (does not require the physical button). | `testRing <timestamp>` on `doorRing`                               |
| `ackRing`      | Acknowledge the active ring.                            | `ackRing` on `doorRing` (only if a ring is active)                 |
| `getActionLog` | Fetch the device action log.                            | multi-response on `response`, ends with `endMultiResponse`         |
| `getRawData`   | Fetch raw button-press data.                            | multi-response on `response`, ends with `endMultiResponse`         |
| `getStartTime` | Query device start timestamp.                           | timestamp string on `response`                                     |
| `reboot`       | Reboot the device.                                      | `ackReboot` on `response`, then the device reboots                 |

## `doorRing` payloads (device → controllers)

| Payload                       | Meaning                                                                                                                                                                                                  |
| ----------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `ring <timestamp>`            | Real doorbell press while auto-buzz is **off**.                                                                                                                                                          |
| `ring auto buzz, <timestamp>` | Real doorbell press while auto-buzz is **on** (the device also buzzes automatically).                                                                                                                    |
| `testRing <timestamp>`        | Simulated ring (never carries the `auto buzz,` prefix, regardless of state).                                                                                                                             |
| `ackRing`                     | The active ring was acknowledged (by a switch on the device or an `ackRing` command).                                                                                                                    |
| `deviceBuzz`                  | The physical "ack & buzz" button on the device triggered the door buzzer. (Auto-buzz on a ring is conveyed by the `ring auto buzz,` prefix instead; server-commanded buzzes by `buzzAck` on `response`.) |
| `autoBuzzOn`                  | Auto-buzz was just turned **on** (device switch 1 or an `autoBuzzOn` command).                                                                                                                           |
| `autoBuzzOff`                 | Auto-buzz was just turned **off**.                                                                                                                                                                       |

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
   `getAutoBuzz` (to sync the toggle) — the Rust server additionally requests `getStartTime`.
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
