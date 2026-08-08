PID=$(pgrep -f doorbell_http_server)
if [ -z "$PID" ]; then
  echo "doorbell_http_server not running."
  exit 1
fi

echo "Stopping doorbell_http_server (PID: $PID) with SIGTERM..."
kill -SIGTERM $PID