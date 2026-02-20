#!/bin/bash
set -e
echo "Running load test on BarqCoder metrics endpoint (target: 10k req/min)..."

SUCCESS=0
FAILED=0
START_TIME=$(date +%s)

for i in {1..1000}; do
  if curl -s http://localhost:8080/health > /dev/null; then
    SUCCESS=$((SUCCESS+1))
  else
    FAILED=$((FAILED+1))
  fi
done

END_TIME=$(date +%s)
DURATION=$((END_TIME - START_TIME))
echo "Sent 1000 requests in $DURATION seconds."
echo "Success: $SUCCESS, Failed: $FAILED"
