#!/bin/bash
# Builds and deploys delivery2door on the Raspberry Pi, run from the laptop.
#
# Runs unattended from the service manager as well as by hand, so it must never wait for input and
# must report its result honestly: the last line is UPDATE_SUCCESS or UPDATE_FAILURE, and the exit
# code matches it. "set -e" is deliberately not used - each step is checked explicitly so the log
# says which one failed.
set -uo pipefail

# Ensure we are in the project root
cd "$(dirname "$0")/.." || { echo "Cannot enter the project root"; echo "UPDATE_FAILURE"; exit 1; }

RASPI_PATH='raspi@192.168.178.46'
TARGET_DIR='~/delivery2door_source'

# BatchMode: never prompt for a password or a host key - without it an unattended run would hang
# forever instead of failing. ConnectTimeout: a Pi that is off must not block the run for minutes.
SSH_OPTS='-o BatchMode=yes -o ConnectTimeout=10'

# Aborts the whole script when a step failed. Used after every transfer, so a broken copy can no
# longer be followed by a build of a half-updated source tree.
failIfError() {
    if [ "$1" -ne 0 ]; then
        echo "*** FAILED: $2 (exit $1)"
        echo "UPDATE_FAILURE (exit $1)"
        exit "$1"
    fi
}

echo "*** Transferring source code to Raspberry Pi"

# Delete previous source files. Only delete source files, not the "target" build files which the cached package builds.
ssh $SSH_OPTS "$RASPI_PATH" "rm -rf $TARGET_DIR/src/* $TARGET_DIR/static/* $TARGET_DIR/templates/*"
failIfError $? "clearing the source directories on the Pi"

# Copy source files.
SCP_TARGET_DIR="$RASPI_PATH:$TARGET_DIR"
scp $SSH_OPTS -r src/ "$SCP_TARGET_DIR"
failIfError $? "copying src/"
scp $SSH_OPTS -r static/ "$SCP_TARGET_DIR"
failIfError $? "copying static/"
scp $SSH_OPTS -r templates/ "$SCP_TARGET_DIR"
failIfError $? "copying templates/"
scp $SSH_OPTS Cargo.toml "$SCP_TARGET_DIR/Cargo.toml"
failIfError $? "copying Cargo.toml"
# The lock file goes along on purpose: the IMAP client is an alpha release, so the Pi has to build
# against exactly the versions that were tested here instead of resolving fresh ones.
scp $SSH_OPTS Cargo.lock "$SCP_TARGET_DIR/Cargo.lock"
failIfError $? "copying Cargo.lock"
scp $SSH_OPTS build.rs "$SCP_TARGET_DIR/build.rs"
failIfError $? "copying build.rs"
scp $SSH_OPTS config/config-release.json "$SCP_TARGET_DIR/config-release.json"
failIfError $? "copying config-release.json"
echo ""

echo "*** Transferring build&deploy script to Raspberry Pi and executing it"

# Copy build&deploy script.
scp $SSH_OPTS scripts/raspi_scripts/build-and-deploy-on-raspi.sh "$RASPI_PATH:~"
failIfError $? "copying the Pi-side build script"

# Execute it. The status is captured here, before the cleanup below: the cleanup used to be the last
# command of the script and so decided its exit code, which made every failed build look successful.
RC=0
ssh $SSH_OPTS "$RASPI_PATH" 'bash -l ~/build-and-deploy-on-raspi.sh' || RC=$?

# Clean up build&deploy script. Runs whether the build worked or not, and its own result is ignored
# on purpose - a leftover script on the Pi must not turn a good build into a failure.
ssh $SSH_OPTS "$RASPI_PATH" 'rm -f ~/build-and-deploy-on-raspi.sh'

if [ $RC -ne 0 ]; then
    echo "UPDATE_FAILURE (exit $RC)"
    exit $RC
fi

echo "UPDATE_SUCCESS"
exit 0
