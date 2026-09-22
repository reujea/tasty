# 레이아웃 프리셋 (Layout presets)

- **Status**: Implemented
- **주체**: 로컬 사용자 · AI Agent (`preset.*`)
- **ADR**: 없음
- **코드**: `tasty-presets` 크레이트, `~/.tasty/presets/{workspace,tab,pane}/<name>.toml`, `preset.*` 핸들러
- **화면**: PresetView (`View` + `sealed::Sealed` 직접 구현, modeless)

## 목적

Workspace/Tab/Pane 레이아웃과 각 leaf surface 의 초기화 파라미터(kind·cwd·시작 명령어·kind 별 params)를 미리 저장해 재사용한다. [닫힌 항목 복원](../closed-tab-restore/index.md)(인메모리 LIFO)과 달리 **디스크 영구 저장**, 반복 사용 목적.

## 내부 동작

### 세 종류

WorkspacePreset(전체: 상위 레이아웃 + 모든 pane/tab/surface) · TabPreset(단일 탭) · PanePreset(단일 페인: 탭 목록 + 활성 탭). 셋 다 `LayoutPreset` trait 구현(`tasty-presets`).

### surface 영속 id

각 `PresetSurface` 는 **preset 파일 내에서만 고유한** 영속 식별자 `id`(`Option<u32>`, TOML `id = N`)를 갖는다. load→편집→save→재load 를 관통해 같은 surface 를 안정적으로 지목하기 위한 것으로, 향후 surface 단위 복구 커맨드의 타겟(= "preset 이름 + surface id")이 된다.

- **preset-local**: 전역 고유성은 요구하지 않는다(uuid 불요). `duplicate_preset` 복제본은 같은 id 집합을 그대로 갖는 것이 옳다.
- **하위호환·마이그레이션**: 구버전 TOML 에는 `id` 가 없다. `serde(default)` 로 결손을 허용하고, `LayoutPreset::normalize_surface_ids` 가 로드/저장 시 결손·중복 id 를 high-water mark 이후 번호로 **파일 전체 단위**(Workspace 는 모든 pane·tab 통합)로 결정적 재부여한다. 로드 시 정규화가 무언가 바꾸면 디스크에 되써 마이그레이션을 영속화한다(RO 파일시스템 등 되쓰기 실패는 로그만 남기고 메모리 정규화는 유지 — 멱등).
- **런타임 id 와 무관**: apply 는 적용 시 런타임 surface id 를 새로 발급하며 이 영속 id 를 쓰지 않는다. 편집기(`DemoLayout`)는 leaf 에 영속 id 를 그대로 채택하고(세션 재부여 없음), 신규 leaf 만 새 id 를 받는다 — split/remove/탭 추가 후에도 기존 surface 의 id 는 불변.

### 저장 / 편집

저장: 사이드바 워크스페이스 카드 우클릭 · 탭 타이틀/탭바 빈 공간 우클릭 · 도구 메뉴 "프리셋". 위치 `~/.tasty/presets/{kind}/<name>.toml`(파일명 = 정본, 같은 kind 내 중복 불가 — 충돌 시 `-N` suffix).

캡처 시 **deferred(미복원) 터미널 탭** — PTY 가 아직 spawn 되지 않아 트리에서 `EmptySurface { deferred_spawn: Some(..) }` placeholder 로 있는 비활성 탭 — 도 `kind="terminal"` + `cwd`(`DeferredSpawn.working_dir`)로 캡처된다. (`EmptySurface::kind()` 는 항상 `"empty"` 라, 캡처 경로가 `is_deferred()` 가드로 가로채 layout 영속화(`SavedSurface::capture_surface`)와 동형으로 처리한다.) 적용 시 빈 패널이 아니라 해당 cwd 의 터미널로 복원된다. PTY 가 한 번도 안 뜬 placeholder 는 세션 데이터(restore_command·scrollback)가 없으므로 cwd 만 옮긴다. convert 버튼만 보이는 진짜 빈 패널(비-deferred `EmptySurface`)은 그대로 `kind="empty"` 로 캡처된다.

**PresetView**(`View` + `sealed::Sealed` 직접 구현, modeless, 종류별 1 인스턴스 — [hierarchy](../../concepts/hierarchy.md))는 L1 scope 탭(Workspace/Tab/Pane) 아래 2-depth list→detail 본문이다:

