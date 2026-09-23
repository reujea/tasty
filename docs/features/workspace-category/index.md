# 워크스페이스 카테고리 (Workspace Category)

- **Status**: Done
- **주체**: AI Agent (CRUD·소속 변경, IPC/CLI 양면) · 로컬 사용자 (사이드바 그룹·전환·생성/이름변경/삭제, 토글 on 시)
- **ADR**: [ADR-0029](../../adr/0029-workspace-category-global-index.md)
- **코드**: `crates/tasty-model/src/workspace_category.rs` · `src/core/state.rs` (`categories` / CRUD 메서드 / `set_category_collapsed`) · `src/core/layout_persistence/{schema,capture,restore}.rs` · `src/adapters/ipc/handler/workspace_category.rs` · `crates/tasty-cli/src/commands/workspace_category.rs` · `src/adapters/ui/sidebar/{view,full,collapsed}.rs` (사이드바 그룹·헤더/레일 키캡) · `src/adapters/ui/switch_overlay.rs` (`SwitchTarget::Category` 판정·`category_switch_held`) · `src/adapters/ui/input/shortcuts/numeric.rs` (카테고리 조합+숫자 → `switch_to_category`) · `src/state/workspace.rs` (`switch_to_category` / `category_last_active` / `relative_workspace_in_active_category` 의 경계 넘기) · `crates/tasty-settings/src/general.rs` (`workspace_switch_crosses_category`) · `src/adapters/ui/input/shortcuts/modifier_hint.rs` (`HintRole::CategorySwitch`) · `crates/tasty-settings/src/keybindings.rs` (`category_switch_modifier` / `category_switch_slot_keys`) · `src/adapters/ui/popup/{rail_category,confirm_delete_category}.rs` · `src/adapters/ui/{dialog,category_actions}.rs` · `src/view/main/redraw.rs` (컨텍스트 메뉴) · `src/state/preset_apply.rs`(`apply_workspace_preset` 의 `category` 파라미터) · `src/adapters/ui/popup/preset_apply.rs`(`preset_apply_target_category` 소비/리셋)
- **화면**: 설정 토글 on 시 사이드바가 카테고리 섹션으로 그룹 렌더. 확장 사이드바는 밴드형 헤더(bg-app 면 + 상/하 hairline, chevron+대문자 캡스 라벨(secondary)+우측 워크스페이스 카운트, 접힘 토글)+소속 행, 축소 레일은 카테고리 경계 `---` 버튼+우측 앵커드 팝업. 우클릭 컨텍스트 메뉴(헤더/배경/행)와 레일 팝업으로 생성/이름변경/삭제/카테고리 이동. 드래그로 다른 카테고리 이동. **카테고리 modifier 조합(`category_switch_modifier`, 기본 Ctrl+Shift) 홀드** 시 카테고리 헤더 우측(확장)·`---` 경계 중앙(레일)에 숫자 키캡이 뜨고, 숫자로 그 카테고리로 전환한다. 갤러리 specimen: Layouts › Sidebar & rail(그룹), Overlays › Workspace categories(다이얼로그·레일 팝업), Overlays › Switch-number overlay › Category switch(modifier held — 부제가 기본 조합을 설정 기본값에서 파생해 표기).

## 목적

워크스페이스 수가 늘면 평면 목록이 길어진다. 카테고리는 워크스페이스를 **그룹(사이드바 폴더)** 으로 묶어 정리·전환을 돕는다. 기본은 off — 켜야 사용자 카테고리를 만들 수 있고, 끄면 모든 워크스페이스가 `normal` 로 모여 현행 평면 동작과 동일해진다.

## 내부 동작 (headless-valid)

