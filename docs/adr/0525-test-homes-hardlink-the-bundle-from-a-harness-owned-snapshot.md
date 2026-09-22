# ADR-0525: 번들을 부르는 시험 홈은 하네스 소유 스냅숏에서 번들을 hardlink 로 받는다

- **Status**: Accepted
- **Date**: 2026-09-23
- **Tags**: testing, harness, plugin, performance, disk-io, hardlink, adr-0182, adr-0191
- **Group**: plugin-system

## Context

[ADR-0182](0182-test-instances-do-not-stage-bundled-plugins-by-default.md) 가 번들 스테이징을
opt-in 으로 돌려, plugin 을 안 부르는 스위트의 격리 홈 쓰기는 사라졌다. 명부에 남은
스위트(`e2e_tests` · `soak_memory` · `attach_markdown_content_loopback`)는 여전히 부팅마다
번들 전량을 격리 홈으로 복사했다. 격리 홈은 매번 새로 만들어지므로 host 에게는 언제나 첫
설치이고, [ADR-0191](0191-two-local-files-are-compared-bytewise-not-hashed.md) 의 내용 판정은
두 번째 이후 부팅에만 듣는다. `e2e_tests` 는 인스턴스 둘(공유 1 · 전용 1)을 띄우므로 한 번 돌
때마다 약 2.4 GB 를 쓴다.

그 쓰기는 초록이다. 대신 같은 박스의 다른 러너가 그 비용을 진다. flake 등록부의 양성
대조는 인스턴스 없이 디스크 쓰기 포화만으로 초 단위 IPC 정체를 재현했고, 부팅 복사가 그
쓰기를 만드는 쪽이다.

제약 셋:

- **제품 동작은 바꾸지 않는다.** 사용자 홈으로 가는 sync 는 복사로 남는다. 설치 경로에는
  서명·업그레이드 판정이 얹혀 있어서 시험 사정으로 바꿀 자리가 아니다.
- hardlink 는 inode 를 공유한다. 누가 한쪽을 **그 자리에서** 고치면 다른 쪽도 바뀐다.
- 정확성은 이미 host 가 쥐고 있다. 같은 버전이면 host 는 번들과 설치본을 내용으로 대조해
  다른 파일만 새로 쓴다.

## Decision

명부에 오른 스위트는 자식을 띄우기 전에 번들을 `<TASTY_HOME>/plugins/<id>/` 에 **hardlink
로 미리 넣는다.** 그러면 host 의 내용 판정이 "이미 같다" 로 끝나 복사가 일어나지 않는다.
hardlink 의 원본은 번들 자체가 아니라 **하네스 소유 스냅숏**이다. 스냅숏은
`target/<profile>/test-bundle-links/<서명>/` 에 있고, 서명은 번들의 경로·크기·mtime 이다.
번들이 바뀔 때 한 번만 복사하고, 서명이 다른 옛 스냅숏은 지운다. 스냅숏의 원본은 자식이
고를 번들이다. 하네스가 제품의 `bundle_root_from_exe_dir` 를 그대로 부르는데, 같은 답을
하네스에 따로 적으면 사본이 갈리기 때문이다. 이 때문에 그 함수를 `pub` 으로 열었고 동작은
바뀌지 않았다. 스냅숏을 만들거나 거는 데 실패하면 host 의 기존 복사로 물러난다. 이 조치는
최적화일 뿐이고 정확성이 걸린 자리가 아니다.

## Consequences

- **얻은 것**: 2026-09-23 에 이 개발 박스에서 `e2e_tests` 를 전·후 같은 계기로 한 번씩 돌려
  쟀다. `/proc/<pid>/io` 를 0.2 s 간격으로 폴링했다.
  - host 프로세스(`tasty`)의 `write_bytes` 합: **2446 MB → 13.7 MB**. 전·후 각 두 번 잰 값이다.
  - 격리 홈에서 hardlink 가 아닌 바이트: **1226 + 1221 MB → 10 + 4 MB**. hardlink 로 들어간
    1216 MB 는 새로 쓴 바이트가 아니다.
  - 시험은 64 개 모두 초록이다. plugin 을 부르는 시험도 포함된다.
