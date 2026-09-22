# ADR-0046: 인바운드 웹훅 — owner 신뢰 모델 + 단방향 ACK/데이터·흐름 분리 불변식

- **Status**: Accepted
- **Date**: 2026-07-11
- **Tags**: webhook, inbound, security, trust-boundary, one-way-ack, data-flow-separation, ipc, owner-trust, cross-platform, adr-0004
- **Group**: webhook-hooks

## Context

tasty 는 GitHub Action 처럼 **외부 이벤트가 HTTP 로 들어오면 tasty 를 구동**하는 인바운드 웹훅 리스너를 신설했다([webhook](../features/webhook/index.md)). 이 리스너는 `0.0.0.0` 에 bind 해 공유기/OS 포워딩 너머의 외부 발신자를 받는다 — 제어용 IPC 채널(loopback 전용, [ADR-0004](0004-ipc-transport-tcp.md))과 신뢰 경계가 근본적으로 다르다. IPC 는 "같은 OS user = `_host` 전권" 이었지만, 웹훅은 **임의의 외부 발신자**가 URL 을 칠 수 있다.

핵심 질문은 "외부 발신자에게 무엇을 허용하고, 무엇으로 막느냐" 였다. 순진한 설계라면 발신자가 실행할 명령/메서드를 페이로드에 담게 하고 caller 별 allowlist 로 메서드를 제한하는 방향으로 갈 수 있다. 그러나:

- **owner(웹훅을 거는 로컬 사용자/Agent)는 이미 tasty 를 IPC 로 전권 조작할 수 있다.** 웹훅이 owner 에게 새 권한/에스컬레이션을 주지 않는다. 따라서 "owner 가 위험한 IPC 를 배선하면 위험하다 / 메서드 allowlist 로 막자" 류는 방어 대상이 아니다 — owner 는 위협 모델 밖이다.
- **유일한 위협 = 웹훅 URL 로 들어오는 외부 악의적 요청.** 그 공격자가 할 수 있는 최대치를 구조적으로 좁히는 게 설계의 전부다.
- 핸들러는 OS 셸이 아니라 **tasty IPC 만** 조작한다(기본 action = `IpcSequence`, [ADR-0047](0047-shared-hook-handler-registry-source-gate.md)). OS RCE 채널이 아니므로 HMAC 같은 하드 보안은 과설계다.

## Decision

웹훅의 방어를 **owner 신뢰 모델 + 두 불변식 + 4중 방어선**으로 구조화한다. 불변식은 "안 하기로 함" 이 아니라 **타입/함수 경계로 구조적으로 불가능하게** 만든다.

**불변식 1 — 데이터/흐름 분리.** 페이로드는 **오직 값(데이터)** 으로만 흐른다. 어떤 IPC 메서드를 어떤 순서/조건으로 부를지는 owner 가 등록 시 `IpcSequence` 에 고정하고, 외부 발신자는 그 params 의 **값 슬롯**만 채운다. `IpcCall.method` 는 owner 가 준 고정 리터럴이고, 치환 엔진(`substitute_params`)은 `method` 를 인자로 받지 않으며 **값 leaf 문자열에만** `${body.x}`/`${header.x}`/`${query.x}` 를 해소한다 — 객체 key·method 위치는 절대 치환하지 않는다. 문자열→실행 파싱 계층이 없으므로 명령 injection 여지가 없다. 조건분기가 생기더라도 데이터 비교일 뿐 코드 eval 이 아니다.

**불변식 2 — 단방향 ACK.** HTTP 응답은 **고정 상태코드 + 고정 문자열 바디**뿐이다(200/401/404/405/410/429). `build_ack(status)` 는 `IpcSequence` 실행 결과를 **인자로 받지 않아** 내부 데이터가 응답으로 샐 코드 경로 자체가 없다. 응답은 핸들러 실행 전/무관하게 확정되고(`요청 파싱 → 매칭 → build_ack 즉시 응답 → 별도 fire-and-forget 실행`), 실행은 `execute_sequence(...) -> ()` 로 결과를 버린다. 외부 발신자는 tasty 내부를 조회할 수 없다 — 웹훅은 명령/RPC/조회가 아니라 **일방적 통지**다. (owner 의 로컬 `webhook.list`/`info` 는 별개의 내부 채널이다.)

