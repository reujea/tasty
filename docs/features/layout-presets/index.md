# 레이아웃 프리셋 (Layout presets)

- **Status**: Implemented
- **주체**: 로컬 사용자 · AI Agent (`preset.*`)
- **설계**: 프리셋 초안과 저장 충돌
- **코드**: `tasty-presets` 크레이트, `~/.tasty/presets/{workspace,tab,pane}/<name>.toml`, `preset.*` 핸들러
- **화면**: PresetView (`View` + `sealed::Sealed` 직접 구현, modeless)

## 목적

Workspace/Tab/Pane 레이아웃과 각 leaf surface 의 초기화 파라미터(kind·cwd·시작 명령어·kind 별 params)를 미리 저장해 재사용한다. [닫힌 항목 복원](../closed-tab-restore/index.md)(인메모리 LIFO)과 달리 **디스크 영구 저장**, 반복 사용 목적.

## 내부 동작

### 세 종류

WorkspacePreset(전체: 상위 레이아웃 + 모든 pane/tab/surface) · TabPreset(단일 탭) · PanePreset(단일 페인: 탭 목록 + 활성 탭). 셋 다 `LayoutPreset` trait 구현(`tasty-presets`).

### surface 영속 id

각 `PresetSurface` 는 **preset 파일 내에서만 고유한** 영속 식별자 `id`(`Option<u32>`, TOML `id = N`)를 갖는다. load→편집→save→재load 를 거쳐도 같은 surface 를 안정적으로 지목하기 위한 것으로, "preset 이름 + surface id"로 저장된 surface를 식별할 수 있다.

- **preset-local**: 전역 고유성은 요구하지 않는다(uuid 불요). `duplicate_preset` 복제본은 같은 id 집합을 그대로 갖는 것이 옳다.
- **하위호환·마이그레이션**: 구버전 TOML 에는 `id` 가 없다. `serde(default)` 로 결손을 허용하고, `LayoutPreset::normalize_surface_ids` 가 로드/저장 시 결손·중복 id 를 high-water mark 이후 번호로 **파일 전체 단위**(Workspace 는 모든 pane·tab 통합)로 결정적 재부여한다. 로드 시 정규화가 무언가 바꾸면 디스크에 되써 마이그레이션을 영속화한다(RO 파일시스템 등 되쓰기 실패는 로그만 남기고 메모리 정규화는 유지 — 멱등).
- **런타임 id 와 무관**: apply 는 적용 시 런타임 surface id 를 새로 발급하며 이 영속 id 를 쓰지 않는다. 편집기(`DemoLayout`)는 leaf 에 영속 id 를 그대로 채택하고(세션 재부여 없음), 신규 leaf 만 새 id 를 받는다 — split/remove/탭 추가 후에도 기존 surface 의 id 는 불변.

### 저장 / 편집

저장: 사이드바 워크스페이스 카드 우클릭 · 탭 타이틀/탭바 빈 공간 우클릭 · 도구 메뉴 "프리셋". 위치 `~/.tasty/presets/{kind}/<name>.toml`(파일명 = 정본, 같은 kind 내 중복 불가 — 충돌 시 `-N` suffix).

저장소는 초기 읽기와 같은 바이트에서 파일·symlink 대상 관측값을 보관한다. 손상·읽기 실패 원본이나 이후 외부에서 바뀐 파일은 저장으로 덮지 않는다. 저장과 로드 시 ID 보정은 같은 디렉터리의 임시 파일 및 플랫폼별 교체를 쓰며 기존 접근 권한을 보존한다. 참여하지 않는 편집기의 최종 확인과 교체 사이 경합까지 막는 계약은 아니다.

`PresetStore::save_command`는 소유된 `OwnedPreset`을 받아 참여자 잠금 안에서 자동 이름을 정하고 저장한 뒤 `PresetChange`를 반환하는 blocking 저장소 경계다. 손상되거나 읽을 수 없어 캐시에 없는 파일도 이름을 점유한다. 별도 캐시에는 성공 결과를 `apply_change`로 메모리 반영할 수 있다. 호스트는 같은 초기 캐시의 worker 전용 복사본으로 이 API를 같은 file writer에서 실행한다. 명령은 합치지 않고 접수 순서로 처리한다. receipt를 받은 후 표시 캐시에 확정 delta를 반영하고 IPC·plugin·원 View에 완료를 전달한다. 접수된 저장은 호출자가 닫혀도 계속한다. 초기 읽기·ID 보정은 load 단계 경계를 유지한다.

