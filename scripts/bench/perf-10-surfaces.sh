#!/usr/bin/env bash
# surface를 분할하고 출력을 발생시켜 마지막 perf 로그를 보여준다.
# tasty CLI와 jq가 필요하며 기존 CLI 연결 대상과 새 cargo run 인스턴스를 별도로 구분하지 않는다.

set -euo pipefail

PLATFORM="$(uname -s | tr '[:upper:]' '[:lower:]')-$(uname -m)"
PROFILE="${PERF_PROFILE:-release}"
case "$PROFILE" in
    release) CARGO_FLAGS=(--release) ;;
    dist)    CARGO_FLAGS=(--profile dist) ;;
    *) echo "error: PERF_PROFILE must be release|dist (got '$PROFILE')" >&2; exit 2 ;;
esac
LOG_DIR="${PERF_LOG_DIR:-${TMPDIR:-/tmp}/tasty-bench}"
LOG="${LOG_DIR}/perf-${PLATFORM}-${PROFILE}.log"
DURATION="${PERF_DURATION_SECS:-60}"

mkdir -p "$LOG_DIR"

if ! command -v tasty >/dev/null 2>&1; then
    echo "error: tasty CLI not on PATH" >&2
    exit 2
fi
if ! command -v jq >/dev/null 2>&1; then
    echo "error: jq required (used to parse 'tasty list surfaces')" >&2
    exit 2
fi

echo "[perf-10-surfaces] platform=$PLATFORM profile=$PROFILE duration=${DURATION}s log=$LOG"

TASTY_LOG="warn,tasty::gfx::perf=info" \
    cargo run "${CARGO_FLAGS[@]}" > "$LOG" 2>&1 &
TASTY_PID=$!
cleanup() {
    kill "$TASTY_PID" 2>/dev/null || true
    wait "$TASTY_PID" 2>/dev/null || true
}
trap cleanup EXIT

for _ in $(seq 1 60); do
    if tasty list info >/dev/null 2>&1; then break; fi
    sleep 0.5
done
tasty list info >/dev/null

FIRST_SID="$(tasty list surfaces | jq -r '.[0].id')"
if [ -z "$FIRST_SID" ] || [ "$FIRST_SID" = "null" ]; then
    echo "error: could not determine first surface id" >&2
    exit 3
fi
for _ in $(seq 1 9); do
    tasty split --level surface --target-surface "$FIRST_SID" --direction vertical >/dev/null
done

CR="$(printf '\r')"
for sid in $(tasty list surfaces | jq -r '.[].id'); do
    tasty send text "for i in \$(seq 1 5000); do echo bench_\$i; done${CR}" \
        --surface "$sid" >/dev/null
done

sleep "$DURATION"

cleanup
trap - EXIT

echo "--- last 12 perf samples ---"
grep "tasty::gfx::perf" "$LOG" | tail -12
