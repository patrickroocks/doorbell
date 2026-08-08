echo "*** Transferring service files to Raspberry Pi and executing it"

# Copy
scp scripts/raspi_scripts/install-service-on-raspi.sh raspi@192.168.178.46:~
scp scripts/raspi_scripts/doorbell.service raspi@192.168.178.46:~

# Execute
ssh raspi@192.168.178.46 'bash ~/install-service-on-raspi.sh'

# Clean up
ssh raspi@192.168.178.46 'rm ~/install-service-on-raspi.sh ~/doorbell.service'