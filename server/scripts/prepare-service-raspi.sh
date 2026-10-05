# Ensure we are in the project root
cd "$(dirname "$0")/.."

# No "-e": the clean-up below has to run even when the step before it failed.
set -uo pipefail

echo "*** Transferring service files to Raspberry Pi and executing it"

RC=0

# Copy
scp scripts/raspi_scripts/install-service-on-raspi.sh raspi@192.168.178.46:~ || RC=$?
scp scripts/raspi_scripts/doorbell.service raspi@192.168.178.46:~ || RC=$?

# Execute
if [ "$RC" -eq 0 ]; then
    ssh raspi@192.168.178.46 'bash ~/install-service-on-raspi.sh' || RC=$?
fi

# Clean up - best effort, and never the reason the whole step counts as failed.
ssh raspi@192.168.178.46 'rm -f ~/install-service-on-raspi.sh ~/doorbell.service' || true

if [ "$RC" -ne 0 ]; then
    echo "ERROR: Installing the systemd service failed on the Raspberry Pi (exit code $RC)." >&2
    echo "UPDATE_FAILURE (exit $RC)"
    exit "$RC"
fi

# The sentinel the service manager looks for: it records a step as done only for a run that both
# exits 0 and says so here. Keep this the last line.
echo "UPDATE_SUCCESS"
