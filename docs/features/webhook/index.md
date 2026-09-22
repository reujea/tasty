# 웹훅 (Inbound webhook listener)

- **Status**: Implemented
- **주체**: 로컬 사용자 · AI Agent (`webhook.*`, local-only)
- **ADR**: [ADR-0046](../../adr/0046-webhook-owner-trust-one-way-ack.md)(신뢰 모델·불변식) · [ADR-0047](../../adr/0047-shared-hook-handler-registry-source-gate.md)(공유 핸들러 레지스트리)
- **코드**: `src/webhook/`(리스너·레지스트리·lifetime·인증·남용차단·영속화) · `src/adapters/ipc/handler/webhook.rs`(IPC) · `crates/tasty-cli/src/commands/webhook.rs`(CLI)
- **화면**: 없음 — headless 전용(경고는 기존 toast/`tracing::warn!` 재사용)

## 목적

GitHub Action 처럼 **외부 이벤트가 HTTP 로 들어오면 tasty 를 구동**하는 경량 인바운드 서버다. tasty 의 제어용 IPC 포트(loopback 전용, [ADR-0004](../../adr/0004-ipc-transport-tcp.md))와 별개로, `0.0.0.0` 의 설정 포트를 열어 외부 발신자의 통지를 받는다. 실제 외부→내부 포워딩은 공유기/OS 몫이고, tasty 가 제공하는 건 "특정 포트에 특정 규칙으로 데이터가 들어오면 지정 핸들러를 구동하라" 이다.

대표 흐름: tasty 의 어떤 기능이 외부에 작업을 걸어둠 → 그 작업이 완료/오류나면 외부가 웹훅으로 통지 → tasty 가 내부적으로 반응(예: `notification.create`). 웹훅 응답은 "잘 전달됨" ACK 뿐이다.

## 내부 동작 (headless-valid)

### 싱글턴 라우터

프로세스당 리스너는 **단 하나**다. 다수의 웹훅 등록을 opaque path 로 멀티플렉싱하며, 개별 웹훅은 port/path 를 지정하지 못한다 — 리스너가 발급·은닉한다. 라우팅 키는 `(port, opaque path)` 로, 현재는 단일 포트만 실사용한다.

- **opaque id**: 등록 시 랜덤 8바이트 → 16 hex 소문자(`gen_opaque_id`). **비순차**라 열거를 막고, keyspace 스캔은 남용차단으로 보완한다.
- **발급 URL**: `http://{host}:{port}/{id}`. `0.0.0.0`/빈 호스트는 표기상 `127.0.0.1` 로 치환.

### 요청 처리 흐름

`tiny_http`([ADR-0048](../../adr/0048-webhook-http-tiny-http-blocking.md))가 bind + accept 하고, 요청마다 worker 스레드에서:

