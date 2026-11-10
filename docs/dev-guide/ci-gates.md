# CI · 훅 게이트 매트릭스 — 어떤 검사가 어디서 도는가

각 검증 명령을 어디서 실행하는지와 실행 결과를 확인하는 방법을 정리한다.
명령의 정책과 임계값은 표에서 연결한 가이드를 따른다.

CI 설정 설명은 작업 트리의 `.github/workflows/`를 기준으로 한다.
원격에 반영됐는지와 실제 실행 결과는
[원격 설정 확인](#트리거는-어느-ref-의-것인가--작업-트리와-원격이-갈린다)을 따른다.
선택 이유는 CI와 복잡도 검사에 있다.

DPI 수동 변환 검사는 `src/`와 `crates/`의 파일 수 하한을 각각 대조한다. 한 루트의 파일 증가로 다른 루트 전체의 누락을 가리지 못하며, 두 누락 방향을 합성 입력으로 검증한다.

구조 writer 검사의 터미널 삽입 탐지는 `engine.runtime.terminals.insert`와 좁은 인자로 받은 `terminals.insert`를 모두 센다. 수신자 이름에 따른 텍스트 검사이며 타입을 판별하는 검사는 아니다.

## 자동으로 도는 것

| 검사 | 명령 | 채널 | 트리거 | 등급 |
|---|---|---|---|---|
| 포맷 | `cargo fmt --check` (+ `crates/tasty-plugin-sdk-wasm/` 매니페스트) | `format-check.yml` (ubuntu-latest) | main push · PR · 수동 | [실측] |
| SemVer 가드 | `cargo test --locked --no-default-features --no-fail-fast --test api_baseline_0_7 --test changelog_unreleased --test cli_naming_count_drift` | `test.yml` 의 `semver-guards` (self-hosted Linux X64) | main push · 수동 | [실측] |
| macOS 컴파일 + 단위테스트 | `cargo check --workspace --locked` · `cargo test --workspace --lib --bins --locked --no-fail-fast` | `crossplatform-check-macos.yml` 의 `check-macos` (self-hosted macOS) | main push(문서 제외) · PR · 수동 | [실측] |
| Windows lint + 단위테스트 **+ 지목 통합** | `cargo clippy --workspace --all-targets --locked` · `cargo test --workspace --lib --bins --locked --no-fail-fast` · `cargo test -p tasty-shm -p tasty-doc-guards --locked --no-fail-fast` | `crossplatform-check.yml` (self-hosted Windows) | main push(문서 제외) · PR · 수동 | [실측] |
| Windows ARM64 컴파일 | `cargo check --workspace --all-targets --target aarch64-pc-windows-msvc --locked` (x64 호스트의 전용 러너에서 교차 컴파일, 테스트·링크 없음, C 의존성은 MSVC ARM64 빌드 도구 필요 — [release](release.md#windows-arm64)) | `crossplatform-check.yml` 의 `check-windows-arm64` (같은 Windows 호스트의 전용 러너 `win-arm64`) | main push(문서 제외) · PR · 수동 | [배선] |
| headless 컴파일 · **전체 스위트** · lint **+ Linux gui 단위테스트** | `cargo check --workspace --no-default-features --locked` · `cargo test --workspace --no-default-features --locked --no-fail-fast` · `cargo clippy --workspace --all-targets --no-default-features --locked` · `xvfb-run … cargo test --workspace --lib --bins --locked --no-fail-fast`(스텝 `cargo test (linux, gui, unit)` — 기본 feature, Xvfb 안에서 실행, 아래 [gui 단위 단계의 Xvfb](#gui-단위-단계의-xvfb)) · **관측(비차단)** `xvfb-run … cargo test --workspace --locked --no-fail-fast --test e2e_tests -- multi_window_owner_routing --exact`(스텝 `cargo test (linux, gui, e2e — 관측용)`, `continue-on-error: true` — 헤드리스 조합에서 컴파일하지 않는 이 시험을 돌리되 실패해도 잡을 차단하지 않는다) · **관측(비차단)** `xvfb-run … cargo test -p tasty-plugin-markdown --locked --no-fail-fast -- --ignored --exact render::webview_layout_tests::address_bar_stays_on_top_in_webkitgtk`·`render::bed_focus_webkit_tests::bed_focus_sheet_switches_state_values_in_place`(스텝 `cargo test (linux, gui, webview layout — 관측용)`, `continue-on-error: true` — markdown 주소창 배치와 바탕 포커스 시트를 WebKitGTK 로 재는 Linux 전용 시험, 아래 [webview 레이아웃 시험](#webview-레이아웃-시험)) | `crossplatform-check.yml` 의 `check-headless` (self-hosted Linux X64) | main push(문서 제외) · PR · 수동 | [실측] |
| **not-debug(release) 컴파일 · gui** | `cargo check --workspace --release --locked` | `crossplatform-check.yml` 의 `check-release` (self-hosted Linux X64) | main push(문서 제외) · PR · 수동 | [실측] |
| Android 터미널 코어 컴파일 | `cargo check -p tasty-terminal -p tasty-terminal-link --target aarch64-linux-android --locked` (x64 러너에서 교차 컴파일, NDK 없음, 링크·실행 없음, 두 크레이트만) | `crossplatform-check.yml` 의 `check-android` (self-hosted Linux X64) | main push(문서 제외) · PR · 수동 | [배선] |
| 문서 가드 | `cargo test -p tasty-doc-guards --locked --no-fail-fast` | `doc-guards.yml` (ubuntu-latest) | main push · PR · 수동 — **경로 필터 없음**([가드 검증](guard-verification.md)) | [실측] |
| 파일 SLOC | `bash scripts/check-file-size.sh` | `complexity-check.yml` (self-hosted Linux X64) | main push(문서 제외) · PR · 수동 | [실측] |
| 동결 총합 래칫 | `bash scripts/check-frozen-sum-ratchet.sh` | `complexity-check.yml` (self-hosted Linux X64, 같은 잡) | main push(문서 제외) · PR · 수동 | [실측] |
| Intent 규율 (범위와 한계는 [Intent 규율 검사](#intent-규율-검사)) | `bash scripts/check-intent-discipline.sh` — **`mask-source` 판정기를 먼저 짓는다** | `script-gates.yml` (self-hosted Linux X64) | main push(문서 제외) · PR · 수동 | [실측] |
| 사유 없는 `#[allow]` (**상한 래칫**, 판정기 `mask-source` 선행) | `bash scripts/check-allow-reason.sh` | `script-gates.yml` (self-hosted Linux X64) | main push(문서 제외) · PR · 수동 | [실측] |
| 공용 순회를 안 거치는 직접 `read_dir` (**상한 래칫**, 판정기 `mask-source` 선행) | `bash scripts/check-shared-walk-ratchet.sh` | `script-gates.yml` (self-hosted Linux X64) | main push(문서 제외) · PR · 수동 | [실측] |
| 구조 mutation 지표와 지정 실행 entry (**기준 파일 래칫**, 판정기 `strip-cfg-test`·`mask-source` 선행) | `bash scripts/check-core-writer-ratchet.sh` | `script-gates.yml` (self-hosted Linux X64) | main push(문서 제외) · PR · 수동. 범위와 한계는 [구조 writer 래칫](#구조-writer-래칫) | 등급 미정 |
| 셸 자산 정적 검사 | `bash scripts/check-shell-assets.sh` | `script-gates.yml` (self-hosted Linux X64) | main push(문서 제외) · PR · 수동. install-shellcheck.sh로 도구를 준비한다. 추적 셸 자산을 검사하며 staged 훅은 새 파일도 확인한다. warning 이상은 실패, 도구 부재는 rc 2다. | 등급 미정 |
| plugin 버전 bump | `bash scripts/check-plugin-version-bump.sh --range <before> <after>` | `plugin-version-check.yml` (self-hosted Linux X64) | main push · PR · 수동. 문서는 제외하되 `src/`·`lang/`·`assets/` 아래 `.md`를 포함한다. path 의존성 변경도 검사하며 패턴 순서가 중요하다. 잡이 strip-cfg-test를 먼저 빌드한다. staged 검사와 push 범위 검사는 구분한다([릴리스](release.md#플러그인-버전-비교)). | [실측] |
| 공급망 | `cargo deny --workspace check`, `cargo deny --manifest-path crates/tasty-plugin-sdk-wasm/Cargo.toml check` | `supply-chain-check.yml` | `--workspace`가 없으면 루트 패키지의 의존성만 보고 플러그인 전용 의존성을 놓친다. workspace에서 제외된 `tasty-plugin-sdk-wasm`은 자체 `Cargo.lock`을 쓰므로 `--workspace`에 잡히지 않아 `--manifest-path`로 따로 검사한다. 같은 잡의 두 번째 단계이며 앞 단계가 실패해도 실행한다. 루트 `deny.toml`을 함께 쓰므로 루트 그래프에만 있는 ignore 항목은 이 단계에서 `advisory-not-detected` 경고로 나오며 실패로 보지 않는다. main push는 `Cargo.lock`·`deny.toml`, `crates/tasty-plugin-sdk-wasm/`의 `Cargo.toml`·`Cargo.lock`, 이 워크플로 파일 변경 시, main 대상 PR은 경로 필터 없이 실행한다. 매주 월 09:00 UTC와 수동 실행도 지원한다. 의존성 변경은 push에서, 새 외부 권고는 주기 실행에서 확인한다. 올릴 수 없는 공지의 예외는 `deny.toml` 주석에 공지별 근거와 재검토 조건을 적고, 라이선스는 전역 `allow` 대신 크레이트별 `exceptions`를 먼저 쓴다. | [실측] |

### 로컬에서 이 게이트들을 돌리기 전에 — **판정기부터**

`mask-source`를 쓰는 검사는 실행 전에 해당 바이너리를 빌드하고 내용 지문을 확인한다.
일반 `cargo build`만으로는 이 바이너리가 만들어지지 않는다.

```bash
cargo build -p tasty-doc-guards --bin mask-source
./target/debug/mask-source --check-fresh .
```

구조 writer 래칫은 test 전용 코드를 지우는 `strip-cfg-test`도 함께 사용한다.
`cargo build -p tasty-doc-guards --bin mask-source --bin strip-cfg-test`로 둘을 한 번에 빌드한다.

바이너리가 없거나 소스와 맞지 않으면 먼저 다시 빌드한다. 기본 상한 검사는 이때 rc 2로
측정을 거부한다. 다른 스크립트는 원문을 대신 셀 수 있으므로 오류·폴백 메시지를 읽는다.
원문은 문자열과 주석까지 세므로 그 결과로 상한이나 예외를 늘리지 않는다.
서로 다른 트리를 비교할 때도 양쪽 검사기가 각 트리의 소스와 맞는지 확인한다.

### 등급 — 이 표의 각 행이 무엇까지 말하는가

표의 등급은 현재 실행 상태와 구분한다. 자동으로 갱신되는 상태 표시가 아니다.

| 표시 | 뜻 |
|---|---|
| [배선] | 작업 트리의 워크플로에 해당 명령과 트리거가 설정되어 있음 |
| [실측] | 해당 잡 또는 단계가 과거에 통과한 기록이 있음 |
| 등급 미정 | 실행 이력과 설정만으로 아직 분류하지 못함 |

[실측]의 과거 근거는 2026-09-06 확인한 Test, Format, Complexity, Script Gates,
Plugin Version, Doc Guards와 공급망 검사다. 크로스플랫폼 네 잡은 같은 날 실행
`34062607769`에서 모두 success였다. 이 기록은 현재 커밋의 통과를 뜻하지 않는다.
현재 상태는 실행 이력에서 확인하고, 실행하지 못한 검사만 '미측정'으로 보고한다.
실패한 검사를 미측정으로 바꾸어 부르지 않는다.

```bash
gh run list --limit 10
gh run view <run-id> --json jobs
```

#### 실행 결과를 보는 단위

워크플로 결론만으로 전체 검증 결과를 판단하지 않는다. 잡과 단계까지 내려가 확인한다.

| 관측 | 보고할 내용 |
|---|---|
| 워크플로 성공, 검증 잡 skipped | 해당 잡은 실행되지 않았음 |
| 잡 실패, 뒤 검증 단계 skipped | 뒤 단계는 미측정이며 앞 단계 실패와 구분 |
| 검증 단계 실패, 잡 성공 | 실패가 `continue-on-error` 등으로 전체 결과에 반영되지 않았음 |

의도한 skip도 실행한 검사는 아니다. 대체 검증에는 대상 커밋과 결과를 적는다.
취소된 실행을 후속 push가 대신한다면 후속 실행의 실제 완료도 확인한다.
미측정이 이어진 기간을 보고하려면 아래 [미측정 구간의 길이](#미측정-구간의-길이--이-문서가-가진-적-없던-축)를 따른다.

검사 입력 범위도 함께 확인한다. 플러그인 버전의 staged 검사와 push 범위 검사는 대상이
다르므로 최종 통합 트리에서는 `--range <직전 push> HEAD`로 다시 검사한다.
설치한 pre-push의 B.9도 Git이 전달한 원격·로컬 SHA로 이 검사를 수행한다.
버전 선택과 여러 변경의 통합 방법은 [릴리스](release.md)를 따른다.

빌드 캐시처럼 로컬 환경에 따라 입력이 달라지는 검사는 검사한 대상 수를 남긴다.
CI에 해당 캐시가 없었다면 그 실행으로 캐시가 있는 환경까지 검증했다고 말할 수 없다.
같은 검사 결과가 머신마다 다를 때는 코드 변경에 원인을 돌리기 전에 입력 범위를 비교한다.
플랫폼별 문제는 해당 OS의 잡이나 문제 조건을 명시적으로 만든 테스트로 확인한다.
예를 들어 Linux 경로만 읽는 테스트로 Windows 경로 구분자 처리를 검증할 수 없다.

#### 사용자 문서에서 자동으로 대조하는 내용

| 내용 | 검사 |
|---|---|
| CLI 명령, CLI 예시 파싱, 릴리스 산출물 파일명, 설치 위치와 패키지 의존성, 훅 이벤트 이름, 설정 절 이름과 컨텍스트 메뉴 항목, 원격 포트 모드 값 | [tasty-page 레포의 checks](https://github.com/zilhak/tasty-page/tree/main/checks)(본체를 main 또는 릴리스 태그로 받아 대조) |
| 릴리스 자산 목록(개발 가이드 표와 업로드 대상) | `crates/tasty-doc-guards/tests/release_assets_match_the_guide.rs` |

사용자 가이드는 tasty-page 레포에 있다. 가이드와 본체 소스의 대조는 그 레포의 CI가 실행하며
이 레포의 검사로는 확인하지 않는다. 대조 범위와 한계는 [그 레포의 checks](https://github.com/zilhak/tasty-page/tree/main/checks)에서 확인한다.

앱 문구(`lang/*.toml`)의 용어 일관성은 `one_concept_one_word_on_the_user_facing_surface.rs`처럼
사용자에게 보이는 문자열을 기준으로 검사할 수 있다.

자동 검사가 어려운 의미 판단과 분류·사유의 등록 여부도 구분한다.
`agent_facing_reads_of_active_state_are_classified.rs`는 관련 코드가 사유와 함께 분류됐는지
확인한다. 이것이 사유의 타당성까지 증명하지는 않는다.
가드가 있다는 사실만으로 보호 범위를 단정하지 말고 실제 조건식과 실패 메시지를 읽는다.

#### 간헐 실패를 조사할 때

시간·프로세스·외부 자원이 관련된 실패는 같은 커밋과 같은 명령으로 한 번 재실행해 비교한다.
두 실행의 성공 여부, 검사 대상 수, 소요시간, 실제 실패 메시지를 함께 남긴다.
재실행 통과는 간헐성이 있다는 증거이며 직전 변경이 원인이 아니라는 증명은 아니다.
다시 실패하면 반복 재시도로 통과를 고르지 말고 재현된 실패를 조사한다.
컴파일·포맷 오류나 정적 검사에서 코드로 원인이 드러나면 바로 수정한다.

실패한 파일을 직접 고치지 않았더라도 호출자·의존성·같은 프로세스의 다른 테스트가 영향을
줄 수 있다. 환경변수, 전역 락, 스레드, fd, 포트 같은 공유 자원도 조사한다.
시간 초과와 러너 부하를 구분하려면 전체 소요시간만 보지 말고 다른 테스트의 시간과 해당
테스트의 제한시간을 비교한다. 이 비교만으로 다른 가능한 원인을 모두 배제하지 않는다.

문서만 담은 push는 크로스플랫폼 잡의 경로 필터에서 제외될 수 있다.
문서 가드는 별도의 `doc-guards.yml`이 실행한다. 로컬에만 있는 커밋은 push 전까지
GitHub의 자동 검사를 받지 않는다. 특정 실행을 검증 근거로 들 때는 로그까지 확인한다.

<a id="그런데-그-둘은-서로-다른-결함이-아니다--판정문이-같은-방향을-가리킨다"></a>
<a id="채널의-상태는-한-run-이-아니라-창으로-적는다--그리고-그-값은-안-낡는다"></a>
<a id="과거값-2026-09-06--2026-09-08--crossplatform-check-최근-25-run"></a>
<a id="방향은-실패문이-이미-답한다--자식이-안-죽는다"></a>
<a id="이-문서는-그것을-안-고친다--이유는-소유가-아니라-검증-채널이다"></a>
<a id="그-값이-커버리지-주장-하나에-붙는다"></a>
<a id="잡별로-갈라-세려면-run-마다"></a>
<a id="다섯째-갈래--그-회차에-워크플로가-아예-안-켜진-것"></a>
<a id="1-경로-필터가-없는-워크플로의-head_sha--main-push-전수"></a>
<a id="doc-guards--format-check--test-셋은-branches-main-만-걸려-있어-매-push-에-켜진다"></a>
<a id="2-그-sha-에서-실제로-켜진-워크플로"></a>
<a id="3-그-push-가-담은-것--앞-push-의-tip-부터-이-tip-까지"></a>
<a id="그-모형이-관측을-재현하는지-먼저-봐라--이것이-이-측정의-양성-대조다"></a>
<a id="안-켜진-횟수-과거값--2026-09-08--main-push-구간-59"></a>
<a id="물음은-필터가-옳은가-가-아니라-그-push-가-담은-것을-보는-채널이-하나라도-있는가"></a>
<a id="pages-만-구조상-가능하다--그리고-그-0-은-우연이-아니다"></a>
<a id="여섯째-갈래--같은-커밋이-두-번-돌아-다른-답을-낸-것-재실행"></a>
<a id="실측-2026-09-08--워크플로-셋--최근-40-실행--run-120"></a>
<a id="재실행은-ㄱ-층을-새로-만든다"></a>
<a id="run_attempt-를-그-판정에-쓰지-마라--안-돈-잡도-2-라고-답한다"></a>
<a id="시도-n-에-실제로-돈-잡--started_at-이-시도-n-1-의-것과-다른-잡"></a>
<a id="판정기를-안-짓는다--칸-ㄱ--되돌아올-조건은-값이다"></a>
<a id="재실행-결과와-실패-원인을-구분한다"></a>
<a id="실패-메시지에서-공통-조건을-확인한다"></a>
<a id="규율-넷째-덮인-필터--안-켜진-것이-안-본-것은-아니다"></a>
<a id="규율-재방송에는-처방을-안-건다--가르는-물음은-앞-스텝이-뒤-스텝의-전제인가"></a>
<a id="남은-자리-여섯을-전부-갈랐다--다섯은-좌변이-아니라-negative-control-이다"></a>
<a id="여섯째는-왜-지금-처방을-안-넣는가"></a>

### 실행 이력을 해석하는 기준

현재 커밋의 성공 여부와 반복되는 실패는 따로 확인한다. 이력을 비교할 때는 기간, 대상 커밋,
명령, 검사 수, 실제 실패 메시지를 기록한다. 다른 OS에서도 실패했다는 사실만으로 공통 코드가
원인이라고 단정하지 않는다. 같은 시간 제한에 걸린 실패도 자식 상태와 이벤트 기록을 대조한다.

워크플로가 시작되지 않은 경우, 잡이나 단계가 생략된 경우, 검사가 실패한 경우를 구분한다.
`continue-on-error`는 자기 단계의 실패를 허용할 뿐 앞 단계 실패 뒤 실행을 보장하지 않는다.
`if: !cancelled()`는 앞 단계 실패 뒤에도 실행하지만 취소된 실행까지 이어 가지는 않는다.

#### 뒤 단계 실행 조건을 바꿀 때

앞 단계가 뒤 검사의 필수 준비인지 먼저 확인한다. checkout이나 clippy 설치가 실패했다면
준비 없이 검사를 실행해도 같은 원인으로 실패한다. 줄바꿈 진단처럼 필수 준비가 아닌 단계는
실패해도 뒤 검사를 실행할 가치가 있다. 필요한 준비 단계도 함께 실행되도록 조건을 검토한다.
`SWALLOWABLE_STEPS`와 `PROTECTED_STEPS` 검사는 조건의 유무와 등록 수를 확인하며
GitHub 조건식의 의미나 단계 간 의존성을 증명하지 않는다.

헤드리스 check와 test처럼 같은 빌드 조합을 확인하는 단계에는 이력만 보고 조건을 추가하지
않는다. 새 단계가 앞에 생기면 필수 준비인지 다시 판단한다. macOS의 `fd budget`은 soft 상한이
4096보다 낮으면 실패하지만 테스트 실행 자체의 전제는 아니다. 이 실패로 유닛 검사가 생략되는
사례가 생기면 실행 조건을 재검토한다.

#### 경로 필터와 재실행

경로 필터가 있는 워크플로는 실행 목록만으로 누락을 셀 수 없다. 경로 필터가 없는 main push
이력을 기준으로 각 push의 변경 경로와 실제 실행 목록을 비교한다. 과거 실행을 조사할 때는
`git show <sha>:.github/workflows/<이름>.yml`로 그 시점의 필터를 읽는다. 예측한 실행 목록과
실제 목록이 다르면 필터 문제로 결론 내리기 전에 조사 방법부터 확인한다.

필터가 검사 입력을 모두 포함하면 제외된 변경은 해당 검사를 바꾸지 않는다. 입력 중 일부가
빠지면 필터를 넓혀야 한다. 외부 보안 권고처럼 파일 변경 없이 달라지는 입력은 주기 실행으로
확인한다. 플러그인 버전 검사는 path 의존성과 `src/`·`lang/`·`assets/` 아래 Markdown도
검사하므로 단순히 모든 `.md`를 제외하면 안 된다.

홈페이지·사용자 가이드·디자인 정본의 빌드, 링크·대비 검사, 가이드와 본체의 대조, 배포는
[tasty-page](https://github.com/zilhak/tasty-page) 레포의 워크플로(`pages.yml`·`checks.yml`·`design.yml`)가
실행한다. 범위와 트리거는 [그 레포의 워크플로](https://github.com/zilhak/tasty-page/tree/main/.github/workflows)와
[사이트 문서](https://github.com/zilhak/tasty-page/blob/main/docs/dev/site.md)에서 확인한다.

앱의 토큰·아이콘이 디자인 정본과 같은지는 `doc-guards.yml`이 확인한다. 이 워크플로는
`crates/tasty-design-tokens/dtcg/SOURCE`에 기록한 tasty-page 커밋을 받아 `TASTY_PAGE_DIR`로 넘기고,
`design_source_*` 시험이 그 트리와 앱을 비교한다. 경로 필터가 없으므로 SOURCE만 바꾼 push에서도 실행된다.
CI 통과는 앱이 기록한 커밋과 같다는 뜻이며, tasty-page `main`이 그 뒤에 바뀌었는지는 확인하지 않는다.

재실행 뒤 성공만 보고 앞선 실패를 지우지 않는다. 실패한 잡만 재실행하면 나머지 잡은 이전
결과를 사용한다. 시도별 jobs 응답의 `run_attempt`만으로 실제 재실행을 판단하지 말고
`started_at`을 이전 시도와 비교한다.

```bash
gh api "/repos/<owner>/<repo>/actions/runs/<run-id>/attempts/<n>/jobs" \
  --jq '.jobs[] | "\(.name)\t\(.conclusion)\t\(.started_at)"'
```

재실행 이력을 자동 집계하는 별도 도구는 두지 않는다. 최근 40회 실행에서 재실행한 run이
8개를 넘거나 결과가 바뀐 run이 3개를 넘으면 수동 확인 비용을 다시 검토한다.
서로 다른 커밋에서 간헐적으로 실패하는 시험은 이 집계로 확인할 수 없다.

#### 러너 비용

self-hosted 잡들은 러너를 공유하므로 실행 대기와 빌드 시간을 함께 본다.
checkout의 정리로 `target/`이 삭제되는 구성에는 로컬 증분 빌드 시간을 적용하지 않는다.
해당 작업 디렉터리의 산출물은 정리되지만 Cargo 캐시나 다른 경로까지 비워진다는 뜻은 아니다.
디스크 여유는 그 실행의 최고 사용량과 비교하고, 로그의 `df`·`du` 값에 측정 시점을 남긴다.

### 잡 시간 상한

모든 잡은 `timeout-minutes`를 둔다. 없으면 멈춘 시험이 GitHub Actions 기본값 360분 동안
러너를 붙잡는다. 상한은 러너를 기다린 시간을 세지 않고 잡이 실행된 시간만 센다.

상한은 성공·실패로 끝난 실행 가운데 가장 긴 시간에 배수를 곱해 5분 단위로 올리고, 10분보다
작으면 10분으로 둔다. 배수는 3이다. 표본이 5회 미만이거나 같은 명령을 같은 러너(또는 같은
호스트의 다른 러너)에서 실행하는 다른 잡의 실측을 대신 쓰는 잡은 5다. 측정한 실행은 checkout이 `target/`을 지운 뒤 처음부터
빌드했으므로 실측값에 콜드 빌드가 이미 들어 있다. 배수는 공유 러너의 부하 차이를 흡수한다.

- 실측: `gh run list --workflow=<파일>`로 실행을 고르고 `gh run view <run-id> --json jobs`의
  `startedAt`·`completedAt`으로 잡 시간을 잰다. 단계가 하나도 없는 취소 잡은 러너를 기다리다
  취소된 것이라 제외한다. 취소된 잡은 끝까지 실행되지 않았으므로 최대에 넣지 않는다.
- 측정 범위: crossplatform-check·supply-chain-check·test는 2026-09-17~2026-10-08의 전체
  실행, release는 2026-09-26의 1회(Windows 두 잡은 2026-09-26~2026-11-10의 전체 실행 5회),
  나머지는 2026-10-07~2026-10-08의 최근 30회다.
- build-check는 같은 기간 실행 기록이 없어 같은 빌드 스크립트를 같은 러너에서 실행하는
  release 잡의 실측을 쓴다. build-check의 build-windows-arm64는 같은 스크립트를 같은 러너
  (`win-arm64`)에서 실행하는 release build-windows-arm64의 실측을 쓴다. release build-windows-arm64의
  실측은 전용 러너로 옮기기 전 x64 러너에서 x64 잡이 끝난 뒤 따로 실행한 값이다. 같은 호스트에서
  두 Windows 빌드가 동시에 돌 때의 시간은 미측정이다. check-windows-arm64도
  실행 기록이 없어 같은 호스트의 다른 러너에서 더 많은 작업(clippy·테스트)을 하는 check-windows의 실측을 쓴다. test-linux-x64도 실행 기록이 없어 같은 러너에서 workspace
  시험을 실행하는 check-headless의 실측을 쓴다. check-android도 실행 기록이 없어 같은 러너에서 `cargo check`를
  실행하는 check-release의 실측을 쓴다.

- macOS dist 빌드 잡은 배수를 쓰지 않는다. 이 러너는 업무용 맥과 함께 쓰며, 부하가 높을 때 40분 안에 끝나지 못해 180분으로 둔다.

| 워크플로 | 잡 | 성공 | 실패 | 실측 최대(분) | 배수 | timeout-minutes |
|---|---|---|---|---|---|---|
| build-check.yml | build-macos | 0 | 0 | 7.9 (release 대리) | 공유 러너 예외 | 180 |
| build-check.yml | build-windows | 0 | 0 | 25.9 (release 대리) | 5 | 130 |
| build-check.yml | build-windows-arm64 | 0 | 0 | 25.9 (release 대리) | 5 | 130 |
| build-check.yml | build-linux-x64 | 0 | 0 | 9.3 (release 대리) | 5 | 50 |
| build-check.yml | build-linux-arm64 | 0 | 0 | 7.2 (release 대리) | 5 | 40 |
| complexity-check.yml | check-file-size | 24 | 0 | 0.3 | 3 | 10 |
| crossplatform-check-macos.yml | check-macos | 6 | 86 | 12.4 | 3 | 40 |
| crossplatform-check.yml | check-windows | 5 | 81 | 13.7 | 3 | 45 |
| crossplatform-check.yml | check-windows-arm64 | 0 | 0 | 13.7 (check-windows 대리) | 5 | 70 |
| crossplatform-check.yml | check-headless | 26 | 38 | 15.7 | 3 | 50 |
| crossplatform-check.yml | check-release | 93 | 3 | 1.7 | 3 | 10 |
| crossplatform-check.yml | check-android | 0 | 0 | 1.7 (check-release 대리) | 5 | 10 |
| doc-guards.yml | doc-guards | 29 | 0 | 1.8 | 3 | 10 |
| format-check.yml | fmt | 30 | 0 | 0.4 | 3 | 10 |
| plugin-version-check.yml | version-bump | 25 | 0 | 0.4 | 3 | 10 |
| release.yml | create-release | 1 | 0 | 0.2 | 5 | 10 |
| release.yml | build-macos | 1 | 0 | 7.9 | 공유 러너 예외 | 180 |
| release.yml | build-windows | 5 | 0 | 25.9 | 3 | 80 |
| release.yml | build-windows-arm64 | 3 | 0 | 25.9 | 5 | 130 |
| release.yml | build-linux-x64 | 1 | 0 | 9.3 | 5 | 50 |
| release.yml | build-linux-arm64 | 1 | 0 | 7.2 | 5 | 40 |
| release.yml | publish-release | 1 | 0 | 0.2 | 5 | 10 |
| script-gates.yml | script-gates | 22 | 0 | 1.0 | 3 | 10 |
| supply-chain-check.yml | cargo-deny | 2 | 73 | 0.4 | 3 | 10 |
| test.yml | semver-guards | 99 | 29 | 2.5 | 3 | 10 |
| test.yml | test-linux-x64 | 0 | 0 | 15.7 (check-headless 대리) | 5 | 80 |

잡을 추가하거나 단계를 늘려 실행 시간이 크게 바뀌면 같은 방법으로 다시 재고 표와 워크플로를
함께 고친다. 상한에 걸린 실행이 멈춤이 아니라 정상 실행이었다면 그 잡을 다시 잰다.
대리 실측을 쓰는 잡은 처음 실행된 뒤 자기 실측으로 바꾼다.
`crates/tasty-doc-guards/tests/ci_job_timeouts_match_the_guide.rs`가 모든 잡에 정수
`timeout-minutes`가 있는지와 이 표의 값이 워크플로와 같은지를 양방향으로 대조한다.
실측값과 배수의 계산은 검사하지 않는다.

### 연속 push와 실행 취소

워크플로마다 `concurrency` 그룹을 ref 단위로 두고 같은 ref의 새 실행이 들어오면 이전 실행을
취소한다. crossplatform-check만 다르다. `cancel-in-progress`가
`${{ github.event_name == 'pull_request' }}`라서 PR에서는 이전 실행을 취소하고, main push와
수동 실행에서는 실행 중인 run을 끝까지 둔다.

- 이유: Windows·headless 잡은 13~16분 걸리고 main push 간격은 그보다 짧은 경우가 많다.
  모두 취소하면 끝까지 가는 run이 거의 남지 않는다.
- 대기(GitHub Actions 문서 기준): 그룹마다 실행 중 run 하나와 대기 run 하나를 둔다. 새 push가 오면
  대기 run이 새 run으로 바뀌므로 대기열은 쌓이지 않는다. 그래서 끝난 run의 다음 실행은 그때의
  최신 커밋을 잰다. 결과는 최신 push보다 한두 커밋 늦을 수 있고, 그 사이 커밋은 따로 재지 않는다.
- 비용: main push가 이어지는 동안 Windows·X64 러너가 계속 이 그룹의 run을 실행한다.
- macOS 잡은 이 그룹에 없다. `crossplatform-check-macos.yml`이 같은 트리거로 따로 실행하고
  `cancel-in-progress: true`로 이전 run을 취소한다. macOS 러너가 offline이면 잡이 러너를 기다리며
  run이 끝나지 않는다. 같은 그룹에 두면 그 run이 그룹의 실행 자리를 차지해 이후 run이 모두
  대기하다 교체된다. 분리하면 Windows·Linux 잡은 macOS 러너 상태와 관계없이 끝까지 가고, macOS
  대기 run은 새 push가 취소하므로 쌓이지 않는다. 대신 macOS 잡도 실행 도중 다음 push에
  취소될 수 있다.
- 다른 워크플로는 잡이 몇 분 안에 끝나 실행 도중 취소되는 일이 드물어 그대로 취소한다.
  잡이 push 간격만큼 길어지면 같은 방식을 검토한다.

결과를 읽을 때 대기 중에 교체된 run은 실행되지 않은 것이다. 실행 중 취소와 구분하려면
[잡과 단계의 결과를 확인한다](#잡과-단계의-결과를-확인한다)를 따른다.

<a id="미측정-구간의-길이--이-문서가-가진-적-없던-축"></a>

### 성공 확인 사이의 간격

검사별 실행 여부와 성공 확인 사이의 간격을 구분한다. 마지막 성공 이후 실패한 검사가
실행됐다면 그 전체 기간을 미측정이라고 부르지 않는다. 단계별 시작·종료·생략 상태로
어떤 검사가 실행되지 않았는지 확인한다. 소요시간만으로 생략된 검사 목록을 추측하지 않는다.

```bash
gh run list --workflow=crossplatform-check.yml --limit 20 \
  --json databaseId,headSha,createdAt
gh run view <run-id> --json jobs
```

검사하는 OS·feature·플래그가 다른 실행으로 빈 기간을 채웠다고 보고하지 않는다.
뒤 커밋의 성공은 중간 커밋 각각의 성공을 소급해 증명하지 않는다.

### push 범위 안쪽의 커밋 — 어느 채널도 안 보고, **안 보기로 했다**

검증을 요구하는 커밋은 push 끝점과 작업을 병합한 끝점이다.
push 범위의 모든 중간 커밋을 다시 빌드하도록 요구하지 않는다.

| 검사 | 대상 |
|---|---|
| pre-commit | staged 트리. P.1의 비교 기준은 `main`과의 merge-base |
| pre-push B.9 | Git이 전달한 원격 tip과 로컬 tip의 차이 |
| pre-push B.4~B.8, B.10 | 훅이 실행되는 작업 트리 |
| CI | push된 tip. 플러그인 버전 검사는 `before`와 tip의 차이 |

검사 결과에는 커밋과 미커밋 변경 유무를 함께 적는다.
이미 뒤 커밋에서 고친 중간 실패만을 없애려고 공유 이력을 다시 쓰지 않는다.
병합 전 amend/fixup은 가능하지만 필수는 아니다.
원인을 좁힐 때는 검증된 끝점을 비교하고, `git bisect`에서는 검증되지 않은 중간 커밋을
필요에 따라 skip한다. 중간 커밋이 항상 빌드된다고 가정하지 않는다.

<a id="러너는-작업-트리를-재사용한다--채널의-성질이지-그-잡의-상태가-아니다"></a>
<a id="10-self-hosted-linux-x64----2-self-hosted-linux----2-self-hosted-linux-arm64"></a>
<a id="3-self-hosted-windows-------3-self-hosted-macos"></a>
<a id="작업-트리가-커밋과-다른데-git-이-같다-고-말하는-상태"></a>
<a id="되돌리는-절차--함정이-둘이다"></a>
<a id="재현은-증상이-아니라-값이-맞아야-재현이다"></a>

### self-hosted 러너의 작업 트리

self-hosted 러너는 작업 디렉터리를 재사용한다. checkout의 `git clean -ffdx`는 미추적 파일을
지우지만, 줄바꿈 정책이 바뀌었다고 모든 추적 파일을 다시 쓰지는 않는다. 따라서
`.gitattributes`가 LF를 요구하고 `git status`가 깨끗해도 기존 CRLF 파일이 남을 수 있다.

```bash
git ls-files --eol
```

Windows 잡의 정규화 단계는 CRLF 파일을 먼저 지운 뒤 `git checkout --force -- .`로 다시
꺼내고 before/after를 기록한다. after가 0이 아니면 실패한다. 삭제 없이 checkout만 하거나
`checkout-index`로 파일만 갱신하면 내용 또는 인덱스의 stat 정보가 남을 수 있다.
이 절차는 CI의 정리된 체크아웃을 위한 것이며 미커밋 작업이 있는 개발 트리에 적용하지 않는다.

재현할 때도 인덱스가 CRLF 크기를 기록한 상태를 만들어야 한다. 파일에 CRLF만 넣으면
stat 차이로 Git이 다시 쓰므로 원래 문제와 다른 조건이 된다.

### 트리거는 어느 ref 의 것인가 — 작업 트리와 원격이 갈린다

문서에서 설명한 설정과 GitHub가 실제 읽은 설정이 같은지 확인한다.
이벤트가 사용하는 원격 ref의 워크플로 파일 전체를 조회해 트리거와 명령을 함께 비교한다.
갱신하지 않은 `origin/main`만으로 현재 원격 상태를 단정하지 않는다.

```bash
git fetch origin main
git show origin/main:.github/workflows/<이름>.yml
git diff origin/main -- .github/workflows/<이름>.yml
gh run list --workflow=<이름>.yml --limit 20
```

새 워크플로는 처음 포함된 push에서도 실행될 수 있다. 다음 push까지 기다려야 한다고
가정하지 말고 실행 이력을 조회한다. 로컬·원격 차이는 push 때 바뀌므로 고정값으로
여러 문서에 복제하지 않는다. 오프라인 문서 검사에 네트워크 조회를 추가하거나 사람이
갱신해야 하는 원격 스냅샷을 두지는 않는다.

### 잡과 단계의 결과를 확인한다

실행됐다는 주장에는 해당 잡과 단계의 결과가 필요하다.
`gh run view <run-id> --json jobs`로 확인하고 실패 원인은 `--log-failed`로 읽는다.
수동 실행 명령이 있다는 것과 실제로 실행해 본 것은 다르다.
실행한 적이 없다면 소요시간과 러너의 디스플레이 등 실행 조건도 확인되지 않은 상태다.
조건에 따라 생략된 잡은 이름이 목록에 있어도 실행 횟수에 포함하지 않는다.

지속되는 실패를 방치하면 새 실패를 구분하기 어려워진다. 앞 단계 실패로 뒤 단계가
생략됐다면 두 결과를 따로 보고한다. 구체적인 분류는 위 '실행 결과를 보는 단위'를 따른다.
검사가 주장한 사실을 실제로 확인하는지는 워크플로 설정만으로 알 수 없다.
검사 자체의 변이 검증은 [문서 작성 규칙](../documentation-model.md#6-작성-규칙-요약)을 따른다.

### 규약 — 채널 주장은 **작업 트리 기준**으로 쓰고, ③층은 여기서만 말한다

#### CI 설명을 작성하고 결과를 확인하는 기준

문서의 CI 설명은 작업 트리의 `.github/workflows/`에 선언된 내용을 기준으로 한다.
실제로 실행됐다는 주장을 하려면 원격 ref와 실행 시각 또는 조회 명령도 남긴다.
다음 항목을 구분한다.

1. 명령이 해당 테스트 타깃과 빌드 조합을 실행하는가.
2. 변경 경로와 이벤트가 워크플로 트리거에 포함되는가.
3. 그 설정이 GitHub가 읽는 원격 ref에 반영됐는가.
4. 해당 잡과 테스트 단계가 실행됐으며 결과를 확인했는가.

다른 문서에서 로컬·원격의 차이를 설명할 때는 이 절을 가리킨다.
원격 실행 사실을 별도로 기록한다면 그 문장에도 관측 시점이나 조회 방법을 적는다.

<a id="반대-방향--배선돼-있는데-아무-문서도-안-적은-채널"></a>
<a id="안-돈다-를-쓰기-전에-두-가지를-갈라라"></a>
<a id="소스-스캔-가드는-지금-어디서-도는가-인구조사"></a>
<a id="모수필터-뒤까지-포함한-순수-스캔-가드-전체"></a>
<a id="첫-행필터-없는-채널을-가진-것"></a>
<a id="순수-소스-스캔-가드-세기--통합-타깃-중-레포-파일을-읽고-프로세스는-안-띄우는-것"></a>

### 자동 검사 설명의 범위

모든 자동 잡을 이 표에 복제하도록 강제하지 않는다. release의 배포 잡은 산출물
배포 절차이며 [릴리스](release.md)에서 설명한다.

컴파일, 실행, 실패 차단은 별개다. clippy `--all-targets`는 통합 타깃을 컴파일하지만
시험을 실행하지 않는다. 헤드리스 전체 스위트는 통합 타깃을 실행하고, 기본 feature의
`--lib --bins`는 통합 타깃을 실행하지 않는다. 타입 검사·deny lint·로컬 훅도 각자의 범위에서
규칙을 검사하므로 워크플로가 없다는 이유만으로 검증 수단이 없다고 쓰지 않는다.
파일 SLOC 검사의 종료코드는 0(통과), 1(위반), 2(측정 실패)다. 검사 도구 오류나 빈 결과를
위반 없음으로 취급해서는 안 된다.

### 소스 스캔 가드의 실행 경로

런타임에 저장소 파일을 읽는 가드는 컴파일만으로 검사 결과를 얻을 수 없다.
`filtered_guards_are_not_totally_blind.rs`는 워크플로의 경로 필터와 가드 입력을 대조한다.
입력이 전부 제외되면 실패하고, 일부만 제외되는 가드는 `PARTIALLY_FILTERED`에서
사유와 함께 관리한다. 문서를 읽으며 제품 크레이트를 링크하는 예외는 `DEP_BEARING`에
등록하며 새 누락과 불필요해진 등록을 양쪽으로 확인한다.

`cli_method_table_parity`는 문서와 CLI 코드를 함께 읽는 부분 제외 사례다. 현재는 제품의
`METHOD_TABLE`·`DEBUG_METHODS`를 사용하므로 단순 파일 이동으로 의존성 없는 문서 가드가
되지 않는다. 입력이 모두 필터 밖으로 바뀌면 분리하거나 텍스트 판독으로 옮길지 재검토한다.
`changelog_unreleased`는 별도의 semver 잡이 이름을 지목해 실행하므로 해당 이름과 소유
패키지를 확인하지 않고 이동하면 그 명령이 실패한다.

필터 없는 실행 경로가 유지되는지는 `filter_free_channel_still_exists.rs`가 별도로 검사한다.
특정 디렉터리에 있다는 이유로 면제하지 않고, 워크플로에서 타깃·패키지·전체 실행을 읽는다.
태그 전용 push와 수동 전용 잡을 매 main push 검사로 세지 않는다. 공용 판독 결과는 다음으로
확인한다. 실행 전 `resolve_judge`와 `--check-fresh`의 최신성 검사를 따른다.

```bash
cargo build -p tasty-doc-guards --bin workflow-channels
./target/debug/workflow-channels .
```

공용 바이너리와 라이브러리의 결과 일치는 사본 간 차이를 검사할 뿐 둘의 정확성까지
증명하지 않는다. 파서 회귀와 실제 검사 범위는 별도 합성 입력·변이로 확인한다.
문자열로 `Command::new` 등을 찾는 스캔 분류는 해당 문자열을 예시로 가진 가드를 제외할 수
있으므로 그 결과를 저장소의 정확한 가드 총수로 쓰지 않는다.

헤드리스 테스트 스텝은 `--skip`을 쓰지 않는다. 창이 필요한 시험은 `#[cfg(feature = "gui")]`로
그 조합에서 컴파일하지 않는다([유닛 테스트 격리 — 사용자 환경을 읽지 않는다](unit-test-isolation.md)).
`headless_skip_names_are_exact.rs`는 워크플로의 헤드리스 스텝에서 skip이 0건인지 확인하고,
합성 스텝으로 판독기가 skip을 여전히 읽는지도 확인한다. skip이 다시 생기면 각 이름이
테스트 하나와만 일치하는지도 검사한다. 상세 계수 방식과 한계는 그 가드의 모듈 설명을 따른다.

## 테스트는 **어디 있느냐**로 채널이 갈린다

위 표에서 가장 자주 오해되는 줄이다. 자동 잡이 돌리는 테스트 명령은 **조합마다 다르다** —
기본 조합은 좁혀져 있고(`--lib --bins`, 또는 `--test <이름>` 으로 이름 지목), 헤드리스
조합만 전체 스위트를 돌린다. 그래서 **같은 주제의 두 가드라도 파일이 어디 있느냐에 따라,
그리고 같은 파일이라도 조합에 따라 채널이 갈린다.**

| 테스트가 어디 있나 | 자동 **실행** | 자동 **컴파일** | 실례 |
|---|---|---|---|
| lib 유닛 테스트 (`src/`·`crates/*/src/` 안의 `#[cfg(test)] mod tests`) | **있다** — 두 조합 모두가 유닛 타깃을 포함한다. 기본 조합은 `crossplatform-check`·`crossplatform-check-macos` 의 **세 잡 모두**가 `--lib --bins` 로 돌린다(`check-macos` · `check-windows` · `check-headless` 의 `cargo test (linux, gui, unit)` 스텝), 헤드리스 조합은 `check-headless` 의 전체 스위트가 담는다. 한때 조합 격자에 빈 칸(Linux + gui + debug)이 있었고 지금은 그 gui 스텝이 채운다 — 아래 절 | 있다 | `ui_font_size_tokens_are_integers_at_every_zoom` |
| 통합 테스트 (`tests/*.rs`) | **헤드리스 조합에만 있다** — `check-headless` 가 전체 스위트를 돌린다(gui feature 로 묶여 이 조합에서 컴파일하지 않는 `multi_window_owner_routing` 은 같은 잡의 관측용 gui/Xvfb 스텝이 돌리지만 `continue-on-error` 라 **차단하지 않는다**). **기본 조합에는 없다** — 그 조합의 세 잡은 `--lib --bins` 이고(예외는 Windows 잡이 지목하는 `-p tasty-shm -p tasty-doc-guards` 뿐이다) `test.yml` 의 전체 스위트는 `workflow_dispatch` 전용 그리고 `check-headless` 는 `paths-ignore: docs/** · **/*.md` 뒤에 있어 **문서만 바뀐 push 에서는 이 칸이 통째로 비는 것**에 유의한다 | **있다** — clippy `--all-targets` 가 타깃으로 잡는다 | `tests/i18n_key_parity.rs` |
| 문서 가드 통합 테스트 (`crates/tasty-doc-guards/tests/*.rs`) | **있다 — 두 조합과 무관하게** `doc-guards.yml` 이 `-p tasty-doc-guards` 로 돌리고, **Windows 잡도 같은 지목으로 돌린다**(그쪽은 OS 축을 연다). 이 잡에는 경로 필터가 없어 문서만 바뀐 push에서도 실행한다([가드 검증](guard-verification.md)). `check-headless` 의 전체 스위트에서도 함께 돈다 | 있다 | `crates/tasty-doc-guards/tests/no_checkbox_in_docs.rs` |
| SemVer 가드 3종 | **있다** — `semver-guards` 가 `--test` 로 이름을 지목한다 (main push) | 있다 | `api_baseline_0_7` · `changelog_unreleased` · `cli_naming_count_drift` |
| 포맷 | **있다** — `format-check.yml` (main push · PR) + pre-commit | — | `cargo fmt --check` |

구조 저널 관련 시험도 위 표의 줄을 그대로 따른다. `tasty-core`·`tasty-event-store` 크레이트 안의 유닛 시험과
본 바이너리의 `src/runtime/`·layout importer 유닛 시험은 lib 유닛 줄이라 기본 조합의 세 잡과 헤드리스 전체 스위트에서 돈다.
실제 프로세스를 강제 종료하는 `crates/tasty-event-store/tests/crash.rs`와 잠금 상속을 확인하는
`crates/tasty-event-store/tests/lock_inheritance.rs`는 통합 테스트라 자동 잡 중에서는 `check-headless` 잡에서만 돈다.
뒤의 것은 `#![cfg(unix)]`라 Windows에서는 시험이 비어 있다.

<a id="처방을-낼-때는-그-처방을-재는-채널의-이름을-함께-적는다"></a>
<a id="새-파일을-만들었으면-그-파일이-사는-패키지-밖도-돌려라"></a>
<a id="같은-파일이라도-컴파일-채널과-실행-채널이-다르다--결론에-둘을-갈라-적는다"></a>
<a id="macos-유닛-테스트의-비용은-이-잡의-시간이-아니다"></a>
<a id="회차-하나의-잡별-시간--최댓값이-임계경로다"></a>
<a id="한-잡-안의-스텝별-시간"></a>
<a id="macos-잡의-fd-예산--여유가-남아-있는지-단정한다"></a>
<a id="측정값-2026-09-06--run-33994212447--commit-5d00e2641"></a>
<a id="다시-재는-법"></a>
<a id="러너-쪽-상한--잡-로그에서"></a>
<a id="우리-쪽-최고-fd--테스트-바이너리를-직접-띄우고-proc-를-표본한다"></a>
<a id="그-경로를-백그라운드로-띄우고-도는-동안-ls-procpidfd--wc--l-의-최댓값을-잡는다"></a>
<a id="안-쟀다--macos-쪽-최고-fd-그리고-재려면-무엇이-필요한가"></a>
<a id="조합-격자의-빈-칸--linux--gui--debug-지금은-채워져-있다"></a>
<a id="크로스-빌드-없이-재는-길--windows-잡의-로그에-물어본다"></a>
<a id="tmpguitxt위에서-만든-gui-게이트-본체-유닛와-차집합"></a>
<a id="헤드리스-커버리지-는-두-가지를-섞어-부른다"></a>
<a id="헤드리스-고유--자기-바이너리를-띄우는-타깃-공용-하네스가-대신-띄우는-경우가-있어"></a>
<a id="cargo_bin_exe-만-보면-놓친다testscommonmodrs-가-spawn_diag-로-띄운다"></a>
<a id="그-잡이-실제로-무엇을-돌렸나--잡-로그가-정본이다"></a>
<a id="조합에서-사라지는-이유는-대개-파일-위치다-실측"></a>
<a id="조건부-allow-도-조합별로-린트-채널을-지운다"></a>
<a id="사람이-돌리는-것-자동-채널-없음"></a>

### 플랫폼과 빌드 조합을 함께 확인한다

Windows 경로 구분자 문제를 Linux 실행만으로 확인했다고 보고하지 않는다.
`source_text::repo_relative`의 실제 구분자 처리는 Windows 실행에서 확인하고,
`src/source_guards/repo_relative_paths.rs`는 공용 정규화 사용 여부를 소스로 검사한다.
두 검사는 다른 사실을 확인한다. `str::lines()`는 후행 CR을 제거하지만 `split('\n')`은
그렇지 않으므로 여러 줄 문자열 비교도 플랫폼별로 확인한다.

문서 가드의 Windows 컴파일은 clippy `--all-targets`, 실행은 Windows 잡의
`cargo test -p tasty-shm -p tasty-doc-guards`가 담당한다. 로컬 크로스 체크는 다음과 같다.
타깃 표준 라이브러리나 C 툴체인이 없어 멈춘 결과를 제품 소스 오류로 보고하지 않는다.

```bash
cargo check -p tasty-doc-guards --all-targets --target x86_64-pc-windows-msvc
```

파일을 추가하거나 옮기면 해당 패키지 밖의 가드도 찾는다.
크레이트의 src/tests 변경은 해당 크레이트와 루트 `tasty --lib` 검사를,
루트 src/tests 변경은 루트 lib와 `tasty-doc-guards` 검사를 확인한다.
`.githooks/pre-commit` W.2의 새 파일 안내와 아래 변경 영향 조사 절을 따른다.

### macOS 유닛 검사 비용과 fd 한도

잡의 실행 시간과 워크플로 완료 지연은 다르다. 병렬 잡은 시작 시각·대기·의존 관계를 포함해
완료 시각을 비교한다. macOS 잡이 가장 늦지 않더라도 러너 점유 비용은 남는다.
같은 ref의 연속 push는 취소 설정([연속 push와 실행 취소](#연속-push와-실행-취소))을 적용받지만 다른 ref의 실행은 공유 러너에 대기할 수 있다.
새 실행을 만들기 전에 기존 jobs와 steps 로그로 시간을 확인한다.

`fd budget`은 soft 상한이 4096 미만이면 실패하며 상한을 변경하지는 않는다.
실제 PTY와 자식 셸을 띄우는 시험(저널·PTY 하네스)의 fd 수요는 별도로 확인해야 한다. 공유 픽스처 `test_state()`는 PTY를 열지 않는다([유닛 테스트 격리](unit-test-isolation.md) §8).
2026-09-06 run 33994212447에서 macOS soft 상한은 10240이었다. 당시 Linux 테스트의
최고 fd는 기본 병렬도 966, threads=3에서 1157이었다. 이는 서로 다른 OS의 측정이며
macOS의 실제 여유를 증명하지 않는다. 병렬도를 줄이면 fd가 반드시 감소한다는 근거도 아니다.

한도 검사 실패 시 하한부터 낮추지 않는다. 해당 러너에서 실제 최고 fd와 수명을 측정한다.
Linux는 직접 실행한 테스트 PID의 `/proc/<pid>/fd`, macOS는 `lsof -p <pid>` 등을 이용하되
표본이 짧게 열린 fd를 놓칠 수 있음을 기록한다. OS·커밋·병렬도·측정 시점을 함께 남긴다.

### OS·feature·profile별 실행 범위

| 조합 | debug | release |
|---|---|---|
| macOS + gui | check-macos: 컴파일·유닛 | — |
| Windows + gui | check-windows: 컴파일·유닛·지목 통합 | — |
| Linux + headless | check-headless: 컴파일·전체 스위트(skip 제외) | — |
| Linux + gui | check-headless의 gui 유닛 단계·관측용 E2E·관측용 webview 레이아웃 | check-release: 컴파일 |

잡 성공만으로 표의 모든 단계를 실행했다고 판단하지 않는다. 단계별 conclusion을 읽는다.
GUI feature 또는 OS 전용 모듈 안의 테스트는 다른 조합에서 컴파일 대상에서 사라질 수 있다.
파일 자체에 cfg가 없어도 부모 모듈의 cfg, `#[path]`, cfg_attr, 매크로가 영향을 준다.
파일 이동이나 생성 시에도 노출 조합을 확인한다. 가드 자신이 cfg로 제외되면 런타임 스캔도
실행되지 않는다.

조합을 비교할 때 `cargo test ... -- --list`의 패키지·타깃·전체 테스트 이름을 보존한다.
다른 모듈의 동명 테스트를 합치지 않는다. 크로스 빌드 산출물의 문자열은 실행 증거가 아니며,
ASCII 중심 `strings`는 한글 이름을 놓친다. 로그에서 ignored와 실제 실행도 구분한다.

헤드리스 제품을 띄우는 E2E는 run_headless와 그 아래 IPC·PTY 경로를 검사한다.
단지 통합 타깃이라 헤드리스 잡에 포함된 순수 파싱·소스 가드는 feature와 무관할 수 있다.
전자는 GUI 실행으로 대체할 수 없고, 후자는 실제 입력과 구현이 같으면 다른 잡에서도 검사할 수 있다.

### 조건부 lint 허용

`cfg_attr(..., allow(...))`는 해당 조합의 lint를 끈다. 상위 모듈의 같은 allow가 남아 있으면
자식의 중복 allow만 지워도 lint가 다시 켜지지 않는다. 단, lint 우선순위는 적용 위치와
`forbid` 여부도 따르므로 모든 allow가 모든 deny를 이긴다고 일반화하지 않는다.
플랫폼·feature·release에 필요한 허용을 일괄 삭제하지 않는다. 알려진 위반을 넣어 해당
조합에서 잡히는지 확인한 뒤 각 허용의 이유를 판단한다.

## 사람이 실행하는 검사

결과는 [검사 결과를 읽는다](self-verification.md#검사-결과를-읽는다)의 방법으로 읽는다. 파이프 뒤의 종료 코드와 필드 위치로 읽은 수는 판정 근거가 아니다.

| 검사 | 명령 | 실행 시점 |
|---|---|---|
| 기본 feature 전체 스위트 | `cargo test --workspace --locked` | 병합 후 최종 트리. test.yml의 전체 실행 잡은 수동 전용 |
| Linux 기본 feature clippy | `cargo clippy --workspace --all-targets --locked` | 해당 작업의 최종 검증. 패키지만 지정한 결과로 대신하지 않음 |
| headless clippy | `cargo clippy --workspace --all-targets --no-default-features --locked` (`check-headless` 의 `cargo clippy (headless)` 스텝과 같은 명령) | headless 경로나 `cfg(not(feature = "gui"))` 코드를 바꿨을 때. 그 코드는 위 기본 feature clippy 에 보이지 않는다. `check-headless` 도 같은 명령을 돌리지만 경고만으로는 실패하지 않으므로 출력의 `warning` 줄을 직접 읽는다. `let_underscore_must_use` 경고는 막지 않는 명부다([error-handling](error-handling.md)의 「`clippy::let_underscore_must_use` 는 무엇을 세는가」) |
| dist 빌드 | `bash scripts/build-macos-dmg.sh` · `./scripts/build-windows.ps1` · `bash scripts/build-linux.sh` | build-check.yml 수동 실행 |

### 크레이트를 지목한 clippy 는 push 와 다른 feature 집합을 잰다

위 표의 "기본 조합 clippy" 행은 워크스페이스 **전체**를 말한다. lane 이 시간을 아끼려고
`cargo clippy -p <크레이트>` 로 좁히면 다른 물음에 답하게 된다 — cargo 는 `--workspace` 일
때 워크스페이스 전체의 feature 를 통합하고, `-p` 일 때는 그 크레이트의 의존 폐포 안에서만
통합한다. **그 차이가 판정을 가르는 자리가 실제로 있고, 패키지 검사에서는 통과한 함수가 push 검사에서
실패할 수 있다.**

- **기제.** 인지복잡도 lint(`cognitive_complexity = "deny"`, 문턱은 [complexity-gate](complexity-gate.md))는
  매크로 전개 뒤를 센다. `tracing` 의 `log` feature 가 켜지면 `tracing::info!`·`warn!` 한
  자리가 log 통합 분기까지 전개돼 훨씬 크게 세어진다. 그래서 같은 함수가 `-p` 에서는 문턱
  아래, `--workspace` 에서는 문턱 위가 된다.
- **누가 켜나.** Linux 에서 `tracing/log` 를 켜는 것은 `calloop` 다(← `calloop-wayland-source`
  ← `smithay-client-toolkit` ← `smithay-clipboard` ← `egui-winit`). 번들 plugin 크레이트는
  이 사슬 밖이라 `-p` 트리에는 `"log"` 가 없다. 헤드리스 조합(`--no-default-features`)의
  워크스페이스 트리에도 있다.
- **플랫폼마다 답이 다르다.** `cargo tree --workspace --target <triple> -e features -i tracing`
  에서 `tracing feature "log"` 가 나오는 것은 Linux 뿐이다 — `x86_64-pc-windows-msvc` ·
  `aarch64-apple-darwin` 트리에는 없다. 그래서 기본 조합 clippy 를 배선한 유일한 자동 잡인
  `check-windows` 는 **이 갈림을 못 본다** — 그 트리의 `tracing` feature 집합이 `-p` 쪽과
  같다. 이 문턱을 넘는 자리를 잡는 채널은 Linux 쪽 둘이다: pre-push `B.4`(훅을 깐
  체크아웃만)와 `check-headless` 의 clippy. 뒤쪽은 `--no-default-features` 라 `gui` 뒤에
  있는 코드는 안 본다 — 그 자리에서는 `B.4` 하나만 남는다.

실측 2026-09-23 — `tasty-plugin-claude` 에 `if` 셋과 `tracing` 매크로 넷을 가진 함수 하나를
심고(변이) 잰 뒤 원복했다. 변이 없는 트리에서는 앞 두 명령이 모두 rc=0 이었다.

| 명령 | rc | 판정 |
|---|---|---|
| `cargo clippy -p tasty-plugin-claude --all-targets -- -D clippy::correctness` | 0 | 문턱 안 넘음 |
| 같은 명령 + `--features tracing/log` | 101 | 인지복잡도 32 로 문턱 초과 |
| `cargo clippy --workspace --all-targets -- -D clippy::correctness` (pre-push `B.4` 와 같은 명령) | 101 | 같은 자리 32 로 문턱 초과 |
| `cargo clippy --workspace --all-targets --no-default-features --locked` (`check-headless` 와 같은 명령) | 101 | 같은 자리 32 로 문턱 초과 |
| `cargo clippy --workspace --all-targets --locked --target x86_64-pc-windows-msvc` (`check-windows` 의 대리) | **미측정** | 이 머신에서는 `libsqlite3-sys` · `mlua-sys` 빌드 스크립트에서 멈춘다. 위 "못 본다" 는 clippy 실측이 아니라 cargo tree 의 feature 집합에서 나온 추론이다 |

패키지만 지정한 통과 결과로 워크스페이스 검사를 대신하지 않는다. 비교 명령은 다음과 같다:

```bash
cargo tree --workspace -e features -i tracing \
  | grep -A1 'tracing feature "log"'        # 이 갈림이 지금 있는가 — 비면 없다
cargo clippy -p <크레이트> --all-targets --features tracing/log \
  -- -D clippy::correctness                 # 싼 쪽 — 이 한 갈래만 닫는다
cargo clippy --workspace --all-targets \
  -- -D clippy::correctness                 # push(B.4)와 같은 범위
```

가운데 명령은 그 크레이트가 `tracing` 을 의존으로 가질 때만 받아들여지고, `tracing/log` 라는
**알려진 한 갈래**만 맞춘다 — feature 통합이 만드는 다른 차이까지 맞추는 것은 마지막 명령
하나다. 첫 명령이 비게 되면(워크스페이스에서 `tracing/log` 를 켜는 의존이 사라지면) 이 갈래의
차이는 없어진다.

<a id="남은-것은-둘이고-둘-다-디스플레이를-요구한다-2026-09-05-실측"></a>
<a id="이-칸은-세-층이고-셋째에는-단일-값이-없다"></a>
<a id="도착-카나리아--키-자극을-재는-회차는-이-셋을-먼저-읽는다"></a>
<a id="누적이-5-에서-멎는-이유--누적이-아니었다-화살표가-반대다"></a>
<a id="그-기제를-값으로-닫는-형태--한-분기를-죽이고-넷이-같은-칸을-가리키는지-본다"></a>
<a id="alt-조합은-못-잰다-가-아니었다--셋업-조합이-낡았던-것이다"></a>
<a id="그래서-33-건을-다시-갈랐다--조합이-낡은-것--조합은-멀쩡한데-죽은-것"></a>
<a id="스텝은-앞-스텝이-죽으면-안-돈다--배선돼-있는데-채널이-없는-회차"></a>
<a id="그-진단은-세-회차-내리-0-회-였다-과거값--그리고-네-번째에-답이-나왔다"></a>
<a id="세-층-중-하나만-워크플로-파서에-기댄다--그-파서는-이제-고정돼-있다"></a>
<a id="자동-채널이-없는-것이-결함이-아닌-갈래"></a>
<a id="판정기가-없던-축--시험이-자기-비용에서-퇴행하는-것"></a>
<a id="지금-값--그리고-그-값이-무엇을-지탱하지-않는가"></a>
<a id="세-형태를-변이로-판정했다--둘은-기각-하나가-섰다"></a>
<a id="채택한-형태--두-번-부르고-둘째가-도출을-안-하는지-묻는다"></a>
<a id="그-3-자리에-하나씩-붙여-봤다--두-자리는-이미-덮여-있었고-안-덮인-것은-다른-것이었다"></a>
<a id="순서가-아니라-효과를-재려면-조건을-만들어야-한다--만들었다"></a>
<a id="채택한-형태의-경계--그리고-앞-회차의-이-자리가-틀렸다"></a>
<a id="script-gatesyml--배선한-날의-상태"></a>

### gui 단위 단계의 Xvfb

`check-headless`의 `cargo test (linux, gui, unit)` 단계는 `env -u WAYLAND_DISPLAY xvfb-run -a
--server-args="-screen 0 1920x1080x24"` 안에서 `cargo test --workspace --lib --bins --locked --no-fail-fast`를
실행한다. gui 조합의 단위 시험 중 `app::shutdown_machine`의 `capture_file_tests`·`settings_export_tests`·
`plugin_file_shutdown_tests`와 `app::journal::commands`의 `font_notice`는 winit `EventLoop`를 X11로 만들고,
`capture_file_tests`를 뺀 나머지는 같은 시험 바이너리를 자식 프로세스로 다시 실행한다. 디스플레이가 없으면 `XOpenDisplayFailed`로 실패한다. 러너에 `xvfb-run`이
없으면 단계가 먼저 실패한다. 이 단계는 `continue-on-error`가 없는 차단 단계다.

`#[ignore]` 시험은 디스플레이가 있어도 이 단계에서 실행되지 않는다. 디스플레이가 생겨 동작이 달라지는
다른 시험이 있는지는 로컬 전체 실행으로 비교한다.

```bash
env -u DISPLAY -u WAYLAND_DISPLAY xvfb-run -a --server-args="-screen 0 1920x1080x24" \
  cargo test --workspace --lib --bins --locked --no-fail-fast > gui-unit.log 2>&1; echo rc=$?
grep -E '^test result:' gui-unit.log | grep -vc ' ok\.'
```

<a id="단위-시험의-node"></a>

### 단위 시험의 node

`host_api::webview::same_document`의 `href_script_reports_only_fragment_moves_within_one_document`는 WebView 판정 스크립트를 node로 실제 실행한다. Linux에서만 컴파일하며 node가 없으면 건너뛰지 않고 실패한다. Linux 러너 머신에는 node가 없으므로 이 시험을 돌리는 `crossplatform-check.yml`의 `check-headless`와 `test.yml`의 `test-linux-x64`가 시험 단계 앞에서 `actions/setup-node`로 node를 준비한다. 다른 잡에 Linux `--lib` 시험을 추가하면 같은 단계를 넣는다.

로컬 Linux 개발 환경에서도 `cargo test --lib`에 node가 PATH에 있어야 한다. 없으면 이 시험 하나가 설치를 안내하는 메시지로 실패한다. Windows·macOS 잡은 이 시험을 컴파일하지 않는다.

### GUI 테스트를 실행하고 해석하는 조건

`multi_window_owner_routing`은 `gui` feature 조합에서만 컴파일된다. 헤드리스에서
`window.create`와 `window.list`를 지원하지 않기 때문이다. 그래서 헤드리스 전체 실행에는
이 시험이 들어가지 않고 이름 제외도 필요 없다. 같은 잡의 관측용 GUI/Xvfb 단계가 실행하지만
`continue-on-error: true`이므로 실패가 잡을 차단하지 않는다. 차단 검사로 승격할 때는
러너의 Xvfb 가용성과 연속 성공 기록을 확인한다. 필요한 연속 횟수 N은 아직 정하지 않았다.

### webview 레이아웃 시험

`crates/tasty-plugin-markdown/src/render/webview_layout_tests.rs` 는 `render_document` 의 HTML 을
화면 밖 WebKitGTK 창에 띄우고 `getBoundingClientRect()` 로 주소창 배치를 잰다. 긴 문서 끝과 앵커
이동 뒤에 주소창 `top` 이 0 인지, 짧은 문서의 `body` 가 뷰포트 높이를 채우는지를 본다. 콜아웃 라벨이 찾기와 선택 문자열에 잡히는지도 실제 찾기 스크립트와 `Selection.toString()` 으로 본다. 생성 CSS
문자열 시험은 선언만 보므로, 문자열 시험이 보지 않는 선언이 배치를 깨는 경우는 이 시험만 실패한다.
`render/bed_focus_webkit_tests.rs` 는 호스트의 바탕 포커스 시트([WebView §바탕 포커스 상태](../design/systems/webview.md#바탕-포커스-상태))를 넣고 뺄 때 문서를 다시 싣지 않고 바탕·표 칸·zebra 계산값이 바뀌는지, 스크롤과 선택이 그대로인지, JS 를 끈 문서의 바탕 픽셀이 바뀌는지, 시트를 먼저 넣고 실은 문서가 처음부터 focus 값인지를 본다.
하네스는 같은 HTML 을 같은 엔진의 기본 설정으로 배치한다. 호스트가 WebView 에 주는 zoom·JS 게이트·콘텐츠
필터는 재현하지 않는다.

- **Linux 전용이다.** 모듈이 `#[cfg(all(test, target_os = "linux"))]` 이고 dev 의존성(webkit2gtk·gtk·
  javascriptcore)도 Linux 대상에만 있다. macOS(WKWebView)·Windows(WebView2)의 배치는 이 시험이 재지 않는다.
- **Linux 에서 플러그인 시험을 빌드하려면 GTK3·WebKitGTK 개발 패키지가 필요하다.** dev 의존성은 feature 와
  무관하므로 `cargo test -p tasty-plugin-markdown`·`cargo test --workspace`·`cargo clippy --all-targets` 가
  `--no-default-features` 조합에서도 그 -sys 크레이트를 빌드한다. 라이브러리가 없으면 레이아웃 시험만 빠지는
  것이 아니라 플러그인 시험 전체가 컴파일 단계에서 실패한다. 설치 패키지는 [빌드 가이드](build.md#linux-시험-빌드의-시스템-라이브러리)에 있다.
- **디스플레이가 필요해 `#[ignore]` 다.** 그래서 `check-headless` 의 gui 유닛 단계(`--lib --bins`)는
  Xvfb 안에서 돌지만 이 시험은 ignored 로만 세고 실행하지 않는다. 실행은 같은 잡의 `cargo test (linux, gui, webview layout — 관측용)`
  단계가 xvfb-run 안에서 이름으로 지목해서 한다. `DISPLAY` 가 없으면 조용히 통과하지 않고 실패한다.
- **파일마다 시험이 하나다.** GTK 는 한 프로세스에서 한 스레드만 초기화하고 libtest 는 `--test-threads=1` 에서도 시험마다 스레드를 따로 만든다(측정). 그래서 두 파일의 시험을 각각 다른 프로세스로 지목한다. 단계 스크립트는 기본 셸의 `-e` 로 돌므로 앞 호출이 실패하면 뒤 호출은 실행되지 않는다.
- 지목 실행은 이름이 어긋나면 `0 passed` 로 통과하므로, 문서 가드
  `the_webview_layout_step_names_the_one_layout_test`(`ci_channel_claims_match_workflows.rs`)가 두 파일 각각의
  `#[ignore]` 시험이 하나뿐인지와, 자동 잡의 호출이 `-p tasty-plugin-markdown … -- --ignored --exact
  render::<모듈>::<그 이름>` 으로 고르는지를 함께 본다. e2e 단계의
  `the_gui_layer_a_display_revives_is_exactly_the_one_named_test` 와 같은 형태다.
- **관측용이다.** e2e 단계와 같이 `continue-on-error: true` 라 실패해도 잡을 차단하지 않으므로, 결과는
  단계 conclusion 으로 확인한다. 차단으로 승격하는 기준(연속 통과 횟수)은 e2e 와 함께 아직 정하지 않았다. X64 러너에서 이 단계의 실행 결과는
  아직 관측 기록이 없다. 러너에 xvfb-run 이 있고 같은 잡의 e2e 단계가 Xvfb 안에서 gui 바이너리를 띄운다는 것만 확인했다.
- 로컬 실행:

```bash
env -u WAYLAND_DISPLAY -u DISPLAY xvfb-run -a --server-args="-screen 0 1920x1080x24" \
  cargo test -p tasty-plugin-markdown --locked --no-fail-fast -- --ignored --exact \
  render::webview_layout_tests::address_bar_stays_on_top_in_webkitgtk
env -u WAYLAND_DISPLAY -u DISPLAY xvfb-run -a --server-args="-screen 0 1920x1080x24" \
  cargo test -p tasty-plugin-markdown --locked --no-fail-fast -- --ignored --exact \
  render::bed_focus_webkit_tests::bed_focus_sheet_switches_state_values_in_place
```

`gui_tests` 에 자동 채널이 없다. 각 시험이 `#[ignore]`이므로 디스플레이만 제공해서는
실행되지 않는다. 수동 실행은 `--ignored`가 필요하며, 전용 디스플레이와 격리된 TASTY_HOME을
사용한다. 검사 전 제품 바이너리가 현재 소스에서 빌드됐는지도 확인한다.

```bash
VERIFY_HOME=$(mktemp -d)
env -u DISPLAY -u WAYLAND_DISPLAY TASTY_HOME="$VERIFY_HOME" \
  xvfb-run -a --server-args="-screen 0 1920x1080x24" \
  cargo test --workspace --locked --test gui_tests -- --ignored --test-threads=1
```

이 스위트의 통과 수에는 환경과 실행 방식에 무관한 단일 값이 없다. 한 인스턴스를 공유하는지,
시험을 프로세스별로 나눴는지, 입력 포커스·팝업·선행 시험 상태를 함께 기록한다.
공유 하네스는 현재 부팅 전 SpawnOnceLatch와 락 poison 복구를 사용한다.
이 사실이 모든 공유 상태 오염이나 키 입력 실패를 해결했다는 뜻은 아니다.

#### 입력 확인과 정리

키 입력을 평가할 때는 `test_notification_panel_{toggle,close_escape,speed}` 세 시험을
먼저 실행한다. 종료코드 0만 보지 말고 정확히 `3 passed`인지 확인한다. 이름 필터가 어긋나
0개를 실행하면 검사한 것이 없다. 세 시험이 실패하면 나머지 키 입력 결과를 제품 단축키의
성공·실패 증거로 쓰지 않고, 입력 도착과 단축키 허용 상태부터 조사한다.

Linux의 WM 없는 Xvfb에서는 직접 창 포커스를 설정해야 한다. enigo는 현재 OS 포커스에
입력하므로 창 존재만으로 입력 도착을 보장할 수 없다. `popups.has_focused()` 등
`keyboard_overlay_open`의 조건도 확인한다. 입력은 도착했지만 팝업이 단축키 처리를 막을 수 있다.
각 조건을 참/거짓으로 기록해 미보고와 false를 구분한다. 시험이 남긴 데이터뿐 아니라
팝업·포커스 상태도 정리하고, 정리용 단축키가 그 상태에 막히지 않는지 확인한다.

시험의 키 조합과 기대 동작은 현재 preset에 맞춘다. 조합이 존재해도 다른 동작에 연결됐으면
유효한 시험이 아니다. 셋업에서 실패한 여러 시험을 독립된 제품 결함으로 세지 않는다.
관측 필드도 실제 입력 경로를 읽어 선택한다. 예를 들어 settings_open_requested는
클릭 경로의 일시 상태이며 키보드의 OpenSettings 이벤트를 직접 증명하지 않는다.

프로세스를 나누면 인스턴스·번들 복사·디스크 비용도 늘어난다. 다른 전량 검사와 겹쳐
부하 실험을 하지 않는다. 공유 상태 문제를 조사할 때는 같은 입력과 환경에서 변경 전후 및
원복을 비교하고, 원하는 시험이 실제로 실행됐는지와 제품 바이너리의 최신성을 기록한다.

GUI 관련 문서 검사는 실행·ignore 플래그·결과 설명을 별도로 대조한다. 워크플로 파서의
회귀는 패키지 전체 시험이 확인하므로 문서 검사 한 타깃의 통과를 파서 전체 검증으로 쓰지 않는다.
시각 품질·IME·DPI처럼 사람이 화면을 읽어 판단하는 절차는
[AI 검증 가이드](../ai-verification/index.md)를 따른다. 준비와 캡처를 자동화하는 것과
화면의 의미를 판정하는 것은 별개다.

### 검사 자체의 비용 회귀

테스트가 통과해도 같은 도출이나 파일 읽기를 반복해 느려질 수 있다.
시간만 보면 러너 부하에 흔들리므로 알려진 캐시·정리 작업은 실행 횟수를 관측한다.
`tests/i18n_key_parity.rs`는 같은 도출을 두 번 호출해 둘째 호출의 새 도출이 0인지 먼저 확인하고,
전체 도출에도 상한을 둔다. 상한을 입력 줄 수에 비례시켜 반복 작업을 허용하지 않는다.
이는 알려진 캐시만 보호하며 모든 테스트의 성능을 검사하는 일반 게이트가 아니다.

이름으로 I/O 함수를 추측하거나 함수 본문만 스캔하면 호출 위치에서 생긴 반복을 놓칠 수 있다.
필요하면 strace로 경로별 파일 열기를 측정하되 라이브러리 로드·자식 프로세스의 파일 열기를
제품 파일 읽기와 구분한다. Linux 측정 결과를 다른 OS의 비용으로 일반화하지 않는다.

공유 인스턴스 재사용과 첫 부팅 실패 뒤 재시도 차단도 별개의 검사다.
`spawn_latch_precedes_the_spawn.rs`는 래치가 spawn보다 앞에 있는지를 확인한다.
`shared_instance_harness`의 실패 주입은 없는 TASTY_E2E_BIN을 사용한 별도 자식 시험에서
spawn 시도 1회와 나머지 호출의 래치 진단을 확인한다. 정상 부팅에서 재사용됐다는 사실만으로
실패 후 재시도도 차단된다고 말할 수 없다. GUI 하네스의 같은 실패 조건은 자동 실행되지 않는다.

웹훅 정리 빈도처럼 구현에 기존 관측점이 있으면 그것을 사용한다. 변이는 원하는 검사가
실패시켰는지 확인한다. 컴파일 오류나 unused lint가 먼저 실패한 결과는 회귀 검출 증거가 아니다.

### 셸 규칙 검사의 범위

사유 없는 allow와 직접 순회 검사는 상한을 넘으면 실패한다. 줄면 통과하며 상한을 고치지 않아도
된다. 상한을 낮출 때는 실제 개선인지 검사 범위 누락인지 먼저 확인한다. 도구 오류를 빈 결과로
취급하지 않으며 예외 경로가 사라진 경우도 실패시킨다. 상한과 현재 검사 범위는
각 스크립트를 따른다. 과거 조사 건수를 별도 정답으로 복제하지 않는다.

### Intent 규율 검사

`check-intent-discipline.sh`는 popup·preset·surface·tab·pane·workspace의 등록된 변경 메서드를
직접 부르는 곳을 찾는다. 대상 메서드와 허용 표지 규칙은 [action-dispatch](../design/flows/action-dispatch.md#intent-discipline-강제)에 있다.
호출 탐지는 `scripts/lib/intent_discipline_scan.py`가 `mask-source`로 주석·문자열을 가린 사본에서 한다.
정규식이 공백과 줄바꿈을 허용해 rustfmt가 `state` / `.popups` / `.open_at_top_of_scope(`로 나눈 체인과
`.delete(` 다음 줄의 `PresetKind`도 잡는다. `intent-exempt` 표지는 원문에서 읽는다.
여러 줄 호출은 수신자 체인의 첫 줄부터 메서드 줄까지와 그 위·아래 한 줄에서 표지를 인정한다.
`[결과사용]`·`[부재 …]` 태그 대조는 셸 부분이 맡는다.

종료코드는 0(위반 0), 1(표지 없는 직접 호출 또는 사유 태그와 코드 불일치), 2(판정기·python 부재, 면제 경로·검사 디렉터리 없음, 탐지 실패)다.
변이 검증으로 다음을 확인했다.

- 표지 없는 한 줄 호출과 여러 줄 체인 호출은 실패하고 메서드 줄을 출력한다.
- 여러 줄 호출에 체인 첫 줄 위, 메서드 줄 위, 메서드 줄, 그 아래 줄 중 한 곳에 표지를 달면 통과한다. 체인 첫 줄보다 두 줄 위의 표지는 인정하지 않는다.
- 한 줄 호출은 같은 줄, 바로 위 줄, 바로 아래 줄의 표지를 인정한다. 두 줄 위의 표지는 인정하지 않는다.
- `#[cfg(test)] mod` 본문, 주석·문자열 안의 같은 체인은 세지 않는다.

한계는 다음과 같다.

- 수신자 타입을 해석하지 않는다. 같은 이름의 다른 타입 메서드는 면제 경로 목록으로 관리한다.
- test 판정은 `*_tests.rs` 파일과 `#[cfg(test)]` 바로 다음의 `mod` 본문만 본다. 다른 cfg 형태나 외부 test 파일 선언은 해석하지 않는다.
- 가변 참조를 다른 이름에 담은 뒤 부르는 경우, 트레이트·매크로를 거치는 경우, 등록되지 않은 메서드는 잡지 못한다.
- `[결과사용]` 대조는 표지 줄이나 다음 줄 하나만 읽는다. 여러 줄 호출에서는 결과를 버리는 문장을 알아보지 못할 수 있다.
- 표지의 사유가 타당한지는 태그 외에는 검증하지 않는다.

### 구조 writer 래칫

`check-core-writer-ratchet.sh`는 호스트의 구조 mutation 텍스트 지표와 지정 실행 entry의 호출 위치를 검사한다.
기준은 `scripts/core-writer-baseline.txt`의 (지표, 파일, 함수)별 개수다. `field`, `terminal`, `subop`,
`appstate`는 src의 기존 이름 기반 지표이며, 수신자 타입을 해석하지 않아 preset draft·불변 계획의
동명 필드도 포함한다. baseline은 이런 값 조립과 실제 실행을 개별 source로 대조한 위치 명부다.
App/journal 디렉터리 전체를 면제하지 않는다.

`boundary`는 src와 crates/tasty-core/src의 `evolve`, `evolve_streams`, `projection::apply`,
`projection::bootstrap::initialize`, mirror push/replace/remove와 display-order 적용 호출을 검사한다.
`scripts/lib/core_writer_scan.py`의 `(파일, 함수, 호출명)`별 허용 횟수는 replay·순수 후보 모델 결정·
publication·원격 표시 projection을 구별한다. 같은 파일의 다른 함수 또는 같은 함수의 추가 호출은
허용되지 않는다. 이 지표의 허용 밖 호출은 0이어야 하며 `--write-baseline`으로 수락할 수 없다.
committed replay의 허용 위치는 `src/runtime/journal.rs`의 `apply_all` 하나다. 손상 범위를 한정하는
복원(`load_recoverable`)도 정상 batch와, 건너뛴 batch의 위치 이동(구조 이벤트를 비운 같은 batch)을
이 함수로 적용하며 별도 evolve 위치를 두지 않는다.

기존 네 지표는 합계 증가나 새로운 위치에서 실패한다. `--write-baseline`은 각 합계가 늘지 않았을
때만 위치 명부를 갱신한다. 이동·삭제 근거 없이 현재 측정값을 기준으로 받아들이면 안 된다.
구조 domain의 private 필드와 읽기 참조는 Rust 타입 경계가 강제한다. 이 텍스트 guard는 그 경계를
대신 증명하거나 모든 canonical mutation을 검출하는 분석기가 아니다.

입력은 test 전용 범위를 비운 뒤 문자열·주석을 마스킹한 사본이다. 명시 test 파일 경로도 제외한다.
함수 본문으로 식별한 호출/대입만 센다. source와 사본 수가 다르거나 명시 경로가 없으면 판정하지 않는다.
`python3 -B -m unittest discover -s scripts/lib -p 'test_*scan.py'`는 View의 직접 mutation,
허용 파일의 다른 함수, 허용 함수의 초과 호출, mirror 허용과 local projection의 분리,
App preset 저장의 함수·메서드 한정 예외를 합성 반례로 확인한다.

한계는 다음과 같다.

- 타입, 별칭, trait dispatch, 매크로 확장을 해석하지 않는다. 지정 이름 밖 API나 함수 인식기가
  본문으로 식별하지 못한 표현은 검사 범위 밖이다.
- canonical crate는 지정 entry 호출만 검사한다. 모든 필드 대입을 감사했다는 뜻이 아니다.
- 기존 이름 지표는 같은 개수의 추가·삭제가 상쇄될 수 있다. 합계 감소만으로 제품 경계 개선을 단정하지 않는다.
- private compiler 경계, 실제 publication barrier 및 resource receipt의 동작은 별도 compile/source/실행 검증 대상이다.

병렬 lane을 합칠 때는 기준 파일을 텍스트로 병합하지 않는다. 기준 파일이 병합에서 충돌하거나,
이 검사 도입 전에 갈라진 lane이 함수 이름을 바꿔 "기준에 없는 함수"로 실패하면 병합된 트리에서
writer가 옮겨졌을 뿐인지 확인한 뒤 `--write-baseline`으로 기준을 다시 만든다. 합계가 늘었으면 이 명령이 거절한다.
다만 `--write-baseline`은 지표 합계만 비교하므로 writer 하나를 지우고 다른 곳에 새 writer 하나를 만든 변경도 받아들인다.
이동인지 새 writer인지는 사람이 검토해야 한다.
새 지표를 추가할 때는 기준 파일 머리글의 `# metrics:` 목록에 없는 지표만 처음 한 번 현재 값으로 기록된다.

<a id="무엇을-돌릴지-고를-때--무엇을-고쳤나-가-아니라-고친-것을-무엇이-보나"></a>
<a id="동결-중에-잴-수-있는-것--낡은-바이너리가-현재-트리를-판정한다"></a>

### 변경을 검사하는 타깃을 찾는다

검증 단위는 패키지·타깃·이름 필터다. 해당 크레이트의 검사만으로 다른 패키지의 가드까지
실행했다고 보고하지 않는다. 작업 주제가 아니라 변경 파일 전체를 입력으로 사용한다.

```bash
git diff --name-only <base>..HEAD
scripts/what-sees-this-change.sh [<base>]
```

파일 이름, 디렉터리, 확장자·shebang으로 전체를 훑는 검사, 문서에 추가한 인용 검사를 각각
확인한다. 첫 둘의 문자열 검색만으로 뒤 둘을 찾을 수 없다. 주석에 이름만 언급한 곳과 실제
파일을 읽는 곳도 구분한다. 발견한 파일을 모두 수정하는 것이 아니라 실행할 검사를 고르는 절차다.

이 스크립트는 위반 판정기가 아닌 후보 탐색 도구다. 변경 파일·리터럴 언급·대응 타깃 수와
주석 근사를 출력한다. 일반 순회 가드, 문서 인용 검사, 공용 하네스 의존성을 모두 찾지는 못한다.
정확한 코드/주석 구분에는 mask-source를 쓰고 나머지 검사는 직접 확인한다.

### 기존 검사 바이너리를 재사용할 수 있는 조건

빌드를 잠시 실행할 수 없어도 저장소 파일을 런타임에 읽는 가드는 기존 바이너리로 확인할 수 있다.
단, 가드 자신과 의존 코드, 빌드 설정, 컴파일에 포함한 입력이 그대로여야 한다.
`include_str!`로 포함한 옛 문서를 읽는 바이너리는 현재 문서를 검사하지 않는다.

Cargo가 기록한 `.d`는 컴파일 입력을 확인하는 자료다. 런타임에 읽는 검사 대상 전체나
호출 그래프가 아니며 변경한 의존 라이브러리·빌드 설정도 따로 확인해야 한다.
상대 경로를 정규화하고 실제 실행할 해시의 산출물을 선택한다. 최신성을 확인할 수 없으면
그 결과를 현재 검사 통과로 보고하지 않는다.

실행 출력과 종료코드는 먼저 온전히 저장하고 필요한 부분을 나중에 읽는다.
`<바이너리> | head`는 SIGPIPE로 검사 자체를 중단할 수 있다.

### 갈래가 여럿일 때 게이트를 어떻게 보고하는가 — **rc 가 이미 답하는 경우가 있다**

상한 검사의 rc 0은 `값 <= 상한`만 뜻한다. 실제 값은 검사 출력에서 읽어 함께 남긴다.
이 성질은 검사 이름으로 추측하지 말고 스크립트의 조건식과 종료 분기에서 확인한다.
여유가 있거나 상한을 바꿨다면 실제 값과 비교 기준도 함께 남긴다.
값을 출력하지 않는 검사는 측정 불가를 0으로 기록하지 않는다.

#### 여러 작업을 합친 뒤 검사한다

저장소 전체 개수를 고정하는 검사는 최종 병합 트리에서 다시 실행한다.
두 작업이 각각 검사 대상과 상한을 1씩 늘리면 Git은 같은 상한 줄을 한 번만 반영할 수 있다.
각 작업의 통과 결과로 병합 뒤 통과를 대신하지 않는다.
늘 때만 실패하는 상한은 줄어든 작업이 상한 줄을 고치지 않으므로 그 줄의 충돌이 적다. 그러나
두 작업이 각자 여유 안에서 늘린 합은 병합 트리에서 상한을 넘을 수 있다. 소스 검사의 개수 기록
방식은 [검사 대상 관리](guard-population.md#늘어나는-수는-상한으로-둔다)에 있다.

발견한 검사 수와 실행한 검사 수를 남기고, 기대한 검사를 하나도 실행하지 않았으면 통과로
보고하지 않는다. 보조 검사기가 없거나 오래되면 먼저 다시 빌드한다.
이 상태에서 상한이나 예외를 늘려 실패를 없애지 않는다. 최종 검사는 push 전에 끝낸다.

이전 트리와의 차이가 필요하면 `scripts/gate-delta.sh <base-rev> [게이트...]`를 쓴다.
이 도구는 base를 임시 워크트리로 꺼내 양쪽에서 검사를 실행하고 값을 뺀다.
결과에는 rc, 실제 값, base 대비 증감, 측정한 tip과 미커밋 변경 유무를 적는다.
`git ls-files`를 사용하는 검사는 미추적 파일을 세지 않으므로 그 상태도 확인한다.

변경 줄의 문자열 개수를 정확한 증감으로 쓰지 않는다. 사유 주석만 지우거나 추가해도
위반 수는 바뀌며, 파일을 검사 범위 안팎으로 옮겨도 값이 바뀐다. 문자열·주석을
검사가 제외하는 경우에는 diff가 오히려 더 많이 셀 수 있다.
여러 작업의 증감을 합산해 병합 결과를 예상하려면 대상과 검사 조건이 같고 변경 영향이
독립적이어야 한다. 같은 파일이나 집합을 함께 바꿨다면 단순 합산은 맞지 않을 수 있다.
이 도구는 두 끝점만 비교하므로 중간에 추가했다가 제거한 변경까지 설명하지는 않는다.

양쪽 트리는 같은 방식으로 측정해야 한다. `gate-delta.sh`는 검사 거부 여부와 원문
폴백 여부를 확인한다. 검사기 소스가 다르거나 한쪽만 폴백하면 측정 불가로 처리한다.
검사기를 환경변수로 지정해도 `resolve_judge`의 내용 지문 검증을 거친다.
검사기가 각 트리에 맞는다는 확인이 다른 모든 오류까지 배제하는 것은 아니다.

기본 상한 검사는 `[<이름>] 판정 불가 —`와 rc 2로 거부한다. 도구는 이 접두사를
사용하므로 일반 설명에 나온 '판정 불가'라는 말만으로 거부했다고 판단하지 않는다.
값은 `… : <수>건 (상한 <수>)` 형식만 읽는다. 다른 출력 형식이나 검사기 주입을 지원하지
않는 스크립트는 이 도구로 측정할 수 없다.

실패하면 양쪽 출력의 마지막 30줄과 잘림 여부를 남긴다. base 워크트리를 만들지 못하면
Git 오류를 출력하고 rc 2로 끝난다. `gate-delta.sh`는 독립 검사가 아닌 측정 도구이므로
`scripts/check-*.sh` 자동 검색에 포함하지 않는다.

<a id="사유-열의-진위는-어디까지-기계가-보는가"></a>

### 예외 사유에서 검사할 수 있는 내용

사유가 적혀 있다는 것과 타당하다는 것은 다르다. 파일·명령·수치처럼 확인 가능한 부분을
사유에 포함하고 실제로 대조한다. Intent 예외의 `[결과사용]`은 호출 결과를 버리지 않는지,
`[부재 <파일> <정규식>]`은 아직 없다고 한 구현이 생겼는지를 확인한다.
모르는 태그를 조용히 무시하지 않는다.

allow 검사에는 cfg_attr 안의 allow와 한글 `이유:` 표지도 포함한다. 여러 줄 사유를 마지막
한 줄만 읽어 근거 없음으로 판정하지 않는다. 함수 역할이나 호출 관계에 관한 사유는
문자열 검사만으로 증명하지 못하므로 직접 읽는다. 한 OS에서 효과가 없다는 이유만으로
다른 OS 조건의 허용을 지우지 않는다.

## 로컬 훅이 앞당겨 주는 것

훅은 **옵트인**이다(`git config core.hooksPath .githooks` 1회) — 설치하지 않아도
커밋·push 는 된다. 그래서 훅은 "게이트" 가 아니라 CI 게이트의 **빠른 피드백**으로
읽는다. 상세는 [git-hooks](git-hooks.md).

| 훅 | 검사 | CI 에도 있는가 |
|---|---|---|
| pre-commit | `cargo fmt --check` | ✅ `format-check.yml` |
| pre-commit | mod/use 선언 순서 · `egui::Window` 직접 사용 · `println!`/`dbg!` | ❌ 훅에만 있다 |
| pre-commit | plugin 산출물이 바뀌었는데 매니페스트 `version` 이 그대로 (P.1) | ✅ `plugin-version-check.yml` — **같은 스크립트를 부른다**. 훅은 index 를 `main` 과의 merge-base 와 비교하고(amend·rebase 에 안 흔들리게), CI 는 밀어넣은 범위의 두 끝점을 비교한다. `tasty.pluginBump=deferred` lane 작업 트리의 훅은 검사하지 않는다. `main`에 반영된 커밋마다의 증가는 병합 단계(`plugin-bump-fixup.sh` 와 그 뒤 커밋별 `--range c^ c` 검사)가 보장하고, B.9·CI 는 범위 끝점 두 개만 비교한다([git-hooks](git-hooks.md#lane-작업-트리의-p1-보류)) |
| pre-commit | 주석 없는 `let _ =` (C.6) | 부분 — 전수판 `crates/tasty-doc-guards/tests/let_underscore_documented.rs` 가 훅의 상위집합이고, 그 전수판을 `doc-guards.yml`(경로 필터 없음) · `check-windows` · `check-headless` 가 자동 실행한다. 자동 잡의 clippy는 `let_underscore_must_use`(warn)로 `let _ = <Result>`를 알린다. 주석의 사유는 검사하지 않아 사유가 있는 코드에도 경고하며, `-D warnings`가 없어 이 경고만으로 빌드를 막지는 않는다([error-handling](error-handling.md)) |
| pre-commit | 로컬 티켓 인용(T.1) | ✅ doc-guards.yml과 pre-push B.7도 no_todo_file_citation을 실행한다. 이 검사는 staged diff가 아닌 전체 작업 트리를 읽는다. |
| pre-push | 플러그인 버전 `--range <원격 tip> <로컬 tip>`(B.9) | ✅ plugin-version-check.yml과 같은 스크립트다. Git이 전달한 두 tip을 사용하며 비교 범위를 알 수 없으면 실패한다. staged 변경을 보는 P.1과 구분한다. |
| pre-push | `cargo clippy --workspace --all-targets -- -D clippy::correctness` | 부분 — Windows 잡의 clippy 는 `--locked` 를 쓰고 correctness deny 를 걸지 않는다. 그리고 이 훅은 Linux 트리의 feature 집합(`tracing/log` 가 켜진 쪽)으로 lint 를 센다 — Windows 잡은 그 갈림을 못 본다([크레이트를 지목한 clippy](#크레이트를-지목한-clippy-는-push-와-다른-feature-집합을-잰다)) |
| pre-push | `cargo check --no-default-features` | ✅ `crossplatform-check.yml` |
| pre-push | `cargo test -p tasty-doc-guards` | ✅ `doc-guards.yml` — **같은 크레이트를 부른다**. 훅은 push 하는 머신에서만 돌아 worker 머신엔 이 채널이 없다 |
| pre-push | `cargo check --workspace --release --locked` | ✅ crossplatform-check.yml의 check-release와 같은 명령이다. debug 검사와 별도로 실행하며 bin 하나로 좁히지 않는다. |
| pre-push | `cargo test --lib --locked --no-fail-fast -- source_guards`(B.10) | ✅ `crossplatform-check.yml`·`crossplatform-check-macos.yml`의 `cargo test --workspace --lib --bins`가 이 모듈을 포함한다. 훅은 Linux·Windows·macOS 중 push하는 머신에서만 돌고 소스 가드만 실행한다. |

pre-push의 기본 feature 개발 빌드·테스트 코드 컴파일 검사는 위 Clippy 명령에 통합한다.
동일한 타깃의 일반 `cargo check`를 다시 실행하지 않으며, release와 headless 검사는 유지한다.

<a id="창의-양-끝원하는-두-커밋-훅의-면제-경로를-그대로-적용한다"></a>
<a id="훅이-어느-os-에서-도는가--위-표에-없는-축"></a>

훅은 설치된 개발 머신에서 실행된다. Linux의 `--all-targets`는 lib·bin·test 등 타깃 종류를
넓힐 뿐 Windows 코드를 검사하는 옵션이 아니다. 소스 문자열 검사도 체크아웃의 CRLF에
영향받을 수 있다. 따라서 훅과 CI의 명령이 같아도 플랫폼까지 같은 검증은 아니다.

필요하면 Windows GNU 크로스 체크로 로컬 피드백을 앞당긴다.

```bash
cargo check --workspace --all-targets --target x86_64-pc-windows-gnu --locked
```

타깃 툴체인과 C 의존성 준비가 필요하며 host 기준 build.rs 분기는 Windows 실행으로 확인한다.
실제 Windows CI는 리소스·링커까지 확인하므로 크로스 체크가 이를 대신하지 않는다.
개발 머신의 cold/warm 빌드 시간을 다른 러너의 비용으로 쓰지 않는다.

훅 전용 검사에는 mod/use 순서, egui::Window 직접 사용, println!/dbg! 제한이 있다.
미설치 환경에서는 실행되지 않는다. let _ = 사유는 별도의 전수 문서 가드도 검사한다.

## 이 문서와 레포가 어긋나지 않게 하는 것

문서의 자동 실행 주장은 실제 워크플로의 트리거·명령·빌드 조합과 대조한다.

`crates/tasty-doc-guards/tests/ci_channel_claims_match_workflows.rs` 가 그 형태를 막는다(이 가드 자신도 통합
테스트라 `doc-guards.yml` · `check-windows` · `check-headless` 세 잡이 돌린다 — 위 규칙이 자기에게도 그대로
적용된다). 문서를 문서로 검사하지 않고 **워크플로에서 자동 트리거를 가진 잡을 읽는다.**
네 축이 있다.

- **명령을 인용한 형태** — **기본 조합**의 자동 잡이 전체 스위트를 돌리는지 보고,
  돌리지 않으면 그것을 강제 장치로 서술한 자리를 전부 짚는다. 문서가 인용하는
  `cargo test --workspace` 는 기본 조합의 명령이라, 헤드리스 잡이 전체 스위트를 돌리는
  것과 섞어 보면 이 축이 통째로 잠잠해진다.
- **명령을 적지 않는 형태** — 자동 잡이 `--test` 로 **이름을 지목한** 통합 테스트 목록을
  워크플로에서 읽어, 그 밖의 `tests/*.rs` 를 집행 장치로 부르는 서술을 짚는다. 좁히지
  않은 자동 잡이 하나라도 있으면 이 축은 스스로 잠잠해진다.
- **반대 방향** — 자동 잡이 lib 유닛 테스트를 돌리는 동안, `src/` 안의 유닛 테스트를 두고
  부재를 적은 서술을 짚는다(사실보다 약하다). 이 전제도 상수가 아니라 워크플로에서 읽는다.
- **조합** — 통합 테스트를 지목하면서 자동 채널의 부재를 적었는데 그 테스트가 실제로는
  도는 자리를 짚는다. 판정 단위는 **그 테스트가 자동으로 도는 조합의 수**다: 0 이면 부재
  서술이 참, 1 이면 어느 조합인지 함께 적어야 참, 2 면 어떻게 적어도 거짓이다. 조합별로
  빌드되는지(`required-features`), 그 호출이 통합 타깃을 만드는지, `--skip` 이 그 타깃을
  통째로 걷어내는지를 함께 본다.

부재를 함께 적은 문장(`수동 전용` 등)은 정당한 서술로 통과시키므로 등록 절차가 없다.
조합이 하나뿐인 채널은 그 조합을 함께 적어야 통과한다(`check-headless 잡에서만` 등).
목록을 가드 안에 복사해 두지 않고 워크플로에서 런타임에 읽으므로, 전체 스위트가 자동
채널에 올라가거나 `--test` 열거·`--skip` 이 바뀌는 날 이 가드는 스스로 따라간다 — 그때
문서를 손으로 다시 훑지 않아도 된다.

## 파생 문서는 채널을 다시 쓰지 않는다

다른 문서에 같은 CI 실행 설명을 복제하면 설정 변경 때 함께 갱신해야 한다.

그래서 규칙은 **다시 서술하지 말고 여기를 링크한다** 이다. 서술이 꼭 필요하면 그 문장이
**실행/컴파일**과 **축 단위 실효성** 둘 다에서 이 문서와 같은 말을 하는지 확인한다.

## 관련

- [git-hooks](git-hooks.md) — 훅 각 검사의 내용과 설치
- [clippy-policy](clippy-policy.md) · [complexity-gate](complexity-gate.md) — lint 정책
- [release 러너](release.md#러너) — self-hosted 러너 구성

## 셸 검사와 커밋 범위

셸 자산은 `check-shell-assets.sh`로 ShellCheck warning 이상을 검사하며 위반을 남기지 않는다.
파일 확장자와 shebang을 함께 사용해 source 라이브러리와 확장자 없는 훅도 포함한다.
pre-commit은 staged 경로를, CI는 추적 파일 전체를 검사한다.
검사기가 없으면 rc 2이며 `install-shellcheck.sh`와 `dev-setup.sh`로 준비한다.
info/style는 검사 범위 밖이다. 버전을 올릴 때는 새 진단을 먼저 확인한다.
커밋별 검증 범위는 위 'push 범위 안쪽의 커밋'을 따른다.

`python3 scripts/lib/test_core_writer_scan.py`는 선택 map/remap과 실제 구조 쓰기를 구별하는 변이 시험이다. Navigation 파일의 `engine.workspaces.clear()`와 App adapter의 `remove_tab`/`take_tab` 호출을 검출한다. 타입을 해석하지 않는 scanner의 한계는 그대로이며 파일 전체 제외를 추가하지 않는다.

활성 상태 읽기 명부는 구 active/focused 식별자와 함께 App의 `presentation()` 및 `.navigation` 접근도 수집한다. 응답 보고·명시 ID 내부의 CWD 상속·사용자 continuation·저장/원격 snapshot을 구분하며, 문자열 집계만으로 대상 선택의 적법성을 판단하지는 않는다.

### Terminal viewport 소유 경계

`terminal_viewport_ownership`은 TerminalState·Terminal·Scrollback·Pty·PtyState·
CommandContext의 struct 본문에서 사용자 offset과 viewport 타입을 금지하고 MainViewState의
원본을 확인한다. 재도입 fixture 변이도 검사한다. 이름과 타입 토큰을 읽는 검사이므로
별칭·간접 참조를 통한 소유까지 증명하지는 않는다. parser 내용 cut의 일관성과 출력·trim·
resize 보정은 tasty-terminal의 viewport 행동 시험으로 확인한다.
