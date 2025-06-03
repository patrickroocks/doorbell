echo "Build new doorbell client..."
rm -rf build
mkdir build
cd build
cmake .. -DCMAKE_PREFIX_PATH=/opt/Qt/6.8.3/gcc_64
make -j8
cd ..

echo "Stop running client..."
pids=$(pgrep "doorbell-client")

if [ -z "$pids" ]; then
  echo "No running instances of doorbell-client found."
else
  echo "Killing the following processes: $pids"
  # Kill the processes
  kill -9 $pids
  echo "Processes killed."
fi

# Wait that the file becomes writeable
sleep 2

echo "Copy new client to relase directory and start it..."

# Copy it
mkdir -p ../client-release
cp build/doorbell-client ../client-release/doorbell-client

# Run it
../client-release/doorbell-client > /dev/null 2>&1 & disown

echo "New doorbell client build and started."
