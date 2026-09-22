# E2E 테스트 — 인스턴스 공유 + 격리 + timeout 정책

`tests/e2e_tests.rs` 를 비롯한 e2e 테스트는 실 tasty 바이너리를 spawn 하여 IPC 로 조작한다. `tests/common/mod.rs` 가 공통 하네스이며 진입점이 둘이다 — 공유 인스턴스 `common::shared()`(기본)와 전용 인스턴스 `TastyInstance::spawn`(예외). 자체 검증 절차는 [self-verification.md](self-verification.md), debug 전용 IPC 는 [debug-ipc.md](debug-ipc.md).

## 0. 전제: plugin 바이너리 최신화 (필수)

**e2e 는 `cargo test --test e2e_tests` 단독 실행 전에 `cargo build --workspace` 가 선행돼야 한다** (또는 처음부터 `cargo test --workspace` 사용). package 한정 test 는 본체(tasty.exe)만 빌드하고 plugin bin crate 들을 빌드하지 않는데, dev bundle(`target/debug/builtin-plugins/`)은 **매니페스트는 소스에서, 바이너리는 target exe 에서 독립적으로** `copy_if_newer` 하므로 stale plugin exe 가 최신 매니페스트를 달고 격리 TASTY_HOME 에 설치된다. 이 drift 는 plugin↔host 계약이 바뀐 직후(예: `markdown.recent` 의 host adapter 이관) namespace 호출을 "Method not found" 로 깨뜨린다. 호스트는 hello 를 매니페스트와 두 축으로 대조해, 바이너리 보고 버전 ≠ 매니페스트 버전이면 `version drift` warn 을, hello 가 주장한 `plugin_id` ≠ 매니페스트 id 면 `identity drift` warn 을 남긴다 — spawn 실패 진단 시 stderr tail 에서 이 두 경고를 먼저 확인.

**이 전제는 이제 글만이 아니라 판정이다.** `spawn_diag::staged_bundle_note` 가 번들 plugin 을 부른다고 선언한 스위트에서 exe 옆의 plugin 바이너리를 세고, 0 이면 **실패 문구 끝에** 무엇을 지을지 붙인다. 그 전에는 같은 상태가 `Method not found` 한 줄로만 나왔고 — 그 문구는 메서드가 사라진 것과 글자 그대로 같아 — 같은 빨강이 회귀로 오독됐다. 이 전제가 이 문서 · `CLAUDE.md` · `spawn_diag::instance_bin` 의 doc 세 곳에 **글로는 이미 적혀 있었다는 것**이 요점이다: 실패하는 순간에 읽히지 않는 글은 판정이 아니다. 그러니 같은 사실을 네 번째 자리에 또 적지 말고, 안 잡히는 갈래가 보이면 그 판정을 넓혀라.

진단은 **spawn 이 아니라 실패 자리**에 붙는다. spawn 에서 세우면 plugin 을 안 쓰는 스위트의 참인 초록까지 같이 사라진다. 부분 스테이징(일부만 지어진 상태)은 안 잡는 **선언된 사각**이고, 그 이유는 그 함수의 doc 에 있다.

## 0-1. 어느 바이너리를 띄우는가

하네스가 띄우는 것은 `CARGO_BIN_EXE_tasty` — **테스트 자신과 같은 feature 로 빌드된 자기 바이너리**다. 이 한 줄이 조합별 성질을 전부 결정한다.

| 조합 | 하네스가 띄우는 것 | 성질 |
|---|---|---|
| 기본 (`gui`) | GUI 바이너리 | 창 + wgpu 디바이스를 반드시 만든다. IPC 는 GPU 부팅이 끝난 뒤에야 시작되므로, GPU 를 못 잡으면 **port file 이 아예 안 써진다**. 그 창이 **어디에** 뜨는지는 `TASTY_E2E_DISPLAY` 가 정한다 — §3 |
| `--no-default-features` | headless 데몬 | 창도 GPU 도 없다. 실측(2026-09-04, `DISPLAY`·`WAYLAND_DISPLAY` 둘 다 없는 상태): port file 까지 **54 ms** |

즉 IPC 만 쓰는 스위트가 GPU 를 통과해야 하는 이유는 검증 내용이 아니라 **빌드 조합**에 있다. 방향 결정과 대안은 [ADR-0127](../adr/0127-e2e-harness-binary-selection.md).

**바이너리 선택은 `spawn_diag::instance_bin()` 한 곳에서 한다.** 두 하네스(`tests/common`·`tests/webhook_common`)와 웹훅 CLI 러너가 모두 이 함수를 거친다 — 하네스마다 다른 바이너리를 고르면 같은 완주 안에서 클라이언트와 서버가 다른 빌드가 될 수 있다.

**헤드리스 데몬은 `scripts/build-e2e-headless.sh` 가 짓는다**(`just e2e-headless-bin`). 빌드가 실패하거나 결과가 낡았으면 **아무것도 내지 않는다** — 호출자가 그때 `TASTY_E2E_BIN` 을 안 넘기면 하네스는 오늘 동작으로 떨어진다. 배선이 틀려도 초록이 거짓이 되지 않는 방향이다. 왜 별도 빌드여야 하는지는 그 스크립트 머리에 적혀 있다: 루트 `tasty` 에는 lib 타깃이 없고(바이너리 전용 패키지다) `gui` 는 **패키지 단위** feature 라, `[[bin]]` 을 하나 더 넣어 그 타깃만 `gui` 를 끄는 것은 cargo 에서 **불가능**하다.

**낡은 override 는 하네스가 거절한다.** `spawn_diag::source_newer_than` 이 `src/`·`crates/` 의 `.rs` 중 그 바이너리보다 새것이거나 **mtime 이 같은** 것이 있으면 패닉으로 세운다 — 낡은 데몬은 정상 부팅해 정상 응답하므로, 안 잡으면 스위트가 **옛 코드에 대해** 판정하고 그 오진은 양방향이다.

**동률을 낡음으로 세는 이유, 그리고 그 판정이 두 층에 있다는 것.** 소스 mtime 이 바이너리와 같은 눈금에 떨어지면 어느 쪽이 먼저인지 파일시스템이 답을 안 준다. 그 **판정 불가**를 "안 낡았다" 로 흡수하면 위 문단이 막으려는 것이 그대로 통과하므로, 판정 불가는 실패 방향으로 보낸다(반대 비용은 다시 빌드 한 번이고, 패닉 문구가 끄는 법까지 알려 준다). ★ 같은 판정이 `scripts/build-e2e-headless.sh` 에도 있다 — 그쪽이 **먼저** 걸러 패닉까지 안 가게 하는 인체공학이고, 하네스가 그 스크립트를 받쳐 주는 관계가 아니다. **두 층이 같은 mtime 을 보므로 한쪽만 고치면 같은 사각이 남는다**: 두 곳의 극성은 함께 움직여야 한다. mtime 을 못 읽는 파일은 두 층 모두 못 본다 — 그것은 선언된 거짓 음성이고, 받쳐 주는 층이 없다.

**그 안에서 판정은 스위트 단위다** — `spawn_diag::daemon_kind()`. 인스턴스를 띄우는 스위트 중 **조합 의존 단언을 가진 것만** 자기 조합의 데몬을 요구하고(`DaemonKind::SameCombo`), 나머지는 헤드리스 데몬으로 충분하다(`HeadlessOk`). 조합 의존은 테스트 쪽 `cfg(feature = "gui")` 로만 드러나지 않는다 — 단언은 하나인데 **데몬 쪽**이 조합마다 다른 호출측을 재는 스위트도 `SameCombo` 다. 헤드리스 데몬을 받으면 초록은 그대로인데 gui 호출측을 안 잰다. 지금 그 형태는 `attach_structure_sync_loopback` 하나다(forward 회신을 gui · 헤드리스 데몬이 서로 다른 함수로 만든다 — [ADR-0170](../adr/0170-e2e-daemon-is-chosen-per-suite-and-off-by-default.md) 보강). 명부는 `HEADLESS_OK_SUITES` 한 곳이고, 그것이 `EXPECTED_INSTANCE_TESTS` 와 갈리지 않는 것은 `tests/e2e_single_instance_guard.rs` 가 **양방향으로** 본다(분류 안 된 스위트 / 명부에만 있는 이름). 분류를 안 하면 안전한 쪽인 `SameCombo` 로 떨어진다 — 놓치면 최적화를 잃을 뿐 틀린 빨강은 안 난다.

**로컬 탈출구 — `TASTY_E2E_BIN`.** 워크트리 여러 개가 같은 GPU 를 다투는 상황에서 IPC 전용 스위트를 GPU 밖으로 뺄 수 있다. 미리 빌드해 둔 headless 바이너리의 경로를 주면 하네스가 그것을 띄운다.

