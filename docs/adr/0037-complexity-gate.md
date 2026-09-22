# ADR-0037: 복잡도 게이트 — clippy cognitive(deny) + tokei 파일 SLOC, baseline 은 위치 단위 동결

- **Status**: Accepted
- **Date**: 2026-07-06
- **Tags**: lint, complexity, ci, quality-gate, clippy, cognitive-complexity, tokei, file-size, maintainability, clippy-policy, ratchet
- **Group**: ci-gates

## Context

정적분석이 소수 함수·파일에 복잡도가 집약됨을 드러냈다 — 함수 cognitive 는 극단적 우편향(중앙값 0, p99≈29)이고, egui 즉시모드 draw·입력 디스패치·manifest 검증 계열이 hotspot 이다. 관측 지표로 hotspot 목록은 확보했으나 **신규 복잡도의 유입을 자동으로 막는 장치가 없었다**. 현재 clippy 는 `too_many_arguments`·`type_complexity` 만 default-warn 이고, CI 에 `-D warnings` 가 없어 실제 차단력이 없다(`crossplatform-check.yml` 의 Windows clippy 는 deny-level 하드에러만 잡는다).

`docs/dev-guide/clippy-policy.md` 는 clippy 내장 threshold config 를 "봐주기용으로 풀지 않는다"고 기술해 왔다(원칙 #3). 이는 **전역 threshold 를 느슨하게 조정해 신규 위반을 은폐하는 행위**를 금한 것이며, 복잡도 상한 강제 자체를 영구 배제한 결정이 아니다(ADR 미승격 상태였다). 사용자가 복잡도 게이트 도입을 결정함에 따라 이를 **신규 정책으로 수립**한다.

제약(선행 조사 실측):
- clippy `cognitive_complexity` 는 nursery lint 이고 upstream 이 "측정 도구로 오용되지 말라"고 문서화했으며, 매크로 전개를 과대계상하는 오탐 이슈가 있다. 반면 **egui 즉시모드 draw 의 `ui.horizontal(|ui|{…})` 클로저를 부모 함수에 합산하지 않아** 구조적 draw 를 자동 배제한다 — 그 결과 clippy cognitive 는 "실 로직 복잡도"만 표면화하는 깨끗한 신호를 준다(clippy 임계 20 ≈ 외부 도구 rca cognitive 50 등가, baseline 33건).
- clippy 에는 **파일 SLOC lint 가 없다.** 파일 단위 상한은 `tokei`(이미 설치·CI 에서 사용 중) 로 측정한다.
- 기존 위반이 다수라 "전체 임계 초과 시 fail"은 첫 실행부터 영구 red.

무거운 대안(외부 `rust-code-analysis-cli` 3지표 + `baseline.json` diff 래칫 + 전용 워크플로)도 조사했으나, 상시 의존·baseline 파일·함수 식별 로직 유지 부담이 크고 절대치 신뢰가 낮아 **경량안을 채택**한다(§Alternatives).

## Decision

**함수 cognitive 복잡도는 clippy 내장 `cognitive_complexity` lint 를 `deny` 로 승격해(임계값 20) 강제하고, 파일 SLOC 은 `tokei` 기반 `scripts/check-file-size.sh`(상한 1000) 로 강제한다. 기존 초과분은 위치 단위 예외(함수 `#[allow]` + `// complexity-exempt:` 사유 / 파일 allowlist)로 동결(grandfather)하고, 신규/증가분만 차단한다.**

- **cognitive (함수)**: `Cargo.toml [workspace.lints.clippy]` 에 `cognitive_complexity = "deny"`, `clippy.toml` 에 `cognitive-complexity-threshold = 20`. nursery lint 라 명시 활성화가 필요하다. deny 라 그 자체로 에러가 되므로 `-D warnings` 는 여전히 쓰지 않는다(S-1 정책 존치). 신규 초과 함수는 기존 Windows clippy CI 잡이 자동 차단한다.
- **파일 SLOC**: `scripts/check-file-size.sh` 가 `tokei --output json` 으로 Rust 파일 code SLOC 를 재고, 1000 초과 파일 중 `.complexity-file-allowlist` 에 없고 skip(테스트 모듈·생성/전사 코드) 대상도 아닌 것이 있으면 exit 1. CI 배선은 `.github/workflows/complexity-check.yml` 의 `check-file-size` 잡(self-hosted Linux X64, tokei 설치 가드)이 담당한다. **이 ADR 을 쓸 당시 트리거는 `pull_request:[main]` 전용이었고, 이 저장소는 PR 을 열지 않으므로 실효 자동성이 없었다** — 결정을 바꾼 것이 아니라 배선이 발사되지 않는다는 사실이 나중에 확인돼 보탠 관찰이다. 그 뒤 [ADR-0131](0131-file-sloc-gate-needs-a-firing-trigger.md) 이 `push:[main]` 을 더했다. **현재 트리거 값과 실제 발사 여부는 여기 적지 않는다** — 시점마다 달라지고, 실제로 한 번 낡았다. 채널 정본은 `docs/dev-guide/ci-gates.md` 이고 거기에 무엇을 언제 쟀는지가 붙어 있다. cognitive 와 관심사 1:1 로 분리하고 tokei 기반 무컴파일이라 mac/win 러너 부담을 피하려 전용 경량 워크플로로 둔다.
- **baseline 동결(래칫)**: 별도 baseline.json 을 두지 않는다. 동결은 **위치 그 자체**가 담당한다 — 함수는 `#[allow(clippy::cognitive_complexity)] // complexity-exempt: <사유>`(현재 34곳 — **추적 `.rs` 안의 출현 수** 기준이다. 레포 전체 `grep -rn` 은 이 문단과 `clippy.toml`·`Cargo.toml`·다른 문서까지 세어 40 이 나온다, 태그 없는 레거시는 발견 즉시 태그를 붙여 편입한다), 파일은 `.complexity-file-allowlist`(현재 23개). 리팩터로 임계 이하로 내려가면 allow/allowlist 항목을 삭제해 래칫을 한 칸 조인다.
- **예외 사유 필수**: `complexity-exempt` 주석은 **왜 분해가 부적절/무의미한지**를 구체적으로 적는다(빈 사유·"TODO" 금지). raw px `#[allow]`·`disallowed-methods` 예외·`intent-exempt` 와 동일한 위치 단위 정당화 관행이다.
- **clippy-policy.md 재맥락화**: "clippy 내장 threshold 를 봐주기용으로 풀지 않는다"(존치)와 "복잡도 상한은 게이트가 담당"(신설)을 계층 분리로 병기.

## Consequences

- **얻은 것**: 신규 복잡도 유입이 차단돼 최악 hotspot 의 재생산이 멈춘다(자동으로 차단되는 것은 **cognitive 축뿐**이다 — 위 배선 주석 참조). cognitive 게이트가 **clippy 내장**이라 새 상시 의존이 없고(파일 SLOC 은 이미 쓰던 tokei 하나), cognitive 는 기존 Windows clippy 잡의 컴파일 산출물을 재활용해 별도 러너가 불요하다. 파일 SLOC 만 tokei 기반 무컴파일 경량 워크플로(`complexity-check.yml`)로 분리 배선하는데, rca+baseline.json 을 요구하던 대안 B 의 전용 워크플로와 달리 tokei 한 줄 호출뿐이라 유지 부담이 없다. egui draw 를 clippy 가 자동 배제하므로 baseline(33)이 거의 순수 로직 함수라 신호가 깨끗하다. 예외가 코드 옆 주석으로 남아 "왜 이 함수가 복잡한가"가 지역적으로 기록된다.
- **잃은 것**: cyclomatic·함수 SLOC 축은 포기한다(clippy 로 불가). clippy cognitive 는 nursery 라 버전 업 시 계산 방식이 흔들려 baseline 이 요동칠 수 있고, 반복 assert 테스트를 과대계상하는 오탐이 있어 테스트 모듈에 예외가 붙는다(현재 3곳). 파일 SLOC 게이트는 tokei·python 존재를 전제한다.
- **운영 비용 / 유지 부담**: `complexity-exempt` 주석과 allowlist 는 리팩터 시 갱신해야 하고, 유효하지 않은 예외가 축적되지 않도록 "해당 함수를 건드리는 PR 에서 예외 유효성 재확인"이 필요하다. 임계값 조정은 본 ADR 의 Reconsideration Trigger 를 근거로 한 새 결정으로만 한다(코드에 흩뿌리지 않고 `clippy.toml`·스크립트 상단 1곳에 모아 diff 로 이력이 남게 한다).

## Alternatives Considered

- **A. 관측만(게이트 없음)** — hotspot 목록을 리뷰 가이드로만 운영. 저비용이나 강제력이 없어 복잡도 총량이 우상향하는 걸 막지 못한다. → 게이트 도입 결정과 배치되어 기각.
- **B. 외부 `rust-code-analysis-cli` 3지표(cognitive+cyclomatic+함수SLOC) + `baseline.json` diff 래칫 + 전용 `complexity-check.yml`** — 커버리지는 넓으나 (1) rca 상시 의존·버전 관리, (2) 수백 엔트리 baseline.json 유지, (3) 라인 이동에 강건한 함수 식별 로직 자작, (4) rca 0.0.25 의 edition 2024·매크로 파싱 한계로 절대치 신뢰 불가 — 유지 부담이 얻는 것보다 크다. → **과한 도구**로 판단해 기각(경량안 채택).
- **전체 스캔 즉시 fail(동결 없이)** — 기존 초과 다수로 첫 실행부터 영구 red → 실질 사용 불가.
- **clippy threshold 를 봐주기용으로 상향** — 전역으로 느슨해져 신규 위반까지 은폐. clippy-policy.md 원칙 #3 위반이라 배제(게이트는 threshold 를 *조인다*, 푸는 게 아니다).

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

- **예외 과다**: `complexity-exempt` 주석이 임계 초과 함수의 일정 비율(예: 30%)을 넘어 게이트가 "통과 의례"로 전락하면, 임계값 재보정 또는 지표 축소를 검토.
- **clippy cognitive 요동**: clippy 버전 업으로 cognitive 계산 방식이 바뀌어 baseline(allow 목록)이 대폭 흔들리거나 오탐이 급증하면, 임계값 재조정 또는 외부 도구(대안 B) 재검토.
- **cyclomatic·파일SLOC 사각 부각**: 평면 match·거대 파일이 실제로 문제를 일으키는데 현 게이트가 못 잡는 사례가 누적되면, 축 추가(외부 도구)를 재검토.
- **clippy 개선**: clippy 가 파일 SLOC·안정적 cognitive 를 default 로 제공하면 스크립트 의존을 줄이고 내장으로 이관 검토.

## References

- 정책: [`docs/dev-guide/clippy-policy.md`](../dev-guide/clippy-policy.md)(본 ADR 로 「복잡도 게이트」 재맥락화), [`docs/dev-guide/complexity-gate.md`](../dev-guide/complexity-gate.md)(게이트 운영 상세), [`docs/dev-guide/error-handling.md`](../dev-guide/error-handling.md)(위치 단위 정당화 관행).
- 선례: `scripts/check-intent-discipline.sh`(소스 파싱 게이트 + `// intent-exempt:` 예외 컨벤션의 원형), `deny.toml`+`supply-chain-check.yml`(외부 도구 게이트 선례).
- 파일 축 후속: [ADR-0131](0131-file-sloc-gate-needs-a-firing-trigger.md)(발사되는 트리거) · [ADR-0165](0165-the-file-sloc-gate-measures-shipped-lines.md)(출하 줄을 잰다) · [ADR-0168](0168-the-file-sloc-threshold-is-not-derived-and-the-freeze-ratchets-one-way.md)(임계 1000 의 유도 없음 · 동결의 성장 방향 트리거).
- 설정: `Cargo.toml [workspace.lints.clippy]`(`cognitive_complexity = "deny"`), `clippy.toml`(`cognitive-complexity-threshold`), `scripts/check-file-size.sh`, `.complexity-file-allowlist`.
