# Doorbell HTTP Server

This project implements a Rust web server, that makes the doorbell device (the Arduino project in `device-arduino`, wired to the house intercom) reachable from a browser. It connects as an MQTT client to the MQTT broker and offers a web UI to configure and diagnose the system. 

Main functionalities:
* Toggle the Auto Buzz state via web UI
* Create time-based auto-buzz rules
* View and manage the device log and server log
* A token-protected REST API for rule management, used by `delivery2door`, a project that turns delivery announcements into auto-buzz rules

Technologies used:
- Rust
- Tokio for asynchronous runtime
- Axum for the web server
- MQTT for communication with the doorbell device
- SQLite for logging
- Askama for the web UI templates
- ngrok for external access

# Setup Rust & dependencies on PC

## Rust

### Install Rust and dependencies

```
curl https://sh.rustup.rs -sSf | sh
sudo apt-get update
sudo apt-get install build-essential libssl-dev pkg-config
```

### Update Rust

```
rustup update stable
rustup component add rust-analyzer
```

### Update packages

Updates the .toml files. Do this after a update of rust.

```
cargo install cargo-edit
cargo upgrade
```

Also update incompatible dependencies, code might have to be fixed afterwards:

```
cargo upgrade --incompatible
```

## Install ngrok

```
curl -sSL https://ngrok-agent.s3.amazonaws.com/ngrok.asc \
  | sudo tee /etc/apt/trusted.gpg.d/ngrok.asc >/dev/null \
  && echo "deb https://ngrok-agent.s3.amazonaws.com bookworm main" \
  | sudo tee /etc/apt/sources.list.d/ngrok.list \
  && sudo apt update \
  && sudo apt install ngrok
```

## Release build on PC

Trigger the release build:

```
cargo build --release
```

Run it:

```
./target/release/doorbell_http_server --config cfg/config_release.json
```

# Config

See `cfg/config_release.json.example` for an example configuration. This file has to be renamed to `cfg/config_release.json`. The list of MQTT servers is tried in order until a connection is successful. Special attributes are:

* `rebootDeviceOnConnect`: If true, the device is rebooted when it connects via this broker. With this, we move the MQTT broker from the device to the Raspi.

# Service state

`GET /serviceState` answers with the state of the service itself without the need of a token or session.

Example request:
```bash
curl localhost:3001/serviceState
```

Example response:
```json
{"serviceStartTime":"2026-09-14 12:41:35","connectionState":"Broker only",
 "mqttServer":"localhost","version":"0.3.0","buildTime":"2026-09-14 12:40:47"}
```

# Rule API for other services

Lets another service create and edit rules without the web forms - used by `delivery2door`, which
turns delivery announcements into auto-buzz rules. Access via the API token specified in the configuration.

```json
"apiTokens": [ "a-long-random-token" ]
```

Every request needs `Authorization: Bearer <token>`; anything else is a 401.

| Method | Path              | Result                                      |
| ------ | ----------------- | ------------------------------------------- |
| POST   | `/api/rules`      | creates a rule, `201 {"id": 42}`            |
| GET    | `/api/rules`      | all rules                                   |
| GET    | `/api/rules/{id}` | one rule, `404` if it does not exist        |
| PUT    | `/api/rules/{id}` | replaces a rule, `404` if it does not exist |
| DELETE | `/api/rules/{id}` | `204`                                       |

The body is the same in both directions, so a rule read back can be sent again unchanged. An empty date means "no limit".

```bash
curl -X POST localhost:3001/api/rules -H 'Authorization: Bearer TOKEN' \
  -H 'Content-Type: application/json' \
  -d '{"name":"DHL 357820075428","enabled":true,"autoAck":true,"singleShot":false,
       "deleteAtEnd":true,"startDate":"2026-08-12","endDate":"2026-08-12",
       "activationTime":"11:45","deactivationTime":"17:15"}'
```

## Rule lifetime: deleteAtEnd

A rule with `deleteAtEnd` removes itself once its last window has ended, i.e. at
`endDate` + `deactivationTime` (for a window running past midnight one day later, since the end date
is the day the last window *starts* on). It therefore requires an end date. Meant for rules created
by another service, which nobody would clean up by hand. The deletion appears in the server log as
"Rule deleted at the end of its lifetime".

# Raspberry Pi deployment

## Build and deploy for Raspberry Pi

* Execute `prepare-raspi.sh` once
* Build with `build-and-deploy-on-raspi.sh`

## Service management on Raspberry Pi

sudo systemctl start doorbell.service    # Start
sudo systemctl stop doorbell.service     # Stop
sudo systemctl restart doorbell.service  # Restart
sudo systemctl status doorbell.service   # Check status
journalctl -u doorbell.service -f        # View logs

# Log Database

Raw view:

```
sqlite3 -header -column data/serverLog.sqlite "SELECT * FROM serverLog;" 2>&1 | head -20
```

Reset the database:

```
./scripts/dev_scripts/reset-log.sh
```