- **좌측 리스트**(196px): 현재 scope 의 저장된 preset 목록. row = 이름 + mono subtitle(workspace 는 저장된 subtitle, 없으면 pane/tab 개수 / tab·pane 은 surface·tab 개수). 선택 row 는 `surface-active` 채움 + 2px accent 좌측 bar. 헤더에 `N presets` + New preset(`+`) 버튼(현재 레이아웃 capture 가 아니라 terminal 1개짜리 최소 preset 생성 — 본문 capture 경로는 컨텍스트 메뉴 저장이 담당). 빈 scope → "저장된 프리셋이 없습니다.".
- **우측 detail**: 44px 툴바(좌: preset 이름+subtitle / 우: rename·duplicate·delete 아이콘 + Edit 버튼) 위에 선택 preset 의 **데모 레이아웃 미리보기**(상위 pane split = 카드+gap, 하위 surface split = hairline, leaf = kind 아이콘 + kind명 + 값 요약, mini-tab 클릭 전환 — 구조·구성만, 라이브 내용 렌더 없음). 툴바 rename·duplicate·delete 는 store 에 직결돼 즉시 동작(rename 은 인라인 입력).
  - **leaf 값 요약**: leaf 는 (선택 여부와 무관하게) 가운데 kind 아이콘·kind명 아래에 설정값을 `키 값` 한 줄씩(중앙 정렬, mono) 요약한다. 라벨은 소문자 필드 키(`cwd`·`startup`·`file`·`url` — 설정 화면의 번역 필드 라벨이 아님), 대상 필드는 kind 의 선언 필드(`SurfaceKindRegistry`/fallback, kind 하드코딩 없음)를 순회해 **값이 비지 않은 것만**. path-like 키(`cwd`·`file`)는 앞자름(경로 꼬리 유지), command/url 키(`startup`·`url`)는 뒤자름. 색은 `preset-leaf-label-fg`(text-muted)/`preset-leaf-value-fg`(text-secondary). **degrade**: 박스 <96×72 → 요약 숨김(아이콘+kind명), 짧은 축 <46 → kind명도 숨김(아이콘만). 선택된 leaf 도 같은 요약을 보인다 — 칸 안에는 잘릴 수 있는 폼을 그리지 않는다.

#### WYSIWYG 편집 모드 (Edit 버튼)

툴바 **Edit** 버튼을 누르면 같은 미리보기 영역이 그 자리에서 **구조**를 편집하는 WYSIWYG 모드로 전환된다. Edit 는 primary **Done** 으로 바뀌고, 옆에 "자동 저장됨" 안내가 표시된다 — 구조 편집에는 **별도 Save 버튼 없음**. 구조 변경(split·제거·탭·pane)은 `PresetStore::save_*_overwrite` 로 즉시 디스크에 write-through 된다(기존 preset 의 메타데이터는 보존하고 레이아웃 트리만 교체). surface 의 **파라미터**(kind · 작업 디렉터리 · 시작 명령어 · kind 선언 필드)는 칸 안이 아니라 아래 **surface 설정 화면**에서 draft 로 고치고 확인해야 저장된다.

- **다른 곳에서 바뀐 preset 은 덮지 않는다**: 편집 화면은 preset 을 열 때 한 번 읽어 둔 트리로 저장한다. 그 사이 저장소의 레이아웃이 바뀌었으면(에이전트의 IPC `preset.save` 등) 구조 편집 자동 저장도, 설정 화면 확인도 **쓰지 않는다** — 그 변경(설정 화면이면 draft 전체)을 버리고, 저장된 판을 다시 불러와 미리보기에 보이며, 경고 toast("다른 곳에서 프리셋이 바뀌어 변경을 저장하지 않고 저장된 내용을 다시 불러옴")를 띄운다. 설정 화면은 닫히고 선택은 풀린다. 이름·subtitle 같은 메타만 바뀐 쓰기는 덮이지 않으므로 막지 않는다. 편집 모드에서는 판정을 저장하는 순간에만 한다 — 저장하기 전까지 편집 중인 미리보기는 옛 트리를 보일 수 있다([ADR-0531](../../adr/0531-the-preset-editor-does-not-overwrite-a-preset-changed-behind-its-cache.md)). **보기 모드**(Edit 를 누르기 전)의 미리보기는 저장소를 따라간다 — 저장소의 레이아웃이 바뀌면 다음 프레임에 저장된 판으로 다시 그리므로, 에이전트가 저장한 뒤 Edit 를 누르기 전에 보기 모드가 한 번이라도 다시 그려졌다면 이어진 편집 모드의 첫 편집은 경합이 되지 않는다. 다시 그릴 때 toast 는 뜨지 않고 선택한 preset·모드는 그대로이며, 미리보기에서 눌러 둔 mini-tab 만 저장된 active 탭으로 돌아간다. 에이전트의 저장은 프리셋 창을 다시 그리게 하지 않으므로(IPC 처리 뒤의 다시 그리기 요청은 메인 창에만 간다 — 프리셋 창의 포커스 여부와 무관하다), 저장 뒤 그 창의 첫 프레임이 Edit 가 눌린 프레임이면 — 예컨대 press 와 release 가 한 이벤트 배치에 들어와 그 뒤에 프레임이 한 번만 그려지거나, 키보드 포커스로 Edit 버튼을 Space/Enter 로 누르면 — 편집 모드는 옛 판으로 시작하고, 그 첫 구조 편집은 위의 경합 판정대로 버려지며 경고 toast 가 뜬다. 이것은 프리셋 창이 포커스인 상태에서도 닿는다. 보통은 포인터 이동·press 프레임이 보기 프레임을 먼저 만들어 드물고, 위 두 경우가 어느 플랫폼에서 실제로 일어나는지는 GUI 로 재지 않았다. 저장소의 레이아웃이 그대로면 보기 모드 미리보기도 다시 짓지 않는다. 편집 모드의 미저장 draft 는 `surface_cfg` 에 따로 살아 캐시 교체와 무관하다 — 그 성질 자체를 재는 시험은 없고 이 기제로만 고정돼 있다([ADR-0564](../../adr/0564-the-preset-view-mode-follows-the-store-and-the-edit-mode-does-not.md)).

