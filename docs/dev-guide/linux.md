# 개발 가이드 — Linux

tasty 를 개발하는 AI 에이전트용 Linux 환경 가이드. (경로 예시는 Linux dev 머신 기준 — 본인 환경에 맞춰 치환.)

## 바이너리 / 실행

```
target/debug/tasty     # 디버그
target/release/tasty   # 릴리스
```

`tasty` 는 PATH 에 없다 — 직접 경로 또는 `cargo run`.

```bash
# GUI 실행 + 준비 대기 — PID 를 저장한다(이름·패턴으로 찾지 않는다)
./target/debug/tasty & TASTY_PID=$!
while [ ! -f ~/.tasty-debug/tasty.port ]; do sleep 0.2; done

# 실행 여부
kill -0 "$TASTY_PID" 2>/dev/null && echo running || echo "not running"

# 다중 인스턴스 — 루트를 분리한다
TASTY_HOME=/tmp/tasty-a ./target/debug/tasty &
```

종료: `system.shutdown` IPC — **debug 전용이고 CLI 는 없다**([api-conventions](api-conventions.md)
"debug 표에 있는데 CLI 가 없는 메서드"). 포트로 raw JSON-RPC 를 보낸다. `kill "$TASTY_PID"` 로 죽이면 포트 파일 수동 삭제(`rm -f ~/.tasty-debug/tasty.port`).

## 빌드 후 재시작

Linux 는 실행 중 바이너리를 `cargo build` 로 교체해도 실행 프로세스에 영향 없다(inode 참조). 실행 중 인스턴스는 옛 바이너리로 계속 동작, 다음 실행부터 새 바이너리.

## 스크린샷

GUI 모드 전용(headless 불가):

```python
call("ui.screenshot", {"path": "/tmp/tasty-capture.png"})   # PNG
```

hover/애니메이션 등 상태 의존 UI 는 정적 캡처로 확인 불가 — 조건(`if response.hovered()` 등)을 임시 제거해 항상 적용 → 빌드/재시작/캡처 → 확인 후 복원. (시각 검증 휴리스틱: [ai-verification/visual-verification](../ai-verification/visual-verification.md).)
