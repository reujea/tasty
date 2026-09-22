# ADR-0520: file-format(detector) 병합도 출처 순서(Host → Plugin → User)로 하고 user 의 `disabled = false` 를 켜기로 읽는다

- **Status**: Accepted
- **Date**: 2026-09-23
- **Tags**: file-format, detector, registry, plugin, settings, boot, patch-semantics

## Context

`FileFormatRegistry`(`crates/tasty-file-format`)는 detector id 마다 출처별 contribution 을 보관하고
finalize(`ensure_finalized`)에서 "마지막 non-None 이 이긴다" 로 병합한다. 병합 순서는 **설치 순서**였다
(`install_one` 은 같은 출처를 지우고 뒤에 붙인다). file handler registry 에서 같은 결함을 고친
[ADR-0427](0427-file-handler-merge-applies-user-patches-last.md) 과 같은 두 경로가 여기에도 있었다.

1. **부팅**: user 설정(`~/.tasty/file-handlers.toml`)을 plugin 보다 먼저 넣으므로 plugin 이 값을 주는
   필드(`display_name_i18n_key` · `icon` · `disabled`)는 plugin 값이 이긴다.
2. **plugin 재기동**: 다시 켠 plugin 의 contribution 이 user 뒤에 붙는다.

또 하나가 겹쳐 있었다. `DetectorDecl.disabled` 가 `bool` 이라 `install_one` 은 `false` 를 "적지 않음"
(`None`)과 구별하지 못했다. Settings 에서 plugin 이 끈 detector 를 켜면 user contribution 에
`Some(false)` 가 들어가고 저장 파일에 `disabled = false` 로 남지만, 다음 부팅에 그 줄은 `None` 으로
읽혀 켜기가 사라졌다 — 순서를 고쳐도 이 경로는 남는다.

handler 와 다른 점: detector id 에는 출처 이름공간이 없다(`DetectorId(decl.id)`). host 와 plugin 이
`markdown` 같은 같은 id 를 contribute 할 수 있으므로, 출처 순서를 들이면 host 와 plugin 사이의 순서도
정하게 된다. 번들 plugin 중 detector 를 contribute 하는 것은 html · markdown · image · mesh-demo 다.

## Decision

- **finalize 가 병합 전에 contribution 을 출처 순 — Host → Plugin → User — 으로 안정 정렬한다**
  (`crates/tasty-file-format/src/registry.rs` 의 `merge_order`). 같은 출처 안(서로 다른 plugin 끼리)은
  설치 순서 그대로다.
- **host 와 plugin 이 같은 id 를 contribute 하면 plugin 이 host 를 덮는다(Host → Plugin).** host 기본값은
  부팅 첫머리에 한 번 설치되고 plugin 은 늘 그 뒤에 설치돼 왔으므로, 지금까지 관측되던 결과가 이것이다.
- **`DetectorDecl.disabled` 를 `Option<bool>` 로 바꾸고, 명시적 `false` 는 user 출처에서만 켜기로
  읽는다.** host · plugin 의 `false` 는 종전대로 "끄지 않음" 이다 — 다른 출처가 끈 것을 켜지 않는다.

## Consequences

- **얻은 것**: 설정 파일의 user patch(표시명 · 아이콘 · 켜기/끄기)가 재시작 뒤에도, plugin 을 껐다 켠
  뒤에도 reload 없이 적용된다. Settings 에서 켠 plugin detector 가 재시작 뒤에도 켜져 있다.
- **잃은 것 / 한계**: 같은 rule 을 두 출처가 적었을 때 finalize 결과에 남는 rule 의 origin 과 rule 의
  나열 순서가 부팅 직후에는 달라진다(전에는 user 가 먼저 설치돼 user origin 이 남았고, 이제 plugin
  origin 이 남는다). 판정은 rule 의 나열 순서에 기대지 않고, user 로 내보내기(`export_user_config`)는
  finalize 결과가 아니라 user contribution 을 읽으므로 저장 내용은 같다. finalize 된 rule 의 origin 을
  읽던 소비자는 Settings detectors 탭 둘이었다 — "user 항목 삭제" 버튼을 보일지(`has_user`)와 출처 칸.
  그대로 두면 user 와 plugin 이 같은 rule 을 적은 detector 에서 부팅 직후 버튼과 `user` 표시가 사라진다
  (전에도 부팅 직후와 reload 뒤가 서로 달랐다). 그래서 둘 다 contribution 을 읽게 옮겼다: 버튼은
  user 출처 contribution 이 있는지(`FileFormatRegistry::has_user_contribution` — rule 없는 표시명 ·
  아이콘 · 켜기/끄기 patch 도 센다), 출처 칸은 rule 을 선언한 출처들(`rule_origins`)이다. 이제 두 표시가
  부팅 직후 · reload 뒤 · plugin 재기동 뒤에 같다. 그 대가로 rule 없이 켜기/끄기나 아이콘만 바꾼 user
  항목에도 삭제 버튼이 보인다 — 누르면 그 patch 를 지워 host · plugin 값으로 돌아간다.