- **surface 선택**: 편집 모드에서는 모든 surface 가 1px hairline 윤곽을 얻고, 칸 가운데를 한 번 클릭하면 **선택만** 된다 — 2px accent inset 윤곽 + 우측 상단 핸들 **둘**: **설정(톱니)** · **remove(제거)**. 선택은 아래 표준 단축키의 대상이라 한 번 클릭으로는 미리보기를 떠나지 않는다. 톱니를 누르거나 칸을 **더블클릭**하면 그 surface 의 설정 화면이 열린다. split-right/split-down 핸들은 아래 **경계 hover-split 존**이 대체해 제거됐다. 마지막 한 장 남은 surface 제거는 무효(트리에 0-surface 탭을 쓰지 않음).
- **경계 hover-split 존 (마우스)**: 선택되지 않은 surface 의 4변 바깥 30% 밴드를 hover 하면 accent 22% 밴드 + 안쪽 변 2px accent 55% 분할선 overlay 가 뜨고 커서가 crosshair 로 바뀐다. 클릭하면 그 변으로 split 된다 — **좌/우 존 = 좌우(row) split, 상/하 존 = 상하(column) split**, **좌·상 존은 새 surface 가 first(좌/상)**, 우·하는 second. 축 길이가 46px 미만이면 그 축 밴드는 소멸(중앙 선택은 항상 가능). 선택된 surface 에서는 존이 뜨지 않는다(배경 클릭으로 선택 해제 후 가능). 기존 surface id 는 보존되고 새 surface 만 새 id 를 받는다.
- **탭 삭제 `×` (마우스)**: 편집 모드에서 탭이 2개 이상인 pane 의 active/hover 탭 우측에 14×14 close `×` 가 노출된다(탭 1개면 숨김 + no-op — pane 은 항상 탭 ≥1). 클릭하면 그 탭이 삭제되고 active 인덱스가 재클램프된다.
- **surface 설정 화면**: 오른쪽 detail 컬럼 **전체(툴바 + 미리보기)** 를 대신하는 세 상자다.
  - **헤더**(높이 `preset-cfg-header-height` 44 — 대체되는 툴바와 같다): kind 아이콘(kind accent) · kind 표시명 · mono breadcrumb `preset › 페인 N › 탭 › 서피스 k`(`페인 N` 은 Workspace scope 에서만, 탭 이름은 Tab scope 가 아닐 때만). 좁으면 breadcrumb 이 말줄임된다. 오른쪽 끝에는 draft 가 저장본과 다를 때만 unsaved 표시(6px `preset-cfg-draft-fg` 점 + 캡션, tooltip "초안 — 확인을 누르면 적용됩니다"). back 버튼은 없다 — 나가는 길은 취소다.
  - **본문**: 유일하게 스크롤되는 상자. padding `preset-cfg-form-padding`(16), 한 열 폼(최대 폭 `preset-cfg-form-max-width` 460, 필드 간격 `preset-cfg-field-gap` 12). 순서는 **종류**(Select) → 그 kind 가 선언한 필드(라벨 위, 입력 전체 폭). `dir`/`file_path` 필드는 mono 입력과 **찾아보기** 버튼(secondary, folderOpen 아이콘)을 한 줄에 둔다. 칸 크기와 무관하게 모든 필드가 온전히 보인다.
  - **footer**: 높이 고정 `preset-cfg-footer-height`(52), 상단 1px separator, 오른쪽 정렬 `[취소 ghost] [확인 primary]`(`button.cancel` / `button.ok`). **변경이 없으면 확인은 비활성**이다 — 변경 판정은 kind 와 현재 kind 가 선언한 키만 본다.
  - **draft**: 값은 draft 에만 쓰이고 트리는 확인 전까지 바뀌지 않는다. draft 안에서 kind 를 바꾸면 필드 목록이 바로 바뀐다 — 값은 지우지 않으므로 두 kind 가 함께 선언한 키(예: `cwd`)는 이어지고, 원래 kind 로 돌아오면 원래 값이 다시 보인다. 새 kind 필드 중 비어 있고 `default` 가 있는 것은 그 값으로 채워 보인다.
  - **확인**: surface 를 한 번에 교체하고 곧바로 저장한다. kind 가 원본과 같으면 선언 필드만 덮어써 **선언되지 않은 params 를 보존**하고, 다르면 kind 전환 정리 규칙(새 kind 가 안 쓰는 전용 컬럼·params 제거 + default 채움)을 **확인 시점에 한 번** 적용한다 — 중간에 거친 kind 때문에 원본 params 가 지워지지 않는다. 저장이 성공해야 미리보기로 돌아오고 그 칸은 **선택된 채** 남는다. **저장 실패** 시 설정 화면과 draft 를 그대로 두고 표준 에러 toast("프리셋 저장 실패")를 띄운다. 화면이 열린 사이 저장소의 레이아웃이 바뀌었으면 위 "다른 곳에서 바뀐 preset 은 덮지 않는다" 대로 저장하지 않는다.
  - **취소**: draft 를 버리고 미리보기로 돌아온다(칸 선택 유지, 디스크 무변경).
  - **키**: `Esc` = 취소(Kind 드롭다운이 열려 있으면 그 드롭다운만 닫는다), 한 줄 입력 안의 `Enter` = 확인(변경이 있을 때만). 다른 popup 과 같이 코드에서 직접 읽는 고정 대화상자 키다 — `KeybindingSettings` 항목이 아니다.
  - **열려 있는 동안**: 왼쪽 preset 리스트와 L1 scope 탭은 `preset-cfg-dim-opacity`(= disabled opacity)로 흐려지고 입력(클릭·hover·스크롤)을 받지 않는다. "자동 저장됨"·Done 은 툴바째 사라진다. 구조 편집 단축키는 동작하지 않는다(미리보기를 그리지 않는다). 창 밖에서 선택이 바뀌거나(컨텍스트 메뉴 "…프리셋으로 저장") 창을 닫으면 draft 는 취소와 똑같이 버리고 묻지 않는다.
  - **kind 드롭다운은 `SurfaceKindRegistry` 를 진실 소스로 삼는다** — 편집기(`PresetView`)가 main engine 의 공유 `surface_registry` Arc 를 받아 프레임마다 스냅샷(`KindCatalog`)을 파생한다. 후보 목록은 런타임 등록 kind(플러그인 on/off)를 즉시 반영하고, 표시명은 registry 의 `display_name_i18n_key` 로 해석한다. `empty` 는 사용자가 직접 만들 수 없는 내부 kind 라 후보에서 제외한다. 설정 화면이 연 leaf 의 저장본 kind(와 draft 의 kind)가 목록에 없으면(비활성 플러그인 등) 유실 방지로 덧붙는다 — 꺼진 plugin kind 로도 되돌아갈 수 있다. registry 미주입(main window 부재 등)이면 정적 fallback 목록(`terminal`/`markdown`/`image`/`explorer`/`html`)으로 graceful 하게 떨어진다.
