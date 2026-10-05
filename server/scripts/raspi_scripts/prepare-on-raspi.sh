#!/bin/bash
#
# Runs ON the Raspberry Pi - copied there and started by scripts/prepare-raspi.sh.
#
# Unattended: the service manager starts it over ssh with no terminal, so nothing here may
# ask a question. Anything that would is given its "yes" on the command line.

# "-e" above all: without it a failing step is invisible, the script reaches its last echo and
# exits 0, and the manager reports a preparation that did nothing as a success.
set -euo pipefail

# Never let a package ask about a config file or a service restart.
export DEBIAN_FRONTEND=noninteractive

# No "set -e" without this: every command below needs root, and without passwordless sudo each one
# fails on its own while the script runs happily to the end. Checked once, up front, so the answer is
# one line instead of a screenful of "a password is required".
if ! sudo -n true 2>/dev/null; then
    echo "ERROR: This script needs passwordless sudo on the Raspberry Pi." >&2
    echo "       It is run unattended from the service manager, so there is no terminal to type a" >&2
    echo "       password into. Raspberry Pi OS used to ship that rule and an upgrade of" >&2
    echo "       raspberrypi-sys-mods removes it, so it has to be put back:" >&2
    echo "         ~/code/dev_ops/raspi/enable_unattended_sudo.sh" >&2
    exit 1
fi

echo "Setting up Raspberry Pi for Doorbell application"

# Setup rust
# ----------

echo "Setup Rust environment"
# "-y": the installer asks for confirmation otherwise and there is nobody to give it - which is what
# "error: Unable to run interactively" means when this runs from the service manager.
if [[ -x "$HOME/.cargo/bin/rustc" ]]; then
    echo "Rust is already installed, skipping rustup"
else
    curl https://sh.rustup.rs -sSf | sh -s -- -y
fi
sudo apt-get update
sudo apt-get install --yes build-essential libssl-dev pkg-config

# Setup tools
# -----------

sudo apt-get install --yes sqlite3


# Setup mosquitto MQTT broker
# ---------------------------

echo "Setup mosquitto"
sudo apt-get install --yes mosquitto mosquitto-clients

# All config files in this directory are included automatically
CONFIG_FILE="/etc/mosquitto/conf.d/doorbell.conf"
sudo tee "$CONFIG_FILE" > /dev/null <<EOF
listener 1883
allow_anonymous true
EOF
sudo chmod 644 "$CONFIG_FILE"
sudo systemctl restart mosquitto

# Create necessary directories
# ----------------------------

echo "Creating doorbell directories"
mkdir -p ~/doorbell_release
mkdir -p ~/doorbell_release/data
mkdir -p ~/doorbell_source

# Setup ngrok
# -----------

echo "Setup ngrok"
# NOTE: "bookworm" is intentional and must not be changed to the actual Debian
# codename of the Raspi (trixie since the last upgrade). The ngrok repository
# only serves a bookworm suite - .../dists/trixie/Release returns HTTP 404,
# so "correcting" the codename here would break 'apt update'.
curl -sSL https://ngrok-agent.s3.amazonaws.com/ngrok.asc \
  | sudo tee /etc/apt/trusted.gpg.d/ngrok.asc >/dev/null \
  && echo "deb https://ngrok-agent.s3.amazonaws.com bookworm main" \
  | sudo tee /etc/apt/sources.list.d/ngrok.list \
  && sudo apt-get update \
  && sudo apt-get install --yes ngrok

echo "------------------------------------------"
echo "TODO_MANUAL_START: Configure ngrok with your authtoken!"
echo "------------------------------------------"
echo "Goto: https://dashboard.ngrok.com/get-started/setup/linux"
echo "and run: ngrok config add-authtoken <your_token>" 
echo "on the raspi"
echo "TODO_MANUAL_END"