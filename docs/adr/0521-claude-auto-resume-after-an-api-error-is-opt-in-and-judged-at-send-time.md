# ADR-0521: API 에러로 끝난 Claude 턴의 자동 재개는 opt-in 이고, 보내는 순간의 사실로 판정한다

- **Status**: Accepted
- **Date**: 2026-09-23
- **Tags**: claude-plugin, stop-failure, auto-resume, settings, defaults, typing-guard, identity-principle-1, adr-0072, adr-0266
- **Group**: agent-integration

## Context

Claude Code 는 내장 재시도를 다 쓴 뒤 API 에러(`529 Overloaded` · 5xx 등)로 턴이 끝나면 `Stop` 대신 `StopFailure` 훅을 쏘고 입력을 기다린다. claude plugin 은 그 훅으로 상태를 `idle` 로 닫고 에러 종류를 surface meta `claude-last-stop-failure` 에 남긴다([docs/plugins/claude/index.md](../plugins/claude/index.md) "Claude Code 훅 통합"). 사람이 없는 세션(무인 conductor · 자식 Claude)은 그 자리에서 방치된다. 사용자 요구는 "이 상태를 감지해 몇 초 뒤 재개 문구를 자동으로 보내는 것" 이었다.

제약:

- **불가침 원칙 1**([identity.md](../identity.md) §2.1): 에이전트 행동이 사용자 상태에 닿으면 안 된다. 재개는 사용자 입력 재현이 아니라 에이전트 기능(`terminal.tell`, release 에 있는 텍스트 전송)이지만, 사용자가 그 surface 입력창에 초안을 쓰고 있으면 재개 문구가 초안 뒤에 붙어 **함께 제출된다** — 그것은 사용자 상태 침범이다.
- 호스트의 `surface.is_typing` 은 `typing`(최근 5 초 안의 키 입력) 과 함께 `idle_seconds`(마지막 사용자 키 입력 뒤 경과, 없으면 `-1`)를 준다. 사용자 키 입력은 GUI 의 키보드·IME 경로만 기록한다 — 에이전트의 `send`/`tell` 은 기록하지 않는다.
- `terminal.tell` 에는 타이핑 가드가 없다. `surface.send_wait_idle` 에는 있지만 본문만 보내고 Enter 를 나눠 보내지 않는다.
- 설정 페이지 항목 종류는 `toggle` · `select` · `number` · 폰트뿐이다 — 자유 문자열 입력이 없다.
- 에러 종류는 Claude Code 가 `StopFailure` matcher 로 선언한 값(`rate_limit` · `overloaded` · `authentication_failed` · `billing_error` · `invalid_request` · `server_error` · `max_output_tokens` · `model_not_found` · `unknown` …)이다. 실측(2026-09-23, Claude Code 2.1.280): 로컬 게이트웨이가 돌려준 `529 overloaded_error` 는 `server_error` 로 분류돼 왔다.

## Decision