**그 "다툰다" 가 무엇을 굶기는지 쟀다 (2026-09-06).** 공유 인스턴스에 `surface.list` 를
50 ms 마다 던져 **한 왕복의 최대 지연**을 관측량으로 삼고, 조건을 하나씩 지웠다.

| 조건 | load | 최악 왕복 | 1 초 초과 |
|---|---|---|---|
| gui, 추가 부하 없음 | 16 | 199 ms | 0 |
| gui, 같은 디스플레이에 인스턴스 18 | 39 | **5.79 s** | 1 |
| **순수 CPU 부하만** (인스턴스는 안 늘림) | 33 | 372 ms | 0 |
| 부하는 공유 디스플레이, **탐침만 전용 Xvfb** | 35 | 525 ms | 0 |
| **부하도 탐침도 같은 Xvfb 한 대** | 27 | 648 ms | 0 |

읽는 법: CPU 총량이 아니고(3 행), 디스플레이를 **공유해서** 도 아니다(5 행 — 한 Xvfb 에
다 몰아도 정체가 없다). 정체는 **실제 GPU·컴포지터가 붙은 X 서버**에서만 났고 그때 그
서버는 한 코어를 채우고 있었다. 그러니 이 탈출구의 값은 "GPU 를 안 쓴다" 가 맞다.
**단, 이 팔들은 "실제 X 서버" 와 "하드웨어 GPU 경로" 를 못 가른다** — Xvfb 는 서버뿐
아니라 렌더러도 소프트웨어라, 가르려면 두 번째 하드웨어 X 가 필요하다(안 쟀다).

**★ 탈출구가 만병통치는 아니다.** 같은 날 헤드리스 조합끼리(X 가 아예 없음) 12 러너로
부하를 준 팔에서는 `surface.list` 한 번이 **30 초 넘게** 답을 못 받았다(하네스 IPC 는
10 초 read timeout 을 3 회 시도하고 셋을 다 태웠다 — `Resource temporarily unavailable`).
그때 load 는 19~24 로 위 3 행(33)보다 **낮았다.** 즉 헤드리스에는 CPU 도 X 도 아닌
**다른 굶음**이 있고 그 기전은 아직 미측정이다. GPU 밖으로 뺐다고 부하 flake 가 사라진다고
읽지 마라.

```
CARGO_TARGET_DIR=target-e2e-headless cargo build --no-default-features --workspace
TASTY_E2E_BIN=$PWD/target-e2e-headless/debug/tasty cargo test --test shared_instance_harness
```

**두 조건이 각각 필요하다 — 어느 하나만 빠져도 데몬이 plugin 없이 뜬다.** 아래 함정 1·2 가 각각을 설명하고, 표는 그 둘을 따로 잰 것이다.

**함정 1 — 반드시 별도 `CARGO_TARGET_DIR` 로 빌드해라.** `CARGO_BIN_EXE_tasty` 와 headless 빌드는 `target/debug/tasty` 라는 **같은 경로**를 다툰다. 같은 target 디렉토리에 headless 를 빌드하면 다음 `cargo test` 가 그것을 gui 로 덮어써서, 아무것도 바뀌지 않았는데 override 가 듣는 것처럼 보인다. 어느 바이너리를 띄우는지를 **경로로 확정하지 않으면 검증이 조용히 다른 것을 잰다** — §0 의 stale plugin 바이너리와 같은 계열의 함정이다. 존재하지 않는 경로를 주면 하네스가 그 자리에서 실패한다(30 초를 기다린 뒤 port file 미작성으로 오진되지 않는다).

**함정 2 — target 디렉토리를 레포 밖에 두면 데몬이 plugin 없이 뜬다.** host 는 plugin 번들을 `exe_dir/builtin-plugins` 에서 찾고, 거기 없으면 **exe 의 두 단계 위를 워크스페이스 루트로 역산**해 dev bundle 을 만든다(`crates/tasty-host-plugin/src/builtin.rs` 의 `ensure_dev_bundle`). `/tmp/tasty-headless/debug/tasty` 면 역산 결과가 `/tmp` 라 `crates/` 가 없고, 동기화가 전부 false 로 떨어져 데몬이 plugin namespace 하나 없이 올라온다. 그러면 namespace 호출이 `Method not found: markdown.recent` 로 죽는다 — **§0 의 stale plugin drift, 그리고 바로 아래 "override 로도 통과하지 않는 스위트" 와 증상이 글자까지 같다.** 즉 빌드 절차 결함이 headless IPC 표면의 차이로 읽힌다. target 을 레포 안(`target/` 의 형제)에 두면 역산이 레포 루트를 맞혀 번들이 **스스로** 만들어진다.

**함정 3 — `--workspace` 를 빼면 그 target 에 plugin 바이너리가 하나도 안 생긴다.** 이건 함정 2 와 **독립**이다. 루트 패키지만 빌드하면 `tasty-plugin-*` bin crate 들이 그 target 에 없고, `sync_builtin_dev` 는 바이너리가 없는 plugin 을 건너뛴다 — target 을 레포 안에 두어 역산이 맞아도 번들은 빈다.

실측(2026-09-05, `markdown_recent_is_read_only`, 팔마다 `builtin-plugins/` 를 지우고 시작)으로 두 축을 따로 갈랐다.

| target 위치 | `--workspace` | 그 target 의 plugin 바이너리 | 호출 후 `builtin-plugins/` | 테스트 |
|---|---|---|---|---|
| 레포 안 | 있음 | 9 | **9 항목** | **PASS** |
| 레포 밖(`/tmp`) | 있음 | 9 | 0 항목 | FAIL — `Method not found: markdown.recent` |
| 레포 안 | 없음 | **0** | 0 항목 | FAIL — 같은 문구 |

첫 두 줄은 `--workspace` 와 바이너리 수가 같고 **위치만** 다르다(함정 2). 첫 줄과 셋째 줄은 위치가 같고 `--workspace` 만 다르다(함정 3). **두 실패의 문구가 같다** — 그래서 증상만 보고 원인을 고를 수 없고, 절차 두 조건을 모두 지키는 것이 유일한 대응이다. 대조군은 CI headless 잡이다: 같은 테스트를 통과시키므로 실패는 조합이 아니라 절차 쪽에 있다.

**번들을 복사해 쓰는 변형**(`cp -r target/debug/builtin-plugins <target>/debug/`)도 동작한다 — exe 옆에 번들이 있으면 역산 분기까지 가지 않는다. 다만 복사본은 **갱신되지 않는다**: 이후 plugin 을 고쳐 다시 빌드해도 그 복사본은 그대로라 §0 의 drift 를 한 겹 더 만든다. 레포 밖 target 을 반드시 써야 할 때의 대안으로만 쓴다.

**첫 namespace 호출이 기동을 기다린다.** 헤드리스는 plugin **프로세스**를 지연 기동한다 — `forward_to_plugin_namespace`(`src/boot/headless_dispatch.rs`)가 소속을 매니페스트로 먼저 확인하고, 맞을 때만 공통 manager가 해당 owner와 필요한 활성 IPC hook extension을 준비한다([ADR-0282](../adr/0282-namespace-invocation-starts-only-its-owner-and-matching-extension.md)). 그래서 그 첫 호출은 기동 시간을 그대로 문다(실측 2026-09-05: 첫 호출 1272 ms, 기동 뒤 92 ms).

소속 판정 자체는 더 이상 기동을 기다리지 않으므로, 예전에 이 자리에 적혀 있던 "첫 호출이 드물게 `Method not found` 로 답한다"(namespace 표가 hello 뒤에 채워져서 기동과 첫 조회가 겹치던 형태)는 그 원인이 사라졌다. 여전히 `-32601` 이 나오면 그건 이름이나 설치를 의심할 신호다 — 설치된 owner가 비활성·자동 비활성이거나 기동에 실패하면 `-32002 plugin '<id>' is not running` 이 온다.

**이 탈출구의 바이너리를 CI 산출물과 같다고 전제하지 마라.** 위 절차는 `--workspace` 없이 빌드한다. 워크스페이스 feature 통합은 root 패키지까지 닿아서, `--workspace` 유무만 다르게 두 번 빌드하면 root 바이너리의 cksum 이 갈린다(실측). 번들을 따로 복사하므로 이 스위트들의 판정에는 영향이 없지만, **바이너리 동일성을 전제로 하는 판정**(재현 빌드 비교 등)에는 쓰지 마라.

