# 웹훅 (Inbound webhook listener)

## 원칙과 현재 동작

[실패·복구 원칙](../../principles/feedback-and-recovery.md)에 따라 webhook 실패와 기본 터미널 작업의 지속 가능성을 구분한다. 명시 포트를 열지 못하면 다른 포트로 옮기지 않고 리스너 없이 실행하며, 원인과 영향 범위를 경고 알림으로 남긴다. 실행 결과는 발신자에게 돌려주지 않고(아래 ACK 불변식) 실패를 알림으로 남긴다.

- **Status**: Implemented
- **주체**: 로컬 사용자 · AI Agent (`webhook.*` — `register` 만 plugin·Agent 허용(Agent 는 `network` 권한), 나머지 local-only)
- **설계**: 이 문서(신뢰 모델·불변식) · [훅 (Surface / Global hooks)](../hooks/index.md)(공유 핸들러 레지스트리)
- **코드**: `src/webhook/`(리스너·레지스트리·lifetime·인증·남용차단·영속화) · `src/adapters/ipc/handler/webhook.rs`(IPC) · `crates/tasty-cli/src/commands/webhook.rs`(CLI)
- **화면**: 전용 화면 없음. GUI와 headless에서 동작하며 시작 경고와 실행 실패는 알림 저장소에 남긴다

## 목적

GitHub Action 처럼 **외부 이벤트가 HTTP 로 들어오면 tasty 를 구동**하는 경량 인바운드 서버다. tasty 의 제어용 IPC 포트(loopback 전용, [IPC 서버](../../architecture/ipc-server.md))와 별개의 HTTP 포트로 통지를 받는다. 기본은 이 컴퓨터(127.0.0.1)에서 오는 호출만 받고, 설정 `[webhook] allow_external = true`(Settings › General · `tasty webhook allow-external on`)를 켜면 모든 IPv4 인터페이스(0.0.0.0)에서 받는다. 실제 외부→내부 포워딩은 공유기/OS 몫이고, tasty 가 제공하는 건 "특정 포트에 특정 규칙으로 데이터가 들어오면 지정 핸들러를 구동하라" 이다.

대표 흐름: tasty 의 어떤 기능이 외부에 작업을 걸어둠 → 그 작업이 완료/오류나면 외부가 웹훅으로 통지 → tasty 가 내부적으로 반응(예: `notification.create`). 웹훅 응답은 "잘 전달됨" ACK 뿐이다.

## 내부 동작 (headless-valid)

### 싱글턴 라우터

프로세스당 리스너는 **단 하나**다. 다수의 웹훅 등록을 opaque path 로 멀티플렉싱하며, 개별 웹훅은 port/path 를 지정하지 못한다 — 리스너가 발급·은닉한다. 라우팅 키는 `(port, opaque path)` 로, 현재는 단일 포트만 실사용한다.

- **opaque id**: 등록 시 랜덤 8 바이트 → 16 hex 소문자(`gen_opaque_id`). **비순차**라 열거를 막고, keyspace 스캔은 남용차단으로 보완한다.
- **발급 URL**: `http://{host}:{port}/{id}`. host·port는 리스너가 실제로 bind한 주소(`RuntimePorts`의 웹훅 기록)에서 온다. `0.0.0.0` bind는 표기상 `127.0.0.1` 로 적는다. 리스너가 없으면(탐색 범위가 모두 막힘, 명시 지정 포트 실패) 포트 없이 `http://127.0.0.1/{id}` 로 적는다. 이때 등록과 저장한 Persistent 웹훅은 그대로 남지만 URL은 응답하지 않으며, 포트를 고쳐 다시 시작하면 같은 id로 다시 받는다. 저장한 Persistent 웹훅의 URL도 재시작 뒤 그 실행의 포트를 따른다.

### 요청 처리 흐름

`tiny_http`가 HTTP를 받고 요청별 worker가 다음 순서로 처리한다.

1. remote IP가 cooldown 중이면 429로 끝낸다. 선검사를 통과한 `Screened`만 application body를 읽는다.
2. 경로·query·헤더를 정리하고 body 크기를 검사한다. 기본 1MiB를 넘으면 413이며, 상한 이내의 잘못된 UTF-8·JSON은 null로 취급한다.
3. 프로세스 registry에서 등록 identity와 인증을 확인하고 순서가 있는 변경을 접수한다. 처리 차례에 identity·lifetime·method를 다시 확인하며 인증 거절은 CountLimit를 차감하지 않는다.
4. 매칭·인증에 성공하면 남은 횟수 변경을 기존 file writer에 제출하고 receipt를 기다린다. 대기는 HTTP 요청 스레드에서 하며 registry 잠금과 메인 루프를 점유하지 않는다. 이 횟수는 접수된 시퀀스 수이며 내부 step 성공 횟수가 아니다.
5. 401·404·405·413을 출처 실패에 집계한다. 200·410·429·503은 세지 않는다.
6. 고정 ACK를 보내고 별도 실행을 진행한다. 413과 선차단 429는 `respond_and_close`로 연결을 닫는다.

