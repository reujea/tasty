# ADR-0531: 프리셋 편집 화면은 캐시가 지어진 뒤 바뀐 preset 을 덮지 않고, 저장소 판을 다시 불러와 알린다

- **Status**: Accepted
- **Date**: 2026-09-23
- **Tags**: preset, layout-presets, concurrency, agent-user-separation, surface-settings, toast, adr-0522

## Context

프리셋 편집 화면(`PresetView`)은 선택한 preset 을 저장소(`PresetStore`)에서 한 번 읽어
`DemoLayout` 으로 짓고 egui temp memory 에 `{kind}:{name}` 키로 캐시한다. 그 캐시로 레이아웃을
통째로 갈아 쓰는 자리는 둘이다(`src/adapters/ui/preset.rs` 의 `persist_layout` 호출부 전수).

- 구조 편집 자동 저장(`draw_preview_editing`) — split · 제거 · 탭 · pane 변형마다.
- surface 설정 화면 확인(`draw_settings_detail`) — [ADR-0522](0522-the-preset-surface-settings-draft-keeps-values-across-kind-switches.md) 의 draft 를 캐시 사본에 적용해 저장.

툴바의 이름 바꾸기 · subtitle 편집 · 복제 · 삭제는 캐시를 안 거치고 저장할 때 저장소를 새로 읽는다.

편집 화면과 IPC `preset.save`/`preset.delete` 는 같은 저장소 인스턴스를 공유하지만 캐시는 저장소가
바뀌어도 다시 지어지지 않는다. 그래서 화면이 열린 사이 에이전트가 같은 이름으로 `preset.save` 하면,
사용자의 다음 구조 편집이나 설정 확인이 **낡은 캐시로 에이전트의 레이아웃을 말없이 덮었다**
(메타는 저장소 값을 그대로 두므로 레이아웃만 사라진다). 에이전트 행동과 사용자 행동이 서로를
침범하는 경합이다([정체성 원칙](../identity.md) 1).

## Decision

**캐시를 지을 때와 저장할 때 저장소의 레이아웃 부분을 기준 판으로 떠 두고, 저장 직전에 저장소의 지금
레이아웃과 대조한다. 다르면 쓰지 않고, 캐시를 저장소 판으로 다시 지은 뒤 경고 toast 로 알린다.**

- 기준 판은 저장이 갈아 쓰는 자리만 본다(`LayoutBase` — workspace 의 `layout` · tab 의 `tab.layout` ·
  pane 의 `pane`). 이름 · subtitle 같은 메타만 바뀐 쓰기는 덮이지 않으므로 경합으로 막지 않는다.
- 경합이면 이번 변경(구조 편집 한 번, 또는 설정 화면의 draft)은 버린다. 설정 화면은 닫고, 선택한
  leaf 도 푼다 — leaf id 가 새 트리에서 같은 leaf 라는 보장이 없다.
- 저장이 성공하면 기준 판을 저장 뒤 저장소 값으로 옮긴다 — 이어지는 자기 저장을 경합으로 보지 않는다.
- preset 이 사라졌으면 전처럼 아무것도 쓰지 않는다(되살리지 않는다).
- 경합이 없으면 동작은 이 결정 전과 같다.

## Consequences

- **얻은 것**: 에이전트의 `preset.save` 가 편집 화면에 가려 사라지지 않는다. 사용자는 자기 변경이
  저장되지 않았다는 것을 toast 로 안다.
- **잃은 것**: 경합이 나면 사용자의 그 변경 하나(설정 화면이면 draft 전체)가 버려진다. 다시 해야 한다.
  캐시는 저장 시점에만 대조하므로, 저장하기 전까지 미리보기는 옛 판을 보여 줄 수 있다.
- **운영 비용 / 유지 부담**: 캐시 한 칸에 기준 판 하나. 캐시로 저장하는 자리를 새로 만들면
  `persist_layout` 을 지나게 하고 `Persisted::Conflict` 를 처리한다.

## Alternatives Considered

- **덮을지 묻는 확인 대화상자를 띄운다** — 사용자가 명시적으로 덮기를 고를 수 있지만, 설정 화면 위에
  새 modal 이 필요하다. 새 modal 은 디자인 시안 → 갤러리 specimen 을 먼저 거쳐야 하고(gallery-first ·
  디자인 값은 Claude Design 이 정한다) 이 결함의 범위를 넘는다. 되살릴 조건은 아래 재검토 조건에 둔다.
- **매 프레임 저장소와 대조해 캐시를 늘 새로 짓는다** — 미리보기는 늘 최신이지만, 사용자가 지난
  프레임에 본 트리를 겨냥한 입력(선택 leaf 기준 단축키)이 다른 트리에 적용될 수 있고, 열린 draft 의
  leaf id 도 흔들린다. 경합 판정이 결국 따로 필요하다.
- **사용자 변경을 저장소 판 위에 다시 적용한다(병합)** — 구조 편집은 leaf/pane id 기준이라 다른
  트리 위에서의 의미가 정의되지 않는다.
- **전체 preset 을 대조한다** — 메타만 바꾼 에이전트 쓰기까지 거절해, 덮지도 않을 쓰기 때문에 사용자
  변경을 버리게 된다.

## Reconsideration Triggers

**채널이 붙는 것** — 판정 시점에 레포가 읽을 수 있는 사실이다.

- 갤러리에 "덮어쓸지 묻는" 확인 대화상자 specimen 이 생긴다(`crates/tasty-gallery` 의 preset 편집기
  카탈로그). 그때 버리는 대신 사용자가 고르게 할지 다시 본다.
- 저장소가 판 번호(revision)나 변경 시각을 싣게 된다. 그때 레이아웃 값 대조를 그 값 대조로 바꿀지 본다.

**원리적으로 안 붙는 것** — 사람이 관측해야 한다. 재는 법을 함께 적는다.

- 경합으로 draft 를 잃는 일이 잦다는 보고. 재는 법: 이슈·사용자 보고, 로그의
  `changed behind the editor` 경고 줄 빈도.

## References

- 선행 결정: [ADR-0522](0522-the-preset-surface-settings-draft-keeps-values-across-kind-switches.md) (설정 화면 draft · 확인 저장 — 다른 조항, 그대로 유효)
- 코드 근거(결정이 실현된 현재 위치): `src/adapters/ui/preset.rs` 의 `persist_layout` · `Persisted` · `reload_after_conflict` · `DemoCache`, `src/adapters/ui/preset/layout_base.rs` 의 `LayoutBase`, 시험 `src/adapters/ui/preset/persist_tests.rs`
- 후속 결정: [0564](0564-the-preset-view-mode-follows-the-store-and-the-edit-mode-does-not.md) (보기 모드 미리보기는 저장소를 따라간다 — "잃은 것" 의 미리보기 부분 중 보기 모드를 닫는다, 이 결정의 편집 모드 조항은 그대로)
- [`docs/features/layout-presets/index.md`](../features/layout-presets/index.md) "편집 모드"