1. **남용차단 선검사** — 출처 IP 가 쿨다운 중이면 즉시 `429`. 경로 파싱·헤더 수집·**JSON 입력 읽기보다 앞**이라, 차단된 출처의 body 를 `read_json_body` 의 입력 버퍼로 모으거나 JSON 으로 파싱하지 않는다. HTTP 라이브러리의 사전 버퍼링과 응답 후 drain 은 이 타입 경계 밖이다. 그 순서는 타입으로 강제된다(선검사를 통과한 요청만 JSON 입력 파서를 호출할 수 있다 — [ADR-0199](../../adr/0199-the-block-is-decided-before-the-body-is-read.md)).
2. path/query 분리, 헤더 소문자 정규화, **JSON 입력을 요청당 상한으로 제한**해 파싱한다(실패 시 `null`). 선언된 `Content-Length` 가 상한을 넘으면 이 파서 함수에서 body 를 읽지 않고, 길이 선언이 없는 chunked 는 판정용 1바이트를 포함해 상한 + 1 바이트까지만 모은다. 어느 방식이든 상한 초과는 `413`([ADR-0200](../../adr/0200-webhook-body-has-a-per-request-byte-cap.md)).
3. **매칭 + 인증**(`match_request`) — path 없음 `404`, lifetime 만료 `410`(lazy 삭제), 메서드 불일치 `405`(카운트 미차감), 인증 불일치 `401`(**카운트 미차감**). 인증 검증은 호출자가 넘긴 술어로 **같은 lock 안에서** 차감보다 **먼저** 한다 — `CountLimit` 이 세는 단위가 "시퀀스를 돌린 횟수" 라서, 시퀀스를 0 번 돌린 요청이 통을 태우면 안 된다. 통의 키는 토큰이 아니라 **등록**이라(모든 발신자가 한 통을 공유한다) 그 차이는 인증을 통과하지 못한 발신자가 owner 의 예산을 태우는 것으로 나타난다.
4. 매칭·인증을 모두 통과하면 카운트 1 차감(소진되면 삭제).
5. **출처 실패 집계** — `404`/`405`/`401`/`413` 을 이 출처의 실패로 센다(`counts_as_failure` → `record_failure`). 통이 둘이고 답도 둘이다: 4 번의 예산(`CountLimit`)에서 401 은 **세면 안 되고**(세는 것이 소모다) 이 통에서는 **세야 한다**(제한이라 안 세는 것이 우회다) — [ADR-0195](../../adr/0195-abuse-counting-includes-rejected-tokens.md).
6. **ACK 즉시 응답**(`build_ack`) — 여기까지 핸들러 실행과 무관.
7. **fire-and-forget** — `execute_sequence` 로 핸들러(IpcSequence)를 메인 루프에 전달. 결과는 응답으로 되돌리지 않는다.

### 단방향 ACK (불변식)

HTTP 응답은 **고정 상태코드 + 고정 문자열 바디**뿐이다. `build_ack(status)` 는 IpcSequence 실행 결과를 **인자로 받지 않아** 내부 데이터가 응답에 실릴 코드 경로 자체가 없다([ADR-0046](../../adr/0046-webhook-owner-trust-one-way-ack.md)).

그래서 **`200` 은 매칭돼 넘겼다는 뜻이지 실행이 됐다는 뜻이 아니다.** 응답은 실행 전에 확정되므로 IpcSequence 가 통째로 실패해도 발신자는 `200` 을 받고, 한 스텝이 실패해도 다음 스텝이 계속 가서 **부분 적용이 정상 종료 상태로 남을 수 있다**(`execute_sequence` — MVP 는 조건분기가 없다). 실패의 유일한 관측점은 `tracing::error!` 로그다. 외부 발신자는 상태코드로 재시도를 정하므로 이 성질이 곧 계약이다.

| 상태 | 코드/바디 | 트리거 |
|------|-----------|--------|
| `Received` | 200 `received` | 매칭 성공 |
| `Unauthorized` | 401 `unauthorized` | 인증 설정됐으나 토큰 미제시/불일치 |
| `NotFound` | 404 `not found` | path 없음 |
| `MethodNotAllowed` | 405 `method not allowed` | 메서드 불일치 |
| `Gone` | 410 `gone` | lifetime 만료(lazy 삭제) |
| `PayloadTooLarge` | 413 `payload too large` | body 가 요청당 상한 초과 |
| `TooManyRequests` | 429 `too many requests` | 남용차단 쿨다운 출처 |

### lifetime 6종

`Lifetime { persistence, limit }` = **{영속성 2} × {제한 3}**.

- **persistence**: `Persistent`(재시작 후 config 복원) / `Temporary`(재시작 시 소멸).
- **limit**: `Unlimited` / `TimeLimit { deadline_unix }`(절대 시각) / `CountLimit { remaining }`. (기존 훅의 `once` = `CountLimit{remaining:1}` 에 해당.)

**만료 집행은 타이머 없이 3시점**에서만 확정된다 — ① 호출 시 lazy 판정(`match_request` 가 만료면 삭제 + `410`), ② 재시작 복원 시 만료 엔트리 필터, ③ 명시적 `webhook.sweep`. 한 번도 안 불린 시간제한 웹훅은 그때까지 등록 상태로 남되, 호출되면 즉시 만료 응답한다.

### 영속화 (`~/.tasty/webhooks.toml`)

