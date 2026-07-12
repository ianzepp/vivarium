#!/usr/bin/env bash
# Simulated LLM harness for stress testing Fleet runtimes.
# Reads commands from stdin, acknowledges them, and loops until "exit".
set -euo pipefail

echo "HARNESS_READY"
while IFS= read -r line; do
    if [[ "$line" == "exit" ]]; then
        echo "HARNESS_EXIT"
        break
    fi
    # Normalize the line and echo a deterministic ack.
    clean="${line//[$'\r\n']/}"
    printf 'ACK:%s\n' "${clean}"
done
echo "HARNESS_DONE"
