# ADR-0539: 헤드리스는 레이아웃을 영속하지 않고, 그 사실을 부팅 때와 `system.info` 로 말한다

- **Status**: Accepted
- **Date**: 2026-09-23
- **Tags**: headless, layout-persistence, settings, restore-layout, agent-facing, build-combination, boot, adr-0346
- **Group**: architecture

## Context

gui 는 창(engine)마다 레이아웃 슬롯 하나를 점유하고, 레이아웃이 바뀌면 그 슬롯 파일에 저장하고,
다음 부팅에서 읽어 복원한다(`general.restore_layout`, 기본 켜짐 —
[layout-persistence](../features/layout-persistence/index.md)). 저장은 `DomainIntent::SaveLayoutNow`,
복원은 `DomainIntent::ApplyPendingLayoutRestore` 이고 둘 다 gui 전용 variant 다. 부르는 자리도
`app::persistence` · `app::window_lifecycle` 뿐이다.

헤드리스(`--no-default-features`)는 engine 을 슬롯 `None` 으로 만든다(`src/boot.rs` 의
`bootstrap_engine`). 슬롯이 없으니 읽지도 쓰지도 않는다. capture · restore · scrollback 경로 전체에 그
빌드의 호출자가 없다. 그 결과는 이미 관측 가능했다 — `system.info` 의 `layout_slot` 이 `null` 이다.

그런데 그것을 **말하는 자리가 없었다.** `general.restore_layout` 은 기본이 켜짐이라 헤드리스
데몬을 띄운 사람은 "재시작하면 워크스페이스가 돌아온다" 고 읽는다. 데몬을 다시 띄우면 기본
워크스페이스 하나로 시작하고, 로그에도 응답에도 그 설정이 무시됐다는 흔적이 없다. 설정을 받아
놓고 아무 일도 안 하는 조용한 무동작이다.

## Decision

**헤드리스는 레이아웃을 영속하지 않는다.** 헤드리스 인스턴스의 워크스페이스는 프로세스 수명
동안만 산다. 이것을 결함이 아니라 경계로 확정하고, 두 자리에서 말하게 한다.

1. **부팅 때 한 번** — `general.restore_layout` 이 켜져 있으면 헤드리스 부팅이 warn 한 줄을 남긴다
   (`layout_persistence_notice`, `src/boot.rs`): 그 설정이 이 빌드에서 아무 일도 안 한다는 것과
   `system.info` 로 확인하는 법. 꺼져 있으면 말할 것이 없다.
2. **in-band** — `system.info` 의 `layout_slot: null` 이 "이 engine 은 슬롯을 잡지 않는다" 의 답이다.
   값은 새로 만든 것이 아니라 이미 있던 것이고, 이 결정은 그 값을 계약으로 고정한다.

거절할 **요청**은 없다. 레이아웃 저장·복원을 부르는 IPC 메서드가 없고, 두 intent 는 헤드리스에
컴파일되지 않는다([ADR-0346](0346-headless-compiles-only-what-it-reaches.md)). 헤드리스에 닿는 입력은
설정 키 하나뿐이라, 그 설정을 읽는 부팅 자리가 거절(무시)을 말하는 자리다.

부팅 1 회 훅 `layout_persistence::migrate_and_gc_on_boot` 는 그대로 돈다 — 레거시 `layout.json` 을
슬롯으로 옮기고 전 슬롯 union 으로 scrollback 을 정리하는 것은 같은 홈을 쓰는 gui 의 데이터를
정리하는 일이지 헤드리스가 영속하는 일이 아니다.

## Consequences

- **얻은 것**: 헤드리스에서 `restore_layout` 이 조용히 무시되지 않는다. 사람은 부팅 로그로, 에이전트는
  `system.info` 로 "이 인스턴스는 재시작하면 비어서 온다" 를 안다. 헤드리스 부팅 동작은 바뀌지 않는다 —
  옛 워크스페이스가 되살아나 셸을 다시 띄우는 일이 생기지 않는다.
- **잃은 것**: 헤드리스 데몬을 재시작하면 워크스페이스가 사라진다. 이것은 전에도 그랬다 — 새로 잃는 것은
  없다.
- **운영 비용 / 유지 부담**: `restore_layout` 의 기본이 켜짐이라, gui 와 같은 홈을 쓰는 헤드리스는 부팅마다
  경고 한 줄을 남긴다. 그 줄을 없애려고 설정을 끄면 같은 홈의 gui 복원도 꺼진다. 헤드리스를 따로 운영하는
  홈에서는 꺼서 없앤다.

## Alternatives Considered

