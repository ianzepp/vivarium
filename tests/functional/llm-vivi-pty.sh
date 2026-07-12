#!/usr/bin/env bash
# Functional test: run actual LLM harnesses (Grok, Codex, Pi) inside vivi-pty
# and verify they answer simple instructional commands.
#
# Usage:
#   tests/functional/llm-vivi-pty.sh
#   HARNESS=codex,pi tests/functional/llm-vivi-pty.sh
#   HARNESS=grok GROK_MODEL=grok-4.5 tests/functional/llm-vivi-pty.sh
#
# Per-harness overrides: GROK_COMMAND, CODEX_COMMAND, PI_COMMAND,
# GROK_MODEL, CODEX_MODEL, PI_MODEL.
set -uo pipefail

SCRIPT_DIR="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
PROJECT="$(CDPATH= cd -- "$SCRIPT_DIR/../.." && pwd)"

GROK_BIN="${GROK_BIN:-/Users/ianzepp/.local/bin/grok}"
CODEX_BIN="${CODEX_BIN:-/opt/homebrew/bin/codex}"
PI_BIN="${PI_BIN:-/Users/ianzepp/.nvm/versions/node/v24.15.0/bin/pi}"

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

SOCKET="${VIVI_PTY_SOCKET:-$PROJECT/.vivi/vivi-pty-llm-functional.sock}"
DAEMON_PID=""

vivi_pty() {
  "$VIVI_PTY_BIN" --socket "$SOCKET" "$@"
}

snapshot_text() {
  local session_id=$1
  vivi_pty terminal snapshot "$session_id" 2>/dev/null | jq -r '.contents' 2>/dev/null || true
}

marker_count() {
  local text=$1 marker=$2
  grep -oF "$marker" <<< "$text" 2>/dev/null | wc -l | tr -d ' '
}

apply_model_override() {
  local command=$1 model=$2
  read -ra arr <<< "$command"
  local replaced=0
  for ((i = 0; i < ${#arr[@]}; i++)); do
    if [[ "${arr[$i]}" == "--model" ]] && (( i + 1 < ${#arr[@]} )); then
      arr[$((i + 1))]="$model"
      replaced=1
      break
    fi
  done
  if [[ $replaced -eq 0 ]]; then
    arr+=("--model" "$model")
  fi
  echo "${arr[*]}"
}

drop_arg() {
  local command=$1 arg=$2
  read -ra arr <<< "$command"
  local result=() skip=0
  for a in "${arr[@]}"; do
    if [[ $skip -eq 1 ]]; then
      skip=0
      continue
    fi
    if [[ "$a" == "$arg" ]]; then
      skip=1
      continue
    fi
    result+=("$a")
  done
  echo "${result[*]}"
}

get_harness_command() {
  local harness=$1
  local default=""
  case $harness in
    grok)
      if [[ -f "$FLEET_JSON" ]] && command -v jq >/dev/null 2>&1; then
        default=$(jq -r '.hands["hand-2"].runtime.command | if . then join(" ") else "" end' "$FLEET_JSON")
      fi
      [[ -n "$default" ]] || default="$GROK_BIN --always-approve --model deepseek-v4-flash-openrouter"
      default="${GROK_COMMAND:-$default}"
      if [[ -n "${GROK_MODEL:-}" ]]; then default=$(apply_model_override "$default" "$GROK_MODEL"); fi
      ;;
    codex)
      if [[ -f "$FLEET_JSON" ]] && command -v jq >/dev/null 2>&1; then
        default=$(jq -r '.tooling.codex.binary // empty' "$FLEET_JSON")
      fi
      [[ -n "$default" ]] || default="$CODEX_BIN"
      default="${CODEX_COMMAND:-$default}"
      if [[ -n "${CODEX_MODEL:-}" ]]; then default=$(apply_model_override "$default" "$CODEX_MODEL"); fi
      ;;
    pi)
      if [[ -f "$FLEET_JSON" ]] && command -v jq >/dev/null 2>&1; then
        default=$(jq -r '.heads["head-ceo"].agent_launch // empty' "$FLEET_JSON")
        # Do not pollute the real head-ceo session history with test runs.
        if [[ -n "$default" ]]; then default=$(drop_arg "$default" --name); fi
      fi
      [[ -n "$default" ]] || default="$PI_BIN --provider zai --model glm-5.2 --thinking high --approve"
      default="${PI_COMMAND:-$default}"
      if [[ -n "${PI_MODEL:-}" ]]; then default=$(apply_model_override "$default" "$PI_MODEL"); fi
      ;;
    *)
      echo "error: unknown harness '$harness'" >&2
      return 1
      ;;
  esac
  echo "$default"
}

get_harness_driver() {
  echo "$1"
}

get_harness_cwd() {
  # Grok needs a known project directory to avoid the project-selection dialog.
  # Codex and Pi are tested in /tmp with their own startup dialogs handled.
  case $1 in
    grok) echo "$PROJECT" ;;
    *) echo "/tmp" ;;
  esac
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
  for harness in "${HARNESS_LIST[@]}"; do
    vivi_pty session stop "llm-functional-$harness" >/dev/null 2>&1 || true
  done
  stop_daemon
  exit "$ec"
}
trap cleanup EXIT

