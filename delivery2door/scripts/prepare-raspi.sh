# Ensure we are in the project root
cd "$(dirname "$0")/.."

# No "-e": the clean-up below has to run even when the step before it failed.
set -uo pipefail

echo "*** Transferring prepare script to Raspberry Pi and executing it"

RC=0

# Copy
scp scripts/raspi_scripts/prepare-on-raspi.sh raspi@192.168.178.46:~ || RC=$?

# Execute
if [ "$RC" -eq 0 ]; then
    ssh raspi@192.168.178.46 'bash ~/prepare-on-raspi.sh' || RC=$?
fi

# Clean up - best effort, and never the reason the whole step counts as failed.
ssh raspi@192.168.178.46 'rm -f ~/prepare-on-raspi.sh' || true

echo ""

if [ "$RC" -ne 0 ]; then
    echo "ERROR: The preparation failed on the Raspberry Pi (exit code $RC)." >&2
    echo "UPDATE_FAILURE (exit $RC)"
    exit "$RC"
fi

# The sentinel the service manager looks for: it records a step as done only for a run that both
# exits 0 and says so here. Keep this the last line.
echo "UPDATE_SUCCESS"
