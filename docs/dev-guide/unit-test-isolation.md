# 유닛 테스트 격리 — 사용자 환경을 읽지 않는다

유닛 테스트의 결과는 **실행하는 사람의 로컬 상태에 좌우되면 안 된다.** 사용자 홈의
`config.toml` 값 하나로 무관한 테스트가 깨지면, "내 변경이 깬 것인가" 판정이 매번 수동
대조가 되고 CI 러너와 개발자 머신의 결과가 갈린다. 결정 근거는
[ADR-0096](../adr/0096-unit-tests-isolated-from-user-environment.md).

e2e 테스트의 격리 단위(프로세스 vs workspace)는 다른 축이다 — [e2e-tests](e2e-tests.md) ·
[ADR-0090](../adr/0090-test-isolation-by-workspace-not-process.md).

## 1. 설정: 테스트 생성자는 `Settings::default()` 를 쓴다

`CoreState` 생성자는 설정을 **어디서 얻는지** 로 갈린다.

| 생성자 | 설정 출처 | 용도 |
|---|---|---|
| `CoreState::new(cols, rows, waker)` | `Settings::default()` | 테스트 / non-host 진입점 |
| `CoreState::new_with_ids(...)` | `Settings::load()` (`$TASTY_HOME/config.toml`) | host 부팅 경로 |

둘 다 내부 `new_with_ids_and_settings(..., settings)` 로 합류한다 — 설정 주입 지점이
여기 하나뿐이라, 새 진입점을 만들 때도 "파일을 읽을 것인가" 를 명시적으로 고르게 된다.

**테스트에서 특정 설정이 필요하면 엔진을 만든 뒤 그 필드만 바꾼다** — 사용자 홈에 그 값이
있기를 기대하지 않는다:

```rust
let (mut state, mut engine) = test_state();
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

### 같은 키에 락이 셋인데 그것이 옳다 — 배제 단위는 레포가 아니라 **테스트 바이너리**다

`TASTY_HOME`/`HOME` 을 지키는 락이 레포에 **셋**이다(실측 2026-09-08).

| 락 | 선언 자리 | 그 락이 사는 테스트 바이너리 |
|---|---|---|
| `TASTY_HOME_ENV_LOCK` | `crates/tasty-test-support/src/lib.rs` | `tasty-test-support` (루트 `tasty` 의 dev-dependency) |
| `HOME_ENV_LOCK` | `crates/tasty-host-plugin/src/test_support.rs` | `tasty-host-plugin` |
| `SERIAL` | `crates/tasty-settings/src/general.rs` | `tasty-settings` |

셋은 **서로 다른 `Mutex` 인스턴스라 서로를 배제하지 않는다.** 위 "락은 키 단위로 하나"
를 읽고 나면 이것이 그 규칙을 어긴 것처럼 보이는데, 아니다 — 그 규칙의 범위가
**한 테스트 바이너리 안**이기 때문이다.

환경변수는 **프로세스별**이고 cargo 는 크레이트마다 별도 테스트 바이너리를 만든다.
그래서 배제가 필요한 단위는 레포가 아니라 바이너리 하나이고, **그 안에서는 각자 하나뿐**
이다. 셋이 한 바이너리에 같이 들어갈 수 없다는 것은 값으로 확인된다:

- `src/lib.rs` 의 `pub(crate) use tasty_test_support as test_support;` — 선언에
  **`#[cfg(test)]` 가 붙어 있다.** 루트에 `lib` 타깃이 생긴 뒤에도([ADR-0325](../adr/0325-the-root-package-splits-into-a-lib-and-a-bin.md))
  그 cfg 는 이 크레이트를 *의존으로* 컴파일할 때 세워지지 않으므로 **다른 크레이트가 이
  이름을 링크할 수 없다.** 게다가 이 패키지를 의존으로 드는 워크스페이스 크레이트는
  **하나도 없다**(`cargo metadata` 로 셌다).
