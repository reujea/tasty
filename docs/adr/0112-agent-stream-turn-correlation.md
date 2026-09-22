# ADR-0112: agent-stream 의 턴 correlation 은 요청자 제공 `request_id` 로 하고, 턴 경계는 transcript 의 turn_end 를 그대로 쓴다

- **Status**: Accepted
- **Date**: 2026-09-04
- **Tags**: agent-stream, plugin, webhook, correlation, turn, sse, inbound, adr-0046, adr-0093, adr-0100

## Context

목표 구성은 웹 FE 가 웹훅으로 프롬프트를 보내고 그 응답을 SSE 로 받아 화면에 뿌리는 것이다. 인바운드(웹훅, [ADR-0046](0046-webhook-owner-trust-one-way-ack.md))와 아웃바운드(SSE, [ADR-0100](0100-agent-stream-sse-endpoint-exposure.md))는 **서로 다른 채널**이다. 문제는 셋이다.

- 웹훅 응답은 **고정 ACK** 뿐이다(ADR-0046 불변식 2) — 실행 결과가 응답에 실릴 코드 경로가 없다. FE 는 웹훅 응답만으로는 아무 correlation 정보를 얻지 못한다.
- transcript JSONL 레코드에는 웹훅 요청과 엮을 키가 없다(`sessionId`/`uuid` 는 Claude Code 내부 식별자라 FE 가 보낸 요청과 무관하다).
- 수집 파이프라인은 이벤트를 `turn_end` 로 닫지만(ADR-0093 결정 4), 그 턴이 **어느 요청에서 비롯됐는지** 표시하지 않는다.

즉 FE 가 다중 요청을 다루면 도착한 SSE 이벤트가 자기가 보낸 어느 요청의 결과인지 알 수 없다. 이 결정은 그 둘을 잇는 correlation 을 어떻게 세울지에 관한 것이다. 보안은 새 결정이 아니다 — 이 배선은 owner 가 여는 `IpcSequence` 이고 ADR-0046 의 owner 신뢰 모델·인증 대응책이 그대로 적용된다(`ShellCommand` 의 webhook 거부도 유지된다).

## Decision

**correlation id 는 웹훅 요청자(FE)가 제공하고, 그 값(`request_id`)이 turn 을 여는 새 IPC 메서드 `agent_stream.turn_start` 로 전달돼 그 턴이 만든 모든 이벤트(SSE·poll 공통)에 실린다. 턴 종료 경계는 이미 존재하는 transcript 의 `turn_end` 신호를 그대로 쓴다.**

- **요청자 제공.** plugin 이 id 를 자체 생성하면 FE 가 그 값을 알 방법이 없다(웹훅 응답이 고정 ACK 라 돌려줄 수 없다). 그래서 `request_id` 는 필수이고, 없으면 `turn_start` 가 거부한다. 요청자 제공이 매칭의 유일한 성립 경로다.
- **외부 입력 검증.** `request_id` 는 발신자 통제 값 leaf 라 `turn_start` 경계에서 상한을 건다: 빈 값 거부, 512 바이트 초과 거부(`request_id_too_long`), 타입은 문자열/숫자만. 상한이 없으면 거대한 값이 열린 턴에 저장돼 그 턴의 모든 이벤트(SSE·poll)에 복제되는 증폭이 된다 — 저장 단계에서 자르지 않고 거부한다. method·key 는 owner 고정 리터럴이라(값/흐름 분리) 주입 대상이 아니다.
- **값/흐름 분리 준수.** 웹훅 `IpcSequence` 는 `[turn_start(surface, ${body.request_id}), claude.tell(${body.prompt}, surface)]` 2 스텝이다. method·surface 는 owner 고정 리터럴이고 `${body.*}` 는 값 leaf 에만 치환된다(ADR-0046 불변식 1). `execute_sequence` 가 스텝을 순차 실행하므로 `turn_start` 가 `claude.tell` 보다 먼저 끝나 그 사이 이벤트가 누락 없이 태깅된다.
- **턴 경계 = transcript.** claude plugin 의 `claude-idle`(Stop) 훅을 구독하지 않는다. transcript 의 `stop_reason` 이 같은 "턴 끝" 신호를 이미 만들고(ADR-0093 결정 4), 훅 구독은 **claude plugin 이 활성일 때만** 성립하는 의존을 새로 만들기 때문이다. 프롬프트당 정확히 한 번(`tool_use` 중간 단계는 종료가 아님) 닫히고, 긴 툴이 걸려도 종료 판정에 영향이 없다.
- **한 surface 당 한 턴.** claude 는 한 번에 한 턴만 처리하므로 겹치는 `turn_start` 는 거부한다(`turn_already_open`). FE 는 앞 턴의 `turn_end`(같은 `request_id`)를 받은 뒤 다음을 보낸다 — plugin 은 요청을 큐잉하지 않는다.
- **막힌 턴 안전망.** `turn_start` 는 됐는데 `claude.tell` 이 실패해 그 턴을 닫을 transcript 이벤트가 오지 않는 경우, 비활동 타임아웃(기본 600s, 인자로 조정)이 `turn_end{reason=stream:turn_timeout}` 로 정리한다.
- **턴 밖 이벤트는 태그 없이 방출.** 사용자가 터미널에서 직접 입력한 응답 등 열린 턴 밖의 이벤트는 `request_id` 없이 나간다(버리지 않는다) — FE 는 필드 존재 여부로 "요청에서 비롯된 것인가" 를 가른다.

