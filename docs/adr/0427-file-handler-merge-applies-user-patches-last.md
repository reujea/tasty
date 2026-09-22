# ADR-0427: file handler 병합은 출처 순서(Host → Plugin → User)로 하고 user patch 를 늘 마지막에 둔다

- **Status**: Accepted
- **Date**: 2026-09-21
- **Tags**: file-handler, registry, plugin, settings, boot, patch-semantics
- **Group**: file-handler

## Context

`FileHandlerRegistry` 는 같은 handler id 에 대한 출처별 contribution 을 보관하고, finalize 에서 그것을
병합한다. 병합은 "마지막 non-None 이 이긴다" 다. 그런데 병합 순서는 **설치 순서**였다
(`push_contribution` 은 뒤에 붙인다).

user 설정의 항목이 plugin handler 를 patch 할 때(예: `com.tasty.markdown/viewer` 에 `priority = 10` 만)
이 순서가 뒤집히는 경로가 둘 있었다.

1. **부팅**: user 설정을 먼저 읽고(`src/core/state.rs` 의 registry 초기화), plugin 은 그 뒤에
   contribute 한다(`crates/tasty-host-plugin/src/manager/lifecycle.rs`). plugin 의 `priority` 는 선언상
   늘 값이 있으므로 user patch 를 덮는다.
2. **plugin 재기동**: plugin 을 끄면 그 contribution 이 빠지고, 다시 켜면 user contribution **뒤에**
   붙는다. 결과는 1 과 같다.

두 경우 모두 사용자가 `file-handler reload` 를 손으로 부르기 전까지 설정이 무시됐다. plugin 이 값을
안 주는 필드(`disabled` 등)만 살아남았다. 실측(2026-09-21, 단위 수준): user patch 뒤에 plugin 을
설치하면 `priority=50 owner=Plugin`, 거기서 reload 하면 `priority=10 owner=User`.

이것은 user patch 의 정의 — "기존 host/plugin contribution 의 메타데이터를 부분 덮어쓴다"
(`UserHandlerUpsertDecl`) — 와 어긋난다. 또 [ADR-0426](0426-file-handler-reload-reports-the-entries-it-dropped.md)
의 `target_not_contributed` 는 "대상이 contribute 되면 그대로 적용된다" 고 말하는데, 이 순서에서는
그 문장이 `priority` 에 대해 거짓이었다.

## Decision

**finalize 가 병합 전에 contribution 을 owner 순 — Host → Plugin → User — 으로 안정 정렬한다.**
정렬은 안정 정렬이라 같은 owner 안의 순서는 설치 순서 그대로다(지금은 owner 당 contribution 이
하나뿐이라 이 경우가 없다 — Consequences). 그래서 user patch 는 설치 시점과 무관하게 늘 마지막으로
이긴다. 정렬은 `crates/tasty-file-handler/src/registry.rs` 의 `merge_order` 한 자리에 있다.

## Consequences

- **얻은 것**: 설정 파일의 user patch 가 재시작 뒤에도, plugin 을 껐다 켠 뒤에도 reload 없이
  적용된다. ADR-0426 의 "대상이 contribute 되면 그대로 적용된다" 가 참이 된다.
- **잃은 것 / 한계**: 없다고 판단한다. 정렬이 host 와 plugin 사이, 또는 plugin 끼리의 순서를 바꿀
  입력이 없다 — id 이름공간이 출처를 가른다. host 는 `host/<short>`, plugin 은 `<plugin_id>/<short>`
  로만 contribute 하고, plugin id 는 `.` 을 반드시 품어(`tasty_utils::plugin_id::is_valid_plugin_id`)
  `host` 와 겹치지 않는다. 그래서 한 id 에 host 와 plugin 이 함께 오지 않고, 서로 다른 plugin 도
  함께 오지 않는다. 또 `push_contribution` 이 같은 owner 의 이전 contribution 을 지우고 붙이므로
  owner 당 contribution 은 하나다. 한 id 에 모일 수 있는 것은 원 출처 하나(host 또는 그 plugin)와
  user 하나뿐이고, 정렬이 실제로 옮기는 것은 user 의 자리뿐이다.
- **운영 비용 / 유지 부담**: 병합 순서가 설치 순서와 갈라졌다. contribution 을 넣는 새 경로가
  생겨도 순서는 `merge_order` 가 정하므로 그 경로는 순서를 신경 쓰지 않아도 된다.
- **호환성**: 부팅 직후의 결과가 바뀐다. 전에는 plugin 값이, 이제는 user patch 값이 나온다. 사용자가
  설정 파일에 적은 대로 되는 쪽이다.

## Alternatives Considered

- **A. `push_contribution` 이 user contribution 을 늘 끝에 다시 놓는다.** 안 고른 이유: 순서의 뜻이
  삽입 경로마다 흩어진다. plugin 설치 · 제거 · user 설치 모두가 그 불변식을 지켜야 하고, 하나라도
  빠뜨리면 같은 결함이 돌아온다. 병합하는 한 자리에서 정하면 넣는 쪽은 신경 쓰지 않아도 된다.
- **B. 부팅 때 user 설정을 plugin 뒤에 읽는다.** 안 고른 이유: 경로 2(plugin 재기동)를 못 고친다.
- **C. 문서만 고친다**("plugin 이 켜진 뒤 reload 해야 적용된다"). 안 고른 이유: 재시작마다 설정이
  무시되는 결함을 사용자에게 떠넘긴다.

## Reconsideration Triggers

**채널이 붙는 것**

- `crates/tasty-file-handler/src/registry_tests.rs` 의
  `a_user_patch_wins_over_a_plugin_installed_after_the_boot_load` ·
  `a_user_patch_wins_over_a_plugin_that_contributes_later_without_a_reload` ·
  `a_patch_leaves_the_report_once_its_plugin_contributes` 가 실패한다 — user patch 가 다시 설치 순서에
  밀렸다. 변이 확인: `merge_order` 의 정렬을 빼면 셋 다 죽는다(2026-09-21).

**원리적으로 안 붙는 것**

- id 이름공간이 바뀌어 한 id 에 host 와 plugin(또는 서로 다른 plugin)이 함께 contribute 할 수 있게
  된다. 그때 Host → Plugin 순서와 plugin 끼리의 순서가 처음으로 뜻을 갖게 되므로 다시 정한다.

## References

- [ADR-0426](0426-file-handler-reload-reports-the-entries-it-dropped.md) — `target_not_contributed` 의 "대상이
  나타나면 적용된다"
- [file-handler 기능 문서](../features/file-handler/index.md) — Contribution 머지 절
- 코드 근거(결정이 실현된 현재 위치): `crates/tasty-file-handler/src/registry.rs` 의 `merge_order` ·
  `ensure_finalized`
