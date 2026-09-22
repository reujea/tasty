# ADR-0093: 에이전트 응답 중계는 화면이 아니라 transcript JSONL 을 읽고, at-least-once 로 전달한다

- **Status**: Accepted
- **Date**: 2026-09-02
- **Tags**: agent-stream, plugin, transcript, tail, relay, at-least-once, claude-plugin, focus-independence
- **Group**: agent-integration

## Context

tasty surface 에서 도는 AI 코딩 에이전트의 응답을 외부(웹 서비스)로 실시간 중계할 수단이 필요했다. 기존 수단은 셋 다 이 용도에 맞지 않는다.

- **`tasty read screen` / `read since-mark`** — pull 이고, 얻는 것은 **렌더링된 화면 텍스트**다(ANSI·박스 문자·줄바꿈 혼입). 문단 경계도 사고/응답 구분도 없다.
- **`output.observe_*` file sink** — 기록되는 것이 원문 라인이 아니라 파서가 뽑은 `ParsedItem` 이고, 에이전트의 자연어 응답은 [출력 파서 카탈로그](../reference/output-parsers.md) 어디에도 걸리지 않아 실질 빈 출력이다.
- **`output-match` 훅 + 셸 핸들러** — 라인 단위 push 는 되지만 역시 화면 텍스트이고, 라인마다 프로세스를 spawn 해 출력량이 많은 에이전트 surface 에는 비용이 크다.

한편 Claude Code 는 세션 대화를 `~/.claude/projects/<project-slug>/<session-id>.jsonl` 에 **append-only** 로 기록한다. 여기에는 사고 블록(`thinking`) · 응답 텍스트(`text`) · 툴 호출(`tool_use`) 이 이미 **구조화된 형태로 분리**돼 있다.

그 파일에 도달하는 경로도 이미 열려 있다. claude plugin 의 `SessionStart` 훅이 세션 id 를 **surface meta(`claude-session-id`)** 로 기록하므로, `surface_id → surface.meta.get → session_id → 파일` 이 host IPC 만으로 닫힌다.

## Decision

에이전트 응답 중계의 소스를 **세션 transcript JSONL** 로 정한다. 수집은 본체가 아니라 **전용 번들 plugin**(`com.tasty.agent-stream`)이 자기 프로세스의 전용 스레드에서 파일을 tail 해 수행하고, `text` / `thinking` / `tool_use` / `turn_end` 네 종류의 이벤트로 정규화한다.

이 결정에 딸린 하위 결정 다섯을 함께 못박는다.

1. **project-slug 는 계산하지 않고 탐색한다.** slug 규칙(`/` → `-`)은 우리가 소유하지 않는 외부 스펙이고, `.` 를 포함한 경로의 처리 규칙을 관찰 샘플만으로 확정할 수 없다. 파일명은 세션 id 그대로이므로 transcript 루트 한 겹 아래를 훑어 `<session-id>.jsonl` 을 찾는다.
2. **전달 보장은 at-least-once — 누락보다 중복을 택한다.** 호스트는 healthcheck 무응답 시 plugin 프로세스를 강제 재시작하므로 메모리 상태가 사라진다. watch 대상과 byte offset 을 `TASTY_PLUGIN_DATA_DIR/watches.json` 에 남겼다가 **저장된 offset 그대로** 재개한다. 마지막 flush 이후의 레코드가 다시 읽힐 수 있고, 그 중복은 소비자가 `record_uuid` 로 접는다. 프로세스가 살아 있는 동안의 중복(파일 절단/교체 후 재동기화)은 plugin 내부 uuid 캐시가 흡수한다.
3. **watch 대상은 surface_id 명시 지정만 — 와일드카드를 두지 않는다.** transcript 는 대화 전문이라, 요청하지 않은 세션까지 자동으로 tail 하면 중계 범위가 호출자의 의도를 넘는다. 또한 "지금 떠 있는 것 전부" 는 시점 의존 대상 집합이라 [포커스 독립성](../identity.md) 의 "대상은 ID 로 직접 지정" 과 어긋난다.
4. **턴 종료 판정은 정상 완료만이 아니다.** `stop_reason` 정상 종료 외에 API 오류 종료(`isApiErrorMessage`), 사용자 취소(`[Request interrupted by user…]`), 대상 surface 소멸, watch 해제, 같은 surface 재-watch 로 인한 등록 교체까지 전부 `turn_end` 이벤트로 닫는다. 정상 완료만 다루면 소비자가 영원히 다음 이벤트를 기다리는 상태가 생긴다.
5. **`turn_end.reason` 은 출처를 접두로 분리한다.** 외부(`stop_reason`) 원문은 `stop:`, 우리가 판정한 예약 사유는 `stream:` 을 붙인다. 접두가 없으면 외부 스펙이 언젠가 `session_ended` 같은 문자열을 `stop_reason` 으로 쓰기 시작했을 때 소비자가 "에이전트가 말한 것" 과 "우리가 판정한 것" 을 구분할 근거를 잃는다. 우리가 소유하지 않는 값과 소유하는 값을 한 필드에 담는 이상, 네임스페이스는 나중에 붙일 수 없다(붙이는 순간 호환이 깨진다).