1. **기본값은 꺼짐이다.** `auto_resume_enabled`(toggle, 기본 `false`) · `auto_resume_delay_secs`(number, 기본 10, 최소 1, 최대 86400 — 하루. 매니페스트 `max` 는 설정 UI 만 막으므로 손으로 고친 설정 파일을 위해 코드도 같은 범위로 자르고, 그래도 시계가 넘치면 예약하지 않는다) · `auto_resume_max_attempts`(number, 기본 5, 최소 1). 사용자 세션에 텍스트를 넣는 기능이라 opt-in 이고, 켜지 않은 사용자의 동작은 바뀌지 않는다.
2. **재개 대상 에러는 `overloaded` 와 `server_error` 둘로 고정한다.** `rate_limit` 은 넣지 않는다 — 한도가 풀리는 시각이 분~시간 단위라 몇 초 뒤의 재개는 다시 실패하고 실패마다 요청을 한 건 더 쓴다. 나머지는 다시 보내도 같은 결과라 사람이 고쳐야 한다.
3. **재개 문구는 i18n 고정 문장이다**(`claude.auto_resume.message`, plugin 활성 locale). 사용자 지정 문구는 두지 않는다.
4. **판정은 만기 시점에 한다.** 예약(`stop-failure` 수신 시, 설정 on · 대상 에러일 때)은 만기 시각만 적고, 만기 스레드가 보내기 직전에 다음을 모두 다시 확인한다 — 설정이 여전히 켜짐 · `terminal.state` 가 `idle` · 전경이 `claude` 이고 pid 가 예약 때와 같음 · 사람 입력 없음 · 연속 상한 미만. 전경 이름은 호스트가 OS 에서 받은 값 그대로라 Windows 에서는 `claude.exe` 이고 대소문자도 보장되지 않는다 — 소문자로 바꾸고 끝의 `.exe` 를 뗀 뒤 `claude` 와 비교한다(셸 판정 `is_known_shell_name` 과 같은 형태). 새 턴 신호(`prompt-submit` · `session-start` · `active`) · 성공 `stop` · `session-end` 는 예약을 지우고, 보내기 직전 턴 번호를 한 번 더 대조한다. 서브에이전트(`agent_id` 가 실린)의 `StopFailure` 는 메인 턴이 계속되므로 예약하지 않는다.
5. **사람 입력 판정은 "실패한 턴이 시작된 뒤 키 입력이 한 번이라도 있었나" 다.** `idle_seconds` 를 그 턴의 `prompt-submit` 이후 경과와 견줘, 작으면 취소한다(턴 시작을 모르면 예약 시각부터). 창을 예약 시각이 아니라 턴 시작에서 여는 이유는, Claude 가 일하는 동안 입력창에 써 둔 미제출 초안도 똑같이 재개 문구 앞에 붙기 때문이다. 이 조건을 통과했는데 `typing` 이 참이면 5 초 미루고 시도 수를 쓰지 않는다.
6. **제출 경로는 `terminal.tell` 이다** — 본문 write 확인 → 정착 지연 → Enter 를 나눠 보내고, 원격 attach 가 점유한 surface 는 거절한다. 거절되면 다시 걸지 않는다.
7. **연속 상한에 닿으면 보내지 않고 `notification.create` 로 한 번 알린다.** 계수는 성공 턴과 새 세션에서 0 이 되고, 재개 문구 자신이 부르는 `prompt-submit` 은 계수를 지우지 않는다. 보낸 횟수는 surface meta `claude-auto-resume-count` 에 남는다(계수가 0 이 되는 자리 — 성공 턴 · 새 세션 · 세션 종료 — 가 지운다). 부모의 완료 로그에는 따로 줄을 쓰지 않는다 — 실패한 턴마다 이미 에러 종류가 붙은 완료 줄이 간다.

### 서브에이전트의 `StopFailure` 는 무시한다

`agent_id` 가 실린 `StopFailure` 는 상태를 `idle` 로 닫지도, 재개를 예약하지도 않는다(`crates/tasty-plugin-claude/src/hook.rs` 의 `is_subagent_stop_failure`). `Stop` 에는 서브에이전트용 `SubagentStop` 이 따로 있지만 `StopFailure` 는 하나뿐이고, 서브에이전트가 실패해도 메인 턴은 그 실패를 tool 결과로 받고 계속 돈다 — 그것을 턴 종료로 받으면 일하는 세션을 입력 대기로 오보고하고, 부모에게 거짓 완료 알림이 가고, 일하는 중인 입력창에 재개 문구를 넣으려 든다.

- **근거의 종류**: 설치된 Claude Code **2.1.280** 바이너리의 `StopFailure` payload 조립부를 정적으로 읽은 것이다 — 그 조립부가 질의 루프 공용이라 서브에이전트 문맥에서도 불릴 수 있고, 공통부의 `agent_id` 는 서브에이전트 문맥에서만 값이 있다.
- **서브에이전트 실패 때 `StopFailure` 가 실제로 오는지: 미측정.** 서브에이전트 안에서 API 에러를 일으켜 훅 payload 를 받아 본 적이 없다. 안 온다면 이 분기는 죽은 코드이고 해가 없다. 온다면 이 분기가 위 오보고를 막는다. 어느 쪽인지 레포 안의 채널로는 안 보인다.
- **대안 — 메인 턴 종료로 취급**(`agent_id` 를 보지 않고 모든 `StopFailure` 를 `idle` 로 닫고 재개를 예약): 판단이 단순하지만, 서브에이전트 실패가 실제로 온다면 위 세 결함이 그대로 난다. 반대로 무시하는 쪽이 틀리는 경우는 "메인 턴이 끝났는데 `agent_id` 가 실려 온다" 뿐이고, 그것은 `agent_id` 의 정의(서브에이전트 문맥에서만 값이 있음)와 어긋난다. 틀렸을 때 비용이 작은 쪽을 골랐다.

