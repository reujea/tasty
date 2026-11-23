# 내부 팝업 시스템

Popup 은 View 내부에 존재하는 가상 창이다 — 터미널과 공존하며 포커스를 독점하지 않는다. 모든 내부 팝업은 **`PopupManager` + `PopupDef`** 로 관리된다(`src/adapters/ui/popup.rs`). 용어 구분은 [concepts/ubiquitous-language](../../concepts/ubiquitous-language.md), 팝업을 *추가하는 법* 은 [dev-guide/popup-implementation](../../dev-guide/popup-implementation.md). 이 문서는 시스템 *동작 모델* 이다.

## 8대 규칙

1. **타이틀과 콘텐츠**: 높이 토큰을 따르는 상단 제목 영역과 하단 콘텐츠로 나눈다.
2. **타이틀바**: 제목은 `font-size-max` 크기로 가운데에, 닫기 버튼은 오른쪽에 둔다. 버튼은 IconButton sm(`close` 아이콘)과 같은 모양·호버·누름 표시를 쓴다. `PopupDef.fullscreen_stage`를 선언한 팝업만 닫기 버튼 왼쪽에 같은 형태의 `fit` 전체화면 버튼과 툴팁을 표시한다. 창 위로 펼친 팝업은 버튼 묶음 맨 앞에 `shrink` 접기 버튼을 더한다([좁은 범위](#좁은-범위-접기와-펼침)). 제목은 버튼 수에 맞는 예약 폭을 양쪽에 대칭으로 비운 스트립 가운데에 오고 그 폭에서 말줄임한다([구현 가이드](../../dev-guide/popup-implementation.md#타이틀-길이-처리-elide)). [무대](fullscreen-stage.md)를 열어도 원본 팝업은 열린 채 유지한다. 무대는 같은 형태의 별도 콘텐츠다.
3. **드래그 이동**: `drag_handle` 영역에서 이동한다. 기본은 타이틀바이며, 타이틀바 없는 `port_scanner`·`remote_tool`·`file_picker`는 실제로 그린 헤더 사각형을 매니저에 전달한다. 첫 프레임은 `DragHandle::Region`을 사용한다. 헤더 글자는 선택하지 않게 하고, 버튼·입력이 포인터를 사용 중이면 드래그를 시작하지 않는다(`is_using_pointer`).
4. **커서**: 드래그 핸들은 grab, 크기 조절 테두리는 해당 방향의 리사이즈 커서를 표시한다.
5. **배경**: 기본은 `surface_raised`, 패널형 팝업은 `bg_panel`이며 타이틀바는 `mantle`을 쓴다. 팝업마다 정의의 `decl.shell`(셸 묶음: Raised·Panel·InfoPanel·Dialog·Menu·BannerMenu·SearchBar)이 배경·테두리·제목줄 아래 선 토큰을 함께 정하고, `popup::draw::shell_colors`가 색을 고른다. `Content`는 관리자가 셸을 그리지 않고 콘텐츠가 자기 토큰으로 그린다(단축키 도움말의 페이드 셸). 터미널과 같은 배경색일 수 있으므로 색 하나만으로 팝업 경계를 판단하지 않는다.
6. **경계**: 팝업은 소속 범위 밖으로 나가지 않으며 크기 조절 때 자동 재배치한다.
7. **다중 팝업**: 나중에 열거나 클릭한 팝업을 앞에 둔다. 키를 받지 않는다고 선언한 팝업(`keyboard: None`)은 예외다. 열거나 눌러도 포커스를 받지 않고 앞으로 오지 않으며 다른 팝업 아래에 그린다. 겹친 영역의 마우스 입력은 최상단 팝업만 받는다([호스트·plugin 순서](#host--plugin-popup-z-order)).
8. **크기 조절과 입력**: `resizable` 팝업은 8방향 테두리(선언이 그립만이라고 하면 오른쪽 아래 그립)에서 크기를 바꾸고 `min_size`와 범위 경계를 지킨다. 우선순위는 egui 위젯 → 닫기·전체화면 버튼 → 크기 조절 테두리 → 드래그 핸들 → 콘텐츠다. 콘텐츠를 그린 뒤 `is_using_pointer()`로 위젯의 입력 사용 여부를 확인한다. 사용자가 직접 크기를 바꾸면 닫기 전까지 sizer가 덮어쓰지 않는다.

(모든 색·치수는 Theme 토큰 — [theme.md](theme.md).)

## 구조

- **`PopupDef`** — 정적·데이터 지향 정의. 팝업 사이 동작 차이를 정하는 공통 선언 `decl: PopupDecl`(`tasty-plugin-protocol`의 직렬화 가능한 타입)과 host만 쓰는 함수 값(title_key/title_fn, sizer, drag_region, draw_fn, on_close, fullscreen_stage)을 담는다. 전부 `src/adapters/ui/popup/defs.rs::all_defs()` 에 모은다. 필드 상세 → [popup-implementation](../../dev-guide/popup-implementation.md).
- **선언 값으로만 갈린다** — scrim·그림자·콘텐츠 여백·셸 색처럼 팝업마다 다른 그리기 값은 `decl`에서 읽는다. 팝업 id를 비교해 정하지 않는다. `popup_attributes_are_declared`가 팝업 공용 코드의 id 명부를 검사한다.
- **`PopupManager`** — 공통 동작(z-order, 드래그, 리사이즈, 타이틀바, clamp, 포커스) 중앙 관리. `register_def` / `open*` / `close` / `close_instance` / `draw`.
- **`PopupState`** — 인스턴스 하나의 상태(선언 id, 인스턴스 id `PopupInstanceId`, 부모 인스턴스, 인스턴스 데이터 `payload`, title, pos, size, open, focused, scope, dragging/resizing, size_user_overridden).
- **범용 render 루프** — `src/adapters/ui/popup/frame.rs::draw_popup_layer`(등록된 모든 `PopupDef` 순회 + close 경로별 `on_close` 훅 drain). toast/banner/tutorial 오버레이 체인은 개념이 다르므로(docs/design/systems/popup.md) `src/adapters/ui/overlay.rs` 로 분리돼 있다. 진입점 `src/adapters/ui.rs::draw_popups` 가 둘을 z-order 순서로 호출한다.

등록된 팝업은 `all_defs()` 가 단일 출처다(예: `notifications`, `convert_surface`, `rename`, `search_bar`, `tools_menu` …). 새 팝업 = 테이블 항목 1개 + draw 함수 하나. 단, plugin 이 소유하는 팝업(예: markdown 파일열기·large-file 확인)은 host `PopupDef` 가 아니라 plugin 매니페스트 `[[contributes.popup]]` 또는 실행 중 `popup.declare`(egui-mesh) 로 등록된다.

## 인스턴스

열린 팝업 하나하나가 인스턴스다. 인스턴스 id(`PopupInstanceId`)는 열 때 받으며 같은 선언으로 연 인스턴스끼리도 다르고, 닫았다 다시 열면 새 값이다. 관리자 목록에는 열린 인스턴스와, 열린 인스턴스가 없는 선언의 닫힌 원본이 함께 있다. 원본은 다음 인스턴스의 처음 위치·크기다.

같은 선언으로 여러 번 열 때 몇 개를 두는지는 선언의 `instances`가 정한다.

| 값 | 열기 요청이 쓰는 인스턴스 |
|---|---|
| `Single` | 창 안에 하나. 범위와 관계없이 열린 인스턴스가 요청이 쓸 인스턴스다. |
| `PerScopeTarget` | 범위 대상마다 하나. 같은 범위에 열린 인스턴스가 요청이 쓸 인스턴스이고, 없으면 새로 만든다. |
| `Multi` | 열 때마다 새 인스턴스. |
| `Queue` | 범위 대상마다 하나. 범위가 다른 요청은 따로 열리고, 같은 범위의 여러 요청은 차례를 기다린다. 인스턴스가 가진 큐에서 기다리는 것은 `info_modal`·`transfer_error`이고, `approval`은 창의 inbox가 차례를 관리하며 인스턴스는 보이는 요청만 가진다. 큐 항목은 그 항목을 넣은 요청의 출처를 함께 가진다(`Queued.origin`). 확인·Dismiss·Retry를 포함해 앞 항목이 어떤 경로로 닫히든 닫기 훅이 그 항목을 꺼내고 다음 항목을 그 항목의 출처로 연다. 사용자 출처가 아닌 항목은 앞 항목이 사용자 출처였어도 포커스 없이 passive 로 열린다. `info_modal`·`transfer_error`의 닫기 훅은 프레임 끝에 돌므로 그 사이 같은 범위에 새 인스턴스가 열려 있으면, 남은 항목을 그 큐의 앞에 넣고 열린 인스턴스의 위치·포커스·출처를 그대로 둔다(`PopupManager::reopen_queue_rest`). |

요청이 쓸 인스턴스가 이미 열려 있을 때의 처리는 요청 종류마다 다르다. `OpenPopup`은 범위 이동·앞으로 가져오기·포커스·데이터 교체를 하지 않는다. 인스턴스 큐를 쓰는 `info_modal`·`transfer_error`이면 요청의 항목을 그 인스턴스 큐의 뒤에 붙이고(`PopupManager::append_queued`), `approval`이면 대기 요청이 이미 inbox에 있으므로 보이는 요청과 코멘트를 그대로 둔다. 다른 팝업이면 아무것도 바꾸지 않는다. 데이터 없이 연 `info_modal`·`transfer_error`·`approval` 인스턴스(debug 열기)는 들어온 데이터가 큐 데이터(안내·승인·전송 실패)일 때만 그대로 자기 데이터로 삼고, 데이터 없는 중복 열기는 무변경이다(`PopupPayload::is_queue_data`). `TogglePopup`은 그 인스턴스를 닫는다. `ReopenPopup`은 그 인스턴스의 범위가 요청 범위와 다르면 닫기 훅 없이 닫고 요청 범위로 다시 열며, 같으면 위치·포커스·z 순서를 그대로 두고 인스턴스 데이터만 바꾼다. Intent를 거치지 않고 관리자를 직접 부르는 열기(`PopupManager::open_request`, 닫기 훅이 큐의 다음 항목을 열 때 등)는 그 인스턴스를 다시 쓴다. 요청 범위로 옮기고 맨 앞으로 올리며, 위치·데이터를 요청 값으로 바꾸고 요청이 포커스를 주면 포커스를 준다.

인스턴스 데이터(`PopupPayload`)는 인스턴스에 있고 닫으면 함께 사라진다. 열기 요청이 데이터를 실어 오고(`OpenRequest::with_payload`), 그리기 함수는 `PopupCx.payload`로, 제목·크기 함수는 세 번째 인자로 받는다. `PerScopeTarget`인 host 정의는 데이터를 인스턴스에 둔 `convert_surface`·`explorer_properties`·`explorer_conflict`·`rename`·`mouse_capture_banner_menu`·`file_handler_picker`·`file_picker`·`transfer_progress`이다. 작용 대상이 범위와 다른 팝업(이름 바꾸기)은 대상 소멸도 인스턴스마다 데이터로 판정해 사유 `TargetGone`으로 닫는다. 열기 요청의 데이터는 `PayloadSlot`에 실려 요청을 복제해도 한 번만 꺼내지므로, 진행 중인 읽기처럼 복제할 수 없는 값도 실을 수 있다. `Queue`인 host 정의는 `info_modal`·`transfer_error`·`approval`이다. `info_modal`·`transfer_error`의 큐는 인스턴스 데이터이고, 같은 범위에 열린 인스턴스가 있으면 같은 팝업의 열기 요청이 그 큐 뒤에 붙는다. `info_modal`은 늘 창 범위라 창에 하나다. `approval`의 대기 목록은 창의 `MainViewState.approval_inbox`에 있고 인스턴스 데이터는 보이는 요청과 그 코멘트다. App이 승인 저장소가 바뀔 때마다 모든 창의 목록을 맞추므로, 목록은 팝업이 닫혀 있어도 갱신을 받아야 한다. `search_bar`는 창에 하나(`Single`)지만 검색할 surface를 인스턴스 데이터(`PopupPayload::Search`)로 둔다. `Multi`인 host 정의는 `restore_notice`이다. 안내마다 인스턴스 하나를 연다([복원 오류 안내의 포커스](#복원-오류-안내의-포커스)). 나머지 host 정의는 `Single`이다. 데이터를 아직 `DialogState`·`MainViewState`의 단일 칸에 두는 팝업은 범위마다 인스턴스를 나누면 한 칸을 함께 쓰게 되기 때문이다.

어느 host 인스턴스든 자식을 가질 수 있다. 열기 요청에 부모(`OpenRequest.parent`)를 실으면 자식은 요청 범위 대신 열린 부모의 범위로 열린다. 부모가 이미 닫혔으면 요청 범위를 쓴다. 열린 자식이 있는 부모는 바깥 클릭으로 닫히지 않는다. 바깥 클릭에서는 자식만 자기 규칙대로 닫히고, 자식이 없어진 다음 바깥 클릭에서 부모가 닫힌다. 부모 인스턴스(`PopupState.parent`)가 닫히면 그 자식 인스턴스도 사유 `ParentClosed`로 닫힌다. plugin 팝업이 부모인 host 파일 피커는 아직 아래 "plugin popup ↔ host popup 부모-자식"의 요청자 기록으로 연결한다.

사용자 요청이 아니라 상태가 여닫기를 정하는 팝업은 여닫기 술어를 둔다(`popup::frame`의 `OPEN_WHEN`). 술어는 열 범위와 덮음 값(`DrivenOpen`)을 돌려주거나 없음을 돌려준다. 관리자(`PopupManager::follow_open_when`)는 그리기 전마다 술어 값을 직전 값과 비교해, 범위가 생기거나 바뀌면 포커스 없이 passive로 열고(같은 인스턴스는 정책대로 옮긴다) 없어지면 닫는다. 덮음 값이 바뀌면 모양이 다른 팝업이므로 열린 인스턴스를 닫고 새로 연다. 값이 같으면 아무것도 하지 않아 다른 경로로 닫힌 팝업을 값이 바뀌기 전까지 다시 열지 않는다. 저장된 원격 행의 연결 팝업(`remote_row`)이 술어를 쓴다. 활성 workspace가 그 팝업을 원하면 그 workspace 범위이고, 마지막 화면 위에 띄울 때는 범위를 덮는다.

열기 요청(`OpenRequest::covers_scope`)은 선언과 다른 덮음 값을 그 인스턴스에만 줄 수 있다. 덮으면 scrim·modal 그림자와 범위의 포인터 입력([덮는 팝업과 포인터](#덮는-팝업과-포인터))을 함께 가지고, 덮지 않으면 scrim 없이 popover 그림자다. 인스턴스가 닫히면 셸 값은 선언 값으로 돌아간다.

## Host ↔ Plugin popup z-order

호스트와 플러그인 팝업은 `next_popup_z_seq()`에서 열기·클릭 순번을 받는다. 매 프레임 각 묶음의 가장 큰 순번을 비교해 앞에 그릴 묶음을 정한다.

| 대상 | 순서 적용 방법 |
|---|---|
| 셸 배경·제목·테두리 | raw layer는 호출 순서만으로 정렬되지 않아 `enforce_host_plugin_popup_z_order`가 set_sublayer로 순서를 지정한다. |
| 플러그인 GPU 콘텐츠 | `render_egui_pass_and_mesh_popups`가 같은 판정으로 host egui와 plugin mesh pass 순서를 정한다. |
| 자기 콘텐츠 보호 | plugin 셸 배경을 콘텐츠를 제외한 네 사각형으로 그려, 먼저 그린 자기 콘텐츠를 덮지 않는다. |
| 클릭·hover·드래그·닫힘 | `occlusion.rs::point_ownership`이 좌표마다 popup 쌍의 z_seq를 비교한다. |

입력 판정의 `Mine`만 클릭 승격을 허용하고 `OutsideAll`만 바깥 클릭 닫힘을 허용한다. 위 팝업 안을 눌렀을 때 아래 팝업이 닫히거나 앞으로 나오지 않아야 한다. 플러그인이 보는 host rect는 같은 프레임 값이며, host가 보는 plugin rect는 이전 프레임 값이라 방금 닫힌 팝업이 클릭 하나를 더 차단할 수 있다.

렌더링 보정은 host 묶음과 plugin 묶음의 비교다. 여러 plugin 콘텐츠는 z_seq 순서지만 셸끼리의 순서는 set_sublayer의 중첩 제한 때문에 완전히 보장하지 않는다. 포커스를 받지 않는 host 팝업(modifier-hint)은 이 host 묶음에 넣지 않으며, plugin 팝업과의 그리기 순서는 정하지 않는다. 입력과 키 소유 판정에서는 순번 0이라 plugin 팝업보다 아래다. 입력 소유 판정은 팝업별 비교이므로 이 두 묶음 렌더링 제한과 구분한다.

## 수명 계약 (open → close → 뒷정리)

`draw_fn`은 인스턴스 문맥 `PopupCx`(선언 id, 인스턴스 id, 포커스, 키 통로, 인스턴스 데이터)를 받는다. 본문은 키를 받는지를 관리자가 넘긴 키 통로(`PopupCx.keys`)의 `receives()`로 읽으며, 그 프레임의 클릭을 반영하기 전 값이다. 그리기 중에는 관리자를 별도로 대여한다. `PopupAction::ReleaseFocus`는 팝업을 닫지 않고 관리자를 복원한 뒤 포커스만 해제한다. 검색 바의 find 단축키가 이 경로로 터미널에 입력을 돌려준다. 스냅샷은 바깥 클릭이 팝업 포커스를 끄기 전 값이라 그 프레임에 입력칸이 포커스를 되찾을 수 있다. 검색 바는 다음 프레임에 스냅샷이 거짓이면 입력칸 포커스를 놓는다([터미널 검색](../../features/terminal-search/index.md#키보드-라우팅-headless-popup-sticky-아님)).

닫기 경로마다 닫기 사유(`PopupCloseReason`, plugin 팝업과 같은 enum)가 붙는다.

| 경로 | 사유 |
|---|---|
| draw_fn의 `PopupAction::Close` | `Content` |
| draw_fn의 `PopupAction::CloseBy(사유)` | 그 사유 |
| 제목줄 닫기 버튼 | `CloseButton` |
| 바깥 클릭 | `OutsideClick` |
| 키 주인 Esc의 중앙 단계 닫기(선언 `escape = Close`, 본문이 그 Esc를 쓰지 않음, `popup::escape_step`) | `Escape` |
| `UiIntent::ClosePopup`, 열린 팝업의 `UiIntent::TogglePopup`, App의 직접 `close()`, debug IPC | `Command` |
| 범위 대상·작용 대상 소멸 | `TargetGone` |
| 부모 인스턴스가 닫힘 | `ParentClosed` |

`close(id)`는 그 선언의 열린 인스턴스를 모두 닫고, `close_instance(인스턴스, 사유)`는 하나만 닫는다.

초안이나 대상 ID의 정리를 draw_fn에만 두면 다른 닫기 경로에서 실행되지 않는다. `PopupDef.on_close`를 등록하면 닫을 때 `closed_queue`에 인스턴스와 사유(`ClosedPopup`)를 넣고 다음 `draw_popup_layer`에서 훅을 한 번 호출한다. 닫힌 경로에 따라 정리를 달리하는 팝업은 사유를 본다. 포트 스캐너는 `Content`·`Escape`일 때만 스캔 상태를 버리고 바깥 클릭·제목줄 닫기·명령으로 닫으면 남긴다. 훅이 다른 팝업을 닫으면 같은 처리 중 이어서 정리하되, 상호 재열기 같은 오류를 막는 라운드 상한을 둔다. `on_close: None`에는 상태가 없거나 남겨도 되는 이유를 적는다. 예를 들어 notifications는 펼친 항목과 스크롤 위치를 다시 열 때 유지하고 tutorial_topics도 남겨도 되는 상태다. notifications fullscreen 무대의 스크롤 상태는 무대 종료 시 별도로 지운다.

절차·필드 상세는 [popup-implementation §닫힘 정리](../../dev-guide/popup-implementation.md#닫힘-정리).

### plugin popup ↔ host popup 부모-자식

plugin 이 `file_picker.trigger`로 host popup 을 열 때 `owner_popup_instance` 로 **자기 popup instance_id 를 전달**하면, host 는 그 값을 자식 쪽 요청자 기록에 보관해 두 popup 을 스택으로 다룬다(이 문서). 전달하지 않으면(예: popup 밖 surface 위젯에서의 호출, Tools 메뉴 진입) 관계 없는 단독 popup 이다.

관계가 성립하면:

- **범위 상속·숨김 보존** — host는 요청자와 부모 instance를 대조해 부모의 선언 종류와 결속 target으로 정한 범위를 자식 파일 피커에 적용한다(이 문서). 부모가 좁은 surface 에서 접히거나 창 위로 펼쳐져도 범위에는 그 배치 결과를 쓰지 않는다. 표시 상태(펼침·접힘)는 부모를 따른다([좁은 범위: 접기와 펼침](#좁은-범위-접기와-펼침)). 부모가 숨으면 자식도 paint/hit/Esc/키 게이트에서 빠지고, 돌아오면 draft·선택·pending 요청을 그대로 이어간다. 숨김은 닫기로 처리하지 않는다. Window 부모의 피커는 창 범위다. owner 없는 피커의 범위는 아래 "파일 피커의 범위"를 따른다.
- **스택 유지** — 자식이 열려 있는 동안 부모는 outside-click dismiss 대상에서 빠진다. 부모를 모달로 잠그는 것이 아니라 dismiss 목록에서만 제외한다(popup은 포커스를 독점하지 않으므로).
- **Esc 소유권** — 경로 키의 수신자는 [입력 계층의 키 경로](../../architecture/input-layer.md#키-경로와-수신자)(`key_route::route_key`)가, 팝업 종류별로 받는 키는 `popup::keys::consumes_key`가 정본이다. 이 단락은 bare Esc 와 경로 키가 아닌 사건, 그리고 그 키를 실은 프레임의 키 주인을 다룬다. Esc 를 포함한 키는 키 주인(`MainViewState::popup_key_owner`) 하나가 받는다. 키 주인은 창의 [포커스 노드](#포커스)가 팝업일 때 그 팝업이다. host 팝업이면 `PopupManager::host_key_owner`(포커스 칸의 host 인스턴스가 열려 있고 범위가 보이며 접힌 묶음의 자식이 아닐 때), plugin 팝업이면 그 인스턴스다. 포커스 노드가 팝업이 아니면 팝업 키 주인은 없다. z 순서로 고르지 않는다. 한 프레임의 키 주인은 값 하나다(`PopupManager::decide_frame_keys`). 창의 키 처리는 그리기보다 먼저 돌므로, 창이 키를 받으면 묶는다. 경로 키는 그 키의 수신자가 팝업일 때 그 팝업을, 경로 키가 아닌 사건은 그 입력 시점(직전 렌더의 범위 보임과 plugin 후보)의 키 주인을 묶고(`MainViewState::window_feeds_key`) 그 키를 실은 프레임은 그 값을 키 주인으로 쓴다. 창이 묶은 몫의 키는 본문 위젯이 아직 egui 포커스를 얻기 전(연 뒤 첫 그리기)에도 그 본문 것이다(`PopupKeys::routed_by_window`. 검색 띠의 find 가 읽는다). 그사이 그 팝업이 닫히거나 범위가 숨어도 다른 팝업으로 옮기지 않고, 그 키는 처리한 노드에서 멈춘 것으로 본다. 묶인 값이 없는 프레임은 host 팝업 층이 이번 프레임의 범위 보임과 표시(접힘·펼침) 판정을 마친 뒤 그리기 전에 한 번 정한다(`MainViewState::decide_frame_key_owner`, plugin 후보도 이번 프레임에 범위가 보이는 인스턴스). host 본문, 접힌 보기 버튼과 펼친 팝업 접기 버튼의 Enter·Space, 그 뒤에 그리는 plugin 팝업 렌더, plugin 붙여넣기 대상이 모두 이 값을 쓴다. 키·글자·IME·붙여넣기 사건 하나의 주인은 그 사건이 들어온 시점의 포커스 노드다. 포커스 노드는 `MainViewState::focus_node` 가 무대 → 설정 열기 요청(`MainViewState::has_settings_open_request`) → 팝업(`PopupManager` 의 포커스 칸) → html 주소 입력칸 → 튜토리얼 말풍선의 키 포커스(`tutorial.keyboard_focus`) → 엔진 포커스 surface 순서로 고른다. 그 값은 포커스 칸을 바꾸는 호출(`set_focus`·`give_focus`·`unfocus_node`·`focus_on_press`·`set_focused`·`set_instance_focused`·`focus_passive`·`focus_passive_in`·`focus_after_close` 등), 무대·html 주소 입력칸의 전이, 설정 열기 요청, 튜토리얼 키 포커스, 엔진 포커스 surface 선택(누름 뒤 활성화, 포커스 이동 단축키·IPC·workspace 전환)으로 바뀐다. 그 호출이 일어나는 곳은 누름(`MainViewState::take_press_snapshot`), 사용자가 연 팝업(`open_request`)과 plugin 팝업 열기(`popup_scope::focus_opened_popup`), 검색 단축키(`search_bar::open_or_focus_for`), passive 팝업에 포커스를 주는 의도(`intent::popup`), 숨은 팝업의 포커스를 접힌 보기 버튼에 넘기기와 펼치기(`popup::display` 의 `hand_hidden_focus_to_button`·`expand`), 팝업 층이 그리는 중 푼 포커스(`popup::frame::draw_popup_layer`), 닫힌 팝업 뒤 포커스(`focus_after_close`), 창의 Esc 처리가 푸는 포커스(`state::escape_gate`), 정규화(`MainViewState::normalize_focus`)다. 무대는 열고 닫을 때(`open_fullscreen_stage`·`close_fullscreen_stage`) 포커스 노드가 되고 풀린다. html 주소 입력칸은 띠를 그릴 때 입력칸의 egui 키 포커스로 포커스를 얻고 잃으며(`HtmlAddressBands::draw`), 팝업이 포커스 노드가 되면 정규화가 그 포커스를 놓는다(`HtmlAddressBands::release_focus`). egui 패스 하나는 입력 몫 하나이고, 몫 하나는 포커스 노드 하나의 몫이다. 창은 누름·키·IME 사건을 처리하기 전에(튜토리얼 입력과 누름 스냅샷보다 먼저) 그 사건이 지금 몫과 다른 포커스 노드의 몫일 수 있으면 그 자리에서 egui 입력을 끊는다(`MainView::cut_input_segment`, `MainViewState::cut_key_frame`, debug 포인터 주입은 `input_segment::press_focus`, debug 글자 주입은 `input_segment::key_focus`, debug 키 주입은 그 함수를 부르는 `debug_input::inject_key`). 사건 시점의 포커스 노드가 지금 몫의 노드와 다르면 사건 종류와 관계없이 끊는다. 사건 처리 중에 포커스 노드를 바꾸는 전이(누름, 무대 열기·닫기, 주소 띠 해제, Esc 해제, 검색 단축키의 포커스 등)는 즉시 적용되고, 다음 사건이 그 차이로 끊는다. 사용자 입력이 포커스를 주며 연 팝업 요청(검색 단축키가 검색창을 새로 열거나 다른 surface 에서 옮기는 다시 열기, Clipboard Viewer 처럼 plugin 팝업을 여는 사용자 단축키 등)도 창이 그 키 사건을 처리한 직후 대기열을 처리해 적용한다(`App::apply_user_focus_opens`). 창 view 앞에서 소비하는 plugin 단축키(`App::try_plugin_shortcut`)와 편집 보류 재생(원래 키, view 로 가는 줄 선 입력)도 같은 처리를 거친다. native WebView 가 넘긴 키 묶음도 키마다 처리한 직후 같은 처리를 거친다(`webview_keys::consume_forwarded_keys`). 키 하나는 게이트·plugin 단축키 단계(`webview_keys::forwarded_gate_and_plugin_steps`: 게이트가 닫히면 `Gated`, plugin 단축키가 처리하면 `Handled`) → 새로고침 → 받은 surface 의 내장 단축키 순서로 처리한다. 이 묶음은 html surface 페이지가 OS 키 포커스를 가진 동안 host 정책이 가져가 큐로 넘긴 키다. 그래서 묶음의 앞 키가 포커스를 주며 연 팝업이 받은 surface 를 덮으면(명령 팔레트 등) 그 게이트가 같은 묶음의 다음 경로 키를 막고(`Gated`), 막은 키는 페이지로 다시 넘기지 않는다. 덮지 않는 팝업이면 다음 경로 키는 받은 surface 에서 시작한 키 경로로 처리한다([입력 계층](../../architecture/input-layer.md#plugin-명령과-webview-전달-키)). 경로 키가 아닌 키는 창의 키 게이트(`window_keys_to_overlay`)로 막는다. native 페이지가 직접 처리하는 키(페이지 예약 액션, host 바인딩이 없는 일반 글자)는 큐에 들지 않으므로 이 판정에 닿지 않는다. 판정(`MainViewState::user_focus_open_queued`)은 처리하면 이 창의 포커스 노드를 바꾸는 요청만 센다. host intent 대기열은 처리기와 같은 열린 인스턴스 값(`intent::popup::opened_for`)과 실제 열기의 포커스 술어(`PopupManager::request_takes_focus`: 등록된 선언, 사용자 출처, 요청의 포커스, 선언이 키보드 포커스를 받는지)를 읽어, 열린 인스턴스를 닫는 토글, 열린 인스턴스에 잇는 열기, 같은 범위 인스턴스의 내용만 바꾸는 다시 열기, 등록되지 않았거나 포커스를 받지 않는 선언의 열기는 세지 않는다. 대기열의 앞 요청이 바꿀 상태는 보지 않고 지금 상태로 판정한다. plugin 팝업 열기 대기열(`pending_popup_opens`)은 여는 처리(`open_one`)와 같은 판정(`popup_scope::open_would_focus_window`)으로 본다. 범위 선언이 없거나 필요한 대상이 없으면 열지 않으므로 세지 않는다. 같은 팝업이 이미 열려 있으면 여는 처리가 그 인스턴스를 돌려받고 창 결속을 바꾸지 않으므로, 연 뒤 포커스와 같은 술어(`popup_scope::focuses_this_window`: 사용자 출처이고 이 창에 그리는 인스턴스인지)로 본다. 그래서 다른 창에 결속해 열린 창 범위 팝업의 단축키는 세지 않는다. 열린 인스턴스가 없으면 새 인스턴스를 이 창에 결속하므로 출처만 본다. plugin 매니저가 없으면 세지 않는다. plugin 팝업 인스턴스는 호스트가 그 자리에서 만들어 포커스 노드로 하므로(`popup_scope::focus_opened_popup`) plugin 왕복을 기다리지 않는다. 열기가 실패하면 그 인스턴스를 남기지 않는다([plugin popup 의 실행 중 선언과 열기](#plugin-popup-의-실행-중-선언과-열기)). 판정이 참이면 intent 대기열 전체와 plugin 팝업 열기 대기열 전체를 평소 순서(intent 다음 plugin 팝업 열기)대로 처리하고, 처리 시점만 앞당긴다. 대기 intent는 사건 경계(IPC 처리 뒤, 사용자 포커스 열기를 만든 키·IME 사건 뒤)에서 처리될 수 있다. 노드 차이로 끊은 자리에 키가 오면 같은 자리에서 한 번 더 끊어 새 노드의 빈 몫을 하나 먼저 둔다. 새 노드의 입력칸은 그 빈 패스에서 egui 키 포커스를 얻고 다음 패스에서 그 키를 받는다(누름 뒤 키와 같은 이유다). 그래서 한 묶음의 [글자, 검색 단축키, 글자]에서 앞 글자는 터미널로, 뒤 글자는 검색창이 이미 열려 있든, 다른 surface 에 열려 있든, 아직 없든 검색 입력칸으로 간다. 같은 노드라도 누름은 지금 몫에 키 입력이 실렸거나 키 주인이 묶였거나 넘길 Esc 가 있으면 끊고, 키·IME 는 지금 몫의 마지막 입력이 누름이거나 지금 몫이 노드 차이로 연 빈 몫이면 끊는다. 키·IME 는 그 사건 시점의 복원 안내 안쪽 포커스(`restore_notice::has_keyboard_focus`)가 지금 몫 키들의 값과 달라도 끊는다. 안내는 포커스 노드가 아니지만 그리기가 몫 하나의 값을 읽기 때문이다. 이 끊기는 빈 몫을 먼저 두지 않는다. 끊는 조건은 `PopupManager::cut_key_frame` 에만 있다. 누른 곳이 egui 화면인지 팝업인지를 가리지 않고, 한 묶음에서 끊는 자리 수에도 제한이 없다. 창의 egui 패스는 일반 프레임과 전체화면 무대 모두 `GpuState::run_main_pass` 하나로 열고 닫는다. 그 함수는 패스 순서를 `gfx::gpu::input_pass::run_input_pass` 에 맡긴다: 몫 열기(`begin_key_pass`) → 직전 일반 패스가 그린 Find 입력칸 알림 → 입력 꺼내기 → 몫 필터 → (필터가 입력을 뺐으면) 편집 보류 폐기 알림 → Find 붙여넣기 수령 → 패스 → 몫 닫기 → 그리기 중 전환 봉인. 창 입력 시험 하네스도 같은 함수로 패스를 돌린다. 입력 꺼내기만 끊은 자리에서 입력을 나누고(`InputCuts::split_pass`, 나눈 패스는 다음 패스를 요청한다), 나눈 직후·몫 필터 전에 편집 보류에 패스 입력을 알리고 도착한 붙여넣기를 합친다(`GpuState::take_pass_input`). 패스를 닫을 때 그 몫의 결속·넘길 Esc·누름 스냅샷·포커스 노드를 함께 꺼낸다(`MainViewState::close_input_pass`). 몫의 포커스 노드는 그 몫에 처음 든 키 사건 시점의 노드이고, 키 없이 누름만 든 몫은 끊어 닫을 때(누름이 옮긴 뒤)의 노드다. 끊어 닫히는 몫에 결속이 없으면 그 노드가 팝업일 때 그 팝업에, 아니면 팝업이 아닌 포커스 노드의 몫으로 묶는다. 그 몫을 그릴 때 뒤 누름이 옮긴 포커스로 다시 정하지 않고, 그리는 동안 포커스 노드를 읽는 판정은 그 몫의 포커스 노드를 읽는다(`MainViewState::pass_focus_node`). 튜토리얼 말풍선, surface 본문과 html 주소 입력칸이 키를 받을지(탐색기 타입어헤드와 주소 입력칸, `MainViewState::pass_key_node` 가 자기 노드인지), 그 판정이다. 탐색기 타입어헤드는 그 몫 키들의 사건 시점 안내 포커스(`MainViewState::pass_notice_focus`)도 읽어, 안내 안쪽 컨트롤이 키보드 포커스를 가진 동안 분류한 글자를 읽지 않는다(`focus_node::explorer_takes_keys`). 그리기 때 안내 포커스를 다시 읽지 않는다. 앞 몫의 안내 밖 누름이 그 포커스를 풀어도 뒤 몫 글자는 안내 포커스 때 분류한 것이다. 무대 노드의 몫은 `pass_key_node` 가 `None` 이라 surface 본문과 주소 입력칸이 받지 않는다. 무엇을 그릴지(무대와 일반 화면)는 지금 무대 상태로 고른다(`MainViewState::pass_draws_stage`, `GpuState::render`). 그 몫의 노드가 그리는 화면에 없으면(그리기 전에 종료 키가 닫은 무대의 몫, 그리기 전에 연 무대 앞의 몫) 패스 입력을 꺼낸 자리에서 위젯에 입력을 넣는 사건(키 누름·글자·IME·붙여넣기·복사·잘라내기·포인터 이동·버튼 누름·휠·터치 시작과 이동·확대)을 뺀다(`MainViewState::drop_input_without_its_node`, `GpuState::run_main_pass`). 누름을 끝내는 사건(버튼 뗌·포인터 사라짐·터치 끝과 취소·키 뗌)은 남긴다. 남긴 버튼 뗌은 egui 의 눌림 상태만 풀고 클릭·끌기 끝·드롭은 만들지 않는다(`end_press_without_click`). egui 가 눌린 상태로 갖지 않은 버튼의 뗌은 뺀다(egui 0.31 은 누른 시점이 없는 뗌도 클릭으로 세어 전역 클릭 시각을 바꾼다). 남긴 첫 뗌 앞에는 누른 자리에서 클릭 거리(`max_click_dist`)를 넘는 포인터 이동을 넣어 그 뗌이 클릭이 되지 않게 한다. 그래서 다음 화면의 한 번 클릭이 더블클릭이 되지 않는다(탐색기 파일 열기 등). 직전 패스의 끌기를 멈추고(`Context::stop_dragging`) 끌어 놓기 payload 를 지워(`DragAndDrop::clear_payload`) 그 화면이 `drag_stopped` 나 드롭을 받지 않는다. 그래서 그리기 전에 무대를 닫은 종료 키와 그 앞 몫(무대 노드에서 든 키·누름)은 누름이 있든 없든 일반 화면으로 그리지만(빈 무대 프레임을 제출하지 않는다) 뒤 화면은 그 몫의 입력을 받지 않는다. 무대에서 누른 채 종료 키로 무대를 닫고 떼거나, 일반 화면에서 누른 채 키가 무대를 열고 떼어도 그 뗌은 지금 그리는 화면에 들어가 egui 의 버튼이 눌린 채로 남지 않는다. 종료 뒤 사건은 노드가 달라 새 몫이 된다. 몫마다 그 몫이 열린 화면(무대 ID, 일반 화면이면 없음)을 적는다. 끊을 때는 사건 시점의 화면을, 패스를 닫고 다음 몫이 없을 때와 봉인할 때는 지금 화면을 적는다. 그리는 중에 화면이 바뀌면(무대 종료 버튼, 팝업 무대 버튼) 패스를 닫은 뒤 그 전환을 처리한다(`MainViewState::seal_after_drawing_transition`, `GpuState::run_main_pass`). 남은 몫(패스 닫기로 승계된 지금 몫과 뒤 큐) 가운데 열린 화면이 그 패스가 그린 화면인 몫에만 전환 앞 표시를 적는다. 다른 화면에서 열린 몫은 표시하지 않는다. 그다음 사건을 받는 열린 몫(뒤 큐의 꼬리, 없으면 지금 몫)이 표시된 몫이면 남은 egui 입력 큐 끝에서 그 몫을 한 번 닫는다(봉인). 봉인은 끊는 조건(`PopupManager::cut_key_frame`)을 거치지 않고 GPU 끊은 자리·팝업 관리자의 뒤 몫·뒤 큐에 한 칸씩 넣는다. 닫는 몫의 노드·결속·넘길 Esc·누름 스냅샷·안내 값은 그대로 두고, 새 몫은 지금 화면에서 열린 결속 없는 몫이다. 그래서 전환 뒤 다음 묶음의 입력(키 없는 이동·휠 포함)은 새 화면의 몫에 들어 그 화면의 hover·스크롤·키가 된다. 열린 몫에 사건도 노드·결속·넘길 Esc·미전달 Esc 뗌 표시(`note_unfed_escape_release` 가 남긴 표시)·누름 스냅샷도 없으면 봉인하지 않고 그 몫의 표시를 지워 지금 화면의 몫으로 바꾼다. 표시된 몫은 노드와 관계없이 열린 화면이 아닌 화면을 그릴 때 위 사건을 뺀다. 같은 화면을 다시 그리면 그 화면의 정상 입력이다. 몫 입력을 뺄 때 뺀 누름의 짝 뗌은 버튼마다 센다(`drop_stray_releases`). 뗌은 먼저 egui 의 눌림(패스 시작의 `button_down` 과 그 패스 앞쪽에서 전달한 누름)을 본다. 눌려 있으면 남겨 그 눌림을 풀고, 눌려 있지 않고 그 버튼의 뺀 누름이 남았으면 그 뗌을 뺀다. 그 버튼의 누름을 전달하면 수를 0 으로 되돌린다. 그래서 뺀 누름의 뗌이 새 화면에 외톨이 클릭으로 들지 않는다. 한 버튼이면 무대에서 누른 채 그리는 중 무대가 닫힌 뒤의 뗌도 눌림만 풀고 위젯 클릭을 만들지 않는다. 다른 버튼이 눌린 채인 다중 버튼의 클릭 집계(`pointer.button_clicked`·전역 클릭 시각)는 egui 0.31 공개 API 로 막을 수 없어 이 정리의 범위 밖이다. 몫 입력을 뺀 패스에 든 보류 편집은 대상이 사라진 것으로 끝나고 원래 키를 재생하지 않는다(`EditKeyState::input_dropped`, [키 매핑의 보류와 재생](../policies/key-mapping.md#보류와-재생)). 그리기 경로에서 지금 포커스·키 게이트를 읽는 자리는 표시·명령 대상뿐이고 소스 가드(`source_guards::draw_pass_key_reads`)가 명부로 고정한다. 누름의 정규화가 html 주소 입력칸의 포커스를 놓으면 입력 시점의 소유권은 바로 놓여 뒤 키는 그 누름이 정한 노드(터미널이면 PTY)로 간다. 입력칸의 편집 끝내기는 놓기 전 몫(주소 입력칸 노드)을 그린 뒤에 적용한다(`HtmlAddressBands::draw` 의 `pass_keys`). 그래서 같은 묶음에서 누름 앞에 친 Enter 는 입력칸이 받아 확정하고, 로컬 파일 주소면 편집이 끝난 뒤에도 그 확정의 파일 확인 결과가 오면 연다. 확정은 입력 시점에 결속된 명령이다. 확정한 주소는 뒤에 포커스가 옮겨도(팝업·페이지·터미널 누름) 연다. 확정을 취소하는 것은 Escape와 새 편집·새 확정뿐이다. 코드로는 편집 중 입력을 고친 경우(`poll_lookup`), 입력칸의 Escape(`HtmlAddressAction::Revert`), 새 확정(`submit`)이 기다리던 확인을 버린다. 입력 시점에 읽는 판정(창의 egui·터미널 게이트, IME, plugin 단축키 대상 `plugin_popup_key_target`)은 지금 받는 몫(마지막으로 끊은 뒤 몫)의 결속을, 결속이 없으면 지금 포커스 노드를 읽는다. 그리기 중 읽는 판정(host 본문, 접힌·펼친 버튼, plugin 렌더, plugin 붙여넣기 대상, `surfaces_take_keys`)은 그 그리기 몫의 결속을 읽는다(`PopupManager::frame_binding`). 끊은 뒤 몫의 Esc 반복 판정과 눌린 키 상태는 앞 몫을 그린 뒤 egui 의 Escape 눌림 상태에서 시작한다(`PopupManager::begin_key_pass`). 한 그리기 몫의 키 주인은 하나다. 누름 뒤 글자는 누름과 다른 패스로 그리므로, 누름이 포커스를 준 입력칸은 누름 몫의 패스에서 egui 키 포커스를 얻고 다음 패스에서 같은 묶음의 그 글자를 받는다(egui 는 누름이 있는 패스에 직전 패스까지 포커스가 없던 입력칸에 그 패스의 글자를 넣지 않는다). egui 로 보내지 않은 키·IME 는 창의 키 처리와 IME 처리가 같은 갈래 판정(`MainViewState::input_destination`)으로 고른 곳(터미널 PTY·egui-mesh·attach mesh)으로 간다. 포커스 가장자리와 접힌 버튼 링은 표시만 하므로 누름을 반영한 포커스를 그린다. 본문이 키를 받는 것은 키 주인 하나뿐이고(관리자가 프레임 시작 키 주인으로 본문 문 `takes_keys` 를 정하고, 그 값과 선언 `keyboard` 가 `None` 이 아닌지로 본문 키 게이트 `PopupCx::keys` 의 `receives()` 를 정한다. 그래서 키 주인이 없는 프레임에 본문 문이 열리는 복원 안내는 카드 안 위젯만 키를 보고 `receives()` 는 거짓이다), 다시 접기 대상과 일반 Esc 대상도 같은 포커스 노드다. 창의 Esc 판정(`MainViewState::window_escape`)은 수식키 없는 Esc 만 받고(`tasty_key_match::is_window_escape`) 설정 열기 요청, 알림 목록 닫기 순서로 본다. 알림 목록은 포커스 노드가 팝업이 아니거나 알림 목록 자신일 때만 닫는다. 창을 덮는 피커처럼 다른 팝업이 포커스 노드면 Esc 는 그 팝업에서 시작한다. 그다음 키 주인이 plugin 팝업이면 그 팝업의 렌더가 받는다(펼쳤으면 다시 접고, 아니면 선언 `escape` 대로 처리한다). host 키 주인이면 선언 `escape` 대로 한다. `Close`·`Unfocus` 는 그 인스턴스 포커스를 풀고 그 Esc 를 본문에 넘기고, `Ignore` 는 넘기지 않고 끝낸다. 창 위로 펼친 host 팝업은 선언과 무관하게 포커스를 풀지 않고 넘긴다. 펼친 묶음 안의 파일 선택기 본문은 그 Esc 를 취소에 쓰지 않아 결과를 남기지 않고 묶음째 다시 접힌다(키 통로 `escape_refolds`, 지금 이 값을 읽는 본문은 `file_picker` 하나다). 하위 화면이 있는 본문(DAG 목록 상세, 원격 도구 양식)은 펼침과 관계없이 Esc 를 소비해 앞 화면으로 돌아간다. 본문을 그린 뒤 팝업 층 중앙 단계(`popup::escape_step`)가 마무리한다. 본문이 그 Esc 를 썼으면(키 통로로 소비, `PopupAction::None` 이 아닌 결과, 그리기 전에 그 인스턴스의 자식 팝업이 열려 있음) 더 하지 않는다. 쓰지 않았으면 펼친 팝업은 다시 접고, 선언이 `Close` 면 사유 `Escape` 로 닫는다. 그래서 드롭다운이 열린 채 누른 Esc 는 드롭다운만 닫고, DAG 목록 상세와 원격 도구 양식은 본문이 Esc 를 소비해 앞 화면으로 돌아간다. 한 노드 안에서는 내용이 먼저 보고, 내용이 쓰지 않은 Esc 만 선언이 처리한다([좁은 범위: 접기와 펼침](#좁은-범위-접기와-펼침)). host 본문은 키 주인일 때만 키 입력(키·글자·붙여넣기·복사·잘라내기·IME)을 본다. 키 주인이 아닌 본문을 그리는 동안에는 그 사건이 egui 입력에서 빠지므로(`popup::body_keys`), 본문이 egui 입력을 직접 읽든 위젯 키 포커스로 읽든 같다. 키 주인이 없는 프레임에는 포커스를 받지 않는 팝업(복원 안내)의 본문이 자기 위젯 키 포커스로 읽는다. 창이 처리한 Esc 누름은 모두 그 프레임의 egui 입력에서 빠진다. Esc 뗌은 빼지 않는다. egui 로 가는 뗌은 egui 의 눌린 키 상태를 풀고, 키 오버레이가 없어 egui 로 보내지 않는 뗌도 egui 의 Escape 눌림 기록을 푼다. 그 뗌은 자기가 실린 입력 몫의 패스를 닫을 때 푼다(`MainView::note_unfed_key`, `PopupManager::note_unfed_escape_release`). 앞 몫이 넘긴 Esc 누름이 남긴 눌림까지 사건 순서대로 풀려 다음 Esc 누름이 반복이 되지 않는다. 두 번째 누름처럼 넘기지 않는 누름은 egui·키 주인 어느 쪽에도 가지 않는다. egui 는 패스 시작에서 입력에 남은 Esc 로 위젯 키 포커스를 풀므로, 남기면 팝업 층보다 먼저 그리는 화면(탐색기 찾기 입력란 등)이 포커스를 잃는다. 키 주인이 그 Esc 를 읽어야 하는 갈래(host 키 주인의 포커스 해제, plugin 키 주인의 닫기·다시 접기)는 뺀 원래 사건(수식키 포함)을 입력 시점의 키 주인에 묶어 두고, 그 키 주인을 그리는 동안에만 다시 넣는다(`popup::body_keys::hand_escape` 가 넣고 돌려준 `PutEscape` 로 그린 뒤 `withdraw_escape` 가 거둔다). host 팝업 층은 본문들을 그리는 동안(`popup::draw`), plugin 팝업은 그 렌더(`plugin_bridge::popup_render::draw_plugin_popups`) 동안이다. 다시 넣을 때는 egui 가 그 Esc 를 패스 시작에서 처리했을 때와 같게 맞춘다. 사건은 빠지기 전 자리(같은 프레임의 뗌보다 앞)에 `events`·`raw.events` 둘 다 들어가고, 반복 여부와 눌린 키 상태(`keys_down`)는 직전 패스의 Escape 눌림 상태에서 egui 와 같은 규칙으로 정한다. 키 주인이 읽지 않은 사건은 그린 뒤 빼지만 눌린 키 상태는 그대로 두어 다음 프레임 판정의 출발점이 된다. 키 주인이 아닌 본문은 키 문이 가리므로 그 인스턴스 본문만 읽는다. 그래서 하위 화면(DAG 목록 상세, 원격 도구 입력 화면)은 한 번의 Esc 로 목록으로 돌아가고, 창이 키 주인을 닫았으면 아무 본문도 읽지 않아 새로 키 주인이 된 다른 범위 팝업이 같은 Esc 로 닫히지 않는다. 한 프레임에 묶인 키 주인이 있으면 그 프레임의 키는 모두 그 주인 것이다. 그리기 전에 Esc 가 또 오면 창은 다른 팝업을 풀거나 닫지 않고 처리한 것으로 끝내며, 같은 프레임의 다른 키도 터미널로 보내지 않는다(`keyboard_overlay_open`). 팝업 층보다 먼저 그리는 화면은 그 프레임의 키 입력을 보지 않는다(`MainViewState::surfaces_take_keys`). 그래서 팝업 키 주인이 있는 동안에는 탐색기 찾기 입력란 같은 팝업 층 아래 화면에 키가 들어가지 않는다. 다른 범위의 그 화면을 누르면 포커스 노드가 그 화면으로 옮겨 팝업 키 주인이 없어지고, 같은 프레임 묶음에 실린 다음 글자부터 그 화면이 받는다. 접힌 보기 버튼과 펼친 팝업의 접기 버튼은 Enter·Space 를 셸이 만든 키 통로의 `BasicKey::Expand` 로 이번 프레임 키 주인일 때만 받고(접기 버튼은 그 버튼이 egui 포커스를 가질 때만), 그 밖에는 포인터 주 버튼 클릭(`clicked_by(Primary)`)만 받는다. 키 주인이 아닌 팝업(`keyboard` 가 `None` 이 아닌 인스턴스)의 텍스트 입력칸이 egui 포커스를 가지면 관리자가 모든 인스턴스를 그린 뒤 그 포커스를 놓는다(`popup::draw::text_focus`). 이번 패스 누름으로 얻은 포커스와 버튼 포커스는 남긴다. 메인 창은 누름을 키 주인 판정보다 먼저 적용해 누른 팝업이 그 패스의 키 주인이 되지만, 별도 창은 패스 전에 키 주인을 정하므로 누른 팝업은 다음 패스에 키 주인이 된다. 그 누름 패스에 얻은 포커스를 놓으면 별도 창에서 누른 입력칸이 포커스를 잃는다. 그래서 키 주인이 아닌 명령 팔레트가 보여도 그 입력칸 캐럿은 보이지 않고, 별도 창에서 누른 입력칸은 포커스를 지킨다.
- **연쇄 정리** — 부모가 어떤 경로로 닫히든 자식 피커에 취소 결과가 채워져, 평소 result 경로 그대로 plugin 에 `cancelled: true` 가 전달되고 피커도 닫힌다. 부모가 결속한 창이 닫힐 때는 창 상태를 버리기 전에 모든 창 상태(버리는 창, 남은 창, 보관된 창)에서 피커를 취소한다. 자식 피커는 부모와 다른 창에 열릴 수 있기 때문이다. 남은 창의 피커는 평소 result 경로로, 버리는 창의 피커는 그 창의 다음 프레임이 없으므로 그 자리에서 결과를 plugin 에 보낸다. 부모 없는 피커나 결과 유실을 남기지 않는다. 사용자가 이미 확정한 결과는 덮지 않는다.

소유 관계는 자식(요청자 기록) 한 곳에만 있다 — 부모 쪽 사본이 없어 둘이 어긋날 수 없다. host는 특정 plugin 이름/kind로 분기하지 않는다. 범위 상속에서는 요청자 plugin과 부모 소유자가 같은지만 확인한다.

## 발화 정책 (CRITICAL)

팝업은 사용자 행동(키보드 단축키·마우스·메뉴)으로 연다. 사용자 조작 근거가 없는 시스템·에이전트·플러그인 요청은 키보드 포커스를 가져오는 팝업을 띄울 수 없다([사용자 통제와 데이터 원칙](../../principles/control-and-data.md), [toast.md](toast.md) 와 같은 기준). 금지 대상은 포커스 탈취다. 사용자 결정을 기다리는 요청과 복원 안내만 아래 허용 범위에서 포커스 없는 passive 팝업으로 보일 수 있다.

- ✅ 단축키/마우스/메뉴 → `UiIntent::OpenPopup` 발화
- ✅ popup A 의 *사용자 액션* cascade → popup B (origin 전파)
- ❌ 사용자 조작 근거가 없는 IPC/CLI/Plugin·시스템 요청으로 키보드 포커스를 가져오는 팝업 열기
- ❌ 결정이 필요 없는 시스템 조건(PTY 종료·시간 경과)으로 자동 popup — 대신 *Domain Intent 로 데이터만 변경*(NotificationStore push 등)하고 UI에서 사용자가 확인. 복원 실패·저장 중단의 복원 안내는 아래 예외다
- ✅ debug 의 `debug.popup.*` — *사용자 입력 재현* 한정 ([debug-ipc](../../dev-guide/debug-ipc.md))
- ✅ 사용자 결정을 기다리는 요청의 **passive 팝업**(아래). 허용 범위는 [승인 팝업](../../features/human-handoff/index.md) 하나다 — `approval.request`, 권한 상승 요청, telemetry cap 의 `require_approval` 이 이 팝업을 쓴다. 다른 팝업을 사용자 조작 근거 없이 열려면 먼저 이 목록에 허용 범위로 추가한다.
- ✅ 복원 자료를 읽지 못했거나 저장이 멈춘 시스템 사건의 **복원 안내**(`restore_notice`). [복구 원칙](../../principles/feedback-and-recovery.md)이 알리게 한 대상이라 시스템이 passive 로 연다(`restore_notice::push`). `keyboard = None` 이라 팝업 포커스를 받지 않는다([복원 오류 안내의 포커스](#복원-오류-안내의-포커스)).

### passive 팝업

origin 이 사용자가 아닌 `UiIntent::OpenPopup`·`TogglePopup` 은 `OpenRequest` 가 정한 위치로 열되 키보드 포커스를 주지 않는다(`intent::popup::open` → `PopupManager::open_request` 의 `by_user` 가 거짓). 빌드와 무관하다. 이 처리는 허용 범위 밖의 요청이 실수로 들어와도 포커스를 빼앗지 않게 하는 안전장치이며, 허용 범위를 넓히지 않는다. 이미 열린 팝업에 온 요청은 그 팝업의 위치·포커스를 바꾸지 않으므로(`Queue` 팝업은 항목만 큐 뒤에 붙는다) 사용자가 준 포커스를 빼앗지도 않는다. 예외는 새 요청이 내용과 대상을 바꾸는 `UiIntent::ReopenPopup`이다. 범위가 다르면 기존 팝업을 닫고 새 범위로 다시 연다(`popup-implementation.md`의 "팝업 열기" 절). `debug.host_popup.open` 은 사용자 조작 재현이라 사용자 출처로 열어 지금처럼 포커스를 준다. 포커스를 받지 않는다고 선언한 팝업(`keyboard = None`: 단축키 도움말·복원 안내)은 예외다. 눌러도, debug로 열어도 팝업 포커스를 받지 않고 passive 도 풀리지 않는다.

- passive 팝업은 포커스 노드가 아니면 `has_focused()` 에 들지 않으므로 열려 있어도 `keyboard_overlay_open()` 이 거짓이다. 예외로 엔진 포커스 surface 를 덮는 passive 팝업은 포커스 노드가 된다(아래 [§포커스](#포커스)의 정규화). 덮인 surface 는 키를 받을 수 없기 때문이며, passive 표시와 WebView 처리는 그대로다. 터미널 입력, explorer·dag_graph 처럼 egui 입력을 받는 surface의 입력, native WebView 의 입력이 그대로 이어진다. Esc 의 포커스 해제 대상(`focused_dismissal_target`)도 아니다.
- native WebView 는 모두 숨기지 않고 키보드 포커스도 회수하지 않는다. 토스트 카드와 같이 팝업 영역과 겹치는 WebView 만, 그 WebView 가 키 입력을 받지 않을 때 숨긴다(`PopupManager::visible_passive_rects`, [webview](webview.md#포커스--회수는-조건부다)). 키를 받는 WebView 와 겹치면 그 WebView 가 팝업을 덮는다. native 위에 egui 를 그리는 층이 없어서이며, 요청은 함께 만든 알림으로도 보인다. 일반 팝업이 떠 있어도 페이지를 보이는 모드는 [WebView §egui 오버레이와 native WebView](webview.md#egui-오버레이와-native-webview)에 있다. 모드는 backend 가 보고한다. Linux(X11)는 XFixes 2.0 과 SHAPE 확장이 있을 때 합성 관리자가 있으면 투명, 없으면 불투명이고, 확장이 없거나 적용이 실패하면 숨김이다. Windows 와 macOS 는 불투명이다. passive 팝업은 그 모드의 대상이 아니다.
- 팝업을 누르면 일반 팝업과 같이 포커스를 받고 passive 가 풀린다(`keyboard = None` 팝업은 제외). 호스트가 확인한 사용자 입력도 같은 전이를 낸다: 파일 피커를 연 plugin 이 같은 surface 에서 증명이 있는 `file_picker.trigger` 를 다시 보내면 사용자 출처 `UiIntent::FocusPassivePopup` 이 그 피커를 창의 포커스 노드로 하고 맨 앞으로 올린다(`PopupManager::focus_passive`). 포커스를 잃은 다른 팝업은 열린 채 남는다. 증명 없는 요청, 다른 plugin 의 요청, 피커 요청자 기록과 출발 surface 가 다른 요청은 같은 범위에서 busy 로 남는다. 그때부터 일반 팝업처럼 오버레이 모드의 대상이 되고(지원하지 않으면 모든 native WebView 를 숨긴다) 포커스를 회수한다. 닫으면 passive 도 풀린다.
- 포커스를 받지 않는다고 선언한 팝업(`keyboard: None`, 복원 안내·단축키 도움말)은 출처나 passive 여부와 관계없이 WebView 를 한 규칙으로 다룬다. 겹친 WebView 를 숨기지 않고, 키보드 포커스를 회수하지 않으며, 오버레이 모드의 대상도 아니다. WebView 는 bounds·표시·포커스를 그대로 두고 팝업 사각형만 그림과 포인터에서 뺀다(`PopupManager::quiet_popup_rects`). 그래서 `has_visible_open`·`visible_passive_rects` 는 이 팝업을 세지 않는다. 판정은 팝업 id 가 아니라 선언 값으로 한다. 이 규칙은 host 팝업에 한한다. plugin 팝업은 `keyboard: none` 을 선언해도 아직 기존 WebView 오버레이 규칙을 따른다(`keyboard` 는 plugin 팝업 인스턴스에 전달하지 않는다, §plugin popup 의 실행 중 선언과 열기).
- 비포커스 상태의 별도 시각 표현은 없다. 포커스가 없는 다른 팝업(알림 패널 등)과 같은 모양이다.
- 팝업 내용이 자기가 키를 받는지 확인할 때는 `PopupCx.keys.receives()` 를 읽는다. draw 동안에는 매니저를 빌릴 수 없어 관리자가 그리기를 시작할 때 키 주인과 선언 `keyboard` 로 정해 넘긴다.

파일 피커의 범위 회귀시험(`src/adapters/ipc/handler/file_picker_scope_tests.rs`)은 plugin이 연 직후의
passive 상태와 실제 포인터 클릭으로 포커스를 받은 상태를 각각 숨겼다가 복원한다. 두 경우 모두
선택·요청·z 순서를 보존하고 숨은 동안 paint/hit/Esc/키 게이트에 참여하지 않으며 포커스 노드도 아니다.
복원하면 피커가 포커스된 surface 를 덮으므로 두 경우 모두 포커스 노드가 된다. passive 표시는 클릭한 경우에만
풀려 있다.

**타입 차원 강제**: Core/Domain 핸들러는 `UiIntent`(`OpenPopup`/`ClosePopup`/`TogglePopup`)를 요청하는 메서드를 갖지 않는다 — GUI adapter(단축키 핸들러·메뉴 콜백·popup draw)에서만 요청할 수 있다. `state.popups.open*` 직접 호출도 금지(Intent 경유). 디스패치 상세는 [`design/flows/action-dispatch.md`](../flows/action-dispatch.md).

## 포커스

새 입력·팝업 동작은 경우별 예외 규칙을 더하기보다 하나의 전파 구조와 그 안의 위치·선언값으로 정한다. 설계 지침으로서 전파 구조는 포커스 대상(키)이나 포인터 대상(마우스)을 시작점으로 노드 트리를 한 노드씩 올라가며 전하고, 어느 노드가 처리하면 전파를 멈춘다. 아래 포커스 노드와 [입력 계층](../../architecture/input-layer.md)은 지금 구현된 경로이며, 모든 입력 경로가 이미 이 지침의 형태라는 뜻은 아니다. "이 상황에서 이 키를 누가 받는가"는 먼저 이 구조에서 답을 찾는다.

팝업 포커스는 창의 **포커스 노드** 하나다(`MainViewState::focus_node`, `src/state/focus_node.rs`). 노드는 무대, 설정 열기 요청, 팝업(host 인스턴스 또는 plugin 인스턴스, `PopupNode`), HTML 주소 띠, 튜토리얼, surface 중 하나이고 앞의 것이 이긴다. 팝업 노드는 `PopupManager` 의 포커스 칸 하나에 있고 host·plugin 팝업 포커스는 이 칸에서만 읽는다. 그래서 같은 선언의 인스턴스가 여러 범위에 열려 있어도 포커스는 하나이고, 다른 범위의 팝업은 열린 채 포커스만 잃는다. 키는 포커스 노드에서 시작한다(위 [§수명 계약](#plugin-popup--host-popup-부모-자식)의 "Esc 소유권"). bare Esc가 아닌 경로 키는 팝업이 쓰지 않으면 위로 오른다. 팝업이 받는 키는 `key_route::start_takes`가, 받지 않는 키가 오를 범위는 `key_nodes::window_key_path`가 정한다. plugin 팝업 선언의 `keyboard` 기본값은 `Text`이고 선언이 `Keys`·`None`을 고를 수 있다(`PopupDeclSpec`). 덮는 팝업이 포커스면 덮인 범위 level의 단축키와 더블탭은 돌지 않는다. 규칙과 경로 키의 범위는 [입력 계층의 키 입력 경로](../../architecture/input-layer.md#키-입력-경로)에 있다. 보이는 범위에 포커스 노드인 팝업이 있으면 키보드 입력이 터미널로 안 간다. `PopupManager::has_focused()`는 칸이 보이는 host 팝업인지 확인한다.

- **누름**: 누름 스냅샷 자리(`MainViewState::take_press_snapshot`)가 직전 프레임의 포인터 대상으로 포커스 노드를 정한다. 창의 마우스 처리와 debug 포인터 주입이 같은 함수를 부르고, 별도 창은 자기 누름 경로에서 정한다. 팝업을 누르면 그 팝업이 노드가 되고 passive 가 풀린다(`keyboard = None` 팝업은 제외). 범위 밖으로 나온 드롭다운 부분을 누르면 그 부모 팝업의 노드로 본다. 팝업 밖(surface, 창 크롬)을 누르면 칸을 비워 노드가 surface 로 돌아간다. 그래서 다른 범위의 화면(탐색기 찾기 입력란 등)을 누르면 다음 글자부터 그 화면이 받고, 포커스를 잃은 팝업은 닫히지 않는다. 오버레이 모드가 숨김이 아니면 페이지 위 클릭도 이 바깥 클릭 규칙을 따르고 페이지로 가지 않는다([WebView §영역과 backend 계약](webview.md#영역과-backend-계약)).
- **닫기**: 포커스 노드인 팝업이 닫히면 칸이 비거나 부모 팝업으로 돌아간다.
- **열기**: 사용자가 연 팝업은 포커스 노드가 된다. plugin 팝업은 입력 시점에 사용자가 그 자리에 있었으면 사용자가 연 것이다. Tools 메뉴·명령 팔레트·단축키로 연 팝업, 사건을 낸 그 surface 가 여는 시점의 엔진 포커스 surface 일 때 surface 사건으로 연 팝업, 그 plugin 의 surface 가 엔진 포커스 surface 일 때 plugin API(`popup.show`)로 연 팝업이 그렇다. 같은 plugin 의 배경 surface 가 낸 사건으로 연 팝업은 passive 다. 포커스 노드는 그 창에 그리는 팝업만 될 수 있어, 다른 창에 결속한 창 범위 plugin 팝업을 다시 요청해도 요청한 창의 포커스 노드는 바뀌지 않는다. 판정은 함수 하나이고(`plugin_bridge::popup_scope::opened_by_user`) plugin 이름이나 종류로 나누지 않는다. IPC 로 연 팝업은 passive 다.
- **정규화**(`MainViewState::normalize_focus`): 팝업 층을 그리기 전과 키·IME 사건을 처리하기 전에 돈다. 먼저 칸의 팝업이 키를 받을 수 없으면(닫힘, 범위 숨음, 접힌 묶음의 자식, 숨은 plugin 팝업) 칸을 비운다. 접힌 묶음은 보기 버튼으로, plugin 부모 아래에서 접힌 host 피커는 plugin 부모로 칸을 넘긴다. 다음으로 엔진 포커스 surface 가 바뀌었는데 칸의 팝업 논리 범위가 새 포커스 경로 위에 없으면 칸을 비운다. 처음 본 surface 는 기록만 한다. 마지막으로 덮는 팝업을 본다. 칸에 키를 받을 수 있는 팝업이 있으면 그 팝업의 배치 범위 경로에서 그 팝업보다 위인 덮는 팝업을, 없으면 엔진 포커스 경로에서 덮는 팝업을 찾아 있으면 그 팝업을 노드로 한다. 덮인 자리는 키를 받을 수 없으므로 passive 팝업도 포함한다. 팝업(host·plugin)이 포커스 노드가 되면 HTML 주소 띠와 튜토리얼 말풍선은 키보드 포커스를 놓는다(포커스 노드는 창에 하나다). 주소 띠의 키 재해석은 주소 띠가 포커스 노드일 때만 돈다. 튜토리얼 말풍선은 포커스 노드가 팝업이면 그리지 않으므로, egui 키 포커스를 가졌던 다음 단추도 그 Enter 로 눌리지 않는다.
- 숨은 팝업은 포커스 노드가 아니다. 범위가 다시 보여도 포커스를 되찾지 않는다(덮는 팝업은 정규화로 다시 노드가 된다). 숨김 때 드래그·리사이즈 캡처는 해제한다. native WebView를 가리는 overlay 판정은 열린 popup 중 보이는 것만 센다.

Modal 의 전역 입력 독점과 다르다 — 팝업 포커스는 **키보드만** 차단하고, 마우스는 [입력 계층](../../architecture/input-layer.md)에 따라 팝업이 소비한다.

plugin의 egui-mesh 팝업은 포커스 노드일 때 키보드 입력을 차단한다. 열려 있기만 한 plugin 팝업(다른 범위, passive)은 키를 가져가지 않는다. PluginManager에 접근할 수 없는 winit 입력 핸들러는 포커스 칸에서 읽은 `MainViewState::plugin_popup_owns_keys`를 쓴다. 렌더 프레임이 갱신하는 `MainViewState.plugin_popup_open` 캐시는 WebView 가림 판정(`has_egui_overlay_open`)만 읽는다.

경로 키는 키 경로의 수신자 하나(`MainView::key_decision`)가 egui에 실을지와 터미널로 보내지 않을지를 함께 정한다(`window_feeds_key`·`window_key_holds_terminal`). 경로 키가 아닌 사건(`key_class::classify_key`)과 IME는 `MainViewState::keyboard_overlay_open()`이 두 판정을 함께 정한다. 서로 다른 조건을 쓰면 양쪽이 모두 처리하거나 모두 버릴 수 있다. IME 라우팅도 같은 조건을 쓴다.

`set_ime_allowed`만은 plugin 팝업을 차단 조건에서 제외한다. 이 팝업에는 호스트 egui 위젯이 없으므로 IME를 끄면 조합 입력을 할 수 없다. `collect_mesh_popup_input`이 이미 호스트 egui에 들어온 이벤트를 plugin에 보내고, 후보창 위치는 plugin이 반환한 값으로 정한다([egui-mesh 채널](../../dev-guide/egui-mesh-channel.md)).

plugin 팝업의 범위 보임과 이 캐시는 렌더 프레임에 갱신되므로 popup 이 열린 **직후 최대 1 프레임** 늦게 반영된다 — 짧은 지연이므로, 이 값을 읽는 입력 경로에서는 열린 직후의 프레임 차이를 고려한다. 사용자가 연 plugin 팝업은 열 때 포커스 칸에 바로 들어간다.

### sticky_focus

선언 값은 남아 있지만 포커스 노드 판정은 이 값을 읽지 않는다. 바깥 누름은 늘 포커스 칸을 비우므로 `true` 여도 바깥 클릭에 포커스가 유지되지 않는다. 지금 이 값을 쓰는 것은 명령 팔레트 하나이며, 팔레트는 `close_on_outside_click` 도 참이라 바깥 클릭에 닫히므로 동작 차이가 없다. 검색 바와 알림 패널은 `sticky_focus` 가 거짓이라 바깥 클릭에 포커스를 잃고 열린 채 남는다.

### close_on_outside_click

`false`(기본): 바깥 클릭 시 언포커스만(팝업 유지, 예: 알림 패널). `true`: 바깥 클릭 시 닫음(예: surface 타입 전환).

## 스코프

팝업은 소속 범위(`PopupScope`)를 가지며 가시성·경계가 결정된다. enum: `Window` / `Workspace(u32)` / `Pane(u32)` / `Tab(u32)` / `Surface(u32)`. 모든 대상은 ID다. 에이전트가 앞쪽 workspace를 닫거나 workspace·탭 순서를 바꿔도 팝업은 연 대상을 따라간다. 같은 인덱스로 밀려온 다른 대상에서 보이지 않는다. `Tab`의 경계는 그 탭이 활성인 pane에서 찾는다. 범위 대상 workspace·pane·탭·surface가 닫혀 사라지면 매 프레임 팝업을 그리기 전에 그 팝업을 닫고 `on_close`를 실행한다. 열린 채 숨어 같은 팝업을 다시 열 수 없게 되는 일을 막는다. 비활성 탭에 있어 보이지 않을 뿐인 surface는 사라진 것으로 보지 않는다. surface 변환은 같은 surface ID를 유지하므로 이 정리로 팝업이 닫히지 않는다. 플러그인 팝업이 연 파일 피커가 이렇게 닫히면 요청한 플러그인은 취소 결과를 받는다. `Window` 범위만 이 정리 대상이 아니다. 범위와 작용 대상이 다른 팝업(요소 자체를 다뤄 그 요소가 보이는 상위 자리로 연 팝업: 이름 바꾸기, 탭·pane 프리셋 적용, 강제 분리·원격 닫기 확인)은 같은 정리 단계가 팝업별 대상 판정(`src/adapters/ui/popup/frame.rs`의 `TARGET_CHECKS`)도 불러, 범위가 살아 있어도 대상이 사라지면 닫는다. 범위가 보이지 않아 그리기 단계의 대상 검사가 돌지 않을 때도 이 판정은 돈다. 그래서 대상이 사라진 이름 바꾸기가 숨은 채 남아 터미널 키를 막는 일은 없다. 대상이 살아 있고 범위만 보이지 않는 이름 바꾸기(활성 workspace·탭을 바꾼 경우)는 닫지 않으며, 그동안 텍스트 입력 게이트는 열린 채로 남는다.

| 스코프 | 가시성 | 경계 clamp | scrim 이 덮는 rect |
|--------|--------|-----------|--------------------|
| Window | 항상(워크스페이스 전환 무관) | 윈도우 | 윈도우 |
| Workspace | 해당 워크스페이스 활성 시 | workspace 영역(탭 바와 pane, 사이드바·상태바 제외) | workspace 영역(탭 바와 pane, 사이드바·상태바 제외) |
| Pane | 해당 pane 보일 때 | pane | pane |
| Tab | 해당 탭이 활성 탭일 때 | 탭 바 아래 콘텐츠 영역 | 탭 바 아래 콘텐츠 영역 |
| Surface | 해당 surface 보일 때 | surface **안쪽 8pt** | surface |

`PopupManager::draw()` 가 `LayoutContext` 를 받아 스코프별 가시성 필터 + clamp rect 를 결정한다.

clamp 는 위치를 경계 안으로 옮긴다. 범위보다 큰 팝업을 어떻게 보일지는 선언 `decl.fit` 이 정한다([좁은 범위: 접기와 펼침](#좁은-범위-접기와-펼침)). `Shrink`(검색 바)만 크기를 경계 이하로 줄인다. 팝업은 필요 크기(`needed_size`)와 그린 크기를 따로 가지므로, 줄어든 뒤 경계가 다시 넓어지면 필요 크기로 돌아간다.

Windows 에서 장식 없는 Tasty 창은 최소화 중에도 작업 표시줄 아이콘 크기(측정 160x28)를 client 크기로 보고한다. 이 값은 창 경계가 아니므로 egui 화면으로 쓰지 않고 직전 화면을 유지한다(`keep_screen_while_minimized`). 최소화한 동안 팝업은 clamp 로 줄거나 옮겨지지 않고, 복원하면 같은 자리·크기로 보인다.

### 좁은 범위: 접기와 펼침

창이 아닌 범위의 팝업이 범위보다 크면 접는다(`decl.fit = Collapse`, 검색 바 밖의 모든 host 팝업과 모든 plugin 팝업). 표시 상태는 인스턴스의 `PopupDisplay`(`src/adapters/ui/popup/display.rs`)이고, plugin 팝업은 팝업 관리자(`PopupManager::plugin_expanded`)에 펼침만 기록한다. 다시 접기는 진입점(Esc·접기 버튼 포인터·Enter·Space, host·plugin 어느 쪽)과 무관하게 `refold`·`refold_plugin` 한 번에 묶음의 host 팝업과 펼친 plugin 부모를 함께 접는다. 디자인 정본은 tasty-page 갤러리 Overlays › Dialogs 의 collapsed popup · popup focus Spec 이고 본체와 갤러리가 `tasty_ui_widgets::collapsed_popup` 을 함께 쓴다.

| 상태 | 언제 | 그리는 것 | 배치 범위 |
|---|---|---|---|
| Fits | 매 프레임 필요 크기가 판정 경계에 두 축 모두 들어갈 때 | 팝업 그대로 | 논리 범위 |
| Collapsed | 매 프레임 한 축이라도 들어가지 않을 때 | 그 팝업의 프레임에 "팝업 보기" 버튼 하나. 콘텐츠를 그리지 않고 plugin 에는 크기·입력을 보내지 않는다 | 논리 범위 |
| Expanded | 보기 버튼을 누르거나, 키 주인일 때 Enter·Space | 창 위의 팝업. 자기 제목줄이 없는 팝업도 host 제목줄을 받는다 | 창(`placement_scope() = Window`) |

- 판정 경계는 범위 사각형에서 선언 `scope_inset` 이면 `min(space-sm, 폭/2, 높이/2)` 를 네 변에서 뺀 것이다. 필요 크기는 sizer 가 마지막으로 잰 값(plugin 은 선언 크기)이다. host·plugin 이 같은 함수(`display::collapse_placement`)로 판정하고, plugin 선언의 `fit`·`scope_inset`·`covers_scope`·`escape` 도 host 와 같은 뜻이다(`fit = Shrink` 면 접지 않는다).
- 접힌 버튼은 경계 가운데에 둔다. plugin `Cursor` 앵커 팝업은 포인터 자리에 두고 경계 안으로 민다. 라벨 버튼 프레임이 들어가지 않으면 아이콘 버튼 프레임이고, 그것도 들어가지 않는 축은 경계 시작에 붙이고 범위에서 자른다. 버튼만 히트 사각형이다. 덮는 팝업이면 접혀도 범위를 덮는다.
- 펼친 팝업은 범위가 넓어져도 판정하지 않고 남는다. 창 가운데에 둔다. 창보다 크면 프레임을 창에 맞춰 왼쪽·위에 두므로 제목줄과 접기·닫기 버튼은 늘 창 안에 온전히 보이고, 내용은 필요 크기를 유지해 넘친 오른쪽·아래만 잘린다(`tasty_ui_widgets::place_expanded`, host·plugin·갤러리 공용). 덮음 선언과 무관하게 창을 덮는다(scrim·포인터·바깥 누름 판정이 창 범위로 나온다). 논리 범위(`scope`)는 바뀌지 않으므로 논리 범위가 숨으면 펼친 팝업도 숨는다.
- 펼친 팝업의 제목줄 버튼 묶음 맨 앞(전체화면·닫기 왼쪽, 다른 버튼이 없으면 닫기 자리)에 접기(`shrink`) 버튼이 있다. 접기 버튼이나 Esc(키 주인일 때)는 다시 판정 상태로 돌린다. 닫기 사건(`on_close`, plugin `popup.closed`)은 없고 포커스·입력 값·인스턴스가 그대로다. 키 주인이 펼친 host 팝업이면 창은 포커스를 풀지 않고 그 Esc 를 본문에 넘기고, 본문이 쓰지 않았으면 팝업 층 중앙 단계가 그 Esc 에 묶인 인스턴스를 접는다(선언 `escape` 와 무관하다). 펼친 plugin 팝업이면 그 프레임 렌더가 접으며 그 Esc 를 plugin 내용에 보내지 않는다. 아래 알림 목록이나 host 팝업은 그 Esc 로 닫히거나 접히지 않는다.
- 자식은 부모가 펼쳤으면 함께 펼치고, 부모가 접혔으면 부모 버튼 하나가 묶음을 대표한다(자식은 그리지 않는다). 부모가 들어가면 자식은 자기 크기로 판정한다. 펼친 묶음에서 Esc 나 접기 버튼은 묶음 뿌리부터 접는다. 부모가 plugin 팝업인 host 파일 피커(`OpenRequest::plugin_parent`)도 같다. plugin 렌더가 적은 부모 표시 상태를 다음 host 판정이 따르고, 어느 쪽을 다시 접어도 함께 접힌다. 묶음이 접히면 그리지 않는 자식의 포커스는 보이는 보기 버튼으로 옮기고, 그 자식은 키 후보에서 빠진다.
- 닫았다 다시 열면 판정부터 한다.
- 접기 버튼은 host·plugin·갤러리가 함께 쓰는 위젯(`tasty_ui_widgets::popup_collapse_button`)이다. 접근 이름은 `popup.collapse_button.label` 이고 Tab 키 포커스를 받으며, 포커스가 있을 때 Enter·Space 로 접는다. 아이콘형 보기 버튼은 라벨을 접근 이름으로 갖는다. 두 버튼 모두 hover 나 키 포커스일 때 툴팁을 보인다.

키 포커스를 가진(키 주인) 좁은 범위 팝업과 접힌 버튼 프레임은 셸 위쪽에 포커스 가장자리(`popup-focus-edge`)를 그린다. 창 범위와 펼친 팝업은 그리지 않는다.

`Workspace` 스코프의 clamp·clip·scrim 은 활성 workspace 의 콘텐츠 영역(탭 바와 pane, `LayoutContext::workspace_rect`)이다. 사이드바·상태바는 범위 밖이다(아래 "scrim 의 범위"). workspace 를 옮기면 그리지 않으므로 이 스코프는 가시성도 정한다.

**스코프는 `PopupDef` 에 못 박히지 않는다.** 등록 때는 `Window` 로 두고, 여는 쪽이 열 때마다
범위를 정한다. `OpenRequest`와 `PopupManager`의 모든 열기 함수가 범위를 받으므로
범위를 주지 않는 열기는 컴파일되지 않는다.

**작용 대상 범위 규칙**: 범위는 팝업이 작용하는 대상(surface·탭·pane·workspace)이 표시되고 조작되는 자리에서 나온다.

- 요소 자체를 다루는 팝업(이름 바꾸기, 닫기·분리 확인, 프리셋으로 그 요소를 통째로 바꾸기)은 그 요소가 보이는 상위 컨테이너 범위로 연다. 폴더 이름을 그 폴더 안이 아니라 상위 폴더에서 바꾸는 것과 같다. workspace 는 창, 탭은 그 탭의 pane, pane 은 그 pane 의 workspace 다. 그래서 활성이 아닌 workspace 를 다루는 팝업도 workspace 를 전환하지 않고 지금 창에 뜬다. 메뉴·단축키·명령 어느 경로로 열어도 범위는 같다.
- 요소의 내용 안에서 일어나는 팝업(파일 피커, 파일 처리기 선택, 배너 메뉴, 업로드·다운로드, plugin surface 팝업, explorer 항목 이름 바꾸기)은 그 요소 범위다.
- explorer 경로의 즐겨찾기 추가는 Window 범위다. 즐겨찾기 목록은 메뉴를 연 explorer surface 의 자료가 아니라 모든 explorer 와 윈도우가 함께 쓰는 전역 목록(`~/.tasty/explorer-favorites.toml`, [explorer](../../surfaces/explorer/index.md))이기 때문이다.

대상을 찾지 못하면 창 범위로 바꾸지 않고 열지 않는다 — 사용자 동작은 비활성 항목이거나 경고 로그,
IPC 요청은 `-32602` 다. 대상 없이 창 전체에 작용하는 팝업(도구 메뉴, 명령 팔레트, 설정 등)도 Window 다. 등록 값이나 직전에 열렸을 때의 범위는 다음 열기에 남지 않는다. 선언의 `decl.scope_kind`는 지금 모든 정의가 `Window`이고 등록 범위와 같다는 것만 시험이 확인한다. 현재 소비자:

| 팝업 | 스코프 | 여는 쪽이 주입하는 값 |
|------|--------|----------------------|
| [DAG 목록](../../features/agent-collaboration/screens/dag-list-popup.md) (`dag_list`) | Workspace | 여는 시점의 활성 workspace ID |
| 변환(`convert_surface`) · 검색바(`search_bar`) | Surface | 포커스 surface id |
| 부모가 있는 파일 피커(`file_picker`) | 부모의 결속 범위 | trigger 때 부모 선언 종류+target을 해석하고, 그리기 전마다 같은 계산으로 따라간다 |
| 부모 없는 plugin 파일 피커(`file_picker`) | Surface | 요청의 `origin_surface_id` |
| 새 탭 메뉴의 파일 피커(`file_picker`) | Pane | 새 탭을 만들 pane id |
| 마우스 캡처 배너 메뉴(`mouse_capture_banner_menu`) | Surface | 배너가 있는 surface |
| 탭 프리셋 적용(`apply_tab_preset`) | Pane | 대상은 메뉴를 연 pane(단축키는 포커스 pane)에서 선택된 탭이고, 그 탭을 통째로 바꾸므로 범위는 그 탭이 보이는 pane 이다. 적용은 그 탭의 pane에 한다 |
| pane 프리셋 적용(`apply_pane_preset`) | Workspace | 대상은 메뉴를 연 pane(단축키는 포커스 pane)이고, 그 pane 을 통째로 바꾸므로 범위는 그 pane 이 있는 workspace 다. 적용도 그 pane에 한다 |
| 강제 분리 확인(`confirm_force_detach_workspace`) · 원격 닫기 확인(`confirm_close_remote`) · workspace 이름·부제 바꾸기(`rename`) | Window | workspace 자체를 다루므로 그 workspace 가 보이는 창. 두 확인은 `src/adapters/ui/workspace_action_popup.rs` 한 함수로, 이름 바꾸기는 `RenameTarget::popup_scope`로 연다. 대상이 활성 workspace 가 아니어도 workspace 를 전환하지 않고 지금 창에 그린다. 다른 workspace 요청이 오면 범위는 그대로이고 내용과 대상만 바뀐다 |
| 탭 이름 바꾸기(`rename`) | Pane | 탭 자체를 다루므로 그 탭이 보이는 pane(탭 바가 있는 곳). 탭의 pane 을 찾지 못하면 열지 않고 경고 로그를 남긴다. 다른 pane 의 탭 요청은 그 pane 에 따로 열리고, 같은 pane 의 요청은 대상과 입력만 바꾼다 |
| 업로드 진행·실패(`transfer_progress` · `transfer_error`) | Surface | 업로드를 시작한 surface. 진행 팝업은 surface마다 하나씩 열리고 그 surface의 행만 가진다. 실패 팝업도 범위마다 따로 열리고 같은 범위의 실패만 차례로 보인다 |
| 다운로드 후 실행(`download_run` · `download_run_large` · `download_run_progress`) | Surface | 원격 파일을 연 surface. 크기 확인·진행·실패도 그 다운로드의 surface 다 |
| 파일 처리기 선택(`file_handler_picker`) | Surface 또는 Window | 요청 출처 surface. 터미널 링크의 연결 동작은 링크를 누른 터미널. 출처 없는 창 단위 요청(출처 없는 IPC·에이전트 요청)은 Window |

`Workspace` 스코프 팝업은 워크스페이스를 옮기면 **그리지 않는다** — 상태는 그대로 남아 있어
돌아오면 보던 화면이 그대로 복원된다. 그 상태의 수명은 스코프가 아니라 `on_close` 가 정한다.

### scrim 의 범위

scrim은 팝업이 속한 범위를 덮는다(이 문서). 범위 사각형은 `PopupManager::scope_rect` 하나가 정하고 배치·clamp·clip·scrim이 같은 값을 쓴다. Surface 범위는 해당 surface의 보더까지 포함하고 인접 surface·사이드바·탭바·상태바는 제외한다. Pane 범위는 탭 바를 포함한 pane 사각형(`LayoutContext::pane_rects`)이고, Tab 범위는 탭 바 아래 활성 탭의 콘텐츠 영역(`LayoutContext::tab_content_rects`)이다. Workspace 범위는 활성 workspace의 콘텐츠 영역(탭 바와 pane, `LayoutContext::workspace_rect`)이며 사이드바·상태바는 범위 밖이다. 저장된 원격 행의 연결 팝업도 이 규칙으로 같은 사각형을 쓴다.

모서리 형태는 범위 대상에 맞춘다. 현재 surface의 반경은 0이며 코드는 범위와 관계없이 반경 0과 같은 `scrim()` 색을 사용한다. 창 범위와 그 프레임에서 범위를 찾지 못한 경우(workspace 영역이 없는 창 포함)는 창 전체를 덮는다.

같은 범위의 팝업이 여럿이어도 scrim은 한 번 그린다. 창 scrim이 있으면 내부 surface scrim은 생략한다. 같은 알파를 중복 적용해 특정 영역만 더 어두워지는 것을 막는다.

**어느 팝업이 scrim 을 까는가는 범위가 아니라 덮음(`PopupState::covers()`)으로 정한다.** 선언 `decl.covers_scope` 의 기본은 덮음이고, 창 위로 펼친 팝업은 선언과 무관하게 창을 덮는다. 범위에 붙은 메뉴와 안내만 덮지 않는다: `approval` · `modifier_hint` · `mouse_capture_banner_menu` · `notifications` · `rail_category` · `restore_notice` · `search_bar` · `tools_menu`, 그리고 열 때 덮음을 정하는 `remote_row`(마지막 화면 위에 열면 덮는다). `search_bar` 는 `Surface` 범위를 쓰지만 anchored + scrim-less 갈래다([`tasty-ui-widgets` — 본체·갤러리 공유 UI primitive](../../architecture/ui-widgets-crate.md)). plugin 팝업은 선언 `covers_scope`(기본 덮음)를 따른다. 목록은 `src/adapters/ui/popup/decl_table_tests.rs` 가 고정한다.

### 덮는 팝업과 포인터

포인터 대상은 위에서부터 찾고, 덮는 팝업을 만나면 그 배치 범위 아래로 내려가지 않는다. 덮는 범위 안에서는 상자 밖이어도 대상이 그 팝업이다. 덮인 범위의 위젯·터미널·배너는 누름·뗌·휠·hover·이동 보고를 받지 않고, 파일을 놓을 수도 없다. 판정 자리는 [입력 계층](../../architecture/input-layer.md#순서-2--덮는-팝업은-그-범위의-포인터-입력을-모두-가진다)에 있다.

- 포인터 대상은 하나다(`MainViewState::popup_pointer_target`). 직전 프레임에 배치·clamp 를 마친 host 상자·범위로 자른 접힌 버튼·자식 드롭다운 오버레이, plugin 상자, 덮는 팝업의 덮는 사각형을 host·plugin 합친 z 순서로 보고 맨 위를 고른다. 창의 마우스 처리, 커서, 놓기, 배너가 이 판정을 쓴다. 직전 위치의 hover 값을 쓰지 않는다.
- 뗌은 누름이 간 곳으로 간다. 아래(터미널·PTY)에서 누른 버튼의 뗌은 덮인 자리에서 떼도 아래로 가고, 위(팝업·egui)가 가져간 누름의 뗌은 위가 받는다. plugin 팝업과 배너의 mesh 도 자기가 받은 누름의 뗌만 받는다(`MeshPointerRoute`). OS 창 테두리 리사이즈를 시작한 누름의 뗌은 리사이즈 것이라 어디에도 가지 않는다. 한계: egui-mesh surface(markdown 등)로 보내는 뗌은 아직 좌표로 정한다.
- 배너는 포인터 대상이 그 배너일 때만 hover 툴팁을 보인다. 포인터가 있던 배너가 덮이면 `PointerGone` 을 받아 hover 를 푼다.
- 놓기는 포인터 대상이 팝업이면 아래에 닿지 않는다. 창 전체 게이트(`keyboard_overlay_open`)로 막지 않으므로 다른 범위의 탐색기 놓기는 그대로 된다. 남아 있던 탐색기 hover 대상도 이 판정을 먼저 거친다.
- 앱이 처리하는 OS 창 테두리 리사이즈는 덮음 밖이라 덮는 팝업이 열려 있어도 시작된다. 리사이즈를 시작한 누름과 그 짝인 뗌은 egui·팝업·surface 어디에도 가지 않는다(`MainView::begin_os_resize_press`, debug 주입 경로도 같다). 그래서 펼친 팝업이 그 누름을 바깥 누름으로 받아 닫히지 않고, 마우스 보고를 켠 터미널이 짝 없는 뗌을 받지 않는다. OS 가 리사이즈 중 뗌을 보내지 않으면 같은 버튼의 다음 누름이 그 기록을 지운다. 메뉴를 닫는 클릭으로 삼킨 누름·뗌도 리사이즈를 시작하지 않으면서 같은 버튼의 기록을 지운다.

상자 밖 누름이 그 팝업에 하는 일(`popup::press::outside_press` 하나를 host `route_outside_press` 와 plugin 렌더가 함께 쓴다):

- 누른 좌표가 팝업의 배치 범위 밖이면 아무 일도 없다.
- 그 좌표를 더 위의 팝업(host 상자·접힌 버튼·자식 드롭다운 오버레이·덮는 범위, plugin 상자·덮는 범위)이 덮었으면 알림을 받지 않는다.
- 누르기 직전 포커스 노드(키 주인 팝업이 있으면 그 논리 범위, 없으면 엔진 포커스 surface, 누름 스냅샷 하나에 남긴다)가 논리 범위 밖이었으면 들어오는 첫 클릭이라 닫거나 포커스를 풀지 않는다. 덮는 팝업이면 그 범위 안 누름이 팝업 것이라 포커스를 받는다.
- 그 밖에는 바깥 클릭 선언대로 닫는다. 닫지 않는 덮는 팝업은 포커스를 받고, 덮지 않는 팝업은 포커스를 잃는다(포커스 노드가 누른 자리로 옮긴다).

scrim 은 어둡게 칠하는 그림이다. 입력은 위 덮음 판정이 정하고, Esc 와 바깥 클릭의 순서는 위 §수명 계약이 정한다.

### plugin popup 의 실행 중 선언과 열기

plugin 은 매니페스트 없이 실행 중에 팝업을 선언하고 연다.

- `popup.declare { popup_id, decl }` 는 선언을 등록하거나 바꾼다. `decl` 은 부분 선언(`PopupDeclSpec`)이다. `scope_kind`·`size` 만 필수이고, 준 필드만 host 의 plugin 기본 선언(`PopupDecl::plugin`: 제목줄·끌기 손잡이 없음, 나머지는 `PopupDecl::new` 기본값) 위에 덮는다. 셸 모양(`shell`·`shadow`·`content_margin`·`chrome`·`drag_handle`)과 `scope_target` 은 plugin 이 정하지 않으며 모르는 필드는 무시한다. id·크기·기준 검사는 매니페스트 선언과 같은 함수(`validate_popup_decl`)로 하고, `at_point` 기준은 거절한다. 매니페스트 `api_version` 이 host 와 다르면 거절한다.
- 같은 id 의 실행 중 선언이 매니페스트 선언보다 먼저다. 이미 열린 인스턴스는 연 때의 선언을 유지한다. 선언은 plugin 프로세스가 끝나면 지워지고, 새 프로세스가 시작할 때 다시 선언한다.
- `popup.show { popup_id, target, context? }` 는 인스턴스를 열고 `{ instance_id }` 로 답한다. `target` 은 필수다. host 가 대신 고르는 기본 대상은 없다. 종류는 선언의 `scope_kind` 와 같아야 하고 창 범위로 낮추지 않는다.

| `target` | 대상 |
|---|---|
| `{ "kind": "surface", "id": N }` | 그 surface. 요청 plugin 소유 surface 여야 한다 |
| `{ "kind": "window", "id": N }` | 그 창. 인스턴스는 그 창에만 그려진다 |
| `{ "kind": "focused_window" }` | 요청을 받은 시점에 포커스된 창. 그때 한 번 정하고 인스턴스는 그 창에만 그려진다. 포커스된 창이 없으면 열지 않는다 |
| `workspace`·`pane`·`tab` `{ "id": N }` | 아직 열지 않는다(`-32602`) |

- 오류: 형식이 틀렸거나 `target` 이 없음, 선언 없음, 종류 불일치, 다른 plugin 의 surface, 열린 인스턴스와 범위가 다름은 `-32602`. 없는 창·surface, 포커스된 창 없음은 `-32004`. 거절하면 인스턴스를 만들거나 바꾸지 않는다.
- plugin 팝업 인스턴스는 (plugin, popup) 마다 하나다. 이미 열려 있으면 그 id 로 답하고 `popup.open` 을 다시 보내지 않는다. 열린 인스턴스의 범위(창·surface)가 이번 `target` 의 범위와 다르면 거절한다(`-32602`, 먼저 닫아야 한다). 다른 창을 가리켜도 열린 인스턴스는 처음 결속한 창에 남는다.

실행 중 선언으로 연 인스턴스에 지금 반영되는 값은 다음 넷이다. plugin 팝업 렌더러는 이 값만 받는다.

| 선언 필드 | 인스턴스에 반영되는 값 |
|---|---|
| `scope_kind` | `window`·`surface` 만 연다. `workspace`·`pane`·`tab` 선언은 열지 않는다 |
| `size` | 처음 크기. 정수 논리 픽셀로 반올림한다 |
| `anchor` | `cursor` 는 포인터 자리. `scope_center`·`scope_top`·`at_point`·`column` 은 범위 가운데 |
| `outside_click` | `close` 만 바깥 클릭으로 닫는다. `unfocus`·`ignore` 는 닫지 않는다. 실행 중 선언의 기본값이 `unfocus` 라서 기본 선언 팝업은 바깥 클릭으로 닫히지 않는다 |

- 받기는 하지만 인스턴스에 전달하지 않는 선언 필드: `instances`(인스턴스는 위처럼 (plugin, popup) 마다 하나), `covers_scope`, `fit`, `scope_inset`, `focus_on_open`, `keyboard`, `escape`, `sticky_focus`, `resizable`, `min_size`. 선언과 `popup_decl` 조회에는 남는다.
- host 가 정하는 필드(`scope_target`·`chrome`·`drag_handle`·`resize_handle`·`shell`·`shadow`·`content_margin`)도 인스턴스에 전달하지 않는다. plugin 팝업 렌더러는 자기 틀(egui-mesh)로 그린다.
- host 는 인스턴스를 만든 뒤 `popup.open` 을 보내고 응답한다. plugin 이 요청 처리 스레드에서 `popup.show` 를 불렀다면 `popup.open` 은 응답을 받은 뒤 처리된다.
- `popup.open` 을 보내지 못하면(송신 큐 포화·연결 종료) 인스턴스를 지우고 열지 않는다(`PluginManager::open_popup_instance` 가 `None`). plugin 이 `popup.open` 에 오류로 답하면 그 인스턴스와 그린 프레임을 지우고(plugin 에 `popup.closed` 는 보내지 않는다), host 는 다음 plugin 팝업 사건 처리에서 각 창의 결속에서 그 id 를 지운다(`take_failed_popup_opens`, `plugin_popup_events::forget_failed_popup_opens`). 포커스 노드였으면 살아 있지 않은 노드가 되어 포커스가 아래 노드로 돌아가고 뒤 글자를 삼키지 않는다. `popup.open` 요청에는 만료가 없다(요청 만료 수거 대상이 아니다). 첫 렌더가 오지 않는 것은 로그로만 남긴다.
- 실행 중 선언으로 연 팝업의 출처는 plugin 이다.

### plugin popup 의 스코프

plugin popup(`[[contributes.popup]]`)은 매니페스트 `scope` 로 범위의 **종류**만 선언한다 —
`window`(기본) 또는 `surface`. 대상은 popup 을 여는 host 진입점이 정한다(이 문서). 대상 surface 는
인스턴스에 바인딩하고, 대상 창은 그 창의 `MainViewState.plugin_window_popups` 에 인스턴스를 넣어
결속한다. plugin 팝업 인스턴스는 PluginManager 하나에 있으므로 창 범위 인스턴스는 결속한 창에만
그린다. 진입점은 결속하지 않은 창 범위 인스턴스를 만들지 않는다. 팝업은 범위 노드인 창의 자식이라
창을 닫으면 그 창에 결속한 plugin 팝업도 사유 `HostShutdown` 으로 닫힌다(plugin 에는 릴리스된
plugin 이 아는 사유만 보낸다). 창 상태를 버리기 전에 모든 창
상태에 닫기 큐 처리를 바로 적용하므로 같은 회차에 다른 창에서 다시 열면 새 인스턴스가 그 창에 결속되고,
그 팝업의 자식 파일 피커는 어느 창에 열려 있든 취소되어 plugin 에 취소 결과가 간다(아래 연쇄 정리). 마지막 창을 닫아 창 상태를 보관(park)하는 경우는 창 상태가 남으므로 결속도 남고,
그 상태가 다시 창에 붙으면 그 창에 그려진다. 이미 열린 인스턴스를 다시 열면 같은 인스턴스를 돌려받고
처음 결속한 대상(surface·창)을 유지한다.
가시성·경계는 위 표와 같은 판정 함수로 같은 frame 의 `LayoutContext` 에서 정한다.

이벤트 trigger popup 의 대상은 host 가 확인한다. 매니저는 surface 트리를 모르므로 `surface`
선언 + 대상이 있는 이벤트 popup 을 대기열에 두고, 앱이 같은 frame 에 꺼내 대상 surface
(webview 또는 egui-mesh plugin surface)의 소유 plugin 을 대조한다. 발행자 대조는 매니저가
대상 유무와 관계없이 먼저 한다 — 다른 plugin 이 발행한 이벤트로는 `surface` 선언 popup 을
창 범위로도 열지 않는다. 이 발행자 검사는 `surface` 선언 popup 에만 적용한다. `window` 선언
popup 은 다른 plugin 이 발행한 이벤트로도 지금처럼 열린다 — 소유를 확인할 대상이 없고, 신뢰
경계는 surface 를 지목하는 popup 에 둔다. 거절하면 popup 을 열지 않고 경고 로그만 남긴다. 발행 자체는 성공한다. `window` 로 낮춰 열지 않는다 — 지목한 대상이 조용히
무시되고 창 scrim 이 다른 surface 를 덮기 때문이다.

| 진입점 | 바인딩되는 대상 |
|--------|----------------|
| 변환 입력 popup(`convert_input_popup`) | 제자리 변환이면 그 surface, 새 탭이면 여는 시점의 focus surface. attach 생성 대기 초안이 선택돼 있으면 그 초안이 생성한 surface. 창 범위는 입력이 일어난 창 |
| 도구 메뉴·명령 팔레트·명령 단축키 popup | 입력 시점의 focus surface. attach 생성 대기 초안이 선택돼 있으면 그 초안이 생성한 surface. 창 범위는 입력이 일어난 창(초안에 걸어 둔 명령도 그 초안의 창). 팔레트·단축키로 여는 내장 이름 바꾸기·프리셋 적용·dag 목록 popup 도 입력 시점의 탭·pane·workspace 에 결속한다 |
| 이벤트 trigger popup | `surface` 범위 이벤트(`EventScope::Surface`)의 payload `surface_id`. surface 선언은 대상이 그 popup 을 선언한 plugin 의 surface 가 아니거나, 다른 plugin 이 발행한 이벤트면 열지 않는다. window 선언은 그 surface 가 있는 창이다 |
| `popup.show` | 요청의 `target`(위 "plugin popup 의 실행 중 선언과 열기") |
| `debug.popup.open` | surface 선언은 요청의 `surface_id`. window 선언은 요청의 `window_id`(창이 하나면 생략 가능) |

`surface` 선언 popup 에 대상이 없으면 `window` 로 바꾸지 않고 열지 않는다. 변환 입력 popup·도구 메뉴·명령
팔레트·명령 단축키는 생성 대기 초안도 대상으로 삼으므로, 대상 surface 도 초안도 없을 때만 열지 않고 경고
로그를 남긴다. 대상 없는 이벤트도 열지 않고 경고 로그만 남긴다(이벤트는 응답이 없다). `debug.popup.open` 은 `surface_id` 가 없거나 없는 surface 를 가리키면 `-32602` 로 거절한다.
`window` 선언 popup 은 대상 surface 가 없어도 입력이 일어난 창에 열린다. 창을 정할 수 없는 진입점은
모든 창에 띄우지 않고 열지 않는다. 대상 surface 가 없는 이벤트는 경고 로그만 남기고, 창이 여럿인데
`window_id` 가 없는 `debug.popup.open` 은 `-32000` 으로 거절한다. 변환 입력 popup·도구 메뉴·명령
팔레트·명령 단축키에서 초안이 대상이면 `window` 선언 popup 도 생성된 surface 를 문맥으로 생성 뒤에 연다. 새 탭용 변환 입력 popup 의
문맥에는 제자리 변환 표시(`surface_id`)가 없다.

`surface` 범위 popup은 앵커의 가운데 기준도 그 surface 영역이다. 범위가 안 보이는 frame 에는
셸·콘텐츠 합성·히트테스트 rect·Esc·바깥 클릭·키 게이트 어디에도 들어가지 않는다 — 보이지
않는 rect 가 클릭을 삼키지 않는다. 인스턴스는 살아 있어 범위가 다시 보이면 그대로 복원된다.
scrim 도 그 범위를 덮는다(위 §scrim 의 범위). 보이는 범위의 안쪽(테두리 inset 을 뺀 영역)이
선언 크기(`size_hint`)보다 좁거나 낮으면 host 팝업과 같이 범위 안의 보기 버튼으로 접고, 누르면 창 위로
펼친다(위 §좁은 범위: 접기와 펼침). 펼친 plugin 팝업은 선언 크기에 host 제목줄을 더한 상자이고(창보다 크면
프레임은 창에 맞추고 콘텐츠는 선언 크기로 합성해 넘친 오른쪽·아래가 잘린다), plugin 콘텐츠는 제목줄 아래
영역에 합성한다. 접기·펼침은 표시만 바꾼다. 자식 파일 피커는 부모의 선언 종류와 결속 대상으로 정한 범위를
쓰고, 표시 상태는 부모를 따른다. 부모를 펼쳐 둔 채 연 피커는 펼친 상태로 열린다.

현재 `scope = "surface"` 선언: markdown `file-open` · `large-file-confirm`.

## Modal 과의 차이

| 항목 | Popup | Modal |
|------|-------|-------|
| 입력 차단 | 키보드: 포커스 시 / 마우스: 입력 계층 | 전역 독점 |
| 동시 열기 | 여럿 | 최대 1 |
| 위치 | 자유 이동 | 중앙 고정 |
| 구현 | PopupManager | 별도 OS 윈도우(View) |

## 관련

- [toast.md](toast.md) — 휘발성 알림 (별도 시스템)
- [banner.md](banner.md) — parent 상단 info+action 오버레이 (별도 시스템)
- [fullscreen-stage.md](fullscreen-stage.md) — 타이틀바 전체화면 버튼이 여는 창 전체 무대 (별도 시스템)
- [dev-guide/popup-implementation](../../dev-guide/popup-implementation.md) — 팝업 추가 절차
- [concepts/ubiquitous-language](../../concepts/ubiquitous-language.md) — Window/Modal/Popup/Toast/Banner 구분

## 복원 오류 안내의 포커스

[복구 원칙](../../principles/feedback-and-recovery.md)의 심각한 복원 실패는 앱 우측 상단 부근에 포커스를 가져가지 않는 팝업으로 알리는 대상이다. 복원 자료를 읽지 못한 안내는 `restore_notice` 정의의 창 범위 인스턴스다(`src/adapters/ui/restore_notice.rs`, 위젯 `tasty_ui_widgets::restore_notice`). 안내마다 인스턴스 하나(`Multi`)이고 작업 영역 오른쪽 위 안내 열(`anchor = Column`)에 연 순서로 쌓인다. 시스템 사건이 여는 팝업이라 passive로 연다. `keyboard = None`이라 겹친 WebView는 숨기지도 키보드 포커스를 잃지도 않고 카드 자리만 구멍으로 뺀다([passive 팝업](#passive-팝업)). `keyboard = None`이라 열거나 눌러도 팝업 포커스를 받지 않고 다른 팝업 아래에 그린다. 범위를 덮지 않고 바깥 클릭·Esc에 닫히지 않으며, ×·Continue·안쪽 컨트롤의 Esc가 같은 응답이다. 셸은 콘텐츠가 그린다(`PopupShell::Content`). 안내 높이는 직전 프레임에 잰 값이라 새 안내는 첫 프레임을 1px 높이로 그리고 다음 프레임에 잰 높이로 늘어난다. 그래서 새 안내 아래에 쌓인 안내가 한 번 아래로 움직일 수 있다. 검색 막대를 같은 프레임에 처음 열거나 옮기면 안내 열은 직전 프레임의 막대 위치를 피하며, 다음 프레임에 맞춰진다. 중앙 부팅 오류 화면은 홈 사용 중·첫 workspace 생성 실패·App 생성·첫 화면 조립처럼 계속할 수 없는 실패에만 남는다. 일반 알림 상세 팝업은 사용자가 열어 읽는 별도 화면이다.
