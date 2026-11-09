# 유닛 테스트 격리 — 사용자 환경을 읽지 않는다

유닛 테스트의 결과는 **실행하는 사람의 로컬 상태에 좌우되면 안 된다.** 사용자 홈의
`config.toml` 값 하나로 무관한 테스트가 깨지면, "내 변경이 깬 것인가" 판정이 매번 수동
대조가 되고 CI 러너와 개발자 머신의 결과가 갈린다. 결정 근거는
이 문서.

e2e 테스트의 격리 단위(프로세스 vs workspace)는 다른 축이다 — [e2e-tests](e2e-tests.md) ·
이 문서.

## 1. 설정: 테스트 생성자는 `Settings::default()` 를 쓴다

`EngineSession` 생성자는 설정을 **어디서 얻는지** 로 갈린다.

| 생성자 | 설정 출처 | 용도 |
|---|---|---|
| `EngineSession::new(cols, rows, waker)` | `Settings::default()` | 테스트 / non-host 진입점 |
| `EngineSession::new_with_ids(...)` | `Settings::load()` (`$TASTY_HOME/config.toml`) | host 부팅 경로 |

둘 다 내부 `new_with_ids_and_settings(..., settings)` 로 합류한다 — 설정 주입 지점이
여기 하나뿐이라, 새 진입점을 만들 때도 "파일을 읽을 것인가" 를 명시적으로 고르게 된다.

**테스트에서 특정 설정이 필요하면 엔진을 만든 뒤 그 필드만 바꾼다** — 사용자 홈에 그 값이
있기를 기대하지 않는다:

```rust
let (mut state, mut session) = test_state();
let mut engine = session.borrow_mut();
engine.settings.general.workspace_categories_enabled = true;
```

파일 로드 동작 자체(파싱·마이그레이션·폴백)의 검증은 `Settings` 쪽 테스트가 담당하며,
그쪽은 임시 디렉토리를 명시적으로 가리킨다.

## 2. 환경변수: 반드시 RAII 가드로 복원한다

`std::env::set_var` / `remove_var` 는 프로세스 전역이다. 테스트가 직접 호출하고 마지막 줄에서
정리하는 방식은 두 곳에서 샌다.

- **패닉 경로** — 단언이 깨지면 정리 줄에 도달하지 못한다.
- **`remove_var` 로 "정리"** — 실행 환경에 원래 값이 있었으면 그 값을 잃는다. `TASTY_HOME` ·
  `TASTY_SURFACE_ID` · `TASTY_AGENT_ID` 는 tasty 터미널 안에서 실제로 설정돼 있다.

어느 쪽이든 같은 프로세스의 **뒤따르는 테스트**가 오염된 환경을 물려받아, 변경과 무관한
실패가 생긴다. 그래서 env 조작은 Drop 에서 원값을 되돌리는 가드로만 한다.

호스트 crate(`src/`)는 `crate::test_support` 의 두 가드를 쓴다.

| 가드 | 역할 |
|---|---|
| `TastyHomeGuard` | 공유 락(`TASTY_HOME_ENV_LOCK`) 획득 + `TASTY_HOME` 을 임시 디렉토리로 교체 + Drop 에서 원값 복원. `path()` 로 그 임시 루트를 얻는다 |
| `EnvVarGuard` | 임의의 키 하나를 교체하고 Drop 에서 원값 복원. 직렬화 락은 호출부 책임 |

```rust
let home = crate::test_support::TastyHomeGuard::new();
let path = next_screenshot_path().expect("dir creation must succeed");
assert!(path.starts_with(home.path().join("screenshots")));
```

**홈을 임시 디렉토리로 바꾸는 것이 목적이면 `TastyHomeGuard` 가 아니라 `IsolatedHome` 이다.**
같은 모듈의 세 번째 가드이고 env 를 **안 만진다** — 그래서 이 절(§2)의 대상이 아니고 락도
없다. 둘 다 있는 이유는 검증 대상이 다르기 때문이다: `TASTY_HOME` env **해석 자체**가 검증
대상이면 `TastyHomeGuard`, 그저 사용자 홈을 안 읽으면 되는 것이면 `IsolatedHome` 이다.
후자가 §7 형태 D 의 표준 처방이다.

다른 crate 는 `test_support` 에 접근할 수 없으므로 같은 형태의 가드를 crate 안에 둔다.

| crate | 가드 | 다루는 키 | 락 |
|---|---|---|---|
| `tasty-host-plugin` | `test_support::HomeEnvGuard` (`bundle_sig` · `manager::pump` 공용) | `HOME` + `TASTY_HOME` | `HOME_ENV_LOCK` |
| `tasty-telemetry` | `agent_id::tests::AgentIdEnvGuard` | `TASTY_AGENT_ID` | `ENV_LOCK` |
| `tasty-cli` | `request::tests::SurfaceIdEnvGuard` | `TASTY_SURFACE_ID` | 단일 `#[test]` 안에 모아 대체 |
| `tasty-cli` | `cwd_resolve::tests::CwdGuard` | 프로세스 cwd | `CWD_LOCK` |
| `tasty-settings` | `general::tests::RelativeHomeGuard` | `TASTY_HOME` | `SERIAL` |

`tasty-settings` 의 `TmpHome` 은 이 표에 없다 — 그것은 **env 를 안 만진다**(홈 경로를
`_in(Some(home.path()))` 로 주입한다). env 를 만지는 것은 상대 `TASTY_HOME` 해석 자체가
검증 대상인 `RelativeHomeGuard` 쪽뿐이다.

**가드가 락을 쥐는가는 가드마다 다르고, 그 차이가 값이다.** `AgentIdEnvGuard` ·
`RelativeHomeGuard` · `CwdGuard` 는 **생성자가 자기 락을 직접 쥔다** — 호출부가 잊어도
직렬화가 안 깨진다. `EnvVarGuard`(본체)는 안 쥔다(락은 호출부 책임)이고,
`SurfaceIdEnvGuard` 는 아예 락이 없다(그 키를 만지는 시나리오를 단일 `#[test]` 안에
모아 구조적으로 직렬이다). **락을 안 쥐는 두 갈래는 그 조건을 호출 자리 주석에 밝혀야
한다** — `no_unserialized_env_mutation` 이 함수 범위에서 직렬화 증거를 찾는데, 가드
안쪽의 `set`/`drop` 에는 `lock()` 호출이 없기 때문이다.

<a id="같은-키에-락이-셋인데-그것이-옳다--배제-단위는-레포가-아니라-테스트-바이너리다"></a>
<a id="이-추론을-지탱하는-것은-pub-여부가-아니다"></a>
<a id="재는-법--경계를-먼저-물어라"></a>

### 환경변수 락의 범위는 테스트 프로세스다

환경변수는 프로세스별 상태다. 같은 테스트 바이너리에서 같은 키를 읽거나 바꾸는 시험은
하나의 락을 공유한다. 다른 바이너리의 락까지 합칠 필요는 없다.
현재 tasty-test-support의 TASTY_HOME_ENV_LOCK, tasty-host-plugin의 HOME_ENV_LOCK,
tasty-settings 테스트의 SERIAL은 각자의 테스트 프로세스에서 사용한다.

경계의 근거는 가시성만이 아니라 cfg(test) 선언이다. 의존성으로 컴파일할 때 테스트 전용
모듈은 포함되지 않는다. 해당 선언을 제품 코드로 옮기면 같은 바이너리에 함께 들어오는지와
락의 공유 범위를 다시 확인한다. tasty-host-plugin의 test-only 선언 검사와 루트의
락 우회 검사도 서로 다른 조건을 보호한다.

TASTY_AGENT_ID를 보호하는 ENV_LOCK이나 cwd를 보호하는 CWD_LOCK은 자원이 다르다.
락 개수만 보고 합치지 말고 보호하는 상태와 프로세스 경계를 확인한다.

### 시험 바이너리를 자식으로 다시 띄울 때는 출력을 캡처한다

Linux의 winit `EventLoop`는 프로세스마다 한 번만 만들 수 있다. `--test-threads=1`로
직렬화해도 두 번째 생성의 `RecreationAttempt`는 해결되지 않는다. 독립된 GUI 시험 본문은
자기 시험 바이너리의 정확한 시험 이름으로 새 프로세스에서 실행한다. 부모는 자식의 성공
종료와 모든 본문 단언 뒤에 기록한 완료 표지를 함께 확인해, 이름 필터 오류로 0개만 실행한
경우를 통과로 세지 않는다. 다른 시험과 EventLoop를 전역으로 공유하거나 시험을 제외하지 않는다.

