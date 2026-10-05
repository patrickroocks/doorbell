PID=$(pgrep -f delivery2door)
if [ -z "$PID" ]; then
  echo "delivery2door not running."
  exit 1
fi

echo "Stopping delivery2door (PID: $PID) with SIGTERM..."
kill -SIGTERM $PID