## Consequences

- **얻은 것**: FE 가 다중 요청을 요청↔응답으로 되짚을 수 있다. correlation 값은 SSE·`poll` 이 쓰는 **같은 직렬화 함수**에 실려 두 채널이 자동으로 일관된다. 턴 종료를 transcript 로 두어 **claude plugin 활성에 의존하지 않고**(수집 파이프라인이 이미 하는 일 위에 얹힘), ADR-0046 의 단방향 ACK 불변식도 건드리지 않는다. 본체 코드 변경이 0 이다(전부 plugin 안).
- **잃은 것**: correlation 은 **요청을 직렬화해 쓰는 전제**에서 정확하다. `execute_sequence` 가 스텝 실패에도 다음을 계속 실행하므로(fire-and-forget), 겹쳐 보내면 `turn_start` 거부 뒤에도 `claude.tell` 이 발사돼 이벤트가 앞 턴의 id 로 태깅되거나 태그 없이 나갈 수 있다 — 겹치는 경우는 best-effort 이고 직렬화가 FE 책임이다. 막힌 턴의 타임아웃은 **아무 출력 없이 타임아웃보다 오래 도는 툴**을 조기 종료할 수 있어, 그런 배치는 타임아웃을 올려야 한다.
- **운영 비용 / 유지 부담**: 열린 턴 상태는 surface 당 하나의 in-memory 엔트리이고 재시작으로 사라진다(스냅샷에 남기지 않는다 — in-flight 턴은 복구 대상이 아니다). 타임아웃 sweep 은 tail 루프의 매 tick(300ms)에 열린 턴 수만큼 도는 값싼 순회다. 웹훅 등록 예시(인증 포함)를 문서로 유지해 무인증 배선 오용을 막아야 한다.
- **남은 상류 상한**: `request_id` 상한은 이 plugin 이 저장·증폭하는 값만 막는다. 웹훅 요청 body 전체를 읽는 본체 리스너(`src/webhook/listener.rs` 의 `read_json_body`)에는 현재 body 바이트 상한이 없다(이 결정 범위 밖 — 모든 웹훅이 공유하는 인프라). **인증은 이 gap 을 막지 못한다**: 리스너는 body 를 먼저 전부 읽고(`listener.rs:96`) 그 뒤에 경로 매칭과 토큰 검증을 한다(`:98` → `:188`). 남용 차단도 걸리지 않는다 — 실패 집계 대상이 404/405 뿐이라(`:101-103`) 401 반복은 쿨다운으로 이어지지 않는다. 따라서 비-loopback 노출 시에는 **리버스 프록시의 body 크기 제한이 현재 유일한 실효 방어**이고, 그 다음은 리스너 자체를 고치는 몫이다. owner 신뢰 모델(ADR-0046)이 덮는 것은 "누가 이 IPC 를 트리거할 수 있는가" 이지 "얼마나 큰 body 를 메모리에 받는가" 가 아니다.

## Alternatives Considered