환경변수를 바꾸지 않으려고 시험이 자기 바이너리(`std::env::current_exe()`)를 `--exact <시험>` 필터와
다른 환경으로 다시 띄우는 경우가 있다. 자식도 시험 하네스라 `running 1 test`, 시험 줄, `test result:`
줄을 찍는다. `status()`·`spawn()` 처럼 stdout·stderr 를 물려받으면 이 줄이 부모 시험의 출력에 섞인다.
결과 줄이 실행 타깃보다 많아지고, 병렬로 찍히던 부모의 시험 줄과 한 줄에 엉켜 `okok` 같은 줄이 생긴다.
결과 줄을 읽는 판정([자체 검증](self-verification.md)의 "검사 결과를 읽는다")이 이것을 오판한다.

- 끝까지 기다리는 자식은 `output()` 으로 캡처하고, 실패할 때만 상태·stdout·stderr 를 실패 메시지에 싣는다.
- 폴링하며 기다리거나 도중에 종료시키는 자식은 파이프가 차서 멈추지 않게 시험의 임시 경로에 있는 파일로
  stdout·stderr 를 보내고, 실패 메시지에 그 내용을 싣는다(`crates/tasty-event-store/tests/crash.rs`,
  `crates/tasty-reaper/tests/bound_spawn.rs`).
- 출력이 필요 없으면 `Stdio::null()` 로 버린다.
- 자식에 `ulimit -f` 같은 파일 크기 상한을 걸면 그 상한은 물려받은 stdout·stderr 에도 적용된다. 부모 출력이
  이미 상한을 넘은 로그 파일이면 자식의 첫 출력부터 EFBIG 로 실패하고 오류 본문도 남지 않는다. 이 경우 출력은
  파이프(`output()`)로 받고 임시 경로의 파일로 보내지 않는다. 그 파일에도 같은 상한이 걸린다
  (`crates/tasty-memory/src/batch/batch_tests.rs` 의 `a_commit_that_hits_a_real_write_error_publishes_nothing`).

실패 경로에서는 자식의 `test result: FAILED` 줄이 부모의 실패 메시지 안에 그대로 나온다. 이때 부모 실행도
실패이므로 판정은 바뀌지 않는다.

### winit EventLoop는 시험 스레드 직렬화로 재생성할 수 없다

Linux의 winit EventLoop는 프로세스에서 한 번만 만들 수 있다. 다른 실제 App 시험이
이미 EventLoop를 만들었다면 `--test-threads=1`도 RecreationAttempt를 막지 못한다.
Export 종료 수집 시험은 같은 시험 바이너리를 `current_exe`와 정확한 `--exact` 이름으로
다시 실행하고, 자식 분기에서만 EventLoop와 실제 App을 만든다. 따라서 부모 시험은
다른 GUI App 시험과 함께 실행되어도 EventLoop를 소비하지 않는다.

자식 표지는 `Command::env`로 그 자식에만 전달한다. 자식 분기는 다시 자식을 만들지 않으며,
final-zero와 timeout 두 실제 App 시나리오의 단언이 모두 끝난 뒤 완료 파일을 쓴다.
부모는 종료 상태와 완료 파일을 함께 확인하므로 잘못된 필터로 시험 0개가 실행돼도 실패한다.
`output()`이 출력을 캡처하고 자식을 기다려 회수한다. DISPLAY와 격리된 홈 환경은
검증 실행기가 제공하며, 자식의 시험 결과 줄은 부모의 통과 건수에 중복 합산하지 않는다.

### 락은 **키 단위**로 하나 — 모듈마다 따로 두지 않는다

같은 crate 안에서 같은 키(또는 서로를 덮는 키 쌍)를 건드리는 테스트가 **서로 다른 락**을
잡으면, 락이 있어도 격리가 되지 않는다. 두 테스트가 같은 테스트 바이너리에서 병렬로 돌며
한쪽이 세운 `TASTY_HOME` 을 다른 쪽이 지우거나 덮어쓰고, 그 순간 앞 테스트의 파일 쓰기가
**사용자의 실제 `~/.tasty{-debug}`** 로 간다. 증상은 스케줄링에 따라 나타났다 사라진다.

`HOME` 과 `TASTY_HOME` 은 `tasty_home()` 이 함께 보는 **한 쌍**이므로 같은 락으로 묶는다.
`tasty-host-plugin` 이 `HomeEnvGuard` 하나로 통일한 이유다 — 그 crate 는 두 키를
`test_support` 한 곳에서만 만지며, 그 사실을
`test_support::tests::home_env_is_only_touched_through_this_module` 가 소스 스캔으로 고정한다
(다른 곳에 `env::set_var("HOME"…)` / `…("TASTY_HOME"…)` 가 생기면 그 테스트가 파일·행을
지목하며 실패한다).

## 3. `TASTY_HOME` 은 `HOME` 을 이긴다

`tasty_utils::path::tasty_home()` 의 우선순위는 **`TASTY_HOME` → `$HOME/.tasty{-debug}`** 다
(`crates/tasty-utils/src/path.rs`). 따라서 `HOME` 만 임시 디렉토리로 바꾸는 격리는 실행
환경에 `TASTY_HOME` 이 잡혀 있으면 통째로 무시된다 — 테스트가 임시 홈이 아니라 그 루트를
읽는다. `HOME` 파생 경로를 강제하려는 가드는 `TASTY_HOME` 도 함께 비워야 한다
(`tasty-host-plugin` 의 `HomeEnvGuard::derived_from_home()` 이 그렇게 한다. 반대로 임시 루트를
직접 지정하면 되는 경우는 `HomeEnvGuard::tasty_home()`).

## 4. 파일시스템 픽스처는 테스트가 직접 만든다

경로 해석처럼 `exists()` 로 실존을 검사하는 함수의 테스트는 **git 에 있는 경로**(`Cargo.toml`,
`src/adapters/ui` 등 `CARGO_MANIFEST_DIR` 기준)나 **테스트가 임시 디렉토리에 스스로 만든 경로**만
입력으로 쓴다. gitignored 로컬 작업 폴더나 사용자 홈처럼 *이 머신에만 있는* 경로의 실존에
기대면 clone 직후·CI 러너에서 결과가 달라진다 — 로컬 상태 의존이라는 점에서 설정·env 와 같은
축이다. 워크스페이스 dev-dependency 인 `tempfile` 로 만들고 `TempDir` 의 Drop 에 정리를 맡긴다.

```rust
let tmp = tempfile::tempdir().expect("tempdir");
std::fs::create_dir(tmp.path().join("notes")).expect("fixture dir");
let result = longest_existing_selection_path("notes/에", Some(tmp.path()), false);
assert_eq!(result, Some(tmp.path().join("notes")));
```

이 규칙은 `crates/tasty-doc-guards/tests/no_todo_file_citation.rs` 와도 맞물린다 — 로컬 작업 폴더를 언급하면 하위
경로가 무엇이든, 아예 없든 그 테스트가 잡으므로(P6) 픽스처 때문에 allowlist 에 예외를 두지
않는다. 금지 범위와 범위 밖 항목은
[Tasty 문서 모델 (Documentation Model)](../documentation-model.md) 가 정본이다.

의존이 없는 작은 fixture에서 직접 이름을 짓는 경우에는 PID로 프로세스를 가르고,
함수 호출 사이에 유지되는 `static` 단조 counter로 같은 프로세스의 재호출을 가른다.
Codex 셸 실행 시험·유효 폰트 시험·파일 선택기 테스트 helper는 이 두 축을 함께 쓴다. 셸 시험의 production
prompt cleanup은 처음 받은 temp root를 재사용해 생성과 회수의 경로 규칙을 유지한다.


이미지 worker 시험(`src/app/image_jobs/tests.rs`)의 완료 통지는 `mpsc::Sender<()>`를
깨우기 콜백에 직접 캡처한다. Sender는 Send + Sync이며 `ImageWorker::new`의 콜백도
그 두 경계를 요구한다. 송신 전용 값에 별도 Mutex를 두지 않아 락 poison으로 완료 통지를
건너뛰는 경로가 없다. 받는 쪽은 기존 `recv_timeout`으로 실제 통지를 확인한다.

### 공용 writer의 작업별 계측 기준값