- **이름/subtitle 인라인 편집**: 편집 모드에서 툴바의 preset 이름은 텍스트 입력으로, subtitle 은 (Workspace 한정 실제 필드일 때) 입력으로 바뀌어 포커스 해제 시 store 에 commit 된다.
- **트리 변형**: 편집 모델(`DemoLayout`)은 3계층 전부를 변형한다 — surface split · surface 제거 · 탭 추가(+) · **탭 삭제(×)** · **pane split** · **pane 제거**. 마우스로는 경계 hover-split 존(surface split·4방향·before/after)·remove 핸들(surface 제거)·`+` 버튼(탭 추가)·`×`(탭 삭제)로 트리거되고, 전부(pane split·pane 제거 포함)는 아래 **표준 단축키**로도 발화한다. 모든 변형은 기존 leaf/pane id 를 보존하며 자동 저장된다. 무효 가드: 마지막 surface 제거·마지막 탭 삭제(pane 은 항상 탭 ≥1)·루트 단일 pane 제거는 no-op. pane split 은 **Workspace scope 에서만** 유효(Pane/Tab scope 는 pane 트리가 없어 no-op).
- **표준 단축키 (focus 기반)**: 편집 모드에서 본체와 동일한 `KeybindingSettings` 단축키로 편집을 조작한다 — 코드에 키를 하드코딩하지 않고 설정 필드를 그대로 매칭한다(§단축키 정책). 대상은 **현재 선택된 surface(leaf)** 와 그 leaf 가 속한 pane 이다. 선택이 없으면 전부 no-op(임의 대상 조작 금지). 텍스트 입력(이름/subtitle) 포커스 중에는 문자 키가 입력으로 가도록 단축키 매칭을 차단한다. surface 설정 화면이 열린 동안에는 단축키가 동작하지 않는다.

  | 단축키 액션 (`KeybindingSettings`) | 대상 | 동작 |
  |-----|------|------|
  | `split_surface_vertical` / `split_surface_horizontal` | 선택 surface | 좌우 / 상하 분할 |
  | `close_surface` | 선택 surface | 제거(마지막 1장이면 no-op) |
  | `new_tab` | 소속 pane | terminal 탭 추가 |
  | `close_active` | 소속 pane | active 탭 삭제 → **마지막 탭이면 pane 제거로 폴백**(라이브 close_active 의 탭→pane 체인과 동형) |
  | `split_pane_vertical` / `split_pane_horizontal` | 소속 pane | 좌우 / 상하 pane 분할(**Workspace scope 한정**) |
  | `close_pane` | 소속 pane | pane 제거(루트 단일 pane 이면 no-op) |

  구현 위치는 `Act` enum 이 `demo_layout.rs` private 이고 편집 대상 `DemoLayout` 이 egui temp 캐시에만 살기 때문에 winit `handle_event` 가 아니라 egui 렌더 경로(`draw_preview` → `DemoLayout::apply_shortcut`)다. **제약**: double-tap 바인딩(`shift+shift`/`ctrl+ctrl`/`alt+alt`)은 `parse_binding` 이 거부하므로 편집기에서 지원하지 않는다 — 해당 액션에 double-tap 바인딩만 지정한 사용자는 일반 조합 바인딩을 추가로 지정해야 한다. 또 `KeybindingSettings` 스냅샷은 편집 창을 **열 때** 캡처되므로(appearance 주입과 동일), 설정 변경은 창을 다시 열어야 반영된다.