## Consequences

- **얻은 것**: 무인 세션이 일시적 서버 과부하에서 스스로 회복한다. 판정이 보내는 순간의 사실이라, 예약 뒤 설정을 끄거나 사람이 먼저 입력하거나 Claude 를 끄면 아무것도 안 나간다. 초안이 있는 입력창에는 끼어들지 않는다.
- **잃은 것**: 기본 꺼짐이라 켜야 쓴다. rate limit 으로 멈춘 세션은 자동으로 안 깨어난다. 재개 문구를 바꿀 수 없다. 창 안에 한 번이라도 키 입력이 있으면(초안을 다 지웠더라도) 재개하지 않는다 — 입력창 내용을 읽을 채널이 없어 보수적으로 멈춘다.
- **붙여넣은 초안도 사용자 입력 가드에 걸린다.** 가드가 기대는 좌변(`surface.is_typing` 의 `idle_seconds`)은 [ADR-0560](0560-paste-is-user-input-and-is-recorded-where-both-paste-paths-meet.md) 이 정한다 — 키보드 · IME · 붙여넣기(단축키 · 명령 팔레트). 이 항목은 처음에 "붙여넣기 단축키는 `record_typing` 을 안 지나므로 붙여넣기만 한 초안은 가드에 안 걸린다" 로 적혀 있었고 실측과 달랐다: 수식키가 붙은 붙여넣기 단축키는 수식키 키다운이 먼저 기록했고, 실제로 뚫린 것은 키가 surface 에 닿지 않는 **명령 팔레트 붙여넣기**였다(0560 Context 의 실측). 파일 드롭은 새 탭을 열 뿐 입력창에 쓰지 않아 좌변 밖이다.
- **관측된 것 — 부모의 완료 줄은 재개가 예약된 턴에도 "입력 대기" 라고 적는다.** 실패한 턴의 완료 줄에 붙는 힌트(`claude.notify.stop_failure_hint`, en: "the turn ended on an API error (…) and is waiting for input")는 자동 재개가 켜져 곧 재개 문구가 갈 때도 같다. 고치지 않는다 — 알림은 빠지지 않고(재개된 턴이 끝나면 완료 알림이 다시 무장돼 한 줄이 더 간다), 문구를 켜짐/꺼짐으로 가르면 그 줄을 파싱하는 부모 쪽에 외부 동작 변경이 생긴다.
- **운영 비용 / 유지 부담**: plugin 에 스레드가 하나 는다(500 ms 주기, 만기된 예약이 있을 때만 IPC). `terminal.tell` 의 본문과 Enter 사이(수백 ms)에 사용자가 입력하면 섞일 수 있다 — `terminal.tell` 이 이미 받아들인 창과 같은 크기이고 더 좁히지 않았다.

## Alternatives Considered

- **기본 켜짐** — 무인 세션에는 편하지만, 기존 사용자의 세션에 예고 없이 텍스트가 들어가기 시작한다. 호환을 깨는 쪽이라 기각.
- **화면의 에러 문자열로 감지** — 형식이 바뀌면 깨지고, 재개 뒤에도 옛 에러 줄이 화면에 남아 무한 재개를 만든다. `StopFailure` 는 턴 종료를 구조적으로 알리고 종류를 준다.
- **사용자 훅(`hook.set --event claude-stop-failure`)에 맡김** — `Custom` 이벤트 payload 는 이벤트 키만 실어 에러 종류가 안 가고, 훅은 surface 마다 등록해야 한다.
- **`surface.send_wait_idle` 로 제출** — 타이핑 가드가 있지만 `typing`(5 초 창)만 보고, Enter 를 분리하지 않아 제출 확인 성질이 없다.
- **`is_typing` 을 폴링해 한 번이라도 참이면 취소** — `idle_seconds` 가 마지막 입력 시각을 직접 주므로 폴링 없이 같은 답을 더 정확히 얻는다.
- **`rate_limit` 포함** — 위 결정 2.
- **예약 시점에 한 번만 판정** — 지연 동안 바뀐 사실(설정 · 사람 입력 · 프로세스 종료)을 못 본다.