여러 소비자가 같은 writer를 쓰면 초기 읽기도 누계에 포함된다. 원격 프로필·패스키 IPC 시험은
child registry의 기존 bootstrap 완료 API로 초기 receipt를 수집하고, 같은 큐에 넣은 flush의
성공 결과를 한 번 받아 앞선 작업이 끝났음을 확인한다. pending·held 항목이 모두 0인 상태에서
읽은 기준값과 원격 요청 완료 뒤의 차이를 비교한다. 논리 성공 2회·replace 3회·delete 1회와
unclassified 증가 0을 그대로 검사하고, 종료 drain 뒤 원 호출자의 중복 응답도 검사한다.
고정 sleep이나 조용한 폴링 구간을 완료로 간주하거나, 초기 작업 수를 상수로 빼지 않는다.
계측 조회는 receipt를 소비하지 않으며, flush 결과는 준비 여부를 검사한 뒤 다시 꺼내지 않는다.

Webhook port 명령 시험도 같은 초기화·기준값 경계를 사용한다. 두 포트 명령의 교체·논리 성공
증가량은 각각 2이며 unclassified 증가는 0이어야 한다. 최신 외부 섹션과 원 호출자 응답을
확인한 뒤, 종료 drain 뒤에도 응답이 중복되지 않는지 검사한다.

### 부팅 소유권 인계와 descriptor 명부

`memory_store_has_one_owner`는 메모리 저장소를 writer로 넘기기 전의 부팅 인계와 실행 중 공유를
구분한다. 추출된 headless 부팅 모듈에서는 `boot_memory`의 정확한 반환 시그니처만 인계로
인정한다. 같은 파일에 새 공유 필드를 붙이거나 함수 이름·위치를 바꾼 입력은 실제 스캐너로
검출한다. writer 연결부의 `Arc::try_unwrap` 단독 소유 확인은 그대로 유지한다.

Attach descriptor의 capability 시험은 실제 광고 배열 전체를 순서대로 비교한다. 현재 공통
descriptor의 HTML entry·resource 항목도 포함하며 GUI와 headless 시험에서 같은 계약을 확인한다.
부분집합 비교로 누락·추가 항목을 숨기지 않는다.

### 자식 CLI의 실제 argv를 확인하는 시험

Codex의 `session/spawn_runtime_tests.rs`는 TUI 명령의 인자를 Python mock에 전달하고,
개별 실행 검증 helper에서 새 세션의 옵션과 resume 설정을 대조한다.
Claude의 `session_options/tests.rs`는 임시 CLI를 실행하는 helper와 종료 코드 42를
대조하는 helper를 사용한다. 호출 시험에서 세 자식의 argv, 옵션 생략, 프로필 원문과
부모·형제 메타데이터를 대조한다. 이 시험들은 실제 provider의 옵션 수용 여부를 검증하지 않는다.

## 5. 확인 방법

사용자 실제 홈으로 테스트를 실행해 격리를 확인하지 않는다. 테스트가 그 홈에 쓴 파일이
다음 실행의 기준값을 바꿀 수 있다. 빈 임시 TASTY_HOME에서 쓰기와 읽기를 각각 확인한다.

```bash
VERIFY_HOME=$(mktemp -d)
TASTY_HOME="$VERIFY_HOME" cargo test --workspace --locked
find "$VERIFY_HOME" -mindepth 1
```

격리된 엔진이 자기 임시 홈을 사용한다면 위 공통 홈에 잔여물이 없어야 한다.
잔여물이 생기면 작성 경로를 찾는다. 그 파일을 별도 임시 홈에 넣고 관련 시험을 단독 실행해
빈 홈에서의 결과와 비교한다. 전체 실행에서는 다른 시험이 같은 파일을 덮어쓸 수 있으므로
전체 통과만으로 사용자 상태를 읽지 않는다고 판단하지 않는다.

순서·병렬도·부하에 따라 실패가 달라지면 홈 잔여물, 실패 메시지, 시간 대조군을 함께 본다.
잔여물이 없다는 사실은 파일 쓰기 누출이 관측되지 않았다는 뜻이며 모든 격리 문제가 없거나
러너만 원인이라는 증명은 아니다. 병렬 부하 실험은 다른 검증과 조율하고 자기 인스턴스로만 한다.

격리 주입을 뺀 변이는 별도의 임시 홈에서 수행한다. 정상 통과 여부뿐 아니라 생성한 파일을
대조한다. 격리가 풀려도 테스트 자체는 통과할 수 있다. 원복 뒤에는 격리와 결과를 다시 확인한다.

### 워크트리별 빌드 결과를 분리한다

`CARGO_MANIFEST_DIR`처럼 컴파일 시점의 경로로 저장소 파일을 읽는 검사는 워크트리마다
독립된 `CARGO_TARGET_DIR`를 사용한다. 실행 디렉터리나 Cargo의 `Fresh` 표시만으로
현재 워크트리를 읽는다고 판단하지 않는다. 검사가 실제로 읽는 저장소 루트를 확인한다.
공용 `flock`은 동시 실행만 막으며, 다른 워크트리의 빌드 결과가 재사용되는 문제를 막지 않는다.

## 6. feature 별 테스트 게이팅

본 바이너리는 `gui` feature 로 갈린다(`--no-default-features` = headless). **gui 전용 타입·모듈을
단정하는 테스트 모듈에는 `#[cfg(test)]` 가 아니라 `#[cfg(all(test, feature = "gui"))]` 를 건다.**
`#[cfg(test)]` 만 걸면 headless 테스트 **바이너리 자체가 컴파일되지 않는다** — 프로덕션 코드는
멀쩡히 `cargo check --no-default-features` 를 통과하는데도 그 feature 조합의 테스트가 한 줄도
실행되지 않는 사각이 생긴다. 개별 테스트 함수만 gui 전용이면 함수에 `#[cfg(feature = "gui")]` 를
건다. GUI 시험에서만 호출하는 helper도 같은 feature 조건으로 제한한다. `test`만 붙여
headless에 사용하지 않는 helper를 남기면 `dead_code` 검사 때문에 시험 바이너리의 컴파일이
실패할 수 있다. 이 경우 lint를 허용하지 않고 호출자와 helper의 cfg를 맞춘다.

같은 이유로 프로덕션 코드에서도 **gui 게이트된 재export 를 경유하지 않는다** — 예를 들어
`crate::terminal::*` 가 gui 전용 재export 라면 headless 에서도 도는 코드는 원본 크레이트를
직접 가리킨다(`tasty_terminal::*`).

CI 는 `.github/workflows/crossplatform-check.yml` 의 `check-headless` 잡이 이 조합
(`--no-default-features`)의 전체 스위트를 돌려 강제한다 — 통합 타깃도 돈다(채널 상세는
[`ci-gates.md`](ci-gates.md)).

## 7. 병렬 실행 경합(flake) — 공유 상태는 직렬화, 외부 자원은 소유

위 1~4 는 테스트가 **사용자 환경**을 읽어 로컬 상태에 좌우되는 축이다. 이 절은 다른 축 —
테스트끼리 **같은 프로세스에서 병렬로** 공유 상태를 밟아 스케줄링에 따라 나타났다 사라지는
실패다. 부류별 표준 처방과 근거·대안·재검토 조건은
이 문서.

### 형태 A — 프로세스 내 전역 공유 상태

`static` 락/셀, 프로세스 env, 프로세스 cwd 처럼 인스턴스가 하나뿐인 상태를 테스트가
바꾼다. 처방은 **그 상태를 만지는 모든 테스트를 하나의 락으로 직렬화**하는 것이다.

- cwd: `set_current_dir` 는 프로세스에 하나뿐이라 자원을 테스트-로컬로 만들 수 없다 —
  직렬화가 유일하다(`tasty-cli` 의 `cwd_resolve` 테스트가 `CWD_LOCK` 을 함수 끝까지 잡는다).
- `static` 전역: 그 전역을 reset/read 하는 테스트는 락을 함수 끝까지, register 만 하는
  헬퍼는 그 호출을 감싼다(`runtime::surface_registry::webview_kind` 의 `WEBVIEW_KIND_TEST_LOCK`).
- env: §2 의 RAII 가드가 같은 처방의 특수형이다(획득 시 락, Drop 시 복원).

**락은 그 락을 잡는 코드끼리만 막는다(§2 "락은 키 단위" 함정의 하위형태).** 어떤 테스트가
락을 안 잡고 같은 자원을 만지거나 — 특히 그 자원을 읽는 **프로덕션 경로**를 간접 호출하면
— 락은 아무것도 막지 못한다. 그래서 처방을 적용하기 전에 "그 자원을 만지는 테스트가
이것뿐인가" 를 먼저 전수로 확인한다. 다른 크레이트의 같은 종류 접근은 별도 테스트
바이너리(별도 프로세스)라 경합이 아니다.