- `crates/tasty-host-plugin/src/lib.rs` 의 `mod test_support;` — 선언에 **`#[cfg(test)]` 가
  붙어 있다.** 그 cfg 는 이 크레이트를 *의존으로* 컴파일할 때 세워지지 않으므로 **다른
  크레이트가 이 모듈을 링크할 수 없다.**
- `tasty-settings` 의 `SERIAL` 은 그 크레이트의 `#[cfg(test)] mod tests` 안에만 있다.

⇒ **셋을 공용 락 하나로 합치는 처방은 틀린다.** 크레이트를 가로지르는 락을 만들려면 세
크레이트가 한 크레이트에 함께 의존해야 하는데, 같은 프로세스에 둘이 있을 수 없으므로
그 결합이 사는 것이 없다. 늘어나는 것은 의존뿐이다.

#### 이 추론을 지탱하는 것은 `pub` 여부가 아니다

`mod test_support` 를 밖에서 못 쓰게 막는 것이 **가시성**이라고 읽기 쉬운데, 아니다.
선언에 붙은 `#[cfg(test)]` 가 막는다 — 그 cfg 는 크레이트를 *의존으로* 컴파일할 때
세워지지 않으므로, `pub` 을 붙여도 다른 크레이트가 그 모듈을 링크할 수 없다. 즉 두 홈
env 락이 한 테스트 바이너리에 함께 서는 상태는 **언어가 이미 막고 있다.**

그래서 무너뜨리려면 가시성이 아니라 **attribute 자체**를 떼야 한다. 이 논증에서 사람이
실수할 수 있는 자리는 그 한 줄뿐이고, `test_support::tests::the_module_declaration_stays_test_only`
(`crates/tasty-host-plugin/src/test_support.rs`)가 그것을 본다 — 실패 메시지는 "고쳐라"
가 아니라 **이 절의 논증이 서 있던 전제가 사라졌다**고 말한다. 그때 다시 물어야 하는
것은 선언 한 줄이 아니라 배제 단위 자체다.

루트 쪽은 물음이 다르다. 위 `#[cfg(test)]` 선언 때문에 링크가 애초에 불가능하므로 여기서
지킬 전제가 없고, 그 크레이트의 `the_lock_declaration_stays_module_private`
(`crates/tasty-doc-guards/tests/tasty_home_env_has_one_touch_point.rs`)가 지키는 것은
**같은 크레이트 안에서** 락을 직접 잡고 env 를 만지는 우회다 — 다른 축이다.

#### 재는 법 — 경계를 먼저 물어라

전역처럼 보이는 자원을 만나면 **그 자원의 경계가 어디인가**를 먼저 묻는다 — 프로세스인가,
레포인가, 기계인가. 경계를 안 묻고 배제 단위를 레포로 잡으면 필요 없는 결합을 만든다.

나머지 두 락에 같은 물음을 대면 답이 다르게 나온다: `ENV_LOCK`(tasty-telemetry)은
`TASTY_AGENT_ID` 를, `CWD_LOCK`(tasty-cli)은 프로세스 cwd 를 지킨다 — **자원이 달라**
위 셋과 합칠 대상이 애초에 아니다. "락이 다섯이니 많다" 는 세는 단위가 틀린 것이다.

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
[ADR-0105](../adr/0105-no-nongit-path-refs-in-tracked-sources.md) 가 정본이다.

의존이 없는 작은 fixture에서 직접 이름을 짓는 경우에는 PID로 프로세스를 가르고,
함수 호출 사이에 유지되는 `static` 단조 counter로 같은 프로세스의 재호출을 가른다.
Codex 셸 실행 시험·유효 폰트 시험·파일 선택기 테스트 helper는 이 두 축을 함께 쓴다. 셸 시험의 production
prompt cleanup은 처음 받은 temp root를 재사용해 생성과 회수의 경로 규칙을 유지한다.

## 5. 확인 방법

