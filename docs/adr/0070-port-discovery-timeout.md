# ADR-0070: 원격 포트 발견에 3중 상한(ssh `ConnectTimeout` + 프로세스 감시 + 호출 전체 예산)을 건다

- **Status**: Accepted
- **Date**: 2026-08-13
- **Tags**: remote-attach, ssh, port-discovery, timeout, no-hang, i18n, adr-0032, adr-0053
- **Group**: remote-attach

## Context

원격 attach 파이프라인에서 **포트 발견 단계만** 무한 대기에 열려 있었다. 응답 없는 호스트
(잘못된 IP, 방화벽 DROP, 꺼진 머신)를 가리키는 프로필을 고르면 조회가 실패로 떨어지지 않고
수 분~무한정 진행 중 상태로 남았다.

세 요인이 겹친 결과다.

1. **`ConnectTimeout` 부재** — `push_common_opts` 가 걸던 `-o` 는 `BatchMode=no` /
   `ServerAliveInterval=15` / `ServerAliveCountMax=3` 뿐이었다. ServerAlive 계열은 **연결이
   수립된 뒤의** keepalive 라 TCP 핸드셰이크 구간을 덮지 않는다. 결과적으로 연결 수립은 OS
   기본 SYN 재시도에 전적으로 맡겨졌다(리눅스 `tcp_syn_retries=6` = 1+2+4+8+16+32+64 ≈ 127초).
2. **프로세스 레벨 상한 부재** — `run_ssh_capture` 가 `Command::output()` 을 썼다. `output()`
   은 자식이 끝날 때까지 무기한 블록하고 `Child` 핸들도 주지 않아 중간에 끊을 수단이 없다.
3. **체인이 대기를 배로 증폭** — auto 모드는 subcommand → file-unix → file-windows 를 **순차**
   시도하고, 명시 `port_file` 경로도 `cat` → `type` 2 회를 돈다. 무응답 호스트면 한 번의 조회가
   6 분을 넘겼다.

같은 파이프라인의 **뒷단은 이미 no-hang 이 보장돼 있었다** — `SshTunnel::wait_ready` 는 5초
데드라인, `remote_browse::probe_method` 는 `PROBE_TIMEOUT`(5초) read/write 타임아웃에 회귀
테스트까지 있다. 즉 터널 이후는 보장되는데 그 **앞단만** 보장 밖이었다.

파급은 팝업 하나에 그치지 않는다. `discover_remote_port` 를 공유하는 소비자 전부가 같이
멈춘다 — CLI(`remote workspaces` / `remote check` / `attach`), IPC(`remote.workspaces` /
`remote.attach` / `remote.profile.detect`), GUI(원격 워크스페이스 추가 팝업, 도구 메뉴 >
Remote connections 감지), 자동 attach 워커. 특히 자동 attach 는 워커가 hang 하는 동안 그
anchor 가 `auto_attach_active` 에 남아 **그 워크스페이스의 신규 attach 와 backoff 재연결이
통째로 억제**되고, `tasty attach --reconnect` 는 실패가 아니라 hang 이라 backoff 자체에
도달하지 못한다. `remote check` 는 "실패(거부/EOF/**타임아웃**)는 dead(exit≠0)" 라는 단발
판정 계약을 문서로 약속하고 있는데, 앞단이 hang 하면 그 계약이 성립하지 않았다.

부차적으로, `BatchMode=no` 때문에 password/host-key 프롬프트가 뜨는 프로필에서는 ssh 가
`Stdio::null()` 인 stdin 대신 `/dev/tty` 로 프롬프트를 내고 무기한 기다릴 수 있다(GUI
사용자는 그 프롬프트를 볼 수 없다). controlling tty 가 없는 환경에서는 `ssh_askpass` 실행
실패로 즉시 exit 255 라 재현되지 않지만, tty 가 있는 터미널에서 tasty 를 띄운 경우는
확인되지 않았다 — 이 구간은 ssh 옵션으로는 끊을 수 없다.

## Decision

포트 발견 경로에 **세 겹의 상한을 모두 건다.** 하나로 통일하지 않는다 — 각각이 다른 구간을
덮고, 어느 하나만으로는 무한 대기가 남기 때문이다.

1. **`-o ConnectTimeout=10`**(`SSH_CONNECT_TIMEOUT`) — 연결 수립(TCP + 배너) 구간. ssh 가
   홉마다 적용하므로 ProxyJump 다단도 각 홉이 이 상한을 받는다. `push_common_opts` 에
   넣으므로 포트 발견뿐 아니라 터널/대화형 ssh 경로에도 함께 적용된다.