#### tracing 콜사이트 관심 캐시

로그를 단정하는 시험에서 구독자는 스레드마다 다르지만 콜사이트의 관심(`Interest`)은 프로세스 전역에 캐시된다.
콜사이트를 처음 치는 스레드는 등록된 dispatcher가 하나뿐일 때(`has_just_one`) 다른 dispatcher를 보지 않고
자기 스레드의 기본 구독자로 관심을 계산한다. 구독자가 없는 스레드면 `NoSubscriber`의 `never`가 저장된다.
`has_just_one`은 dispatcher를 등록할 때만 갱신된다(tracing-core 0.1.36 `callsite.rs`).

시험이 `rebuild_interest_cache`를 부른 뒤 경고를 내기 전에, 구독자 없는 다른 시험이 같은 콜사이트를 처음 치면
경고가 시험의 구독자까지 오지 않는다. 등록이 진행 중이면 `sometimes`가 돌아와 첫 경고만 전달되기도 한다.
`rebuild_interest_cache`는 그때까지 등록된 콜사이트만 다시 계산하므로 이 창을 닫지 못한다. 제품 경로를 부르는
시험이면 모두 그 콜사이트를 치므로 형태 A의 락으로 막으려면 그 시험들을 전부 찾아 잠금에 넣어야 한다.

처방은 `tasty_test_support::rerun_alone`으로 단정하는 시험을 자기 바이너리의 새 프로세스에서 혼자 실행하는
것이다. 자식의 성공 종료와 `test result: ok. 1 passed`를 함께 확인하고 실패하면 자식의 출력을 실패 메시지에
싣는다. `surface_lifecycle`의 경고 시험은 같은 모듈의 형제 시험이 구독자 없이 같은 경고를 쳐서, 모듈을
`--test-threads=4`로 1000회 실행하면 182회 실패했다. 격리한 뒤 같은 실행에서 0회다. 경고 줄을 지우는 변이에서는
자식의 단정이 실패하고 부모가 그 출력을 싣고 실패한다.

같은 시험 바이너리 안에서 그 콜사이트를 구독자 없이 치는 시험이나 worker가 있는지 먼저 확인한다. 전체 lib에서
단정하는 콜사이트 앞에 구독자 유무와 스레드 이름을 남기는 임시 계측을 넣어 2회 실행해 찾는다. 파일 로드 경고는
다른 모듈의 시험 세 개가 구독자 없이 친다(`load_diagnostics_tests`). 그 시험들과 함께 8스레드로 600회 실행하면 69회
실패했고 격리 뒤 0회다. 훅 시퀀스 단계 로그는 `hook-sequence` worker가 구독자 없이 친다(`hook_handler::exec`).
부하 없는 반복에서는 300회 재현되지 않았고, `rebuild_interest_cache` 뒤에 1500 ms를 쉬게 하면 20회 모두 실패했다.
격리 뒤 같은 지연에서 0회다. 치는 쪽이 없으면 격리하지 않는다. 저널 캡처 경고와 탐색기 종료 경고가 그렇다.

### 형태 B — 프로세스 밖 OS 자원 (포트·경로·소켓 TOCTOU)

`TcpListener::bind(":0")` 로 포트를 얻고 놓았다가 다시 bind 하는 사이, 고정 이름의 임시
파일을 공유하는 사이 다른 프로세스·테스트가 끼어든다. 처방은 **자원을 놓지 않거나**(리스너를
잡은 채 검증), 놓아야 하면 **lease + 재시도**, 고정 경로면 **유니크 이름**이다.

- 포트: `tasty-ssh` 의 `reserve_local_port` 는 포트를 점유하는 리스너를 함께 반환한다 —
  리스너를 잡고 있는 한 그 포트는 이 프로세스 소유라 rebind 레이스가 없다. 프로덕션에서
  ssh 가 rebind 해야 하는 경우만 명시적으로 drop 하고, 그 창은 ready-probe 재시도가 흡수한다.

### 형태 C — 부하 의존 벽시계 마감(deadline)

공유 상태 경합이 아니라, "N 초 안에 일어난다" 를 **벽시계로 단정**하는 테스트가 러너 부하에
비례해 깨진다. 단독 실행은 빠르게 통과(수십~수백 ms)인데 완주/부하에서 deadline 을 소진하고
실패한다. 처방은 벽시계 폴링을 **이벤트 대기**로 바꾸는 것 — 단, 그 이벤트(waker·EOF)가 부하나
플랫폼 때문에 유실·지연될 수 있으면(Windows ConPTY 는 자식이 죽어도 read EOF 가 늦다) **제품이
이미 가진 폴 주기를 상한으로 결합**한다(이벤트 우선 + 폴백).

- 같은 프로세스 안: `process_exited_eventually_emitted` 는 `recv_timeout` 상한을 제품의
  alive-check 주기(`ALIVE_CHECK_INTERVAL`)로 잘라, waker 가 안 와도 그 주기마다 `process()` 가
  `try_wait` 폴백을 돈다. 자식 사망은 확정이므로 다음 주기에서 잡힌다.
- 별도 프로세스(자식 셸): `spawn_shell` 이 `JoinHandle` 을 반환하게 하고 테스트가 `join` 한다 —
  벽시계 없이 자식 `output()` 완료를 기다린다. 프로덕션은 그 핸들을 `let _` 로 drop 해
  fire-and-forget 을 유지한다(핸들을 버려도 스레드는 detach 되어 계속 돈다).
- 조용한 구간을 "끝" 으로 읽지 않는다: `admissions_beyond_the_disk_credit_wait_for_a_released_credit_instead_of_failing`
  은 완료를 `recv_timeout(1초)` 가 실패할 때까지 모아 "남은 것은 credit 대기 중" 으로 판정했다.
  worker 처리가 부하로 1초 넘게 늦으면 루프가 일찍 끝나고, 늦게 온 완료를 다음 단정이 받는다.
  지금은 요청을 순서대로 처리하는 worker 에 읽기 요청(barrier)을 하나 더 넣고 그 완료까지 모은다.
- 기한은 그 계약이 말하는 구간만 잰다: `disable_returns_before_a_stalled_child_exits` 의 500 ms 상한은
  "메인 루프가 회수(최대 2초)를 기다리지 않는다"를 가른다. 제품에서 설정 파일 확정은 writer worker 가 하고
  메인 루프는 `apply_config_change` 만 부르므로, 시험도 `prepare_config_change`·`execute` 로 확정한 뒤
  `apply_config_change` 만 잰다. 확정의 fsync 는 다른 프로세스의 디스크 사용에 좌우돼 load average 와 무관하게
  500 ms 를 넘는다. 벽시계는 남긴다. 회수 스레드를 띄운 뒤 일부만 기다리는 회귀는 회수 항목 상태로 구별되지
  않기 때문이다. 회수를 끝내고 돌아오는 회귀는 `retiring_finished` 단정이 따로 잡는다.