**"결과가 같은가" 로는 안 본다.** 완주 초록인데 격리가 안 된 형태가 있기 때문이다(§7 형태 D)
— 그때는 같은 파일을 쓰는 다른 시험이 먼저 `save()` 로 오염을 덮어써서, 어느 홈에서 돌리든
같은 초록이 나온다. 그리고 **사용자 실제 홈으로 완주하지 마라** — 그것은 관측이 아니라 오염
행위다. 그 완주가 `~/.tasty{-debug}/` 에 쓴 파일이 다음 실행의 기준선이 된다.

세 관측 모두 **빈 임시 홈**에서 한다.

```bash
H=$(mktemp -d)
TASTY_HOME=$H cargo test --workspace --locked
find "$H" -mindepth 1                                # (가) 출력이 비어야 한다
```

- **(가) 잔여물 0** — 완주 뒤 그 홈에 파일도 디렉토리도 안 생긴다. 초록 여부가 아니라
  **무엇이 쓰였는가**를 본다. 생긴 것이 있으면 그 경로가 새는 자리를 지목한다.
- **(나) 되먹임 불변** — (가) 에서 잔여물이 나왔으면 그것을 빈 홈에 되돌려 두고, 그 파일을
  읽는 시험을 **단독으로** 돌린다. 빈 홈에서의 단독 결과와 갈리면 그 시험은 사용자 홈을
  기준선으로 삼고 있다. 완주로는 이 갈림이 안 보인다.
- **(다) 동시성 불변** — 서로 다른 작업 트리에서 **동시에** 돌린 두 완주의 failed 수가
  서로, 그리고 단독과 같다. 실행 순서 고정(`-- --test-threads=1`)은 같은 축의 약한 형태다.
  ★ **이 좌변은 공유 상태가 없어도 갈린다** — failed 수만 세면 **러너 굶김을 오염으로 읽는다.**
  실측 2026-09-20(20 CPU · 부하 55): 두 완주를 `--test-threads=8` 로 겹치자 한쪽만 2 건
  빨갰는데 **공유 홈 잔여물은 0** 이었고, 같은 기계에서 `--test-threads=3` 으로 낮추니 양쪽이
  단독과 같은 `0 failed` 가 됐다. 빨간 2 건은 시간 단정이었고, 그 시험 자신의 대조군
  ([`ControlProbe`](../adr/0181-a-latency-assertion-must-carry-a-control-that-load-moves-and-code-does-not.md))
  이 "대조군도 기준선의 6.6 배로 부풀었다 — 러너가 굶은 것이라 코드에 대한 증거가 아니다" 를
  실패문에 찍어 축을 갈라 줬다. 그러니 이 관측은 **(가) 와 짝으로만 읽는다**: 잔여물이 0 인데
  failed 수가 갈리면 그것은 격리가 아니라 기계다. 대조군이 없는 단정이 갈렸으면 스레드 수를
  낮춰 한 번 더 재고 나서 판정한다.

```bash
TASTY_HOME=$(mktemp -d) cargo test --workspace --locked -- --test-threads=1
```

**격리가 정말 배선됐는지는 변이로 확인한다** — 주입을 한 줄 빼고 그 시험이 다시 실제 홈을
읽는지 본다. 안 죽으면 배선이 안 된 것이다(절차는 [self-verification](self-verification.md)).

실측 2026-09-20: `IsolatedHome` 의 override 를 세우고 내리는 두 줄을 빼고 빈 홈으로 본
바이너리 스위트를 완주하자 그 홈에 `child-terminals.json` · `file-handler-recent.json` ·
`plugins-logs/` · `zsh-integration/.zshenv` 가 나타났다(잔여물 0 → 5 자리). **그런데 완주는
그대로 초록이었다** (`ok. 2616 passed; 0 failed`). 이 절 맨 앞의 "결과가 같은가로는 안 본다" 가
그 값이다 — 격리가 통째로 풀려도 초록은 안 움직이므로, 변이를 죽이는 좌변은 통과 수가 아니라
**(가) 의 잔여물 목록**이어야 한다.

## 6. feature 별 테스트 게이팅

