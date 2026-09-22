# ADR-0043: convert 시 파일 입력이 필요한 kind 를 capability 로 라우팅

- **Status**: Accepted
- **Date**: 2026-07-09
- **Tags**: surface-kind, convert, plugin, de-pluginize, capability, popup, adr-0028, adr-0042
- **Group**: plugin-system

## Context

surface convert 라우팅(`src/adapters/ui/popup/convert.rs`)에는 `match kind { "terminal" => …, "markdown" => …, other => … }` 하드코딩이 있었다. `markdown` 팔은 변환 전 "어느 파일을 열지" 고르는 파일 입력 폼을 먼저 띄워야 해서 즉시 변환하는 generic Kind 와 동작이 달랐고, 그 폼은 host 가 소유한 `markdown_open` egui 팝업이었다.

markdown de-pluginize 작업에서 이 파일 입력 폼은 Phase 3 에 plugin egui-mesh 팝업(`[[contributes.popup]] id = "file-open"`)으로 이관됐다. 그 결과 host 의 `markdown_open` 팝업은 제거됐고, 그것을 열던 opener(convert 팝업, `open_markdown`/`convert_to_markdown` 단축키, context menu)는 죽은 팝업 id 를 가리키게 되어 파일열기/convert-to-markdown 기능이 일시적으로 공백 상태였다.

Phase 4 의 요구:

1. host 가 특정 plugin 이름(`markdown`)이나 event key 로 조건분기하지 않고, **어느 kind 가 convert 시 파일 입력을 요구하는지**와 **그 입력 팝업이 무엇인지**를 generic 데이터로만 판정해야 한다(불가침 원칙: host 는 plugin 이름을 모른다).
2. Phase 3 가 남긴 기능 공백(파일열기/convert-to-markdown no-op)을 plugin file-open 팝업으로 재배선해 복원해야 한다.

host 가 plugin 팝업을 여는 generic API 는 이미 존재한다: `PluginManager::open_popup_instance(plugin_id, popup_id, context)`. 사용자 메뉴(tools_menu)가 `pending_popup_opens: Vec<(plugin_id, popup_id, context)>` 큐에 넣으면 App 메인 루프가 drain 해 dispatch 하는 경로도 성숙해 있다. kind → 소유 plugin_id 매핑은 `SurfaceKindRegistry` 등록 시점(`egui_mesh.rs`/`remote_kind.rs`)에만 알려져 있고, `SurfaceKindDef` 자체에는 plugin_id 필드가 없다.

## Decision

`SurfaceKindDecl`/`SurfaceKindDef` 에 두 capability 필드를 추가한다:

- `convert_requires_input: bool` — 이 kind 로 convert 하려면 host 가 먼저 파일 입력 팝업을 띄워야 하는지. convert 라우팅이 `terminal`(PTY 전용 host 경로) 다음으로 이 플래그를 보고 `ConvertAction::RequiresInput(kind)` 인지 `ConvertAction::Kind(kind)`(즉시 빈 params 변환)인지 판정한다.
- `convert_input_popup: Option<String>` — 그 kind 를 소유한 plugin 의 file-input 팝업 **qualified id** (`"<plugin_id>/<popup_id>"`).

매니페스트에는 plugin 이 **local** popup id(`convert_input_popup = "file-open"`)만 선언하고, host 는 kind 등록 시점(`egui_mesh.rs`/`remote_kind.rs`, 소유 plugin_id 를 아는 유일한 지점)에서 이를 `"<plugin_id>/file-open"` 으로 qualify 해 `SurfaceKindDef` 에 저장한다. convert/open opener 는 `AppState::enqueue_convert_input_popup(engine, kind, convert_surface_id)` 를 호출하는데, 이 helper 는 registry 에서 `convert_input_popup` 을 읽어 `(plugin_id, popup_id)` 로 split 하고 `pending_popup_opens` 에 넣기만 한다 — host 는 kind 이름도 event key 도 하드코딩하지 않고 데이터만 따른다.

새 탭 열기와 제자리 변환은 **동일 팝업**을 open context 의 `surface_id` 유무로 구분한다: `Some(sid)` 이면 plugin 이 `markdown.navigate {surface_id, path}` 로 제자리 변환하고, 없으면 `file_handler.dispatch` 로 새 탭을 연다. plugin 의 `file-open` 팝업은 open context 에서 `surface_id` 를 읽어 `FileOpenState.convert_surface_id` 에 보관하고 [열기] 시 분기한다.