- **첫 완주 비용**: 번들이 바뀐 뒤 처음 도는 완주는 하네스가 스냅숏을 한 번 복사한다(약
  1.2 GB). 같은 측정에서 번들 스테이징까지 비어 있던 첫 완주의 하네스 쓰기는 2436 MB 였다.
  그중 1.2 GB 는 자식이 부팅하며 어차피 했을 dev 스테이징이다. 그 뒤 완주에서 하네스 쓰기는
  0 이다. ★ **"번들이 바뀐 뒤 첫 완주" 의 전·후 대조는 재지 않았다.** 전 상태의 첫 완주는
  dev 스테이징까지 host 가 하므로 계기를 따로 짜야 한다.
- **쓰기량을 다른 계기로 한 번 더 쟀다.** `/usr/bin/time` 의 `%O`(회수된 자식을 포함한
  rusage 블록 출력 × 512 B)로 `e2e_tests` 완주를 전·후 8 회씩 쟀다. 전 2497272 블록
  (**약 1278 MB**, 8 회 모두 같은 값) → 후 6680~6688 블록(**약 3.4 MB**)이다. 전용 인스턴스
  하나분이다 — 공유 인스턴스는 `OnceLock` 정적이라 wait 되지 않아 rusage 에 안 잡힌다.
  위 `/proc/<pid>/io` 의 인스턴스당 1226 MB → 10 MB 와 같은 크기다.
- **벽시계는 이 결정의 목적이 아니고, 자기 스위트는 느린 쪽으로 기운다.** ADR-0182 와 같은
  이유로, 지우는 것은 그 스위트의 시간이 아니라 그 스위트가 만드는 dirty page 다. 그 목적
  지표는 위 두 계기에서 모두 달성됐다. 잰 값은 다음과 같다.
  - 구현 측정: 단독 완주 전 7.3~8.0 s, 후 7.6~10.5 s.
  - 번갈아 잰 측정: 같은 target·같은 tasty 바이너리·전용 Xvfb 에서 `e2e_tests` 시험
    바이너리만 전/후로 갈아 끼워 A B B A … 순으로 8 쌍 돌렸다. 조건은 load 40~70, 다른
    lane 열둘 가동 중이었다. 따뜻한 상태 벽시계(s)는 전 10.17 · 10.03 · 14.71 · 8.45 · 9.08 ·
    9.42 · 8.17 · 12.50(중앙 약 9.7), 후 11.87 · 11.49 · 23.95 · 10.60 · 13.95 · 8.36 · 7.63 ·
    14.01(중앙 약 11.7)이다. 인접 쌍 8 개 중 6 개에서 후가 느렸다.
  - 단일 부팅(`lifecycle_toggles_answer_without_a_window --exact`, 5 쌍 번갈아): 전 평균
    3.07 s · 후 2.86 s 로 차이가 없다. 공유 인스턴스의 부팅 자체는 안 느려졌다.
  - 이 계수를 회귀의 크기로 읽지 않는 이유: 공유 머신의 A/B 벽시계는 순서를 뒤집어도
    편향이 반대로 안 뒤집힌다(이 레포에서 이미 확인된 함정이다 — 부하 궤적이 편향을
    정한다). 그래서 "8 쌍 중 6 쌍" 은 그 시각의 부하 궤적에 종속된 값이다. 한가한 머신의
    값은 없다 — 아래 재검토 조건이 그것을 잰다.
  - 동시 러너 수에 대한 효과는 재지 않았다.
- **잃은 것**: 디스크에 스냅숏 하나(약 1.2 GB)가 빌드 트리마다 남는다. `cargo clean` 이
  지운다. 전제도 하나 생긴다. **설치 폴더 안 파일을 그 자리에서 고치는 코드가 없어야 한다.**
