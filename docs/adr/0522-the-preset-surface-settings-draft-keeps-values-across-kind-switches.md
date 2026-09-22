# ADR-0522: 프리셋 surface 설정 화면의 draft 는 kind 를 바꿔도 값을 지우지 않는다

- **Status**: Accepted
- **Date**: 2026-09-23
- **Tags**: preset, layout-presets, draft, surface-settings, design-parity, egui, input, adr-0510
- **Group**: ui-theme-gallery

## Context

프리셋 편집기에서 surface 파라미터(kind · 작업 디렉터리 · 시작 명령 · kind 가 선언한 필드)를 고치는 자리가 칸 안의 인라인 폼에서 오른쪽 영역 전체를 차지하는 **설정 화면**으로 옮겨졌다. 설정 화면은 draft 다 — 확인을 눌러야 적용·저장되고 취소는 버린다. 시안은 Claude Design 의 `gallery/preset_editor.jsx`(`SurfaceSettings` · `useSurfaceCfg` · `switchKind` · `normalize`)이고, 이 레포의 사이트 사본에도 같은 파일이 있다.

시안을 그대로 옮기면 기존 편집기 계약과 부딪히는 자리가 있었다.

- 시안의 `switchKind` 는 kind 를 바꿀 때마다 draft 를 **새 kind 가 선언한 키만** 남긴 객체로 다시 만든다. 그러면 A → B → A 로 kind 를 잠깐 바꿨다 되돌리는 것만으로, A 만 선언하던 값이 사라진다. 이 구현의 요구사항은 그 왕복에서 원래 params 가 돌아와야 한다는 것이었다.
- 시안의 확인은 `normalize(draft)` 로 surface 를 통째로 갈아 끼운다. 편집기는 이미 kind 가 선언하지 않은 params 를 보존하는 왕복 계약을 갖고 있다(`demo_layout.rs` 의 `set_field_writes_param_and_preserves_unknown_params`). 통째로 갈면 그 계약이 설정 화면 경로에서만 깨진다.
- 시안은 진입할 때 Kind `Select` 에 autofocus 한다. 본체의 공용 `select` 위젯은 클릭으로만 열리고 키보드로는 조작되지 않는다.
- 설정 화면을 여는 더블클릭은 시안에서 칸 전체가 받는다. 본체의 칸 가장자리에는 이미 경계 split 존이 있고, 그 위의 클릭은 분할이다.
- 저장은 디스크 쓰기라 실패할 수 있다. 실패했을 때 화면을 닫을지 남길지 정해야 했다.

## Decision

**설정 화면의 draft 는 kind 전환에 값을 잃지 않고, 정리는 확인 시점에 한 번만 한다.** 나머지 네 자리도 이 lane 에서 함께 정했다.

1. **kind 전환은 draft 값을 지우지 않는다.** `LeafDraft::switch_kind` 는 kind 만 바꾸고 cwd · 시작 명령 · params 를 draft 에 그대로 둔다. 두 kind 가 함께 선언한 키는 이어지고, 원래 kind 로 돌아오면 원래 값이 다시 보인다. 새 kind 가 쓰지 않는 컬럼·params 의 정리는 확인 시점에 **최종 kind 가 원본과 다를 때만** `DemoLayout::apply_leaf_draft` → `set_kind` 로 한 번 한다. kind 가 같으면 선언된 필드만 `set_field` 하므로 선언되지 않은 params 는 보존된다. dirty 판정은 시안과 같다 — kind 와 **지금 kind 가 선언한 키**만 본다.
2. **전환 시 default 를 미리 채운다.** 새 kind 가 선언한 필드 중 값이 비어 있고 `default` 가 있는 것은 전환하는 순간 그 값으로 채워 보여 준다. 확인 시 `set_kind` 가 채울 값과 같으므로, 사용자가 보는 값이 저장될 값이다.
3. **Kind autofocus 는 없다.** 포커스를 받아도 키보드로 열 수 없는 위젯이라 이득이 없다. `Esc` 는 select popup 이 열려 있으면 화면을 닫지 않고 popup 만 닫게 둔다.
4. **저장이 실패하면 화면과 draft 가 남는다.** 확인은 캐시된 layout 의 사본에 draft 를 적용하고 `persist_layout` 이 성공했을 때만 그 사본을 캐시에 반영하고 화면을 닫는다. 실패하면 `tracing::warn!` 한 줄과 에러 toast(`preset.toast.save_failed`)를 남기고 캐시·draft·화면을 그대로 둔다 — 사용자는 원인을 고친 뒤 같은 draft 로 다시 확인할 수 있다. 시안의 노트도 같은 동작을 적는다.
5. **더블클릭 진입은 split 존 밖에서만이다.** 칸 가운데 더블클릭은 설정 화면을 연다. 경계 split 존 위의 더블클릭은 기존대로 분할을 한 번 더 한다(빠른 연속 split). 설정 화면으로 가는 다른 길(선택된 칸의 톱니 핸들)은 그대로 있다.

## Consequences