`Persistent` 웹훅만 저장한다(`Temporary` 는 저장 안 함). 저장 항목: `id`, `methods`, `handler`(또는 인라인 `sequence`), `limit`(kind + `deadline_unix`/`remaining`), `auth`. `TimeLimit` deadline 은 절대 Unix 시각이라 재시작 후에도 정확히 만료한다. 재시작 복원(`restore_into_registry`)은 이미 만료된 엔트리를 등록하지 않고 파일에서 정리한다. 최상위 `port` 키(포트 설정)과 `[[webhook]]` 배열이 같은 파일을 공유하며, 각각의 writer 가 상대 섹션을 보존한다.

### 선택적 인증 (가벼운 발신자 확인)

**서명 검증은 구현돼 있지 않다.** 발신자를 가리는 수단은 비순차 opaque path 와, 걸었을 때만 동작하는 고정 공유 토큰 둘뿐이다 — 발신자가 HMAC 서명 헤더를 실어 보내도 읽는 코드가 없다. 그 부재는 의도된 결정이고(핸들러가 OS 를 못 건드리고 tasty IPC 만 조작한다는 위협모델, [ADR-0046](../../adr/0046-webhook-owner-trust-one-way-ack.md)), 리스너가 `0.0.0.0` 에 bind 한다는 사실과 함께 읽어야 한다 — 인증을 걸지 않은 웹훅은 그 URL 에 닿는 누구든 발화시킨다.

인증 자체는 웹훅별 옵션이다 — 걸면 지정 위치의 고정 토큰이 일치해야 통과하고, **미설정이면 무인증 통과**한다. tasty 는 인증을 강제하지 않는다.

위치 4종(`AuthLocation`): `QueryKey`(`?key=<token>`) · `BearerHeader`(`Authorization: Bearer <token>`) · `BodyField`(바디 JSON 점구분 경로의 문자열 leaf) · `HeaderKey`(임의 헤더). 토큰 비교는 **상수시간**(`ct_eq`)이고, 조회 응답은 위치·키 이름만 노출하고 **토큰 값은 절대 반환하지 않는다**(`auth_summary`). 그 은닉은 **IPC 응답 경로에만** 걸린다 — `Persistent` 웹훅의 토큰은 `~/.tasty/webhooks.toml` 에 **평문**으로 들어간다. 파일 권한을 코드가 명시적으로 거는 자리는 없고, unix 에서 `0600` 이 되는 것은 `atomic_write` 가 쓰는 `tempfile` 의 기본 모드가 `persist` 를 넘어 그대로 남기 때문이다 — 의도해 건 것이 아니라 **부수효과**라, 쓰기 경로를 평범한 `fs::write` 로 바꾸면 umask 값으로 조용히 넓어진다.

### body 상한 (요청당)

JSON 처리에 허용하는 요청 body 는 **기본 1 MiB** 까지다. `read_json_body` 는 선언된 `Content-Length` 가 상한을 넘으면 입력을 읽지 않고 거부한다. 선언 길이가 없는 chunked 는 판정용 1바이트를 더해 최대 `상한 + 1`바이트를 모으며, UTF-8 변환 전에 길이를 검사한다. 무효 UTF-8도 초과하면 `413 payload too large`이고 시퀀스는 실행하지 않는다. 상한 이내의 무효 UTF-8·비-JSON은 기존대로 `null`이다. 초과 판정은 경로 매칭·인증보다 앞이다.

**이 상한은 HTTP 연결 전체의 I/O나 프로세스 전체 메모리 상한이 아니다.** 입력 버퍼에 저장하는 바이트 수, JSON 표현의 부가 메모리, HTTP 라이브러리의 연결 정리 비용은 서로 다르다. `tiny_http` 0.12.0의 일반 Content-Length 리더는 `Request`를 정리할 때 미읽은 나머지를 읽어 버린다(drain). 이때 잔여 선언 길이만큼 임시 버퍼를 요청하며, 완료·EOF·오류까지 기다릴 수 있다. 413이 먼저 전달되거나 서버 송신 방향의 EOF를 받아도 서버 수신 방향의 정리가 끝났다는 뜻은 아니다. 이 잔여 읽기를 끊는 지원 API는 현재 리스너에 없다.

