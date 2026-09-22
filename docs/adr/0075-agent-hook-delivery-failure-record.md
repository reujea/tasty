# ADR-0075: agent hook 전달 실패를 CLI 로컬 파일에 기록하고, exit code 는 노출하지 않는다

- **Status**: Accepted
- **Date**: 2026-08-20
- **Tags**: agent-hooks, observability, cli, error-handling
- **Group**: agent-integration

## Context

Claude Code / Codex CLI 는 턴 경계마다 `tasty claude hook <event>` / `tasty codex hook <event>` 를 **동기 실행**해 자식의 상태 전환(idle / needs_input / active)을 tasty 로 push 한다. 재전송이 없는 **1 회성 push** 라 유실은 곧 영구 손실이다.

그런데 설치되는 명령이 `[ -n "$TASTY_SURFACE_ID" ] && tasty … hook <e> || true` 형태여서 **exit code 가 통째로 버려졌다.** IPC 연결 실패든 JSON-RPC 에러든 tasty 쪽에도, 사용자 쪽에도, 어떤 로그에도 흔적이 남지 않는다. 자식이 영구히 `active` 로 남은 사고에서 원인을 특정하지 못한 이유가 이것이다.

CLI 는 실패를 **이미 알고 있다** — 연결 실패는 `anyhow::Result` 로, JSON-RPC 에러는 `eprintln!` + `exit(1)` 로 보고한다. 없는 정보를 새로 만드는 문제가 아니라, 있는 정보를 버리지 않는 문제다.

제약이 세 가지 있다.

1. **`|| true` 는 두 가지 일을 겸한다.** `$TASTY_SURFACE_ID` 미설정(= tasty 밖에서 Claude Code 를 쓰는 사용자)에게 오류를 안 보이게 하는 **정당한 목적**과, hook 명령 실패를 삼키는 **문제**가 한 연산자에 묶여 있다. 분리 없이 제거하면 전자가 깨진다.
2. **실패 보고 채널이 IPC 면 안 된다.** 실패의 주된 원인이 "tasty 에 닿지 못함" 이므로 `telemetry.record` 같은 IPC 경로로는 그 실패를 보고할 수 없다(chicken-and-egg).
3. **비-0 exit 의 파급을 이 저장소에서 확인할 수 없다.** Claude Code / Codex 가 hook 의 비-0 exit 를 무시하는지, 경고를 띄우는지, 턴을 차단하는지는 외부 도구의 런타임 동작이다.

## Decision

**가드를 `if` 블록으로 올려 두 역할을 분리하고, hook 실패는 CLI 가 `<tasty_home>/hook-failures.log` 에 한 줄 append 한다. 셸 래퍼의 최종 exit 는 계속 0 으로 유지한다.**

설치되는 명령:

```
if [ -n "$TASTY_SURFACE_ID" ]; then tasty claude hook <token> || true; fi
```

- 바깥 `if` — tasty 밖 환경은 블록에 진입조차 하지 않는다(가드 자체가 명시적 성공 종료). 기존의 무소음 동작이 그대로 유지된다.
- 안쪽 `|| true` — 오직 hook 명령 실패만 담당한다. 실패 처리 정책을 바꿀 때 손댈 지점이 한 군데로 좁혀진다.
- 기록 — `tasty-cli` 의 `hook_failure::record` 가 포트 파일 부재 / connect 실패 / JSON-RPC 에러 **세 지점 모두**에서 `<tasty_home>/hook-failures.log` 에 `<UTC> method=… event=… surface=… reason=…` 한 줄을 남긴다. `event` 는 `params.event`(hook 이벤트 이름)에서 꺼낸다 — `method` 는 `claude.hook` 하나로 고정이라 이게 없으면 `stop` 실패와 `session-end` 실패를 구분할 수 없다. 이벤트 이름을 갖지 않는 hook(`claude.checklist_hook`)은 `event=-`. best-effort(기록 실패는 무시), 256 KiB 에서 `.log.1` 로 1 단 로테이션.

- 언어 — `reason` 의 **값**은 사용자 로케일과 무관한 **영어 고정**이다. 같은 실패를 사용자에게 알리는 stderr 는 번역문을 그대로 쓴다. 대상이 다르기 때문이다: stderr 는 지금 화면 앞의 사람이 읽고, 이 파일은 사후에 — 사람이 아니라 에이전트가 — 알려진 실패 패턴과 대조하며 읽는다. 진단 문구가 설정에 따라 흔들리면 그 대조도 `grep` 도 성립하지 않는다. 집행은 타입으로 한다: `hook_failure::record` 는 `DiagnosticEnglish` 만 받으므로 번역 결과를 그냥 넘길 수 없고, 그 타입의 탈출구(`new_unchecked`)에 번역 호출이 들어가는 것은 `crates/tasty-doc-guards/tests/hook_failure_reason_stays_english.rs` 가 소스 스캔으로 막는다. 진입점이 `run_dynamic_client` 하나뿐인데도 가드를 둔 이유는, 서로 모르는 두 변경이 각각 그 자리에 번역문을 흘려 넣은 전례가 있기 때문이다.

기록 대상은 method 의 마지막 dot 세그먼트가 `hook` 이거나 `_hook` 으로 끝나는 요청으로 좁힌다(`claude.hook` / `codex.hook` / `claude.checklist_hook`). 대화형 CLI 실패는 사용자가 stderr 로 즉시 보므로 무흔적 문제가 없고, 파일만 시끄러워진다.

