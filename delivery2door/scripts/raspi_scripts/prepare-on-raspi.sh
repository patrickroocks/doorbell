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

echo "Setting up Raspberry Pi for the Delivery2Door application"

# Setup rust
# ----------
# Harmless to run again if the doorbell server was already set up on this Pi.

echo "Setup Rust environment"
# Tested by path, not with "command -v": a non-interactive ssh shell does not have ~/.cargo/bin on
# its PATH, so an existing installation would look missing and be installed over.
if [[ -x "$HOME/.cargo/bin/rustc" ]]; then
    echo "Rust is already installed, skipping rustup"
else
    # "-y": the installer asks for confirmation otherwise and there is nobody to give it.
    curl https://sh.rustup.rs -sSf | sh -s -- -y
fi
sudo apt-get update
# libssl-dev and pkg-config are needed by native-tls, which secures the IMAP connection.
sudo apt-get install --yes build-essential libssl-dev pkg-config

# Setup tools
# -----------

sudo apt-get install --yes sqlite3

# Create necessary directories
# ----------------------------

echo "Creating delivery2door directories"
mkdir -p ~/delivery2door_release
mkdir -p ~/delivery2door_release/data
mkdir -p ~/delivery2door_source

# No broker and no tunnel here: this service talks IMAP to the mail provider and HTTP to the doorbell
# service on localhost. Both are reached as a client, so nothing has to be installed for them.

# Wrapped in the markers the service manager looks for: it keeps this text, shows it, and will not
# let the installation go on until somebody confirms they have done it.
echo "---------------------------------------------------------------"
echo "TODO_MANUAL_START: Connect this service to the doorbell service"
echo "---------------------------------------------------------------"
echo "The doorbell service needs an apiTokens entry that matches"
echo "\"doorbell.apiToken\" of config-release.json, and it has to"
echo "run on the baseUrl configured there."
echo "TODO_MANUAL_END"