이름 변경은 root 참여자 잠금과 원본 대상 잠금을 유지한 채 새 파일을 확정하고 원본을 삭제한다. 직렬화·새 파일 저장 실패로 원본 캐시를 먼저 지우지 않는다. 새 파일을 확정한 뒤 원본 정리 실패 또는 외부 변경을 발견하면 `RenamePartial` 오류를 반환하며 새 이름의 캐시도 보존한다. 이때 재시작하면 두 파일이 모두 보일 수 있다. `rename_command`의 `PresetMutationOutcome`은 부분 실패에도 확정된 새 이름의 `changes`를 싣는다. 별도 캐시는 이를 반영한 뒤 `result`를 전달해야 한다. 두 파일의 transaction이나 실패 시 rollback을 약속하지 않는다. 삭제는 원본 관측값을 검사하고, 삭제 성공 또는 실제 파일 부재를 확인한 뒤 캐시를 지운다. symlink 프리셋 삭제는 링크를 지우며 그 대상 파일은 남긴다.

rename의 새 파일은 원본의 owner·mode·ACL을 payload 쓰기 전에 복사한다(`file_replace::replace_preserving`). 기존 대상 파일이 있으면 그 대상의 접근 권한을 보존하는 공용 교체 정책을 유지한다. 잠금 namespace로 향하는 프리셋 symlink는 초기 읽기·ID 보정·교체 전에 거절하고, root 잠금 anchor가 점유되었거나 잠금 파일이 symlink이면 명령을 거절한다. 잠금 파일을 사용자 프리셋으로 교체하거나 같은 잠금을 재획득하지 않는다.

캡처 시 **아직 실제화되지 않은 탭**은 지금 화면이 아니라 저장된 원본으로 캡처한다. journal 복원을 기다리는 자리(`JournalPlaceholder`)는 고정한 journal 참조(kind·data·creation seed)를 event store에서 풀어 그 surface 값으로 채운다(`PresetCaptureDraft::resolve`). plugin kind 등록을 기다리는 자리(`EmptySurface`의 `deferred`)는 원래 kind와 plugin snapshot으로 캡처한다(`EmptySurface::kind()`는 항상 `"empty"`라 캡처 경로가 먼저 가로챈다). convert 버튼만 보이는 진짜 빈 패널(`deferred` 없는 `EmptySurface`)은 그대로 `kind="empty"`로 캡처된다.

**PresetView**(`View` + `sealed::Sealed` 직접 구현, modeless, 종류별 1 인스턴스 — [hierarchy](../../concepts/hierarchy.md))는 L1 scope 탭(Workspace/Tab/Pane) 아래 2-depth list→detail 본문이다:

- **좌측 리스트**(196px): 현재 scope 의 저장된 preset 목록. row = 이름 + 라벨 글꼴 subtitle(workspace 는 저장된 subtitle, 없으면 pane/tab 개수 / tab·pane 은 surface·tab 개수). 선택 row 는 `surface-active` 채움 + 2px accent 좌측 bar. 헤더에 `N presets` + New preset(`+`) 버튼(현재 레이아웃 capture 가 아니라 terminal 1개짜리 최소 preset 생성 — 본문 capture 경로는 컨텍스트 메뉴 저장이 담당). 빈 scope → "저장된 프리셋이 없습니다.".
- **우측 detail**: 44px 툴바(좌: preset 이름+subtitle / 우: rename·duplicate·delete 아이콘 + Edit 버튼) 위에 선택 preset 의 **데모 레이아웃 미리보기**(상위 pane split = 카드+gap, 하위 surface split = hairline, leaf = kind 아이콘 + kind명 + 값 요약, mini-tab 클릭 전환 — 구조·구성만, 라이브 내용 렌더 없음). 툴바 rename·duplicate·delete는 파일 명령을 접수한다(rename은 인라인 입력).
  - **leaf 값 요약**: leaf 는 (선택 여부와 무관하게) 가운데 kind 아이콘·kind명 아래에 설정값을 `키 값` 한 줄씩(중앙 정렬, mono) 요약한다. 라벨은 소문자 필드 키(`cwd`·`startup`·`file`·`url` — 설정 화면의 번역 필드 라벨이 아님), 대상 필드는 kind 의 선언 필드(`SurfaceKindRegistry`/fallback, kind 하드코딩 없음)를 순회해 **값이 비지 않은 것만**. path-like 키(`cwd`·`file`)는 앞자름(경로 꼬리 유지), command/url 키(`startup`·`url`)는 뒤자름. 색은 `preset-leaf-label-fg`(text-muted)/`preset-leaf-value-fg`(text-secondary). **degrade**: 박스 <96×72 → 요약 숨김(아이콘+kind명), 짧은 축 <46 → kind명도 숨김(아이콘만). 선택된 leaf 도 같은 요약을 보인다 — 칸 안에는 잘릴 수 있는 폼을 그리지 않는다.

