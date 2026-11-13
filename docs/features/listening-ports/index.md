# 리스닝 포트 뷰어 (Listening ports)

- **Status**: Implemented
- **주체**: 로컬 사용자 (원격 접속 사용자는 mirror 로 동일 화면을 본다)
- **설계**: 없음
- **코드**: `src/adapters/ui/popup/port_scanner.rs`, `crates/tasty-portscan`
- **화면**: [아래 절](#화면)

## 목적

로컬 사용자가 워크스페이스(또는 시스템 전체)의 listening TCP 포트를 한눈에 보고, 행을 선택해 그 주소를 클립보드로 복사할 수 있게 하는 **GUI 진단 편의 기능**. "내 dev 서버가 몇 번에 떴지?" 를 셸 명령 없이 UI 로 확인한다.

## 내부 동작

### 스캔 상태 머신

`Idle → (열림 / scope 변경 / Refresh) → Loading → (poll) → Ready | Failed`. 팝업을 닫으면 `Idle` 로 reset.

- 백그라운드 스레드가 스캔하고 mpsc 채널로 결과를 보낸다. 메인 루프가 매 프레임 poll 한다.
- 동일 scope 결과는 캐시(`CachedScan`)로 재사용한다.

### 즐겨찾기 (favorites)

메인 테이블 각 행 leading(Port 컬럼 앞) 28px 별 아이콘으로 `(addr, port)` 를 즐겨찾기에 등록/해제한다. 확인 절차 없이 클릭 즉시 토글.

- **영속**: 전역(surface 무관)·`~/.tasty/port-favorites.toml`. 프로세스 원본 하나를 모든 윈도우가 함께 쓰며 토글 시 즉시 저장한다 — 앱 재시작 후에도 유지되고, 다른 윈도우에서 바꾼 즐겨찾기도 바로 보인다.
- **상단 즐겨찾기 섹션**: 팝업 필터 행과 테이블 사이에 항상 노출되는 높이가 제한된 영역(캡션 22px + 리스트 최대 112px = 5행, 6번째부터 스크롤). 캡션은 "Favorites"(+개수, 0개면 개수 생략) 좌측 / "system-wide" 우측. 0개일 때도 캡션은 유지하고 안내 문구 1행을 보여준다(Explorer 사이드바 즐겨찾기와 동일 관례).
- **system-wide 판정**: 즐겨찾기 섹션의 LISTEN/NONE 배지는 메인 테이블의 scope(Tasty/System)·검색·상태 필터와 **무관하게 항상 시스템 전체 기준**이다. 즐겨찾기가 1개 이상 등록되면 `scan_all()` 기반 별도 스캔(`MainViewState.port_favorites_scan`)이 병행 실행되어 각 즐겨찾기 항목을 매칭한다(0개면 스캔하지 않음). 매칭 있고 LISTEN → 실행중 배지(pulse), 매칭 있고 다른 상태 → waiting 배지, 매칭 자체가 없음(NONE) → idle 배지 + `NONE` 라벨(기존 `StatusDot` idle 상태 재사용, 새 색 없음).
- **즐겨찾기 행 요약**: 별(항상 on, 클릭 시 제거) · `{addr}:{port}`(mono) · 매칭 있으면 `{process} · {pid}`(+workspace) 없으면 "not running" · 우측 상태 배지. 메인 테이블의 7컬럼 그리드를 그대로 쓰지 않는 요약형이다. 상세는 주소에서 `space-md` 떨어져 왼쪽에 붙고 넘치면 말줄임한다. 상태 배지는 행 오른쪽 끝의 최소 112px(`size-112`, 역할 토큰 없음, UI 배율을 곱함) 칸 안에서 오른쪽에 붙어 상세가 길어도 점에 닿지 않는다. 행은 왼쪽 여백 없이 28px 별 칸부터 시작하고 별은 칸 가운데에 놓여, 표의 별 열(역시 가운데 정렬)과 같은 세로선에 선다. 캡션과 빈 안내 줄만 popup 좌우 여백을 둔다. 본체 popup과 갤러리 예제는 `tasty_ui_widgets::ports_favorite_detail_and_state` 하나로 그린다.

### 데이터 (7컬럼)

Port / Proto / Address / Process / Workspace / Tab / State.

- **Proto**: 현재 TCP 고정 (UDP 미지원).
- **Workspace / Tab**: Tasty 프로세스 트리 소속 행만 채워지고, External 행은 em-dash(`—`).
- **State**: 연결 상태(`PortState` — Listen / Established / CloseWait / …). `LISTEN` → green dot + pulse, 그 외 → yellow dot. `reduced_motion` 시 정적 dot. 백엔드는 전 상태를 수집하지만 뷰어는 기본적으로 LISTEN 만 표시한다(아래 상태 필터).

### scope (Tasty / System)

- **Tasty**: Tasty 셸 프로세스 트리 자손 PID 가 listening 중인 포트만.
- **System**: host 의 모든 TCP 소켓. Tasty 자손 PID 와 일치하는 행은 workspace/tab 으로 태그.
- toggle 시 자동 재스캔.

### 검색 / 정렬

- **검색**: 전 컬럼 case-insensitive substring (port / addr / pid / process / workspace / tab). 빈 query 는 전체 통과. **데이터 기준**이라 컬럼 숨김과 독립(숨긴 컬럼도 검색 대상).
- **정렬**: Port / Address / Process / Workspace / Tab 가능, **Proto / State 는 불가**. None·빈 값은 정렬 방향과 무관하게 항상 tail. 숨긴 컬럼이 활성 sort key 여도 정렬은 그대로 유지(데이터 정렬은 표시와 독립).
- 필터·정렬·컬럼 표시 상태는 `egui::Memory` 에 영속.

### 컬럼 폭 / 가로 스크롤

- 각 컬럼은 **최소폭**을 가진다 (Port 84 / Proto 76 / Address 140 / Process 200 / Workspace 120 / Tab 80 / State 140). Address 최소폭은 `port-addr-col-min-width`, Process 최소폭은 `port-process-col-min-width` 토큰이다. Process 외의 컬럼은 최소폭을 그대로 쓰고 늘거나 줄지 않는다(Address 도 고정). 보이는 컬럼 최소폭 합이 본문 가용폭을 넘으면 컬럼을 줄이거나 숨기지 않고 **테이블 본문이 가로 스크롤**된다. 가용폭이 남으면 Process 가 여유폭을 모두 받아 빈 공간 없이 채우고, Process 가 숨겨져 있으면 마지막 컬럼이 받는다. 기본 popup 폭(660)에서는 최소폭 합이 더 커서 본문이 가로 스크롤된다.
- 컬럼 정의(최소폭·flex 여부·정렬)와 표 꾸밈은 갤러리 예제와 같은 공용 정의를 쓴다.
- 가로 스크롤은 **본문 영역에만** 갇힌다. sticky 헤더는 본문과 수평 동기 이동(세로로는 고정), footer / 구역 divider 는 popup 폭에 고정 유지된다.
- 행·헤더 치수는 공용 Table 기본값을 쓴다. 헤더와 본문 행 높이는 `table-cell-height`(28), 헤더 셀 좌우 패딩은 `table-cell-padding-x`(12)다. 본문 스크롤 영역은 최소 한 행 높이를 남긴다. 포트 스캐너 전용 치수는 없다.

### 컬럼 표시/숨김 (column chooser)

- 헤더 우측 컬럼 아이콘 버튼을 누르면 컬럼 목록 팝업이 열리고, 컬럼별 체크박스로 표시/숨김을 토글한다 (예: System scope 에서 의미가 옅은 Workspace / Tab 숨김).
- **Port 는 식별 / 기본 정렬 컬럼이라 항상 표시**(체크박스 잠금) — 전부 숨김이 구조적으로 불가능하다.
- 표시 상태는 `egui::Memory` 에 영속하여 팝업을 닫았다 열어도 유지된다. (per-column 값 필터가 아니라 컬럼 전체의 표시/숨김이다.)

### 상태 필터 (state filter, 기본 LISTEN-only)

- 필터 행 우측의 funnel 버튼을 누르면 **표시할 TCP 상태** 드롭다운이 열린다. `FilterState.visible_states` 는 **표시(shown) 집합**으로, 기본값은 `{Listen}`(LISTEN-only). 백엔드가 전 상태를 수집해도 뷰어는 기본적으로 listening 소켓만 보여준다.
- 드롭다운은 **현재 scope 에 실제로 존재하는 상태들**(필터 전 전체 rows 에서 도출, LISTEN 선두 + 알파벳)만 체크박스로 나열한다 — 꺼둔 상태도 다시 켤 수 있다. 체크 = 표시. **모두 선택 / 모두 해제 / 초기화(LISTEN-only 복원) / 적용** 버튼을 제공한다.
- **Apply-on-confirm**: 드롭다운 편집은 임시 draft(egui temp memory)에만 쌓이고 **적용**을 눌러야 반영된다. 드롭다운이 열려 있을 때 Escape 는 드롭다운만 닫고 팝업은 유지한다.
- 표시 상태 집합은 `egui::Memory` 에 영속해 재오픈에도 유지된다(앱 종료 시 휘발 → LISTEN-only 복원).
- **적용 순서**: 상태 필터 → 검색 → 정렬. footer `total` 은 *상태 통과·검색 전* 개수, 헤더 `{listening} listening` 은 상태 필터와 무관한 scope 전체 LISTEN 개수다.

### 빈 결과 4분기

`Ready` 인데 행이 0개일 때 우선순위: **search_zero**(검색어 있음) → **state_filtered**(검색은 비었으나 상태 필터가 scope 행을 전부 가림) → **system_empty**(System scope) → **tasty_empty**(Tasty scope). `Loading` / `Failed` 는 별도 분기. state_filtered 분기 덕에 LISTEN 0개를 "포트 없음" 으로 오인하지 않는다.

### 레이아웃 (footer 하단 고정)

- 헤더 / 필터 행을 그린 뒤 남은 영역을 `TopBottomPanel::bottom`(footer) + `CentralPanel`(본문)로 나눈다. footer 가 하단 공간을 먼저 예약하므로 **footer 는 항상 popup 하단에 고정**되고, 본문이 남은 높이 전체를 채운다.
- 행이 적으면 테이블이 위로 붙고 **빈 공간은 본문(목록) 영역 하단(footer 위)** 에 남는다 — footer 아래가 비지 않는다.
- 구역 divider(헤더/필터/footer 위)는 popup 전체폭(`full.x_range()`)에 고정. 본문 가로 스크롤은 테이블 자체 ScrollArea 에 갇혀 footer/divider 폭에 영향을 주지 않는다.

### 헤더 / footer 구성

- **헤더**: leading 포트 아이콘 + 제목 + accent Tag(`{listening} listening` / `scanning…`) + 검색 입력 + 컬럼 chooser 아이콘 버튼 + Refresh 아이콘 버튼(상시 노출, 현재 scope 재스캔) + close(`×`).
- **필터 행**: 좌측 `Show all (system-wide)` 체크박스 + 우측 상태 필터 funnel 버튼.
- **footer**: 카운터(`{shown} of {total} ports`) + `Copy address`(행 미선택 시 disabled) + `Close`. popup 하단 고정.

### 행 선택 / 주소 복사

- 행을 클릭하면 선택(강조)되고, 같은 행을 다시 클릭하면 해제된다(`selected_port`, `egui::Memory` 영속). 셀 글자(주소·프로세스명 등) 위에서도 동일하게 행 선택이 잡힌다 — 대신 셀 텍스트를 드래그로 선택·복사할 수는 없다([`tasty-ui-widgets` — 본체·갤러리 공유 UI primitive](../../architecture/ui-widgets-crate.md), 대체는 아래 `Copy address`).
- footer `Copy address` 는 선택 행의 `host:port`(IPv6 는 `[..]` bracket)를 클립보드에 복사한다(egui 복사 출력 → [텍스트 복사의 공통 경로](../clipboard/index.md#텍스트-복사의-공통-경로)). 성공하면 창 범위 `toast.copied` 토스트가 뜬다.

## 인터페이스

- **사용자 트리거**: 사이드바 [도구 메뉴](../tools-menu/index.md) 의 `Listening ports…` → `port_scanner` popup (`PopupScope::Window`). 행 클릭으로 선택 후 footer `Copy address` 로 주소를 복사한다.
- **IPC/CLI**: 없음 — **의도된 설계.** 포트 목록은 agent 가 일반 셸 명령(`ss` / `lsof` / `netstat`)으로 직접 조회 가능하므로 tasty 가 중복 제공하지 않는다. 이 기능은 *사람이 UI 로 편하게 보는* 편의일 뿐이다.

## 비-목표

- UDP 포트 (TCP 전용).
- 포트/프로세스 종료(kill) — 보기 전용.
- IPC/CLI 제공 (셸로 충분하므로 중복하지 않음).

## Acceptance Criteria

- Tools 메뉴 `Listening ports…` 클릭 시 팝업이 열리고 스캔이 시작된다 (Loading → Ready).
- `LISTEN` 포트는 green dot, 그 외 상태는 yellow dot 으로 표시된다.
- scope 를 System 으로 토글하면 재스캔되어 host 전체 포트가 나온다.
- 검색어 입력 시 모든 컬럼에 substring 매칭으로 행이 필터된다.
- 행이 0개일 때 scope/검색/상태필터 조합에 맞는 빈 메시지(search_zero / state_filtered / system_empty / tasty_empty)가 뜬다.
- 기본 열림 시 LISTEN 상태 행만 표시된다. 상태 필터로 ESTABLISHED 등을 켜면 해당 행이 나타나고 footer `{shown} of {total}` 이 갱신된다. 드롭다운 열림 중 Escape 는 드롭다운만 닫는다.
- 팝업이 좁거나 화면 가장자리에 있어 상태 필터/컬럼 chooser 드롭다운이 팝업 경계 밖으로 삐져나가도, 그 밖으로 나간 부분(체크박스 등)을 클릭하면 정상적으로 토글되고 드롭다운·팝업 모두 열린 상태를 유지한다(바깥 클릭으로 오판되어 닫히지 않는다).
- footer 는 팝업을 리사이즈해도 항상 하단에 고정되고, 행이 적을 때 빈 공간은 목록 영역 하단(footer 위)에 생긴다.
- 행 클릭 시 선택 강조되고 재클릭 시 해제된다. 클릭으로 브라우저가 열리지 않는다.
- footer `Copy address` 는 선택 시에만 활성화되고, 클릭 시 선택 행의 주소가 클립보드에 복사된다.
- 헤더 Refresh 버튼이 상시 노출되어 정상 상태에서도 재스캔할 수 있다.
- 보이는 컬럼 최소폭 합이 본문 폭을 넘으면 테이블이 가로 스크롤되고, footer `Close` / `Copy address` 와 헤더 구역은 잘리지 않고 popup 폭에 고정된다.
- 헤더 컬럼 chooser 로 컬럼을 숨기면 폭이 줄고, 팝업을 닫았다 열어도 표시 상태가 유지된다. Port 컬럼은 숨길 수 없다.
- 메인 테이블 행의 별 아이콘을 클릭하면 즉시 즐겨찾기가 토글되고(확인 절차 없음), 별을 클릭해도 행 선택 상태는 바뀌지 않는다.
- 즐겨찾기로 등록한 `(addr, port)` 는 앱을 재시작해도 상단 즐겨찾기 섹션에 남아있다.
- Given 윈도우 두 개 When 각 윈도우의 팝업에서 다른 포트를 즐겨찾기에 넣는다 Then 두 윈도우의 즐겨찾기 섹션에 둘 다 보이고 재시작 뒤에도 남는다.
- 즐겨찾기 섹션의 LISTEN/NONE 판정은 메인 테이블을 Tasty scope 로 보고 있어도 시스템 전체 기준이다 — Tasty 프로세스가 아닌 포트를 즐겨찾기해도 정상 배지가 뜬다.
- 즐겨찾기가 0개면 상단 섹션은 캡션 + 안내 문구만 보이고, 5개를 초과하면 리스트가 스크롤된다.
- Given 즐겨찾기 행이 있을 때 When 상세가 길어도 Then 상세는 상태 칸 앞에서 말줄임되고 상태 점과 닿지 않는다.

> GUI 기능이라 검증은 gallery 데모 + 스크린샷(시각)으로 한다. scan/filter 로직은 `tasty-portscan` 단위 테스트로 독립 검증 가능.

## 구현

- crate `tasty-portscan` — `scan_all()` / `scan_for_pids(pids)` / `collect_descendant_pids(pid)`, OS 백엔드 분기(Linux `/proc/net/tcp`, macOS `lsof`, Windows `GetExtendedTcpTable`), 캐시 `cache.rs`.
- popup: `src/adapters/ui/popup/port_scanner.rs` — `draw_port_scanner_popup`(상태 연결) + `draw_port_scanner_view`(pure view).
- 비동기 상태: `MainViewState.port_scan: PortScanState`(메인 테이블, Tasty/System scope) + `MainViewState.port_favorites_scan: PortScanState`(즐겨찾기 전용, 항상 system-wide `scan_all()`, 즐겨찾기 1개 이상일 때만 kick). 둘 다 `kick_off_scan`/`poll_state` 를 공유하는 동일한 `PortScanState` 슬롯이다. 필터 상태: `egui::Memory`.
- 즐겨찾기 영속: `src/core/port_favorites.rs` 의 `PortFavorites`(`~/.tasty/port-favorites.toml`). 프로세스 원본 `RuntimeRegistries::port_favorites`(`SharedList<PortFavorites>`)를 두고 engine 의 `EngineRuntime::port_favorites` 는 그리기용 사본이다. 별 토글은 `EngineAction::TogglePortFavorite` 가 원본 기준으로 `contains`/`add`/`remove` 한 뒤 원본 전체 snapshot을 file writer에 제출한다. 표시 revision과 저장 완료를 구분하고 실패하면 경고 알림을 남기며 목록은 유지한다. 동작 규칙은 [App·Engine·View 상태 소유권](../../dev-guide/app-state-ownership.md#윈도우가-함께-쓰는-영속-목록).
- gallery 데모: `crates/tasty-gallery/src/catalog/components/port_scanner.rs`.

## 화면

화면정의서 — **리스닝 포트 팝업 (화면)**.

- **시각 소스**: tasty-page `vendor/ui_kits/terminal/overlays/port_scanner.jsx`

### 트리거

사이드바 하단 **Tools 메뉴** → `Listening ports…` 클릭. Window 스코프 팝업으로 열린다.

### UI 요소 인벤토리

- **팝업 프레임**: 660×520 (디자인 원본), headless.
- **헤더 행**: leading 포트 아이콘 + 제목 "Listening ports" + accent Tag(`{listening} listening` / `scanning…`) + 단일 라인 검색 입력 + 컬럼 chooser 아이콘 버튼 + Refresh 아이콘 버튼(상시 노출) + close(`×`).
- **필터 행**: 좌측 `Show all (system-wide)`(ko `전체 보기`) 체크박스 — scope 토글 (Tasty ↔ System) + 우측 상태 필터 funnel 버튼.
- **즐겨찾기 섹션**: 필터 행과 테이블 사이, 항상 노출(높이 제한 — 캡션 22px + 리스트 최대 112px). 좌측 "Favorites"(+개수) / 우측 "system-wide". 0개면 흐린 별(37%) + 안내 문구 1행. 1개 이상이면 행마다 별(항상 on, 클릭 시 제거) · `{addr}:{port}`(mono) · `{process} · {pid}`(+workspace) 또는 "not running" · 우측 상태 배지(LISTEN → running+pulse, 그 외 매칭 → waiting, 매칭 없음 → idle+`NONE`). 5행 초과 시 스크롤.
- **컬럼 chooser 팝업**: 헤더 컬럼 아이콘 버튼 클릭 시 열린다. 컬럼별 체크박스로 표시/숨김 토글. Port 는 잠금(항상 표시). leading fav 컬럼은 chooser 대상이 아니다(항상 표시).
- **테이블 (leading fav 컬럼 + 최대 7컬럼)**: 별(28px, 헤더 라벨 없음, 클릭 시 즐겨찾기 토글) + Port / Proto / Address / Process / Workspace / Tab / State (chooser 로 숨긴 컬럼은 제외).
  - 각 컬럼 최소폭 보유 — 보이는 컬럼 최소폭 합 > 본문 폭이면 본문이 **가로 스크롤**(말줄임 대신). sticky 헤더는 본문과 수평 동기 이동.
  - 정렬 가능 헤더(Port/Address/Process/Workspace/Tab) 클릭 시 `▲`/`▼` 인디케이터. Proto/State 헤더는 비정렬.
  - State 셀: 상태 dot(색 + pulse) + 상태 텍스트.
  - 셀 글꼴: Port·Proto·Address 는 mono 열이라 고정폭 글꼴이고, Process 이름과 나머지 열은 UI 글꼴이다. 크기는 모두 font-size-body 다. Process 셀의 PID Tag 는 숫자만 보이고(이름과 spacing-sm 간격), 포인터를 올리면 `PID {pid}` 툴팁(`port_scanner.pid_tooltip`)으로 뜻을 알린다. 컬럼이 좁으면 프로세스 이름만 말줄임하고 PID Tag 는 그대로 보인다. 말줄임한 이름은 포인터를 올리면 전체가 보인다.
  - 셀 글자색: Port·Proto·Address(mono 열)와 Process 이름은 text-primary 다. Workspace·Tab 이름은 색을 두지 않아 공용 표의 행 글자색(쉬는 행 table-row-fg, hover 행 text-secondary-raised, 선택 행 table-row-fg-selected)을 따른다. 빈 값 대시는 text-muted, State 라벨은 StatusDot 의 글자색이다. 선택 행에서는 모든 칸의 글자가 table-row-fg-selected, 켜진 별이 port-star-on-active, 꺼진 별이 port-star-off-active, 상태 점이 status-dot-*-active 다. PID Tag는 자체 채움이라 그대로다.
  - 행 클릭: 선택 토글(선택 행 강조). 브라우저 오픈 없음. 별 클릭은 행 선택에 영향을 주지 않는다.
- **footer**: `{shown} of {total} ports` 카운터 + `Copy address`(선택 없으면 disabled) + `Close`.

### 상태별 시각

부모 기획의 스캔 상태 → 화면 표현:

- **Loading**: 본문 중앙 `Spinner` + "Scanning…", footer 에 동일 메시지.
- **Ready**: 테이블 렌더.
- **Failed**: 에러 메시지 (재스캔은 헤더 Refresh 버튼으로).
- **빈 결과**: search_zero / state_filtered / system_empty / tasty_empty 각각의 메시지 (부모 기획의 4분기).

### 시각 소스

tasty-page `vendor/ui_kits/terminal/overlays/port_scanner.jsx` — 팝업 치수·색·dot·레이아웃 수치의 단일 출처. 스크린샷: tasty-page `vendor/screens/port_scanner-*.png`.