외부 값은 params의 문자열 leaf에 `${body.x}`·`${header.x}`·`${query.x}`로 치환한다.
메서드명·객체 key·실행 순서는 owner가 고정한다. HTTP 응답에 시퀀스 결과를 넣지 않는다.
직접 ShellCommand를 바인딩할 수 없지만 owner가 고른 IPC의 효과까지 자동으로 안전해지는 것은 아니다.

### 단방향 ACK (불변식)

HTTP 응답은 **고정 상태코드 + 고정 문자열 바디**뿐이다. `build_ack(status)` 는 IpcSequence 실행 결과를 **인자로 받지 않아** 내부 데이터가 응답에 실릴 코드 경로 자체가 없다(이 문서).

그래서 **`200` 은 매칭돼 넘겼다는 뜻이지 실행이 됐다는 뜻이 아니다.** 응답은 실행 전에 확정되므로 IpcSequence 가 통째로 실패해도 발신자는 `200` 을 받고, 한 스텝이 실패해도 다음 스텝이 계속 가서 **부분 적용이 정상 종료 상태로 남을 수 있다**(`execute_sequence` — 현재는 조건 분기가 없다). 실패는 로그와 함께 알림으로 남는다(아래 "실행 실패 알림"). 발신자 응답은 바뀌지 않는다. 외부 발신자는 상태코드로 재시도를 정하므로 이 성질이 곧 계약이다.

| 상태 | 코드/바디 | 트리거 |
|------|-----------|--------|
| `Received` | 200 `received` | 매칭 성공 |
| `Unauthorized` | 401 `unauthorized` | 인증 설정됐으나 토큰 미제시/불일치 |
| `NotFound` | 404 `not found` | path 없음 |
| `MethodNotAllowed` | 405 `method not allowed` | 메서드 불일치 |
| `Gone` | 410 `gone` | lifetime 만료(lazy 삭제) |
| `PayloadTooLarge` | 413 `payload too large` | body 가 요청당 상한 초과 |
| `TooManyRequests` | 429 `too many requests` | 남용차단 쿨다운 출처 |
| `Unavailable` | 503 `unavailable` | registry 접수 거절·결과 불명. 차감·시퀀스 실행 없음 |


### 실행 실패 알림

ACK를 보낸 뒤 실행한 IpcSequence가 성공하지 못하면 `src/hook_runtime/notice.rs`가 알림 하나를 남긴다. 대상은 전역 시스템 알림 선례대로 활성 workspace, `surface_id: 0`이고 GUI `notification.created`의 `source`는 `webhook`이다. 본문은 웹훅 id와 성공하지 못한 스텝(순번·메서드·사유)을 적는다.

| 경우 | 레벨 | 제목 키 |
|---|---|---|
| 성공한 스텝이 없음, 또는 injector가 없어 실행하지 못함 | 오류 | `hook_notice.webhook_failed` |
| 일부 스텝만 실패 | 경고 | `hook_notice.webhook_partial` |
| 실패는 없지만 응답 대기(10초)가 끝나 결과를 모르는 스텝이 있음 | 경고 | `hook_notice.webhook_unknown` |
| 응답할 호출자가 없는 webhooks.toml 저장 실패 | 경고 | `hook_notice.webhook_save_title` |

