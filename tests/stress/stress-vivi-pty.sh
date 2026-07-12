#!/usr/bin/env bash
# Stress-test the same fake harness under vivi-pty.
# Usage: stress-vivi-pty.sh [iterations] [commands_per_iteration]
set -euo pipefail

SCRIPT_DIR="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
PROJECT="$(CDPATH= cd -- "$SCRIPT_DIR/../.." && pwd)"
FAKE_HARNESS="$SCRIPT_DIR/fake-harness.sh"
ITERATIONS="${1:-50}"
COMMANDS_PER_ITER="${2:-10}"

VIVI_PTY_BIN="${VIVI_PTY_BIN:-$(command -v vivi-pty || true)}"
[[ -x "$VIVI_PTY_BIN" ]] || { echo "vivi-pty not found" >&2; exit 2; }

SOCKET="$PROJECT/.vivi/vivi-pty-stress.sock"

DAEMON_PID=""

start_daemon() {
  if [[ -S "$SOCKET" ]] && "$VIVI_PTY_BIN" info --socket "$SOCKET" >/dev/null 2>&1; then
    return 0
  fi
  rm -f "$SOCKET"
  "$VIVI_PTY_BIN" daemon --project "$PROJECT" --socket "$SOCKET" >/dev/null 2>&1 &
  DAEMON_PID=$!
  local i=0
  while [[ $i -lt 30 ]]; do
    sleep 0.2
    if [[ -S "$SOCKET" ]] && "$VIVI_PTY_BIN" info --socket "$SOCKET" >/dev/null 2>&1; then
      return 0
    fi
    i=$((i + 1))
  done
  echo "daemon failed to start" >&2
  return 1
}

stop_daemon() {
  if [[ -n "$DAEMON_PID" ]]; then
    kill "$DAEMON_PID" >/dev/null 2>&1 || true
    wait "$DAEMON_PID" 2>/dev/null || true
  fi
  rm -f "$SOCKET"
  sleep 0.2
}

start_session() {
  local session_id=$1
  "$VIVI_PTY_BIN" session start "$session_id" --driver generic --cwd "$PROJECT" --socket "$SOCKET" -- "$FAKE_HARNESS" >/dev/null 2>&1
}

stop_session() {
  local session_id=$1
  "$VIVI_PTY_BIN" session stop "$session_id" --socket "$SOCKET" >/dev/null 2>&1 || true
}

send_cmd() {
  local session_id=$1
  local cmd=$2
  "$VIVI_PTY_BIN" terminal write "$session_id" "$cmd" --enter --socket "$SOCKET" >/dev/null 2>&1
}

snapshot() {
  local session_id=$1
  "$VIVI_PTY_BIN" terminal snapshot "$session_id" --socket "$SOCKET" 2>/dev/null || true
}

wait_for() {
  local session_id=$1
  local marker=$2
  local deadline=$(( $(date +%s) + 5 ))
  while [[ $(date +%s) -lt $deadline ]]; do
    if snapshot "$session_id" | grep -q "$marker"; then
      return 0
    fi
    sleep 0.05
  done
  return 1
}

cleanup_all() {
  # Stop any leftover sessions from this run.
  for i in $(seq 1 "$ITERATIONS"); do
    stop_session "stress-pty-$i" >/dev/null 2>&1 || true
  done
  stop_daemon
}
trap cleanup_all EXIT

start_daemon

START_ALL=$(date +%s.%N)
SUCCESSES=0
FAILURES=0

for i in $(seq 1 "$ITERATIONS"); do
  iter_start=$(date +%s.%N)
  session_id="stress-pty-$i"

  if ! start_session "$session_id"; then
    echo "iter $i: session start failed"
    FAILURES=$((FAILURES + 1))
    continue
  fi

  if ! wait_for "$session_id" "HARNESS_READY"; then
    echo "iter $i: harness did not start"
    FAILURES=$((FAILURES + 1))
    stop_session "$session_id"
    continue
  fi

  ok=1
  for c in $(seq 1 "$COMMANDS_PER_ITER"); do
    cmd="cmd-${i}-${c}"
    send_cmd "$session_id" "$cmd"
    if ! wait_for "$session_id" "ACK:cmd-${i}-${c}"; then
      echo "iter $i cmd $c: ack not found"
      ok=0
    fi
  done

  send_cmd "$session_id" "exit"
  if ! wait_for "$session_id" "HARNESS_EXIT"; then
    echo "iter $i: exit marker not found"
    ok=0
  fi

  sleep 0.1
  stop_session "$session_id"

  if [[ "$ok" -eq 1 ]]; then
    SUCCESSES=$((SUCCESSES + 1))
  else
    FAILURES=$((FAILURES + 1))
  fi

  iter_end=$(date +%s.%N)
  iter_ms=$(echo "($iter_end - $iter_start) * 1000" | bc | cut -d. -f1)
  echo "vivi-pty iter $i: ${iter_ms}ms"
done

END_ALL=$(date +%s.%N)
total_ms=$(echo "($END_ALL - $START_ALL) * 1000" | bc | cut -d. -f1)

# Check for leftover processes.
orphan_count=0
if [[ -n "$DAEMON_PID" ]] && kill -0 "$DAEMON_PID" >/dev/null 2>&1; then
  orphan_count=1
fi

echo "---"
echo "vivi-pty stress: $SUCCESSES/$ITERATIONS successful, $FAILURES failures"
echo "total time: ${total_ms}ms"
echo "avg per iteration: $(echo "scale=2; $total_ms / $ITERATIONS" | bc)ms"
echo "daemon processes for $SOCKET: $orphan_count"
