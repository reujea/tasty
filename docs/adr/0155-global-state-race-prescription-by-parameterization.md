# ADR-0155: 전역 상태 경합 flake 의 두 갈래와 처방을 "인자화됐는가" 에 건다

- **Status**: Accepted
- **Date**: 2026-09-05
- **Tags**: testing, flaky-tests, concurrency, test-isolation, env, adr-0129
- **Group**: testing

## Context

"프로세스/인스턴스 전역 상태를 여러 테스트가 병렬로 밟아 깨진다" 는 flake 가 한 라운드에
세 층에서 관측됐다.

- **프로세스 전역 env var** — `TASTY_HOME`/`HOME` 을 테스트가 `set_var` 로 바꾸는 곳
  (`tasty-settings` 셸 통합, `tasty-host-plugin` 홈 격리 test_support).
- **바이너리 내 static 레지스트리** — `webview_kind` 의 `WEBVIEW_KINDS`
  (`static RwLock<Option<HashSet<String>>>`) 를 두 테스트가 `reset_for_test()` 로 비운다.
- **공유 tasty 인스턴스의 plugin 등록 상태** — e2e 공유 하네스에서 plugin 등록이 완료되기
  전에 요청이 나가는 경합.

셋 다 "전역 상태 경합" 이라는 한 단어로 뭉칠 수 있으나 **처방이 같지 않다.** 한 단어로
뭉치면 "전역이니 락 걸면 된다" 로 가는데, 그게 틀리는 자리가 있다. [ADR-0129](0129-flaky-test-classes-and-standard-fixes.md)
가 flake 를 부류 A/B/C 로 갈랐다면, 이 ADR 은 그중 A(인-프로세스 공유 상태) 안에서 **처방
등급이 갈리는 축**을 확정한다.

## Decision

**전역 상태 경합은 두 갈래로 가르고, 처방은 crate 이름이 아니라 "프로덕션이 이미 그 상태를
인자로 받는가" 에 건다.**

### 두 갈래

- **(가) 테스트 전용 전역 + `cfg(test)` 변경자** — 그 전역을 바꾸는 코드가 `#[cfg(test)]`
  뿐이라 프로덕션은 안 건드린다. 테스트끼리만 경합한다.
  → **모듈 전용 락으로 충분.** 예: `webview_kind`(`WEBVIEW_KIND_TEST_LOCK`, `reset_for_test`
  는 `cfg(test)`).
- **(나) 프로덕션도 읽는 OS/프로세스 전역 + 비원자 변경** — `set_var` 처럼 프로덕션 읽기
  (`var_os`)와 UB 를 이루는 전역. glibc `environ` 은 `set_var` 시 배열을 realloc 하고, 동시
  `var_os` 는 use-after-free 다(edition 2024 가 `set_var`/`remove_var` 를 `unsafe` 로 표시한
  이유). → **락으로 불충분.** 락 밖 읽기 테스트가 피해자이기 때문이다.

### 처방 등급 ("안 깨진다" 는 확률이 아니라 불변식이라야 말할 수 있다)

- **ⓐ 전역 뮤텍스로 쓰기 테스트 직렬화** — (가)엔 충분. (나)엔 **불충분**: 읽기까지 다 락을
  잡게 하려면 수십 자리에 락을 넣고 사람이 계속 기억해야 한다 = 확률 저감. "덜 깨진다" 만
  말할 수 있고 "안 깨진다" 는 못 말한다.
- **ⓑ 프로덕션이 env 를 안 읽고 홈 루트를 인자(`dir`)로 받게** — env 미접촉 → 경합 원천
  소멸(구조적 불변식). 근본이다. 비용은 자리마다 다르다(아래 ★).
- **ⓒ 쓰기 테스트를 별 바이너리(`tests/*.rs`)로 분리** — 그 바이너리에 `set_var` 가 0 이면
  같은 프로세스의 write·read 동시성이 구조적으로 불가 → 불변식. 내부 단위 테스트를 옮기려면
  pub 노출 비용이 든다.