**4중 방어선**(공격자가 URL 을 쳤을 때):
1. **데이터/흐름 분리**(불변식 1) — 공격자는 값만, 메서드/흐름은 못 고른다.
2. **선택적 인증** — 웹훅별 고정 토큰(위치 4종), 미설정 시 무인증. "기대한 발신자 확인" 수준의 가벼운 체크.
3. **opaque URL** — 비순차 짧은해시(16 hex) path 로 열거 방지.
4. **남용차단** — 없는 path/메서드(404/405) 반복 출처를 임계치 초과 시 쿨다운 동안 즉시 `429`. 짧은해시 keyspace 스캔 방어.

## Consequences

- **얻은 것**: 외부 공격자의 최대 권한이 "owner 가 고정한 IPC 시퀀스에 데이터 값 채워 트리거" 로 구조적으로 상한이 걸린다. 응답 경로에 내부 데이터가 실릴 수 없어 정보 유출 표면이 없다. OS 셸 채널이 없어(웹훅=`IpcSequence` only) RCE 표면도 없다. HMAC/서명 인프라 없이도 위협 모델을 충족한다.
- **잃은 것**: 웹훅은 조회/응답형 API 로 쓸 수 없다(설계상 통지 전용). owner 가 값 슬롯에 민감한 IPC(예: 셸에 그대로 닿는 배선)를 열어두면 그 트리거 책임은 owner 몫이다 — allowlist 가 아니라 "인증을 걸어 트리거 주체를 좁히는" 도구를 제공하는 방식이다.
- **운영 비용 / 유지 부담**: 두 불변식은 코드 구조로 강제하되 리뷰·테스트로 회귀를 막아야 한다(응답 바디가 어떤 params/치환 조합에서도 고정임을 테스트로 고정). 인증이 하드 보안이 아님을 문서로 계속 명확히 해야 오용(민감 배선 + 무인증)을 예방한다.

## Alternatives Considered

- **caller/메서드 allowlist 로 방어**: "웹훅이 부를 수 있는 IPC 메서드"를 화이트리스트로 제한. — owner 는 위협이 아니고 외부 공격자는 애초에 메서드를 못 고르므로(불변식 1) 방어 가치가 없다. 유지보수 비용만 늘어 채택하지 않음.
- **페이로드가 명령 문자열/메서드를 지정**(발신자가 "무엇을 할지" 결정): CLI 명령 문자열을 파싱·실행하는 계층을 두는 방향. — 명령 injection 표면을 정면으로 여는 설계라 거부. 데이터/흐름 분리의 정반대.
- **HMAC/서명 강제 인증**: 모든 웹훅에 서명 검증 강제. — 핸들러가 OS 무관 tasty IPC 만 조작하고 데이터/흐름이 분리돼 있어 과설계. 가벼운 옵션 인증 + opaque URL + 남용차단으로 충분. 필요하면 owner 가 인증을 걸어 발신자를 좁힌다.
- **양방향 응답(내부 상태 반환)**: 발신자에게 실행 결과를 응답으로 돌려줌. — 정보 유출 표면을 만들고 "통지" 의미를 넘어선다. 응답 빌더가 실행 결과에 접근조차 못 하게 구조로 차단.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

- 웹훅 핸들러가 **OS 를 직접 건드리는 action**(셸 등)을 웹훅 source 로 허용하게 되면(현재 `ShellCommand` 는 webhook 바인딩 불가) — RCE 표면이 생기므로 인증/서명 강도를 재평가한다.
- 웹훅이 **조회/응답형 상호작용**을 요구받으면(단방향 통지 전제가 깨짐) — 단방향 ACK 불변식 전체를 재설계.
- **멀티테넌트/공유 머신**에서 owner 경계가 흔들리면([ADR-0004](0004-ipc-transport-tcp.md) 와 동일 트리거) — "owner = 전권 신뢰" 가정을 재검토.

## References

- [`features/webhook/index.md`](../features/webhook/index.md) — 웹훅 동작 전체(불변식·4중 방어선·lifetime·인증·남용차단)
- [ADR-0047](0047-shared-hook-handler-registry-source-gate.md) — 공유 훅 핸들러 레지스트리 + source 게이트(셸 웹훅 거부)
- [ADR-0048](0048-webhook-http-tiny-http-blocking.md) — HTTP 레이어(tiny_http, blocking, TLS 위임)
- [ADR-0004](0004-ipc-transport-tcp.md) — 제어용 IPC 의 loopback + owner 전권 신뢰 모델(대비되는 신뢰 경계)
- 부분 개정: [0195](0195-abuse-counting-includes-rejected-tokens.md) (4중 방어선 4 — 남용차단의 집계 대상에 `401` 추가)
- 코드: `src/webhook/{ack,auth,abuse,registry,listener}.rs`, `src/hook_handler/exec.rs`(`substitute_params`/`execute_sequence`), `src/adapters/ipc/handler/webhook.rs`