**★ 조합 교차는 스위트 단위 판정으로 닫혔다.** `TASTY_E2E_BIN` 은 **데몬만** 다른 조합으로 바꾼다. 테스트 바이너리는 여전히 자기 조합으로 컴파일돼 있어서, 데몬의 동작을 `cfg(feature = "gui")` 로 갈라 단언하는 테스트는 그 단언이 **구조적으로 뒤집힌다** — `tests/e2e_tests.rs` 의 `..._answers_in_both_combos` 계열이 그 형태다. 그래서 **그런 단언을 가진 스위트는 override 를 받지 않는다**(`daemon_kind()`). override 를 켜도 `e2e_tests` 는 자기 조합의 데몬을 그대로 띄우고, GPU 부팅을 건너뛰는 것은 나머지 스위트다.

그 판정의 근거가 된 실측(2026-09-05, 스위트 단위 판정을 넣기 **전**, 인스턴스 11 스위트를 gui 테스트 바이너리 + headless 데몬으로): 스위트 단위 **10 / 11 통과**, `e2e_tests` 만 30 passed / 5 failed. 그 5 중 **4 건이 조합 교차**이고(`an_engine_query_that_reads_no_window_answers_in_both_combos` · `app_layer_methods_that_need_no_window_answer_in_both_combos` · `debug_surfaces_that_read_no_window_answer_in_both_combos` · `a_request_naming_an_unowned_target_is_rejected`), 나머지 1 건이 `multi_window_owner_routing`(실제 창). **앞의 4 건은 동종 조합에서는 양쪽 다 통과한다** — CI headless 잡이 `--skip` 하지 않고 통과시키는 것이 그 증거다. 즉 이 4 건은 제품 결함도 headless 미배선도 아니고 **탈출구 자신의 대가**였다 — 지금은 `e2e_tests` 가 override 를 안 받으므로 나지 않는다. `multi_window_owner_routing` 도 같은 이유로 함께 해소된다. 아래 `--skip` 목록과 혼동하지 마라: 저것은 조합과 무관하게 빠지는 것들이고, 이것은 조합을 교차시켰을 때만 생긴다.

**override 로도 통과하지 않는 스위트가 있다.** headless 데몬은 GUI 바이너리와 IPC 표면이 다르다 — 실제 창이 검증 대상인 스위트, 그리고 headless 에 아직 배선되지 않은 경로에 의존하는 스위트가 그렇다. 어느 것이 왜 빠지는지는 `.github/workflows/crossplatform-check.yml` 의 headless 스텝 주석이 `--skip` 목록과 함께 사유를 적어 둔다 — **여기 복제하지 않는다**(사유가 갈리면 어느 쪽이 정본인지 알 수 없게 된다).

## 1. 인스턴스 공유 원칙 (필수)

**tasty 인스턴스는 test binary 당 1 개가 기본이고, 격리는 프로세스가 아니라 workspace 단위로 한다.** 새 e2e 테스트를 쓸 때 인스턴스를 새로 띄우지 말고 `common::shared()` 를 받아 `create_workspace()` 로 자기 workspace 를 만들어라. 결정의 근거·대안·재검토 조건은 [ADR-0090](../adr/0090-test-isolation-by-workspace-not-process.md).

**왜 인스턴스를 아끼나**

- tasty 는 GUI 앱이라 뜰 때마다 창이 생기고 **OS 포커스를 훔친다** — 개발자가 다른 창에서 작업 중이면 테스트가 키 입력을 가로챈다.
- 창 spawn/kill 자체가 dev 프로필 기준 수 초다. `#[test]` 마다 띄우면 그 비용이 테스트 수만큼 곱해진다.

**왜 workspace 로 격리해도 되나**

- attach 점유는 `OccupancyRegistry` 의 `surface_locks` / `workspace_locks` — **workspace/surface 단위 lock** 이다([attach-behavior.md](attach-behavior.md)). 서로 다른 workspace 를 잡는 테스트들은 한 인스턴스 위에서 병렬 공존한다.
- IPC 로 만든 workspace 는 `IntentOrigin::Agent` 라 active 를 전환하지 않는다([identity.md](../identity.md) 원칙 1·3). 여러 테스트가 병렬로 만들어도 서로의 활성 상태를 흔들지 않는다.
- `workspace.create` 응답이 `id` 와 `surface_id` 를 함께 주므로 한 번의 호출로 격리 단위 전체를 얻는다.

**공유 범위는 test binary 단위다 — `cargo test` 전체가 아니다.** `OnceLock` 은 프로세스 로컬 정적 상태이고 cargo 는 test 타겟마다 별도 프로세스를 띄우므로, 이 구조로 도달 가능한 하한은 *바이너리당 1 개*다. 이걸 "저장소 전체 1 개" 로 오해하면 인스턴스 총량을 잘못 계산하게 된다. 총량을 더 줄이려면 test binary 개수 자체를 줄여야 한다.

**전용 인스턴스가 정당한 예외는 두 종류뿐이다** — 둘 다 *프로세스 경계 자체가 검증 대상*인 경우다.

| 예외 | 사례 | 이유 |
|------|------|------|
| 기동 시점 config 이 달라야 함 | `spawn_with_inherit_cwd(true)` — `tests/attach_convert_cwd_loopback.rs` · `spawn_with_env(&[("TASTY_DEBUG_IPC_ROUND_TIME_BUDGET_MS", "0")])` — `tests/e2e_tests.rs` 의 `concurrent_requests_are_all_answered_when_every_round_is_cut` | `inherit_cwd` 는 격리 HOME 의 `config.toml` 에 미리 쓰는 값이라 이미 떠 있는 인스턴스에 런타임으로 바꿔 끼울 수 없다. debug 전용 회차 시간 예산([debug-ipc.md](debug-ipc.md))도 부팅 뒤 한 번 읽는 환경 변수라 같은 이유다 — 공유 인스턴스에 켜면 다른 시험이 모두 잘린 회차 위에서 돈다 |
| 프로세스 자원을 외부에서 측정 | `tests/soak_memory.rs` | 프로세스 트리 RSS 를 `pid()` 로 밖에서 잰다. 다른 테스트의 활동이 섞이면 측정이 무의미해진다 |

웹훅 하네스(`tests/webhook_common/mod.rs`)는 인스턴스마다 `TASTY_HOME`/`webhooks.toml` 을 시딩해야 해서 공유 진입점이 없고, 재시작 영속성 테스트는 같은 HOME 을 물려받는 두 번째 인스턴스가 검증 대상 그 자체다.

### 1-1. 시나리오 하나에 `#[test]` 하나

**한 `#[test]` 에 시나리오를 직렬로 쌓지 않는다.** 앞에서 하나가 죽으면 뒤의 전부가 실행되지
않고, 그 하나 때문에 CI 가 파일 전체를 `--skip` 하게 된다 — GUI 를 요구하지 않는 시나리오까지
같이 사라진다. `tests/e2e_tests.rs` 가 실제로 그 형태였고(33 개 시나리오 / `#[test]` 1 개),
헤드리스 조합에서 파일이 통째로 빠져 있었다. 벽은 마지막 시나리오의 `window.create` 하나였다.

지금은 시나리오마다 `#[test]` 가 하나고, 각 테스트는 `common::shared()` + `create_workspace()`
로 자기 workspace 안에서만 움직인다. 창을 요구하는 단언은 한 테스트에 모아 두어 `--skip` 이
**파일이 아니라 테스트**를 가리킨다. 판정 단위의 근거는 [ADR-0127](../adr/0127-e2e-harness-binary-selection.md)
의 「경계는 테스트 단위로 긋는다」 절.

**따라오는 제약 — 전역 목록 위에서는 길이 산술을 쓰지 않는다.** 테스트는 병렬로 도는데
`pane.list` / `workspace.list` / `hook.list` / `pty.list` / notification 은 workspace 로
격리되지 않는 전역 목록이라, `before + 1` 같은 델타는 다른 테스트가 동시에 만들고 지우면
깨진다. **"내 것이 있는가/없는가"**(`any` / `all`) 로 쓰거나, 세야 한다면 자기 workspace 로
필터한 뒤 센다.

**선례**: `tests/gui_common/mod.rs` 는 `OnceLock` + `atexit` 기반 공유 인스턴스와 "테스트마다 자기 workspace" 전략을 이미 구현해 둔 참조 구현이다. 다만 그것을 쓰는 `gui_tests.rs` 는 전수 `#[ignore]` 라 `cargo test --workspace` 에서도 실행되지 않는다 — 어느 채널에도 한 번도 걸리지 않았고, 선례로 보이지도 않았다.