2. **프로세스 레벨 감시 20초**(`PORT_DISCOVERY_STEP_TIMEOUT`, ssh 자식 1 개당) — `output()`
   대신 `spawn()` + `try_wait` 폴링. `ConnectTimeout` 이 못 보는 구간(인증 핸드셰이크, 원격
   명령 실행, `/dev/tty` 프롬프트 대기)을 덮는다. 상한을 넘기면 kill + wait 로 좀비 없이
   거둔다(`SshTunnel::drop` 과 같은 패턴). 값은 연결 상한의 2 배 — 2-hop ProxyJump 가 정상
   경로로 들어가도록.
3. **호출 1 회 전체 예산 45초**(`PORT_DISCOVERY_TOTAL_TIMEOUT`) — `discover_remote_port` /
   `detect_port_mode` 진입 시 데드라인을 만들어 체인 각 단계에 나눠준다. 각 단계는
   `min(단계 상한, 남은 예산)` 만 받고, 예산이 소진되면 남은 단계는 ssh 를 띄우지도 않고
   즉시 타임아웃으로 떨어진다. 이것이 "단계 상한 × 단계 수" 의 곱을 끊는다.

세 상한 모두 `pub const` 로 노출한다(소비자가 UI 문구/진행 표시를 같은 값에 맞출 수 있도록 —
`remote_browse::PROBE_TIMEOUT` 의 공개 방식을 따른다).

**타임아웃은 `PortDiscoveryFailureKind::TimedOut` 새 variant** 로 분류한다
(`lang/{en,ko,ja}.toml` `[ssh.port_discovery] timed_out` 3 개 언어 동시 추가). auto 체인의
대표 에러 선정(`pick_most_informative`)에서는 **최우선**이다.

**사용자 `extra_options` 의 `ConnectTimeout` 이 기본값을 이긴다.** ssh(1) 은 같은 키가 여러 번
오면 먼저 나온 값을 쓰므로, 기본값을 `extra_options` **뒤에** push 한다.

## Consequences

- **얻은 것**:
  - 포트 발견이 무한 대기하지 않는다. 무응답 호스트 기준 auto 체인 실측 ~30초(3 × 연결
    상한), 최악 45초 상한. `remote check` 의 dead 판정 계약이 앞단에서도 성립한다.
  - 자동 attach 워커가 상한 안에 끝나므로 `auto_attach_active` anchor 가 해제되고, 그
    워크스페이스의 재시도·backoff 재연결 억제가 풀린다.
  - `remote.profile.detect` / 도구 메뉴 재감지의 "감지 중" 스피너도 같은 상한에 묶인다
    (같은 `run_ssh_capture` 를 공유하므로 자동으로 함께 고쳐진다).
  - `/dev/tty` 프롬프트 hang(미검증 부차 가설)도 프로세스 레벨 감시가 함께 끊는다 — 별도
    작업으로 분리할 필요가 없어졌다.
- **잃은 것**:
  - 값이 고정 상수라 극단적으로 느린 회선/다단 ProxyJump 에서 정상 연결이 오탐 실패할
    가능성이 남는다. 회피 수단은 프로필 `extra_options` 에 `ConnectTimeout=<큰 값>` 을
    넣는 것뿐이고, 전체 예산(45초)은 사용자가 넘길 수 없다.
  - stdout/stderr 를 파이프로 잡으므로 원격이 파이프 버퍼(수십 KB)를 넘겨 쓰면 자식이
    블록될 수 있다. 그 경우도 상한에서 kill 되므로 hang 은 아니지만, `output()` 이 하던
    "무제한 수집" 은 아니다(포트 발견 출력은 한 줄이라 정상 경로에는 영향 없음).
  - 실패 분류가 3 → 4 종이 되어 i18n 키와 문서가 늘었다.
- **운영 비용 / 유지 부담**: 상수 3 개의 상호 불변식(단계 상한 > 연결 상한, 전체 예산 <
  단계 상한 × 체인 길이)을 단위 테스트로 고정했다. 값을 바꿀 때 그 테스트가 먼저 깨진다.
  `try_wait` 폴링은 5ms→50ms 램프라 정상 경로(수백 ms)의 지연을 사실상 더하지 않는다.

## Alternatives Considered