- **얻은 것**: kind 를 둘러보다 되돌려도 입력한 값이 사라지지 않는다. 설정 화면 경로도 편집기의 기존 왕복 계약(선언되지 않은 params 보존)을 지킨다. 저장 실패가 입력을 날리지 않는다. 경계 존의 분할 동작이 새 진입 경로에 가려지지 않는다.
- **잃은 것**: 시안과 동작이 갈린다 — draft 안에는 지금 kind 가 쓰지 않는 값이 숨어 있을 수 있다(확인 전까지는 저장되지 않는다). 키보드만으로 Kind 를 고르는 길이 없다. 경계 존 위에서는 더블클릭으로 설정 화면을 열 수 없다.
- **운영 비용 / 유지 부담**: 확인 경로에 kind 가 같을 때와 다를 때 두 갈래가 있다. `surface_draft.rs` 의 단위 시험 `draft_kind_roundtrip_preserves_original_params` · `draft_apply_same_kind_keeps_unknown_params` · `draft_apply_kind_change_runs_cleanup_once` · `dirty_ignores_undeclared_params_and_sees_declared_ones` 가 1 과 그 두 갈래를 고정한다. 시안과 다르게 둔 자리의 현재 목록은 `docs/design/systems/design-parity-notes.md` 의 "preset 편집기 surface 설정 화면" 절이 들고 있다.

## Alternatives Considered

- **시안의 `switchKind` 를 그대로 옮긴다(전환마다 새 kind 가 선언한 키만 남긴다)** — 왕복에서 원래 params 가 사라진다. 요구사항이 그 왕복을 명시했고, 전환은 확인 전의 탐색 동작이라 되돌릴 수 있어야 한다.
- **시안의 `normalize` 처럼 확인 때 surface 를 통째로 갈아 끼운다** — 선언되지 않은 params 를 지워 편집기의 기존 왕복 계약과 어긋난다. 같은 surface 를 구조 편집 경로와 설정 화면 경로가 서로 다르게 다루게 된다.
- **전환 시 빈 필드를 비워 두고 default 는 확인 때만 채운다** — 화면에 보이는 값과 저장되는 값이 달라진다.
- **Kind 에 autofocus 한다** — 받은 포커스로 할 수 있는 일이 없다. select 위젯이 키보드 조작을 얻으면 이 판단은 바뀐다.
- **저장 실패 시 화면을 닫고 toast 만 남긴다** — draft 가 사라져 사용자가 다시 입력해야 한다.
- **칸 전체의 더블클릭으로 연다(시안)** — 경계 존의 빠른 연속 split 이 설정 화면 열기로 바뀐다. 첫 클릭이 이미 분할을 했으므로 두 동작이 한 제스처에 섞인다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

**채널이 붙는 것** — 판정 시점에 레포가 읽을 수 있는 사실이다.

- 공용 `select` 위젯(`tasty_ui_widgets::select`)이 키보드로 열리고 항목을 고를 수 있게 되면 3(autofocus 없음)을 다시 본다.
- 편집기의 선언되지 않은 params 보존 계약(`set_field_writes_param_and_preserves_unknown_params`)이 지워지거나 반대로 바뀌면 1 의 "kind 가 같으면 선언 필드만 덮는다" 를 다시 본다.
- 경계 split 존이 편집 모드의 칸에서 없어지면 5 는 근거를 잃는다 — 칸 전체 더블클릭(시안)으로 돌아간다.

**원리적으로 안 붙는 것** — 사람이 관측해야 한다. 재는 법을 함께 적는다.

- 시안이 kind 전환 규칙을 바꾸거나 이 결정과 같은 동작으로 정리하면 1·2 를 다시 본다. 재는 법: 원격 `gallery/preset_editor.jsx` 의 `switchKind` · `normalize` 를 이 ADR 의 1·2 와 대조한다.
- draft 에 숨은 값 때문에 확인 결과가 예상과 다르다는 보고가 나오면 1 을 다시 본다. 재는 법: 그 보고의 재현 절차가 kind 전환을 포함하는지 본다.

## References

- 운영 문서: [`docs/features/layout-presets/index.md`](../features/layout-presets/index.md) (설정 화면 절) · [`docs/design/systems/design-parity-notes.md`](../design/systems/design-parity-notes.md) ("preset 편집기 surface 설정 화면" 절)
- 코드 근거(현재 위치): `src/adapters/ui/preset/demo_layout/surface_draft.rs` 의 `LeafDraft::switch_kind` · `LeafDraft::is_dirty` · `DemoLayout::apply_leaf_draft`, `src/adapters/ui/preset.rs` 의 확인 갈래(`CfgOutcome::Confirm`), `src/adapters/ui/preset/demo_layout.rs` 의 칸 더블클릭 갈래(`Act::OpenSettings`), `src/adapters/ui/preset/surface_settings.rs`
- 시안: 사이트 사본 `site/vendor/gallery/preset_editor.jsx` (`SurfaceSettings` · `switchKind` · `normalize`)
- 디자인 흐름: [ADR-0510](0510-design-work-flows-from-claude-design-through-gallery-app-and-site.md)
- 선행 결정 없음 — 탐색: `git grep -l -e 'switch_kind' -e 'LeafDraft' -e 'apply_leaf_draft' -e 'set_kind' -e 'demo_layout' -- docs/adr/` (이 ADR 을 쓰기 전 0 건), `git grep -l -i 'preset' -- docs/adr/` 의 결과(인덱스 제외 15 편) 중 `draft` 도 담은 둘은 0063(popup 닫힘 뒷정리의 `preset_apply` 사례)과 0269(단축키 가져오기의 draft → Save)이고, 둘 다 프리셋 편집기의 surface draft 를 정하지 않는다