**집행**: 이 원칙은 `tests/e2e_single_instance_guard.rs` 가 강제한다 — 통합 테스트라 **컴파일은 두 조합 모두 자동, 실행은 헤드리스 조합에서만 자동**이다(기본 조합의 전체 스위트는 자동 채널이 없다). 조합별 실태와 단서(`paths-ignore` 등)는 [ci-gates](ci-gates.md) 가 정본이고 여기 복제하지 않는다. 세 축을 본다 — ① 파일당 전용 spawn 호출 수(미등록 파일은 0 회, 예외는 `ALLOWLIST_FILES` 에 이유와 함께 등록), ② 인스턴스를 띄우는 test 파일 목록 고정(`EXPECTED_INSTANCE_TESTS`), ③ 바이너리 선택이 §0-1 의 한 곳을 거치는지(`BIN_SELECTION_ALLOWLIST` — **면제는 파일 통째가 아니라 횟수까지 묶는다**). ②가 필요한 이유는 파일당 spawn 을 아무리 조여도 binary 가 늘면 총량이 다시 증가하기 때문이다. 실행 중 tasty PID 개수를 세는 **동적 가드는 일부러 쓰지 않는다** — 근거는 ADR-0090 의 대안 D.

## 2. 공유 하네스 (`common::shared()`)

`common::shared()` 는 **호출한 test binary 하나가 공유하는** tasty 인스턴스를 돌려준다. 첫 호출에서만 프로세스를 띄우고 이후 호출은 같은 `&'static` 핸들을 준다.

| 축 | 정책 | 이유 |
|----|------|------|
| 공유 범위 | test binary 당 1개 | §1 참조 |
| 직렬화 | **안 한다** — lock 없이 `&'static` 만 공유 | IPC 서버는 연결마다 별도 스레드로 받아 mpsc 로 큐잉하므로 동시 호출이 안전하다. (`gui_common::shared()` 가 `MutexGuard` 로 완전 직렬화하는 건 실제 데스크톱 마우스/포커스를 뺏는 입력 주입을 쓰기 때문이고, 이쪽은 IPC 전용이라 해당 없음) |
| 테스트 격리 | `TastyInstance::create_workspace()` 로 테스트마다 자기 workspace | IPC 생성은 `IntentOrigin::Agent` 라 active 를 전환하지 않고(원칙 1·3), attach 점유도 workspace/surface 단위 lock 이라 서로 다른 workspace 는 병렬 공존한다 |
| 정리 | `Drop` 이 아니라 `atexit` | 정적 저장이라 `Drop` 이 영원히 돌지 않는다. atexit 가 graceful `system.shutdown` → force kill → port file·격리 HOME 삭제를 수행한다. `Drop` 은 전용 인스턴스 경로로 그대로 남는다. 다만 Drop/atexit 둘 다 프로세스가 강제로 죽으면 실행되지 않는다 — 그 구멍은 §2-1 참조 |
| spawn 실패 | 첫 실패 후 **재시도하지 않는다** | `OnceLock::get_or_init` 은 초기화 클로저가 panic 하면 미초기화로 남아 다음 테스트가 그대로 재시도한다 — 부팅 timeout 상황에서 테스트 수만큼 GUI 프로세스가 더 뜨는 증폭을 막는다. 핸들이 서기 **전**의 패닉(S1 timeout · 조기 종료 · 입력 장치 생성 실패 등)은 `Drop` 을 못 부르므로, 그 구간의 자식은 `spawn_diag::ChildReaper` 가 소유해 되감기에서 kill 하고 거둔다(`Child` 의 Drop 은 kill 하지 않아 그냥 두면 orphan 이 된다). 세 하네스(`common`·`webhook_common`·`gui_common`) 모두 같은 타입을 거친다. 회수기 자체의 계약은 `spawn_diag` 의 단위 시험 둘이 잰다(패닉하면 죽이고 거둔다 · 넘기면 산 채로 넘긴다). 하네스가 그것을 실제 갈래에서 쓰는지는 시험이 없다 — 재는 법은 §2-1 끝 |

격리 헬퍼가 돌려주는 `TestWorkspace` 는 `workspace.create` 응답의 `id` / `index` / `surface_id` 를 그대로 담는다. 공유 경로에서는 `first_surface_id()` / `first_pane_id()`(목록의 `[0]` 번째를 집는다 — 전용 인스턴스 전용) 대신 `first_surface_id_in_workspace()` / `first_pane_id_in_workspace()` 를 쓴다. 갓 만든 workspace 의 PTY 가 필요하면 `wait_for_shell()` 로 첫 프롬프트를 기다린다.

**workspace 로 격리되지 않는 전역 상태**: headless PTY(`pty.*`), `global_hook.*`, notification 은 전역 목록이라 같은 binary 의 다른 테스트가 만든 항목까지 함께 조회된다. 공유 인스턴스 위의 목록 검증은 "내 것이 있는가"(`any`) 형태로 쓰고 길이나 `[0]` 번째를 assert 하지 않는다. surface hook(`hook.unset`)과 headless PTY(`pty.kill`)는 인스턴스가 test 프로세스와 함께 죽으므로 회수가 필수는 아니지만, 같은 binary 의 후속 테스트를 오염시키지 않도록 만든 테스트가 회수하는 것을 기본으로 한다. workspace 자체는 회수하지 않는다 — `workspace.close` IPC 가 없고 회수할 이유도 없다.

`attach_*` test binary 들이 쓰는 attach 스트림 frame/handshake 헬퍼(`read_frame` / `write_control_frame` / `open_workspace_attach` / `open_surface_attach` / `open_stream_without_attach` / `wait_for_control_event`)는 **`tests/attach_common/mod.rs`** 한 곳에 있다 — `tests/common`(인스턴스 하네스)·`tests/webhook_common`(웹훅 하네스)과 같은 층위의 세 번째 공유 test 모듈이다. 개별 `#[test]` 파일끼리는 서로 `mod` 할 수 없지만 디렉토리 모듈은 여러 test binary 가 각자 `mod attach_common;` 으로 가져갈 수 있으므로, 파일마다 복제하지 않는다. 이 모듈에는 "첫 workspace 를 집는" 헬퍼를 두지 않는다 — 공유 인스턴스 위에서 그 습관이 남으면 남의 격리 단위를 밟는다.

하네스 자체 검증은 `tests/shared_instance_harness.rs` — 공유 재사용(spawn 횟수 1 · 동일 port), workspace id 유일성과 그것이 `workspace.list[0]` 이 아님, 전역 `pty.list` 의 `any` assert 가 병렬/`--test-threads=1` 양쪽에서 통과하는지를 확인한다.

### 2-1. 강제 종료 시 자식 정리 — `PR_SET_PDEATHSIG` (Linux)

위 표의 "정리"(Drop/atexit)는 **test 프로세스가 정상적으로든 panic 으로든 unwind 하며 끝날 때만** 동작한다. test 프로세스 자체가 `SIGKILL` 등으로 즉사하면(예: CI 러너 timeout, 셸 도구의 강제 종료) Drop 도 atexit 도 실행되지 않아, 이미 spawn 된 tasty 자식이 영구히 orphan 으로 남는다 — 실제로 이 경로로 leak 된 프로세스가 발견된 적이 있다.

`tests/spawn_diag/mod.rs` 의 `spawn_child()` 가 이 구멍을 막는다: 자식에 `prctl(PR_SET_PDEATHSIG, SIGKILL)` 을 걸어, 부모가 어떤 식으로 죽든 커널이 자식을 대신 죽이게 한다(Linux 전용 — `#[cfg(target_os = "linux")]`, 다른 OS 는 이 보호 없이 `Command::spawn()` 그대로). 세 하네스(`common`·`webhook_common`·`gui_common`)가 모두 이 함수로 띄운다.

**함정 — PDEATHSIG 는 프로세스가 아니라 스레드에 묶인다** (`man 2 prctl` 경고: "the parent ... is considered to be the thread that created this process"). `common::shared()` 의 최초 호출자는 자기 테스트가 끝나면 죽는 cargo test 워커 스레드라, naive 하게 호출 스레드에서 그대로 fork 하면 **그 스레드가 죽는 순간 공유 인스턴스까지 죽어** 이후 다른 스레드에서 도는 나머지 테스트가 전부 "Connection reset" 으로 깨진다(실측: 최초 구현이 정확히 이 증상으로 `attach_git_query_loopback`/`shared_instance_harness` 를 깨뜨렸다). 그래서 실제 fork 는 **프로세스 수명 동안 파킹만 하는 전용 스레드**에서 수행한다 — 커널이 추적하는 "부모 스레드" 를 프로세스 수명과 맞추는 것이 핵심이다. 이 하네스를 고칠 때 fork 지점을 다시 호출 스레드로 되돌리지 말 것.

**두 회수 경로가 하네스에서 실제로 도는지는 자동 채널이 없다.** 둘 다 실패하는 부팅이나 죽는 바이너리에서만 드러나고, 그 조건을 만드는 잡이 없다. 손으로 재는 법 — 대조 팔(회수 코드를 뺀 사본)과 같이 돌려야 값이 된다:

- **핸들 전 패닉**(`ChildReaper`): 웹훅·범용 하네스는 `TASTY_E2E_BIN` 을 포트 파일을 안 쓰고 자기 pid 를 파일에 적은 뒤 오래 자는 셸 스크립트로 덮고, override 를 받는 스위트에 `catch_unwind` 로 spawn 을 감싼 임시 시험을 넣어 패닉 **뒤에** 그 pid 가 사는지 본다. 바이너리가 끝나면 PDEATHSIG 가 어차피 죽이므로 반드시 **같은 프로세스 안에서** 물어야 한다. GUI 하네스는 바이너리를 덮을 수 없어 부팅 상한 상수를 0 으로 바꾼 사본을 전용 Xvfb 에서 돌리고 `/proc/*/environ` 의 격리 `TASTY_HOME` 접두(`tasty-gui-test-home-<시험 pid>-`)로 남은 프로세스를 센다.
- **바이너리 즉사**(`spawn_child`): 부팅에 성공한 뒤 시험이 자기에게 `SIGKILL` 을 보내게 하고, 자식 pid 가 남는지 본다. `xvfb-run` 은 끝나며 X 서버를 내려 GUI 를 함께 죽이므로 이 물음을 가린다 — Xvfb 를 직접 띄우고 그 PID 를 저장해 두었다가 확인 뒤에 내린다.

실측 2026-09-14(Linux, 대조 팔 = 회수 코드가 없던 사본): 웹훅 하네스 상한 초과 뒤 가짜 데몬 생존 `true` → `false`, GUI 하네스 상한 초과 뒤 생존 프로세스 2 → 0, GUI 하네스 바이너리 `SIGKILL` 뒤 창 생존(부모가 init 계열로 바뀐 고아) → 소멸.

### 2-1-1. 도는 인스턴스를 찾을 때 port 파일을 **글롭으로 잡지 마라**

`/tmp` 에는 죽은 port 파일이 남는다. 위 §2-1 의 구멍(SIGKILL 이면 Drop 도 atexit 도 안 돈다)이
남기는 것이고, 그 밖에도 과거 회차의 잔해가 쌓인다. 실측 2026-09-07: 죽은
`tasty-gui-test-*.port` 112 개.

그래서 `/tmp/tasty-*-test-*.port` 를 글롭으로 훑어 "지금 도는 인스턴스" 를 고르면 **거의
항상 죽은 파일을 먼저 집는다.** 이 함정이 특히 나쁜 이유는 산출이 `0` 이 아니라
`Connection refused` 라는 데 있다 — "대상이 없다" 가 아니라 **"대상이 있는데 안 붙는다"** 로
보이므로, 사람은 계기가 아니라 표적을 의심한다. 실측으로 그 자리에서 한 판을 잃었다.

자리에 따라 두 처방을 쓴다.

- **홈을 아는 자리**(하네스 코드·자동화): 격리 홈 디렉터리 이름의 `<pid>-<nanos>` 접미사로
  같은 접미사의 port 파일을 **짝지어** 하나만 본다. 후보를 고르는 문제 자체가 사라진다.
- **홈을 모르는 자리**(사람이 손으로 따라가는 절차): 후보마다 **붙어 보고** 안 되는 것을
  버린다. 살아 있는지는 연결로만 판정한다 — 파일의 존재도, 새로움도 근거가 아니다.

☆ 두 처방 다 "가장 새 파일을 고른다" 를 대체한다. mtime 은 여기서도 근거가 아니다 —
같은 `/tmp` 를 여러 회차·여러 lane 이 함께 쓰므로 가장 새 파일이 내 것이라는 보장이 없다.

### 2-2. 웹훅 포트 선택 (`webhook_common`)

웹훅 리스너의 포트는 **기동 시점 설정값**이라(`TASTY_HOME/webhooks.toml` 의 `port = N`) 하네스가 미리 정해 시딩해야 한다. 그 순간부터 자식이 실제로 bind 할 때까지 아무도 그 번호를 지키지 않으면, 같은 실행의 다른 테스트·다른 워크트리·무관한 프로세스가 가져갈 수 있다. 두 장치로 막는다.

- **예약(`PortLease`)** — `free_port()` 는 번호만 주지 않고 `TcpListener` 를 살린 채 돌려준다. 예약은 `Command::spawn` 직전에만 풀린다. 번호만 빼내 버리는 형태를 타입이 막는다.
- **재시도** — 예약을 풀고 자식이 부팅을 마칠 때까지의 구간(수 초)은 예약으로 닫을 수 없다(같은 포트를 두 소켓이 동시에 listen 할 수 없다). 이 구간에서 뺏기면 리스너가 남긴 bind 실패 경고를 근거로 **도난을 확정**하고, 새 포트로 다시 띄운다(최대 2 회).

재시작 시나리오(같은 홈의 두 번째 인스턴스)는 재시도 대상이 아니다 — 웹훅 URL 이 재시작 간 고정이어야 해서 하네스가 번호를 바꿀 수 없다. 그래서 전용 진입점 `WebhookInstance::builder_for_restart()` 는 포트를 인자로 받지 않고 홈의 `webhooks.toml` 을 SoT 로 읽는다. 호출부가 번호를 따로 들고 다니면 1 차 인스턴스가 재시도로 포트를 바꿨을 때 그 값이 조용히 낡는다.

포트를 뺏겨 실패할 때의 메시지는 "웹훅이 안 떴다" 가 아니라 어느 포트를 누가 가져갔는지와 리스너 경고 원문을 싣는다. bind 실패 경고가 없는 경우(부팅 지연·리스너 init 미호출)와 문구로 구분된다.

## 3. 환경 격리

인스턴스마다 `$TMPDIR/tasty-test-home-{pid}-{nanos}/` 를 새 HOME 으로 만들고, host 환경 누수를 spawn 직전에 차단한다. ("인스턴스마다" 이지 "테스트마다" 가 아니다 — §1 참조.)

| env | 처리 | 이유 |
|-----|------|------|
| `HOME` / `ZDOTDIR` | 격리 HOME override | zsh rc 위치 격리 (macOS/Linux 의 `~/.tasty/` 격리도 겸함) |
| `TASTY_HOME` | 격리 `.tasty` 로 명시 | tasty 루트 해석은 `directories::BaseDirs`(=Windows 는 USERPROFILE) 기반이라 **HOME 만으로는 Windows 에서 격리되지 않는다** — 실사용자 `~/.tasty-debug` 세션 복원이 새어든다. `TASTY_HOME` 이 루트 override 의 SoT |
| `SHELL` | 제거 | host login shell 누수 차단(`detect_bash` 의 `$SHELL` 경로) |
| `OH_MY_ZSH` / `ZSH` | 제거 | oh-my-zsh customization 누수 차단 |
| `TASTY_SURFACE_ID` | 제거 | 부모가 tasty 안일 때 augmented-help 분기 차단 |
| `TASTY_DEBUG_OS_OPEN_LOG` | 격리 홈 아래 `os-open.log` 로 지정. 정의 자리는 `tests/spawn_diag` 의 `apply_os_open_record` 하나이고 세 하네스가 부른다. 범용 하네스는 `os_open_log()` 로 그 경로를 준다 | 자식의 OS 열기(브라우저 · 파일 관리자)가 **실행자의 이미 떠 있는 브라우저**로 URL 을 넘긴다 — 격리 HOME 도 `TASTY_E2E_DISPLAY` 도 그 채널을 못 막는다. 이 값 아래에서 자식은 띄우지 않고 기록만 한다([debug-ipc.md](debug-ipc.md), [ADR-0511](../adr/0511-os-open-is-recorded-not-launched-under-a-debug-switch.md)). debug 스위치라 release 로 지은 자식은 무시한다. `e2e_tests` 의 `directory_dispatch_is_recorded_instead_of_opened_under_the_harness`(gui 조합 · debug 빌드)가 기록을 단언한다 |
| `BROWSER` (unix) | 격리 홈 아래 `os-open-browser.sh` — 받은 인자를 `os-open.log` 에 `BROWSER\t<인자>` 로 적기만 한다. 같은 `apply_os_open_record` 가 준다. 경로에 공백·`:` 가 있으면 `true`, 스크립트를 못 쓰면 하네스가 선다 | 번들 plugin(markdown 외부 링크)은 자기 프로세스에서 `webbrowser::open` 을 부르므로 위 스위치 밖이다. `webbrowser` 는 Linux·BSD 에서 `BROWSER` 를 먼저 보고 성공하면 멈춘다 — macOS·Windows 는 이것으로 안 막힌다. 빈 `BROWSER=` 는 xdg desktop entry 를 직접 실행하므로 쓰지 않는다([ADR-0511](../adr/0511-os-open-is-recorded-not-launched-under-a-debug-switch.md)). 빌드 프로필과 무관하게 준다 |
| `DISPLAY` / `WAYLAND_DISPLAY` | **격리하지 않는다 — 이름을 요구한다.** linux 의 gui 조합에서 `TASTY_E2E_DISPLAY` 를 읽어 자식 `DISPLAY` 로 명시 전달하고(그때 `WAYLAND_DISPLAY` 는 제거), 값이 없으면 spawn 을 세운다. 아래 "어느 디스플레이에 뜨는가" | gui 데몬은 창을 만들고 그 창이 어디 뜨는지는 이 값이 정한다. 지우면 winit 이 즉사해 부팅 자체가 없다 — 다른 축과 성질이 다르다 |
| `TASTY_LOG` | 본체 기본 필터와 **같은 모양** (`warn,wgpu_hal=error,wgpu_core=error,naga=error,egui_winit::clipboard=off`, 웹훅 하네스는 뒤에 `,tasty::webhook::listener=info`). 정의 자리는 `tests/spawn_diag` 의 `LOG_ENV`/`LOG_FILTER` 하나다 | child stderr 폭주에 의한 OS pipe backpressure 회피 + host 의 `TASTY_LOG` 누수 차단. 본체가 읽는 변수는 `TASTY_LOG` 다 — `RUST_LOG` 는 무시된다([crash-diagnostics](crash-diagnostics.md)). **`warn` 한 단어만 주면 안 된다** — 지정하는 순간 본체 기본 필터가 통째로 대체돼 `wgpu_hal=error` 등 억제가 풀리고 로그가 오히려 늘어난다(실측: 미지정 7줄 · `warn` 12줄 · 이 값 7줄) |

