# 환경 노트 (에이전트 부트스트랩)

AI 에이전트가 tasty 를 조작하기 전 알아야 할 OS별 경로·실행 확인·접속 패턴. 메서드 표는 [api.md](api.md).

## 경로 (모든 OS)

| 파일 | 경로 | 설명 |
|------|------|------|
| 포트 파일 | `~/.tasty/tasty.port` | IPC 동적 포트. 실행 시 생성 |
| 설정 | `~/.tasty/config.toml` | 사용자 설정 |

(Windows 는 `~` = `%USERPROFILE%`. debug 빌드는 루트 자체가 분리되어 `~/.tasty-debug/` 아래에 같은 파일들을 둔다 — 격리 상세 [dev-guide/self-verification 독립 검증](../dev-guide/self-verification.md#독립-검증--개발도-agent-가-스스로-확인할-수-있어야-한다).)

## 실행 여부 확인

```bash
pgrep -x tasty >/dev/null && echo running || echo "not running"
# 포트 파일은 있으나 프로세스가 없으면 stale — 삭제
[ -f ~/.tasty/tasty.port ] && ! pgrep -x tasty >/dev/null && rm ~/.tasty/tasty.port
```

검증용 debug 인스턴스(`cargo run`)는 루트가 `~/.tasty-debug/` 라 포트파일이 `~/.tasty-debug/tasty.port` 다 — 위 release 체크와 무관하게 따로 띄워도 충돌하지 않는다.

## 실행 / 대기 / 종료

```bash
tasty &
until tasty list info 2>/dev/null; do sleep 0.2; done    # 포트 뜰 때까지 (sleep 루프 대신 조건검사)
tasty list tree            # 구조 (ID 포함)
tasty send text "ls -la\r" --surface <id>
# 종료: system.shutdown IPC 는 debug 빌드 전용 — release 는 프로세스를 끝낸다(kill <pid>)
```

## IPC 직접 (Python)

```python
import socket, json, os
port = int(open(os.path.expanduser("~/.tasty/tasty.port")).read().strip())
s = socket.socket(); s.settimeout(5); s.connect(("127.0.0.1", port))
def call(m, p=None):
    s.sendall((json.dumps({"jsonrpc":"2.0","id":1,"method":m,"params":p or {}}) + "\n").encode())
    return json.loads(s.recv(1<<16).decode())
call("system.info")
```

## 스크린샷

`tasty screenshot --path <png>` (IPC `ui.screenshot {path}`) 는 **GUI 모드**에서 동작하는 정식
release 기능이다(focus 독립 — `--surface <id>` 로 터미널 surface 를 오프스크린 캡처하거나
`--window <id>` 로 창 프레임 캡처). 결과 PNG. 상세 [screenshot-methods](../ai-verification/screenshot-methods.md).

## 관련

- [api.md](api.md) — 주요 IPC/CLI · [dev-guide/self-verification](../dev-guide/self-verification.md) — 검증 시나리오
