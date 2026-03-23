#!/bin/sh
set -e

cleanup() {
    kill 0 2>/dev/null
    exit 0
}
trap cleanup INT TERM

Xvfb :99 -screen 0 640x640x24 &
export DISPLAY=:99
sleep 1

x11vnc -display :99 -forever -nopw -listen 0.0.0.0 -rfbport 5900 -q &

websockify --web=/usr/share/novnc 6080 localhost:5900 &

echo ""
echo "Open http://localhost:6080/vnc.html in your browser"
echo ""

exec demetra-sim