(나)에서 ⓐ 는 부적합하다(불변식을 못 준다). ⓑ/ⓒ 가 불변식을 준다. 둘 중 선택은 캡슐화와
비용으로 한다.

### ★ 처방을 crate 가 아니라 "인자화됐는가" 에 건다

crate 마다 우월한 처방이 다르지만, 갈림의 진짜 축은 crate 이름이 아니다.

- **프로덕션이 이미 `_in(dir)` 로 인자화돼 있으면 → ⓑ-완성이 소수술.** 테스트를 `_in` 으로
  돌리면 env 미접촉이 된다. 실례 `tasty-settings`: `bash_rcfile_args_in`/`zsh_shell_envs_in`/
  `tasty_bashrc_default_path_in` 등 pure 본문이 이미 있고 pub wrapper 만 `tasty_dir()` 를
  읽었다. wrapper 는 그대로 두고 테스트만 `_in(Some(dir))` 로 돌렸다.
- **프로덕션이 `tasty_home()` 을 직접 읽으면 → ⓑ 는 대수술.** ⓒ(이동) 또는 thread-local
  override 를 비교해야 한다. 실례 `tasty-host-plugin`: `known_plugins.rs`·`discovery.rs`·
  `registry_state.rs` 가 `tasty_utils::path::tasty_home()` 을 직접 읽는다 — 인자화하려면
  호출 사슬 전체를 바꿔야 한다.

**다음 사람 판정 기준: "이 자리의 프로덕션이 홈 루트를 인자로 받는가" 를 먼저 봐라.** 받으면
ⓑ-완성, 안 받으면 인자화 비용(몇 곳인가)과 ⓒ/thread-local 을 비교하라. "전역 경합이니 락"
도, "전역 경합이니 이동" 도 성급하다.

### 한 crate 안에서도 처방이 섞일 수 있다 (settings 실측)

`tasty-settings` 는 순수 ⓑ 가 아니라 **ⓑ-완성(다수) + ⓐ-락(소수) 혼합**이다. 셸 통합 경로를
계산하는 대다수 테스트는 `_in(dir)` 로 격리했지만, **상대 `TASTY_HOME` 의 상대→절대 해석 자체가
검증 대상인** 3 개 테스트(`relative_tasty_home_is_absolutized_for_child_processes`,
`bash_rcfile_args_uses_the_resolved_root`, `effective_shell_envs_uses_the_resolved_root`)는 env
설정이 곧 피험자라 `set_var` 를 뺄 수 없다 — 이들만 `set_var` + 직렬화 락을 유지한다. 즉
**처방은 crate 단위가 아니라 "그 테스트에서 env 가 도구인가 피험자인가" 로도 한 번 더 갈린다.**

### host-plugin 실측 — (C) thread-local override, 같은 혼합이 반복된다

`tasty-host-plugin` 은 프로덕션이 `tasty_home()` 을 **직접** 읽으므로(`known_plugins`/
`discovery`/`registry_state`/`lifecycle`/`process`) ⓑ-완성이 대수술이다(dir 인자를 약 40 개
함수 시그니처로 전파, crate 경계를 넘어 앱 boot 층까지). 대신 ⓒ 계열의 **thread-local
override** 를 골랐다: `tasty_utils::path::tasty_home()` 이 이 스레드의 override 를 env 보다
먼저 보고, 테스트는 env 를 만지지 않는다. 고른 근거는 **측정**이다 — 그 방식의 이론적 구멍
("thread-local 은 자식 스레드에 상속되지 않는다")이 이 스위트에서 실제로 걸리는 자리가 **0**
이다(홈을 읽는 테스트는 전부 메인 스레드, 자식 스레드 spawn 테스트는 락·소켓·프로세스만
건드림). 그 구멍 0 을 (B)의 40 과 **같은 단위(자리 수)** 로 놓아 처방을 골랐다.

