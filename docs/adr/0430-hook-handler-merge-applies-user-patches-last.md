# ADR-0430: hook handler 병합은 출처 순서(Host → Plugin → User)로 하고 user patch 를 늘 마지막에 둔다 — ADR-0047 의 병합 순서 조항 개정

- **Status**: Accepted
- **Date**: 2026-09-21
- **Tags**: hook-handler, registry, plugin, settings, boot, headless, patch-semantics, adr-0047, adr-0427
- **Group**: webhook-hooks

## Context

`HookHandlerRegistry`(`src/hook_handler/registry.rs`)는 같은 handler id 에 대한 출처별 contribution 을
보관하고, finalize 에서 그것을 병합한다. 병합은 **첫 contribution 을 base 로 두고 나머지를 "마지막
non-None 이 이긴다" 로 덮는다.** [ADR-0047](0047-shared-hook-handler-registry-source-gate.md) 은 이
병합을 "install 순서 보존" 으로 정했고, `push_contribution` 은 뒤에 붙인다. 그래서 병합 순서가 곧
설치 순서였다.

`~/.tasty/hook-handlers.toml` 의 항목이 plugin hook handler 를 patch 할 때(예:
`com.example.hookp/notify` 에 `priority = 10` 만) 이 순서가 뒤집히는 경로가 둘 있다. 이는 file handler
registry 에서 [ADR-0427](0427-file-handler-merge-applies-user-patches-last.md) 이 고친 것과 같은 결함이다.

1. **headless 부팅**: `src/boot.rs` 가 `install_default_sources()` 로 host 기본값과 user 설정을 먼저
   넣는다. plugin 은 필요해질 때(`plugin enable`, namespace forward, kind 지목 생성 요청) 기동되고 그때
   contribute 한다. plugin 의 `priority` 는 선언상 늘 값이 있으므로 user patch 를 덮는다.
2. **plugin 재기동**: plugin 을 끄면 그 contribution 이 빠지고, 다시 켜면 user contribution **뒤에**
   붙는다. 결과는 1 과 같다.

GUI 부팅은 이 결함에 걸리지 않았다. `src/app/window_lifecycle.rs` 의 `discover_and_start` 가 plugin
contribution 을 먼저 설치하고, 그 뒤 `src/app/boot_machine.rs` 가 `install_default_sources()` 를
부른다. 그래서 설치 순서가 이미 Plugin → User 였다.

실측(2026-09-21, 격리 `TASTY_HOME`, 매니페스트만 있는 테스트 plugin `com.example.hookp` 가
`notify` 를 `priority = 50` 으로 contribute, user patch `priority = 10`):

| 바이너리 · 경로 | `hook-handler get` 결과 |
|---|---|
| 수정 전 GUI debug · 부팅 직후 | `priority 10 · owner user` — 걸리지 않음 |
| 수정 전 GUI debug · `plugin disable` → `plugin enable`, reload 없음 | `priority 50 · owner com.example.hookp` — 결함 |
| 수정 전 headless debug · 부팅 뒤 `plugin enable` | `priority 50 · owner com.example.hookp` — 결함 |

두 결함 경로 모두 `tasty hook-handler reload` 를 손으로 부르기 전까지 설정이 무시됐다. plugin 이 값을
주는 필드(`priority` · `source` · `action` · `display_name_i18n_key`)가 다 그랬다. 이것은 user patch 의
정의 — 기존 host/plugin contribution 위에 적은 필드만 덮는다(`UserHookHandlerUpsertDecl`) — 와
어긋난다.

## Decision

**finalize 가 병합 전에 contribution 을 owner 순 — Host → Plugin → User — 으로 안정 정렬하고, 정렬
뒤의 첫째를 base 로 쓴다.** 정렬은 `src/hook_handler/registry.rs` 의 `merge_order` 한 자리에 있다.
같은 owner 안의 순서는 설치 순서 그대로다. 그래서 user patch 는 설치 시점과 무관하게 늘 마지막으로
이기고, base 는 원 출처(host 또는 plugin)가 된다. user 만 있는 id(`user/<short>`)는 user 가 base 다.

이 결정은 ADR-0047 의 **병합 순서** 조항("patch semantics(install 순서 보존, `Some` 필드만 덮어씀)")
중 "install 순서 보존" 만 개정한다. 개정하지 않는 것:

