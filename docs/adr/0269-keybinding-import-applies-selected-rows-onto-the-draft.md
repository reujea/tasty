# ADR-0269: 단축키 가져오기는 고른 행을 draft 에 얹고, 번들에 없는 plugin override 는 건드리지 않는다

- **Status**: Accepted
- **Date**: 2026-09-13
- **Tags**: keybindings, import-export, settings, draft, plugin, option-migration, conflict

## Context

설정 › 단축키 › 가져오기/내보내기는 다른 환경에서 내보낸 번들(`crates/tasty-host-plugin/src/keybinding_bundle.rs`)을 읽어 미리보기로 보여 주고, 사용자가 Apply 를 눌러야 draft 에 들어간다. 그 "들어간다" 의 뜻이 여러 갈래로 열려 있었다.

- 번들은 호스트 단축키(`KeybindingSettings`)와 plugin override(`PluginsConfig.keybindings`) 두 원본을 함께 싣는다. 설정 창의 draft 도 둘이다 — settings draft 와 `plugin_shortcuts_draft`.
- 번들을 만든 환경에는 없던 plugin 이 이 환경에 있을 수 있다. 번들에 그 plugin 의 override 가 없다는 것은 "지워라" 인가, "모른다" 인가.
- 비-macOS 에서 `option` 이 든 바인딩은 매칭되지 않으므로 대체 조합을 정해야 한다(`keybinding_bundle::option_migration`). 대체 값은 새 충돌을 만들 수 있고, 사용자는 어떤 자리를 이 환경에서 비워 두고 싶을 수도 있다.
- `plugin_shortcuts_draft` 는 모달이 닫힐 때 Save/Cancel 구분 없이 회수돼 적용되고 있었다. 가져오기가 그 draft 에 대량으로 쓰면 Cancel 이 반쪽이 된다.

## Decision

**가져오기 Apply 는 미리보기 표에서 고른 행만 두 draft 에 쓰고, 디스크 커밋은 footer Save 가 한다.**

- **행이 적용 단위다.** 일반 콤보는 필드 하나(그 필드의 콤보 목록 전체), quick-switch 는 축 하나(modifier · 슬롯 전부 · 다음/이전), 스크립트는 스크립트 하나, plugin 은 명령 하나. 축을 슬롯 단위로 쪼개지 않는 이유는 슬롯이 raw 키라 modifier 와 떨어지면 뜻이 바뀌기 때문이다.
- **스크립트 행은 현재와 번들의 합집합이다.** 번들에 없는 현재 스크립트 바인딩은 적용하면 사라지는 행으로 보인다 — 스크립트는 이 환경의 레지스트리에 있는 것만 번들에서 살아남으므로(모르는 스크립트는 decode 가 버린다) 양쪽이 같은 대상을 가리킨다.
- **plugin 행은 번들에 있는 명령만이다.** 이 환경에만 있는 override 는 표에 오르지도 않고 바뀌지도 않는다.
- **option 마이그레이션은 번들 전체에 대해 요구한다** — 선택 여부와 무관하게 미해결이 하나라도 있으면 Apply 가 비활성이다. 자리마다 대체 조합을 정하거나 **비워 둘 수 있고**, 비워 두기는 해소로 센다. 축 modifier 는 비울 수 없다(조합 하나를 반드시 갖는다).
- **충돌은 해소된 번들 구성 안에서 판정한다**(`resolve_migration` 의 전후 차분). 새 충돌이 있으면 설정 창의 단축키 충돌 확인을 띄우고, 수락하면 충돌 상대 중 **계획 밖의 자리**를 비운다(`ConflictPolicy::UnbindOther`). 대체 값끼리 겹치면 비울 쪽을 정할 수 없어 수락해도 적용되지 않는다.
- **`plugin_shortcuts_draft` 는 Save 로 닫혔을 때만 회수된다.** Cancel 은 그 draft 를 비우고, 창 닫기·설정 토글 키로 닫힌 경우는 회수 시점에 버린다.