본 바이너리는 `gui` feature 로 갈린다(`--no-default-features` = headless). **gui 전용 타입·모듈을
단정하는 테스트 모듈에는 `#[cfg(test)]` 가 아니라 `#[cfg(all(test, feature = "gui"))]` 를 건다.**
`#[cfg(test)]` 만 걸면 headless 테스트 **바이너리 자체가 컴파일되지 않는다** — 프로덕션 코드는
멀쩡히 `cargo check --no-default-features` 를 통과하는데도 그 feature 조합의 테스트가 한 줄도
실행되지 않는 사각이 생긴다. 개별 테스트 함수만 gui 전용이면 함수에 `#[cfg(feature = "gui")]` 를
건다.

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
[ADR-0129](../adr/0129-flaky-test-classes-and-standard-fixes.md).

### 형태 A — 프로세스 내 전역 공유 상태

`static` 락/셀, 프로세스 env, 프로세스 cwd 처럼 인스턴스가 하나뿐인 상태를 테스트가
바꾼다. 처방은 **그 상태를 만지는 모든 테스트를 하나의 락으로 직렬화**하는 것이다.

- cwd: `set_current_dir` 는 프로세스에 하나뿐이라 자원을 테스트-로컬로 만들 수 없다 —
  직렬화가 유일하다(`tasty-cli` 의 `cwd_resolve` 테스트가 `CWD_LOCK` 을 함수 끝까지 잡는다).
- `static` 전역: 그 전역을 reset/read 하는 테스트는 락을 함수 끝까지, register 만 하는
  헬퍼는 그 호출을 감싼다(`surface_registry::webview_kind` 의 `WEBVIEW_KIND_TEST_LOCK`).
- env: §2 의 RAII 가드가 같은 처방의 특수형이다(획득 시 락, Drop 시 복원).

**락은 그 락을 잡는 코드끼리만 막는다(§2 "락은 키 단위" 함정의 하위형태).** 어떤 테스트가
락을 안 잡고 같은 자원을 만지거나 — 특히 그 자원을 읽는 **프로덕션 경로**를 간접 호출하면
— 락은 아무것도 막지 못한다. 그래서 처방을 적용하기 전에 "그 자원을 만지는 테스트가
이것뿐인가" 를 먼저 전수로 확인한다. 다른 크레이트의 같은 종류 접근은 별도 테스트
바이너리(별도 프로세스)라 경합이 아니다.

### 형태 B — 프로세스 밖 OS 자원 (포트·경로·소켓 TOCTOU)

`TcpListener::bind(":0")` 로 포트를 얻고 놓았다가 다시 bind 하는 사이, 고정 이름의 임시
파일을 공유하는 사이 다른 프로세스·테스트가 끼어든다. 처방은 **자원을 놓지 않거나**(리스너를
잡은 채 검증), 놓아야 하면 **lease + 재시도**, 고정 경로면 **유니크 이름**이다.

- 포트: `tasty-ssh` 의 `reserve_local_port` 는 포트를 점유하는 리스너를 함께 반환한다 —
  리스너를 잡고 있는 한 그 포트는 이 프로세스 소유라 rebind 레이스가 없다. 프로덕션에서
  ssh 가 rebind 해야 하는 경우만 명시적으로 drop 하고, 그 창은 ready-probe 재시도가 흡수한다.

### 형태 C — 부하 의존 벽시계 마감(deadline)

공유 상태 경합이 아니라, "N 초 안에 일어난다" 를 **벽시계로 단정**하는 테스트가 러너 부하에
비례해 깨진다. 단독 실행은 빠르게 green(수십~수백 ms)인데 완주/부하에서 deadline 을 소진하고
red 다. 처방은 벽시계 폴링을 **이벤트 대기**로 바꾸는 것 — 단, 그 이벤트(waker·EOF)가 부하나
플랫폼 때문에 유실·지연될 수 있으면(Windows ConPTY 는 자식이 죽어도 read EOF 가 늦다) **제품이
이미 가진 폴 주기를 상한으로 결합**한다(이벤트 우선 + 폴백).