- 정체 감지는 worker가 잠든 채 진행이 없는 시간만 센다: `app::journal` 시험은 시험 스레드에서
  `poll_bootstrap` 을 부르며 저널 worker의 응답을 기다린다. 벽시계 10초 기한은 두 가지 이유로
  교착 없이 실패했다. 호출 안에서 시험 스레드가 직접 하는 작업(디버그 빌드에서 64 MiB 응답 크기
  계산 한 번에 수 초)이 기한에 들어갔고, 같은 디스크를 쓰는 다른 프로세스 때문에 worker의
  fsync가 몇 초씩 늦어졌다(worker가 `D` 상태로 ext4 저널 커밋을 기다렸다). 지금은
  `StallBudget`(`src/runtime/journal_product/stall_budget.rs`)이 폴링 사이에 쉰 시간 가운데 worker가 잠든
  상태(`S`)였던 시간만 10초 한도로 센다. 실행 중(`R`)이거나 끊을 수 없는 대기(`D`)인 시간은 세지
  않는다. 응답을 잃었거나 잠금을 기다리며 멈춘 worker는 잠들어 있으므로 그대로 10초 안에
  실패한다. 쉬지 않고 도는 worker는 벽시계 상한 300초로 잡는다. worker 상태는 Linux의
  `/proc/<pid>/task/<tid>/stat` 에서만 읽고, 다른 OS에서는 쉰 시간을 모두 센다.
  - 상태가 `R`이어도 flags에 `PF_EXITING`(0x4)이 서 있으면 일하는 중으로 보지 않는다. join이
    돌아온 직후에도 커널이 그 스레드의 종료 경로를 아직 실행하고 있어 `R`로 읽힐 수 있다.
    worker가 시작할 때 기록한 시작 시각(`stat` 22번째 필드)과 다른 스레드도 일하는 중으로 보지
    않는다. 같은 TID를 물려받은 다른 스레드를 worker로 읽지 않기 위해서다.
  - 정체로 판정하면 패닉 전에 그 worker를 버린다고 표시한다. `JournalWorker` 의 Drop은 평소
    worker를 join하는데, 표시가 있으면 join하지 않고 스레드를 떼어 낸다. 그래서 멈춘 worker가
    있어도 시험은 그 자리에서 걸리지 않고 실패로 끝난다.
  - 저널 애플리케이션의 시험(`StallBudget::new`)은 터미널 준비 스레드도 함께 본다. 새 터미널의 셸
    시작은 그 스레드가 맡으므로 그동안 저널 worker가 잠들어 있어도 정체로 세지 않는다. 정체로
    판정하면 준비 스레드도 버린다고 표시해 Drop이 join하지 않는다.
  - 자식 프로세스를 기다리는 시험(`StallBudget::for_process`)은 자식의 모든 스레드를
    `/proc/<pid>/task/*/stat`에서 그때마다 다시 읽는다. 스레드 하나라도 `R`이나 `D`면 쉰 시간을 세지
    않는다. `tests::group::crash`의 자식은 리더가 잠든 채 저널 worker가 fsync로 일하므로 리더만 보면
    fsync가 느릴 때 진행 중인 자식을 정체로 읽는다. 시작 시각 비교는 리더의 `stat`에서 해서 같은 PID를
    물려받은 다른 프로세스를 일하는 중으로 읽지 않는다. 자식의 모든 스레드가 잠든 채 표지를 쓰지 않으면
    10초 안에 실패한다(`a_child_whose_threads_all_sleep_without_a_marker_fails_as_stalled`).
  - `runtime::journal_product` 시험도 worker의 완료를 같은 `StallBudget`으로 기다린다(`receive`).
    완료 하나를 `recv_timeout(10초)`로 받던 때는 부하 평균이 코어 수의 두세 배인 병렬 실행에서
    실행마다 다른 시험이 `Timeout`으로 실패했다. worker가 완료 채널을 닫으면 정체 한도를 기다리지
    않고 바로 실패한다.
  - headless 부트 시험(`boot::journal_event_tests`)은 엔진 준비와 `workspace.create` 완료까지 받는 깨움이
    모두 저널 worker 에서 온다. 두 구간을 벽시계 10초 하나로 묶었을 때는 부하 평균이 코어 수의 두세 배인
    단독 실행에서도 깨움 사이가 수 초씩 벌어져 `bootstrap wake: Timeout` 으로 실패했다. 지금은 같은
    `StallBudget` 으로 기다린다. 쉰 시간은 깨움이 와도 되돌리지 않고 두 단계를 합쳐 누적하므로, worker 가
    잠든 채 기다린 시간이 시험 전체에서 10초를 넘으면 실패한다. 정리 기한이 지났을 때만 깨움을 대신 만드는
    규칙은 그대로이고, 그때도 nap 을 거쳐 쉰 시간을 센다. 기한이 과거에 머문 채 응답이 오지 않아도 한도
    안에서 실패한다. 정리 기한까지 기다리는 시간도 worker 는 잠들어 있으므로 쉰 시간에 들어간다. 이 시험의
    정리 기한은 측정한 실행에서 늘 없었다. 정리 경로를 오래 타는 시험을 이 방식으로 바꿀 때는 기한 대기를
    따로 다룬다.
  - PTY 출력·자식 회수 대기는 `StallBudget::for_process(pid)`로 PTY 자식을 본다
    (`ThreadProbe::process`, `/proc/<pid>/stat`). 자식이 실행 중이거나 디스크 대기인 시간은
    세지 않는다. 그러나 출력 전달과 회수는 Tasty 안의 PTY 읽기 스레드와 회수 스레드가 맡고,
    그동안 자식은 이미 잠들었거나(출력 뒤 `exec sleep`) 좀비다. 그래서 빠지는 시간은 셸이 시작하는
    짧은 구간뿐이고, 이 대기들의 기한은 사실상 쉰 시간 10초다. 출력이 오지 않거나 회수 기록이
    남지 않으면 10초 안에 실패한다. 회수할 이전 PTY가 없는 cleanup 대기는 지켜볼 프로세스 없이
    기다린 시간을 모두 센다. 부하 중 이 대기가 "the watched process slept 10s"로 실패하면 그때 읽기·회수 스레드를
    직접 지켜보도록 바꾼다.
- 다른 스레드의 진행을 기다리면 진행이 멈춘 채 잠든 시간만 센다: tasty-event-store 의
  `concurrent_reader_never_sees_a_partial_batch` 는 writer 스레드가 batch 200 개를 쓰는 동안 다른
  연결로 읽는다. 벽시계 60초 기한일 때는 부하 평균 15 에서 writer 가 156 개까지만 써 실패했다.
  지금은 `ProgressWait`(`crates/tasty-event-store/src/tests/progress_wait.rs`)가 관측한 진행
  (`last_batch`)이 바뀌면 다시 세고, 바뀌지 않은 동안 writer 가 잠든 시간만 10초 한도로 센다.
  writer 가 실행 중(`R`)이거나 디스크 대기(`D`)인 시간은 세지 않고, 쉬지 않고 도는 writer 는 벽시계
  300초로 잡는다. 이 크레이트는 본체의 `StallBudget` 을 쓸 수 없어 판정을 따로 둔다. 같은 한도(10초·300초)와 `R`·`D` 제외를 쓰되, `StallBudget` 과 달리 진행이 보이면 정체 시간을 되돌린다. writer 는
  시작할 때 `/proc/thread-self` 로 자기 `stat` 경로를 넘기고, Linux 가 아니면 진행 없는 시간을 모두 센다.
  batch 마다 400ms 쉬게 한 writer 는 예전 기한으로 60.03초에 `last_batch: Some(148)` 에서 실패했고 지금은
  89.09초에 통과한다. batch 11 뒤 멈춘 writer 는 10.08초에 실패한다.
- 끝난 작업의 결과와 진행 중인 작업을 나눈다: `http_endpoint::runtime` 시험은 실제 profile 작업을 벽시계
  3초 안에 기다렸다. fsync마다 1200 ms를 쉬게 하면 대상 시험이 3회 모두 실패했고, 3500 ms면 첫 실패의 잠금
  poison으로 8건이 함께 실패했다. 지금은 `wait_result`가 작업의 끝을 먼저 읽고 결과를 읽는다. 작업은 끝나기 전에
  결과를 게시하므로 끝났는데 결과가 없으면 시간과 무관하게 바로 `lost`로 실패한다. 끝을 알리는 신호는 대상마다
  다르다. job은 semaphore permit 반환, `spawn_blocking`은 `JoinHandle::is_finished`, setup은 결과를 넘긴 뒤
  부르는 `wake`다. 결과가 없는 retirement 대기(`reaped`)와 끝나지 않는 작업은 `HANG_LIMIT` 300초에 `hung`으로
  실패한다. `serial()`은 poison을 무시하고 앞 시험이 남긴 소유자가 끝난 뒤에 시작하므로 실패는 원인 시험 1건으로
  남는다. 시간으로 단정하던 나머지도 구조로 바꿨다. Drop 시간을 재던 100 ms는 job을 붙잡아 둔 채 Drop이
  돌아오는지로, 경합 중 permit 유지는 worker를 경합 안의 `current` 호출에 세워 두고 확인한다. 1초 admission
  만료는 제품 계약 검증이라 남긴다. 잠금을 쥔 자식이 release 전에는 끝나지 않으므로 결과가 시간에 좌우되지 않는다.
