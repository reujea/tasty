# ADR-0516: 웹훅 413 은 레포에 둔 tiny_http 패치로 잔여 body 를 읽지 않고 연결을 닫는다

- **Status**: Accepted
- **Date**: 2026-09-23
- **Tags**: webhook, body-limit, tiny-http, connection-drain, resource-bound, dependency, vendoring, adr-0281, adr-0200, adr-0048

## Context

[ADR-0281](0281-webhook-parser-cap-and-connection-drain.md) 은 웹훅 body 상한의 선행 가정이 틀렸음을
기록했다. 고정된 `tiny_http` 0.12.0 은 1024 바이트를 넘는 `Content-Length` body 를 `EqualReader` 로
주고, 요청이 파괴될 때 그 Drop 이 **읽히지 않은 선언 길이를 끝까지 읽는다**(drain). 그 읽기는
잔여 길이만큼의 버퍼를 한 번에 요청하고, 잔여 소진·EOF·읽기 오류까지 요청을 처리한 worker 를
붙잡는다. 413 을 보낸 뒤에도 상대가 나머지 body 나 FIN 을 보내 줘야 정리가 끝났다.
ADR-0281 은 목표(잔여 읽기 없이 거부 연결을 정리)를 유지하고 완결 조건 다섯을 적었으며, 구현
방식(의존성 패치·교체)의 선택을 미뤘다. 이 ADR 은 그 선택이다.

drain 이 존재하는 이유는 연결 재사용이다. keep-alive 연결에서 다음 요청을 읽으려면 스트림이
요청 경계에 있어야 하고, 읽지 않은 body 를 건너뛸 방법은 읽는 것뿐이다. 그러니 "읽지 않는다" 는
"그 연결을 재사용하지 않는다" 와 한 묶음이다. 상류 0.12.0 의 공개 API 에는 그 묶음을 요청 단위로
고르는 길이 없다 — 응답의 `Connection` 헤더는 `add_header` 가 버리고, `into_writer`·`upgrade` 는 drain
을 피하지 못하며, `Server::unblock` 은 요청 단위가 아니다(ADR-0281 Alternatives 에 실측).

## Decision

**`tiny_http` 0.12.0 을 레포의 `vendor/tiny_http/` 에 옮기고 `[patch.crates-io]` 로 끼운 뒤, 새 공개
메서드 `Request::respond_and_close` 하나를 더하는 최소 패치를 얹는다. 웹훅 리스너는 **body 를 읽지 않고
답하는 두 자리 — 상한 초과 `413` 과 남용차단 `429` — 에 그 메서드를 쓴다.** 그 밖의 `tiny_http`
동작·기존 API 는 상류와 같다.

두 자리를 함께 고치는 이유는 결함이 응답 코드가 아니라 **"body 를 안 읽고 답한다" 는 성질**에 붙기
때문이다. 실측(아래)에서 미패치 429 경로도 같은 abort 로 끝났다. 다른 응답(`200`·`404`·`405`·`401`·`410`)은
`read_json_body` 가 이미 body 를 상한까지 읽은 뒤에 나가므로 이 자리에 해당하지 않는다.

`respond_and_close(response)` 가 하는 일:

- 응답에 `Connection: close` 를 붙여 보낸다 — 상대(특히 keep-alive 클라이언트)가 이 연결에 다음
  요청을 싣지 않게 알린다.
- 그 연결이 공유하는 닫기 플래그를 세운다. `EqualReader` 의 Drop 은 플래그가 서 있으면 잔여를 읽지
  않는다 — 잔여 길이 비례 할당도 worker 대기도 없다.
- 연결 스레드는 다음 요청 헤더를 읽기 **전에** 앞 요청이 스트림을 넘겨줄 때까지 기다리고(읽기 없이),
  플래그가 서 있으면 요청을 더 읽지 않고 연결을 끝낸다. 연결 객체가 파괴되며 소켓의 송신 방향(FIN)과
  수신 방향이 닫히고 fd 가 반납된다. 요청의 `Connection: close` 유무·`Expect: 100-continue` 유무와
  무관하게 같은 경로다.

패치 자리는 여섯 파일이고 전부 `tasty patch` 주석으로 표시한다. 옮긴 것·뺀 것·패치 목록은
[`vendor/tiny_http/PATCHES.md`](../../vendor/tiny_http/PATCHES.md).