- **운영 비용 / 유지 부담**: `DetectorDecl` 을 리터럴로 만드는 자리는 `disabled: None` 을 쓴다. plugin
  manifest 의 JSON 은 `Option` 으로 그대로 역직렬화된다.
- **호환성**: 부팅 직후의 결과가 바뀐다. 전에는 plugin 값이, 이제는 user patch 값이 나온다. host · plugin
  의 선언 해석은 그대로다.

## Alternatives Considered

- **Plugin → Host 순서(host 가 plugin 을 덮는다)** — host 기본값을 plugin 이 못 바꾸게 한다. 안 골랐다:
  지금까지의 결과를 뒤집어 번들 plugin 이 host 기본값에 얹던 메타(표시명 등)가 사라진다.
- **detector id 에 출처 이름공간을 들인다(handler 처럼 `host/…` · `<plugin>/…`)** — host 와 plugin 사이
  순서를 정할 필요가 없어진다. 안 골랐다: 저장 파일 · extension_priority 표 · handler 의 detector 참조가
  전부 id 로 이어져 있어 마이그레이션이 필요하다.
- **순서만 고치고 `disabled` 는 두기** — user 가 켠 plugin detector 가 재부팅에 여전히 꺼진다. 완결되지
  않는다.

## Reconsideration Triggers

**채널이 붙는 것** — 판정 시점에 레포가 읽을 수 있는 사실이다.

- `crates/tasty-file-format/src/registry/tests.rs` 의
  `a_user_patch_wins_over_a_plugin_installed_after_the_boot_load` ·
  `a_user_patch_wins_over_a_plugin_that_contributes_again_without_a_reload` ·
  `a_user_enable_beats_a_plugin_disable_across_boot_and_plugin_restart` ·
  `a_plugin_overrides_a_host_default_whatever_the_install_order` ·
  `the_user_entry_is_seen_the_same_after_boot_and_after_a_reload` 가 실패한다. 변이 확인(2026-09-23):
  `merge_order` 의 정렬을 빼면 다섯 다 죽고, user 의 `Some(false)` 해석을 빼면 셋째가 죽고,
  `has_user_contribution` 을 finalize 된 rule 의 origin 으로 판정하게 되돌리면 다섯째가 죽는다.
- `src/view/settings/ui/file_handler_tab/detectors.rs` 의
  `a_user_rule_shared_with_a_plugin_keeps_the_user_origin_and_remove_button` 가 Settings 탭 한 행의
  출처 칸 · 삭제 버튼 판정(`detector_row_origin`)을 본다. 변이 확인(2026-09-23): 그 판정의 버튼 쪽이나
  출처 칸 쪽을 finalize 된 rule 의 origin 으로 되돌리면 죽는다. `draw_detectors` 가 그 함수를 안
  거치고 다시 직접 판정하는 것은 시험이 못 본다.
- plugin 이 `extension_priority` 를 contribute 하게 된다 — 지금은 host · user 만 그 표를 쓰므로 표의
  출처 순서는 설치 순서(host 가 부팅 첫머리)로 충분하다.

**원리적으로 안 붙는 것** — 사람이 관측해야 한다.

- 번들 · 외부 plugin 이 host 기본 detector 의 메타를 덮는 것이 문제로 보고된다. 재는 법: host 기본값
  (`crates/tasty-file-format` 의 `HOST_DEFAULTS_TOML`)과 plugin manifest 의 detector 선언에서 같은 id 를
  센다.

## References

- 짝 결정: [ADR-0427](0427-file-handler-merge-applies-user-patches-last.md) — file handler registry 의 같은 결정
- [file-handler 기능 문서](../features/file-handler/index.md) — Contribution 머지 절
- 코드 근거(결정이 실현된 현재 위치): `crates/tasty-file-format/src/registry.rs` 의 `merge_order` ·
  `ensure_finalized`, `registry/helpers.rs` 의 `install_one`, `config.rs` 의 `DetectorDecl`