### 어느 디스플레이에 뜨는가

**gui 조합의 데몬은 창을 만든다**(§0-1). 그 창이 어디에 뜨는지는 `DISPLAY` 가 정하는데,
하네스가 그것을 정하지 않으면 `Command` 가 부모의 값을 물려주므로 **실행자가 보고 있는
화면이 그대로 시험의 디스플레이가 된다.** 그 상태는 조용하다 — 시험은 통과하고 창만
사람 화면에 뜬다. 실측 2026-09-20: 전용 Xvfb `:77` 위에서 돌린 자식의
`/proc/<pid>/environ` 이 `DISPLAY=:77` 이었고 그 디스플레이에 `1280x720` 짜리
`Tasty (Debug)` 창이 떴다. 격리 `HOME` 은 이것을 막지 못한다 — X 서버가 로컬 사용자를
인증하면 `~/.Xauthority` 없이도 붙는다.

**그렇다고 지울 수는 없다.** 같은 바이너리를 `DISPLAY`·`WAYLAND_DISPLAY` 없이 띄우면
winit 이 `neither WAYLAND_DISPLAY nor WAYLAND_SOCKET nor DISPLAY is set` 로 즉사하고
port file 이 안 써진다(§5 의 `NO_DISPLAY_MARKERS` 가 그 시그니처다). 디스플레이는
격리할 누수가 아니라 **필요한 입력**이다. 그래서 하네스는 격리 대신 **이름**을 요구한다
([ADR-0297](../adr/0297-the-e2e-harness-names-a-display-instead-of-isolating-it.md)).

```
Xvfb :77 -screen 0 1920x1080x24 -nolisten tcp -ac &
TASTY_E2E_DISPLAY=:77 cargo test --test e2e_tests
```

| `TASTY_E2E_DISPLAY` | 하네스가 하는 일 |
|---|---|
| 디스플레이 이름(`:77` 등) | 자식 `DISPLAY` 로 명시 전달 + `WAYLAND_DISPLAY` 제거 |
| `inherit` | 부모 값을 그대로 물려준다(변경 전 동작) — 창을 눈으로 보며 고칠 때 |
| 없음 / 빈 값 | spawn 을 세운다. 실패 문구가 위 두 길을 다 찍는다 |

요구가 서는 조건은 셋의 곱이다 — linux · `gui` feature · `TASTY_E2E_BIN` override 가 안
먹히는 스위트. 그래서 **헤드리스 조합은 이 변수를 요구받지 않는다**(`check-headless` 가
그대로 돈다). macOS/Windows 도 요구하지 않는다 — 거기서는 창이 언제나 사용자 세션에
뜨고, 줄 수 있는 다른 답이 없다.

**선언된 사각**: override 가 *gui* 바이너리를 가리키면 창이 뜨는데 요구가 안 선다. 경로만
보고 그 바이너리의 조합을 알 방법이 없어서다.

Xvfb 를 직접 띄웠으면 **저장한 PID 로 회수한다** — `xvfb-run` 의 `$!` 는 래퍼라 안의
프로세스가 고아로 남는다([screenshot-methods](../ai-verification/screenshot-methods.md)).

### 번들 plugin 은 opt-in 이다

격리 HOME 은 매번 빈 상태로 시작하므로, host 가 부팅할 때 번들 plugin 9 개를 통째로
새로 복사한다 — debug 빌드에서 부팅당 약 1.1 GB 다. 그런데 인스턴스를 띄우는 스위트
대부분은 plugin 을 한 번도 호출하지 않는다.

그래서 하네스는 **기본적으로 빈 디렉터리를 번들 루트로 지정**하고
(`TASTY_BUILTIN_PLUGINS_DIR`), plugin 을 실제로 호출하는 스위트만 명부에 올린다.
명부는 `tests/spawn_diag` 에 하나로 두고 인스턴스를 띄우는 하네스 전부가 `apply_bundle_opt_in` 으로 거친다.

- **명부에 무엇을 넣을지는 이름이 아니라 성질로 정한다** — plugin 네임스페이스를
  호출하는가, plugin 이 뒷받침하는 surface 타입을 여는가. 이름 모양으로 고르면 샌다:
  `explorer` 는 plugin 처럼 생겼지만 호스트 view 이고, `webhook.*` 는 메서드 메타에
  `plugin` 권한 등급이 붙어 있지만 핸들러는 host 다.
- **빠뜨리면 조용히 넘어가지 않는다** — 그 스위트의 plugin 호출이
  `-32601 Method not found` 로 실패해 그 자리에서 빨개진다. 반대 극성(기본 스테이징 +
  예외 명부)은 명부가 낡아도 초록이라 비용만 조용히 자란다.
- 판정은 **테스트 바이너리 단위**다. 공유 인스턴스가 프로세스당 `OnceLock` 이라
  (§2) 테스트 함수 안에서 정하면 먼저 도는 테스트가 초기화를 가져가 경합한다.

실측(같은 스위트를 번들만 바꿔 잰 대조): 격리 홈 최대 **1148 MB → 4 MB**.
★ 그 "같은 스위트" 가 **어느 스위트인지는 기록에 없고**, 바로 아래 1160 의 `e2e_tests` 와는
**다른 스위트**다 — 두 수를 한 계열로 읽지 마라. 근거와 못 좁힌 이유는 [ADR-0182](../adr/0182-test-instances-do-not-stage-bundled-plugins-by-default.md)
의 "격리 홈 최대라는 이름의 세 값" 이 정본이고 여기 복제하지 않는다.
명부가 실제로 가르는지는 따로 본다 — 명부 밖 `attach_silent_disconnect` 4 MB,
명부 안 `e2e_tests` 1160 MB. **초록만으로는 판정이 안 된다**: 명부 밖 스위트는
지렛대가 걸리든 안 걸리든 통과하므로, 통과는 두 경우 모두와 양립한다.
**벽시계는 74 s → 74 s 로 안 줄었다** — 이 조치가 지우는 것은 그 스위트의 시간이 아니라
그 스위트가 만드는 dirty page 이고, 그 부담은 같은 박스에서 동시에 도는 다른 러너가 진다.
동시 러너 수 대비 효과는 측정하지 않았다. 근거와 대안은
[ADR-0182](../adr/0182-test-instances-do-not-stage-bundled-plugins-by-default.md).

### 명부 안 스위트는 번들을 hardlink 로 받는다

명부에 오른 스위트(plugin 을 실제로 부르는 쪽)는 번들이 필요하다. 그 격리 홈에는 번들을
**복사하지 않고 hardlink 로 미리 넣는다** — `apply_bundle_opt_in` 이 자식을 띄우기 전에
`<TASTY_HOME>/plugins/<id>/` 를 채운다. host 는 이미 있는 builtin 을 같은 버전이면 **내용으로**
대조하므로(ADR-0191) 같은 파일을 다시 쓰지 않는다. 제품의 설치 경로는 그대로다 — 사용자
홈으로의 sync 는 지금도 복사다.

