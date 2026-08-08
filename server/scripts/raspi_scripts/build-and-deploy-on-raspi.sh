# ----------------------- Path variables -----------------------

DIR_SOURCE=~/doorbell_source
DIR_RELEASE=~/doorbell_release
BINARY_NAME=doorbell_http_server

PATH_COMPILE=$DIR_SOURCE/target/release/$BINARY_NAME
PATH_RELEASE=$DIR_RELEASE/$BINARY_NAME

# ----------------------- Build -----------------------

echo "*** Build doorbell application on Raspberry Pi"

echo "Delete previous build if exists"
rm -f $PATH_COMPILE

echo "Building the application"
cd $DIR_SOURCE
cargo build --release
cd ~

if [ ! -f "$PATH_COMPILE" ]; then
    echo "Build failed! Exiting."
    exit 1
fi

echo ""


# ----------------------- Deploy -----------------------

echo "*** Deploying the application"

# Stop the service if it's running
echo "Stopping doorbell service"
sudo systemctl stop doorbell.service 2>/dev/null || echo "Service not running or not installed yet"

# Copy binary
cp $PATH_COMPILE $PATH_RELEASE
# Copy dependent files
cp $DIR_SOURCE/config_release.json $DIR_RELEASE/config_release.json
mkdir -p $DIR_RELEASE/static
cp $DIR_SOURCE/static/* $DIR_RELEASE/static/

# Copy templates directory
mkdir -p $DIR_RELEASE/templates
cp $DIR_SOURCE/templates/* $DIR_RELEASE/templates/

echo "Deployment completed successfully!"

# Restart the service
echo "Starting doorbell service"
sudo systemctl start doorbell.service

# Show service status
echo "Current service status:"
sudo systemctl status doorbell.service --no-pager -l