chunked의 디코더 Drop에는 같은 Content-Length drain 루프가 없다. 그러나 parser가 읽은 양을 라이브러리의 버퍼링·다음 요청 처리까지 포함한 전체 소켓 읽기량으로 일반화하지 않는다. 선행 가정 오류와 현재 보장·미충족 transport 요구는 [ADR-0281](../../adr/0281-webhook-parser-cap-and-connection-drain.md)에 둔다. 잔여 읽기 없이 거부 연결을 종료한다는 목표는 유지되며, 문서 정정이 그 요구를 구현한 것은 아니다.

- `TASTY_WEBHOOK_MAX_BODY_BYTES` 로 조정한다(0·파싱 실패는 기본값). 수십 MB 짜리 payload 를 보내는 발신자를 붙이려면 이 값을 올린다.
- **요청당 JSON 입력 상한이다** — 동시 요청 수·연결 수·총 메모리·drain 시간은 이 값으로 제한하지 않는다.
- `Content-Length` 가 1024 이하인 body 는 HTTP 라이브러리가 요청을 만드는 단계에서 먼저 읽으므로, 이 상한도 남용차단도 그 구간에는 닿지 않는다.

### 남용차단 (일시 거부)

매칭·인증에 실패한 요청을 반복하는 출처를 일시 거부해, 짧은해시 keyspace 스캔·스팸과 **토큰 무차별 대입**을 막는다. 순수 코어 로직(`AbuseTracker`, 시각 주입)이라 결정론적으로 테스트된다.

- **무엇이 실패인가는 `counts_as_failure` 하나가 정한다** — 없는 path(`404`) · 메서드 불일치(`405`) · 인증 불일치(`401`) · body 상한 초과(`413`) 를 세고, 정상 매칭(`200`) · 만료(`410`) · 이미 쿨다운(`429`) 은 안 센다. 410 을 빼는 이유는 만료 URL 에 **추측할 공간이 없기** 때문이고(같은 URL 을 몇 번 두드려도 얻는 정보가 0), 401 을 넣는 이유가 정확히 그 반대다. 와일드카드 없는 전수 `match` 라 ACK 상태가 늘면 그 자리에서 컴파일이 멈춘다.
- 한 출처가 `window`(기본 10초) 내 `threshold`(기본 20)회 이상 실패하면 `cooldown`(기본 60초) 동안 즉시 `429`. 정상 200 응답은 집계하지 않으므로 정상 웹훅 트래픽은 영향받지 않는다.
- 임계치/윈도우/쿨다운은 환경변수 오버라이드(`TASTY_WEBHOOK_ABUSE_THRESHOLD` / `_WINDOW_SECS` / `_COOLDOWN_SECS`). 기본값의 근거와 출처 키 선택은 [ADR-0196](../../adr/0196-abuse-thresholds-and-source-key.md).
- **차단은 정확히 `cooldown` 만큼이다** — 차단 중에 더 두드려도 만료가 밀리지 않고, 풀릴 때 카운터가 백지가 된다. 그래서 토큰·경로를 잘못 설정한 정상 발신자는 개입 없이 스스로 돌아온다. 카운팅 창은 슬라이딩이 아니라 고정이라, 창 경계에서는 짧은 순간 임계치의 두 배 가까이가 통과할 수 있다([ADR-0198](../../adr/0198-a-cooldown-is-fixed-at-entry.md)).
- **출처 표에는 절대 상한이 없다.** `PRUNE_TRIGGER_SOURCES`(4096)는 상한이 아니라 **정리를 시도할 크기 문턱**이다 — 윈도우 안 엔트리는 보존되므로 그 수만큼 표가 자란다. 크기를 정하는 것은 유입률 × 윈도우이고, 유입이 멎으면 다음 실패 한 건이 표를 회수한다. 문턱을 넘은 뒤의 정리 순회는 윈도우당 한 번만 돈다([ADR-0197](../../adr/0197-the-source-table-cap-is-a-prune-trigger.md)).
- **카운터는 in-memory 다 — 재시작하면 남은 쿨다운이 사라진다.** 디스크에 남기지 않으므로 tasty 를 다시 띄운 직후의 출처는 전부 백지에서 시작한다(`webhooks.toml` 에 저장되는 것은 `Persistent` 웹훅 등록이지 남용 상태가 아니다).