#### WYSIWYG 편집 모드 (Edit 버튼)

툴바 **Edit** 버튼을 누르면 같은 미리보기 영역이 그 자리에서 **구조**를 편집하는 WYSIWYG 모드로 전환된다. Edit 는 primary **Done** 으로 바뀌고, 옆에 저장 방식을 설명하는 "자동 저장" 안내가 표시된다 — 구조 편집에는 **별도 Save 버튼 없음**. 구조 변경(split·제거·탭·pane)은 소유된 파일 명령으로 저장한다(기존 preset 의 메타데이터는 보존하고 레이아웃 트리만 교체). surface 의 **파라미터**(kind · 작업 디렉터리 · 시작 명령어 · kind 선언 필드)는 칸 안이 아니라 아래 **surface 설정 화면**에서 draft 로 고치고 확인해야 저장된다.

##### 저장소 변경과 편집 충돌

| 화면 상태 | 외부에서 같은 프리셋을 저장했을 때 |
|---|---|
| 보기 모드 | 다음 그리기 프레임에 레이아웃을 비교하고 달라졌으면 미리보기를 다시 만든다. |
| 보기→편집 전환 | 편집에 들어가기 전 한 번 더 비교한다. 저장 뒤 첫 프레임이 Edit 클릭인 경우도 포함한다. |
| 편집 중 | 캐시와 열린 초안을 유지한다. 저장 직전에만 저장소와 비교한다. |
| 저장 충돌 | 이번 구조 변경 또는 설정 초안을 저장하지 않고 버린다. 저장소 내용을 다시 읽고 경고 토스트를 띄운다. 설정 화면을 닫고 leaf 선택을 해제한다. |
| 일반 디스크 저장 실패 | 캐시·초안·설정 화면을 유지하고 오류를 알린다. 같은 초안으로 재시도할 수 있다. |

충돌 비교는 `LayoutBase`의 레이아웃 부분에 한정한다. 이름·subtitle만 바뀐 경우에는 이를 덮어쓰지 않으므로 충돌로 보지 않는다. 저장이 성공하면 기준 레이아웃도 갱신한다. 저장소에서 프리셋이 삭제되었으면 다시 만들지 않는다.

보기 갱신은 선택한 프리셋·모드·목록 스크롤을 바꾸거나 토스트를 띄우지 않는다. 레이아웃이 같으면 mini-tab 등 임시 보기 상태도 유지한다. 레이아웃이 바뀌면 mini-tab은 저장된 active 탭으로 돌아간다. 새 트리와 연결할 안정적인 pane ID가 없기 때문이다. IPC 저장은 메인 창에 다시 그리기를 요청하므로 프리셋 창이 즉시 새 프레임을 받는다고 가정하지 않는다.

캐시와 이전 모드 기록은 `{kind}:{name}`별로 관리한다. 현재·직전 pass에서 사용한 프리셋만 남겨 방문한 만큼 무한히 쌓이지 않게 한다. 같은 프리셋을 한 프레임에서 보기와 편집 양쪽으로 그리는 조합은 아직 지원 대상으로 정하지 않았다. 같은 캐시가 저장소를 따르면서 편집 내용을 유지할 수는 없으며 현재 호출자도 없다. 설정 초안은 `surface_cfg`에 따로 저장한다.

저장 결과를 기다리는 동안 편집기 입력과 구조 단축키를 막는다. 완료가 확인된 뒤 편집을 계속하며, 일반 실패는 기존 초안·선택·설정 화면을 되살린다. 배치의 앞 변경만 확정됐다면 이를 유지하고 첫 실패를 알린다. 저장이 끝난 View와 동일한 identity에만 완료 표시를 적용한다.