- 순서대로 처리하는 worker의 결과는 fence로 받고 시간으로 판정하지 않는다: 원격 파일 시험
  (`remote_file_tests`)은 `poll_file_writer`와 `try_recv`를 벽시계 5초 동안 돌았다. Windows에서 CPU 바쁜
  프로세스 48개를 띄우면 `held_inline_add_and_remove_answer_original_callers_once_with_actual_operations`가
  20회 중 10회 `original remote caller never completed`로 실패했고, 기한이 지난 13건의 계측은 모두 worker가
  파일을 교체하는 중이었다. 지금은 `file_writer::settle::settle`이 큐에 flush를 넣고 그 완료를 조건 변수로
  기다린다. flush가 끝나면 앞의 요청은 모두 결과를 냈으므로, 거둔 뒤에도 응답이 없으면 느린 것이 아니라
  잃은 것이라 시간과 무관하게 바로 실패한다. Linux에서 fsync마다 3초를 쉬게 하면 수정 전에는 두 시험이
  같은 메시지로 실패했고 수정 후에는 9초에 통과한다. 10초를 쉬게 해도 30~33초에 통과한다. 응답 전달을 막은
  변이에서는 대상 시험 8개 중 응답을 기다리는 6개가 실패하고, 전체 실행은 0.15초에 끝난다.
  `remote_tool_files`와 `detection`의 시험도 같은 방식으로 기다린다.
  - 정체 감지는 별도 계약이다. 교착을 시간 없이 판별할 수 없으므로 `HANG_LIMIT`(벽시계 300초)이
    끝나지 않는 대기만 끝낸다. `StallBudget`을 쓰지 않은 이유는 Windows에서 쉰 시간을 모두 세어 10초 기한과
    다르지 않기 때문이다. 잠금을 놓지 않은 변이는 301초에 flush 대기로 실패한다. 부하가 이 상한을 넘기면
    다시 실패할 수 있지만 그때의 메시지는 응답 유실과 구별되고 writer 계측을 함께 출력한다.
- 제품이 시각으로 판정하면 시험이 시각을 정한다: tasty-task-runtime 의 TTL 갱신 시험은 러너가
  tick 마다 갱신하는 lease·permit 이 TTL(300ms) 뒤에도 남는지 본다. 실제 시각으로 tick 을 돌리면
  부하로 tick 이 늦을 때 갱신 전에 만료돼 실패했다. 지금은 `HostExecutor` 가 점유를 얻고 갱신할 때
  `HoldingClock` 을 읽고, 시험은 `HoldingClock::manual` 시각을 tick 마다 TTL 의 1/6 씩 올린다.
  다른 holder 의 획득과 만료 판정도 그 시각으로 하며, 매 tick 의 `maintain` 직전에 점유가 아직
  만료되지 않았는지 단언해 갱신 누락과 늦은 갱신 주기를 함께 잡는다.
  주입 시계와 실제 시각이 섞인 경우도 같다. `pty.rs`의 유휴 회수 시험은 spawn 기준 시각이 `FakeClock`의
  고정 시각이고 `lazy_sweep`은 실제 시각과 비교한다. 한도 1초일 때는 `core()` 생성 뒤 1초가 지나면
  회수 단계 전에 spawn이나 list가 먼저 회수해 실패했다. `core()` 바로 뒤에 1100ms를 쉬면 항상 재현됐고,
  바로 앞에 쉬면 재현되지 않았다. CPU 부하를 걸었을 때 `core()`부터 list까지는 sub에서 최대 771ms였으므로
  부하 반복으로는 재현되지 않았다. 지금은 실제 시간이 닿지 않는 한도(1시간)를 쓰고 `now + 한도`를 넘겨
  회수하므로 시험이 넘긴 시각만 판정을 정한다.
  판정이 여러 경로에서 벽시계를 따로 읽으면 경로마다 시각 인자를 넘기는 대신 시각의 출처를 시험이 정한다.
  codex 플러그인의 attach 결과 재질문과 전달 재시도는 `store::now_ms`로 다음 시각을 정하고 판정한다. 시험은
  "다음 시각 전에는 묻지 않는다"를 바로 다음 호출로 단정했다. 그래서 두 호출 사이가 1초(`backoff(1)`)를 넘으면
  재질문이 먼저 나갔고, 기록을 저장하는 fsync도 이 시간에 들어갔다. fsync마다 200 ms를 쉬게 하면 한 시험의
  서로 다른 단정이 5회 모두 실패했고(지연에 따라 실패 줄이 달랐다), 600 ms로 패키지 전체를 돌리면 세 시험이
  실패했다. 지금은 `store::test_clock::freeze`가 시험 스레드의 시각을 멈추고, 시험이 `advance`로 재시도 시각
  1 ms 전과 그 시각을 각각 만들어 묻지 않음과 물음을 단정한다. 다음 시각을 저장하는 값을 없애거나 두 배로
  늘리는 변이와 "시각 전" 판정을 없애는 변이에서 시험이 실패한다. 이 시계는 호출한 스레드만 멈추므로 판정이
  시험 스레드에서 도는 시험에만 쓴다.
- **타임아웃 상향은 처방이 아니다** — 발생 빈도만 낮추고(확률 저감) 부하가 그 상한을 넘는
  날 다시 깨진다. 근거는 유닛 테스트 격리.

### 형태 D — 사용자 홈을 기준선으로 삼는 시험

시험이 `tasty_home()` 아래의 파일을 읽는 프로덕션 경로를 간접 호출한다. 그 파일은 앞선
완주가 남긴 것이라 **기계마다 다르고**, 같은 기계에서도 어제의 실행이 오늘의 기준선을 바꾼다.
§1~4 가 *읽는 값*(설정 · env)을 갈아끼웠다면 이쪽은 **읽는 자리**가 그대로 남은 축이다.

**전체 실행은 통과하지만 단독 실행은 실패할 수 있다.** 완주에서는 같은 파일을 쓰는 다른
시험이 먼저 `save()` 로 오염을 덮어쓰므로, 문제의 시험이 읽을 때쯤 내용이 이미 깨끗하다.
그 시험만 떼어 돌리면 덮어쓸 것이 없어 오염이 그대로 읽힌다. 그래서 전체 실행 통과만으로 격리를 증명할 수 없다.

처방은 직렬화가 아니라 **자원의 테스트-로컬화**다 — `tasty_utils::path::push_home_override`
(스레드 로컬, env 미조작, 테스트 전용 홈 경로 주입)로 그 시험 전용 임시 홈을 세운다.

- 본체는 `EngineSession` 이 `IsolatedHome` 을 **마지막 필드**로 들고, 조립 지점
  `new_with_ids_and_settings` 가 그것을 첫 줄에서 세운다. 그래서 생성자를 무엇으로 부르든
  — 픽스처를 거치든 그 함수를 직접 부르든 — 같은 격리를 받고, 생성 이후의 `save()` 까지
  엔진 수명 내내 덮인다(마지막 필드인 것이 그 계약이다: drop 순서가 선언 순서다).
- 엔진 없이 홈을 읽는 것을 만드는 시험은 그 헬퍼가 직접 가드를 쥔다 —
  `plugin_bridge::key_dispatch` 의 `mgr()` 이 그 형태다(`PluginManager::with_registries` 가
  생성 중에 `plugins-logs` 를 만든다).

**시험마다 다른 키(surface id 등)를 고르는 완화책은 처방이 아니다.** 같은 파일을 쓰는 자리가
하나 늘 때마다 손으로 지켜야 하고, 그 규율이 지켜졌는지 재는 채널이 없다 — 어겨도 완주는
통과하므로 실행 결과만으로 드러나지 않는다.

### 형태 E — 시험이 쓴 실행 파일을 곧바로 실행

스텁 판정기처럼 시험이 실행 파일을 쓰고 바로 실행하면, 같은 프로세스의 다른 시험 스레드가 그
사이에 fork 한 자식이 쓰기 fd 를 물려받는다. 그 자식이 exec 해 fd 를 닫기 전까지 커널은 파일이
쓰기 중이라고 보고 실행을 `ETXTBSY`(문구 "Text file busy", 셸 종료코드 126)로 거부한다.
`O_CLOEXEC` 는 exec 이후의 누수만 막고 fork 와 exec 사이는 막지 못하며, 이름 바꾸기도 같은
inode 라 소용없다. 병렬 시험이 프로세스를 많이 띄울수록 자주 깨진다.

처방은 **시험 프로세스가 그 파일을 쓰기로 열지 않는 것**이다. 자식 셸이 쓰게 하거나
(`sh -c 'cat > "$1" && chmod 755 "$1"'` 에 내용을 stdin 으로 준다), 저장소에 실행 권한과 함께
둔 고정 픽스처를 가리킨다. 실행을 재시도하는 것은 빈도만 줄인다.
시험용 실행 파일은 `tasty_test_support::write_executable`(unix)로 쓴다. 이 도우미가 자식 셸
방식이고, 같은 조건의 경합 시험을 함께 둔다.