mini-tab strip 은 `tab_bar.rs`, split 라인은 `divider.rs` 위젯을 재사용한다.

### 적용 — 포커스 규칙

- 단축키(`apply_workspace_preset`/`apply_tab_preset`/`apply_pane_preset`, 기본 빈 칸): 적용 popup → 선택 → 새 인스턴스 생성 + **포커스 이동**.
- **CLI/IPC `preset.apply` 는 항상 `focus: false`** — 포커스 독립 원칙. 단축키 호출만 포커스 이동.

terminal 시작 명령어는 PTY ready 직후 stdin 에 한 줄 자동 입력.

## 인터페이스

`preset.{list,get,save,delete,rename,capture,apply}`(`SurfaceRead`/`SurfaceWrite`) — `tasty preset {list,get,save,delete,rename,capture,apply}`. 표 → [reference/api](../../reference/api.md#구조--workspace--pane--tab--surface--split--tree).

`preset.get`/`preset.save` 는 preset 을 JSON 으로 그대로 직렬화/역직렬화하므로 각 surface 의 영속 `id`(위 [surface 영속 id](#surface-영속-id))가 공개 스키마에 자동 노출·왕복된다 — 향후 surface 단위 타겟팅(`--surface-id N`)의 토대다. `save` 로 들어온 결손·중복 id 는 저장 시 정규화된다.

## 관련

- [closed-tab-restore](../closed-tab-restore/index.md) · [layout-persistence](../layout-persistence/index.md) · [work-area](../work-area/index.md)