- **원본은 번들이 아니라 하네스 소유 스냅숏이다** — `target/<profile>/test-bundle-links/<서명>/`.
  hardlink 는 inode 를 공유하므로 번들(`builtin-plugins/`)에 직접 걸면 `just build-plugins` 의
  제자리 `cp` 가 돌고 있는 시험의 plugin 을 바꾸거나 `Text file busy` 로 빌드가 실패한다.
  스냅숏은 번들의 경로·크기·mtime 서명으로 이름을 붙여 **번들이 바뀔 때만 한 번** 복사하고,
  서명이 다른 옛 스냅숏은 지운다.
- **스냅숏의 원본은 자식이 고를 번들이다** — 하네스가 제품의 `bundle_root_from_exe_dir` 를
  그대로 부른다(debug 에서는 그 안의 dev 스테이징도 돈다 — 자식이 부팅하며 할 쓰기를 먼저 할
  뿐이고 자식 쪽은 no-op 이 된다). `TASTY_BUILTIN_PLUGINS_DIR` 이 걸려 있으면 그것이 이긴다.
  `TASTY_E2E_BIN` override 중에는 자식의 프로필을 하네스가 모르므로 채우지 않는다.
- **실패는 전부 변경 전 동작으로 물러난다** — 스냅숏 자리와 `temp_dir` 이 다른 파일시스템이면
  (시작할 때 hardlink 한 번으로 잰다) 스냅숏을 안 만들고, 걸기가 중간에 멈추면 못 건 파일을
  host 가 복사로 채운다. 스냅숏이 낡았어도 host 가 내용으로 대조해 다른 파일만 새로 쓴다 —
  틀리는 것은 비용뿐이고 결과가 아니다. 사유는 `tracing::warn!` 으로 남는다.
- **제자리 쓰기가 없다는 것이 전제다** — host 의 설치·갱신은 전부 임시 파일 + rename 이고
  청소는 unlink 라 공유 inode 를 안 건드린다. plugin 프로세스의 영속 쓰기는 설치 폴더가 아니라
  `TASTY_PLUGIN_DATA_DIR` 로 간다. 이 전제를 깨는 코드(설치 폴더 안 파일을 열어 고치는 것)를
  더하면 스냅숏이 오염된다 — 그래도 다음 홈은 host 의 내용 대조가 바로잡지만 그 스냅숏에서 건
  홈들은 오염된 채로 부팅한다. 전수 근거는 ADR-0525.

실측(2026-09-23, 이 개발 박스, `e2e_tests` 한 번 = 인스턴스 2 개, 같은 계기로 전·후):
host 프로세스(`tasty`)의 `/proc/<pid>/io` `write_bytes` 합 **2446 MB → 13.7 MB**, 격리 홈의
hardlink 아닌 바이트 **1226 + 1221 MB → 10 + 4 MB**. 번들이 바뀐 뒤 첫 완주는 하네스가
스냅숏을 한 번 쓴다(약 1.2 GB — 그 뒤 완주는 0). 근거·대안은
[ADR-0525](../adr/0525-test-homes-hardlink-the-bundle-from-a-harness-owned-snapshot.md).

격리 HOME 에 사전 작성하는 파일:

| 파일 | 내용 | 이유 |
|------|------|------|
| `.zshrc` / `.bashrc` | 빈 파일 | shell rc customization 차단 |
| `.tasty/config.toml` | `shell="/bin/sh"`(POSIX) / Git Bash(Windows) + `restore_layout=false` | `is_shell_valid()` 즉시 true → `detect_bash()`(host `/etc/passwd` 의존) 미호출 → **shell_setup_mode 진입 차단** |

`shell_setup_mode` 에 진입하면 port file 이 영구히 안 써져 spawn 이 timeout panic 한다. config.toml 사전 작성이 이 경로의 *결정적* 차단이다.

## 4. Timeout (2단계)

| 단계 | 상수 | 값 | 조건 |
|------|------|-----|------|
| S1 | `SPAWN_PORT_TIMEOUT` | 40 s | `--port-file` 에 port 가 쓰여짐 |
| S2 | `SPAWN_SHELL_TIMEOUT` | 20 s | first surface `screen_text` 가 non-empty (첫 PTY prompt) |

값은 단순 증가가 아니라 **결정적 fix(config.toml 사전 작성) 위의 마진**이다 — dev cold path worst-case(GPU init + plugin discover/extract + theme/db init, dev 프로필 ~3.5× 느림) + self-hosted runner 변동 폭 흡수.

두 상수는 **`tests/spawn_diag`** 한 곳에만 있고 `tests/common`·`tests/webhook_common` 이 그것을 쓴다. 두 하네스는 같은 바이너리를 같은 방식으로 띄우므로 같은 단계에 다른 잣대를 댈 근거가 없다(이전에는 30/15 와 40/20 으로 갈려 있었고 그 차이의 근거가 어디에도 없었다). 통합값은 둘 중 큰 쪽이다 — 낮추는 쪽은 근거 없는 동작 축소이고, 올리는 쪽이 늘리는 것은 *이미 실패할 spawn* 이 보고되기까지의 시간뿐인데 그 시간은 아래 조기 종료 감지가 대부분 없앤다.

**상한을 다 기다리지 않는 경우**: port file 대기 루프가 매 바퀴 자식의 종료를 확인한다. 자식이 이미 죽었으면(디스플레이 부재·설정 오류 등 즉사 계열) 그 자리에서 실패시킨다 — 죽은 프로세스를 40 초 더 기다릴 이유가 없다.

## 5. stderr tail 진단

spawn timeout panic 시 child stderr 마지막 30 라인을 panic 메시지에 첨부한다. `Stdio::piped()` + background drain thread + 링버퍼(capacity 256)로 OS pipe buffer(Linux 64KB / macOS 16KB)가 차서 child 가 write block 되는 것을 방지. `TASTY_LOG` 로 verbosity 를 cap 한다(drain 1차 + cap 2차 방어) — 값은 §4 의 env 표대로 **본체 기본 필터와 같은 모양**이어야 한다. 30 줄짜리 tail 은 노이즈 몇 줄에도 밀려나므로, 필터를 느슨하게 주는 것이 곧 진단 손실이다.

GUI 하네스도 `StderrCapture` 를 `GuiTestInstance` 필드로 보관한다. 부팅 뒤 IPC
연결·송신·응답 해석 실패와 UI 조건 대기 만료는 원래 오류/상태와 함께 **지금까지 수집한**
stderr 꼬리를 낸다. 살아 있는 자식의 실패 진단에서 drain을 join하지 않는다.
전용 인스턴스의 `Drop` 은 자식 kill → wait → stderr join → 임시파일 정리 순서다.
공유 `static` 인스턴스는 Rust가 Drop하지 않으므로 기존 atexit 정리 경로를 사용하며,
그 경로의 drain 합류를 이 Drop 계약이 보장하지는 않는다.

`gui_common::stderr_tests` 는 실제 Tasty 대신 가짜 셸·IPC 서버로 부팅 후 진단과
자식 회수를 검사한다. Enigo 필드 생성에 디스플레이가 필요해 기본은 ignored이며,
Linux 격리 X 디스플레이에서 `cargo test --locked --test gui_tests gui_common::stderr_tests -- --ignored --test-threads=1` 로 실행한다. 데스크톱 입력은 보내지 않는다.
자동 실행 채널은 없다. 이 시험은 자식 회수와 꼬리 보존을 검사하며, drain의 명시적
join 호출 자체는 별도 계측 없이는 합류 누락과 빠른 자연 종료를 구분하지 못한다.

### 범용 하네스의 부팅 이정표

`tests/common`은 spawn 호출 직전의 `Instant` 하나를 기준으로 자식 PID와
`spawn_returned_ms`, `port_found_ms`, `first_ipc_response_ms`, `shell_ready_ms`를 기록한다.
첫 IPC 응답은 JSON 해석이 성공한 응답(error 응답 포함), shell-ready는 초기 surface의
첫 non-empty 화면 관측이다. 이후 IPC 호출과 새 workspace의 출력은 이 첫 시각들을
덮어쓰지 않는다. `pending`은 아직 관측하지 못한 다음 단계이며 원인 판정은 아니다.

이정표는 stderr ring 밖의 고정 슬롯에 보관한다. spawn/IPC 패닉의 unwind에서
`startup failure` 스냅샷을 다시 출력하므로, 후속 stderr가 30줄 tail을 밀어내도 초기
시각은 실패 출력에 남는다. `startup_diagnostics()`로 성공한 인스턴스의 값도 읽는다.
출력은 기존 `spawn_diag::init_test_tracing()`의 libtest writer를 사용한다. 이미 등록된
subscriber는 교체하지 않으므로 그 subscriber가 로그를 차단하면 수집되지 않는다.
새 ring, 전역 panic hook, 타임아웃 상향은 사용하지 않는다. 외부 SIGKILL이나 출력
수집기 자체의 유실은 unwind 진단이 보장하지 못한다.

