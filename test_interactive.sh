#!/bin/bash
# Quick test to see raw mode behavior
echo "Starting ccmonitor-launcher in background..."
./target/release/ccmonitor-launcher --verbose claude &
PID=$!
sleep 2
echo "Sending test input..."
echo "hello" 
sleep 1
echo "Killing process..."
kill $PID 2>/dev/null
wait $PID 2>/dev/null
echo "Test complete"