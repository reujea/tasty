# ADR-0246: 텔레메트리에는 옵트아웃 축을 두지 않는다 — 그 권한 토큰은 경계가 아니라 선언이다

- **Status**: Accepted
- **Date**: 2026-09-08
- **Tags**: telemetry, privacy, permissions, plugin, trust-boundary, cap, non-goal, adr-0141
- **Group**: permission-security

## Context

dispatcher 미들웨어가 에이전트의 IPC 호출마다 `ipc_calls` 이벤트를 하나씩 남긴다
(`record_telemetry_and_audit` → `telemetry::record_ipc_call`). 그런데 **끄는 수단이
하나도 없다** — `crates/tasty-settings/src` 에 telemetry 설정 키가 없고, `tasty telemetry`
아래 명령에도 기록을 멈추는 것이 없다. 이것을 결함으로 볼 것인가가 물음이었다.

재 본 사실 넷 (2026-09-08 기준):

1. **호스트가 밖으로 안 보낸다.** `src`·`crates` 전체에 `reqwest::` 사용이 0 건이고,
   워크스페이스 매니페스트 어디에도 그 의존이 없다. 네트워크 표면은
   `src/webhook/listener.rs` 하나이고 **인바운드**다.
2. **이 기록은 단순 로그가 아니라 런타임 동작의 입력이다.** `check_cap_gate`
   (`src/adapters/ipc/handler.rs`)가 `telemetry::check_cap_block` 의 판정으로 plugin caller 의
   호출을 실제로 거부한다(`-32007 cap_blocked`). 그 cap 이 발화하는 근거가 `ipc_calls`
   누적이다. `AnomalyDetector` 도 같은 스트림을 먹는다.
3. **보존 상한이 이미 있다.** `src/store/log_retention.rs` 의 `TELEMETRY_EVENT`
   (`keep: 20_000`, ttl 없음) · `TELEMETRY_ANOMALY`(`keep: 5_000`, ttl `LOG_TTL_MS`).
4. **`telemetry` 권한 토큰은 방어선이 아니다.** `crates/tasty-ipc/src/method_meta.rs` 가
   `telemetry.*` 12 개를 그 토큰 하나로 열지만, 저장소인 `~/.tasty/memory.db` 는 **평문
   sqlite** 이고(`crates/tasty-memory/src` 에 sqlcipher·암호화 흔적 0 건) plugin 은
   **샌드박스 없는 사용자 권한 프로세스**다(`crates/tasty-host-plugin/src` 에 seccomp·
   namespace·jail 흔적 0 건). 토큰이 없어도 파일을 직접 열면 된다.

## Decision

**텔레메트리 기록에 사용자 옵트아웃 축을 두지 않는다.** 끄는 설정 키도, 끄는 CLI 명령도
만들지 않는다. 기록은 항상 돈다.

그리고 **`telemetry` 권한 토큰이 하는 일을 명시적으로 좁힌다 — 그것은 선언이지
경계가 아니다.** 매니페스트에 `"telemetry"` 가 적혀 있다는 것은 그 plugin 이 무엇을
만지겠다고 *밝혔다*는 뜻이고, 그 이상을 주장하지 않는다. tasty 는 OS 가 이미 정한 경계
위에 가짜 경계를 하나 더 그리지 않는다 — attach 의 신뢰경계를 SSH 로 둔 것과 같은 판단이다.
따라서 "권한으로 막혀 있으니 안전하다" 를 이 데이터의 안전 근거로 쓰지 않는다.

## Consequences

- **얻은 것**: cap·anomaly 가 눈이 멀 수 있는 갈래가 생기지 않는다. 옵트아웃 스위치가
  없으므로 "상한을 걸었는데 안 걸리는" 조용한 무력화 상태도 없다. 그리고 이 데이터의
  안전 근거가 **"밖으로 안 나간다"** 하나로 좁혀져, 그 전제가 깨지는 순간이 재검토
  조건으로 관측 가능해진다.
- **잃은 것**: 자기 기기에 남는 기록을 멈추고 싶은 사용자에게 줄 수단이 없다.
  받아들이는 이유는 그 기록이 기기 밖으로 나가지 않고, 상한이 걸려 있으며, 멈추면
  안전장치가 함께 죽기 때문이다.
- **운영 비용 / 유지 부담**: 없다. 코드 변경이 없는 결정이다.

## Alternatives Considered

- **A: 설정 키 하나로 기록 전체를 끈다** — cap 이 발화할 근거를 잃어 비용/폭주 상한이
  사실상 해제된다. 사용자는 "껐다" 고 생각하지만 무엇이 함께 꺼졌는지는 안 보인다.
- **B: 기록은 유지하고 조회만 막는다** — 막을 대상이 없다. 조회 경로를 닫아도 평문 sqlite
  파일이 그 자리에 있다. 방어 없는 방어를 문서로만 주장하게 된다.
- **C: `telemetry` 토큰을 읽기/기록/cap 으로 쪼갠다** — 토큰이 경계가 아니므로 쪼개도
  막히는 것이 없다. 선언의 해상도는 올라가지만, 그 대가로 "쪼갰으니 안전하다" 는 잘못된
  결론을 부르기 쉽다. 선언 해상도가 필요해지면 그때 별도로 판단한다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

**채널이 붙는 것** — 판정 시점에 레포가 읽을 수 있는 사실이다.

- 호스트에 **아웃바운드 전송 경로**가 생긴다. 이 결정의 첫째 근거가 그 부재다 —
  `src`·`crates` 에서 아웃바운드 HTTP 클라이언트 사용이 0 이 아니게 되는 순간.
- `~/.tasty/memory.db` 가 **암호화**되거나 plugin 이 **샌드박스**에 들어간다. 그러면
  `telemetry` 토큰이 실제 경계가 되고, 위 "선언이지 경계가 아니다" 가 거짓이 된다.
- **cap 이 telemetry 기록 없이도 동작하는 구조**가 된다. 그때는 "기록만 끄기" 가 성립하며,
  A 를 기각한 이유가 사라진다.

**원리적으로 안 붙는 것** — 사람이 관측해야 한다. 재는 법을 함께 적는다.

- 권한을 받은 plugin 이 이 기록을 읽어 자기 프로세스에서 밖으로 보낸 사례가 관측된다.
  재는 법: 그 plugin 의 소스·네트워크 동작을 사람이 확인한다 — 호스트는 plugin 프로세스의
  아웃바운드를 보지도 막지도 않으므로 레포 안에 좌변이 없다.

## References

- [ADR-0141](0141-host-key-namespace-is-reserved-in-raw-memory-kv.md) — `tasty.` 접두 예약.
  raw kv 옆문은 이 ADR 과 별개로 이미 닫혀 있다
- [docs/features/telemetry/index.md](../features/telemetry/index.md) — 현재 운영 상태
- [docs/dev-guide/plugin-permissions.md](../dev-guide/plugin-permissions.md) — `telemetry` 토큰이 여는 것
- 코드 근거 (**결정이 실현된 현재 위치**): `src/adapters/ipc/handler.rs` 의 `check_cap_gate` ·
  `src/adapters/ipc/handler/telemetry.rs` 의 `check_cap_block` · `record_ipc_call` ·
  `src/store/log_retention.rs` 의 `TELEMETRY_EVENT` · `TELEMETRY_ANOMALY`
