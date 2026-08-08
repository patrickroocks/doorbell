
# Setup Rust & dependencies on PC

## Setup Rust

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

# Build and deploy for Raspberry Pi

* Execute `prepare-raspi.sh` once
* Build with `build-and-deploy-on-raspi.sh`

# Service management on Raspberry Pi

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