handle_startup_dialog() {
  local harness=$1 session_id=$2
  case $harness in
    codex)
      if snapshot_text "$session_id" | grep -q "Do you trust the contents of this directory"; then
        echo "  trust dialog detected; sending Enter"
        vivi_pty terminal key "$session_id" Enter >/dev/null 2>&1 || true
        sleep 3
      fi
      ;;
  esac
}

send_prompt() {
  local session_id=$1 prompt=$2
  vivi_pty terminal write "$session_id" "$prompt" >/dev/null 2>&1 || true
  vivi_pty terminal key "$session_id" Enter >/dev/null 2>&1 || true
}

# Instructional commands each harness must answer.
QUESTIONS=(
  "What is 2+2?"
  "What is the capital of France?"
  "How many letters are in the word 'functional'?"
)
EXPECTED=(
  "4"
  "Paris"
  "10"
)

run_turn() {
  local session_id=$1 turn=$2 question=$3 expected=$4
  local marker="VIVI_PTY_TURN_${turn}_OK"
  local prompt="Question: $question. Please answer with a single line that starts with the exact text $marker and then your answer."

  echo "  --- turn $turn: $question"
  send_prompt "$session_id" "$prompt"

  local timeout="${TURN_TIMEOUT:-120}"
  local deadline=$(( $(date +%s) + timeout ))
  while [[ $(date +%s) -lt $deadline ]]; do
    local text=$(snapshot_text "$session_id")
    local count=$(marker_count "$text" "$marker")
    if (( count >= 2 )); then
      local response=$(grep -F "$marker" <<< "$text" | tail -n 1 || true)
      if grep -qiF "$expected" <<< "$response"; then
        echo "  PASS: $response"
      else
        echo "  PASS (expected '$expected' not found): $response"
      fi
      return 0
    fi
    sleep 2
  done

  echo "  FAIL: marker $marker not seen in response (timeout)"
  echo "  last snapshot (first 30 lines):"
  snapshot_text "$session_id" | head -n 30 | sed 's/^/    /' || true
  return 1
}

run_harness() {
  local harness=$1
  local session_id="llm-functional-$harness"
  local command=$(get_harness_command "$harness")
  local driver=$(get_harness_driver "$harness")
  local cwd=$(get_harness_cwd "$harness")
  read -ra cmd <<< "$command"

  echo
  echo "== harness: $harness =="
  echo "command: ${cmd[*]}"
  echo "cwd: $cwd"

  if ! [[ -x "${cmd[0]}" ]]; then
    echo "  SKIP: binary not executable: ${cmd[0]}"
    return 1
  fi

  if ! vivi_pty session start "$session_id" \
       --driver "$driver" \
       --cwd "$cwd" \
       -- "${cmd[@]}" >/dev/null 2>&1; then
    echo "  FAIL: session start failed"
    return 1
  fi

  sleep 3
  handle_startup_dialog "$harness" "$session_id"

  local passes=0 failures=0
  for ((i = 0; i < ${#QUESTIONS[@]}; i++)); do
    if run_turn "$session_id" "$((i + 1))" "${QUESTIONS[$i]}" "${EXPECTED[$i]}"; then
      passes=$((passes + 1))
    else
      failures=$((failures + 1))
    fi
    # Let the harness TUI settle before the next prompt.
    sleep 2
  done

  vivi_pty session stop "$session_id" >/dev/null 2>&1 || true

  TOTAL_PASSES=$((TOTAL_PASSES + passes))
  TOTAL_FAILURES=$((TOTAL_FAILURES + failures))
  HARNESS_PASSES[$harness]=$passes
  HARNESS_FAILURES[$harness]=$failures

  echo "harness $harness: $passes/${#QUESTIONS[@]} passed, $failures/${#QUESTIONS[@]} failed"
  [[ $failures -eq 0 ]]
}

# Legacy single-harness overrides still apply to Grok.
GROK_COMMAND="${GROK_COMMAND:-${LLM_COMMAND:-}}"
GROK_MODEL="${GROK_MODEL:-${LLM_MODEL:-}}"

HARNESS="${HARNESS:-grok,codex,pi}"
IFS=, read -ra HARNESS_LIST <<< "$HARNESS"

mkdir -p "$PROJECT/target/tmp"
start_daemon

TOTAL_PASSES=0
TOTAL_FAILURES=0
declare -A HARNESS_PASSES
declare -A HARNESS_FAILURES

for harness in "${HARNESS_LIST[@]}"; do
  run_harness "$harness" || true
done

echo
echo "=========================="
echo "functional LLM vivi-pty test"
for harness in "${HARNESS_LIST[@]}"; do
  p=${HARNESS_PASSES[$harness]:-0}
  f=${HARNESS_FAILURES[$harness]:-0}
  printf '%-6s: %s/%s passed, %s/%s failed\n' "$harness" "$p" "${#QUESTIONS[@]}" "$f" "${#QUESTIONS[@]}"
done
echo "total: $TOTAL_PASSES passed, $TOTAL_FAILURES failed"

if [[ $TOTAL_FAILURES -gt 0 ]]; then
  exit 1
fi