기록 위치를 `tasty_home()`(=`TASTY_HOME` 또는 `~/.tasty{-debug}`)으로 잡은 이유는 **CLI 가 닿으려 했던 대상 인스턴스의 홈**이기 때문이다 — 접속 대상은 `tasty_home()/tasty.port` 가 정하므로, 기록이 다른 곳에 남으면 사후 대조가 어긋난다. 부모가 자식 셸에 브로드캐스트하는 `TASTY_PARENT_HOME` 은 접속 대상 결정에 관여하지 않으므로 쓰지 않는다.

함께: loopback IPC connect 에 **3 초 상한**을 건다. 목적지가 항상 `127.0.0.1` 이라 평시엔 즉시 RST 로 거부되어 이 값에 닿지 않지만, RST 가 돌아오지 않는 상황(로컬 방화벽 DROP 등)에서는 OS 기본값까지 매달린다 — hook 은 턴 경계에서 동기 실행되므로 그때는 유실이 아니라 **턴 자체가 멈춘다.**

## Consequences

- **얻은 것**: 상태 push 유실이 사실로 기록된다. [ADR-0072](0072-child-state-hook-observation-fusion.md) 의 `hook_silence` 파생 판정(`confidence: heuristic`)과 대조하면 "무출력 + 그 시각 전송 실패 기록" 이 추정에서 확증으로 바뀐다. 명령 문자열 생성이 한 곳(`install::tasty_guarded_command`)으로 모여, `install.rs` 만 고치고 `profile.rs` 를 빠뜨리는 사고가 구조적으로 막힌다. hook 이 무한정 블록하지 않는다(실측: 블랙홀 포트에서 3.03s 종료, 상한 없던 형태는 15s 시점에도 진행 중).
- **잃은 것**: hook 실패가 여전히 에이전트에게는 성공으로 보인다(exit 0). 실패를 **즉시** 알아채려면 사람이나 도구가 로그 파일을 봐야 한다. 명령 문자열이 바뀌어 기존 사용자는 `tasty claude install` / `tasty codex install` 재실행이 필요하다.
- **운영 비용 / 유지 부담**: 정상 환경에서 파일은 생성조차 되지 않는다(성공은 기록하지 않는다). 고장 상황에서도 턴당 한 줄이고 상한은 512 KiB(현재 파일 + `.log.1`). 명령 문자열을 다시 바꿀 때마다 marker substring 매칭 호환을 검증해야 한다 — 깨지면 옛 entry 가 남은 채 새 entry 가 추가돼 hook 이 두 번 발화한다(회귀 테스트로 고정).

## Alternatives Considered

- **`|| true` 를 그냥 제거해 비-0 exit 를 노출** — 관측 가능성은 가장 직접적이지만, Claude Code / Codex 가 비-0 hook exit 를 어떻게 다루는지 확인할 수 없다. 턴을 차단하는 런타임이라면 "관측을 얻으려다 에이전트를 멈추는" 훨씬 나쁜 회귀가 된다. 실측 전에는 채택하지 않는다.
- **`telemetry.record` IPC 로 보고** — 호스트에 이미 있는 경로지만, 실패 원인이 IPC 불통일 때 바로 그 채널을 쓸 수 없다(chicken-and-egg). 연결이 살아있는 실패(JSON-RPC 에러)만 부분적으로 커버돼 채널이 반쪽이 된다.
- **셸 명령 안에서 직접 파일에 append** — plugin 크레이트만 고치면 되지만, 리다이렉션 대상 디렉터리가 없을 때 `2>>` 가 열기에 실패하면 **명령 자체가 실행되지 않는다.** 타임스탬프·사유를 담으려면 문자열이 길어지고, JSON/TOML 이스케이프와 Windows 셸 분기까지 얽혀 취약하다.
- **hook 명령을 재시도하게 만들기** — 유실을 줄이지만 턴 경계의 동기 실행 시간을 늘린다. 기록이 없으면 재시도가 효과가 있었는지도 알 수 없으므로, 기록이 먼저다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

- Claude Code / Codex 가 hook 의 비-0 exit 를 **무시한다는 실측**이 확보되면 — 그때는 exit code 를 그대로 노출해 사용자가 즉시 알게 하는 쪽이 낫다.
- 기록 파일이 실사용에서 의미 있는 크기로 자라면(= hook 실패가 상시 발생) — 로테이션 상한이 아니라 실패 원인 자체를 고쳐야 한다는 신호다.
- 실패 기록을 child state 판정의 입력으로 승격해 `confidence` 를 `heuristic` → `confirmed` 로 올리기로 하면 — 파일이 아닌 구조화된 채널이 필요해질 수 있다.
- loopback connect 3 초 상한에 정상 상황이 걸리는 사례가 나오면.

## References

- 부분 개정: [0164](0164-hook-failure-locale-invariance-rests-on-fields.md) (`reason` 의 값을 로케일 무관 영어로 고정한 언어 조항 개정 — 로케일 무관성을 `code` 등 좌표 필드가 지고 산문은 만든 쪽의 언어를 따른다. 이 ADR 의 나머지는 그대로 유효하다).
- [ADR-0072 child state hook observation fusion](0072-child-state-hook-observation-fusion.md) — hook 침묵을 판정 축으로 쓰는 파생 판정(상호보완, 순서 의존 없음)
- [ADR-0070 port discovery timeout](0070-port-discovery-timeout.md) — 외부 프로세스 대기에 상한을 거는 같은 계열 결정
- [dev-guide/error-handling](../dev-guide/error-handling.md) — `Result` 를 무시하지 않는 정책. 셸의 `|| true` 는 그 정책의 셸 판본이다
- [plugins/claude](../plugins/claude/index.md) · [plugins/codex](../plugins/codex/index.md) — 설치되는 hook 명령 문자열과 재설치 요구