여기서도 도구/피험자 구분이 적중한다 — 홈 가드를 쓰는 자리 6 중 **도구 5 / 피험자 1**. 피험자 1 은
`HomeEnvGuard::derived_from_home` 의 파생 검증(`test_support` 의 `guard_isolates…` unix 블록)
으로, `TASTY_HOME` 을 비웠을 때 `tasty_home()` 이 `$HOME/.tasty{-debug}` 로 파생되는지를 본다
→ env 가 피험자라 `set_var(HOME)` 를 뺄 수 없다. **★ 그 피험자 1 조차 host-plugin 관심사가
아니다** — 검증 대상이 `tasty_utils::path` 의 폴백 규칙이라, SoT 로 보면 자리가 틀렸다(도구 /
피험자-여기 / **피험자-딴 데** 의 세 번째 갈래). Q1 에서 이관 대신 잔류를 골랐다: 이관하면
tasty-utils 자체의 테스트 격리 처방(아직 미결)을 이 축이 부작용으로 확정하게 되기 때문이다.

## Consequences

- **얻은 것**: (나)에 대해 "안 깨진다" 를 불변식으로 말할 수 있게 됐다. `tasty-settings` 의
  셸 통합 테스트는 env 를 아예 만지지 않으므로 다른 완주·다른 테스트의 `set_var` 와 경합할
  수 없다. 다음 사람이 처방을 고를 결정 규칙(인자화 여부)이 생겼다.
- **잃은 것**: ⓑ-완성은 프로덕션에 pure 본문(`_in`)을 요구한다 — 없으면 만드는 비용이 든다.
  ⓒ 는 내부 단위 테스트를 통합 테스트로 옮기며 pub 노출을 강제한다.
- **운영 비용 / 유지 부담**: (나)의 불변식은 **결정론적 소스스캔**으로 지킨다("이 바이너리에
  `set_var` 0" / "이 전역은 이 모듈에서만 변경"). 부하 의존 flake 는 초록 N 회로 "고쳤다" 를
  증명하지 못하므로(부하가 낮으면 원래 안 난다) 초록 횟수를 회귀 가드로 쓰지 않는다. 합성
  픽스처(2 스레드 write/read 경합)는 "레이스 실재" 를 보이는 데모용이며 그 자체가 확률적이라
  회귀 가드로는 부적합하다.

### 소스스캔 가드의 함정 (가드 이름 ≠ 가드 술어)

`tasty-host-plugin` 의 `home_env_is_only_touched_through_this_module` 는 이름과 달리 **쓰기
유일 모듈** 만 고정하고 **읽기는 못 막는다.** (나)의 피해자는 읽기 쪽이므로 이 가드가 초록이어도
읽기-쓰기 경합은 안 잡힌다. 가드가 재는 술어를 이름과 일치시키고, 초록이 무엇을 뜻하는지(같은
바이너리 write·read 동시성 불가)와 뜻하지 않는 것(프로덕션 경로 UB, 다른 env 변수, 다른 crate)을
doc 에 명시한다. `tasty-settings` 는 불변식이 "`set_var` 0" 이 아니라 "env-피험자 3 곳만" 이라
0-상한 소스스캔 가드가 성립하지 않는다 — 그래서 가드 대신 pure 본문 옆 주석으로 "왜 이 3 곳만
env 를 만지는가" 를 박았다(가드를 못 만드는 게 아니라, 부분 허용 불변식엔 allowlist 가드의
유지비가 값을 넘는다는 판단).

## Alternatives Considered

- **모두 ⓐ(락) 로 통일** — (나)에서 불변식을 못 준다. 락은 쓰기만 직렬화하고 읽기는 lock-free
  로 남아, 프로덕션 UB 경로와 락 밖 읽기 테스트를 보호하지 못한다. 기각.
- **모두 ⓒ(별 바이너리로 이동) 로 통일** — 이미 `_in(dir)` 이 있는 crate(settings)에서는
  불필요하게 pub 표면을 늘리고 내부 단위 테스트를 통합 테스트로 강등한다. 인자화된 자리엔
  ⓑ-완성이 더 싸다. 기각.