- **데이터**: 각 `Workspace` 는 소속 카테고리 id(`category`)를 갖는다. `CoreState.categories: Vec<WorkspaceCategory>` 가 카테고리 목록이며 **Vec 순서 = 사이드바 섹션 순서**.
- **`normal` 예약**: id `0`, 이름 `normal`, **`categories[0]` 위치 고정**. rename/delete 불가, 생성 시 이름으로 사용 불가(대소문자 무시). 미지정 워크스페이스의 기본 소속. `ensure_normal_category` 가 생성/복원 직후 이 불변식(0번 고정 + 발급기 floor + dangling 소속 귀속)을 보장한다.
- **이름 규칙**: trim 후 빈 이름 거부, `normal`(대소문자 무시) 예약어 거부, 기존 이름과 대소문자 무시 중복 거부.
- **삭제**: 카테고리를 지우면 그 안의 워크스페이스는 **순서를 보존하며** `normal` 로 귀속한다. 워크스페이스의 전역 인덱스는 불변이므로 사용자 active 는 영향받지 않는다(원칙 1·3).
- **reorder**: `categories` Vec 순서 변경. **from/to == 0 거부**(normal 0번 고정).
- **인덱싱**: 사용자 active 워크스페이스는 전역 인덱스 단일 진실 소스로 유지([ADR-0029](../../adr/0029-workspace-category-global-index.md)). 카테고리-로컬 전환(`switch_workspace_in_active_category`)은 active 카테고리의 로컬 인덱스를 전역 인덱스로 변환해 기존 전환 경로를 재사용한다. `Alt+숫자` 는 토글 on 이면 active 카테고리 내 로컬 전환, off 면 전역 전환(무회귀).
- **워크스페이스 축 next/prev 의 카테고리 경계 넘기 옵션**: "다음/이전 워크스페이스" quick-switch(기본 vim 스타일 `j`/`k`, `next_workspace_in_active_category`/`prev_workspace_in_active_category`)는 기본적으로 활성 카테고리 **로컬 목록 안에서만** wrap-around 한다. 설정 → 일반 → "다음/이전 워크스페이스가 카테고리 경계를 넘음"(`workspace_switch_crosses_category`, 기본 off)을 켜면, 카테고리 마지막 워크스페이스에서 "다음"은 **다음 카테고리의 첫 워크스페이스**로, 카테고리 첫 워크스페이스에서 "이전"은 **이전 카테고리의 마지막 워크스페이스**로 넘어가며 카테고리 목록 자체도 wrap 한다. 착지는 항상 방향에 맞는 첫/마지막 워크스페이스이며, 아래 카테고리 quick-switch 의 **last-active 착지와는 다르다**(방향성 유지가 우선). 카테고리가 1개뿐이면(`workspace_categories_enabled` off 포함) 이 옵션이 on 이어도 기존 로컬 wrap 과 동일하게 동작한다. 이 옵션은 **워크스페이스 축**의 경계 동작이며, 아래의 **카테고리 축** quick-switch(`Ctrl+Shift`+숫자, `next_category`/`prev_category`)와는 독립적인 별개 기능이다 — 두 축을 함께 켜도 서로 간섭하지 않는다.
- **카테고리 quick-switch (카테고리 조합+숫자, 기본 Ctrl+Shift)**: 기존 switch-number 오버레이 축을 재사용한다. 카테고리는 독립 축 `category_switch_modifier`(기본 `ctrl+shift`)를 가지며 그 조합이 `SwitchTarget::Category` 로 판정된다(`switch_overlay::switch_target_for` — 세 축 각각의 조합을 정확 일치 매칭). 워크스페이스 오버레이와 **modifier-exclusive** — 서로 다른 조합이라 동시에 그려지지 않는다(같은 조합을 갖는 상태는 설정 충돌 차단으로 저장 불가). 기본값 `ctrl+shift` 는 macOS 스크린샷 예약(`⌘⇧3/4/5`)과 겹치지 않는다. 번호는 카테고리 순서대로 reserved `normal`("Workspaces")=1, 1–9 then 0(10th), 11번째+ 는 키캡 없음(작동 안 할 숫자는 칠하지 않음). 전환 시 (1) 대상이 접혀 있으면 `set_category_collapsed(false)` 로 **자동 확장**하고 슬롯 파일에 영속, (2) 그 카테고리의 **last-active** 워크스페이스로 착지(방문 이력 없으면 첫 워크스페이스). last-active 는 `AppState.category_last_active: HashMap<WorkspaceCategoryId, usize>` 에 `switch_workspace` 마다 기록된다. 슬롯 키는 `KeybindingSettings.category_switch_slot_keys`(기본 `["1".."9","0"]`). 오버레이 modifier 는 egui raw_input(사용자 키)만 보므로 IPC/CLI 로는 유발 불가(원칙 1). 전 기능은 folders 토글 on 게이트.
- **영속**: 슬롯 파일(`~/.tasty/layouts/NN.json`)에 `categories`(이름·접힘 상태) + 각 워크스페이스의 `category` 가 저장된다. 둘 다 `#[serde(default)]` — 구버전 스냅샷은 카테고리 없이 로드되어 `normal` 단일로 무손실 마이그레이션된다.
- **토글 마이그레이션**: `workspace_categories_enabled` on→off 시 normal 외 모든 카테고리를 제거하고 워크스페이스를 normal 로 귀속한다(전역 인덱스·active 불변).