## Consequences

- **얻은 것**: Apply 가 Preset 과 같은 2 단계 계약(draft → Save)을 지킨다. 한 환경에만 설치된 plugin 의 설정이 다른 환경의 번들 때문에 조용히 지워지지 않는다. Cancel 이 plugin 단축키까지 되돌린다 — Plugins 서브탭에서 편집한 override 도 같은 규칙을 따르게 됐다.
- **잃은 것**: "번들과 정확히 같은 plugin override 집합" 으로 맞추는 수단이 없다 — 이 환경에만 있는 override 는 Plugins 서브탭에서 따로 지워야 한다. 충돌 판정이 번들 구성 안에서만 이뤄지므로, 일부 행만 고른 경우 현재 draft 에 남은 행과의 충돌은 이 화면이 잡지 않는다(기존 편집 화면들의 충돌 확인도 녹화 시점에만 돈다).
- **운영 비용 / 유지 부담**: 행 모델(`src/view/settings/ui/keybindings_tab/import_export/model.rs`)이 번들의 네 자리를 열거한다 — `KeybindingSettings` 에 새 종류의 바인딩 자리가 생기면 코덱·마이그레이션 스캔과 함께 여기도 늘어야 한다.

## Alternatives Considered

- **번들로 통째 교체(선택 없음)**: 디자인이 행 단위 선택을 요구했고, 통째 교체는 이 환경에만 있는 plugin override 를 지울지 말지를 강제로 정해야 한다.
- **번들에 없는 plugin override 를 지운다**: 번들을 만든 환경이 그 plugin 을 몰랐을 뿐인 경우(가장 흔하다)에 사용자 설정을 잃는다. 없음은 정보가 아니다.
- **선택된 행에 대해서만 마이그레이션을 요구한다**: 행과 자리의 대응은 되지만, 고르지 않은 행의 `option` 바인딩은 어차피 적용되지 않으므로 요구하지 않아도 되는 대신 "미해결 수" 가 선택에 따라 오르내려 Apply 비활성의 사유가 흔들린다. 디자인은 카드 카운터와 back bar 카운터를 같은 수로 둔다.
- **충돌을 현재 draft 와 합친 결과로 판정한다**: 선택 조합마다 결과가 달라져 마이그레이션 행에 붙이는 충돌 표시가 선택 토글에 따라 깜빡인다. 번들 안의 판정은 선택과 독립이다.

## Reconsideration Triggers

**채널이 붙는 것** — 판정 시점에 레포가 읽을 수 있는 사실이다.

- 번들 스키마(`BUNDLE_VERSION`)가 올라 새 바인딩 자리를 싣게 되면 행 모델의 네 그룹을 다시 본다.
- 설정 창에 draft 전체에 대한 충돌 검사가 생기면, 부분 선택 시 충돌을 이 화면이 따로 판정할 이유가 사라진다.

**원리적으로 안 붙는 것** — 사람이 관측해야 한다. 재는 법을 함께 적는다.

- 사용자가 "번들과 똑같이 맞추기" 를 반복해서 요구한다. 재는 법: 이슈·피드백에서 이 화면 뒤에 Plugins 서브탭에서 override 를 수동으로 지우는 흐름이 보고되는지.

## References

- `docs/features/keybindings/index.md` — "가져오기 / 내보내기" 절(현재 동작)
- `docs/features/settings/screens/settings.md` — 단축키 L2 목록
- 코드 근거(결정이 실현된 현재 위치): `src/view/settings/ui/keybindings_tab/import_export.rs` 의 `ImportExportState::apply`, `import_export/model.rs` 의 `row_keys` · `apply_rows`, `crates/tasty-host-plugin/src/keybinding_bundle/option_migration.rs` 의 `resolve_migration` · `ConflictPolicy`, `src/view/settings.rs` 의 `SettingsView::take_plugin_shortcut_draft`
- 번들 형식: [ADR-0257](0257-the-keybinding-bundle-is-a-toml-file-with-a-schema-tag.md)
