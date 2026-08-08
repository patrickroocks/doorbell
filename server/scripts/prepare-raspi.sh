# Ensure we are in the project root
cd "$(dirname "$0")/.."

echo "*** Transferring prepare script and service files to Raspberry Pi and executing it"

# Copy
scp scripts/raspi_scripts/prepare-on-raspi.sh raspi@192.168.178.46:~

# Execute
ssh raspi@192.168.178.46 'bash ~/prepare-on-raspi.sh'

# Clean up
ssh raspi@192.168.178.46 'rm ~/prepare-on-raspi.sh'

echo ""

./scripts/prepare-service-raspi.sh