구조 편집 저장과 설정 확인은 모두 `persist_layout`을 거치고 `Persisted::Conflict`를 처리한다. 관련 테스트는 `persist_tests.rs`, `view_refresh_tests.rs`, `cache_slot_tests.rs`다. 선택 이유.


##### 편집 조작과 surface 설정

- **surface 선택**: 편집 모드에서는 모든 surface 가 1px hairline 윤곽을 얻고, 칸 가운데를 한 번 클릭하면 **선택만** 된다 — 2px accent inset 윤곽 + 우측 상단 핸들 **둘**: **설정(톱니)** · **remove(제거)**. 선택은 아래 표준 단축키의 대상이라 한 번 클릭으로는 미리보기를 떠나지 않는다. 톱니를 누르거나 칸을 **더블클릭**하면 그 surface 의 설정 화면이 열린다. 분할은 아래의 **경계 hover-split 영역**을 사용한다. 마지막 한 장 남은 surface 제거는 무효(트리에 0-surface 탭을 쓰지 않음).
- **경계 hover-split 존 (마우스)**: 선택되지 않은 surface 의 4변 바깥 30% 밴드를 hover 하면 accent 22% 밴드 + 안쪽 변 2px accent 55% 분할선 overlay 가 뜨고 커서가 crosshair 로 바뀐다. 클릭하면 그 변으로 split 된다 — **좌/우 존 = 좌우(row) split, 상/하 존 = 상하(column) split**, **좌·상 존은 새 surface 가 first(좌/상)**, 우·하는 second. 축 길이가 46px 미만이면 그 축 밴드는 소멸(중앙 선택은 항상 가능). 선택된 surface 에서는 존이 뜨지 않는다(배경 클릭으로 선택 해제 후 가능). 기존 surface id 는 보존되고 새 surface 만 새 id 를 받는다.
- **탭 삭제 `×` (마우스)**: 편집 모드에서 탭이 2개 이상인 pane 의 active/hover 탭 우측에 14×14 close `×` 가 노출된다(탭 1개면 숨김 + no-op — pane 은 항상 탭 ≥1). 클릭하면 그 탭이 삭제되고 active 인덱스가 재클램프된다.
- **surface 설정 화면**: 오른쪽 detail 컬럼 **전체(툴바 + 미리보기)** 를 대신하는 세 영역으로 구성된다.
  - **헤더**(높이 `preset-cfg-header-height` 44 — 대체되는 툴바와 같다): kind 아이콘(kind accent) · kind 표시명 · mono breadcrumb `preset › 페인 N › 탭 › 서피스 k`(`페인 N` 은 Workspace scope 에서만, 탭 이름은 Tab scope 가 아닐 때만). 좁으면 breadcrumb 이 말줄임된다. 오른쪽 끝에는 draft 가 저장본과 다를 때만 unsaved 표시(6px `preset-cfg-draft-fg` 점 + 캡션, tooltip "초안 — 확인을 누르면 적용됩니다"). back 버튼은 없다 — 나가는 길은 취소다.
  - **본문**: 스크롤되는 유일한 영역. padding `preset-cfg-form-padding`(16), 한 열 폼(최대 폭 `preset-cfg-form-max-width` 460, 필드 간격 `preset-cfg-field-gap` 12). 순서는 **종류**(Select) → 그 kind 가 선언한 필드(라벨 위, 입력 전체 폭). `dir`/`file_path` 필드는 mono 입력과 **찾아보기** 버튼(secondary, folderOpen 아이콘)을 한 줄에 둔다. 칸 크기와 무관하게 모든 필드가 온전히 보인다.
  - **footer**: 높이 고정 `preset-cfg-footer-height`(52), 상단 1px separator, 오른쪽 정렬 `[취소 ghost] [확인 primary]`(`button.cancel` / `button.ok`). **변경이 없으면 확인은 비활성**이다 — 변경 판정은 kind 와 현재 kind 가 선언한 키만 본다.
  - **draft**: 값은 draft 에만 쓰이고 트리는 확인 전까지 바뀌지 않는다. draft 안에서 kind 를 바꾸면 필드 목록이 바로 바뀐다 — 값은 지우지 않으므로 두 kind 가 함께 선언한 키(예: `cwd`)는 이어지고, 원래 kind 로 돌아오면 원래 값이 다시 보인다. 새 kind 필드 중 비어 있고 `default` 가 있는 것은 그 값으로 채워 보인다.
  - **확인**: surface 를 한 번에 교체하고 곧바로 저장한다. kind 가 원본과 같으면 선언 필드만 덮어써 **선언되지 않은 params 를 보존**하고, 다르면 kind 전환 정리 규칙(새 kind 가 안 쓰는 전용 컬럼·params 제거 + default 채움)을 **확인 시점에 한 번** 적용한다 — 중간에 거친 kind 때문에 원본 params 가 지워지지 않는다. 저장이 성공해야 미리보기로 돌아오고 그 칸은 **선택된 채** 남는다. **저장 실패** 시 설정 화면과 draft 를 그대로 두고 표준 에러 toast("프리셋 저장 실패")를 띄운다. 화면이 열린 사이 저장소의 레이아웃이 바뀌었으면 위 "저장소 변경과 편집 충돌" 규칙에 따라 저장하지 않는다.
  - **취소**: draft 를 버리고 미리보기로 돌아온다(칸 선택 유지, 디스크 무변경).
  - **키**: `Esc` = 취소(Kind 드롭다운이 열려 있으면 그 드롭다운만 닫는다), 한 줄 입력 안의 `Enter` = 확인(변경이 있을 때만). 다른 popup 과 같이 코드에서 직접 읽는 고정 대화상자 키다 — `KeybindingSettings` 항목이 아니다.
  - **열려 있는 동안**: 왼쪽 preset 리스트와 L1 scope 탭은 `preset-cfg-dim-opacity`(= `state-dim-opacity`)로 흐려지고 입력(클릭·hover·스크롤)을 받지 않는다. "자동 저장"·Done 은 툴바째 사라진다. 구조 편집 단축키는 동작하지 않는다(미리보기를 그리지 않는다). 창 밖에서 선택이 바뀌거나(컨텍스트 메뉴 "…프리셋으로 저장") 창을 닫으면 draft 는 취소와 똑같이 버리고 묻지 않는다.
  - **kind 드롭다운은 `SurfaceKindRegistry` 를 기준으로 삼는다** — 편집기(`PresetView`)가 main engine 의 공유 `surface_registry` Arc 를 받아 프레임마다 스냅샷(`KindCatalog`)을 파생한다. 후보 목록은 런타임 등록 kind(플러그인 on/off)를 즉시 반영하고, 표시명은 registry 의 `display_name_i18n_key` 로 해석한다. `empty` 는 사용자가 직접 만들 수 없는 내부 kind 라 후보에서 제외한다. 설정 화면이 연 leaf 의 저장본 kind(와 draft 의 kind)가 목록에 없으면(비활성 플러그인 등) 유실 방지로 덧붙는다 — 꺼진 plugin kind 로도 되돌아갈 수 있다. registry 미주입(main window 부재 등)이면 정적 fallback 목록(`terminal`/`markdown`/`image`/`explorer`/`html`)을 사용한다.
