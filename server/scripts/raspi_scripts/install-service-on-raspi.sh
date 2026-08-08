#!/bin/bash

# Script to install and enable the doorbell systemd service

SERVICE_NAME=doorbell.service
SERVICE_FILE="$(dirname "$0")/$SERVICE_NAME"
SYSTEMD_DIR=/etc/systemd/system

echo "*** Installing doorbell systemd service"

# Check if service file exists
if [ ! -f "$SERVICE_FILE" ]; then
    echo "Error: Service file $SERVICE_FILE not found!"
    exit 1
fi

# Copy service file to systemd directory (requires sudo)
echo "Copying service file to $SYSTEMD_DIR"
sudo cp $SERVICE_FILE $SYSTEMD_DIR/

# Reload systemd daemon
echo "Reloading systemd daemon"
sudo systemctl daemon-reload

# Enable service to start on boot
echo "Enabling $SERVICE_NAME to start on boot"
sudo systemctl enable $SERVICE_NAME

# Check status
echo ""
echo "Service installed and enabled successfully!"
echo "You can now manage the service with:"
echo "  sudo systemctl start $SERVICE_NAME"
echo "  sudo systemctl stop $SERVICE_NAME"
echo "  sudo systemctl restart $SERVICE_NAME"
echo "  sudo systemctl status $SERVICE_NAME"
