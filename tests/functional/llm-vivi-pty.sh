#!/usr/bin/env bash
# Functional test: run an actual LLM harness inside vivi-pty and verify it
# answers a series of simple instructional commands.
#
# Defaults to the hand-2 runtime from .vivi/fleet.json. Override the full
# command with LLM_COMMAND, or just the model with LLM_MODEL.
#
# Example:
#   tests/functional/llm-vivi-pty.sh
#   LLM_MODEL=grok-4.5 tests/functional/llm-vivi-pty.sh
set -uo pipefail

SCRIPT_DIR="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
PROJECT="$(CDPATH= cd -- "$SCRIPT_DIR/../.." && pwd)"

VIVI_PTY_BIN="${VIVI_PTY_BIN:-}"
if [[ -z "$VIVI_PTY_BIN" ]]; then
  if [[ -x "$PROJECT/target/release/vivi-pty" ]]; then
    VIVI_PTY_BIN="$PROJECT/target/release/vivi-pty"
  else
    VIVI_PTY_BIN="$(command -v vivi-pty || true)"
  fi
fi
if [[ -z "$VIVI_PTY_BIN" ]] || ! [[ -x "$VIVI_PTY_BIN" ]]; then
  echo "error: vivi-pty binary not found. Set VIVI_PTY_BIN or build with 'cargo build --release -p vivi-pty'." >&2
  exit 2
fi

FLEET_JSON="$PROJECT/.vivi/fleet.json"
if [[ -f "$FLEET_JSON" ]] && command -v jq >/dev/null 2>&1; then
  DEFAULT_LLM_COMMAND="$(jq -r '.hands["hand-2"].runtime.command | if . then join(" ") else "" end' "$FLEET_JSON")"
fi
if [[ -z "${DEFAULT_LLM_COMMAND:-}" ]]; then
  DEFAULT_LLM_COMMAND="/Users/ianzepp/.local/bin/grok --always-approve --model deepseek-v4-flash-openrouter"
fi

LLM_COMMAND="${LLM_COMMAND:-$DEFAULT_LLM_COMMAND}"
read -ra LLM_CMD <<< "$LLM_COMMAND"

if [[ -n "${LLM_MODEL:-}" ]]; then
  replaced=0
  for ((i = 0; i < ${#LLM_CMD[@]}; i++)); do
    if [[ "${LLM_CMD[$i]}" == "--model" ]] && (( i + 1 < ${#LLM_CMD[@]} )); then
      LLM_CMD[$((i + 1))]="$LLM_MODEL"
      replaced=1
      break
    fi
  done
  if [[ $replaced -eq 0 ]]; then
    echo "warning: LLM_MODEL set but command has no --model flag to replace" >&2
  fi
fi

if ! [[ -x "${LLM_CMD[0]}" ]]; then
  echo "error: LLM binary not executable: ${LLM_CMD[0]}" >&2
  exit 2
fi

SOCKET="${VIVI_PTY_SOCKET:-$PROJECT/.vivi/vivi-pty-llm-functional.sock}"
SESSION_ID="llm-functional"
DAEMON_PID=""

vivi_pty() {
  "$VIVI_PTY_BIN" --socket "$SOCKET" "$@"
}

snapshot_text() {
  vivi_pty terminal snapshot "$SESSION_ID" 2>/dev/null | jq -r '.contents' 2>/dev/null || true
}

marker_count() {
  local text=$1 marker=$2
  grep -oF "$marker" <<< "$text" 2>/dev/null | wc -l | tr -d ' '
}

start_daemon() {
  if [[ -S "$SOCKET" ]]; then
    rm -f "$SOCKET"
  fi
  vivi_pty daemon >"$PROJECT/target/tmp/llm-functional-daemon.log" 2>&1 &
  DAEMON_PID=$!
  local i=0
  while (( i < 30 )); do
    sleep 0.2
    if [[ -S "$SOCKET" ]] && vivi_pty info >/dev/null 2>&1; then
      return 0
    fi
    i=$((i + 1))
  done
  echo "error: daemon failed to start" >&2
  cat "$PROJECT/target/tmp/llm-functional-daemon.log" >&2 || true
  return 1
}

stop_daemon() {
  if [[ -n "$DAEMON_PID" ]]; then
    kill "$DAEMON_PID" >/dev/null 2>&1 || true
    wait "$DAEMON_PID" 2>/dev/null || true
  fi
  rm -f "$SOCKET"
}

cleanup() {
  local ec=$?
  vivi_pty session stop "$SESSION_ID" >/dev/null 2>&1 || true
  stop_daemon
  exit "$ec"
}
trap cleanup EXIT

mkdir -p "$PROJECT/target/tmp"
start_daemon

# Start the LLM harness inside vivi-pty, mirroring the hand-2 fleet role.
vivi_pty session start "$SESSION_ID" \
  --driver grok \
  --cwd "$PROJECT" \
  -- "${LLM_CMD[@]}" >/dev/null 2>&1

echo "LLM session started: ${LLM_CMD[*]}"

# Give the interactive TUI time to render.
sleep 3

# Define the instructional commands. Each answer must include a unique marker.
declare -a QUESTIONS=(
  "What is 2+2?"
  "What is the capital of France?"
  "How many letters are in the word 'functional'?"
)
declare -a EXPECTED=(
  "4"
  "Paris"
  "10"
)

PASSES=0
FAILURES=0
TOTAL=${#QUESTIONS[@]}

for ((i = 0; i < TOTAL; i++)); do
  turn=$((i + 1))
  marker="VIVI_PTY_TURN_${turn}_OK"
  question="${QUESTIONS[$i]}"
  expected="${EXPECTED[$i]}"
  prompt="Question: $question. Please answer with a single line that starts with the exact text $marker and then your answer."

  echo
  echo "--- turn $turn: $question"
  vivi_pty terminal write "$SESSION_ID" "$prompt" --enter >/dev/null 2>&1

  deadline=$(( $(date +%s) + 90 ))
  found=0
  response=""
  while [[ $(date +%s) -lt $deadline ]]; do
    text=$(snapshot_text)
    count=$(marker_count "$text" "$marker")
    if (( count >= 2 )); then
      found=1
      response=$(grep -F "$marker" <<< "$text" | tail -n 1 || true)
      break
    fi
    sleep 1
  done

  if [[ $found -eq 0 ]]; then
    echo "FAIL: marker $marker not seen in response (timeout)"
    FAILURES=$((FAILURES + 1))
    continue
  fi

  if grep -qiF "$expected" <<< "$response"; then
    echo "PASS: $response"
    PASSES=$((PASSES + 1))
  else
    echo "FAIL: response did not contain expected '$expected': $response"
    FAILURES=$((FAILURES + 1))
  fi
done

echo
echo "=========================="
echo "functional LLM vivi-pty test"
echo "passed: $PASSES/$TOTAL"
echo "failed: $FAILURES/$TOTAL"

if [[ $FAILURES -gt 0 ]]; then
  exit 1
fi