- **이름/subtitle 인라인 편집**: 편집 모드에서 툴바의 preset 이름은 텍스트 입력으로, subtitle 은 (Workspace 한정 실제 필드일 때) 입력으로 바뀌어 포커스 해제 시 store 에 commit 된다.
- **트리 변형**: 편집 모델(`DemoLayout`)은 3계층 전부를 변형한다 — surface split · surface 제거 · 탭 추가(+) · **탭 삭제(×)** · **pane split** · **pane 제거**. 마우스로는 경계 hover-split 존(surface split·4방향·before/after)·remove 핸들(surface 제거)·`+` 버튼(탭 추가)·`×`(탭 삭제)로 트리거되고, 전부(pane split·pane 제거 포함)는 아래 **표준 단축키**로도 실행한다. 모든 변형은 기존 leaf/pane id 를 보존하며 자동 저장된다. 무효 가드: 마지막 surface 제거·마지막 탭 삭제(pane 은 항상 탭 ≥1)·루트 단일 pane 제거는 no-op. pane split 은 **Workspace scope 에서만** 유효(Pane/Tab scope 는 pane 트리가 없어 no-op).
- **표준 단축키 (focus 기반)**: 편집 모드에서 본체와 동일한 `KeybindingSettings` 단축키로 편집을 조작한다 — 코드에 키를 하드코딩하지 않고 설정 필드를 그대로 매칭한다(§단축키 정책). 대상은 **현재 선택된 surface(leaf)** 와 그 leaf 가 속한 pane 이다. 선택이 없으면 전부 no-op(임의 대상 조작 금지). 텍스트 입력(이름/subtitle) 포커스 중에는 문자 키가 입력으로 가도록 단축키 매칭을 차단한다. surface 설정 화면이 열린 동안에는 단축키가 동작하지 않는다.

  | 단축키 액션 (`KeybindingSettings`) | 대상 | 동작 |
  |-----|------|------|
  | `split_surface_vertical` / `split_surface_horizontal` | 선택 surface | 좌우 / 상하 분할 |
  | `close_surface` | 선택 surface | 제거(마지막 1장이면 no-op) |
  | `new_tab` | 소속 pane | terminal 탭 추가 |
  | `close_active` | 소속 pane | active 탭 삭제 → **마지막 탭이면 pane 제거로 폴백**(라이브 close_active 의 탭→pane 체인과 동형) |
  | `split_pane_vertical` / `split_pane_horizontal` | 소속 pane | 좌우 / 상하 pane 분할(**Workspace scope 한정**) |
  | `close_pane` | 소속 pane | pane 제거(루트 단일 pane 이면 no-op) |

  구현 위치는 `Act` enum 이 `demo_layout.rs` private 이고 편집 대상 `DemoLayout` 이 egui 임시 캐시에만 있기 때문에 winit `handle_event` 가 아니라 egui 렌더 경로(`draw_preview` → `DemoLayout::apply_shortcut`)다. **제약**: double-tap 바인딩(`shift+shift`/`ctrl+ctrl`/`alt+alt`)은 `parse_binding` 이 거부하므로 편집기에서 지원하지 않는다 — 해당 액션에 double-tap 바인딩만 지정한 사용자는 일반 조합 바인딩을 추가로 지정해야 한다. 또 `KeybindingSettings` 스냅샷은 편집 창을 **열 때** 캡처되므로(appearance 주입과 동일), 설정 변경은 창을 다시 열어야 반영된다.

