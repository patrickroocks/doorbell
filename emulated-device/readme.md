# Doorbell device emulator

A small console application that partially emulates the ESP32 doorbell device
(`broker-arduino/`) for debugging and demonstrating the system without the real hardware.
It speaks the MQTT protocol described in [doc/mqtt-contract.md](../doc/mqtt-contract.md). 
The LEDs and relays are not emulated.

## Build and run (debug)

Requires an MQTT broker (e.g. Mosquitto) reachable at the host/port in `config.json` — the
same broker the Rust server connects to. The emulator exits immediately if it cannot connect.

Build:

```
cargo build
```

Run:

```
./target/debug/doorbell_emulated_device
```

## Keys

| Key | Emulates              | Effect                                                                         |
| --- | --------------------- | ------------------------------------------------------------------------------ |
| `r` | doorbell ring button  | publishes `ring`/`ring auto buzz, <ts>` on `doorRing` (auto-buzzes if enabled) |
| `a` | switch 1 (auto-buzz)  | toggles auto-buzz, publishes `autoBuzzOn`/`autoBuzzOff`                        |
| `b` | switch 2 (ack & buzz) | acknowledges the ring; if auto-buzz is off, buzzes and publishes `deviceBuzz`  |
| `k` | switch 3 (ack)        | acknowledges the active ring                                                   |
| `q` | —                     | quit (also Esc / Ctrl-C)                                                       |

Incoming commands from a controller (`ping`, `buzz`, `testRing`, `autoBuzzOn/Off`,
`getAutoBuzz`, `getActionLog`, `getStartTime`, `reboot`, …) are handled automatically and
reflected in the rolling event log.