정상 실행은 알림을 남기지 않는다. 같은 실패가 반복되면 같은 본문의 알림은 10초에 한 번만 남는다([알림](../notifications/index.md#작업-스레드의-시스템-알림)).

### lifetime 6종

`Lifetime { persistence, limit }` = **{영속성 2} × {제한 3}**.

- **persistence**: `Persistent`(재시작 후 config 복원) / `Temporary`(재시작 시 소멸).
- **limit**: `Unlimited` / `TimeLimit { deadline_unix }`(절대 시각) / `CountLimit { remaining }`. (기존 훅의 `once` = `CountLimit{remaining:1}` 에 해당.)

**만료 집행은 타이머 없이 3시점**에서만 확정된다 — ① 호출 시 lazy 판정(`match_request` 가 만료면 삭제 + `410`), ② 재시작 복원 시 만료 엔트리 필터, ③ 명시적 `webhook.sweep`. 한 번도 안 불린 시간제한 웹훅은 그때까지 등록 상태로 남되, 호출되면 즉시 만료 응답한다.

### 영속화 (`~/.tasty/webhooks.toml`)

`Persistent` 웹훅만 저장한다(`Temporary` 는 저장 안 함). 저장 항목: `id`, `methods`, `handler`(또는 인라인 `sequence`), `limit`(kind + `deadline_unix`/`remaining`), `auth`. `TimeLimit` deadline 은 절대 Unix 시각이라 재시작 후에도 정확히 만료한다. 재시작 복원(`restore_into_registry`)은 이미 만료된 엔트리를 등록하지 않고 파일에서 정리한다. 최상위 `port`(명시 지정 포트)·`format`(파일 형식 표시)과 `[[webhook]]` 배열이 같은 파일을 공유하며, 각각의 writer 가 상대 섹션을 보존한다. host의 배열 교체와 standalone CLI의 포트 변경은 같은 참여자 파일 잠금 안에서 최신 TOML을 읽어 해당 섹션만 바꾼다. 읽기·파싱 실패면 어느 writer도 쓰지 않는다. 외부 편집이 읽기 뒤 바뀌었으면 교체 전에 fingerprint·링크 대상을 대조해 거절한다(마지막 확인과 rename 사이 경합은 남는다). TOML 주석·공백의 바이트 보존은 보장하지 않는다. Persistent 등록·해제는 저장 성공 receipt 뒤에만 메모리에 공개하고 원 IPC 호출자에게 한 번 응답한다. 저장 실패·접수 거절은 이전 등록을 유지한 채 `-32603`, `applied:false/saved:false`로 답한다. 앞선 결과를 정산한 상태에서 다음 webhook 절을 구성하므로 실패한 등록이 뒤의 저장에 다시 섞이지 않는다. 만료·횟수 차감·복원 정리의 알려진 backend 저장 실패에는 기존처럼 경고 로그와 경고 알림(제목 `hook_notice.webhook_save_title`, source `webhook`)을 남기고 이번 실행의 차감·제거를 적용한다. HTTP는 이 알려진 실패를 확인한 뒤 기존 ACK·시퀀스 정책을 따른다. 따라서 재시작에는 해당 차감·제거가 반영되지 않을 수 있다. 명시적 sweep은 실제 제거 ID와 저장이 실패했으면 `saved:false`를 반환한다. 접수 거절과 결과 불명은 이 계속 실행 정책에 포함하지 않는다. HTTP는 고정 `503 unavailable`로 끝내고 차감·실행하지 않는다. 결과 불명은 IPC의 `saved:null/outcome_unknown:true`로 알리며 재실행하지 않고, 그 registry의 후속 변경도 새 프로세스 복원 전까지 거절한다. 저장 포트 읽기는 경고 후 없는 것으로 다루고, 포트 저장·삭제는 오류로 끝난다.

`system.pressure.webhook_registry`는 coordinator 접수·거절·정산 건수, pending, 결과·HTTP 문맥의 보관 개수·추정 바이트, closing, dirty, outcome_unknown만 표시한다. 접수 건수는 HTTP 200이나 파일 교체 성공 횟수가 아니다. dirty는 알려진 저장 실패 뒤 메모리에 남은 영속 변경이며 다음 성공한 전체 절 저장에서 해제한다. Unknown은 재시작 복원 전까지 유지한다. 경로·ID·요청 원문·인증 값은 지표에 싣지 않는다.

종료는 registry와 listener의 새 접수를 먼저 닫는다. 이미 접수된 파일 명령·IPC 응답·HTTP 실행 문맥과 accept 스레드를 폴링한 뒤 공용 file writer를 닫는다. 다른 파일 소비자의 collector도 계속 진행하며 메인 루프에서 join을 기다리지 않는다. 아직 접수하지 않은 네트워크 body reader 전체의 join을 보장하는 계약은 아니다. 보관 작업이 모두 회수돼도 dirty·Unknown이 남으면 저장 성공으로 설명하지 않고 별도 종료 경고를 남긴다. 미회수는 기존 종료 기한의 경고로 구분한다.

### 선택적 인증 (가벼운 발신자 확인)

인증은 등록별 선택 사항이다. 없으면 URL에 도달한 누구나 작업을 촉발할 수 있다.
고정 공유 토큰을 확인하며 HMAC·서명 검증은 지원하지 않는다. TLS도 리스너가 직접 제공하지 않는다.

| 위치 | 입력 |
|------|------|
| QueryKey | 지정 query key |
| BearerHeader | Authorization: Bearer 토큰 |
| BodyField | JSON 점 구분 경로의 문자열 값 |
| HeaderKey | 지정 HTTP header |

비교는 `ct_eq`를 사용한다. 조회 응답은 위치·key 이름만 보여주고 토큰 값은 반환하지 않는다.
Persistent 토큰은 `webhooks.toml`에 평문으로 저장된다. 새 Unix 파일은 0600으로 만들고, 기존 파일은 공용 교체 함수가 native 권한·소유권을 보존한다. 파싱 오류 로그에 파일 원문을 넣지 않는다.
저장 경로를 바꾸면 권한도 함께 확인해야 하며 이 응답 은닉을 저장 암호화로 해석하지 않는다.

### body 상한 (요청당)

JSON 입력은 기본 1MiB이며 `TASTY_WEBHOOK_MAX_BODY_BYTES`의 양수로 조정한다. 0·파싱 실패는 기본값이다.
유효 Content-Length가 상한을 넘으면 application body 읽기 전에 거절한다.
chunked·길이 미상은 상한+1 바이트를 읽어 초과를 구별하며 UTF-8 변환 전에 판정한다.
초과 요청은 시퀀스를 실행하지 않고 CountLimit도 차감하지 않는다.

413과 선차단 429는 vendor/tiny_http의 `Request::respond_and_close`를 쓴다.
Connection:close를 보내고 공유 종료 flag로 EqualReader Drop의 잔여 읽기와 길이 비례 allocation을 막는다.
큰 미완료 body에서는 상대의 body·FIN 없이 worker와 소켓 양방향을 정리한다.
거절 연결은 재사용하지 않는다. body가 전송 중이면 RST 때문에 client가 ACK를 받지 못할 수도 있다.

이 값은 application 입력 버퍼 상한이며 전체 메모리·동시 요청·연결·JSON 표현·IPC 치환 후 크기의 상한은 아니다.
Content-Length≤1024는 라이브러리가 요청 생성 전에 먼저 읽는다.
이 작은 body에서 client가 close 헤더를 무시하면 이미 다음 헤더를 기다리던 연결은 기존 idle keep-alive처럼 남을 수 있다.

vendor 업데이트는 응답 수신과 worker·수신 방향 종료를 별도로 확인한다.
`listener_body_tests.rs`의 raw_oversize·raw_blocked_source 검사와 정상 keep-alive·chunked·SSE를 함께 본다.
사본의 상류 보안 수정은 수동으로 추적하며 자동 advisory 검사가 모두 대조한다고 가정하지 않는다.
패치 범위는 [PATCHES.md](../../../vendor/tiny_http/PATCHES.md)에 있다.

### 남용차단 (일시 거부)

키는 소켓의 remote IP다. 포트는 제외하며 X-Forwarded-For를 출처로 그대로 사용하지 않는다.
같은 NAT·proxy 뒤 발신자는 실패 수와 cooldown을 공유할 수 있다.

- `counts_as_failure`는 401·404·405·413만 센다. 인증 실패는 이 제한에 포함되지만 등록의 CountLimit를 줄이지 않는다.
- 기본 10 초 고정 창에서 실패 20 회가 되면 60 초 cooldown으로 들어가 이후 같은 IP 요청을 429로 거절한다.
- `TASTY_WEBHOOK_ABUSE_THRESHOLD`, `TASTY_WEBHOOK_ABUSE_WINDOW_SECS`, `TASTY_WEBHOOK_ABUSE_COOLDOWN_SECS`로 양수를 지정한다.
  미설정·0·파싱 실패는 기본값이며 threshold는 u32 최댓값 이내로 제한한다.
- 차단 중 재시도는 종료 시각을 연장하지 않는다. 만료 확인 때 카운터와 창 시작을 초기화한다.
- 고정 창 경계에는 짧은 구간에 `2×threshold−1`회 실패가 처리될 수 있다.
  창마다 threshold 미만으로 반복하면 cooldown에 들어가지 않으므로 모든 패턴의 일정 처리율 상한이나 짧은 토큰의 안전을 보장하지 않는다.
- `PRUNE_TRIGGER_SOURCES=4096`은 표 크기 상한이 아니다. 기준을 넘으면 최대 창마다 한 번만 훑고
  cooldown 중이거나 창 안의 항목을 보존한다. 많은 출처로 표가 커질 수 있다.
  timer로 비우지 않고 다음 실패에서 정리 조건을 확인한다.
- 상태는 메모리에만 있으며 재시작하면 없어진다. Persistent webhook 등록과 별개다.

정상 200은 실패 수를 올리지 않지만 이미 같은 IP가 차단돼 있으면 정상 요청도 429를 받는다.
IPv6·proxy 출처 처리나 실제 메모리 제한을 추가할 때는 차단 항목을 밀어내 우회하지 못하도록 함께 설계한다.

### 리스너 주소와 포트


**주소**: 기본 `127.0.0.1`. config.toml의 `[webhook] allow_external = true`면 `0.0.0.0`(`webhook::bind_ip`). 다음 실행부터 적용된다.

**포트 결정 순서** (`webhook::bind`):

1. 실행 인자 `--webhook-port <N>`(GUI·headless 공통). 있으면 명시 지정이다.
2. 데이터 폴더 `webhooks.toml`의 `port`(`tasty webhook port <N>`으로 저장). 인자가 없을 때만 쓰며 명시 지정이다.
3. 둘 다 없으면 탐색: `28429`부터 하나씩 올리며 bind를 시도해 처음 성공한 포트를 쓴다. 상한은 64개(`28429..=28492`, `PROBE_COUNT`).

- 점유 여부는 미리 조회하지 않고 실제 bind 결과로 판단한다.
- **명시 지정 포트를 bind하지 못하면 리스너 없이 실행한다**(`WebhookInitReport::ExplicitUnavailable`). 다른 포트로 옮기지 않는다. 터미널과 IPC 등 나머지 기능은 그대로 쓸 수 있다. 원인(포트·OS 오류)·영향(웹훅 URL 무응답)·포트를 바꾸는 방법(인자로 지정했으면 `--webhook-port`, 저장 값이면 `tasty webhook port <N>`·`--unset`)을 경고 알림 한 건으로 남기고, `webhook.config` 조회의 `unavailable`에도 남긴다. 로그에는 `webhook listener bind ... failed` 오류 한 줄이 함께 남는다.
- 탐색 범위가 모두 막히면 리스너 없이 실행하고 경고 알림을 남긴다(`WebhookInitReport::Unavailable`).
- 탐색으로 `28429`가 아닌 포트를 얻었고 복원한 Persistent 웹훅이 있으면 경고 알림을 남긴다(`MovedWithPersistent`). 새 URL은 `tasty webhook list`로 확인한다. 저장한 웹훅이 없으면 경고하지 않는다.
- 포트는 열었지만 요청을 받을 accept 스레드를 띄우지 못하면 소켓을 닫고 리스너 없이 실행한다(`ListenerFailed`). 이때 포트를 기록하지 않으므로 `bound: false`이고, 원인은 `unavailable`과 경고 알림(`webhook.warn.listener_failed`)에 남는다.
- **시작 경고의 전달**: 위 네 경우 모두 토스트가 아니라 알림 저장소의 경고(`level: warning`)로 남긴다. 리스너 시작은 사용자의 즉각적인 입력이 아니기 때문이다. GUI는 첫 윈도우가 등록된 뒤 그 윈도우의 활성 workspace에, headless는 활성 workspace에 저장한다(`src/system_notice.rs`의 대기열을 메인 루프가 옮긴다). 제목은 리스너가 없는 경우 `webhook.warn.title`("이번 실행에서 웹훅이 꺼졌습니다"), 포트만 바뀐 `MovedWithPersistent`는 `webhook.warn.moved_title`("웹훅 포트가 바뀌었습니다")이고 GUI `notification.created` 이벤트의 `source`는 `webhook.start`다. 웹훅 요청의 실행 실패와는 이 source로 구분한다. 정상 bind(`Bound`)는 알림을 남기지 않는다.
- 같은 본문이 GUI·headless 모두 `tracing::warn!` 로그에도 남는다.
- bind한 포트는 설정 파일에 써 넣지 않는다. 써 넣으면 다음 실행부터 명시 지정이 되기 때문이다.
- **Tasty 없이 바꾸기**: `tasty webhook port <N>`·`tasty webhook port --unset`(이전 이름 `tasty webhook config --port <N>`)은 실행 중인 인스턴스가 있으면 IPC `webhook.config`로 저장한다. 포트 파일(`tasty.port`)이 없거나 그 포트로의 연결이 거절되면(인스턴스 없음) CLI가 webhooks.toml을 직접 고치고 `{saved_port, applies_from: "next_start", instance_running: false, file, message}`를 출력한다. 연결 시간 초과 같은 다른 실패는 고치지 않고 오류로 끝난다. 연결은 됐지만 상대가 Tasty로 응답하지 않으면(JSON-RPC 오류가 아닌 실패: 연결 재설정, JSON이 아닌 응답) 다른 프로그램이 그 포트를 쥐었을 수 있으므로 파일을 고치지 않고, 원래 오류 아래에 그 가능성과 webhooks.toml의 `port` 줄을 직접 고치거나 남은 포트 파일을 지우는 방법을 안내한 뒤 종료 코드 1로 끝난다(`cli.webhook.port_not_tasty`). 파일 규칙(형식 표시, 이전 자동 기록 포트 정리, 다른 키 보존)은 `tasty_settings::webhook_port_file` 한 곳에 있고 인스턴스와 CLI가 같이 쓴다. 저장한 포트가 막혀 리스너가 꺼졌을 때 경고 알림이 이 명령을 알려 준다. 인자 없는 조회는 실행 중인 인스턴스가 있어야 한다.
- **이전 형식 파일**: 이전 버전은 파일이 없을 때 `port = 28429`를 자동으로 써 넣었다. `format = 2` 표시가 없는 파일의 `port = 28429`는 그 자동 값으로 보고 처음 읽을 때 지운다(탐색으로 바뀐다). 다른 값은 사용자가 정한 명시 지정으로 남긴다. 이 모듈이 파일을 쓸 때마다 `format = 2`를 함께 쓴다.
- 같은 컴퓨터에서 release·debug·격리 홈 인스턴스를 함께 띄우면 각 인스턴스가 비어 있는 다음 포트를 쓴다(측정: 다른 프로세스가 28429를 쥔 상태에서 격리 인스턴스가 28430).

**단일 실행 넘김**: release에서 같은 데이터 폴더의 Tasty가 이미 실행 중이고 두 번째 실행에 `--webhook-port`가 있으면, 실행 중인 쪽에 `webhook.config`로 포트를 묻는다. 다르면(리스너가 없는 경우 포함) 넘기지 않고 메시지 상자로 알린 뒤 종료 코드 1로 끝낸다. 같으면 평소처럼 넘긴다. 조회하지 못하면 막지 않고 평소처럼 넘기며, 확인하지 못했다는 경고를 `tracing::warn!`과 실행 기록(launch log)에 남긴다.

**값이 저장되는 파일**:

| 값 | 파일 | 바꾸는 방법 | 적용 |
|---|---|---|---|
| 외부 수신 허용 `allow_external` | config.toml `[webhook]` | Settings › General(Save/Cancel), `tasty webhook allow-external on\|off` | 다음 실행 |
| 명시 지정 포트 `port` | webhooks.toml 최상위 | `tasty webhook port <N>`·`--unset`(Tasty 없이도 가능) | 다음 실행 |
| 파일 형식 표시 `format` | webhooks.toml 최상위 | 포트를 쓸 때 자동 | — |
| Persistent 등록 `[[webhook]]` | webhooks.toml | `tasty webhook register --persistent`·`unregister` | 즉시 |
| 이번 실행만의 포트 | 저장 안 함 | `--webhook-port <N>` | 그 실행 |

외부 수신 허용은 설정 윈도우의 저장·취소 흐름을 따르도록 config.toml에, 포트는 인스턴스 없이도 고칠 수 있도록 웹훅 파일에 둔다.

실행 중 `webhook.config`의 port 조회·저장·삭제는 같은 파일 writer의 ordered command로 처리한다.
조회에서 필요한 옛 형식 마이그레이션도 worker 안에서 끝낸 뒤 원 호출자에게 한 번 응답한다.
메인 루프는 파일 잠금을 기다리지 않으며 `allow_external`은 기존 설정 snapshot 완료 경계를 쓴다.
포트 저장 오류는 `saved:false`, 결과를 잃은 경우는 `saved:null/outcome_unknown:true`로 구분하고
명령을 재실행하지 않는다. 실행 중 리스너 주소는 바꾸지 않는다. 조회 성공은 저장 성공 표시가
아니며, 읽기·마이그레이션 실패 시 backend의 기존 경고와 메모리 해석 규칙을 따른다.
등록·해제·Count/Time·sweep은 같은 file writer에 연결된 프로세스 소유 registry coordinator를 쓴다. 등록 목록은 engine마다 복사하지 않는다. pending 명령은 합치거나 재실행하지 않고, 앞선 receipt를 정산한 뒤 다음 절을 구성한다. 별도 보관의 개수와 추정 직렬화 바이트는 공용 writer 기본 queue 한도(4096건·16MiB)로 제한하며, 실행 중 절과 결과를 포함한다. HTTP 문맥의 보관 예산은 ACK·시퀀스 종료까지 유지한다. 종료 중 새 시퀀스 IPC는 GUI·headless 모두 종료 오류로 답하며, 이미 정리 중인 engine에서 실행하거나 응답 대기자를 버리지 않는다. 이는 등록의 인증·수명 정책이나 전체 HTTP 연결·프로세스 메모리 상한이 아니다.

### 포트 기록 (`RuntimePorts`)

이 인스턴스가 bind에 성공한 IPC 포트와 웹훅 포트를 `src/runtime_ports.rs`의 `RuntimePorts`가 한 번씩 기록한다. 포트가 필요한 곳(웹훅 URL, `webhook.config` 조회, 자기 자신 attach 거절, 인스턴스 파일)은 여기서 읽는다.

- `AppServices`가 GUI·headless 공통 앱 초기화에서 하나 만들어 소유한다. 프로세스 전역 static으로 두지 않아, 한 시험 프로세스가 여러 `AppServices`를 병렬로 만들어도 서로의 기록을 보지 않는다. 웹훅 레지스트리는 리스너 시작 때 같은 기록을 넘겨받는다.
- 기록은 bind 성공 뒤에만 한다. 각 항목은 주소와 정한 방법(`argument`·`config`·`probe`·`dynamic`)을 가진다.
- 데이터 폴더의 `tasty.port` 파일은 매니저가 아니라 IPC 서버가 bind 시점에 직접 쓴다(`src/adapters/production/tcp_ipc_server.rs`). 이전 완료 알림 로그(notify)를 지운 뒤에 주소를 공개하는 순서를 서버 안에서 지키기 위해서다. 값은 매니저에 기록한 것과 같은 bind 결과(`listener.local_addr()`)다.

### 부팅 초기화

GUI(`boot::run_gui`)와 headless(`boot::run_headless`)는 설정을 읽은 직후 `webhook::prepare(--webhook-port, &settings.webhook)`로 소켓을 선점하고 `Hub::webhook`에 둔다. IPC와 injector가 준비되면 `webhook::start`가 초기 파일을 읽어 `Persistent` 웹훅을 복원한다. 만료 항목 정리의 file receipt까지 정산한 뒤 그 소켓으로 accept를 시작하며, 성공했을 때만 포트를 기록한다. 복원과 file receipt 사이에는 요청을 접수하지 않는다(GUI `src/app/boot_machine.rs`, headless `boot::start_ipc_and_seed`).

## 인터페이스

`webhook.register` 는 plugin 도 호출할 수 있고(Agent 는 `network` 권한), 나머지는 **`local_only`** — CLI/로컬 클라이언트만 가능하다. 포커스 독립(대상은 `id` 로 지정, `list` 는 전 범위 순회).

| IPC | CLI | 동작 |
|-----|-----|------|
| `webhook.register` | `tasty webhook register` | 필요 메서드 + (`--handler <id>` xor `--sequence <json>`) + lifetime + 선택 인증 → `{id, url, ...}` 반환. `--method` 를 생략하면 CLI 는 `methods` 를 `null` 로 보내 서버 기본값 `POST`를 사용한다(빈 배열은 서버가 거절한다). `--sequence` 가 JSON 으로 안 읽히면 CLI 가 요청을 보내지 않고 인자 이름과 원인을 찍은 뒤 종료 코드 1 로 끝난다 |
| `webhook.list` | `tasty webhook list` | 전체 목록(각 항목 URL·메서드·steps·lifetime·인증여부) |
| `webhook.info` | `tasty webhook info --id <id>` | 단일 상세 |
| `webhook.unregister` | `tasty webhook unregister --id <id>` | 등록 해제(path 회수) |
| `webhook.sweep` | `tasty webhook sweep` | 만료 웹훅 일괄 정리 → 제거된 id 목록 |
| `webhook.config` | `tasty webhook port [<N> \| --unset]` · `tasty webhook allow-external [on\|off]` · `tasty webhook config [--port <N>]`(deprecated, 동작은 `webhook port`와 같고 출력의 `warnings`에 `deprecated: tasty webhook config, use tasty webhook port`를 싣는다) | 인자 없으면 실행 중인 리스너를 조회한다: `port`·`address`·`bound`·`source`(`argument`/`config`/`probe`)·`explicit`·`saved_port`(다음 실행 값)·`allow_external`(다음 실행 값)·`unavailable`(명시 지정 포트를 열지 못했거나 accept 스레드를 띄우지 못해 리스너가 없으면 `{port, source, error}`, 아니면 `null`). `port`로 저장, `unset_port: true`로 저장 값 삭제, `allow_external`로 외부 수신 설정 변경(설정에 적용하고 `config.toml`에 저장한 뒤 응답하며, 저장 실패는 `-32603`, `data.saved: false` — [완료 의미](../../architecture/data-flows.md#에이전트plugin-설정-변경-요청의-완료-의미)). 변경은 모두 다음 실행부터 적용된다(`applies_from: "next_start"`, `restart_required`) |

- **register 게이트**: `methods` 빈 배열 거부, `handler`/`sequence` 정확히 하나. `handler` 는 `validate_binding(handler, Webhook)` 로 검증 — 셸/hook-전용 핸들러는 거부([훅 (Surface / Global hooks)](../hooks/index.md)). 인라인 `sequence` 는 익명 핸들러(`user/wh-<slug>`)로 레지스트리에 등록된다.
- **lifetime 파라미터**: `--persistent`(bool), `--ttl-secs` xor `--count`(둘 다 없으면 `Unlimited`).
- **auth 파라미터**: `--auth-location <query|bearer|body|header>` + `--auth-token`(상호 requires), bearer 외에는 `--auth-key`.
- **핸들러**가 소비하는 페이로드→params 치환·source 게이트는 [훅 (Surface / Global hooks)](../hooks/index.md) 참조.
- **리스너 설정 GUI**: Settings › General › General 격자의 마지막 행(Language 뒤) "Accept webhook calls from other computers" 스위치가 `webhook.allow_external`을 바꾼다. 라벨이 라벨 열보다 길면 열 안에서 줄을 바꾼다. 행 바로 아래(격자의 행 간격 `space-sm`)에 경고 callout이 스위치 상태와 관계없이 항상 보이며, 모양은 Terminal › TUI의 OSC 52 경고와 같고 폭은 `measure-md`까지다. 포트는 CLI 전용이다.
- **핸들러 레지스트리 GUI**: [Settings › Handler › Hook Handlers](../settings/screens/settings.md) 서브탭에서 레지스트리(host 기본 + plugin 기여 + user 매핑)를 조회·편집한다(토글/셸 명령 인라인 편집/user 행 추가·제거, `~/.tasty/hook-handlers.toml` 영속). **제거는 user 행만** — host/plugin 행은 그 자리에 자물쇠 글리프가 오고, 지워도 finalize 가 되살린다. host/plugin 행을 고치면(시퀀스·명령·Switch) 같은 id 의 user patch 로 저장되고, 그 행은 출처 Tag 뒤에 "edited" Tag 와 Revert 를 단다. Revert 는 Save 때 patch 를 지워(`tasty hook-handler remove --id` 와 같은 효과) 기본값으로 돌아가며, Save 전에는 Undo 와 "reverts on save" Tag 로 대기한다. 대기 중에는 Switch 가 기본값을 보이며 잠기고(툴팁 "Reverts on save. Undo to change it.") `Edit` 도 Undo 나 Save 전까지 비활성이다. `IpcSequence` 행은 mono 한 줄 요약과 `Edit`(그 자리 문자열 편집기)이고, 한 줄 형식으로 쓸 수 없는 시퀀스는 [`tasty hook-handler get`/`upsert`](../hooks/index.md#핸들러-레지스트리-hook_handler) 로 고친다(TOML 손편집 + `reload` 도 그대로 된다). **고쳐도 이미 등록된 웹훅은 안 바뀐다** — 엔트리가 등록 시점 스냅샷을 소유하므로 다시 등록해야 한다. **리스너(bind/port/secret) 설정은 이 서브탭에 없다** — 외부 수신 허용은 Settings › General, 포트는 CLI(`webhook.config`).

## 비-목표 (Out of scope)

- **HTTPS/TLS 종단** — 리버스 프록시/공유기에 위임(사용자 요구가 "포워딩은 OS/공유기 몫").
- **외부 발신자의 조회/응답 채널** — 응답은 ACK 전용. 내부 상태 조회는 로컬 소유자 채널(`list`/`info`)로만.
- **웹훅에서의 OS 셸 실행** — 셸(`ShellCommand`)은 기존 훅(source `hook`) 전용, 웹훅 바인딩 불가. 셸 핸들러가 훅 트리거/수동으로 실행될 때 받는 `TASTY_HOOK_*` env 목록은 [hooks 문서의 셸 핸들러 환경변수 절](../hooks/index.md#셸-핸들러-환경변수-tasty_hook_) 참조.
- **plugin 의 웹훅 관리** — 현재 plugin 은 `webhook.register` 만 호출할 수 있다(나머지는 local-only).
- **plugin 프로세스의 직접 소켓 소유** — 코어가 소켓을 소유한다.
- **웹훅 외 프로토콜(raw TCP 등)** — HTTP 웹훅으로 확정.

## Acceptance Criteria

- Given 포트 설정됨 When `webhook.register --method POST --sequence '<ipc-seq>'` Then 발급 URL 반환, `curl -XPOST` 시 IpcSequence 가 실행되고 응답은 고정 ACK 바디만.
- Given 등록된 웹훅 When `webhook.unregister` 후 그 path 호출 Then `404`.
- Given `CountLimit{remaining:N}` When 접수되는 호출을 N+1 회 Then N 회 후 소멸(다음 호출 `410`/`404`).
- Given `CountLimit` + 인증 설정된 웹훅 When 토큰 불일치 호출 Then `401` 이고 **remaining 은 그대로**.
- Given `TimeLimit` deadline 경과 When 호출 또는 `webhook.sweep` Then `410` + 삭제.
- Given 인증 설정된 웹훅 When 토큰 불일치 Then `401`; 미설정 웹훅은 무인증 통과.
- Given 없는 path 를 임계치 초과 반복 When 같은 출처 재요청 Then 쿨다운 동안 `429`(다른 출처의 정상 웹훅은 계속 처리).
- Given 인증 설정된 웹훅 When 같은 출처가 틀린 토큰을 임계치 초과 반복 Then 쿨다운 동안 `429`.
- Given `ShellCommand` 핸들러 When 웹훅 바인딩 시도 Then source 게이트로 거부.
- Given 다른 프로세스가 28429를 점유 When 인자 없이 실행 Then 28430에 bind하고 `tasty webhook port`가 `port: 28430`, `source: "probe"`를 돌려준다.
- Given 28429 점유 When `--webhook-port 28429`로 실행 Then 다른 포트에 bind하지 않고 리스너 없이 시작하며, 터미널은 쓸 수 있고 경고 알림 한 건과 `tasty webhook port`의 `bound: false`·`unavailable.port: 28429`가 원인을 보인다.
- Given 기본 설정 Then 리스너는 `127.0.0.1`에만 있다. `allow_external = true`로 다시 실행하면 `0.0.0.0`.
- Given 두 인스턴스(다른 데이터 폴더)를 함께 실행 Then 둘 다 서로 다른 포트에 리스너를 띄운다.
- Given 포트가 탐색으로 밀렸고 Persistent 웹훅이 있음 When 실행 Then 경고가 뜨고 `tasty webhook list`의 URL이 새 포트를 쓴다.
- Given release 인스턴스가 웹훅 포트 P로 실행 중 When 같은 데이터 폴더에서 `--webhook-port Q`(Q≠P)로 다시 실행 Then 메시지 상자 뒤 종료 코드 1, Q=P면 평소처럼 넘긴다.

## 관련

- [hooks](../hooks/index.md) — 내부 이벤트 트리거(웹훅과 대칭인 trigger 출처) · [notifications](../notifications/index.md) · [file-handler](../file-handler/index.md)(레지스트리 정본 템플릿)
- [API](../../reference/api.md#기타-호스트) · 웹훅 요청 처리와 제한 · [Lua와 훅 실행](../hooks/index.md)