## 인터페이스

- **AI Agent (IPC/CLI)** — 카테고리 CRUD·reorder·소속 변경은 양면 노출(원칙 2):
  - `workspace_category.list` / `tasty workspace-category list`
  - `workspace_category.create {name}` / `tasty workspace-category create --name X`
  - `workspace_category.rename {id,name}` / `tasty workspace-category rename --id N --name X`
  - `workspace_category.delete {id}` / `tasty workspace-category delete --id N`
  - `workspace_category.move {id,to_index}` / `tasty workspace-category move --id N --to B` — `id` 는 창을 건너 유일해 라우팅이 주인 창을 짚는다. 종전의 `{from_index,to_index}` (`--from A`)도 받지만 그 순번은 창 안의 위치라 **포커스된 창**에 떨어진다([focus](../../design/policies/focus.md)). 둘을 함께 주면 거절한다.
  - `workspace.create` / `workspace.update` 의 `category`(id 또는 이름) 파라미터 — `tasty new/set workspace --category <name|id>`
  - `workspace.list` 응답에 `category` / `category_name`
- **사용자 트리거**: 설정 → 일반 → "워크스페이스 카테고리" 토글. on 이면 사이드바가 카테고리 섹션으로 그룹 렌더되고 **New Workspace(+) 버튼은 확장·축소 양쪽에서 숨긴다**(생성은 카테고리 헤더 메뉴·레일 `---` 팝업의 Add workspace 로 — 배경 우클릭 메뉴는 새 카테고리·원격 워크스페이스 추가뿐 — off 면 + 버튼 표시), 헤더 클릭(접힘 토글)·우클릭 컨텍스트 메뉴(빈 배경 → 새 카테고리/원격 워크스페이스 추가, 헤더 → 워크스페이스 추가(그 카테고리 소속 생성, normal 포함)/프리셋으로부터 워크스페이스 생성(저장된 `WorkspacePreset` 을 그 카테고리 소속으로 적용 — "+" 버튼 메뉴와 동일 팝업 재사용)/원격 워크스페이스 추가/이름변경/삭제/새 카테고리 — normal 은 추가·프리셋 생성·원격 추가·새 카테고리만, 워크스페이스 행 → 카테고리로 이동/새 카테고리)·축소 레일 `---` 팝업(Add workspace/Collapse/Rename/Delete)·드래그 앤 드롭(다른 카테고리로 이동)으로 조작한다. **단축키(`new_workspace`, 기본 `Alt+N`)·Command Palette·더블탭 변형으로 생성**하는 경우도 마우스 경로와 동일하게 카테고리 인지형이다 — `category` 를 고정값으로 넘기지 않고 현재 활성 워크스페이스가 속한 카테고리를 그대로 계승한다(parked 상태, 워크스페이스 0개일 때만 normal fallback). 토글 off 면 활성 워크스페이스가 항상 normal 이라 결과가 이전과 동일하다(회귀 없음). "원격 워크스페이스 추가"는 토글 off 인 배경 우클릭(flat 모드)·+ 버튼 우클릭에도 동일하게 뜨고, 워크스페이스 카드 우클릭에는 없다 — 자세한 배치 근거는 [`features/remote-attach/`](../remote-attach/index.md#원격-워크스페이스-추가-팝업-gui-picker--사용자-경로) 참고. 생성/이름변경은 360px 단일필드 다이얼로그(라이브 검증), 삭제는 destructive confirm 을 거친다.
- **전체 접기/펴기 단축키**: `KeybindingSettings.toggle_categories_collapsed`(기본 빈 binding — 사용자가 Settings › Keybindings 에서 지정). 하나라도 펼쳐져 있으면 전부 접고, 전부 접혀 있으면 전부 편다(normal 포함, `CoreState::toggle_all_categories_collapsed`). 카테고리 토글 off 면 매칭·consume 하지 않아 키가 다른 binding 으로 흐른다. Command Palette 파리티는 `dispatch_action_by_id("toggle_categories_collapsed")`.

## 비-목표 (Out of scope)

- **선택(active) 카테고리 변경·접힘 토글** 은 사용자 UI 상태 — IPC 에 노출하지 않는다(원칙 1·3). 카테고리 IPC 의 어떤 부수효과도 사용자 active/포커스를 바꾸지 않는다.
- 카테고리 자동 삭제 없음(빈 카테고리 허용).

## Acceptance Criteria

- Given 토글 off When 구버전 레이아웃 스냅샷 로드 Then 모든 워크스페이스가 `normal` 로 귀속(무손실).
- Given `workspace_category.create {name:"normal"}` Then 예약어 거부.
- Given 카테고리 A 에 워크스페이스 존재 When `workspace_category.delete A` Then 워크스페이스는 normal 로 이동, active 전역 인덱스 불변.
- Given `workspace_category.move {from_index:0}` 또는 `{to_index:0}` Then 거부(normal 0번 고정).
- Given 토글 on Then 사이드바가 카테고리 섹션(chevron 헤더 + 소속 행)으로 그룹 렌더, 토글 off 면 평면 렌더(회귀 없음).
- Given 카테고리 헤더 클릭 When 접힘 토글 Then 접힘 상태가 슬롯 파일에 영속되고 확장↔레일이 공유.
- Given `toggle_categories_collapsed` 바인딩 When 하나라도 펼쳐진 상태에서 누름 Then 전부 접힘, 다시 누르면 전부 펴짐(normal 포함). 카테고리 토글 off 면 no-op(키 흐름).
- Given 워크스페이스를 다른 카테고리 섹션으로 드래그 Then 소속이 그 카테고리로 변경(전역 인덱스 불변).
- Given 카테고리 생성/이름변경 다이얼로그 When 빈/normal/중복 입력 Then 인라인 danger 에러 + 확인 비활성.
- Given 카테고리 삭제 When destructive confirm 확인 Then 카테고리 제거 + 워크스페이스 normal 귀속.
- Given 토글 on When 카테고리 조합(기본 Ctrl+Shift) 홀드 Then 카테고리 헤더(확장)·`---` 경계(레일)에 숫자 키캡, 워크스페이스 행은 status dot 유지(modifier-exclusive). 토글 off 면 키캡·역할 없음.
- Given 카테고리 조합+숫자 When 대상 카테고리가 접힘 Then 자동 확장(슬롯 파일 영속) 후 그 카테고리 last-active 워크스페이스로 전환, 방문 이력 없으면 첫 워크스페이스.
- Given folders on When modifier-hint 패널에서 카테고리 조합 홀드 Then "카테고리 전환" 역할 행(폴더 글리프) 노출, off 면 없음.
- Given `workspace_switch_crosses_category` off(기본) Then 워크스페이스 next/prev 는 카테고리 로컬 wrap 유지(회귀 없음).
- Given `workspace_switch_crosses_category` on Then 카테고리 마지막 워크스페이스에서 next → 다음 카테고리 첫 워크스페이스, 첫 워크스페이스에서 prev → 이전 카테고리 마지막 워크스페이스로 이동하며 카테고리 목록도 wrap.
- Given `workspace_switch_crosses_category` on When 카테고리가 1개뿐(`workspace_categories_enabled` off 포함) Then off 일 때와 동일하게 로컬 wrap.