- **"웹훅 수신 ~ `claude-idle`" 구간으로 묶는 단순안(요청 id 없이)**: 단일 사용자·단일 요청이면 충분하다. 그러나 FE 가 다중 요청을 다루면 어느 응답이 어느 요청인지 여전히 구분 못 한다. 요구가 "FE 가 응답을 요청에 되짚는다" 라 채택하지 않았다 — 요청자 제공 id 만이 이를 성립시킨다.
- **plugin 이 correlation id 를 자체 생성**: 웹훅 응답이 고정 ACK 라 FE 가 그 값을 받을 길이 없어 매칭 자체가 불가능하다. 거부.
- **턴 종료를 `claude-idle` 훅 구독으로 감지**: Stop 훅은 턴 완료의 권위 있는 단일 신호다. 그러나 claude plugin 이 활성이고 그 hook 이벤트가 카탈로그에 선언돼 있어야만 성립하는 의존을 새로 만든다. transcript 가 같은 신호를 자족적으로 이미 만들므로, 그 의존을 지지 않는 쪽을 택했다.
- **턴 종료를 transcript `message.stop_reason` 이 아니라 별도 메시지 카운팅으로 판정**: 한 프롬프트가 여러 메시지를 만들 때 경계가 모호해진다. `stop_reason`(중간 `tool_use` 제외)이 프롬프트당 한 번 종료를 이미 정확히 표시하므로 불필요.
- **겹치는 턴을 큐잉하거나 덮어쓴다**: 큐잉은 claude 가 단일 턴이라 plugin 이 별도 대기열을 관리해야 하고, 덮어쓰기는 앞 턴의 늦은 이벤트가 새 id 로 잘못 태깅된다. 둘 다 correlation 정확도를 해쳐, 단순한 거부 + FE 직렬화 계약을 택했다.
- **막힌 턴을 타임아웃 없이 다음 `turn_start` 가 정리(supersede)**: 겹침을 거부하는 정책과 충돌한다(거부하면 정리 기회가 안 온다). 그래서 독립적인 비활동 타임아웃을 둔다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

- **다중 동시 요청을 한 surface 에서 지원해야 한다**(FE 가 직렬화 없이 겹쳐 보내는 것이 요구가 된다) — 한 surface 당 한 턴 전제가 깨지므로 claude 측 큐잉/멀티턴 모델과 함께 재설계한다.
- **`execute_sequence` 가 스텝 실패 시 남은 스텝을 중단하도록 바뀐다** — `turn_start` 거부가 `claude.tell` 발사를 실제로 막게 되어, 겹침 거부가 best-effort 가 아니라 hard 보장이 된다. 정책 문구를 그에 맞춰 조인다.
- **턴 종료 신호로 transcript 가 부족해진다**(예: 한 프롬프트가 여러 `end_turn` 을 내거나, tool 경계 판정이 바뀜) — `claude-idle` 훅 구독을 다시 저울질한다.
- **correlation 대상이 turn 을 넘어선다**(메시지 단위·툴 호출 단위 매칭 요구) — 태깅 입도를 재설계한다.

## References

- [ADR-0046](0046-webhook-owner-trust-one-way-ack.md) — 인바운드 웹훅 owner 신뢰 모델 · 값/흐름 분리 · 단방향 ACK · 인증 대응책
- [ADR-0093](0093-agent-response-relay-reads-transcript-jsonl.md) — transcript 수집 · at-least-once · `turn_end` 종료 판정
- [ADR-0100](0100-agent-stream-sse-endpoint-exposure.md) — SSE 엔드포인트 노출 · 토큰 저장(평문·0600)
- [`plugins/agent-stream`](../plugins/agent-stream/index.md) — 턴 correlation 절 · 등록 예시 · FE 계약
- 코드: `crates/tasty-plugin-agent-stream/src/{registry,handlers,pump}.rs`, `src/hook_handler/exec.rs`(`substitute_params`/`execute_sequence`)
- 위 "남은 상류 상한" 이 적어 둔 남용차단 쪽 빚(`401` 반복이 쿨다운으로 이어지지 않던 것)은 [ADR-0195](0195-abuse-counting-includes-rejected-tokens.md) 가 갚았다. JSON 입력 body 상한과 `413` 거부는 [ADR-0200](0200-webhook-body-has-a-per-request-byte-cap.md)에 따라 구현됐다. 연결 정리의 잔여 읽기는 별도 요구로 [ADR-0281](0281-webhook-parser-cap-and-connection-drain.md) (Superseded by ADR-0516)에 기록됐고, [ADR-0516](0516-the-webhook-413-closes-the-connection-through-a-vendored-tiny-http-patch.md) 이 413·429 에서 잔여 body 를 읽지 않고 연결을 닫는 것으로 충족했다. 위 Consequences는 이 ADR의 결정 당시 상태이며, 현재 운영 상태는 [웹훅 문서](../features/webhook/index.md)를 따른다. 이 연결 I/O 발견으로 턴 correlation 계약을 미구현으로 되돌리지 않는다.