- **전제의 현재 상태(소스 전수, 2026-09-23)**:
  - host 의 설치·갱신은 전부 `copy_atomic`(임시 파일 + rename)으로 모인다. 청소는 unlink 다.
  - IPC `plugin.install` 의 `fs::copy` 는 목적지가 이미 있으면 거부하므로 언제나 새 경로에 쓴다.
  - plugin 프로세스의 영속 쓰기는 `TASTY_PLUGIN_DATA_DIR` 로 간다. 번들 plugin 코드에
    `current_exe`·`current_dir` 호출은 0 건이다.
  - 조건부로만 성립하는 자리 둘:
    - plugin 프로세스의 cwd 가 설치 폴더다. 상대 경로 쓰기가 생기면 제자리 쓰기가 된다.
    - codex 의 install 은 IPC 로 받은 절대 경로에 쓴다.
- **전제가 깨졌을 때**: 오염은 스냅숏에서 멈춘다. 번들(`builtin-plugins/`)과 사용자 홈에는
  닿지 않는다. 오염된 스냅숏에서 건 홈은 host 의 내용 대조가 번들 쪽으로 되돌리므로 다음
  부팅부터 바로잡힌다. 다만 서명이 안 바뀌어 스냅숏은 오염된 채 남고, 그동안 비용이 되살아난다.

## Alternatives Considered

- **번들(`builtin-plugins/`)에서 직접 hardlink**: 스냅숏 복사가 없어 가장 싸다. 고르지 않은
  이유는 원본 쪽에 제자리 쓰기가 실제로 있기 때문이다. `Justfile` 의 `build-plugins` ·
  `build-plugin` 은 `cp` 로 스테이징본을 덮어쓴다. 직접 걸면 돌고 있는 시험의 plugin
  바이너리가 바뀌고, 실행 중이면 `Text file busy` 로 사용자의 빌드가 실패한다. 반대 방향도
  위험하다. 홈 쪽 제자리 쓰기가 개발 번들을 오염시키고, 그 번들이 다시 사용자 debug 홈으로
  sync 된다.
- **스위트마다 스냅숏 복사**: 스위트 하나가 띄우는 인스턴스는 한두 개라, 스위트마다 1.1 GB
  를 쓰면 지우려던 비용이 그대로 남는다. 그래서 스냅숏의 수명을 스위트가 아니라 번들 서명에
  건다.
- **심볼릭 링크**: host 의 `sync_dir` 은 심볼릭 링크를 파일로 보고 `fs::copy` 로 따라가 읽는다.
  내용이 같으면 쓰지 않으므로 효과는 같다. 그러나 `copy_atomic` 이 링크를 실파일로 바꾸는
  순간 공유가 끊기고, 링크가 가리키는 쪽은 언제든 바뀔 수 있다. hardlink 는 원본 이름이
  바뀌거나 지워져도 inode 가 살아 있다.
- **host 에 "시험이면 설치를 건너뛰라" 스위치(debug 격리)**: 제품 의미를 시험 문제로 바꾸는
  것이다. 하네스가 미리 채운 홈으로 풀리므로 스위치를 두지 않는다.
- **서명에 내용 해시를 쓴다**: 번들을 매번 1.1 GB 읽어야 한다. 서명이 틀려서 드는 대가는
  비용뿐이고 결과는 안 틀린다(host 가 내용으로 대조한다). 그래서 값싼 서명을 골랐다.

## Reconsideration Triggers

**채널이 붙는 것**: 판정 시점에 레포에서 읽을 수 있는 사실이다.

- 설치 폴더 안 파일을 그 자리에서 여는 쓰기(`OpenOptions`·`set_permissions`·`set_len` 등)가
  `crates/tasty-host-plugin` 이나 번들 plugin 에 생길 때. 지금은 이를 잡는 가드가 없고, 이
  ADR 과 `docs/dev-guide/e2e-tests.md` 의 서술이 전부다.