### 포트 설정 (설정값 only)

리스너 포트는 **오로지 설정값**에서 온다 — tasty 가 임의 포트로 몰래 대체 bind 하지 않는다(자동 폴백 없음).

- 설정 파일이 처음 없으면 시드 포트 `28429`(User Ports 범위 임의값, 알려진 서비스 포트 아님)를 기록한다.
- 포트가 비면 리스너를 띄우지 않고 경고한다(`PortNotConfigured`). bind 실패(충돌/권한)도 경고하고 사용자가 설정을 고치게 위임한다(`BindFailed`, 자동 회피 없음).
- 경고는 **기존 인프라 재사용** — GUI 는 toast(`ToastManager`), headless 는 `tracing::warn!`. 신규 디자인 컴포넌트가 없어 화면이 없다.

### 부팅 초기화

공용 헬퍼 `webhook::init_from_config(injector)` 를 두 진입점에서 호출한다 — GUI 는 `window_lifecycle` 의 `start_ipc`/injector 확보 직후, headless 는 `boot` 의 IPC 시작 이후. 두 전제(core config 로드 + 메인 루프 IPC 처리 가능)를 만족한 시점이다. 중복 호출은 리스너 내부 bind 가드로 무해하다. init 후 `Persistent` 웹훅을 복원한다.

## 인터페이스

전부 **`local_only`** — plugin 은 호출할 수 없고 CLI/로컬 클라이언트만 가능하다. 포커스 독립(대상은 `id` 로 지정, `list` 는 전 범위 순회).

| IPC | CLI | 동작 |
|-----|-----|------|
| `webhook.register` | `tasty webhook register` | 필요 메서드 + (`--handler <id>` xor `--sequence <json>`) + lifetime + 선택 인증 → `{id, url, ...}` 반환. `--method` 를 생략하면 CLI 는 `methods` 를 `null` 로 보내 서버 기본값 `POST` 가 선다(빈 배열은 서버가 거절한다). `--sequence` 가 JSON 으로 안 읽히면 CLI 가 요청을 보내지 않고 인자 이름과 원인을 찍은 뒤 종료 코드 1 로 끝난다 |
| `webhook.list` | `tasty webhook list` | 전체 목록(각 항목 URL·메서드·steps·lifetime·인증여부) |
| `webhook.info` | `tasty webhook info --id <id>` | 단일 상세 |
| `webhook.unregister` | `tasty webhook unregister --id <id>` | 등록 해제(path 회수) |
| `webhook.sweep` | `tasty webhook sweep` | 만료 웹훅 일괄 정리 → 제거된 id 목록 |
| `webhook.config` | `tasty webhook config [--port <N>]` | 포트 조회/설정(설정은 재시작 후 반영) |