- **crate 이름으로 처방 배정** — "settings 는 ⓑ, host-plugin 은 ⓒ" 식 매핑은 실제 갈림축
  (인자화 여부)을 가린다. host-plugin 안에서도 dir 을 받는 자리는 ⓑ 가 낫고, settings 안에서도
  env-피험자 테스트는 ⓐ 가 유일해였다. 처방은 crate 가 아니라 자리(인자화·도구/피험자)로 건다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

- ⓑ-완성을 host-plugin 프로덕션 전반(`tasty_home()` 직접 읽기)으로 넓혀야 할 이유가 생길 때
  — 예: 핫 경로에서 실제 UB 관측, 또는 파일 IO 인자화가 다른 요구로 필요해질 때.
- e2e 공유 하네스의 plugin 등록 경합에 대한 처방이 확정될 때(현재 원인만 확정, 처방 미착수).
- `set_var` 없이도 env-피험자 테스트를 결정론적으로 검증할 수단(예: 프로세스 격리 실행기)이
  생겨 settings 의 ⓐ-락 3 곳을 제거할 수 있게 될 때.
- **host-plugin 의 (C) 전제(구멍 0)가 깨질 때** — 프로덕션이 `spawn` 한 자식 스레드 본문에서
  `tasty_home()` 을 읽기 시작하면 그 스레드는 override 를 상속받지 못해 실제 홈/env 로
  폴백한다. 그러면 (C) 를 고른 근거가 사라지므로 처방 선택 자체를 다시 연다. **이 트리거는
  가드로 코드화돼 있다** — `tasty-host-plugin` 의 `test_support::tests::
  spawned_thread_bodies_do_not_read_tasty_home` 소스 스캔이 `spawn(` 클로저 본문의 `tasty_home`
  **직접** 호출을 잡고, 실패 메시지가 이 ADR 을 다시 열라고 말한다(705 식 전제-가드). 다만
  경유 함수를 통한 **간접** 호출은 그 근사가 못 잡으므로, 간접 경로가 자식 스레드로 들어오는
  변경은 이 문장 트리거로 함께 본다.
- **tasty-utils 가 자체 테스트 격리를 갖출 때** — `guard_isolates…` 의 파생 검증 1 곳은 사실
  `tasty_utils::path` 의 폴백 규칙을 검증하는 것이라, tasty-utils 가 자체 격리(그 crate 의
  ⓐ/ⓑ/ⓒ 처방은 현재 미결)를 갖추면 그리로 이관해야 한다. **이 트리거는 가드로 만들기
  어려워 문장으로 둔다** — 외부 crate 의 테스트 인프라 존재 여부는 host-plugin 소스 스캔으로
  관측되지 않기 때문이다.

## References

- [ADR-0129](0129-flaky-test-classes-and-standard-fixes.md) — flake 부류 A/B/C 와 표준 처방
  (이 ADR 은 그 A 안의 처방 등급 축을 확정).
- [ci-gates](../dev-guide/ci-gates.md) — 자동 채널(check-windows / check-headless)에 flake 가
  상주하면 이후 작업이 진짜 회귀를 "또 그 flake" 로 넘기는 문제.
- 소스: `crates/tasty-settings/src/general.rs`(ⓑ-완성 + ⓐ-락 혼합),
  `src/core/surface_registry/webview_kind.rs`(가 갈래 = 락으로 충분),
  `crates/tasty-host-plugin/src/{known_plugins.rs,discovery.rs,registry_state.rs}`
  (`tasty_home()` 직접 읽기 = ⓑ 대수술이라 (C) 를 골랐다).
- (C) 구현: `crates/tasty-utils/src/path.rs`(feature-gated thread-local override 훅
  `push_home_override`/`pop_home_override`, `test-support` feature),
  `crates/tasty-host-plugin/src/test_support.rs`(`HomeEnvGuard` 의 두 갈래 = override 도구 /
  `set_var` 피험자, 전제 가드 `spawned_thread_bodies_do_not_read_tasty_home`).
