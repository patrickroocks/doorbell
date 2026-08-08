# Ensure we are in the project root
cd "$(dirname "$0")/.."

echo "*** Transferring source code to Raspberry Pi"

RASPI_PATH='raspi@192.168.178.46'

TARGET_DIR='~/doorbell_source'
# Delete previous source files. Only delete source files, not the "target" build files which the cached package builds.
ssh $RASPI_PATH "rm -rf $TARGET_DIR/src/*"
ssh $RASPI_PATH "rm -rf $TARGET_DIR/static/*"
ssh $RASPI_PATH "rm -rf $TARGET_DIR/templates/*"

# Copy source files.
SCP_TARGET_DIR="$RASPI_PATH:$TARGET_DIR"
scp -r src/ $SCP_TARGET_DIR
scp -r static/ $SCP_TARGET_DIR
scp -r templates/ $SCP_TARGET_DIR
scp Cargo.toml $SCP_TARGET_DIR/Cargo.toml
scp build.rs $SCP_TARGET_DIR/build.rs
scp cfg/config_release.json $SCP_TARGET_DIR/config_release.json
echo ""

echo "*** Transferring build&deploy script to Raspberry Pi and executing it"

# Copy build&deploy script.
scp scripts/raspi_scripts/build-and-deploy-on-raspi.sh $RASPI_PATH:~

# Execute it.
ssh $RASPI_PATH 'bash -l ~/build-and-deploy-on-raspi.sh'

# Clean up build&deploy script.
ssh $RASPI_PATH 'rm ~/build-and-deploy-on-raspi.sh'