- 같은 프로세스 안: `process_exited_eventually_emitted` 는 `recv_timeout` 상한을 제품의
  alive-check 주기(`ALIVE_CHECK_INTERVAL`)로 잘라, waker 가 안 와도 그 주기마다 `process()` 가
  `try_wait` 폴백을 돈다. 자식 사망은 확정이므로 다음 주기에서 잡힌다.
- 별도 프로세스(자식 셸): `spawn_shell` 이 `JoinHandle` 을 반환하게 하고 테스트가 `join` 한다 —
  벽시계 없이 자식 `output()` 완료를 기다린다. 프로덕션은 그 핸들을 `let _` 로 drop 해
  fire-and-forget 을 유지한다(핸들을 버려도 스레드는 detach 되어 계속 돈다).
- **타임아웃 상향은 처방이 아니다** — 발생 빈도만 낮추고(확률 저감) 부하가 그 상한을 넘는
  날 다시 깨진다. 근거는 ADR-0129 "완화와 은폐의 경계".

### 형태 D — 사용자 홈을 기준선으로 삼는 시험

시험이 `tasty_home()` 아래의 파일을 읽는 프로덕션 경로를 간접 호출한다. 그 파일은 앞선
완주가 남긴 것이라 **기계마다 다르고**, 같은 기계에서도 어제의 실행이 오늘의 기준선을 바꾼다.
§1~4 가 *읽는 값*(설정 · env)을 갈아끼웠다면 이쪽은 **읽는 자리**가 그대로 남은 축이다.

**방향이 A·B·C 와 반대다 — 완주가 초록이고 단독이 빨갛다.** 완주에서는 같은 파일을 쓰는 다른
시험이 먼저 `save()` 로 오염을 덮어쓰므로, 문제의 시험이 읽을 때쯤 내용이 이미 깨끗하다.
그 시험만 떼어 돌리면 덮어쓸 것이 없어 오염이 그대로 읽힌다. 그래서 완주 초록은 근거가 안 된다.

처방은 직렬화가 아니라 **자원의 테스트-로컬화**다 — `tasty_utils::path::push_home_override`
(스레드 로컬, env 미조작, ADR-0155 의 처방 등급 ⓒ)로 그 시험 전용 임시 홈을 세운다.

- 본체는 `CoreState` 가 `IsolatedHome` 을 **마지막 필드**로 들고, 조립 지점
  `new_with_ids_and_settings` 가 그것을 첫 줄에서 세운다. 그래서 생성자를 무엇으로 부르든
  — 픽스처를 거치든 그 함수를 직접 부르든 — 같은 격리를 받고, 생성 이후의 `save()` 까지
  엔진 수명 내내 덮인다(마지막 필드인 것이 그 계약이다: drop 순서가 선언 순서다).
- 엔진 없이 홈을 읽는 것을 만드는 시험은 그 헬퍼가 직접 가드를 쥔다 —
  `plugin_bridge::key_dispatch` 의 `mgr()` 이 그 형태다(`PluginManager::with_registries` 가
  생성 중에 `plugins-logs` 를 만든다).

**시험마다 다른 키(surface id 등)를 고르는 완화책은 처방이 아니다.** 같은 파일을 쓰는 자리가
하나 늘 때마다 손으로 지켜야 하고, 그 규율이 지켜졌는지 재는 채널이 없다 — 어겨도 완주는
초록이라 아무 신호가 안 난다.

### 형태 판별식 — 증상 하나로는 안 갈린다

| 지문 | 형태 | 근본 처방 |
|---|---|---|
| deadline 시간을 정확히 소진한 뒤 실패("30초"/"5초"), 단독은 수백 ms | C | 벽시계→이벤트+폴 주기 폴백 |
| 즉시 실패, 단독 green·병렬 red, 락 없는 `static`/env/cwd 접근 | A | 직렬화 락 또는 자원 테스트-로컬화 |
| 즉시 실패, 고정 포트/경로/파일 이름, drop-후-rebind, Windows 파일 시맨틱 가정 | B | 놓지-않기 / lease+재시도 / unique 이름(tempdir) |
| **완주 green·단독 red**, 실패가 사용자 홈의 파일 내용에 좌우됨 | D | 홈 override 로 시험(엔진)마다 전용 홈 |