- **`ConnectTimeout` 만 건다** — 가장 작은 변경이지만 연결 수립 구간만 덮는다. 인증
  핸드셰이크 이후의 정지, 원격 명령이 안 끝나는 경우, `/dev/tty` 프롬프트 대기는 그대로
  무한이다. 조사 단계의 부차 가설이 정확히 이 사각지대라 채택하지 않았다.
- **프로세스 레벨 감시만 건다** — 무한 대기는 막지만, 무응답 호스트에서 매 단계가 감시 상한을
  **꽉 채운다**(OS SYN 재시도가 그때까지 계속되므로). `ConnectTimeout` 을 함께 걸면 dead
  호스트가 10초에 판정돼 체감이 크게 낫다. 둘은 대체재가 아니라 보완재다.
- **타임아웃을 `SshConnectionFailed` 로 접는다** — i18n 키 3 개를 아낄 수 있다. 그러나
  ① 사용자가 취할 조치가 다르고(도달성/회선 점검 vs 인증·호스트키 점검), ②
  `classify_by_exit_code` 가 시그널 종료를 `SshConnectionFailed` 로 보는데 우리가 건 kill 도
  시그널 종료라, 접으면 "우리가 죽인 것" 과 "원격발 시그널 종료" 가 같은 분류로 뭉개진다.
  분류를 분리해야 그 구분이 코드에 남는다.
- **체인 전체 상한 없이 단계 상한만 둔다** — 구현이 단순하지만 auto 체인에서 총 대기가 3 배
  (60초)가 되고, 명시 `port_file` 은 2 배가 된다. 요구사항이 명시적으로 "곱해진 값이 기다릴 수
  있는 시간인지" 판단을 요구한 지점이라 전체 예산을 별도로 뒀다.
- **기본 `ConnectTimeout` 을 `extra_options` 보다 앞에 둔다** — 프로필로 못 늘리게 되어,
  느린 회선 사용자에게 회피 수단이 사라진다. ssh(1) 의 first-wins 규칙을 그대로 활용해
  사용자 지정이 이기게 했다.
- **체인 3 단계를 병렬 실행한다** — 무응답 호스트의 총 대기를 1 단계분으로 줄일 수 있으나,
  살아있는 호스트에 매번 ssh 3 개를 동시에 띄우게 되어 정상 경로의 비용/부하가 커진다.
  실패 경로를 위해 성공 경로를 비싸게 만드는 거래라 채택하지 않았다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

- 정상 프로필(느린 회선, 다단 ProxyJump, 고지연 링크)이 상한 때문에 오탐 실패한다는 보고가
  나온다 → 값 상향, 또는 전체 예산도 프로필에서 조정 가능하게 열어야 하는지 검토.
- 포트 발견이 워커 스레드가 아니라 취소 가능한 비동기 작업으로 재구성된다(팝업 Cancel 이
  진행 중 조회를 즉시 끊는 형태) → 고정 상한의 역할이 줄어 값 재검토.
- auto 체인이 3 단계를 넘게 늘어난다 → 전체 예산 대비 단계당 배분이 다시 굶는지 재계산.
- `BatchMode=no` 정책이 바뀌어(예: 포트 발견 한정 `BatchMode=yes`) `/dev/tty` 프롬프트
  경로가 사라진다 → 프로세스 레벨 감시의 근거 하나가 줄어드므로 필요성 재평가.

## References

- [`docs/features/remote-attach/index.md`](../features/remote-attach/index.md) — "원격 포트 발견 실패 진단" / "원격 생존 확인"
- [`docs/dev-guide/attach-behavior.md`](../dev-guide/attach-behavior.md) — SSH 터널, 연결 생존 확인(read timeout + heartbeat), `remote check` 3 단계 판정
- [`docs/adr/0032-remote-attach-two-layer-split.md`](0032-remote-attach-two-layer-split.md) — ssh / tasty-attach 2-레이어 프로필 모델
- [`docs/adr/0053-native-file-picker-remote-attach-channel.md`](0053-native-file-picker-remote-attach-channel.md) — "무응답은 시간으로 끊는다"(soft timeout 8초) 선례
- [`docs/dev-guide/error-handling.md`](../dev-guide/error-handling.md) — 실패를 삼키지 않고 분류/로그하는 규칙
- `crates/tasty-ssh/src/lib.rs` — `SSH_CONNECT_TIMEOUT` / `PORT_DISCOVERY_STEP_TIMEOUT` / `PORT_DISCOVERY_TOTAL_TIMEOUT`, `run_capture_with_budget`