### 실제 private profile의 잠금 admission

Linux profile의 namespace lock도 fork 자식이 같은 open-file description을 물려받으면
부모가 fd를 닫은 뒤까지 유지된다. `O_CLOEXEC`는 이 구간의 `LOCK_NB` 거절을 막지 못한다.
namespace 잠금의 WouldBlock은 실제 용량 부족과 별개인 Busy로 분류한다.
production `ProfileWork`가 기존 worker permit을 보유한 채 제한된 admission 기한 안에서만
Busy를 기다리므로, runtime 양성 fixture는 terminal Capacity를 재시도하지 않는다.
root 상한 8개와 readiness 이후 claim 기한은 그대로다.

`runtime::tests::profile_fixture`는 CLOEXEC fd를 실제 fork 자식에 남겨 admission 만료를
확인한 뒤 그 정확한 자식을 해제·회수하고 실제 profile/lease를 받는다.
채워진 namespace의 Capacity는 terminal이며, 여덟 actual profile job의 잠금 대기가
permit을 유지하고 아홉 번째 job을 거절하는 별도 제어 시험을 둔다.
`preparation::admission_tests`는 세 실제 live root가 있는 namespace에서 sweep 및 create
잠금을 각각 잡고 해제한 뒤 하나의 실제 profile이 준비되는지 검사한다. 세 live root를
만드는 양성 fixture 초기화와 제어용 잠금 획득은, 병렬 fork가 물려받은 setup 잠금의
typed Busy만 최대 2초 재시도한다. 실제 held-lock 단계나 Capacity 거절 제어는 이 setup
재시도로 감싸지 않는다. receipt Drop과
current만 바뀐 취소도 잠금을 해제하기 전에 worker를 끝내며, poison·안전하지 않은 경로는
복구하거나 chmod하지 않는다. 실제 WebKit 수명이나 과거 native 실패의 잠금 소유자를
증명하는 시험은 아니다.

### 잠금 해제를 `try_lock`으로 바로 단정하는 시험

webhooks.toml 섹션 API가 반환 전에 잠금을 풀었는지 `try_lock`으로 단정하는 시험도 같은 경로로 흔들린다.
같은 시험 바이너리의 다른 시험이 띄운 자식이 fork와 exec 사이에 잠금 파일의 open file description을
물려받으면, 부모가 fd를 닫은 뒤에도 자식이 exec할 때까지 flock이 남아 단정이 실패한다. 제품 잠금이
늦게 풀린 것이 아니다.

이 단정을 쓰는 시험은 위의 자식 재실행 방식으로 자기 시험 하나만 `--test-threads=1`로 새 프로세스에서
실행한다. 그 프로세스에서는 다른 시험이 자식을 띄우지 않는다. 부모는 자식의 성공 종료와
`test result: ok. 1 passed`를 함께 확인해 이름 필터 오류를 통과로 세지 않는다.
`crates/tasty-settings/src/webhook_port_file/backend_tests.rs`의 `unlocked()`는 이 자식 안에서만
실행되도록 확인한다. `crates/tasty-cli/src/local/port.rs`의 offline adapter 시험도 같은 방식이다.
재시도형 단정은 쓰지 않는다. 해제가 늦어지는 결함도 통과시키기 때문이다.

<a id="형태-판별식--증상-하나로는-안-갈린다"></a>

### 실패 형태는 조사 단서로 사용한다

| 관측 | 우선 확인할 것 |
|---|---|
| 제한시간을 소진하고 단독 실행은 빠름 | 스케줄링 지연, 대기 조건, 이벤트 누락 |
| 병렬 실행에서 즉시 실패 | 락 없는 전역 상태, 환경변수·cwd 변경 |
| 로그 단정만 병렬 실행에서 수가 모자람 | 형태 A의 tracing 콜사이트 관심 캐시: 그 콜사이트를 구독자 없이 치는 다른 시험 |
| 고정 포트·경로에서 실패 | 자원 소유, 재바인딩 사이의 경쟁, OS 파일 처리 차이 |
| 시험이 쓴 스크립트 실행이 종료코드 126 | 형태 E: 쓰기 fd 를 물려받은 다른 스레드의 자식 |
| 전체 실행은 통과하지만 단독 실행은 실패 | 다른 시험이 만든 선행 상태, 사용자 홈 기준값 |

한 패턴이 한 원인을 확정하지는 않는다. 성공·실패 실행의 자식 생존 상태, 이벤트,
사용한 경로와 대기 시간을 비교해 원인을 좁힌다. 양쪽에 공통으로 있는 로그만으로
실패 원인을 단정하지 않는다.

### temp 경로 판정의 변수 범위

`tasty-doc-guards`의 `temp_path` 판정기는 경로가 참조한 단순 `let` 바인딩의 유일성
성분을 같은 함수의 유효한 블록 안에서만 전파한다. 선언 이전에는 보이지 않고,
성분이 없는 동명 바인딩도 앞의 값을 가린다. 안쪽 블록을 나가면 바깥 바인딩이 다시
보이며, 다른 함수와 중첩 함수의 지역 변수는 섞지 않는다. 직접 성분을 읽는 기존
줄 창도 다른 함수나 해당 자리의 블록 바깥에서 성분을 빌리지 않는다.

검증은 전체 census의 합계뿐 아니라 **어느 자리가 이동했는지**를 대조한다.
`cargo test -p tasty-doc-guards --lib temp_path`는 동명 변수·shadowing·블록·선언 순서와
주석/문자열 마스킹을 검사한다. 이전 파일 전체 전파를 되살린 변이는 이 대조에서
실패해야 한다. `--test no_unshared_fixed_temp_path`는 실제 저장소의 공유 고정 경로,
시계 단독 격리, 미검토 재호출/전달 자리의 래칫을 함께 검사한다. 가드를 수선하면서
새로 드러난 약한 fixture도 성분 자체를 고치며 pin이나 면제로 통과시키지 않는다.

이 판정은 소스 텍스트 검사다. 매크로 확장·타입 해석·destructuring 패턴·임의 재대입의
데이터 흐름까지 Rust 컴파일러처럼 해석하지 않는다. 직접 성분의 줄 창과 helper로
넘겨진 temp root의 관측 한계도 남는다.

<a id="부하-재현과-ci러너-관측"></a>
<a id="가드가-실제로-도는지는-이름-으로-스캔은-집합-동등-으로-확인한다"></a>

### 부하와 CI 결과를 비교한다

부하 실험은 다른 검증과 조율한 환경에서 같은 커밋·명령으로 반복하고 재현률을 기록한다.
성공·실패 실행의 실제 상태와 시간 대조군을 비교한다. 특정 OS에서만 재현되는 문제는
해당 OS의 로그와 코드로 확인한다. 로컬 한 번의 성공이나 직전 변경 파일 목록만으로
원인이 해결됐거나 변경과 무관하다고 결론 내리지 않는다.

```bash
gh run view <run_id> --json jobs
gh api "repos/<owner>/<repo>/actions/jobs/<job_id>/logs"
```

잡 결론뿐 아니라 각 단계의 실제 실행·실패·생략을 읽는다. 로그가 만료되면 확인한 범위가
줄었다고 보고한다. 단계 번호가 없다는 사실만으로 실제 실행 여부를 추측하지 않는다.

소스 가드는 읽은 파일 집합과 하한을 함께 확인한다. 정확한 개수도 같은 수의 다른 파일로
바뀐 경우를 놓칠 수 있으므로 추가·누락 경로를 양방향으로 비교한다.
두 feature 조합에서 검사하려면 각각 `-- --list`로 이름을 찾고 실제 실행 로그도 읽는다.
목록 조회는 시험 실행이 아니다. 패키지·타깃 경계를 남겨 같은 이름을 합치지 않는다.

<a id="8-공유-픽스처-test_state-는-진짜-프로세스를-띄운다"></a>
<a id="그-spawn-을-건너뛰는-길"></a>

## 8. 공유 픽스처 `test_state()` 는 프로세스를 띄우지 않는다

`src/state/tests.rs` 의 `test_state()` · `test_state_with_memory()` · `test_mirror_state()` ·
`test_state_from_model()` 는 유닛 테스트가 `RequestContext` + `EngineSession` 한 쌍을 얻는
표준 통로다. Session 소유자는 시험이 끝날 때까지 유지하고 `borrow_mut()`로 구조·실행 자원을
대여한다. 네 함수는 같은 `state_fixture` 로 합류한다.