- **방향부터 본다.** "단독 green / 완주 red" 는 A·B·C 이고, **반대 방향(완주 green / 단독 red)
  이면 D 다** — 완주에서 다른 시험이 오염을 덮어쓰기 때문이라, 이 방향은 A·B·C 어느 칸도
  내지 못한다. 방향을 안 보고 완주만 돌리면 D 는 아예 증상이 없다.
- "단독 green / 완주 red" 안에서는 A·C 가 공통이라 그것만으로 안 갈린다 — **시간**을 본다:
  deadline 을 정확히 소진하면 C, 즉시 실패면 A/B(그 다음 자원 종류로 A·B 를 가른다).
- 원인을 **후보 열거로 추정하지 말고 계측으로** 가른다 — 실패 지점에 상태 플래그(예: 자식
  사망 여부 · 이벤트 방출 여부)를 심어 그 조합이 어느 칸인지로 답을 낸다. 성공 회차·실패
  회차 **두 극에 다 있는** 로그·패닉은 판별력이 0 이다(ADR-0129 triage 규칙).

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

### 부하 재현과 CI(러너) 관측

로컬 재현: CPU 를 코어 수만큼 busy loop 로 포화시킨 뒤 대상 테스트를 30 회쯤 반복해
**재현률**(N/30)로 잰다. 단독 반복 green 과 대비하면 부하 축임이 드러난다. 고치기 전/후
재현률을 두 열로 낸다(예: before 8/30 → after 0/30).

- ★ **러너가 로컬보다 형태 C 를 더 잘 드러낸다**: 어떤 형태 C 는 로컬(예: 24-core Linux)에서
  8/30 인데 Windows 러너에서는 거의 매번 났다 — cmd spawn 이 sh 보다 무거워 임계를 쉽게 넘긴다.
  그러면 **그 러너의 연속 green 이 로컬 반복보다 강한 증거**다. 로컬에서 재현이 안 되는 형태 C
  도 있다(그 러너에서만 임계를 넘음) — 그때 판정 채널은 CI 하나뿐이다.

CI 잡에서 어느 스텝·테스트가 죽는지는 잡 초록/빨강이 아니라 스텝 단위로 본다:

```bash
gh run view <run_id> --json jobs \
  -q '.jobs[]|select(.name=="<job>")|.steps[]|"\(.number) \(.conclusion)\t\(.name)"'
gh api "repos/<owner>/<repo>/actions/jobs/<job_id>/logs" \
  | grep -E '\.\.\. FAILED|test result: FAILED\.'
```

함정:

- `gh run list` 는 워크플로 결론까지만 준다 — 스텝은 위 `--json jobs` 로.
- 로그에서 `grep … | head` 는 앞부분(대개 green)만 자른다 — `test result: FAILED` 나
  `… FAILED` 를 직접 노리거나 `tail` 을 쓴다.
- 스텝 번호가 건너뛰면(5 다음 10) 그 사이는 **안 돈 것**이다 — 미실행을 성공으로 세지 않는다.
- 오래된 run 은 로그가 만료돼 상세 회수가 안 된다 — 그때는 소스 정적 대조로 좁힌다.
- **빨간 잡은 그 아래를 가린다**: 앞 스텝이 죽으면 뒤 스텝은 안 돌고, 한 스텝 안에서도 상시
  red 가 그 밑의 간헐 flake 를 가린다(`--no-fail-fast` 라도 간헐분은 회차마다 다르게 뜬다).
  그래서 위 겹을 걷어내야 아래가 드러나며, **"초록 1 회"는 "고쳤다"가 아니라 "이번엔 안 났다"
  일 수 있다** — 근본 수정인지 간헐인지는 **연속 관측**과 소스(그 사이 그 파일이 실제로
  바뀌었나: `git log <base>..<tip> -- <path>`)로 가른다.

