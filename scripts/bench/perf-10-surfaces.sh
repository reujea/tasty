#!/usr/bin/env bash
#
# 10-surface 시나리오 기반 perf 측정.
# 사전: tasty release 빌드 + `tasty` CLI 가 PATH 에 있어야 함.
# 출력: ${PERF_LOG_DIR:-${TMPDIR:-/tmp}/tasty-bench}/perf-{platform}-{profile}.log 의 마지막 12 `perf` 라인.
#
# 측정 segment 정의 / window 크기 등은 docs/dev-guide/gpu-rendering.md#성능-측정 참조.

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

# 1) tasty 실행 (perf 로그만 노출, 일반 noise 억제)
RUST_LOG="tasty::gfx::perf=info,tasty=warn" \
    cargo run "${CARGO_FLAGS[@]}" > "$LOG" 2>&1 &
TASTY_PID=$!
cleanup() {
    kill "$TASTY_PID" 2>/dev/null || true
    wait "$TASTY_PID" 2>/dev/null || true
}
trap cleanup EXIT

# 2) tasty ready 대기 (`tasty list info` 가 0 종료할 때까지)
for _ in $(seq 1 60); do
    if tasty list info >/dev/null 2>&1; then break; fi
    sleep 0.5
done
tasty list info >/dev/null

# 3) 첫 surface 의 ID 확보 후 surface 분할 × 9
FIRST_SID="$(tasty list surfaces | jq -r '.[0].id')"
if [ -z "$FIRST_SID" ] || [ "$FIRST_SID" = "null" ]; then
    echo "error: could not determine first surface id" >&2
    exit 3
fi
for _ in $(seq 1 9); do
    tasty split --level surface --target-surface "$FIRST_SID" --direction vertical >/dev/null
done

# 4) 각 surface 에 5000 줄 출력 트리거
CR="$(printf '\r')"
for sid in $(tasty list surfaces | jq -r '.[].id'); do
    tasty send text "for i in \$(seq 1 5000); do echo bench_\$i; done${CR}" \
        --surface "$sid" >/dev/null
done

# 5) 측정 (DURATION 초)
sleep "$DURATION"

# 6) 종료
cleanup
trap - EXIT

# 7) 마지막 12 perf 라인 추출 (≈60s @ 5s/dump)
echo "--- last 12 perf samples ---"
grep "tasty::gfx::perf" "$LOG" | tail -12