- 명부(`SUITES_THAT_CALL_BUNDLED_PLUGINS`)가 비게 될 때. 그러면 이 경로는 쓰이지 않는다.

**원리적으로 안 붙는 것**: 사람이 관측해야 한다.

- CI 러너에서 `target` 과 `temp_dir` 이 다른 파일시스템이라 이 경로가 늘 물러나고 있을 때.
  재는 법: 명부 안 스위트의 출력에서 `번들 hardlink 를 안 쓴다` warn 줄을 본다. 통과한 시험의
  출력은 캡처되므로 `--nocapture` 로 돌린다.
  구독자는 `apply_bundle_opt_in` 이 첫 줄에서 설치하므로 하네스 종류와 무관하게 보인다.
  실측 2026-09-23: 스냅숏 자리(`target/debug/test-bundle-links`)를 일반 파일로 막고
  `e2e_tests lifecycle_toggles_answer_without_a_window --exact --nocapture` 로 돌리면 그 줄이
  1 줄 찍힌다. 그 설치 한 줄을 빼면 같은 조건에서 0 줄이다.
- 한가한 머신(다른 lane·빌드가 안 도는 상태, load 가 코어 수보다 한참 낮을 때)에서 다시
  쟀는데도 자기 스위트가 유의하게 느릴 때. 그러면 원인을 파고든다. 추정 원인(미검증)은
  이렇다: 후에는 host 가 번들과 홈을 **둘 다** 약 1.1 GB 씩 바이트 비교로 읽는다. 전에는
  번들 읽기 + 홈 쓰기였다.
  재는 법: 같은 target·같은 tasty 바이너리에서 `e2e_tests` 시험 바이너리만 이 ADR 직전
  하네스와 지금 하네스로 갈아 끼우고, A B B A 순으로 8 쌍 이상 따뜻한 상태 완주 벽시계를
  잰다. 재는 동안 `uptime` 의 load 를 함께 적는다. 두 분포가 겹치지 않거나 인접 쌍의
  대부분에서 후가 느리면 판정이 뒤집힌다. 그때 원인을 가르는 첫 계기는 gui 부팅이 찍는
  `tasty::boot` 타깃의 `T3a plugin_discovery (install_builtins + refresh_packages)` 줄의 `ms`
  를 전·후로 견주는 것이다. 하네스가 자식에 주는 필터(`spawn_diag::LOG_FILTER`)는 warn 이라
  이 info 줄이 안 나온다 — 재는 동안만 `tasty::boot=info` 를 덧붙이고 자식 stderr 를 받아
  본다. 헤드리스 부팅에는 이 줄이 없다. 단일 부팅
  (`lifecycle_toggles_answer_without_a_window --exact`)도 함께 재서, 차이가 공유 인스턴스
  부팅에서 나는지 전용 인스턴스 쪽에서 나는지 가른다.

## References

- `tests/spawn_diag/mod.rs`: `apply_bundle_opt_in` · `prefill_bundle_links` · `bundle_link_snapshot`
- `crates/tasty-host-plugin/src/builtin.rs`: `bundle_root_from_exe_dir` · `install_builtin_overwrite_present` · `copy_atomic`
- `docs/dev-guide/e2e-tests.md`: "명부 안 스위트는 번들을 hardlink 로 받는다"
- 선행 결정: [ADR-0182](0182-test-instances-do-not-stage-bundled-plugins-by-default.md). 첫
  스테이징을 없앤 결정이고, 그 Alternatives 가 hardlink 를 명부 안 스위트의 후속 여지로 남겼다.
  이 ADR 이 그 후속이다.
- 선행 결정: [ADR-0191](0191-two-local-files-are-compared-bytewise-not-hashed.md). 이 결정이
  기대는 내용 판정이다.