`shared_instance_harness`의 `startup_tests`는 소유한 fake 자식과 loopback IPC로
포트 전 지연·포트 후 응답 지연·조기사망·응답 오류를 유발해 실제 공용 경로를 검사한다.
포트 전 지연은 자식의 준비 신호 → 부모의 포트 부재 확인 → 해제 신호 → 포트 공개로
동기화한다. 부모 관측 시작과 자식 진행은 별개이므로 포트 구간에 시간 하한을 두지 않는다.
부모가 늦게 실행되고, 공개 완료 신호까지 받은 뒤 부팅 관측을 시작하는 경우도 검사한다.
이 경우에도 실제 부팅 완료·이정표 순서·첫 응답 시각 보존을 단언한다. 준비 신호 전에
포트를 공개하는 변이는 포트 부재 단언으로 검출한다.
필터 실행: `cargo test --locked --test shared_instance_harness startup_tests -- --nocapture`.
실제 Tasty/GUI를 띄우는 같은 타깃의 다른 시험과 구분한다.

시각은 부모가 관측한 경과이며 자식 내부의 복사·bootstrap 시간으로 귀속하지 않는다.
GUI 내부 구간은 기존 `tasty::boot` trace를 별도로 읽는다. headless port 파일도 dispatch
준비 완료를 뜻하지 않는다. 내부 원인 분해에는 같은 실행의 추가 근거가 필요하다.
attention의 quiet window와 frame 대기에는 이 계측을 일반화하지 않는다. 재발한 시험의
handshake/raise/read/clear/quiet 경과와 frame tag가 없는 과거 총시간은 미귀속이다.

## 5-1. 마커 대기 만료 진단 (`tests/marker_wait`)

훅이 남기는 마커 파일을 기다리는 자리는 셋이다(`hooks_detection_e2e` · `hook_env_integration` ·
`webhook_integration`). 대기 함수는 `tests/marker_wait` 한 곳에 있고 세 타깃이 `mod` 로 함께 쓴다 —
같은 물음에 답하는 사본이 셋이면 하나를 고쳐도 나머지 둘은 안 고쳐진다.

**만료 메시지가 두 사건을 가른다.** 마커가 안 나온 것과, 마커는 나왔는데 이 폴링 루프가 굶어
못 본 것은 처방이 정반대인데 종전 메시지(`marker file … not written within 15s`)는 둘을 같은
말로 덮었다. 가르는 값은 **실제 확인 횟수**다 — 예산을 폴 간격으로 나눈 기대치와 비교해,
기대의 절반에 못 미치면 굶주림이라고 메시지가 직접 적는다. 그때 상한을 올리는 것은 처방이
아니다(형태 C 완화가 은폐로 작동하는 자리다 — [ADR-0129](../adr/0129-flaky-test-classes-and-standard-fixes.md)).

메시지에 함께 싣는 것: 경과 · 예산 · 확인 횟수와 기대치 · 1 분 부하 · 호출자가 준 증거(선택).
**부하는 기록이지 판정이 아니다** — 실측에서 부하 평균은 지연을 예측하지 못했다(최대 지연이
낮은 부하 회차에서 났다).

## 6. Flaky 대응 절차

0. **panic 메시지 두 번째 줄의 판정문을 먼저 읽는다.** 하네스가 stderr 시그니처로 단서를 미리 갈라 놓는다. 단서마다 **확신 수준이 다르다** — 문장이 단정하는 것만 원인으로 받아들인다.
   - "디스플레이 서버가 없다 — 코드 인과가 아니다" → **단정.** 이 시그니처는 정상 부팅 stderr 에 나오지 않는다. `xvfb-run -a` 위에서 다시 돌린다.
   - "GPU 가속 경로 폴백 흔적이 있다 — 이것만으로는 원인 판정이 되지 않는다" → **단정이 아니다.** 아래 6-1 로 가되, 거기서 GPU 경합이 아니라고 판명되면 1~4 의 일반 절차를 그대로 밟는다. 이 줄을 봤다고 코드를 건너뛰지 않는다.
   - "부팅 차단 시그니처는 없다" → 아래 1~4 의 일반 절차로.
1. panic 의 `--- stderr (last 30 lines) ---` 확인.
2. 범용 하네스는 `startup failure`의 `pending`과 네 이정표로 포트 전·첫 응답 전·셸 출력 전을 구분한다. 다른 하네스는 실패한 호출 단계를 함께 읽는다. `IPC server listening` 줄의 부재는 기본 필터·tail 탈락으로도 생기고, 존재는 dispatch 준비를 보증하지 않으므로 그 한 줄로 S1/S2를 판정하지 않는다.
3. 재현: `cargo test --test e2e_tests -- --nocapture`.
4. 결정적 차단이 깨졌으면 `TastyInstance::spawn` 의 env/config 보강.

### 6-1. GPU 분기

**시그니처만으로 원인이 정해지지 않는다.** `renderD128` · `VK_ERROR_` · `DRI3` · `libEGL` · `tu_knl` · `failed to open device` 는 드라이버가 **가속 경로를 포기하고 폴백할 때** 나오는 줄이고, 폴백한 뒤 부팅은 대개 성공한다 — 실측(2026-09-04) 결과 이 개발 머신에서는 **정상적으로 부팅해 port file 을 쓴 인스턴스의 stderr 에 여섯 개가 전부** 들어 있었다. 즉 이 줄들은 "GPU 가 없어서 못 떴다" 의 증거가 아니라 "이 머신은 원래 소프트웨어 렌더로 돈다" 는 배경 소음이다.

그래서 이 분기는 **경합 여부를 직접 확인하는 절차**다.

- 같은 머신에서 다른 워크트리·인스턴스가 GPU 를 쓰고 있는지 먼저 본다. 여러 에이전트가 병렬로 작업하는 환경에서는 GUI 를 띄우는 검증을 세마포어로 직렬화한다.
- 같은 실행에서 **깨지는 스위트가 매번 다르면** 코드 인과가 아니라 자원 경합의 무작위 희생자라는 신호다.
- 다른 GUI 사용자가 없는데도 재현되면 **경합이 아니다** — 6 의 1~4 로 돌아가 코드 쪽(부팅 지연·plugin·셸 설정)을 본다.
- 단독 재실행으로 통과하면 경합 판정이 확정된다.

정상 부팅에 나오지 않는 GPU 시그니처를 관측하게 되면 `tests/spawn_diag` 의 `GPU_FALLBACK_MARKERS` 를 그것으로 바꾸고 판정문의 단정을 되살릴 수 있다. 지금 그렇게 하지 않은 이유는 이 머신에서 GPU 초기화가 **항상** 소프트웨어 폴백으로 성공해 그런 로그를 채집할 수 없었기 때문이다.

## VTE 시뮬레이터 (`tasty-tui-simulator`)

터미널 동작 검증용 도구 — 고수준 명령을 raw VTE escape 시퀀스로 변환해 출력한다(터미널 입장에선 실제 TUI 앱과 같은 바이트 스트림). **인터랙티브 모드**(stdin REPL — 외부에서 `surface.send` 로 명령 단계 전송, 명령마다 `OK` 동기화)와 원샷 시나리오를 제공한다. 명령: cursor/print/sgr/fg·bg/altscreen/scroll-region/erase/raw/esc 등, 종료 제어 `quit`/`exit-code N`/`crash`(SIGABRT)/`panic`. debug 의 `debug.cell_info`/`debug.screen_attrs`([debug-ipc](debug-ipc.md))와 조합하면 셀 속성을 결정적으로 자동 검증할 수 있다.

로직은 `lib.rs` 에 있고 두 진입점이 공유한다(SoT 하나) — 독립 바이너리 `tasty-tui-sim`(`cargo build -p tasty-tui-simulator`, release 빌드 가능) 과 `tasty debug sim <subcommand>`(debug 빌드 한정). **debug 빌드에선 별도 빌드/PATH 설정 없이** `tasty debug sim ...` 으로 바로 호출할 수 있다(이미 `tasty` 가 PATH 에 있으므로). surface 안에서 stdout 에 직접 VTE 를 뿜는 로컬 동작이라 IPC 를 거치지 않는다. 자세한 명령 목록·부하 모드(`flood`)는 [tui-testing](tui-testing.md).

## 관련

- [ADR-0090](../adr/0090-test-isolation-by-workspace-not-process.md) — 격리 단위를 workspace 로 정한 근거·대안·재검토 조건
- [self-verification.md](self-verification.md) — 커밋 전 시나리오 재현
- [attach-behavior.md](attach-behavior.md) — 점유 레지스트리(workspace/surface 단위 lock)
- [build.md](build.md) — dev/release/dist 프로필 (timeout 값 산정 근거)