### 사본의 위치를 `crates/` 밖에 둔다

`crates/` 아래에 두면 워크스페이스 `members = ["crates/*"]` 가 그것을 멤버로 삼는다. 그러면 크레이트 수
좌변(루트 `CLAUDE.md` 빌드 절 · `docs/architecture/index.md` · README 배지)이 바뀌고, 워크스페이스 lint
상속·clippy·fmt·파일 SLOC 게이트(`scripts/check-file-size.sh` 의 `SCAN_DIRS=(src crates)`)가 상류 코드를
tasty 코드처럼 판정한다. 상류 코드를 tasty 규칙에 맞게 고치면 사본이 상류와 멀어져 "무엇이 패치인가" 가
흐려진다. 그래서 `vendor/tiny_http/` 에 두고 워크스페이스 `exclude` 에 넣는다(워크스페이스 루트 아래의
path 의존은 기본으로 멤버가 된다). 멤버 수는 60 그대로다.

### ADR-0281 금지 목록과의 관계

FD 해킹 · `mem::forget` · 요청/스레드 누수 · 위장 upgrade 중 어느 것도 쓰지 않는다. 소켓은 연결 객체의
정상 파괴 경로(`RefinedTcpStream` 의 Drop)로 닫히고, 요청 객체는 `respond` 로 소비된다.

### 잰 것 (2026-09-23, Linux x86_64, TLS 미사용)

`src/webhook/listener_body_tests.rs` 가 실제 `Screened` 경로를 raw TCP 로 두드린다. 상한 2048,
`Content-Length` 2049, body 0 바이트, 클라이언트는 나머지 body 도 FIN 도 안 보낸다.

- keep-alive · `Connection: close` · `Expect: 100-continue` 세 경로 모두 `413` 응답 전체(`Connection:
  close` 포함, `100 Continue` 없음)를 받고 서버가 연결을 닫는다(응답 EOF).
- 응답 EOF 만을 완료 신호로 쓰지 않는다: 요청 처리 스레드의 반환을 따로 기다리고, 그 뒤 클라이언트가
  body 바이트를 더 써 보면 상대 커널이 거부한다(쓰기 오류) — 서버의 수신 방향이 닫혔다는 관측이다.
  drain 중인 서버는 그 바이트를 계속 받는다.
- `Content-Length` 2^40 에서도 같은 결과다. drain 은 잔여 길이 버퍼를 한 번에 요청하므로 이 값은
  할당 프로브다 — 패치를 끈 변이에서는 `memory allocation of 1099511627776 bytes failed` 로 시험
  프로세스가 abort 했다.
- 변이 셋이 각각 시험을 빨갛게 만들었다: 리스너가 `respond` 로 되돌아감(세 경로 실패 + 할당 abort) ·
  `EqualReader` 의 플래그 검사 제거(close 경로는 처리 스레드 5 초 timeout, 나머지는 응답 EOF 없음) ·
  연결 스레드의 플래그 검사 제거(keep-alive · Expect · 2^40 경로의 응답 EOF 없음).
- 거부되지 않은 요청은 그대로다: 같은 keep-alive 연결에 1500 바이트 body 두 건이 둘 다 `200` 이고
  `Connection: close` 가 없다. chunked 경계·무효 UTF-8·작은 `Content-Length` 시험도 변화 없이 통과한다.
- SSE 는 같은 사본 위에서 `tasty-plugin-agent-stream` 시험 137 건이 통과한다 — 정상 구독(이벤트 스트림
  응답 + live 프레임 · `Last-Event-ID` 재생) · 빈 바디 `401` · `404` 를 실제 소켓으로 두드리는 것들이다.

#### 실행 인스턴스에서 (격리 `TASTY_HOME` · 헤드리스 debug 데몬 · 기본 상한 1 MiB)

같은 프로브를 패치 전/후 바이너리에 각각 쏘았다(413 응답을 받은 뒤 body 바이트를 5 초 동안 계속 쓴다).