- **register 게이트**: `methods` 빈 배열 거부, `handler`/`sequence` 정확히 하나. `handler` 는 `validate_binding(handler, Webhook)` 로 검증 — 셸/hook-전용 핸들러는 거부([ADR-0047](../../adr/0047-shared-hook-handler-registry-source-gate.md)). 인라인 `sequence` 는 익명 핸들러(`user/wh-<slug>`)로 레지스트리에 등록된다.
- **lifetime 파라미터**: `--persistent`(bool), `--ttl-secs` xor `--count`(둘 다 없으면 `Unlimited`).
- **auth 파라미터**: `--auth-location <query|bearer|body|header>` + `--auth-token`(상호 requires), bearer 외에는 `--auth-key`.
- **핸들러**가 소비하는 페이로드→params 치환·source 게이트는 [공유 훅 핸들러 레지스트리(ADR-0047)](../../adr/0047-shared-hook-handler-registry-source-gate.md) 참조.
- **핸들러 레지스트리 GUI**: [Settings › Handler › Hook Handlers](../settings/screens/settings.md) 서브탭에서 레지스트리(host 기본 + plugin 기여 + user 매핑)를 조회·편집한다(토글/셸 명령 인라인 편집/user 행 추가·제거, `~/.tasty/hook-handlers.toml` 영속). **제거는 user 행만** — host/plugin 행은 그 자리에 자물쇠 글리프가 오고, 지워도 finalize 가 되살린다. `IpcSequence` 행은 mono 한 줄 요약만 두고 GUI 편집 경로가 없다 — 시퀀스 본문은 [`tasty hook-handler get`/`upsert`](../hooks/index.md#핸들러-레지스트리-hook_handler) 로 고친다(TOML 손편집 + `reload` 도 그대로 된다). **고쳐도 이미 등록된 웹훅은 안 바뀐다** — 엔트리가 등록 시점 스냅샷을 소유하므로 다시 등록해야 한다. **리스너(bind/port/secret) 설정은 이 서브탭에 없다** — 위 CLI(`webhook.config`) 전용.

## 비-목표 (Out of scope)

- **HTTPS/TLS 종단** — 리버스 프록시/공유기에 위임(사용자 요구가 "포워딩은 OS/공유기 몫").
- **외부 발신자의 조회/응답 채널** — 응답은 ACK 전용. 내부 상태 조회는 로컬 소유자 채널(`list`/`info`)로만.
- **웹훅에서의 OS 셸 실행** — 셸(`ShellCommand`)은 기존 훅(source `hook`) 전용, 웹훅 바인딩 불가. 셸 핸들러가 훅 트리거/수동 발화로 실행될 때 받는 `TASTY_HOOK_*` env 목록은 [hooks 문서의 셸 핸들러 환경변수 절](../hooks/index.md#셸-핸들러-환경변수-tasty_hook_) 참조.
- **plugin 의 웹훅 등록** — 현재 `webhook.*` 는 local-only(plugin 미노출).
- **plugin 프로세스의 직접 소켓 소유** — 코어가 소켓을 소유한다.
- **웹훅 외 프로토콜(raw TCP 등)** — HTTP 웹훅으로 확정.

## Acceptance Criteria

- Given 포트 설정됨 When `webhook.register --method POST --sequence '<ipc-seq>'` Then 발급 URL 반환, `curl -XPOST` 시 IpcSequence 가 실행되고 응답은 고정 ACK 바디만.
- Given 등록된 웹훅 When `webhook.unregister` 후 그 path 호출 Then `404`.
- Given `CountLimit{remaining:N}` When 접수되는 호출을 N+1 회 Then N 회 후 소멸(다음 호출 `410`/`404`).
- Given `CountLimit` + 인증 설정된 웹훅 When 토큰 불일치 호출 Then `401` 이고 **remaining 은 그대로**.
- Given `TimeLimit` deadline 경과 When 호출 또는 `webhook.sweep` Then `410` + 삭제.
- Given 인증 설정된 웹훅 When 토큰 불일치 Then `401`; 미설정 웹훅은 무인증 통과.
- Given 없는 path 를 임계치 초과 반복 When 같은 출처 재요청 Then 쿨다운 동안 `429`(정상 웹훅 무영향).
- Given 인증 설정된 웹훅 When 같은 출처가 틀린 토큰을 임계치 초과 반복 Then 쿨다운 동안 `429`.
- Given `ShellCommand` 핸들러 When 웹훅 바인딩 시도 Then source 게이트로 거부.

## 관련

- [hooks](../hooks/index.md) — 내부 이벤트 트리거(웹훅과 대칭인 trigger 출처) · [notifications](../notifications/index.md) · [file-handler](../file-handler/index.md)(레지스트리 정본 템플릿)
- [api](../../reference/api.md#기타-호스트) · [ADR-0046](../../adr/0046-webhook-owner-trust-one-way-ack.md) · [ADR-0047](../../adr/0047-shared-hook-handler-registry-source-gate.md) · [ADR-0048](../../adr/0048-webhook-http-tiny-http-blocking.md)