기존 event-key trigger(`com.tasty.markdown.file_open`)도 매니페스트에 남겨 두어, host 이벤트 발행 경로로도 같은 팝업을 열 수 있게 한다(두 진입로 공존).

## Consequences

- **얻은 것**: convert 라우팅에서 `"markdown"` 조건분기 제거(이제 `terminal` PTY 특수경로만 host 하드코딩으로 남고 정당). "파일 입력 필요 + 그 팝업" 이 순수 데이터(capability + qualified popup id)로 표현돼, 다른 plugin kind 도 동일 메커니즘으로 convert-input 팝업을 붙일 수 있다. Phase 3 기능 공백(파일열기/convert-to-markdown) 복원. host 소유 dead 필드(`markdown_convert_surface_id`/`markdown_open_buffer`/`file_open_pane_id`) 제거.
- **잃은 것**: `convert_input_popup` 이 등록 시점에 plugin_id 로 qualify 되는 규약이 `egui_mesh.rs`/`remote_kind.rs` 두 곳에 중복(둘 다 egui-mesh/remote 등록 경로라 불가피). `convert_requires_input` 과 `convert_input_popup.is_some()` 이 사실상 동치라 약간의 표현 중복 — 명시성을 위해 둘 다 유지.
- **운영 비용 / 유지 부담**: 새 plugin kind 가 convert-input 팝업을 원하면 매니페스트에 두 필드 + `[[contributes.popup]]` 만 선언하면 되고 host 코드 변경 불필요.

## Alternatives Considered

- **A: generic convert-input event 를 host 가 publish → plugin popup 이 event-trigger 로 open.** host 가 `emit_host_event("<kind>.convert_input", {surface_id})` 류를 쏘고 plugin 매니페스트가 그 event_key 로 팝업을 trigger 하는 방식. event_key 를 kind 로부터 generic 하게 구성해야 하는데(`format!("{kind}.convert_input")` 같은) 이는 host↔plugin 양쪽에 암묵적 네이밍 규약을 새로 만든다. 또 event 는 fire-and-forget 이라 "이 특정 팝업을 지금 연다"는 직접성이 약하고, surface_id 를 payload 로 실어도 어느 팝업이 소비할지 host 가 보장 못 한다. `open_popup_instance` 직접 호출이 더 명시적이고 기존 tools_menu 패턴과 동일해 채택하지 않음.
- **B: `convert_requires_input` 없이 `convert_input_popup: Option<String>` 단일 필드로 판정.** `is_some()` 이면 입력 필요로 간주. 필드 하나 줄지만, "입력이 필요하다"는 의미론적 capability 와 "어느 팝업을 연다"는 구현 디테일을 한 필드에 뭉쳐 라우팅 판정 코드의 의도가 흐려진다. 다른 capability 플래그들(`records_recent`, `zoomable` 등)과 표현을 맞춰 bool + 참조를 분리.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

- convert-input 팝업을 쓰는 kind 가 3종 이상으로 늘어 qualify 중복(`egui_mesh.rs`/`remote_kind.rs`)이 실질 유지 부담이 될 때 → 등록 공통 helper 로 추출.
- 파일 외의 입력(URL, 다중 필드 등)을 요구하는 convert 가 생겨 "file-open" 단일 팝업 가정이 깨질 때.
- `open_markdown`/`convert_to_markdown` 키바인딩을 plugin command 로 이전(현재 defer)해 host `KeybindingSettings` 의 markdown-특정 필드를 제거할 때 → opener 가 kind 를 데이터로 넘기는 방식 자체가 바뀔 수 있음.

## References

- [ADR-0028: plugin egui-mesh render channel](0028-plugin-egui-mesh-render-channel.md) — plugin 팝업 egui-mesh 렌더 + host 화이트리스트
- [ADR-0042: fs.pick_file native dialog host delegation](0042-fs-pick-file-native-dialog-host-delegation.md) — file-open 팝업의 browse 위임
- `src/core/surface_registry.rs` (`SurfaceKindDef`), `src/core/surface_registry/{egui_mesh,remote_kind... }` — qualify 등록
- `src/state.rs` (`AppState::enqueue_convert_input_popup`), `src/adapters/ui/popup/convert.rs` (`ConvertAction::RequiresInput`)
- `crates/tasty-plugin-markdown/tasty-plugin.toml`, `crates/tasty-plugin-markdown/src/popup.rs` (`FileOpenState`)