| 프로브(`Content-Length`, body 0 바이트) | 패치 전 | 패치 후 |
|---|---|---|
| keep-alive, 1 MiB+1 | 응답 EOF 없음(8 s timeout) · 413 뒤 받아 간 body **247,808 B** | EOF 즉시 · 1,024 B 뒤 `EPIPE` |
| `Connection: close`, 1 MiB+1 | EOF 즉시인데 수신은 계속 — **246,784 B** | EOF 즉시 · 1,024 B 뒤 `EPIPE` |
| `Expect: 100-continue`, 1 MiB+1 | 응답 EOF 없음 · **225,280 B**(`100 Continue` 없음) | EOF 즉시 · 1,024 B 뒤 `EPIPE` |
| 1 TiB(2^40) | **데몬이 죽었다** — exit `-6`(SIGABRT), 로그 `memory allocation of 1099511627776 bytes failed` | 데몬 생존 · RSS 28,052 KB · EOF 즉시 |

413 응답 자체는 네 프로브 모두 두 바이너리에서 전달됐고, 응답의 `Connection: close` 는 패치 후에만 있다.
패치 후 `CountLimit` 은 413 넷을 받고도 `remaining: 1` 이었고, 이어진 정상 호출이 `200` 과 함께 시퀀스를
실행하고(데몬 로그의 `notification.create`) 통을 소진해 웹훅이 사라졌다 — 초과 요청의 시퀀스 무실행 ·
CountLimit 미차감 · 정상 경로 보존이 같은 인스턴스에서 확인된다.

**패치 전의 1 TiB 갈래는 문서화되지 않은 원격 종료 경로였다.** 등록·인증과 무관하게(상한 판정이 매칭보다
앞이다) 요청 하나로 프로세스가 죽는다 — 남용차단 임계에 닿기도 전이다.

같은 프로브를 **남용차단 `429`** 경로에도 쏘았다(없는 path 로 404 를 반복해 쿨다운을 띄운 뒤 1 TiB 선언).
미패치는 물론이고 413 만 고친 중간 상태에서도 데몬이 같은 문구로 abort 했다(exit `-6`, 둘 다 21 번째
요청에서 429 진입). 그래서 `reject_if_abusive` 도 같은 메서드로 답한다 — 고친 뒤 그 자리는 시험
`raw_blocked_source_is_closed_without_draining` 이 재고, 그 한 줄을 되돌리는 변이는 시험 프로세스를
abort 시킨다.

## Consequences

- **얻은 것**: ADR-0281 의 완결 조건이 충족된다. 413 뒤 잔여 선언 길이 비례 할당과 worker 대기가
  없어지고, 상대의 협조 없이 연결이 정리된다. chunked 413 도 같은 메서드로 닫혀, 디코더가 멈춘 자리부터
  다음 요청을 파싱하려던 상류 경로(스트림이 요청 경계에 있지 않다)를 더 밟지 않는다.
- **잃은 것**: 413 을 받은 keep-alive 연결은 재사용되지 않는다 — 클라이언트는 다음 요청에 새 연결을 연다.
  body 가 이미 오고 있던 연결을 닫으면 커널이 RST 를 보낼 수 있고, 네트워크 상황에 따라 클라이언트가
  413 을 읽기 전에 RST 를 받을 수 있다(상한을 넘기는 발신자에게만 해당한다). 거부를 위해 잔여를 읽어
  주는 lingering close 는 이 결정이 없애려는 바로 그 drain 이라 두지 않는다.
- **운영 비용 / 유지 부담**: 상류 크레이트 사본 하나를 레포가 소유한다. `cargo update` 가 이 의존을
  올리지 않고, 상류의 보안 수정도 자동으로 들어오지 않는다. `cargo deny` 의 advisories 검사가 path 의존이
  된 이 사본을 crates.io 권고와 대조하는지는 재지 않았다 — 대조한다고 가정하지 않는다(licenses · bans ·
  sources 는 이 사본을 넣은 트리에서 `ok` 로 실측했다). 번들 plugin 중 `tiny_http` 를 쓰는 agent-stream 의 산출물도
  이 사본으로 빌드되지만 그 동작은 상류와 같다 — `respond_and_close` 를 부르지 않는다.
- **남는 한계**: 작은 `Content-Length` body(1024 이하)는 상류가 요청을 만들 때 이미 다 읽으므로 그 경로에는
  drain 이 원래 없다. 그 경로에서 `respond_and_close` 는 응답의 `Connection: close` 로 닫는다 — 연결
  스레드가 이미 다음 헤더를 기다리는 중이라, 클라이언트가 그 헤더를 무시하고 연결을 붙잡으면 다른 idle
  keep-alive 연결과 같이 남는다(상류에 idle timeout 이 없는 것과 같은 성질이다).

## Alternatives Considered