### 가드가 실제로 도는지는 "이름" 으로, 스캔은 "집합 동등" 으로 확인한다

재진입을 막는 소스 스캔 가드는 자기 모수를 함께 낸다. 스캔한 파일 수를 **하한**으로만
두면 하한 위로 빠지는 부분 누락을 못 잡는다 — **스캔 모집단을 집합으로 못박아** 추가·삭제를
양방향으로 잡는다(하한 < 정확 건수 < 집합 동등).

가드가 두 CI 채널(check-windows=기본, check-headless=`--no-default-features`)에서 **실제로
실행되는지**는 잡 초록이 아니라 두 조합의 실행 목록에 그 가드 이름이 뜨는지로 본다:

```bash
cargo test --workspace --lib --bins --locked -- --list | grep <가드 이름>
cargo test --workspace --lib --bins --no-default-features --locked -- --list | grep <가드 이름>
```

둘 다에서 이름이 나와야 두 채널 모두 그 가드를 본다(§6 cfg 게이팅 함정의 실측판). 한쪽에만
뜨면 반대 채널은 그 가드를 영영 못 본다. `--list` 의 stdout 에는 타깃 경계가 없어 workspace
목록은 동명 타깃이 dedup 된다 — 타깃별로 가르려면 `2>&1` 로 stderr 의 `Running` 줄을 함께
받는다. 그리고 검사가 "위반 0" 을 냈을 때는 그것이 "위반 없음" 인지 "그 검사가 대상을 스캔
범위에 안 넣어 아무것도 안 본 것" 인지 — 검사가 실제로 본 모수를 함께 확인해 가른다.

## 8. 공유 픽스처 `test_state()` 는 **진짜 프로세스를 띄운다**

`src/state/tests.rs` 의 `test_state()` / `test_state_with_memory()` 는 유닛 테스트가
`AppState` + `CoreState` 한 쌍을 얻는 표준 통로다. 그 안에서 `CoreState::new` 이 도는데,
이 생성자는 **기본 워크스페이스를 만들면서 실제 PTY 를 열고 실제 셸을 fork 한다**
(`Pane::spawn_terminal` → `tasty_terminal::Terminal::new` → `portable_pty` →
`std::process::Command::spawn`).

그러니 이 픽스처를 쓰면 그 시험은 **파일 몇 개를 읽는 시험이 아니라 프로세스를 하나
띄우는 시험**이다. 따라오는 것:

- 자식 셸 프로세스 하나와 그 PTY(master `/dev/ptmx` + slave `/dev/pts/N`).
- PTY 마다 exit-watcher OS 스레드 하나(`src/core/pty_registry.rs`).
- `std::process::Command::spawn` 이 exec 결과를 부모에게 알리려고 내부에서 만드는
  AF_UNIX SEQPACKET socketpair 한 쌍. **이것은 우리 코드의 채널이 아니다** — 그래서
  "socketpair 한 번 = 자식 프로세스 spawn 한 번" 이라는 등식이 성립하고, 아래 명령이
  spawn 횟수를 그대로 센다.

### 몇 번 띄우는지는 이렇게 센다

```bash
cargo test -p tasty --lib --no-run                    # 테스트 바이너리 경로를 찍는다
strace -f -e trace=socketpair -o /tmp/sp.txt <그 경로>
grep -c 'socketpair(AF_UNIX' /tmp/sp.txt
```

수를 여기 적지 않는다([ADR-0139](../adr/0139-numbers-in-docs-are-classified-by-lineage-not-by-name.md))
— 시험이 늘면 같이 는다. 이 수의 성질만 적어 둘 값이 있다: **병렬도에 안 움직인다.**
`--test-threads` 를 바꿔도 호출 총수는 같다(바뀌는 것은 동시에 살아 있는 수뿐이다).
그래서 이 한 수는 "얼마나 많이 띄우는가" 만 재고 "얼마나 겹치는가" 에 오염되지 않는다.