- **A. 헤드리스도 영속한다(슬롯 하나를 잡아 저장·복원).** 안 고른 이유가 셋이다. ① 기본이 켜짐이라 모든
  헤드리스 데몬의 부팅 동작이 바뀐다 — 재시작하면 옛 워크스페이스가 되살아나고 그 수만큼 셸이 뜬다. 에이전트는
  surface 를 ID 로 다루는데 복원된 surface 는 새 ID 를 받으므로, 되살아난 것은 에이전트가 쥐고 있던 대상이
  아니다. ② 같은 홈을 쓰는 gui 와 슬롯 파일을 나눠 써야 한다 — 슬롯 점유는 살아 있는 engine 집합으로만
  판정하므로 두 프로세스가 같은 슬롯을 덮어쓸 수 있다. ③ plugin surface 복원은 plugin 이 hello 를 마친 뒤에야
  가능한데 헤드리스 plugin 관리자는 필요할 때만 뜬다 — 복원하면 plugin surface 가 사라진다. 셋 다 결함 수정이
  아니라 새 기능의 설계다.
- **B. 헤드리스에서 `restore_layout` 을 켠 설정을 오류로 거절한다(부팅 실패).** 같은 설정 파일을 gui 와 나눠
  쓰므로 gui 에 맞는 설정이 헤드리스를 못 띄우게 된다. 설정은 기본값이 켜짐이라 설정 파일이 없는 첫 부팅도
  막힌다.
- **C. `system.info` 에 새 칸(`layout_persistence: false`)을 더한다.** 같은 사실을 이미 `layout_slot: null` 이
  말한다. 같은 사실에 두 칸을 두면 둘이 어긋날 자리가 생긴다.
- **D. 말하지 않는다(문서에만 적는다).** 설정의 기본이 켜짐이라 문서를 안 읽은 사람에게는 조용한 무동작이
  그대로 남는다.

## Reconsideration Triggers

**채널이 붙는 것**

- `tests/e2e_tests.rs` 의 `a_headless_daemon_answers_that_it_holds_no_layout_slot` 이 실패한다 — 헤드리스가
  슬롯을 잡기 시작했다(영속 배선이 생겼다)는 뜻이다. 그때 이 ADR 을 다시 연다. 변이 확인: 헤드리스
  `bootstrap_engine` 이 `Some(1)` 슬롯을 넘기게 하면 이 시험이 죽는다(2026-09-23).
- `src/boot.rs` 의 `a_restore_layout_setting_is_announced_as_ignored` 가 실패한다 — 고지 **문구**가 무엇을
  안 하는지·어떻게 확인하는지를 더는 말하지 않는다는 뜻이다. 이 시험은 함수의 반환값만 본다.
  변이 확인: `layout_persistence_notice` 가 늘 `None` 을 돌려주게 하면 죽는다(2026-09-23).
- `tests/e2e_tests.rs` 의 `a_headless_daemon_warns_at_boot_that_restore_layout_is_ignored` 가 실패한다 — 부팅이
  그 고지를 **warn 으로** 내지 않는다는 뜻이다(설정을 켠 헤드리스 자식의 stderr 를 제품 기본 필터 그대로 읽는다).
  위 단위 시험은 레벨을 못 보므로 이 칸이 따로 있다. 변이 확인: 부팅의 `tracing::warn!` 을 `trace!` 나
  `info!` 로 내리면 죽는다(2026-09-23).

**원리적으로 안 붙는 것**

- 헤드리스 재시작 뒤 작업 복원을 요구하는 사용 사례가 생긴다(예: 원격 attach 서버를 재부팅해도 점유하던
  워크스페이스가 돌아와야 한다). 재는 법: 사용자·에이전트 요청으로 드러난다. 그때 위 A 의 셋 — 부팅 동작 변경 ·
  슬롯 공유 · plugin surface 복원 — 을 설계로 푼다.

## References

- [headless-build-boundaries](../dev-guide/headless-build-boundaries.md) — "두 조합이 다르게 두는 것"
- [layout-persistence](../features/layout-persistence/index.md) — 슬롯 모델과 헤드리스 제약
- 선행 결정: [ADR-0346](0346-headless-compiles-only-what-it-reaches.md) (저장·복원 intent 가 헤드리스에 컴파일되지 않는 근거) · [ADR-0087](0087-layout-slot-occupancy-model.md) (슬롯 점유 모델 — 헤드리스를 다루지 않는다). 레이아웃 영속의 헤드리스 경계를 정한 ADR 은 없었다. 탐색: `git grep -l -e 'restore_layout' -e 'layout_slot' -- docs/adr/`
- 코드 근거(결정이 실현된 현재 위치): `src/boot.rs` 의 `layout_persistence_notice` · `bootstrap_engine`