- **다른 HTTP 서버로 교체**(hyper · 수제 파서 등): drain 을 안 하는 서버는 있다. 그러나 웹훅과 agent-stream
  SSE 의 정상 경로(구독 · 빈 오류 ACK · keep-alive · chunked · 100-continue)를 전부 다시 재야 하고, 교체된
  서버의 요청 스머글링 방어·헤더 처리 차이를 새로 떠안는다. ADR-0048 이 tiny_http 를 고른 근거(blocking
  std::thread 일관성)도 다시 열린다. 이 결함 하나를 고치는 비용으로 크다 — 기각.
- **모든 요청에서 drain 을 없애기**(API 추가 없이 `EqualReader` 의 Drop 을 바꾸고, 잔여가 있으면 항상 연결을
  닫기): API 는 그대로지만 거부하지 않은 요청의 동작까지 바뀐다 — body 를 일부만 읽고 답하는 모든 소비자의
  keep-alive 재사용이 사라진다. 요청 단위로 고를 수 없어 "그 외 동작 불변" 을 깬다 — 기각.
- **git 포크를 `[patch]` 로 가리키기**(winit 과 같은 방식): 원격 레포 소유·가용성이 빌드 재현성에 들어온다.
  패치가 크레이트 하나 안의 수십 줄이라 레포 안에 두는 편이 리뷰·재현이 쉽다 — 기각.
- **사본을 `crates/` 아래에 둔다**: 위 Decision 의 "사본의 위치" 절 — 기각.

## Reconsideration Triggers

**채널이 붙는 것**

- 루트 `Cargo.toml` 의 `[patch.crates-io]` 에서 `tiny_http` 줄이 바뀌거나 빠지면, 또는 `vendor/tiny_http/src`
  가 바뀌면 — `src/webhook/listener_body_tests.rs` 의 `raw_oversize_*` 시험 넷이 그 사본으로 다시 돈다.
  패치가 빠지면 `Request::respond_and_close` 가 없어 본체가 컴파일되지 않는다.
- 웹훅의 413 응답 경로(`Screened::respond`)가 바뀌면 — 같은 시험 넷이 ADR-0281 의 완결 조건을 다시 잰다.

**원리적으로 안 붙는 것**

- 상류 `tiny_http` 가 요청 단위로 잔여 body 를 읽지 않고 연결을 닫는 공개 API 를 내면 — 이 사본을 걷고
  `[patch.crates-io]` 줄을 지운 뒤 그 API 로 옮긴다. 재는 법: 상류 릴리스의 `Request` API 와
  `EqualReader::drop` 을 읽고, body 를 안 보낸 `Content-Length` 초과 요청에서 위 raw 시험 넷이 통과하는지 본다.
- 상류에 보안 권고가 붙으면 — 사본은 자동으로 안 따라간다. 재는 법: RustSec 에서 `tiny_http` 를 조회하고,
  해당 수정을 사본에 옮기거나 상류 새 버전 위에 이 패치를 다시 얹는다.
- 413 뒤 RST 로 응답을 못 받는다는 발신자 보고가 나오면 — 재는 법: 실제 네트워크(루프백 아님)에서 상한을
  넘는 body 를 계속 보내는 클라이언트가 413 을 받는 비율을 잰다.

## References

- 결정 대상: [ADR-0281](0281-webhook-parser-cap-and-connection-drain.md) (완결 조건 다섯 · 금지 목록)
- [ADR-0200](0200-webhook-body-has-a-per-request-byte-cap.md) — body 상한과 잔여 읽기 중단 목표
- [ADR-0048](0048-webhook-http-tiny-http-blocking.md) — HTTP 레이어 선택
- [`vendor/tiny_http/PATCHES.md`](../../vendor/tiny_http/PATCHES.md) — 사본 범위와 패치 목록
- [의존성 이슈](../dev-guide/dep-issues.md) — 탈출 대기 중인 의존 패치 목록
- [웹훅](../features/webhook/index.md) — 현재 보장
- 현재 코드(결정이 실현된 위치): `src/webhook/listener.rs` 의 `Screened::respond`,
  `vendor/tiny_http/src/request.rs` 의 `Request::respond_and_close`, `vendor/tiny_http/src/client.rs` 의
  `ClientConnection::next`, `vendor/tiny_http/src/util/equal_reader.rs` 의 `EqualReader::drop`