## Consequences

- **얻은 것**: 화면 스크레이핑 없이 사고/응답/툴 호출이 구분된 구조화 스트림을 얻는다. 본체 코드 변경이 0 이다 — plugin 은 별도 프로세스이고 필요한 host 접점은 `surface.meta.get` / `surface.locate` 두 개뿐이다. 소스가 파일이라 에이전트 프로세스에 아무 것도 주입하지 않는다.
- **잃은 것**: transcript 레코드 스키마는 우리가 소유하지 않는 외부 형식이다. Claude Code 가 필드명을 바꾸면 정규화가 조용히 비게 될 수 있다(파싱 실패는 라인 단위로 스킵된다). 또 소비자는 중복 이벤트를 접을 책임을 진다.
- **문서 배치**: 이 기능은 [features/](../features/index.md) 에 항목을 두지 않는다. features/ 는 tasty 본체가 사용자에게 제공하는 기능의 인덱스이고, 이 수집 파이프라인은 `bundle = false` 인 plugin 이라 배포본에 들어가지 않는다 — 최종 사용자가 켤 수 있는 기능이 아직 아니다. 문서는 [plugins/agent-stream](../plugins/agent-stream/index.md) 한 곳에 둔다. 외부 방출(SSE)까지 붙어 배포 대상이 되는 시점에 features/ 항목을 신설한다.
- **운영 비용 / 유지 부담**: watch 대상 수에 비례해 300ms 주기 파일 폴링 + 3s 주기 host IPC 2 회가 든다. 대상이 없으면 두 비용 모두 0 이다. 스키마 변화는 정규화 단위 테스트가 회귀를 잡는다.

## Alternatives Considered

- **A: 화면 텍스트(`read since-mark` / `output-match`)를 파싱한다** — 이미 있는 수단이지만 렌더링 결과라 ANSI·박스 문자가 섞이고 사고/응답 구분이 사라진다. 라인마다 프로세스를 띄우는 비용도 크다.
- **B: 에이전트 프로세스의 stdout 을 가로챈다** — 에이전트는 TUI 라 stdout 이 곧 화면이다. A 와 같은 문제에 더해 PTY 를 이중으로 다뤄야 한다.
- **C: claude plugin 안에 넣는다** — 같은 요구가 codex 에도 그대로 있고 transcript 소스만 다르다. claude plugin 은 이미 최대 통합 레퍼런스라 더 부풀리지 않는다. 경로 해석이 host IPC 만으로 닫히므로 claude plugin 에 대한 코드 의존도 필요 없다.
- **D: 본체가 transcript 를 읽어 IPC 로 뿌린다** — plugin 이 별도 프로세스라 이미 파일을 직접 읽을 수 있고, 상주 폴링 루프를 본체 이벤트 루프에 얹으면 렌더 지연 위험만 는다.
- **E: 재시작 시 파일 끝에서 재개한다(at-most-once)** — 중복은 없지만 죽어 있던 동안의 응답이 조용히 사라진다. 중계 파이프라인에서는 이게 더 나쁜 실패다.
- **F: 세션 id 없이 "가장 최근 수정된 transcript" 를 고른다** — 여러 에이전트가 동시에 도는 것이 tasty 의 기본 가정이라, 엉뚱한 세션에 붙을 수 있다. 세션 id meta 가 없으면 **거부**하는 쪽을 택했다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

- Claude Code 가 세션 스트림을 **공식 API/소켓**으로 노출한다 — 그때는 파일 tail 대신 그 채널을 쓴다.
- transcript 레코드 스키마가 바뀌어 정규화가 실질적으로 비게 된다.
- 소비자 측에서 중복 제거가 불가능/과도한 부담으로 판명된다 — exactly-once 를 위한 영속 dedupe 로 옮길지 재검토한다.
- codex 등 두 번째 에이전트를 붙일 때 transcript 위치·형식 차이가 커서 단일 정규화 모델로 담기 어려워진다.

## References

- [plugins/agent-stream](../plugins/agent-stream/index.md) — 이 결정의 구현
- [dev-guide/plugin-development](../dev-guide/plugin-development.md) — §7 런타임 계약(healthcheck·강제 재시작) · §10 한계(async 미지원, 권한 게이트는 host IPC 만 막음)
- [concepts/plugins](../concepts/plugins.md) — plugin 프로세스 모델
- [features/terminal-output](../features/terminal-output/index.md) · [reference/output-parsers](../reference/output-parsers.md) — 기각한 대안 A 의 기존 수단
- [identity](../identity.md) §2.3 — 포커스 독립성(대상은 ID 로 직접 지정)
