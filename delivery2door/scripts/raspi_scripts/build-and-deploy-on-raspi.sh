#!/bin/bash
# Builds and deploys delivery2door on the Raspberry Pi itself. Copied here and started by the laptop-side
# build-and-deploy-raspi.sh, never run by hand.
#
# Runs without a terminal, so nothing in it may ask a question: every sudo uses "-n" and fails
# instead of waiting for a password. "set -e" is deliberately not used - each step is checked
# explicitly so the log says which one failed.
set -uo pipefail

# ----------------------- Path variables -----------------------

DIR_SOURCE=~/delivery2door_source
DIR_RELEASE=~/delivery2door_release
BINARY_NAME=delivery2door
SERVICE_NAME=delivery2door.service

PATH_COMPILE=$DIR_SOURCE/target/release/$BINARY_NAME
PATH_RELEASE=$DIR_RELEASE/$BINARY_NAME

# Aborts the script when a step failed, naming the step in the log.
failIfError() {
    if [ "$1" -ne 0 ]; then
        echo "*** FAILED: $2 (exit $1)"
        exit "$1"
    fi
}

# ----------------------- Build -----------------------

echo "*** Build delivery2door application on Raspberry Pi"

echo "Delete previous build if exists"
rm -f "$PATH_COMPILE"

echo "Building the application"
cd "$DIR_SOURCE"
failIfError $? "entering the source directory $DIR_SOURCE"
cargo build --release
failIfError $? "cargo build --release"
cd ~ || exit 1

if [ ! -f "$PATH_COMPILE" ]; then
    echo "Build failed! Exiting."
    exit 1
fi

echo ""

# ----------------------- Deploy -----------------------

echo "*** Deploying the application"

# Stop the service if it's running. "-n" so a missing NOPASSWD rule fails right here instead of
# blocking forever on a password prompt that nobody can answer - sudo reads /dev/tty directly, so
# redirecting stdin would not help. A unit that is not installed yet is not an error.
echo "Stopping $SERVICE_NAME"
sudo -n systemctl stop "$SERVICE_NAME" 2>/dev/null || echo "Service not running or not installed yet"

# Copy binary
cp "$PATH_COMPILE" "$PATH_RELEASE"
failIfError $? "copying the binary to $PATH_RELEASE"
# Copy dependent files
cp "$DIR_SOURCE/config-release.json" "$DIR_RELEASE/config-release.json"
failIfError $? "copying config-release.json"
mkdir -p "$DIR_RELEASE/static"
cp $DIR_SOURCE/static/* "$DIR_RELEASE/static/"
failIfError $? "copying the static files"

# Copy templates directory
mkdir -p "$DIR_RELEASE/templates"
cp $DIR_SOURCE/templates/* "$DIR_RELEASE/templates/"
failIfError $? "copying the templates"

# The databases live here and must survive a deployment, so the directory is only created.
mkdir -p "$DIR_RELEASE/data"

# Restart the service
echo "Starting $SERVICE_NAME"
sudo -n systemctl start "$SERVICE_NAME"
failIfError $? "starting $SERVICE_NAME"

# Show service status. Only for the log: "systemctl status" answers 3 for a stopped unit, so letting
# it decide the exit code would report a perfectly good deployment as a failure - and, worse, a
# service that died at startup as a success in the other direction. "is-active" below decides.
echo "Current service status:"
sudo -n systemctl status "$SERVICE_NAME" --no-pager -l

# The unit may have started and then died on a bad config, which "systemctl start" does not report.
sudo -n systemctl is-active --quiet "$SERVICE_NAME"
failIfError $? "$SERVICE_NAME is not active after the start"

# Only now - with the binary in place and the unit actually up - is the deployment done.
echo "Deployment completed successfully!"
exit 0