- 엔진은 `EngineSession::new_with_ids_and_settings` 로 만든다. 서비스와 자원 소유자만
  할당하고 구조나 PTY 는 만들지 않는다. 시험 빌드에서는 생성 중에 `IsolatedHome` 을 잡는다.
- 구조는 canonical 사실로 넣는다. 카테고리·워크스페이스·탭과 terminal surface 하나의
  `DomainEvent` 로 만든 `JournalModel`(또는 `test_state_from_model` 이 받은 모델)을
  `projection::bootstrap::initialize` 로 투영한다. `test_mirror_state()` 는 mirror
  워크스페이스를 넣는다.
- terminal surface 에는 `Terminal::new_detached` 를 붙인다. PTY·자식 프로세스·reader
  스레드가 없는 터미널이다. `empty` surface 는 `EmptySurface` 를 받고, 그 밖의 kind 값은
  시험이 직접 설치한다.
- WebView kind 와 오버레이 등록(`markdown`)은 플러그인 프로세스 없이 등록한다.

실제 셸·PTY 를 실행하는 시험은 이 픽스처가 아니라 저널·PTY 하네스를 쓴다. 예를 들어
`runtime::journal_product` 의 준비 시험은 `effect_runner::prepare_and_wait` 로 터미널 준비 스레드에서
`/bin/sh` 를 띄운다. 준비 스레드의 실행을 붙잡아 두려면 `TerminalSpawner::hold` 를 쓴다.

<a id="몇-번-띄우는지는-이렇게-센다"></a>
<a id="안-쟀다--그-총수의-귀속-그리고-재려면-무엇이-필요한가"></a>
<a id="접두마다"></a>
<a id="안-쟀다--동시에-살아-있는-fd-의-곡선"></a>

### 프로세스 수와 fd 사용량을 측정한다

어떤 시험이 프로세스나 PTY 를 여는지는 이름이나 픽스처로 짐작하지 않고 실행해서 센다.

```bash
cargo test -p tasty --lib --no-run
strace -f -e trace=execve,socketpair,openat -o /tmp/sp.txt <테스트 바이너리> --exact <시험 이름>
grep -c 'execve(' /tmp/sp.txt
grep -c 'socketpair(AF_UNIX' /tmp/sp.txt
grep -c ptmx /tmp/sp.txt
```

`execve` 첫 줄은 시험 바이너리 자신이다. `socketpair` 는 `std::process::Command::spawn` 이
exec 결과를 알리려고 만드는 AF_UNIX SEQPACKET 쌍을 포함하는 보조 지표이고, `ptmx` 는 PTY
master 를 연 횟수다. 다른 소켓 사용과 프로세스 생성 방식을 구분하지 않으므로 일반적인
spawn 총수와 같다고 단정하지 않는다. 직접 spawn 경로를 계측하거나 호출 스택과 대조해 지표가
맞는지 확인한다. `--exact` 에 넘긴 이름이 비면 전체가 돌므로 test result 줄의 수를 확인한다.

어느 시험이 얼마나 만드는지 찾으려면 전체 이름을 기준으로 부분집합을 나눠 비교한다.
필터가 겹치거나 빠지지 않았는지 확인한다. 공유 초기화와 실행 순서 때문에 부분 실행의 합이
전체와 다를 수도 있으므로 합이 다르다는 이유만으로 분할 오류라고 결론 내리지 않는다.

총 spawn 수와 동시에 열린 fd의 최댓값은 다르다. 병렬도를 달리해 직접 실행한 PID의
fd 수와 종류를 비교한다. 폴링은 짧게 열린 fd를 놓칠 수 있으며 openat/close만 추적해도
socket·pipe·dup으로 만든 fd를 모두 재구성할 수는 없다. 사용한 관측 범위를 함께 기록한다.

### 픽스처의 ID는 실행 순서에 따라 달라진다

`test_state()`가 만드는 워크스페이스·pane·tab·surface ID는 프로세스 전역 카운터에서 나온다.
그래서 같은 시험도 앞서 같은 프로세스에서 픽스처를 몇 번 만들었는지에 따라 다른 ID를 받는다.
이 값과 따로 할당된 ID가 같은 엔진에서 만나는 시험은 함께 도는 시험에 따라 결과가 바뀐다.

- 예: `runtime::journal_product` 시험은 새 저널에서 surface ID 1·2를 예약한다. 그 엔진을
  `test_state()`로 만들면 픽스처가 2를 받은 시험에서 준비가 "already has a live owner"로 거절된다.
- 처방: 이런 시험은 `test_state_from_model(test_model(...))`로 겹칠 수 없는 고정 ID(예: 100)의
  모델을 직접 준다. 픽스처 ID에 의존하는 다른 시험이 같은 이유로 깨지지 않도록 카운터 값을
  단언하거나 하드코딩하지 않는다.
- 확인: 해당 시험들만 이름으로 골라 함께 여러 번 실행한다. 단독 통과나 전체 실행 통과는
  순서가 우연히 맞았다는 뜻일 수 있다.

### 왜 이것이 격리 문서에 있나

§7 형태 B(프로세스 밖 OS 자원)의 모집단은 픽스처 이름으로 정해지지 않는다. `test_state()` 를
쓰는 시험은 PTY 나 자식 프로세스를 잡지 않고, 저널·PTY 하네스나 `tasty_terminal` 의 spawn
경로를 부르는 시험이 잡는다. 어떤 시험이 그 자원을 만지는지는 위 측정으로 확인한다.

## 실패가 실행 순서와 부하에 따라 달라질 때

먼저 실패한 실행과 성공한 실행의 같은 로그를 비교한다. 양쪽에 모두 있는 경고만으로
실패 원인을 정하지 않는다. 단독 재실행이 통과했다는 사실만으로 원인까지 확정하지 않는다.

| 공유 대상 | 먼저 적용할 방법 | 남는 제한 |
|---|---|---|
| 테스트가 직접 초기화하는 전역 값 | 초기화부터 단언까지 같은 락을 유지 | 같은 값을 읽는 모든 테스트도 락에 참여해야 함 |
| 제품 코드도 읽는 환경변수·홈 경로 | 값을 인자로 전달하거나 테스트별 인스턴스 사용 | 쓰는 테스트만 잠그면 제품 읽기 경로는 보호되지 않음 |
| 사용 중이어야 할 포트 | 리스너를 유지한 채 넘기는 `PortLease` | 소비자가 번호만 받아 다시 bind하면 재시도가 여전히 필요 |
| 닫힌 포트를 전제한 오류 검사 | 가능하면 실제 소켓 대신 오류 변환 함수를 검사 | OS 동작이 대상이면 재시도 사유를 명시 |
| 파일·소켓 이름 | 임시 디렉터리나 프로세스별 고유 이름 | 여러 실행이 같은 고정 경로를 사용하지 않도록 확인 |
| 프로세스 종료·이벤트 도착 | 이벤트를 기다리고 타임아웃은 무한 대기 방지에 사용 | 성능 요구가 있을 때는 별도 지연 검사 필요 |

재시도, 동시 실행 제한, 타임아웃 증가는 실패 빈도를 줄일 수 있다.
그 조치를 썼다면 조치 없이 재현한 조건과 부하를 남기고, 근본 수정 작업을 별도로 추적한다.
로컬에서만 적용한 완화가 CI에서도 적용된다고 가정하지 않는다.

가드가 GUI와 headless 양쪽에서 실행된다고 보고하려면 두 빌드 조합의 `--list`에서
테스트 이름을 확인한다. 이름이 있는 것과 같은 파일들을 검사하는 것은 별개다.
검사 파일 수 하한은 큰 누락만 잡고, 위반 건수 고정은 위반 없는 파일의 누락을 놓친다.
파일 목록 전체가 같은지는 경로 집합을 비교해야 한다. 아직 구현하지 않은 검사는
요구사항으로 표시하며 현재 보호 수단에 포함하지 않는다.

### 하네스의 공유 락

하네스 락은 선언 위치보다 획득 방법과 보호 범위를 확인한다.
`test_harness_lock_unwrap_ratchet`은 루트 `tests/`의 비면제 `.lock().unwrap()` 등을 검사한다.
검사할 획득 지점의 최소 개수는 대규모 누락을 찾기 위한 하한이며 정확한 전체 개수가 아니다.
하한보다 많은 부분 누락과 크레이트 내부 하네스는 이 검사만으로 확인할 수 없다.
락을 잡은 채 단언이 실패할 수 있는지, poison 뒤에도 진단 수집이 필요한지는 별도로 검토한다.