mini-tab strip 은 `tab_bar.rs`, split 라인은 `divider.rs` 위젯을 재사용한다.

### 적용 — 포커스 규칙

- 단축키(`apply_workspace_preset`/`apply_tab_preset`/`apply_pane_preset`, 기본 빈 칸): 적용 popup → 선택 → 새 인스턴스 생성 + **포커스 이동**.
- **CLI/IPC `preset.apply` 는 항상 `focus: false`** — 포커스 독립 원칙. 단축키 호출만 포커스 이동.

terminal 시작 명령어는 `cd <cwd>\r` 와 합쳐 고정 preset 준비 입력으로 만들어 journal 실행에 전달한다(`src/runtime/preset_plan.rs`).

### 설정 화면의 입력 예외

Kind Select는 현재 키보드로 열 수 없어 진입할 때 autofocus하지 않는다. 드롭다운이 열린 상태의 Esc는 드롭다운만 닫는다. 설정 화면 더블클릭 진입은 경계 split 영역 밖에서만 가능하며, split 영역의 연속 클릭은 기존 분할 조작을 유지한다. 설정 화면을 연 동안 초안의 dirty 여부는 현재 kind와 그 kind가 선언한 키만 비교한다.

## 인터페이스

`preset.{list,get,save,delete,rename,capture,apply}`(`SurfaceRead`/`SurfaceWrite`) — `tasty preset {list,get,save,delete,rename,capture,apply}`. 표 → [reference/api](../../reference/api.md#구조--workspace--pane--tab--surface--split--tree).

`preset.get`/`preset.save` 는 preset 을 JSON 으로 그대로 직렬화/역직렬화하므로 각 surface 의 영속 `id`(위 [surface 영속 id](#surface-영속-id))가 공개 스키마에 자동 노출·왕복된다 — 실행 중 surface의 ID와는 별개다. `save` 로 들어온 결손·중복 id 는 저장 시 정규화된다.

## 관련

- [closed-tab-restore](../closed-tab-restore/index.md) · [layout-persistence](../layout-persistence/index.md) · [work-area](../work-area/index.md)

Surface 설정의 dir/file_path 찾아보기는 같은 View의 Tasty 폴더/파일 피커다. 확정 경로는 그 필드 draft에만 들어가며 설정 화면의 확인이 저장한다. 취소는 값을 보존하고 피커가 열린 동안 Esc는 피커만 닫는다. 요청의 preset key·leaf·field·kind·편집 세대를 대조하므로 이름 변경, 다른 leaf 재열기나 kind 변경 뒤의 늦은 결과를 적용하지 않는다.
