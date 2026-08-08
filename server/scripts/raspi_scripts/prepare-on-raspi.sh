echo "Setting up Raspberry Pi for Doorbell application"

# Setup rust
# ----------

echo "Setup Rust environment"
curl https://sh.rustup.rs -sSf | sh
sudo apt-get update
sudo apt-get install build-essential libssl-dev pkg-config

# Setup tools
# -----------

sudo apt install sqlite3


# Setup mosquitto MQTT broker
# ---------------------------

echo "Setup mosquitto"
sudo apt install mosquitto mosquitto-clients

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
  && sudo apt update \
  && sudo apt install ngrok

echo "------------------------------------------"
echo "TODO: Configure ngrok with your authtoken!"
echo "------------------------------------------"
echo "Goto: https://dashboard.ngrok.com/get-started/setup/linux"
echo "and run: ngrok config add-authtoken <your_token>" 