#### 안 쟀다 — 그 총수의 **귀속**, 그리고 재려면 무엇이 필요한가

총수는 위 명령이 답하지만 **어느 시험이 그중 몇을 띄우는지**는 안 쟀다. 호출 스택이 필요한데
`strace -k` 는 이 환경의 빌드에 없다. 다만 계기를 바꾸지 않고도 갈 길이 있다 — 위 성질(총수가
병렬도에 안 움직인다)에서 **집합을 쪼개 각각 세면 합이 총수와 같다**가 따라온다. 그러니 필요한
것은 새 계기가 아니라 **분할**이다: 테스트 이름을 모듈 접두로 나눠 같은 명령을 부분집합마다
돌리고, 부분의 합이 총수와 맞는지로 분할이 샜는지 확인한다. 스택이 꼭 필요하면 libunwind 를
붙인 `strace -k` 빌드나 gdb `catch syscall` + backtrace 가 대안이다.

분할의 모양은 이렇다 — 필터 하나가 부분집합 하나이고, 부분마다 위 명령을 그대로 돌린다.

```bash
BIN=<위 --no-run 이 찍은 경로>
"$BIN" --list | sed -n 's/: test$//p' | cut -d: -f1 | sort -u   # 모듈 접두 목록
# 접두마다:
strace -f -e trace=socketpair -o /tmp/sp-<접두>.txt "$BIN" <접두>
grep -c 'socketpair(AF_UNIX' /tmp/sp-<접두>.txt
```

**합이 총수와 다르면 분할이 샌 것이다** — 접두가 겹치거나(같은 시험을 두 번 셌다) 어느 접두에도
안 걸린 시험이 있다. 그때는 귀속을 읽지 말고 분할부터 고친다.

#### 안 쟀다 — **동시에** 살아 있는 fd 의 곡선

위 총수와 **다른 물음**이다(저쪽은 "몇 번", 이쪽은 "몇 겹"). 병렬도를 올릴 때 동시 생존 fd 가
어디서 꺾이는지는 안 쟀다. 재려면 둘이 필요하다.

- `--test-threads` 를 1·2·4·8·… 로 올리며 같은 표본을 돌려 **최댓값 곡선**을 얻는다. 한 점만
  재면 꺾임이 안 보인다.
- 꺾이는 지점에서 fd 를 **종류별**(pts · socket · pipe)로 나눠 센다. 어느 자원이 상한을
  만드는지는 총수가 아니라 그 나눔이 답한다.

★ 곡선이 평평해 보이면 그것을 결론으로 쓰기 전에 계기부터 의심한다 — 폴링은 수명이 짧은 fd 를
놓친다. `strace -e trace=openat,close` 로 열림·닫힘을 시간축에 붙여 재구성하면 폴링이 놓친
것이 있는지 갈린다.

### 그 spawn 을 건너뛰는 길

생성자 안에는 있다 — `pending_layout_restore` 가 차 있으면 기본 워크스페이스를 안 만들고,
따라서 셸도 안 띄운다. 그 자리를 채우는 것은 `restore_layout` 설정이 켜져 있고 `layout_slot`
이 실제로 읽히는 경우뿐이다.

**그러나 테스트에서 닿는 길은 아니다.** `CoreState::new` 은 `layout_slot` 에 `None` 을
넘기므로 그 가지가 아예 안 돈다. 지금 유닛 테스트가 이 spawn 을 피하는 수단은 **없다** —
`test_state()` 를 안 쓰는 것 말고는.

### 왜 이것이 격리 문서에 있나

§7 형태 B(프로세스 밖 OS 자원)의 모집단이 눈에 보이는 것보다 넓기 때문이다. PTY·자식
프로세스를 다루는 시험만 그 자원을 잡는 것이 아니라, **이 픽스처를 쓰는 모든 시험**이 잡는다.
어떤 시험이 그 자원을 만지는지 이름으로 짐작하면 틀린다 — `test_state()` 를 부르는지로 본다.