## Reconsideration Triggers

**채널이 붙는 것** — 판정 시점에 레포가 읽을 수 있는 사실이다.

- 설정 페이지에 자유 문자열 항목 종류가 생긴다 → 사용자 지정 재개 문구를 재검토한다. 재는 법: `crates/tasty-plugin-manifest/src/types.rs` 의 `SettingsItemDecl` 변형.
- 호스트가 입력창의 미제출 내용(또는 "초안 있음")을 알려 주는 IPC 를 갖는다 → 결정 5 의 보수적 취소를 그 사실로 바꾼다.
- 본체의 사용자 입력 좌변([ADR-0560](0560-paste-is-user-input-and-is-recorded-where-both-paste-paths-meet.md))이 바뀐다 — 입력창에 내용을 넣는 새 사용자 경로가 기록 없이 생기거나, 기록하는 경로가 빠진다 → 결정 5 의 가드가 그 경로의 초안을 놓치는지 다시 본다. 재는 법: 0560 의 재검토 조건 셋(사용자 출처 `SendToSurface` 라벨 · 파일 핸들러 액션 종류 · `run_paste` 밖의 붙여넣기 진입점).
- Claude Code 가 서브에이전트용 실패 이벤트(예: `SubagentStopFailure`)를 따로 갖거나, `StopFailure` payload 조립부가 서브에이전트 문맥에서 불리지 않게 바뀐다 → "서브에이전트의 `StopFailure` 는 무시한다" 의 분기를 재검토한다(앞쪽이면 이벤트 이름으로 가르고, 뒤쪽이면 분기를 지운다). 재는 법: 설치된 Claude Code 바이너리의 `hook_event_name:"StopFailure"` 조립부와 훅 이벤트 목록. 근거가 특정 버전(2.1.280)의 정적 판독이므로 Claude Code 를 올릴 때마다 다시 읽는다.
- `StopFailure` payload 에 재시도 대기 시각(예: rate limit 해제 시각)이 실린다 → `rate_limit` 을 그 시각 기준으로 재개하는 것을 재검토한다. 재는 법: 설치된 Claude Code 바이너리의 `hook_event_name:"StopFailure"` 조립부.

**원리적으로 안 붙는 것** — 사람이 관측해야 한다.

- 실제 Anthropic API 의 529 가 `overloaded` 로 오는지 `server_error` 로 오는지, 그리고 둘 외의 값으로 오는 일시적 에러가 있는지. 재는 법: `StopFailure` 에 payload 를 파일로 덤프하는 사용자 훅을 하나 더 걸고 과부하 시점의 `error` 값을 본다.
- 서브에이전트 안에서 난 API 에러에 `StopFailure` 가 실제로 오는지(현재 미측정). 재는 법: `StopFailure` payload 를 파일로 덤프하는 사용자 훅을 걸고, 서브에이전트(Agent 툴)가 API 에러를 맞는 상황에서 `agent_id` 가 실린 payload 가 오는지 본다. 온 적이 없으면 그 분기는 "근거 없음" 이 아니라 "관측 안 됨" 으로 남는다.
- 켜 둔 사용자가 "원치 않는 재개" 를 보고한다 → 결정 5 의 창과 결정 4 의 확인 목록을 재검토한다.

## References

- [docs/plugins/claude/index.md](../plugins/claude/index.md) — "Claude Code 훅 통합"(StopFailure) · "API 에러 뒤 자동 재개"
- `crates/tasty-plugin-claude/src/auto_resume.rs` — 판정(`judge`) · 예약 표 · 만기 스레드
- [ADR-0560](0560-paste-is-user-input-and-is-recorded-where-both-paste-paths-meet.md) — 결정 5 가 읽는 사용자 입력의 좌변(키보드 · IME · 붙여넣기)
- [ADR-0072](0072-child-state-hook-observation-fusion.md) · [ADR-0266](0266-derived-stale-must-reach-the-push-channel.md) — 상태 전이는 훅이 push 하고 파생 상태는 출력 전용이라는 계약(재개는 상태를 쓰지 않는다)
- 선행 결정 없음 — 탐색: `git grep -l 'is_typing\|자동 재개\|auto.resume\|StopFailure' -- docs/adr/` (0288 · 0291 은 Codex 완료 채널이라 무관)