- patch semantics — `Some` 필드만 덮어쓴다.
- 조회 정렬 — priority↑ → owner tie-break(User > Plugin > Host) → id.
- source 게이트, actor 별 action 스키마, 셸 불변식.
- 프로세스 전역 싱글턴.
- 명명(`hook` / `webhook`).

**host ↔ plugin 순서는 결정할 거리가 아니다.** file handler 와 같은 이유다. id 이름공간이 출처를
가른다. host 는 `host/<short>`, plugin 은 `<plugin_id>/<short>` 로만 contribute 하고(`install_host` ·
`install_plugin`), plugin id 는 `.` 을 반드시 품어(`tasty_utils::plugin_id::is_valid_plugin_id`)
`host` 와 겹치지 않는다. 그래서 한 id 에 host 와 plugin 이 함께 오지 않고, 서로 다른 plugin 도 함께
오지 않는다. `push_contribution` 이 같은 owner 의 이전 contribution 을 지우고 붙이므로 owner 당
contribution 은 하나다. 정렬이 실제로 옮기는 것은 user 의 자리뿐이다.

## Consequences

- **얻은 것**: 설정 파일의 user patch 가 headless 부팅 뒤에도, plugin 을 껐다 켠 뒤에도 reload 없이
  적용된다. 실측(위 표와 같은 조건, 수정 뒤 바이너리): 세 경로 모두 `priority 10 · owner user`.
- **잃은 것 / 한계**: 없다고 판단한다. 정렬이 host 와 plugin 사이, 또는 plugin 끼리의 순서를 바꿀
  입력이 없다(위 Decision 의 id 이름공간).
- **운영 비용 / 유지 부담**: 병합 순서가 설치 순서와 갈라졌다. contribution 을 넣는 새 경로가 생겨도
  순서는 `merge_order` 가 정하므로 그 경로는 순서를 신경 쓰지 않아도 된다. file handler 와 hook handler
  의 두 `merge_order` 는 같은 모양의 복제다 — 레지스트리가 다른 크레이트에 있고 contribution 타입이
  다르다.
- **호환성**: 결함 경로의 결과가 바뀐다. 전에는 plugin 값이, 이제는 user patch 값이 나온다. 사용자가
  설정 파일에 적은 대로 되는 쪽이다. IPC 응답 형태·CLI 출력 형식·plugin wire 는 그대로다. GUI 부팅
  직후의 결과는 전과 같다.

## Alternatives Considered

- **A. `push_contribution` 이 user contribution 을 늘 끝에 다시 놓는다.** 안 고른 이유: 순서의 뜻이
  삽입 경로마다 흩어진다. ADR-0427 이 같은 이유로 기각했다.
- **B. headless 부팅이 user 설정을 plugin 뒤에 읽는다.** 안 고른 이유: headless 는 plugin 을 필요할
  때 띄우므로 "뒤" 가 한 시점이 아니다. 또 경로 2(plugin 재기동)를 못 고친다.
- **C. 문서만 고친다**("plugin 이 켜진 뒤 reload 해야 적용된다"). 안 고른 이유: 결함을 사용자에게
  떠넘긴다.

## Reconsideration Triggers

**채널이 붙는 것**

- `src/hook_handler/registry_tests.rs` 의 `a_user_patch_wins_over_a_plugin_installed_after_the_boot_load`
  · `a_user_patch_wins_over_a_plugin_that_contributes_later_without_a_reload` 가 실패한다 — user patch 가
  다시 설치 순서에 밀렸다. 변이 확인: `merge_order` 의 정렬을 빼면 둘 다 죽는다(2026-09-21, 29 passed
  · 2 failed).

**원리적으로 안 붙는 것**

- id 이름공간이 바뀌어 한 id 에 host 와 plugin(또는 서로 다른 plugin)이 함께 contribute 할 수 있게
  된다. 그때 Host → Plugin 순서와 plugin 끼리의 순서가 처음으로 뜻을 갖게 되므로 다시 정한다.

## References

- 개정 대상: [ADR-0047](0047-shared-hook-handler-registry-source-gate.md) (병합 순서 조항의 "install 순서 보존")
- 개정 패턴 선례: [ADR-0030](0030-image-egui-mesh-bitmap-texture.md)
- [ADR-0427](0427-file-handler-merge-applies-user-patches-last.md) — file handler registry 의 같은 결함과 같은 처방
- [hooks 기능 문서](../features/hooks/index.md) — 핸들러 레지스트리 절
- 코드 근거(결정이 실현된 현재 위치): `src/hook_handler/registry.rs` 의 `merge_order` · `merge_contribution`
