#!/usr/bin/env bash
# Stress-test the same fake harness under tmux.
# Usage: stress-tmux.sh [iterations] [commands_per_iteration]
set -euo pipefail

SCRIPT_DIR="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
FAKE_HARNESS="$SCRIPT_DIR/fake-harness.sh"
ITERATIONS="${1:-50}"
COMMANDS_PER_ITER="${2:-10}"
SESSION="stress-tmux-$$"
TARGET="$SESSION"

TMUX_BIN="${TMUX_BIN:-$(command -v tmux || true)}"
[[ -x "$TMUX_BIN" ]] || { echo "tmux not found" >&2; exit 2; }

cleanup() {
  "$TMUX_BIN" kill-session -t "$SESSION" >/dev/null 2>&1 || true
}
trap cleanup EXIT

send_cmd() {
  local cmd=$1
  "$TMUX_BIN" send-keys -t "$TARGET" -l -- "$cmd"
  sleep 0.05
  "$TMUX_BIN" send-keys -t "$TARGET" Enter
}

capture() {
  "$TMUX_BIN" capture-pane -t "$TARGET" -p -S -100 2>/dev/null || true
}

wait_for() {
  local marker=$1
  local deadline=$(( $(date +%s) + 5 ))
  while [[ $(date +%s) -lt $deadline ]]; do
    if capture | grep -q "$marker"; then
      return 0
    fi
    sleep 0.05
  done
  return 1
}

START_ALL=$(date +%s.%N)
SUCCESSES=0
FAILURES=0

for i in $(seq 1 "$ITERATIONS"); do
  iter_start=$(date +%s.%N)

  # Create session and run harness. Keep the pane alive briefly after the
  # harness exits so we can capture the final HARNESS_EXIT marker.
  "$TMUX_BIN" new-session -d -s "$SESSION" -x 120 -y 40 "bash -c '$FAKE_HARNESS; sleep 30'"
  if ! wait_for "HARNESS_READY"; then
    echo "iter $i: harness did not start"
    FAILURES=$((FAILURES + 1))
    cleanup
    continue
  fi

  ok=1
  for c in $(seq 1 "$COMMANDS_PER_ITER"); do
    cmd="cmd-${i}-${c}"
    send_cmd "$cmd"
    if ! wait_for "ACK:cmd-${i}-${c}"; then
      echo "iter $i cmd $c: ack not found"
      ok=0
    fi
  done

  send_cmd "exit"
  if ! wait_for "HARNESS_EXIT"; then
    echo "iter $i: exit marker not found"
    ok=0
  fi

  # Give the process a moment to finish, then kill the session.
  sleep 0.1
  cleanup

  if [[ "$ok" -eq 1 ]]; then
    SUCCESSES=$((SUCCESSES + 1))
  else
    FAILURES=$((FAILURES + 1))
  fi

  iter_end=$(date +%s.%N)
  iter_ms=$(echo "($iter_end - $iter_start) * 1000" | bc | cut -d. -f1)
  echo "tmux iter $i: ${iter_ms}ms"
done

END_ALL=$(date +%s.%N)
total_ms=$(echo "($END_ALL - $START_ALL) * 1000" | bc | cut -d. -f1)

# Check for orphaned tmux sessions.
orphan_count=$("$TMUX_BIN" ls 2>/dev/null | grep -c "^${SESSION}" || true)

echo "---"
echo "tmux stress: $SUCCESSES/$ITERATIONS successful, $FAILURES failures"
echo "total time: ${total_ms}ms"
echo "avg per iteration: $(echo "scale=2; $total_ms / $ITERATIONS" | bc)ms"
echo "orphan sessions matching $SESSION: $orphan_count"
