<a id="design--gallery--host-3자-매핑"></a>

# 디자인·갤러리·본체 대응표

디자인 JSX의 컴포넌트, 본체 함수, 갤러리 카탈로그 항목의 대응표다. 구조를 옮길 때의 제약은 [디자인과 구현의 차이](design-parity-notes.md), 색·치수·배율 규칙은 [테마 가이드](theme.md#ui-디자인-규칙-필수)를 함께 확인한다.

갤러리는 `cargo run -p tasty-gallery`로 실행한다. 상단 도구 모음에서 테마·UI 배율을 바꾸고 왼쪽 카탈로그에서 예제를 선택한다. 등록 위치는 `crates/tasty-gallery/src/catalog/{components,widgets}/<name>.rs`의 `draw(ui, theme)`와 `catalog.rs::pages()`의 `section(...)`·`spec(...)`이다.

## remote_tool (Overlays)

디자인 `ui_kits/terminal/overlays/remote_tool.jsx` ↔ `src/adapters/ui/popup/remote_tool.rs`.

| 디자인 jsx 컴포넌트 | tasty 함수 | 갤러리 항목 |
|---|---|---|
| `RemoteTool`(container) | `draw_remote_tool_popup` | `components/remote.rs` `remote` spec (프레임 + 3탭 + add-bar + 목록). 셸(프레임·add-bar·목록 배치)은 갤러리가 본체 구성을 다시 그린 사본, **안의 탭 스트립과 로컬 ssh 섹션은 공용 view 호출** |
| `TabBtn`(내부, 3탭) | `draw_tab_bar` (wrapper) → `tasty_ui_widgets::draw_tab_strip` | `components/remote.rs` `tab_bar` 이 **같은 공용 view 를 호출**한다 |
| `WarnBadge` | `tasty_ui_widgets::warn_badge` — 경고 글리프(icon-glyph-xs) + `font-label` micro 글자 pill. 높이 `tag-size`, 좌우 `tag-padding-x`, 간격 `tag-gap`, 모서리 `tag-radius`, 채움 accent-warning × `tint-fill-alpha`, 테두리 accent-warning 40% | 같은 공용 view 를 세 탭 행과 프로토콜 필터가 부른다 |
| `ListShell` | `draw_profile_list` / `draw_attach_list` / `draw_passkey_list` (add-bar+scroll 합침) | — |
| `ProtocolFilter`(add-bar 버튼) | `tasty_ui_widgets::draw_protocol_filter_button` | `components/remote.rs` `remote-filter` spec — 닫힘 2 상태(가린 것 없음 / 1 개 가림) |
| `ProtocolFilter`(드롭다운/팝오버) | `tasty_ui_widgets::draw_protocol_filter_body` (본체 wrapper `draw_protocol_filter` 가 memory·배치·닫기를 소유) | `components/remote.rs` `remote-filter` spec — 열림 1 상태 |
| 행 셸(`ProfileRow`·`AttachRow`·`PasskeyRow` 공통) | `tasty_ui_widgets::remote_list_row` + `remote_row_title` — 위아래 `space-md`·좌우 `space-xs`, 텍스트 열과 동작 묶음 사이 `space-sm`, 줄 간격 `size-2`, 아래 1px `separator`. 동작 묶음(Sm IconButton, 간격 `size-1`)은 행 오른쪽 끝 고정 자리라 긴 텍스트가 밀어내지 않는다. 첫 줄은 이름(body, text-primary · 흐린 행은 text-disabled) + `  (label)`(text-muted)을 칩 폭을 뺀 나머지에서 말줄임하고, 칩은 Tag 또는 `WarnBadge` 다. 시안의 이름 weight 600 은 egui 에 굵은 UI 글꼴이 없어 [표현 차이](design-parity-notes.md#css와-egui의-표현-차이)대로 색으로만 근사한다 | 갤러리 세 행이 같은 공용 view 를 부른다 |
| `ProfileRow` | `draw_profile_row` | `components/remote.rs` `profile_row` (`remote` spec) — 시안 seed 처럼 감지 실패(legacy-box, passkey missing 배지)와 모르는 type(scratch) 행을 포함한다. 행 동작 버튼은 본체와 같이 오른쪽부터 삭제(`TRASH`)·편집·재탐지(ssh 만) |
| `ProfileForm` | `draw_profile_form` | `components/remote/forms.rs` `form_card` (`remote-profile-form` spec — SSH 추가·Host 검증 오류, `remote-generic-passkey-forms` spec — generic key-value와 Unknown type 배지). 머리줄·탭·행·세그먼트는 `remote.rs`의 Attach 폼 헬퍼를 함께 쓴다 |
| `LocalSshSection`(kit 정의 — 위 `space-md` 여백·`border-frame` 선·`space-sm` 안쪽 여백, 헤더와 빈 상태 줄 `size-2`/`space-xs`, 행 `space-xs`·alias↔target `label-detail-gap`) | `tasty_ui_widgets::draw_local_ssh_section` (본체 wrapper: `remote_tool.rs` 동명 함수 — i18n + 빈 상태 원인 판정) | `components/remote.rs` `remote` spec 이 **같은 공용 view 를 호출**한다. 호스트 3건 목록 옆에 no hosts·no file·unreadable config 빈 상태 세 장 |
| `AttachRow` | `draw_attach_row` | `components/remote.rs` `attach_row` (`remote-attach` spec) — 행 삭제 아이콘은 본체와 같이 `TRASH`. `x`(`CLOSE`)는 닫기·해제·필드 제거에만 쓴다 |
| `AttachForm` | `draw_attach_form` | `components/remote.rs` `attach_form_card` (`remote-attach-form` spec, ref/inline 2변종) |
| `PasskeyRow` | `draw_passkey_row` | `components/remote/passkeys.rs` `passkey_row` (`remote-passkeys` spec, Attach 앞) — gallery 미러 `RemoteFrame tab="passkeys"`의 네 행(가림, 긴 경로 보임(IconButton active + eyeOff), 가림, 모르는 kind)을 Mocha·Latte 두 장으로 그린다. 시안 seed kind `file`·`secret`은 tasty kind `path`·`inline`으로 옮긴다. 행 구조는 kit `PasskeyRow`(행 `space-md space-xs`, 이름 + kind Tag, 둘째 줄 mono 한 줄 말줄임, 동작 reveal·편집·trash 간격 `size-1`)다. 본체와 갤러리 모두 공용 행 셸을 쓴다 |
| `PasskeyForm` | `draw_passkey_form` | `components/remote/forms.rs` `form_card` (`remote-generic-passkey-forms` spec — kind path 한 줄, inline 세 줄 secret) |
| `ConfirmDelete` | `draw_confirm_delete` | — |
| `PasskeySelect` | `passkey_dropdown_row` | — |

Attach 갤러리 specimen 은 디자인 **gallery 미러**(`gallery/parts/overlay-frames.jsx` `RemoteFrame
tab="attach"` / `RemoteFormFrame` variant `attach-ref`·`attach-inline`)를 전사한 것이다.

탭 스트립과 세그먼트는 역할과 색이 다르다. 화면을 전환하는 탭은 2px `accent-primary` 밑줄과 weight 600을 사용한다. 값을 선택하는 세그먼트는 `accent-primary` 채움과 `text-on-accent` 글자를 쓴다. `surface-active`는 행 선택용이므로 두 컴포넌트에 대신 쓰지 않는다. 본체의 `tasty_ui_widgets::segmented`와 갤러리의 `seg_chip`, explorer 툴바의 view-mode 토글(본체 `seg_toggle`와 갤러리 미러, `segtoggle-on-bg`·`segtoggle-on-fg` 토큰)이 같은 규칙을 따른다.

로컬 SSH config는 카드 대신 섹션 헤더와 2줄 행으로 표시한다. 헤더는 11px 대문자 라벨·고정폭 경로·오른쪽 개수이며, 행은 alias와 `user@host:port`다. 행 아이콘 버튼 대신 ghost `Add profile`을 사용하고, 등록된 호스트에는 `in profiles` Tag를 표시한다. 호스트 없음·파일 없음·읽기 실패는 `text-muted` 한 줄로 알리며, 읽기 실패 문구는 권한·디렉터리·UTF-8 아님 같은 원인을 나누지 않는다. 본체와 갤러리 모두 `tasty_ui_widgets::draw_local_ssh_section`을 호출한다.

디자인과의 차이는 헤더·빈 줄 위 세로 여백 2px다. 4px 그리드 밖이며 대응 토큰이 없어 적용하지 않았다. 가로 들여쓰기 4px는 `space-xs`를 사용한다. 헤더 자간은 `letter-spacing-caps`(0.04em)다. 생성 접근자 `letter_spacing_caps(font_size_caption)`의 값을 `TextFormat::extra_letter_spacing`으로 적용하며 caption 11px에서는 0.44px이다.

**셸은 공유하지 않고 사본으로 둔다.** `draw_remote_tool_popup`은 MainViewState·CoreState를 받으므로 갤러리의 `(ui, &Theme)` 콜백에서 직접 호출할 수 없다. 프로필·Passkey 읽기, 폼 상태, `FILTER_MEMORY_ID`·`FILTER_POPUP_ID`, 배치는 본체가 맡는다. 셸을 props 로 떼어 내면 이 상태 전부를 갤러리 쪽 가짜 값으로 다시 만들어야 하므로, 공유 범위는 상태 없이 그릴 수 있는 내부 위젯(탭 스트립, 로컬 SSH 섹션, 필터 버튼·드롭다운, 버튼·배지·텍스트 헬퍼)으로 한정한다. 이 함수들은 `crates/tasty-ui-widgets/src/remote_tool.rs`에서 props를 받아 본체와 갤러리가 공유한다.

사본인 셸과 행은 캡처를 맞대어 비교한다. 격리한 debug 인스턴스에서 `tasty debug host-popup open --popup-id remote_tool`로 본체 팝업을 열고 `tasty screenshot --window <id>`로 Profiles·Attach·Passkeys 탭을 Mocha·Latte 각각 찍는다. 갤러리는 `TASTY_GALLERY_SHOT`으로 Overlays 페이지의 `remote`·`remote-attach` 카드를 찍고, Latte는 `TASTY_GALLERY_THEME=latte`를 함께 준다. 두 캡처에서 프레임 여백, add-bar 구성, 행 높이, 행 동작 버튼의 아이콘·순서·비활성 표시가 같은지 본다. 셸을 바꾸는 커밋은 같은 방법으로 변경 전후를 비교한다.

필터 예제는 닫힘과 열림 모습을 나란히 보여 준다. 열림 전이를 재현하지 않으며 목록 높이를 먼저 확보한다. 본체 팝업과 달리 갤러리 카드의 남은 높이가 작으면 같은 ScrollArea도 마지막 행을 자르기 때문이다.

필터 드롭다운 폭은 `remote-filter-dropdown-width`(240) 토큰이며 테두리를 포함한 폭(border-box)이다. 공용 `filter_dropdown_content_width`가 호출자의 프레임 좌우 합(테두리 + 안쪽 여백)을 빼 안쪽 최소 폭을 낸다. 본체는 egui popup 프레임(`Frame::popup`)의 `total_margin`을, 갤러리 카드는 좌우 테두리 두 개를 뺀다. 그래서 두 구현의 바깥 테두리 폭이 모두 토큰 값과 같다.

드롭다운 틀은 버튼에 붙는 팝오버 공통인 메뉴 컨테이너 토큰(`menu-bg` · `menu-border` · `menu-radius` · `shadow-popover`)이다. 본체는 `apply_theme_to_egui`가 egui popup 틀에 이 값을 넣고, 갤러리 카드는 같은 토큰을 쓰는 `frame_card_menu`에 담는다.

드롭다운 안쪽은 kit `ProtocolFilter`의 네 구획(제목 · 목록 · 일괄 선택 링크 · Reset/Apply)을 공용 `draw_protocol_filter_body`가 그린다. 구획마다 위아래 `space-sm`, 좌우 `space-md` 여백을 두고 구획 사이에 `separator` 1px 선을 프레임 폭 전체에 긋는다. 그래서 호출자는 안쪽 여백이 없는 프레임에 담는다. 본체는 popup을 띄우는 scope의 `menu_margin`을 0으로 두고, popup 안쪽 Ui는 컨텍스트 스타일을 받으므로 프레임 여백은 그 scope에서 잰다. 갤러리 카드는 `frame_card_menu`에 그대로 담는다. 목록 스크롤은 `min_scrolled_height`도 상한 168로 두어, 가로로 감싸는 갤러리 행처럼 바깥 높이가 좁은 자리에서도 내용 높이(최대 168)까지 자란다. 제목은 대문자 `font-label` `font-size-micro`, 자간 `letter-spacing-caps`(0.04em, 생성 접근자 `letter_spacing_caps(font_size)`), 링크는 `font-size-caption` accent 글자 버튼과 separator 색 `·`이다. 체크박스 라벨은 공용 checkbox 라벨(body 크기, UI 글꼴)이며 시안도 같은 라벨을 쓴다.

## remote_attach — RA02 "Add remote workspace" (Overlays)

디자인 `ui_kits/terminal/overlays/remote_attach.jsx` `RemoteAttach` (+ 갤러리 미러
`gallery/parts/overlay-frames.jsx` `RemoteAttachFrame({state})`) ↔ 본체
`src/adapters/ui/popup/remote_attach.rs`.

갤러리 공개 진입점 `remote_attach::{draw, draw_new_row, draw_states, draw_empty_plans, draw_decisions}`와 본체 대조용
치수 상수는 `catalog/components/remote_attach.rs`에 유지한다. 행·pane 구현 좌표는 아래와 같다.

| 디자인 jsx 컴포넌트 | 갤러리 항목 (`catalog/components/` 기준) | 본체 함수 |
|---|---|---|
| `RemoteAttach`(container) | `remote_attach.rs`의 `ra_card` (`header`+`body`+`footer`, 680×460 프레임) | `draw_remote_attach_popup` |
| `RaAttachProfileRow` | `remote_attach/rows.rs`의 `profile_row` (`remote-workspace-attach` spec 좌 pane) | `profile_row` |
| `RaNewWsRow` | `remote_attach/new_row.rs`의 `new_ws_row` + `dot_slot_glyph` / `new_ws_error` / `row_separator` (`remote-workspace-attach-new-row` spec, 5상태) | `draw_ws_list` → `new_ws_row` (+ `dot_slot_glyph` / `new_ws_error` / `row_separator`) |
| `RaRemoteWsRow` | `remote_attach/rows.rs`의 `ws_row` (+ `dot_slot_status`) | `ws_row` |
| DS `CenterState`(우측 pane 의 initial·connecting·error) | `remote_attach/panes.rs`의 `right_pane` → 공용 `CenterState` (`remote-workspace-attach-states` spec) | `draw_right_pane` → 공용 `CenterState` |
| `RemoteAttachFrame emptyPlan="A"`(채택하지 않은 비교안) | `RaState::EmptyPlanA` → `remote_attach/panes.rs`의 `plan_a_center`. 공용 `CenterState`가 아니라 시안 `center()` 값을 쓴다: `paneEmpty` 16 × 1.4 `text-placeholder` · 제목 13 `text-muted` · 보조 줄 11(호스트 mono, 줄 높이 1.5) · Secondary sm 생성 버튼 · 안쪽 여백 `space-xl`/`space-lg` · 간격 `space-sm` (`remote-workspace-attach-empty-plans` spec, plan B 카드와 나란히) | 없음 — 본체는 plan B(빈 목록이면 새 행을 미리 선택)만 구현한다 |
| `RaInUseBadge` | `remote_attach/rows.rs`의 `tag`(`attached`) | `ws_row`의 `tag`(`attached`) |
| `RaInactiveBadge` | `remote_attach/rows.rs`의 `tag_with_icon`(`warning`) | `profile_row`의 `tag_with_icon`(`warning`) |
| loaded 렌더 경로(`conn==="loaded"`) | `remote_attach/panes.rs`의 `loaded_pane` (+ `remote_attach/rows.rs`의 `empty_line`) | `draw_right_pane`의 `Loaded` 분기 → `draw_ws_list` |
| footer `Connect` / `Create & connect` | `remote_attach.rs`의 `footer` | `draw_footer` |

**"+ New workspace" 행 (RA02).** 우측 목록의 **첫 행**으로, 원격에 워크스페이스를 하나
만들어 그것을 mirror 하는 경로다(이름/cwd 를 묻지 않는다 — 원격 기본값). 버튼이 아니라
**목록 행**이라 이웃 ws 행과 같은 select-then-confirm 을 따르고, 확정은 footer 가 한다
(그때 라벨이 `Create & connect` 로 바뀐다). 실제 ws 행과는 **세 채널 동시**로 구분한다 —
`plus` 글리프 · accent 라벨 · 행 아래 1px 구분선. 색 하나로만 구분하지 않는다.

- **empty(원격 ws 0개)는 center-state 가 아니다.** loaded 렌더 경로는 **하나**이고, ws 가
  없으면 caps 헤더 + 새 행 하나 + muted 한 줄로 degrade 한다. 그 행은 **미리 선택**돼 있어
  pane 이 뜬 순간부터 footer 가 살아 있다. center-state + CTA 버튼 안은 같은 동작의 확정
  방식이 원격 상태에 따라 둘로 갈리므로 채택하지 않았다.
- **selected 에서만 라벨이 accent → text-primary 로 바뀐다.** accent 를 `surface-active`
  위에 남기면 3.17:1 이라 고른 순간 가장 안 읽힌다. 구분은 글리프·구분선·accent 바가 계속
  진다.
- **글리프는 status-dot 슬롯(8px) 안에서 center.** 14px `plus` 가 슬롯 좌우로 대칭
  overflow 하므로 이름 열의 좌측 정렬선이 아래 ws 행들과 픽셀 동일하다. 갤러리에서는 ws
  행의 dot 도 `remote_attach/rows.rs`의 같은 `dot_slot`으로 슬롯을 잡는다 — `status_dot` 위젯이 라벨이 비어도 dot
  뒤에 자기 gap 을 할당해서, 그대로 부르면 두 행의 이름 열이 6px 어긋난다.
- **생성 중 / 실패는 행 인라인.** 왕복이 1~3초라 pane 을 통째로 바꾸면 사용자가 읽던 목록을
  버린다(생성 중엔 아래 목록 dim + inert). 실패도 목록을 가리지 않는다 — 실패 후 다음 수가
  보통 기존 워크스페이스 선택이기 때문. 원격 메시지는 3줄 clamp + 전문은 tooltip.

**본체 구현**: `draw_ws_list`의 첫 행 `new_ws_row`가 `ListAction::Select(WsSel::New)`를
반환한다. footer 확정은 `start_create` → `spawn_create`의 원격 `workspace.create`로
이어지고, `poll_create`가 받은 새 workspace ID는 `push_attach`를 통해 기존 attach 큐에
합류한다. 상세 동작은 [remote-attach](../../features/remote-attach/index.md)의 GUI picker 절을 따른다.
살아 있는 터널 포트로 생성 요청을 보내며 왕복 상한은 `src/adapters/ui/popup/remote_attach.rs`의
상수를 따른다. 새 예제의 반영 순서는 [gallery-first](../../dev-guide/gallery-first.md)를 따른다.

## switch_overlay (Overlays)

디자인 `gallery/layouts/index.jsx` "Switch-number overlay" 섹션 ↔ 본체 draw
(`src/adapters/ui/tab_bar.rs` 탭 스트립 + `sidebar/view.rs` full/collapsed). 탭·사이드바의 본체와 갤러리 대응은 아래 표와 같다.

| 디자인 jsx 컴포넌트 | 갤러리 항목 (`catalog/components/switch_overlay.rs`) | 본체 함수 |
|---|---|---|
| `NumCap`(키캡) | `keycap_at` (헬퍼) — 공용 위젯 `tasty_ui_widgets::num_keycap` 호출 | ✅ `switch_overlay::paint_keycap` (공통) — 같은 그리기 함수를 `paint_num_keycap` 으로 호출 |
| `TabStripMock` | `tab_strip` → `draw_tab` (`switch-tab` specimen) | ✅ `tab_bar/view.rs` `draw_pane_tab_bars_view` → `tab_bar/tab.rs` `draw_tab` (아이콘 위치에 표시) |
| `WsRowMock` / `SidebarMock` | `full_ws` → `draw_workspace` (`switch-ws` specimen, full) | ✅ `sidebar/view.rs` `draw_workspace_card` (상태 점 위치에 표시) |
| `RailMock` | `rail_ws` → `draw_workspace` (collapsed cluster) | ✅ `sidebar/view.rs` `draw_collapsed_sidebar_view` (아바타 위치에 표시) |
| `CatSwitchSidebarMock` | `full_cat` → `draw_category` (`switch-cat` specimen, full) | ✅ `sidebar/view.rs` (헤더 우측 키캡, `category_switch_held`) |
| `CatSwitchRailMock` | `rail_cat` → `draw_category` (collapsed cluster) | ✅ `sidebar/view.rs` `draw_rail_category_button` (`---` 중앙 키캡) |

공용 모듈 `src/adapters/ui/switch_overlay.rs`가 보조키와 대상의 대응(`switch_target_for`), 키캡 그리기(`paint_keycap`), 숫자 매핑(`tab_digit` 0~9, `workspace_digit` 1~9)을 담당한다.

탭바는 `ModifiersChanged`에서 갱신한 `state.switch_overlay()`로부터 `switch_overlay_pane: Option<u32>`를 받아 `PaneTabBarsProps`에 전달한다. `tab_keycap_for`는 포커스된 pane에만 키캡을 표시한다. 단축키도 그 pane의 탭만 전환하므로 다른 pane은 기존 아이콘을 유지한다.

사이드바의 `full`·`collapsed` wrapper는 입력 보조키와 설정에서 `workspace_switch_held`를 계산한다. workspace 전환은 pane에 한정되지 않는다. 키캡은 기존 탭 아이콘·상태 점·레일 아바타의 16px 슬롯을 그대로 사용하고 키를 놓으면 원래 표시로 돌아간다. 실제 사용자 보조키만 읽으므로 IPC로 강제 표시할 수 없다.

**카테고리 quick-switch (기본 Ctrl+Shift, `draw_category`)**: 카테고리는 자기 modifier 필드
(`category_switch_modifier`, 기본 `"ctrl+shift"`)를 갖는 별도 설정이다. `switch_target_for` 가 세 축(탭/워크스페이스/카테고리)
각각의 modifier 조합을 `Combo::parse_modifiers` 로 파싱해 현재 눌린 조합과 **정확히 일치**할 때만
해당 대상을 반환한다. 다른 보조키가 추가되면 일치하지 않으며 우선순위로 대상을 고르지 않는다. full 은
카테고리 헤더 **우측**에 키캡(chevron은 접기 상태를 표시하므로 유지, status dot 없음), rail 은 `---` 경계
**중앙**에 키캡. 번호는 reserved normal("Workspaces")=1, 1–9 then 0(10th), 11th+ 없음. 전환 시 접힘이면 자동
확장(슬롯 파일 영속) + 그 카테고리 last-active 이동(`state/workspace.rs` `switch_to_category`, 다음/이전
카테고리 자체 전환은 `next_category`/`prev_category` 가 이 함수를 재사용). folders 토글 게이트.
discoverability 는 modifier-hint 패널의 `HintRole::CategorySwitch`(폴더 글리프, folders on).

**"개별 지정" 모드와 오버레이**: 세 축 중 하나라도 modifier 를 "개별 지정"
(`KeybindingSettings::INDIVIDUAL_SWITCH_MODIFIER`)으로 바꾸면 그 축은 `switch_target_for` 가 절대
반환하지 않으므로(sentinel 파싱 실패) 이 switch-number 오버레이가 그 축에서 자동으로 뜨지 않는다 —
슬롯마다 콤보가 달라 통일된 숫자 힌트를 그릴 근거가 없기 때문(의도된 동작, [keybindings](../../features/keybindings/index.md) 참조).

**등록**: `catalog.rs` Overlays 페이지 `section("switch", "Switch-number overlay", [spec("switch-tab",
…, draw_tab), spec("switch-ws", …, draw_workspace), spec("switch-cat", …, draw_category)])` — search 와
approval 사이(디자인 순서와 동일). 3 specimen(tab / workspace / category), workspace·category 는 released /
held-full / released-rail / held-rail cluster.

`kbd()`·`num_keycap()`은 자신의 영역을 할당하는 egui 위젯이라 이미 정해진 16px 슬롯에 바로 넣을 수 없다. 공용 `paint_num_keycap(painter, theme, center, ..)`는 주어진 좌표에 그림만 그린다. `num_keycap`과 본체의 `paint_keycap` 모두 이 함수를 사용한다.

색과 치수는 `switch-overlay-*` component 토큰을 따른다. 모두 `kbd-*` 별칭이며 active만 `accent_primary`·`text_on_accent`를 사용한다. 별도 Theme 필드는 없다.

## preset demo-layout (Overlays)

디자인 `gallery/overlays-windows/preset-editor.jsx` (`SurfaceView`/`Pane`/`PaneTree`/`SurfaceBox`) ↔ 갤러리
`catalog/components/preset_editor.rs` ↔ 본체 `src/adapters/ui/preset/demo_layout.rs`. 저장된
`Preset*` 트리를 **구조만** 축소 렌더하는 read-only 미리보기다. 라이브 surface
렌더(터미널 GPU/WebView)는 재사용하지 않고 전용 placeholder 위젯으로 그린다.

| 디자인 jsx 컴포넌트 | 갤러리 (`preset_editor.rs`) | 본체 (`demo_layout.rs`) |
|---|---|---|
| `SurfaceBox` (leaf, kind 라벨만) | `draw_surface_box` | `draw_surface_box` (`Leaf{kind,label}`) |
| `SurfaceView` (하위 surface split, 1px hairline) | `draw_surf` | `draw_surf` (`SurfNode`) |
| `Pane` (mini tab strip + 활성 탭 본문) | `draw_pane_card` | `draw_pane_card` — strip **클릭 가능**(live) |
| `PaneTree` (상위 pane split, `space-xs` bg-app gap) | `draw_pane_tree` | `draw_pane_tree` (`PaneNode`) |
| `PreviewBody` (scope 분기) | `draw_scope_body` | `DemoLayout::show` (`Root::Panes`/`TabFrame`) |
| `KINDS`(아이콘/accent) | `Kind::{icon,accent}` (정적 5종 — terminal·markdown·editor·log·plugin:portscan) | `kind_icon`/`kind_accent` (kind str→`icons::Icon`, plugin kind 중립 fallback) |
| `PresetWindow` (제목줄 · L1 범위 탭 · 196 목록 · 44 도구줄 · 미리보기) | `preset_view.rs` `draw` (범위 탭·목록 선택·Edit 토글만 동작) | `src/adapters/ui/preset.rs` `draw_preset_panel` |
| `activeKind`(탭 대표 kind) | `tab_kind` | `SurfNode::rep_kind` |
| `SurfaceBox` edit 핸들(`MiniHandle` 설정 · remove) | `draw_handle_cluster_mock` | `draw_handle_cluster` (톱니 → `Act::OpenSettings`, remove → `Act::Remove` · split-right/down 제거 — 경계 존이 대체) |
| `SurfaceBox` `onDoubleClick` → `openCfg` | — (정적) | `draw_surface_box` 의 `double_clicked()`(존 밖) → `Act::OpenSettings` → `ShowOutcome::OpenSettings` |
| `pickZone`/경계 split 존 overlay | `draw_split_zone_overlay_mock` (Left 고정 예시) | `pick_zone` + `draw_split_zone_overlay` (커서 기반 4변 · crosshair · before/row 매핑) |
| mini tab close `×` | `draw_edit_direct_mock` (active rest + hover 예시) | `draw_pane_card` 탭 루프(`show_close` · `Act::RemoveTab`) |
| `AddTabBtn` `+` (22×20 hover) | `draw_edit_direct_mock` (hover 고정) | `draw_pane_card` add-tab(`ADD_TAB_W` · overlay_hover) |

**갤러리 vs 본체 차이**: 갤러리 specimen 은 binary 미의존(정적 샘플 트리·정적 라벨, mini-tab 클릭
전환 없음). 편집 직접조작(경계 split 존·tab ×·add-tab)은 정적이라 hover/pointer/crosshair 축이
없어 **고정 상태 예시**로만 전사한다(정적↔live 차이는 [design-parity-notes](design-parity-notes.md)
"preset 편집기 — 정적 specimen…" 참조). 본체는 실제 `WorkspacePreset`/`TabPreset`/`PanePreset` 을 공통 preview 모델(`SurfNode`/
`PaneNode`/`Root`)로 정규화하고, leaf 라벨을 주입 resolver 로 해석한다. split 방향은 라이브
모델 의미(`Vertical`=좌우/row, `Horizontal`=상하/column, capture·apply 와 일치)를 따른다.

### surface 설정 화면

디자인 `gallery/overlays-windows/preset-editor.jsx` 의 `SurfaceSettings` / `useSurfaceCfg` / `locate` /
`SettingsDemo` ↔ 갤러리 `catalog/components/preset_surface_settings.rs` ↔ 본체
`src/adapters/ui/preset/surface_settings.rs` + `src/adapters/ui/preset/demo_layout/surface_draft.rs`.

| 디자인 jsx | 갤러리 (`preset_surface_settings.rs`) | 본체 |
|---|---|---|
| `SurfaceSettings` (header / body / footer 세 상자) | `draw_screen` | `draw_surface_settings` |
| header (kind 아이콘 · 이름 · breadcrumb · unsaved) | `draw_header` | `draw_header` |
| body (`Field` 목록, max 460, 스크롤) | `draw_body` | `draw_body` → `draw_form` |
| footer (`[Cancel ghost] [OK primary]`) | `draw_footer` | `draw_footer` |
| `Field` 라벨 | `field_label` | `field_label` (`font-label` micro uppercase muted + 3px) |
| `useSurfaceCfg` 의 `{ id, draft, orig }` | — (정적) | `SurfaceCfg` (`PresetView::surface_cfg`) |
| `normalize` / `isDirty` | 프레임별 `dirty` 고정값 | `LeafDraft::is_dirty` |
| `switchKind` | — | `LeafDraft::switch_kind` (값을 지우지 않는다 — 아래 parity-notes) |
| `applyAction({type:"surface"})` | — | `DemoLayout::apply_leaf_draft` |
| `locate` breadcrumb | 데모별 고정 `path` | `DemoLayout::leaf_location` + `breadcrumb` |
| `PresetWindow` 의 `cfgOpen ? <SurfaceSettings/> : 툴바+미리보기` · `dim` | — | `draw_preset_panel` 의 `locked` 분기 · `draw_settings_detail` · `block_input` |
| `SettingsDemo` 상태 프레임 5종 | `demos()` | — |

**kind→표시명 (i18n)**: 라벨은 `surface.kind.<kind>` 키로 해석(= registry `display_name_i18n_key`
규약). 호스트 lang 에 빌트인 `terminal`/`empty` 키를 추가했고(`lang/{en,ko,ja,zh-Hans}.toml`
`[surface.kind]`), plugin kind(markdown/image/…)는 각 plugin lang 의 `[surface.kind]` 가 제공.
`PresetView` 는 main engine 의 공유 `surface_registry` Arc 를 받아 프레임마다 경량 스냅샷
(`KindCatalog`)을 파생한다 — kind 드롭다운 후보는 런타임 등록 kind 를 반영하고, 표시명은
registry `display_name_i18n_key` 로 해석하며(미번역/미등록이면 `fallback_kind_label` capitalize
로 graceful fallback), `empty` 는 후보에서 제외한다. registry 미주입(갤러리·main
부재)이면 빈 catalog → 정적 목록으로 떨어진다.

**본체 연결**: `draw_preset_panel`(`src/adapters/ui/preset.rs`)이 선택 preset 으로 `DemoLayout` 을
빌드해 egui temp memory 에 `(key, layout)` 으로 유지(탭 클릭 전환 지속), 남은 영역에 캔버스
프레임 + `DemoLayout::show`/`show_edit` 렌더. `PresetView` 가 파생한 `KindCatalog` 를
`draw_preset_panel → draw_preview → DemoLayout` 으로 전달해 설정 화면 드롭다운·mutation 라벨의
kind 소스로 쓴다. 설정 화면(`draw_settings_detail`)도 같은 캐시 인스턴스를 읽고, 확인 시 그
사본에 draft 를 적용해 저장이 성공해야만 캐시를 바꾼다.

## workspace-category (Layouts / Overlays)

사이드바 폴더(카테고리) — 확장 그룹 / 축소 레일 `---` / 컨텍스트 메뉴 / 생성·이름변경·삭제
다이얼로그 / 레일 팝업. 갤러리 specimen 은 binary 미의존 정적 재현(Theme 토큰).

| 디자인 jsx 컴포넌트 | 본체 함수 | 갤러리 항목 |
|---|---|---|
| `chrome.jsx` `CategoryHeader` | `sidebar/view.rs::draw_category_header` | `sidebar` "Categories · full" (`sidebar.rs::full_categories`) |
| `chrome.jsx` `Sidebar`(grouped) | `sidebar/view.rs::draw_full_sidebar_view`(+`full.rs::build_category_sections`) | `sidebar` "Categories · full" |
| `chrome.jsx` `RailCategoryBtn` | `sidebar/view.rs::draw_rail_category_button`(선은 `tasty_ui_widgets::paint_rail_category_line`) | `sidebar` "Categories · rail" (`sidebar.rs::rail_categories`, 같은 선 함수) |
| `chrome.jsx` `CollapsedSidebar`(grouped) | `sidebar/view.rs::draw_collapsed_sidebar_view` | `sidebar` "Categories · rail" |
| `overlays/sidebar_context_menu.jsx` `RailCategoryPopup` | `popup/rail_category.rs::draw_rail_category_popup` | `workspace-categories` "Rail popup" (`category_dialogs.rs::rail_category_menu`, 레일 예제와 같은 함수) · `rail-category` 레일 `---` 버튼 오른쪽에 붙은 상태(`category_dialogs.rs::draw_rail`, 시안 `RailCategoryFrame` 정적 사본) |
| `overlays/sidebar_context_menu.jsx` `SidebarContextMenu` | `view/main/redraw.rs`(native menu: Workspace/WorkspaceCategoryHeader/SidebarBackground) | `sidebar-context-menu` 대상별 네 메뉴(`sidebar_context_menu.rs`, 시안 정적 사본) |
| `overlays-shared.jsx` `RenameFrame` · `explorer-ops-remote.jsx` `RenamePending` | `dialog.rs::draw_rename_popup` → 공용 `tasty_ui_widgets::rename_popup` | `rename_popup`(Workspace title · Long name · Latte · Validation error) · `explorer_rename_popup` · `explorer_favorite_popup`(즐겨찾기 추가) · `explorer-rename-popup-ground`. 예제는 본체와 같은 view 를 부르고 자동 포커스만 끈다. 대상별 치수 변형은 `RenamePopupLayout`(Dialog = `RenameFrame`, Explorer = `RenamePending`)이다 |
| `overlays-dialogs.jsx` `CategoryEditFrame` | `dialog.rs::draw_rename_popup`(+`RenameTarget::NewCategory`/`CategoryName`, 라이브 검증) | `workspace-categories` "Create / rename" · "Validation error" (`category_dialogs.rs::edit_dialog`, 같은 공용 rename view) |
| `overlays-dialogs.jsx` `CategoryDeleteFrame` | `popup/confirm_delete_category.rs::draw_confirm_delete_category` | `workspace-categories` "Delete confirm" (`category_dialogs.rs::delete_confirm`) |

**갤러리 vs 본체 차이**: 본체 컨텍스트 메뉴는 OS native(`show_context_menu`)라 같은 함수를 호출할 수 없다.
갤러리 `sidebar-context-menu`는 시안의 대상별 네 메뉴를 공용 메뉴 항목으로 옮긴 정적 사본이다. 시안과
본체 모두 "카테고리로 이동"을 하위 메뉴 없이 비활성 머리글로 두고, 그 아래에 현재 카테고리를 뺀 대상
카테고리를 평면 나열한다. 나머지는 Theme 토큰으로 시각만 재현하며 상태(접힘/빈 카테고리/검증 에러)는
mock 데이터로 주입한다. 앵커 메뉴 팝업(Tools 메뉴·레일 카테고리 팝업·컨텍스트 메뉴 예제)의 틀은 메뉴
컨테이너 토큰(`menu-bg`·`menu-border`·`menu-radius`)이고 안쪽 둘레는 네 변 모두 `popup-content-margin`이다.

## 이미 갤러리에 있는 관련 항목 (참고)

`catalog/components/` 에 등록된 것: `command_palette` · `port_scanner` · `convert` ·
`approval` · `file_handler_picker` · `markdown_open` · `rename_popup` · `toast` ·
`sidebar` · `tab_bar` · `apply_preset`. 이들은 props 분리가 돼 있어 갤러리로 즉시 검증 가능.

<a id="디자인-spec-제목과-갤러리-예제"></a>

## 디자인 Spec 제목과 갤러리 예제

시안 카탈로그(tasty-page `vendor/gallery/<페이지>/index.jsx`)의 `<Spec title>` 과 갤러리 예제 제목은 맞추지 않는다. 디자인
Spec 하나가 갤러리 예제 둘이거나, 둘이 하나이거나, 경계가 다른 경우가 있어 제목만 바꿔서는 1:1 이 되지
않기 때문이다. 아래는 제목이 서로 달라 제목 대조로는 찾을 수 없는 Spec 의 대응이다. 제목 전체나 " — " 앞부분이
같은 Spec 은 제목으로 찾을 수 있어 싣지 않는다. 컴포넌트·본체 함수 대응이 이미 다른 절에 있으면 마지막 열이
그 절을 가리킨다.

| 디자인 Spec | 갤러리 예제 | 컴포넌트 대응 · 비고 |
|---|---|---|
| `gallery/dag/index.jsx` "Transition selection — four states on one edge style" | `dag-routes` | 아래 Task DAG 절 `routes` 행 |
| `gallery/dag/index.jsx` "One row per DAG — across all workspaces" | `dag-rows` | 아래 Task DAG 절 `dagRowItems` 행 |
| `gallery/foundations/index.jsx` "Depth reads through surface tint, never shadow" | `elevation` | 떠 있는 표면의 두 그림자는 `floating` |
| `gallery/layouts/index.jsx` "Full sidebar ↔ collapsed rail" | `sidebar` |  |
| `gallery/layouts/running-count.jsx` "Workspace row — running count" | `workspace-row-running-count` (`components/sidebar_running_count.rs`) | 프레임은 `running-count-values`·`-row-states`·`-attention`·`-narrowest`·`-zero-column`·`-categories`·`-marks`·`-tooltip`과 세 폭(150·180·220) 목록이다. 칸·숫자·몸통 x는 본체 `draw_workspace_card`와 같은 `tasty_ui_widgets` 함수(`running_count_column`·`paint_running_count`·`workspace_row_body_offset`)로 그린다. 0은 숫자와 툴팁 없이 칸만 남겨 몸통 x가 0~99에서 같다. 다른 행 표본(`sidebar`·`sidebar-attached-ring`·attach 로딩·`movesource`·`attention-rows`·거절 행·`switch-ws`·`switch-cat`)도 같은 함수를 쓴다. 실행 중 표본은 값 1, idle은 0이다. 접힌 레일과 드래그 고스트에는 숫자가 없다 |
| `gallery/layouts/index.jsx` "Workspace categories (sidebar folders)" | `sidebar` "Categories · full" · "Categories · rail" | [workspace-category](#workspace-category-layouts--overlays) |
| `gallery/layouts/index.jsx` "Rail marks — category line and mirror chip (2026-10-12)" | `rail-marks` (`components/rail_marks.rs`) | Mocha·Latte 보드마다 카테고리 경계 셋을 둔 레일 조각, 선 상태(idle·hover·pressed·modifier held 키캡·카테고리를 끈 레일의 맨 위 구분선 28 × 2), mirror 칩 상태(idle·active·hover·+notif·+attached ring)를 그린다. 선·맨 위 구분선·칩은 본체 레일과 같은 `tasty_ui_widgets` 함수(`paint_rail_category_line`·`paint_rail_flat_divider`·`paint_workspace_mirror_chip`)로 그리며, 길이·크기는 전용 토큰 `rail-category-line-length`(24)·`rail-flat-divider-length`(28)·`workspace-mirror-chip-size`(12)다. 아바타·버튼은 본체 레일 슬롯 32 × 28 · 32 × 16이다(시안 보드는 36 × 36 · 36 × 16) |
| `gallery/layouts/index.jsx` "Category quick-switch — Ctrl+Shift held" | `switch-cat` (Overlays) | [switch_overlay](#switch_overlay-overlays) `CatSwitchSidebarMock` 행. 디자인은 Layouts, 갤러리는 Overlays 페이지 |
| `gallery/layouts/index.jsx` "One sidebar, one detail pane" | `onedepth` | [1 depth](#1-depth-general-list--detail) |
| `gallery/layouts/index.jsx` "Top tabs over a filterable section list" | `twodepth` | [2 depth](#2-depth-settings-idiom) |
| `gallery/layouts/index.jsx` "Stacked tab tiers — workspace over panes" | `multitab` |  |
| `gallery/layouts/surface-focus.jsx` "Surface focus on every kind" | `surface-focus-every-kind` | `surface_focus.rs`. kind 9개를 Mocha·Latte, focused·unfocused로 그리고 누르면 포커스가 옮는 예제를 둔다. 바탕은 `tasty_ui_widgets::surface_bed`, 띠는 `tasty_ui_widgets::SurfaceFocusEdge`. 시안 states 계약 `Surface focus edge`(host 4 × rest·focus)도 같은 표본에 둔다 |
| `gallery/layouts/surface-focus.jsx` "Focus with rings, marks and keyboard focus" | `surface-focus-edge-with-marks` | `surface_focus.rs`. 칠하는 순서와 링이 있을 때 켜진 띠가 아래 행만 쓰는 규칙은 `SurfaceFocusEdge`에 있다 |
| `gallery/layouts/index.jsx` "Mixed split — terminal + markdown" | `mixed-split` | `layouts_settled.rs::draw_mixed_split`. 포커스가 터미널인 줄과 Markdown인 줄 두 칸 |
| `gallery/layouts/index.jsx` "Focused · unfocused · agent" | `surface` | `components/surface_highlights.rs` |
| `gallery/layouts/index.jsx` "Occupancy & completion borders" | `occupancy` | `components/occupancy_borders.rs` |
| `gallery/layouts/index.jsx` "What the 24px bar carries, and what goes first when it can't" | `statusbar` | [공용 그리기 함수](#공용-crate-view-specimen-복제-0--본체와-같은-함수-호출) status bar 행. 탈락 단계는 `components/status_bar.rs` 의 cluster 셋 |
| `gallery/loading/index.jsx` "Boot screen — full client area, centered stack" | `boot-loading-default` |  |
| `gallery/loading/index.jsx` "Same stack, no responsive shrink" | `boot-loading-min` |  |
| `gallery/loading/index.jsx` "Fixed-height phase slot — swap/omit never shifts the stack" | `boot-loading-phases` · `boot-loading-no-text` | 디자인 한 Spec ↔ 갤러리 둘 |
| `gallery/loading/index.jsx` "Spinner.jsx reused — size 16 → 32 for boot hero" | `boot-loading-spinner` |  |
| `gallery/overlays-banners/index.jsx` "Per-program capture opt-out — list editor" | `banner-blacklist` |  |
| `gallery/overlays-dialogs/index.jsx` "Every overlay sits on the same scrim" | `scrim` |  |
| `gallery/overlays-dialogs/index.jsx` "Scope — a surface-bound popup dims its surface, not the window" | `scrim-scope` | 디자인의 rejected 비교 두 장(window scrim·scrim 없음)은 갤러리에 없고 "window scope" 장을 둔다 |
| `gallery/overlays-dialogs/index.jsx` "Both split directions, a clamped popup, and a child picker" | `scrim-scope`(top/bottom split · narrow clamp) · `scrim-scope-child` | 디자인 두 Spec 과 갤러리 두 예제의 경계가 다르다(N:M) |
| `gallery/overlays-dialogs/index.jsx` "Approval popup — the mauve gate" | `approval` |  |
| `gallery/overlays-dialogs/index.jsx` "Danger choice — tinted fill with readable ink (2026-10-10 b12)" | `approval` | 같은 예제 페이지 아래 단. 선택지는 본체와 같은 `approval_choice` 가 그린다 |
| `gallery/overlays-dialogs/index.jsx` "New / rename category — the Rename dialog, reused" | `workspace-categories` | [workspace-category](#workspace-category-layouts--overlays) `CategoryEditFrame` 행 |
| `gallery/overlays-popups/index.jsx` "Tab switch overlay — modifier held" | `switch-tab` | [switch_overlay](#switch_overlay-overlays) |
| `gallery/overlays-popups/index.jsx` "Workspace switch overlay — modifier held" | `switch-ws` | [switch_overlay](#switch_overlay-overlays) |
| `gallery/overlays-popups/index.jsx` "Category switch overlay — Ctrl+Shift held" | `switch-cat` | [switch_overlay](#switch_overlay-overlays) |
| `gallery/overlays-popups/opaque-over-page.jsx` "Contract — popup over a page in opaque mode" | `webview-opaque` (`components/webview_opaque.rs`) | 여섯 결정·토큰·입력 경로 Note. 상자 가장자리 판정은 본체와 같은 `tasty_egui_theme::overlay_edge::BoxEdge` 다 |
| `gallery/overlays-popups/opaque-over-page.jsx` "A — convert_surface over a full-pane markdown page" | `webview-opaque-convert` | 셸은 `convert` 예제와 같은 그리기 함수(`convert::card`)에 가장자리만 바꿔 그린다. Mocha·Latte 줄은 툴바 테마와 관계없이 고정이다 |
| `gallery/overlays-popups/opaque-over-page.jsx` "B — command palette across markdown page \| terminal" | `webview-opaque-palette` | 네 번째 열이 기각안(페이지 밖에만 scrim)이다 |
| `gallery/overlays-popups/opaque-over-page.jsx` "C — Tools menu straddling sidebar \| page" | `webview-opaque-tools` | 메뉴는 `tools-menu` 예제와 같은 `tools_card` 다 |
| `gallery/overlays-popups/opaque-over-page.jsx` "D — file picker with the path-bar dropdown hanging over the page" | `webview-opaque-file-picker` | 카드와 `…` 메뉴는 `filepicker` 예제와 같은 함수다. 메뉴 항목은 갤러리 경로 데이터가 숨긴 조상(3개)이라 시안(6개)과 달리 카드 아래로 늘어지지 않는다. 두 상자의 점선은 따로 그린다 |
| `gallery/overlays-tutorial/index.jsx` "Step 1 / 4 — 워크스페이스" | `tutorial-composite` | `widgets/tutorial.rs` |
| `gallery/overlays-windows/index.jsx` "Two-pane remote-workspace picker — 4 states" | `remote-workspace-attach` · `remote-workspace-attach-states` | 아래 remote_attach 절(디자인 한 Spec ↔ 갤러리 둘) |
| `gallery/overlays-windows/index.jsx` "“+ New workspace” row — the escape from a dead-end remote" | `remote-workspace-attach-new-row` | 아래 remote_attach 절 `RaNewWsRow` 행 |
| `gallery/overlays-windows/index.jsx` "Gestures & folder targets — the six open branches" | `filepicker-gesture-table` | [파일 피커](#파일-피커-overlays) |
| `gallery/overlays-windows/index.jsx` "Settings · General › Remote transfer — 5th L2 subtab" | `settings-remote-transfer` | [Remote transfer](#settings--general--remote-transfer) |
| `gallery/overlays-windows/index.jsx` "General › General — row grid, row captions, webhook row" | `settings-general-row-grid` | [행 격자와 웹훅 외부 수신 행](#settings--general--general--행-격자와-웹훅-외부-수신-행) |
| `gallery/overlays-windows/index.jsx` "FileHandler › File Extension Mapping — order + Add" | `settings-file-extension-mapping` | [Handler 하위 탭](#settings--handler-하위-탭) |
| `gallery/overlays-windows/index.jsx` "Status table, one action per row, one request button" | `settings-macos-permissions` | [Permissions (macOS)](#settings--general--permissions-macos) |
| `gallery/overlays-windows/index.jsx` "Copy fingerprint · Add plugin that can't be added" | `plugins-window` · `plugin-add-hint-slot` | [plugins window](#overlays--plugins-window) |
| `gallery/overlays-windows/index.jsx` "Diff variant — file row → unified diff" | `git-viewer`(diff cluster) · `git-diff-toolbar` | [git-viewer](#git-viewer-plugins) |
| `gallery/plugins/index.jsx` "Sidebar Favorites — populated vs. empty state" | `explorer-sidebar` |  |
| `gallery/plugins/index.jsx` "Sidebar layout — Favorites PINNED to the bottom (2-region split)" | `explorer-sidebar` | 디자인 두 Spec ↔ 갤러리 하나 |
| `gallery/plugins/index.jsx` "Short cell — Favorites drops below 240, the cell stops at 180" | `explorer-sidebar-short-cell` | 하한 180(`explorer-min-height`)·표본 body 84·compact 무대 "content body 84 (cell at 180)" 까지 반영. 라벨·Meta·Note 의 하한 숫자는 Theme 토큰에서 읽는다. Meta 의 split·refusal·scope 줄과 Note 는 시안 문구를 따르고 형제 칸 최소 56 은 `split-sibling-min-height` 토큰에서 읽는다. 본체 판정은 [분할 비율과 탐색기 칸 하한](../../features/work-area/index.md#분할-비율과-탐색기-칸-하한)에 있다 |
| `gallery/plugins/index.jsx` "Right-click context menu — target resolves to 4 shapes" | `explorer-context` (Overlays) |  |
| `gallery/explorer-ops/create-drag-search.jsx` "Toolbar — create group and view group (wide · narrow · remote · read-only)" | `explorer-commands` | 명령 묶음은 공용 `explorer_commands`. 시안의 Preview panel 토글과 More 메뉴의 체크 표시는 그리지 않는다(Spec Note) |
| `gallery/explorer-ops/address-and-toolbar.jsx` "Undo / Redo rows — copy · empty history · Windows · shortcuts" | `explorer-context` (Overlays) | 설정의 첫 바인딩과 빈 이력의 고정된 두 행을 공용 `menu_item`으로 전시. 줄바꿈 무대의 available·empty 열 옆에 Linux·macOS 툴팁과 Windows 이유 라벨의 한 행짜리 stale 메뉴를 세 번째 열로 묶는다. 각 메뉴 폭은 240 |
| `gallery/explorer-ops/create-drag-search.jsx` "Context menu — create rows (empty area · folder) and Properties" | `explorer-context` (Overlays) | 빈 영역 첫 묶음과 폴더 파일 조작 묶음의 New folder · New file |
| `gallery/explorer-ops/create-drag-search.jsx` "Name input — inline, at the top of the list (Detail · List · Grid)" | `explorer-name-input` | 편집 줄은 공용 `explorer_name_row`. 상세 열은 공용 `explorer_detail_columns` |
| `gallery/explorer-ops/create-drag-search.jsx` "Name errors — empty · invalid character · already exists" | `explorer-name-errors` | 오류 상자는 공용 `explorer_name_error` |
| `gallery/explorer-ops/remaining-states.jsx` "Hidden files · show in enclosing folder · undo / redo" 의 숨김 파일 부분 | `explorer-hidden-files` | 이름·글리프 색은 본체와 같은 `explorer_hidden_fg`. 시안 More 행의 단축키 칸은 본체 네이티브 메뉴에 없어 그리지 않는다. 들어 있는 폴더에서 보기는 `explorer-context` 의 search result 메뉴(Open · 들어 있는 폴더에서 보기 · 구분선 · Copy path, `folderOpen` 글리프)가, Redo 는 같은 예제의 실행 취소 행이 맡는다. 본체 네이티브 메뉴도 열기 · 들어 있는 폴더에서 보기 · 구분선 순서지만 글리프가 없고 구분선 뒤에 일반 파일 메뉴 행이 이어진다([디자인과 구현의 차이](design-parity-notes.md#explorer--검색-결과-메뉴)) |
| `gallery/explorer-ops/remaining-states.jsx` "Create inside a folder — input under the target row" | `explorer-create-in-folder` | 편집 줄은 공용 `explorer_name_row_in`(들여쓰기·caption), 대상 표시는 `paint_drop_tint`·`paint_drop_ring` |
| `gallery/explorer-ops/create-drag-search.jsx` "Find bar — filter the current folder" | `explorer-find-bar` | Find 바는 공용 `explorer_find_bar`, 이름 강조는 `explorer_match_job` |
| `gallery/explorer-ops/create-drag-search.jsx` "Subfolders — recursive search · searching · no results · errors · stopped" | `explorer-subfolder-search` | Folder 열은 공용 `explorer_detail_columns` 의 검색 구성. Size·Date 열 폭은 상세 보기와 같다 |
| `gallery/explorer-ops/properties-and-preview.jsx` "Properties — own path · folder permissions · links · partial size" | `explorer-properties` | 단일 항목 Path·폴더 권한, 대상 메타데이터, 끊긴 링크와 Size 아래 부분 집계 경고 |
| `gallery/explorer-ops/properties-and-preview.jsx` "Preview panel in a low cell — header only" | `explorer-preview` | 본문 높이 임계값 바로 아래·동일·위 예제. 120 미만이면 머리만 표시하고 아래 구분선을 숨긴다 |
| `gallery/explorer-ops/properties-and-preview.jsx` "Progress — counting stage · zero-byte work" | `explorer-ops-progress` | 복사·이동 집계는 막대 없이, 0바이트 파일 전송은 파일 수 기반 막대를 공용 `op_status_line`으로 전시 |
| `gallery/explorer-ops/remote-writes.jsx` "D1 · A remote that can't write — rows shown disabled, reason in the tooltip and toast" | `explorer-remote-write-refused` (Plugins) | 쓸 수 있는 원격 메뉴와 꺼진 쓰기 행 메뉴, 툴팁 표본, 단축키 info 카드. 이유 문구는 옛 Tasty 의 `explorer.remote.write_needs_update` 하나다. 화면 없는 server 도 원격 쓰기를 실행하므로 시안의 `write_headless` 문구가 가리키는 상태가 없어 옮기지 않았다([디자인 정합 지침](design-parity-notes.md#explorer--원격-쓰기와-응답-대기)). 툴바의 꺼진 create 묶음은 `explorer-commands` 의 원격 행이 맡는다 |
| `gallery/explorer-ops/remote-writes.jsx` "D2 · Waiting for the remote — the input row stays, read-only, with a spinner" | `explorer-remote-waiting` (Plugins) | 입력 줄은 공용 `explorer_name_row_in` 의 `pending`(읽기 전용 Input, 칸 뒤 icon-size-sm Spinner 와 툴팁). 이름 변경 팝업은 본체와 예제가 같은 공용 view(`tasty_ui_widgets::rename_popup`)를 부른다: 기다리는 동안 공용 Input readOnly(input-readonly-bg·border·fg)와 Spinner 대기 줄, 평소 input-bg·input-border(포커스 input-border-focus)다. 팝업 바탕은 dialog-bg 다 |
| `gallery/explorer-ops/remote-writes.jsx` "Rename popup — the read-only field against the popup ground" | `explorer-rename-popup-ground` (Plugins) | 평소·대기 두 팝업을 본체와 같은 공용 rename view 로 dialog-bg 위에 그린다. 시안의 평소 팝업에 있는 Kbd 안내 줄은 본체 팝업에 없어 그리지 않는다(Spec Note). 같은 Spec 의 surface-raised 위 Input 비교(기본 바탕 vs raised)는 공용 Input 예제의 raised 행이 맡는다 |
| `gallery/explorer-ops/remote-writes.jsx` "D3 · Connection lost — result unknown" | `explorer-remote-connection-lost` (Plugins) | 경고 카드의 설명 문단은 공용 `result_card` 의 `body`. 본체는 같은 props 로 칸의 알림 카드(`view/ops_notice.rs`, `NoticeKind::Lost`)를 그린다. 되돌리기·남은 원본 지우기는 시안 동사 목록에 없어 오류 토스트로 남는다 |
| `gallery/explorer-ops/remote-writes.jsx` "D4 · Remote progress line and queue — host shown" | `explorer-remote-progress` (Plugins) | 공용 `op_status_line` 의 `host`(`HostMark`)와 `op_queue_popover` 의 `QueueHeader`. 머리 줄 세 조각(On · 호스트 · 나머지 문구) 사이는 `explorer-remote-queue-head-gap` 이다. 본체와 같은 위젯이다 |
| `gallery/explorer-ops/remote-writes.jsx` "D5 · Drag chip — new refusal reasons · conflict popup unchanged" | `explorer-remote-drag` (Plugins) | 거절 이유는 `explorer.drag.other_computer`·`explorer.drag.remote` 의 새 문구 |
| `gallery/explorer-ops/index.jsx` "Running — on the explorer's own status line · queue" | `explorer-ops-progress` | 상태줄은 공용 `op_status_line`, 대기열은 `op_queue_popover` |
| `gallery/explorer-ops/index.jsx` "Name conflict — surface-scoped prompt · apply to all · waiting" | `explorer-ops-conflict` | 카드는 공용 `conflict_card`(바깥 틀 없음). 본체는 칸 범위 popup 이 틀과 scrim 을 그린다 |
| `gallery/explorer-ops/index.jsx` "Results — done · cancelled · partial + retry · failed · trash unavailable · undo" | `explorer-ops-results` | 카드는 공용 `result_card` |
| `gallery/explorer-ops/staging-left.jsx` "Result card — one extra line, kind raised to at least Warning" | `explorer-ops-results-staging-left` | 네 카드(모두 끝난 복사 · 취소 · 부분 실패 이동 · 복사 되돌리기)를 내장 언어마다 한 묶음씩(en·ko·ja·zh-Hans) 그린다. 카드는 공용 `result_card`, 줄 문구는 `explorer.result.staging_left` 이고 문구는 본체 번역표를 읽는다. 동작 버튼은 본체 `card_actions` 를 따른다: 순서는 Retry · Copy paths · Undo 이고, 취소 카드는 하지 않은 28개를 세는 Retry 28 을 둔다. 부분 실패 카드는 실패 줄 셋이 줄 상한(3)을 채워 폴더 줄이 "and 1 more" 에 든다(시안 Meta 의 "after item failure lines · counts toward and N more" 규칙). 시안 표본은 이 카드에서 폴더 줄을 보이고 Undo 를 Copy paths 앞에 두며 취소 카드에 Retry 가 없다([정합 지침](design-parity-notes.md#explorer--남은-임시-폴더-카드)) |
| `gallery/explorer-ops/create-drag-search.jsx` "Drag chip — one item · many items · move / copy / refused" · "Drop targets — folder row · grid cell · tree node · favorite · current folder" · "Files from the OS — copy into the explorer, not open" | `explorer-ops-drag` | 디자인 세 Spec ↔ 갤러리 하나. 칩은 공용 `drag_chip`, 대상 표시는 `paint_drop_tint`·`paint_drop_ring` |
| `gallery/explorer-ops/create-drag-search.jsx` "Drop target on the bed" | `explorer-ops-drop-target-bed` | 칠 순서는 바탕, 틴트, 행 fill, 잉크, 링이다. 틴트는 공용 `paint_drop_tint`·`DropTintSlot`, 링은 `paint_drop_ring`이다. 행 글자는 `RowState` 칸을 읽는다 |
| `gallery/explorer-ops/results-and-refusals.jsx` "Requests refused — queue full · request too large" | `explorer-ops-refused` | 카드는 공용 `result_card`. 본체는 칸 안 상태줄 위에 경고 카드로 띄우고, Show queue 는 그 칸의 작업이 돌 때만 붙는다 |
| `gallery/explorer-ops/results-and-refusals.jsx` "Retry of originals — its own progress words · one result card · leftover reasons" | `explorer-ops-remove-originals` | 상태줄은 공용 `op_status_line`, 카드는 `result_card` |
| `gallery/explorer-ops/remaining-states.jsx` "Keyboard current item" · "Drag-select rectangle" | `explorer-keyboard-select` | 디자인 두 Spec ↔ 갤러리 하나. 테두리는 공용 `paint_cursor_ring`, 사각형은 `paint_marquee`(본체 `view/cursor.rs`·`view/marquee.rs` 와 같은 함수) |
| `gallery/plugins/index.jsx` "HTML — address bar band" · `gallery/plugins/html-address-ink.jsx` (idle·read-only ink) | `html-address-states` · `html-address-placement` · `html-address-live` | shared `html_address_bar`; host `src/adapters/ui/surface/html_address.rs`. `html-address-states`의 empty·loading 페이지 영역은 본체와 같은 `tasty_ui_widgets::webview_page_tile`이고, page·failed 영역은 갤러리 표본이다. empty·loading은 본체 타일과 같은 배치라 정본 `HtmlPageState`와 세로 배치·페이지 영역 테두리가 다르다(디자인 요청 중) |
| `gallery/plugins/index.jsx` "HTML — webview chrome (4 states)" | `html-chrome` | [surface viewers](#surface-viewers-plugins) |
| `gallery/plugins/index.jsx` "HTML — focus edge above the band" | `html-focus-edge` | `components/html_chrome.rs::draw_focus_edge` |
| `gallery/plugins/index.jsx` "Markdown — document background follows focus" | `markdown-doc-bg` | `plugins_settled.rs::draw_doc_background`. 표·코드와 tile 4종을 두 상태로 그린다 |
| `gallery/plugins/index.jsx` "Markdown — quote and find ink on both beds" | `markdown-quote-and-find-ink` | `markdown_viewer/quote_find.rs`. 인용은 `md-quote-fg`, 적중은 `md-find-hit-*`, 현재 적중은 `md-find-current-*`이고 두 상태로 그린다 |
| `gallery/plugins/index.jsx` "HTML — settings (Appearance › HTML viewer)" | `plugin-settings` (Components) | [플러그인 설정 페이지](#플러그인-설정-페이지) |
| `gallery/plugins/index.jsx` "Image — viewer & edit (paint) modes" | `image-viewer` · `image-paint` | [surface viewers](#surface-viewers-plugins)(디자인 한 Spec ↔ 갤러리 둘) |
| `gallery/overlays-windows/preset-editor.jsx` "WYSIWYG edit mode — direct manipulation of STRUCTURE" | `preseteditor` | [preset demo-layout](#preset-demo-layout-overlays) |

<a id="공용-crate-view-specimen-복제-0--본체와-같은-함수-호출"></a>

## 공용 그리기 함수를 사용하는 예제

본체 view 를 `crates/tasty-ui-widgets` 로 올려 **본체 wrapper 와 갤러리 specimen 이 같은
함수를 호출**하는 항목. 아래 "시각 복제 specimen" 과 달리 레이아웃·색·치수를 갤러리가
재선언하지 않으므로 그리기 코드의 차이는 줄어든다. 입력 rect·테마·배율이 다르면 화면도 달라지므로 같은 조건의 캡처 비교는 필요하다. 새 bar/패널은 복제보다
이 경로를 우선한다([gallery-first](../../dev-guide/gallery-first.md)).

| 디자인 원본 | 공용 crate view | 본체 wrapper | 갤러리 specimen |
|---|---|---|---|
| `gallery/layouts/index.jsx` **Workspace status bar** (하단 24px 바, 좌 요약 / 우 리마인더) | `tasty_ui_widgets::draw_status_bar_view` (`crates/tasty-ui-widgets/src/status_bar.rs`, `StatusBarData`→`StatusBarDrawResult`) | `src/adapters/ui/status_bar.rs::draw_status_bar` (Area·z-order·i18n 라벨 주입·action 적용) | `statusbar` (Layouts › Status bar, `components/status_bar.rs::draw`) · `statusbar-theme-cell` (`components/status_bar_theme_cell.rs::draw`) |
| `gallery/layouts/remote-location.jsx` **Remote item** (`statusbar-remote-item`) | 같은 `draw_status_bar_view` 의 `StatusBarData::remote`(`StatusBarRemote`) | `src/adapters/ui/status_bar.rs::draw_status_bar`(`remote_of` 가 `EngineRead::mirror_location_of` 를 읽는다) | `statusbar-remote-item` (Layouts › Status bar, `components/status_bar_remote.rs::draw` — Mocha·Latte 나란히) |
| `gallery/layouts/remote-location.jsx` **Path link on a mirror** (`terminal-link-remote`) | `tasty_ui_widgets::LinkRemoteTip`(`crates/tasty-ui-widgets/src/remote_mark.rs`, 자리 `link_tip_rect`) | `src/adapters/ui/link_remote_tip.rs::draw`(Area `Order::Tooltip`) | `terminal-link-remote` (Layouts › Status bar, `components/terminal_link_remote.rs::draw` — 링크 셀은 mock, 툴팁은 같은 `LinkRemoteTip::paint`) |
| `gallery/parts/overlay-frames.jsx` `NumCap` (16px 숫자 키캡) | `tasty_ui_widgets::paint_num_keycap` (`crates/tasty-ui-widgets/src/chip.rs`; 레이아웃 갈래는 같은 파일의 `num_keycap`) | `src/adapters/ui/switch_overlay.rs::paint_keycap` (slot 좌표·등장 페이드 alpha) | `switch` (Overlays, `components/switch_overlay.rs::keycap_at`) |
| `gallery/overlays-banners/index.jsx` `BannerShellG` (배너 셸) | `tasty_ui_widgets::banner_shell` (`crates/tasty-ui-widgets/src/banner.rs`; inset 기하 `inset_banner_zone`·`inset_content_rect`) | `src/adapters/ui/banner.rs::BannerManager::draw` (배치·z-order·디밍·hover) | `banner-*` (Overlays, `widgets/banner.rs` · `widgets/banner_mouse_capture.rs`) · `html-script-*` (`widgets/html_script_banner.rs`) |
| `ui_kits/terminal/overlays/html_remote_banner.jsx` `HtmlRemoteBanner`·`HtmlRemoteMarker` | `tasty_ui_widgets::html_remote_banner`·`html_remote_marker` (`crates/tasty-ui-widgets/src/html_remote_banner.rs`) | `src/adapters/ui/surface/html_script_banner.rs::draw_stack`(크기 → 원격 → 스크립트 inset 쌓기) · `src/adapters/ui/surface/html_address.rs`(주소 입력란 끝 표지) | `html-remote-*`·`http-s-page-*` (Overlays, `widgets/html_remote_banner.rs`·`widgets/html_web_page.rs`) |
| `ui_kits/terminal/overlays/html_script_banner.jsx` `HtmlScriptBanner`·`HtmlScriptMarker` | `tasty_ui_widgets::html_script_banner`·`html_script_marker` (`crates/tasty-ui-widgets/src/html_script_banner.rs`) | `src/adapters/ui/surface/html_script_banner.rs::draw`(webview chrome 위 inset, WebView 영역 축소) · `src/adapters/ui/tab_bar/tab.rs`(탭 오른쪽 묶음의 표지 칸·lock 클릭) | `html-script-*` (Overlays, `widgets/html_script_banner.rs`) |
| `gallery/overlays-banners/index.jsx` `RefusalRowG`·`RefusalRailG` (저장 행 거절 표지) | `tasty_ui_widgets::attach_refusal_mark`·`paint_attach_refusal_chip`·`attach_refusal_avatar_tooltip` (`crates/tasty-ui-widgets/src/attach_refusal.rs`) | `src/adapters/ui/sidebar/view.rs::draw_workspace_card`(행 표지, 배지 묶음 왼쪽) · `draw_collapsed_avatar`(레일 왼쪽 위 칩). 툴팁 문장은 `src/adapters/ui/sidebar/remote_row.rs`가 만든다 | `attachrefusal` (Overlays, `widgets/attach_refusal.rs`) |
| `gallery/overlays-banners/index.jsx` `AttachSizeSyncBannerG` (attach mirror 터미널 크기 동기화 실패) | `tasty_ui_widgets::attach_size_sync_banner`·`attach_size_sync_banner_content` (`crates/tasty-ui-widgets/src/attach_size_sync.rs`). 본문은 두 줄이다. 이름 줄은 한 줄로 두고 이름 칸만 `attach_sync_name_max_width`(160)에서 하한 `attach_sync_name_min_width`(40)까지 본래 폭 비율로 줄인 뒤 말줄임하며, 수·구분자·` ×k`·`+n` 은 줄지 않고 넘치는 끝은 잘라 그린다. 하한은 줄어들 때만 적용한다. 칸 = min(본래 폭, max(40, 줄인 폭))이라 40보다 짧은 이름은 제 폭을 지키고 뒤 조각이 바로 붙는다. 같은 이름은 한 항목으로 묶어 이름 뒤에 ` ×k`(`banner_body_fg`)를 붙인다. `N surfaces` 는 surface 수, `+n` 은 남은 항목 수다. 제목과 고정 문구는 줄 수를 제한하지 않는다. 고정 문구 줄(`remote.size_sync.hint`)은 감싸고 자르지 않는다. 다시 시도 중의 앞 Spinner 는 `Button::leading_icon_size` 로 14 칸에 `Spinner::paint_in` 을 그린다. 좁은 스코프 배치는 `banner_is_narrow` 판정을 view 의 `narrow` 로 받는다 | `src/adapters/ui/banner.rs`(`BannerContentSource::AttachSizeSync`, Workspace 범위) · `src/adapters/ui/attach_size_sync.rs`(창별 실패 상태·탭 이름) · `src/adapters/ui/overlay.rs`(버튼 → `Intent::AttachSizeSync`) | `attachsizesync` (Overlays, `widgets/attach_size_sync.rs`). 시안의 두 Spec 을 그대로 둔다: 기본·다시 시도 중·세 surface·같은 이름 두 번·같은 이름과 다른 이름 묶음, 그리고 Narrow Spec(360 스코프에 거절 · 한 surface · Retry all · 다시 시도 중을 Mocha·Latte 로). 시안 Meta "body lines"·"plugin body rows"·"plugin tooltip" 의 plugin 쪽은 Stage 가 없어 같은 카드 끝 Banner body lines 에 `plugin_banner_body` 360 표본 둘(세 줄 · 두 줄 높이로 고정한 콘텐츠)로 둔다. 표본은 본체 호스트와 같이 `plugin_banner_body_host_tooltip` 안에서 그리고 `show_plugin_banner_body_tooltip` 으로 카드 아래 툴팁을 그린다 |
| `gallery/overlays-banners/index.jsx` `BodyInkGrid` (`#bodyink`, 배너 본문 글자색 Mocha·Latte 대조) | 새 위젯 없음. 배너 본문은 `banner_body_fg()`, 크기 동기화 줄의 이름은 `text_secondary_raised()`를 읽는다 | 없음 | 별도 카드를 만들지 않는다. 같은 글자색은 각 배너 카드의 Mocha·Latte 표본과 `TASTY_GALLERY_THEME=latte` 캡처로 확인한다 |

crate 쪽 view 가 **소유하지 않는 것**(=본체 wrapper 잔류): `egui::Area` 와 `LayerId`
(부유 배치·z-order 는 본체 정책), i18n 라벨·tooltip 문자열(위젯 crate 는 `tasty-i18n`
비의존 — `multi_select` 와 동일 정책), 글로벌 `theme()` 를 읽는 `status_bar_bottom_inset`.

<a id="overlay-시각-복제-specimen-본체-의존-0"></a>

## 본체 화면을 복제한 오버레이 예제

본체 view 의 시각만 로컬 mock props 로 복제한 Overlays 항목. 본체 binary crate(`tasty`)에
의존 불가하므로 layout·색·폰트·간격·보더는 모두 Theme 토큰에서 가져오고 상태는 mock 으로
주입한다. 본체 view 변경 시 시각 동기화는 수동 검증.

| 디자인 원본 | 본체 view | 갤러리 specimen |
|---|---|---|
| `overlays/search_bar.jsx` (CSS 셸 360×38 · Input 높이 28, 배율 1) | `src/adapters/ui/search_bar.rs::draw_search_bar` | `search_bar` (Overlays) |
| `overlays/tools_menu.jsx` (내용 폭, `tools-menu-min-width` 160 ~ `tools-menu-max-width` 240) | `src/adapters/ui/tools_menu.rs::draw_tools_menu`(공용 `menu_item` 행) | `tools_menu` (Overlays) |
| `overlays/command_palette.jsx` (`palette-width` 540, × UI 배율) | `src/adapters/ui/popup/command_palette.rs::draw_command_palette_view` | `command_palette` (Overlays "Command palette") |
| `gallery/overlays-dialogs/index.jsx` §`convert` · kit `info_modal.jsx` `ConvertSurfacePopup` (`convert-popup-width` 240 × ui_scale) | `src/adapters/ui/popup/convert.rs::draw_convert_view`(공용 `menu_item_kbd` 행 — 끝에 단축키 키캡) | `convert` · `convert-shortcut` · `convert-narrow` (Overlays "Convert surface", `components/convert.rs::draw`·`draw_shortcut`·`draw_narrow`) |
| `gallery/overlays-dialogs/index.jsx` §`filehandler` (420px · 프레임은 `gallery/parts/overlay-frames.jsx` `FileHandlerFrame`) | `src/adapters/ui/popup/file_handler_picker.rs::draw_file_handler_picker_view` | `file_handler_picker` (Overlays "File handler picker", `components/file_handler_picker.rs::draw`) |
| `gallery/overlays-dialogs/index.jsx` `file-handler-remote-path` (`FhRemoteMark`) | `src/adapters/ui/popup/file_handler_picker.rs::header_band`(공용 `tasty_ui_widgets::paint_fh_remote_mark`, 경로 상자 `file_handler::target_budget_chars_beside`) | `file-handler-remote-path` (Overlays, `components/file_handler_picker.rs::draw_remote_path` — 프레임은 mock, 표지와 경로 생략은 본체와 같은 함수. 본문은 원격 핸들러 목록(빈 상태·전체 핸들러 그룹·1 회성 띠 없음), Tag 는 경로 링크 `format unknown`·이름으로 연 경로 accent 형식. 디자인의 재연결 중·끊김 표지 줄은 공용 위젯에 그 모양이 없어 그리지 않는다) |
| (시안 없음 — 확정 토큰 + `icons.json` `close`/`fit` 조합뿐이라 신규 시각 결정이 없었다, 근거 → [fullscreen-stage §디자인 소스](fullscreen-stage.md#디자인-소스--신규-시안-없이-만든-이유)) | `src/adapters/ui/fullscreen.rs::draw_fullscreen_stage`(셸: scrim+제목+종료 버튼) | `fullscreen-stage` (Overlays, `components/fullscreen_stage.rs::draw`) |
| `ui_kits/terminal/overlays/info_modal.jsx` `PopupTitleBar`(fullscreen) · `gallery/overlays-dialogs/index.jsx` "Popup title bar" Spec | `src/adapters/ui/popup/draw.rs`(타이틀바 전체화면 버튼 — `fit` 아이콘, 24 칸) | `fullscreen-stage-titlebar` (Overlays, `components/fullscreen_stage.rs::draw_titlebar`) |

팔레트와 검색 바의 입력 칸은 본체와 갤러리 모두 공용 `Input`의 `Ground::Raised`를 쓴다. 채움은 `input-raised-bg`, 휴지·호버 테두리는 `input-raised-border`, 포커스는 `border-focus` 테두리와 1px 링이다. 검색 바의 정규식 오류는 포커스 유무와 관계없이 `accent-danger` 테두리를 쓰며 포커스 시 같은 색 링을 더한다. `command_palette` 견본은 normal·focused, `search_bar` 견본은 normal·focused·invalid focused·invalid blurred를 제공하며 현재 갤러리 테마(Mocha·Latte)를 따른다. 본체 검색 바 sizer는 `input-height` 28 + 상하 `popup-content-margin` 4씩으로 할당 높이 36을 만들고, 매니저가 그 바깥에 1px 테두리를 그려 외곽 높이는 38이다(배율 1). 본문 높이는 28이며 링은 패딩 영역까지 그릴 수 있다. 기존 할당 폭 360은 유지하므로 본체의 테두리 포함 폭은 362이고 CSS 셸의 border-box 폭은 360이다. 갤러리도 Input 28과 패딩·프레임 테두리를 사용하고, 오류 견본의 `.*`는 켜짐·`ab`는 꺼짐이다. 견본 포커스 표시는 `Input::focus_look`이고 실제 입력의 포커스는 TextEdit가 관리한다.

**`command_palette` 의 단축키만은 복제가 아니다.** 프레임·행·footer 는 위 표대로 mock 복제지만,
행 우측의 키캡은 본체와 갤러리가 **같은 함수**(`tasty_ui_widgets::kbd_parts_at`)를 부른다 —
본체는 `draw_keycaps` 를 거쳐, 갤러리는 `menu_item_kbd` 를 거쳐 닿는다. 그래서 키캡 치수·색은
수동 동기화 대상이 아니다. 단축키를 **나뉜 토큰**(`["Ctrl", "T"]`)으로 넘기는 것도 양쪽이 같다 —
`ctrl++` 같은 조합에서 `+` 가 구분자인지 키인지 갈리지 않게 문자열을 쪼개지 않는다.

**`file_handler_picker` 의 세 좌표는 한 형상이다.** 폭 420px · headless 헤더(경로 한 번) · 제목 줄 형식 Tag ·
후보/Recent 단일 목록 · 2px accent 좌측 바 · `icon · name · origin` 행 · plugin mauve · fallback 안내 띠 ·
빈 상태 블록 · 264px 목록 상한 + 하단 페이드 · [취소]/[열기] footer. 갤러리는 디자인 원본 의 10 Spec 을 그대로
미러한다(`filehandler` · `-format` · `-recent` · `-fallback` · `-empty` · `-long` · `-headless` · `-footer` ·
`-rows` · `-default`). 거기에 2026-09-20 결정이 둘을 더한다 — `-when`(상대시간 어휘 6 단계와 예약된 열)과
`-path-cut`(헤더 경로 앞자름 세 표본). 뒤쪽은 표본을 **본체와 같은 함수**에 넣어 그 자리에서 자른다.

디자인·본체·갤러리는 모양을 맞추지만 그리기 코드 전체를 공유하지는 않는다. 갤러리는 정적 예제이고 본체는 상호작용하는 화면이다. 공용 부분은 다음과 같다.

- `crates/tasty-ui-widgets/src/tokens.rs`의 `FH_*` 치수: 420·14·10·6·5·264·20·32·34·0.8. 대응 Theme 토큰이 없어 용도를 명시한 상수로 둔다.
- `crates/tasty-ui-widgets/src/file_handler.rs`의 ID·헤더 경로 앞자름과 문자 예산 계산.
- 예약된 “언제” 열의 `component.fh-when-width`(56px)는 Theme에서 읽는다.

420·264·20·32는 4px 배수지만 해당 역할의 토큰은 없다. 숫자가 같아도 프레임 폭·목록 상한·페이드 높이·블록 여백을 다른 역할의 토큰으로 대체하지 않는다. [토큰 매핑](design-token-mapping.md)의 File handler picker 절과 `src/source_guards/on_scale_length_literal.rs`의 `AREAS`가 같은 구분을 사용한다.

footer에는 “Always open …” 체크박스를 두지 않는다. 파일 피커는 이번 열기만 처리하며 저장되는 연결 설정은 설정 › 핸들러에서 관리한다. 행 아이콘은 action이 여는 surface kind에서 고르고, 알 수 없으면 `file`을 쓴다. 이름은 선언된 이름을 우선하며 없으면 ID의 마지막 `/` 뒤 부분을 고정폭 글꼴로 표시한다.

디자인의 “Footer — settled” 예제에는 기각한 체크박스 두 형태도 있지만 갤러리에는 확정한 footer만 표시한다. 그룹 라벨은 대문자·11px·색을 적용한다. 자간은 egui에 `RichText::extra_letter_spacing`이 없어 생략한 것이 아니라 디자인 `FileHandlerFrame`에 값이 없어서 적용하지 않는다.

## Overlays — plugins window

디자인 `ui_kits/terminal/overlays/plugins_window.jsx` (820×540 모달) ↔ 본체 `src/view/plugins/`
↔ 갤러리 `Plugins manager window` (Overlays). 본체 binary 의존 0 — 로컬 mock 데이터로 시각 복제.

본체는 `TopBottomPanel`/`SidePanel` 을 `Context` 에 직접 붙여 창 전체를 채우므로 갤러리가 그
함수를 호출할 수 없다 — 같은 구조를 rect 기준으로 전사한다. 전사할 고정 창 크기가 본체에
없어서 무대 크기는 토큰으로 조립한다(`list_w() + measure_md` × `measure_sm`). 디자인의 820×540
은 여기 들어오지 않는다. (Layouts 의 `1-depth (general shell)` specimen
`crates/tasty-gallery/src/catalog/widgets/layout_1depth.rs` 은 이 창이 아니라 **리스트→상세
배치 관용구 자체**를 보이는 별개 specimen 이다.)

| 디자인 jsx 컴포넌트 | 본체 | 갤러리 함수 (`crates/tasty-gallery/src/catalog/components/plugins_window.rs`) |
|---|---|---|
| `PluginsWindow`(container) | `src/view/plugins/ui.rs` `draw_plugins_panel` | `window` + `stage_size` — 탭 상태 `Tab`(Installed / Attention / Add{preview}) 로 본문이 갈린다 |
| header + `Seg` 세그먼트 | 같음(헤더 밴드) | `header` / `segment_tab` — Installed \| Attention(danger 배지) \| Add plugin. 필터 입력은 Installed 탭에서만 |
| installed list+detail | `src/view/plugins/ui/list.rs` `draw_list_tab` | `plugins_window/installed.rs`: `list_pane` / `detail_pane` — 상세 블록 전량(빈 상태 · health error 박스 · Homepage · Surface kinds · Permissions · Commands · Install path/Log · 제거 확인 2 분기 · 하단 액션 바). 이름 줄은 공용 `plugin_detail_name_row`(이름 `font_size_max` · text-primary — 시안 semibold 는 굵은 UI 글꼴이 없어 크기·색으로 근사, 버전 `tag` Default, 기본 제공이면 배지 caption · accent-agent, 사이 `spacing_sm`), 설명은 공용 `plugin_detail_description`(body · text-secondary, 폭 상한 `measure_lg`)이다. 시안의 설명 줄 높이 1.6 은 토큰이 아니라 egui 기본 줄 높이로 둔다. 이름 줄 아래 메타 줄은 공용 `plugin_detail_meta`(mono caption · text-muted, 항목 사이 ` · `, 간격 `spacing_sm`, 이름 줄과 `spacing_xs`)로 작성자와 id 를 잇는다(시안 `author · cat` — 매니페스트에 분류가 없어 두 번째 자리에 id). 시안처럼 절 사이에 구분선을 긋지 않고, Homepage 뒤와 Permissions·Commands·Install path 앞을 공용 `plugin_detail_section_gap`(`spacing_lg`, 호출 Ui 의 세로 item_spacing 을 뺀 나머지만 더함)으로 띄운다. Permissions 값은 본체·갤러리 모두 공용 `tag` Default 다. Surface kinds·Permissions·Commands 는 공용 `plugin_detail_section`(시안 `Mono` 머리글 — 대문자 `font-label` micro · text-muted · `letter-spacing-caps`, 본문과 `spacing_sm`)으로 그린다. Commands 행은 공용 `plugin_command_row`(제목 mono term-sm · text-secondary 말줄임, 오른쪽 `Kbd`, 사이 `spacing_lg`, 아래 1px `separator` 를 포함한 높이 `settings_row_min_height`)다. 상세 아래 액션 바는 공용 `plugin_detail_bar`(위 1px `separator`, 안쪽 여백 세로 `spacing_md`·가로 `PLUGIN_ADD_INSET`, 왼쪽 `switch_with_label_color` 한 칸 — Switch + 내장 라벨 Enabled/Disabled body · text-secondary, 오른쪽 Configure Button ghost + `settings` 아이콘 · Uninstall Button secondary + `danger_ink`)이며 본문 스크롤 밖에 남는다. 바는 상세 열의 여백 밖에 열 폭 전체로 붙어 위 구분선이 열 양끝에 닿고 아래 끝이 열 아래 끝과 같다(갤러리는 `detail_pane` 의 `rect` 바닥, 본체는 여백 0 인 CentralPanel 안에서 본문만 패널 기본 여백 안에 둔다). 오른쪽 묶음 폭을 먼저 재고 왼쪽에서 오른쪽으로 만들어 키보드 초점 순서가 화면 순서(스위치 → Configure → Uninstall)와 같다. 제거 확인(경고 문구 accent-attention + Confirm uninstall · Cancel)은 시안에 없는 단계라 본문 끝에 둔다. 설치 경로 절은 본체와 갤러리 모두 공용 `tasty-ui-widgets` `plugin_install_paths`(`PluginInstallPathsView`)를 부른다 — 머리글 줄 `font-label` caps `INSTALL PATH` · 오른쪽 `Open folder`(Button secondary sm + `folder` 아이콘), 아래 설치 경로·`Log:` 줄 mono caption text-muted `break_anywhere` 줄바꿈·선택 가능, 줄 간격 `spacing_sm`. 별도 Spec `plugins-install-path`(`plugins_window::draw_install_paths`)가 시안 Spec 의 두 열 폭(380 ≈ 720 창 · 540 ≈ 880 창)에서 같은 위젯을 그린다. Commands 절 키캡은 공용 `plugin_command_row` + `plugin_keycap_parts`(`PluginKeycapStyle` — 플랫폼·수식키 표시 스타일)이고, 시안 Spec "Installed detail — command keycaps per platform" 은 `installed::keycap_platforms` 가 Windows/Linux · macOS 낱말 · macOS 기호 세 줄로 그린다 |
| `AttentionPanel` (5케이스) | `src/view/plugins/ui/attention.rs` `draw_attention_tab` — 목록 행은 공용 `paint_plugin_attention_row`(아바타 · 이름 text-secondary · `font-label` micro 사유 라벨 · severity 점, 이름·라벨 말줄임, 선택 행은 surface-active + 왼쪽 `selection-edge-width` severity 띠 + text-on-active). 사유별 절 머리글(Permission changes · Signature · Log · Folder)은 공용 `label_header`(시안 `Mono`: 대문자 `font-label` micro · text-muted · `letter-spacing-caps`) | `plugins_window/attention.rs`: `list_pane` / `detail_pane` / `banner` / `reason_detail` / `action_bar` / `reason_cards`. 상세 상태는 기본(Permissions changed, 메타 줄 homepage 링크), `Attention — signature invalid`(설명·homepage 대신 공용 `plugin_detail_desc_hidden` 안내), `Attention — reserved id` 셋이다 |
| Spec `plugin-reserved-id` (`ReservedRow` · `ReservedDetail` · `ReservedNotice`) | `src/view/plugins/ui/attention.rs` — 정체 블록(폴더 이름 · `wrap_long` id, 버전 태그 없음), 공용 `plugin_reserved_id_banner`, `plugin_folder_path`, `plugin_attention_bar`의 `Remove`(secondary + danger ink), 공용 `plugin_uninstall_confirm_bar`(`wrap: true`, `busy`) | `plugins_window/reserved.rs` `draw`가 `plugins-window` Spec 안에 행 · 상세 세 상태(평소 · 확인 · 제거 중) · 알림(공용 `notification_item_expanded`)을 Mocha·Latte로, 좁은 폭 320의 긴 폴더·id와 영어 외 내장 언어(ko·ja·zh-Hans) 길이(`t_lang`)를 Mocha로 그린다. 확인 바의 줄바꿈은 디자인 `flexWrap` 세 경우(한 줄 · 버튼 묶음 아래 · 글리프도 위)와 같다. Installed 확인은 `wrap: false`로 글 열이 줄바꿈한다 |
| `FingerprintLine` · `shortFingerprint` | `src/view/plugins/ui/attention.rs` `fingerprint_line` → 공용 `tasty-ui-widgets` `plugin_fingerprint_line` — Attention 서명 절과 Add 신뢰 상자가 함께 쓴다. `font-label` caption 라벨(text-secondary)·mono 값(text-muted) 뒤에 IconButton sm `copy`(툴팁 Copy fingerprint), 간격 `spacing_sm`. 값은 `short_fingerprint`가 colon-hex 16바이트 초과를 앞 8 + ` … ` + 뒤 8바이트로 줄이고, 줄였을 때만 값 툴팁이 전체 값을 보인다. 복사는 전체 값이다. 값이 없으면 줄을 그리지 않는다. Attention 액션 바에는 복사 버튼이 없다 | `plugins_window/attention.rs` `fingerprint_line`(같은 공용 view, Attention `reason_detail` · Add `trust_box`) |
| Attention › Signature invalid (`detail.cause`) | `attention.rs` `draw_reason_detail` → 공용 `plugin_signature_invalid_detail` — `Signature` 머리글 아래 고정 문구(term-sm, text-muted, 폭 상한 `measure_lg`)와 실패 원인 한 줄(mono caption, text-muted). 원인은 `RejectedPlugin.cause`(서명 검증 오류의 표시 문자열)다. fingerprint 줄은 없다 | `plugins_window/attention.rs` `reason_detail` — 같은 공용 view, 원인 예시 `tasty-plugin.toml.sig sidecar missing` |
| `AddPluginForm` (trust 흐름) · `TrustBox` | `src/view/plugins/ui/add.rs` `draw_add_tab` → `draw_add_form` — 제목 없이 공용 `plugin_add_path_picker`(`font-label` 머리글 `Plugin folder` · mono 입력 + 폴더 아이콘 · `Find folder…` secondary + folder 아이콘 · `Verify` primary · 백틱 구간을 mono text-secondary 로 그리는 term-sm 설명 문단, 간격 `spacing_sm`, 폭 상한 `measure_xl`) 바로 아래에 확인 전 `plugin_add_empty_hint` 또는 `plugin_manifest_card` + `plugin_trust_box`, 맨 아래 `plugin_add_bar`(`crates/tasty-ui-widgets/src/plugin_add.rs`)를 부른다. 매니페스트를 읽지 못하면 안내 상자 자리에 `plugin_add_read_error`(같은 크기, accent-danger 1px 실선, alertTriangle 16 + `plugins.add_read_error`(읽기·파싱 실패) 또는 `plugins.add_invalid`(선언 검사 실패) body accent-danger, 아래 줄 오류 원문 mono caption text-muted, 사이 `label_detail_gap`, 채움·동작 없음)를 둔다. 블록 사이 간격은 `spacing_lg` | `plugins_window/add.rs`: `form_pane`(확인 전 · 확인 후) / `open_values` / `trust_box` / `action_bar` / `add_bars` / `trust_boxes` — 같은 공용 위젯을 부른다. 매니페스트 카드는 surface-raised · border-default · 안쪽 여백 `spacing_lg` · 항목 간격 `spacing_md`, `font-label` 머리글은 micro · text-muted · `letter-spacing-caps`. 카드와 신뢰 상자 사이와 상자 가로 여백은 `PLUGIN_ADD_INSET`(디자인 `--tasty-size-14`). Homepage 는 text-secondary 밑줄 링크(hover text-primary, 포커스 focus ring), 빈 목록은 caption `None`. 신뢰 상자는 다섯 가지(`PluginTrustKind`: Trusted success · UnknownKey/PermissionsChanged warning · MissingPubkey/SignatureError danger)이며 tone의 tint fill·border, 글리프 16, 제목 body·본문 term-sm text-secondary. 액션 바는 위 구분선, 왼쪽 Grants 문구 또는 막힌 이유(caption·text-muted), 오른쪽 Cancel(Ghost) + Add plugin(Primary, 추가하면 신뢰하게 되는 경우 Trust & add). 매니페스트를 확인하기 전에는 Cancel 만 둔다. 막히면(이미 설치됨 · 공개 키 파일 없음 · 서명 확인 실패, 본체 `add_blocked_reason_key`) 버튼을 disabled로 둔다. 본체는 경로 선택과 카드·신뢰 상자를 `measure_xl` 폭 열의 스크롤 영역에 두고 액션 바의 실제 높이를 뺀 나머지만 준다(`draw_add_footer`). 갤러리 예제는 스크롤 없이 전체를 그린다. 확인 전 안내 상자의 테두리는 공용 `paint_dashed_outline` 으로 그리는 border-default 1px 점선(`border_dash` 4 / `border_dash_gap` 4, 곧은 변만, 모서리 실선 호)이다. 갤러리 Spec `plugin-add-hint-slot`(`plugins_window::draw_hint_slot`)이 점선 안내와 읽기 오류 상자를 Mocha·Latte 짝으로, 점선 규칙을 반경 있는 상자·없는 상자로 보인다 |
| `PluginAvatar` | `src/view/plugins/ui/list.rs` · `attention.rs` — 목록 행(32)과 상세 identity(46) 넷 | 공용 위젯 `tasty-ui-widgets` `plugin_avatar` / `paint_plugin_avatar` 를 `plugins_window/installed.rs` · `attention.rs` 의 `list_pane` · `detail_pane` 이 부른다 |

severity 는 본체 `src/view/plugins/ui.rs` `is_danger` 를 따른다 — 서명 계열과 예약 id 는 danger, 권한
변경·런타임 오류는 warning. Installed 목록의 health dot 과는 다른 축이다(health dot 은 실행 중
실패 하나만 본다).

검증: specimen 이 아홉 상태(Installed 넷 — 선택 · health error · 무선택 · uninstall 확인(예제 전용 높이 `INSTALLED_CONFIRM_STAGE_H` 640),
Attention 셋 — 목록 있음 · 예약 id · 빈 상태, Add 둘 — 확인 전 · 입력 아래 매니페스트)와 액션 바 일곱(Grants 3 신뢰·미신뢰, 이미 설치됨, 공개 키 파일 없음, Grants 1, No permissions, 서명 확인 실패), 열린 값(Homepage 링크 · `None` · 긴 fingerprint), 신뢰 상자 다섯을 Mocha·Latte로 세로로 모두
그리므로 탭 전환 없이 대조한다. Installed 무대는 `measure_xl`, 입력 아래 매니페스트 무대는 경로 선택 블록·카드·신뢰 상자·fingerprint 줄·액션 바를 담도록 예제 전용 높이 `ADD_VERIFIED_STAGE_H`(760)로 높다 — 본체는
그 자리를 `ScrollArea` 로 접지만 갤러리는 접으면 캡처에서 사라진다. 페이지는 Overlays(idx 3)
이고 이 섹션은 그 페이지 아래쪽(뒤에 `drop-overlay` 하나)이라 스크롤 오프셋을 준다 — 정확한 y 는 위에 섹션이 늘면 밀리므로
오프셋 몇 개를 한 배치로 훑어 고른다([screenshot-methods](../../ai-verification/screenshot-methods.md)).

```bash
TASTY_GALLERY_SIZE=1500x1100 TASTY_GALLERY_SHOT="3@69000:/abs/a.png,3@70000:/abs/b.png,3@71000:/abs/c.png" \
  ./target/debug/tasty-gallery
```

본체 대조는 Plugins 창을 띄우고 그 창 id 로 찍는다. 이 창은 사이드바 버튼에서만 열리고 그
경로를 여는 IPC 가 없으므로(사용자 조작 재현은 release 에 없다 — [사용자 통제와 데이터 원칙](../../principles/control-and-data.md)), 열기는 창 클릭으로
한다. 창 제목은 `Tasty Plugins` 다([screenshot-methods](../../ai-verification/screenshot-methods.md)
의 창 제목 표).

```bash
tasty screenshot --path /abs/host.png --window <Tasty Plugins 창 id>
```

<a id="specimen-공용-헬퍼-dedup"></a>

## 갤러리 예제의 공용 헬퍼

specimen 간 중복 chrome 을 한 곳으로 모은 카탈로그 헬퍼 (`crates/tasty-gallery/src/catalog/`):

| 헬퍼 | 제공 | 쓰는 곳 |
|---|---|---|
| `spec.rs` | `section` / `spec` / `stage`(`StageVariant`) / `cluster` / `meta`(`TokenChip`) / `note` / `do_` / `dont`. `TokenChip::new`는 색 스와치를 그리고, `TokenChip::without_color`는 시안 `Meta`에서 `color`가 없는 토큰(치수·불투명도·폰트 등)을 스와치 없이 그린다. `meta`는 시안 `.meta`·`.dl`처럼 Layout spec과 Tokens used를 같은 폭 두 열로 나누고, 키 열은 가장 긴 키 폭, 값은 값 열 안에서 줄바꿈한다. 창 폭 900 이하에서는 한 열로 쌓는다. Tokens used 는 시안 `.chips`·`.chip`처럼 칩을 가로로 놓고 폭을 넘으면 다음 줄로 보낸다. 칩은 `surface_raised` 배경·`border_default` 1px 테두리·`corner_radius_sm`, 글자는 mono `font_size_caption`(토큰명 `text_primary`, 용도는 `— ` 뒤 `text_muted`)이고, 스와치는 `border_strong` 1px 테두리를 둘러 패널과 같은 색도 구분된다. 시안의 칩 간격 6·세로 여백 3은 토큰이 아니라 `spacing_xs`(4)로, 가로 여백 8은 `spacing_sm`, 스와치 11은 `font_size_caption` 값으로 그린다. Do / Don't 콜아웃 채움은 `tint_fill_alpha()` | 카탈로그 `.rs` 대부분 |
| `toast_card.rs` | `tasty-type-appearance` 의 `ToastKind` · `tasty-ui-widgets` 의 `draw_toast_single_card` 재수출 — 정의는 여기 없다 | toast(components/widgets) · kb import/export |
| `popup_frame.rs` | `draw` (`ContentInset` · `TitleButtons`) — surface-raised 프레임 + border-strong + 타이틀바 우측 버튼군(`draw_title_buttons`: IconButton sm 규칙의 `close` / 전체화면 `fit`) + 제목(`draw_title_text`: 본체와 같은 `popup_title_text_rect`·`elide_popup_title`, 버튼 수와 무관하게 대칭, 잘리면 띠를 돌려줘 `title_tooltip`이 전체 제목 Tooltip 을 붙인다) | notification_panel · info_modal · fullscreen_stage (뒤의 둘은 `draw_title_buttons`·`draw_title_text`·`TITLE_BAR_HEIGHT` 만) |

<a id="토큰-이름-표기"></a>

### 토큰 이름 표기

갤러리가 그리는 글자(Meta의 Layout spec 값과 Tokens used 칩, Spec 제목·설명, 예제 안의 라벨)에는 토큰 이름을 시안 CSS 변수에서 `--tasty-` 접두를 뗀 이름으로 쓴다. 시안 칩의 `--tasty-bg-app`은 갤러리에서 `bg-app`이다. 갤러리 UI 글꼴은 하이픈 두 개를 붙여 그리므로 접두를 그대로 두면 `-tasty-bg-app`처럼 한 줄로 읽힌다. 소스 주석에서 시안 CSS 변수를 가리킬 때는 접두를 둔 원래 이름을 쓴다.

시안도 Meta에서는 같은 표기를 쓴다. 시안 소스는 `--tasty-` 이름을 그대로 두고, `gallery/parts/specimen.jsx`의 `Meta`가 칩과 Layout spec 값을 그릴 때 접두를 떼며 칩 툴팁에는 전체 이름을 보인다. Meta 밖의 설명문(Spec 본문 등)은 시안에서 접두를 유지한다. 갤러리는 글꼴 때문에 설명문에서도 접두를 떼므로, 이 부분은 그리는 방식만 다르다.

`crates/tasty-gallery/tests/meta_token_notation.rs`가 모든 Spec을 GPU 없이 한 프레임 그리고, 출력 shape의 글자와 Section·Spec 제목에 `--tasty-`가 있으면 실패한다. 그리지 않는 문자열과 주석은 보지 않는다.

<a id="primitive-컴포넌트-레이어-components"></a>

## 기본 컴포넌트 (Components)

디자인 `components/**` 의 atomic primitive ↔ `tasty-ui-widgets` 공용 함수 ↔ 갤러리
`Components` specimen 3자 매핑. 본체 팝업과 갤러리가 **동일** `tasty_ui_widgets::*` 를
호출(mirror 아님 — demo=main). 위젯 구현은 `crates/tasty-ui-widgets/`(메인+갤러리 양쪽 의존).

| 디자인 컴포넌트 | tasty-ui-widgets | 갤러리 specimen |
|---|---|---|
| `core/IconButton` | `IconButton` (ghost/solid/active, sm/md) | `prim_icon_button` |
| `core/Button` | `Button` (primary/secondary/ghost/danger/agent × sm/md/lg, leading_icon/trailing_icon) · `ground(Ground::Raised)` — surface-raised 위 ghost 글자 `button-ghost-raised-fg`, hover `button-ghost-raised-fg-hover`(다른 variant 는 그대로) | `prim_button` (클러스터 "ghost ground=\"raised\"" — Mocha·Latte surface-raised 칸에 panel ghost 와 raised ghost) |
| `core/Button` disabled | `Button::enabled(false)` — 모든 변형이 같은 `button-disabled-*` 박스와 잉크, ghost 는 박스 없음, opacity 없음 | `prim_button::draw_disabled`(Spec "Disabled — ink, never opacity", Mocha·Latte) |
| `forms/Input` | `Input` (icon/addon/mono/invalid/disabled, focus ring) · `ground(Ground::Raised)` — surface-raised 위 상자 `input-raised-bg`·`input-raised-border`, disabled `state-disabled-fill-raised`·`state-disabled-border-raised`, readonly `input-readonly-raised-*`(invalid·focus 테두리는 그대로) · `read_only` — disabled와 같은 `input-readonly-*` 상자, 값은 text-secondary, 포커스(1px focus 테두리, ring 없음)·선택·복사 가능 · 캐럿은 깜박이지 않는 1px text-primary 막대(`apply_theme_to_egui`) · 글자 칸·addon·trailing 은 상자 padding(`input-padding-x`) 바로 안쪽에 놓인다. 시안 `input` 의 padding 이 0 이므로 egui TextEdit 의 가로 margin 도 0 이고 세로는 egui 기본값이다 | `prim_input` (클러스터 "readOnly vs disabled — same box, readable value") · 캐럿은 `prim_text_caret`(Spec "Text caret — steady, never blinks". 캐럿은 실제 포커스에서만 그려져 입력란을 눌러 확인한다) |
| `forms/CodeArea` | `CodeArea` → `CodeAreaOutput` — Input 과 같은 상자(`input-bg`·`input-border`·focus 테두리+ring·invalid·disabled), mono `codearea-font-size` · 줄 높이 `line-height-ui` · 줄바꿈 없음, 안쪽 여백 `codearea-padding-y`·`-x`, 거터(최소 `codearea-gutter-width`, `codearea-gutter-bg`·`-fg`, 오른쪽 `codearea-gutter-border`, 가로 스크롤에도 고정), `error_line` = 거터 번호 `codearea-error-fg` + `tint-fill-alpha` danger 띠(invalid 함의), 바깥 높이(테두리 포함, kit 의 border-box)가 `codearea-max-height` 에 닿으면 스크롤. Enter 는 줄바꿈이다. 확정·취소 키는 위젯이 정하지 않고 호출자가 `CodeAreaKeys` 로 넘기며(본체는 단축키 설정 `code_area_apply`·`code_area_cancel`, 기본 Mod+Enter·Esc), 포커스 중 맞으면 `submit`·`cancel` 로 돌려준다. 갤러리 표본은 키를 넘기지 않는다. 오류 줄 번호의 semibold 는 egui 에 굵은 UI 글꼴이 없어 재현하지 않는다 | `prim_code_area` (시안 forms 카드 표본: 폭 420 · minRows 3 · errorLine 2, 빈 입력 · disabled) |
| `core/Tag` | `tag` · `tag_with_icon` (default/accent/agent/success/warning/danger/info/attached + dot 또는 앞 글리프. 상태 variant도 `tag-bg` 채움, 글자는 `tag-*-fg`) · disabled `tag_disabled` — 모든 variant가 `tag-disabled-*` 중립 상자와 ink, 점도 같은 ink · 대문자 `tag_caps`(시안 `caps` prop) — 라벨을 대문자로 바꾸고 `letter_spacing_caps(tag_font_size)` 자간으로 그린다. 대문자로 그리는 Tag는 모두 이 함수를 쓴다(사이드바 원격 미러 표지) · 낱말은 `font-label`, 값은 `tag_mono`·`tag_mono_width`(시안 `mono` prop) — ID·버전·크기·권한·surface kind처럼 모든 언어에서 고정폭인 값 | `prim_chips` (클러스터 "disabled", "mono") · 대문자는 `sidebar` 미러 행 |
| `core/Badge` | `badge` / `badge_dot` · disabled `badge_disabled` — `badge-disabled-*` 중립 채움과 ink | `prim_chips` (클러스터 "disabled") |
| `core/Tag`·`core/Badge` disabled 문맥 | `disabled_chip_scope` / `in_disabled_chip_scope` — 문맥 안의 `tag`·`badge`·`badge_dot`은 variant와 관계없이 disabled 변형으로 그린다 | disabled ListCtrl 행의 trailing(아래 `data/ListCtrl`) |
| `core/Kbd` | `kbd`(키캡 시퀀스, 키 글자 `kbd-fg`, 사이 `+`는 `kbd-plus-fg`, 선택 행은 `kbd-plus-fg-active`) | `prim_chips` |
| `gallery/components/index.jsx` "Selected row — meaning glyphs, match run, Kbd +" | 쉬는 행과 활성 행의 글리프 색, Tag, 일치 구간 밑줄, 키캡 `+` | `selected-row` (`components/selected_row.rs`) |
| `forms/Checkbox` | `checkbox` | `prim_forms` |
| `forms/Switch` | `switch` | `prim_forms` |
| `forms/Select` | `select`(토큰 트리거 + egui popup 안의 공용 옵션 행 `menu_option` — 현재 값만 selected. 트리거 테두리는 쉴 때 `select-border`, 호버 `border-strong`, 목록이 열려 있는 동안 MultiSelect 와 같은 `select-border-focus` 에 시안 focus box-shadow 와 같은 바깥 1px ring. 글자는 값 `select-fg` · 미선택 `select-placeholder` · disabled 는 값 유무와 관계없이 `state-disabled-fg`(chevron 도). 회귀 검사 `crates/tasty-ui-widgets/tests/select_open_border.rs`) · `select_pinned`(상호작용 없이 같은 칠로 normal·hover·open 모양을 고정한 트리거 — 갤러리 상태 견본용) · `select_on`·`select_or_placeholder_on`(바탕 `Ground` 를 받는 같은 트리거 — `Ground::Raised` 면 상자 `select-raised-bg`·쉬는 테두리 `select-raised-border`, disabled `select-raised-disabled-bg`·`select-raised-disabled-border`, hover·open·글자는 그대로) · `select_or_placeholder`(같은 트리거 + "아직 안 고름" 상태 — 트리거에만 placeholder 를 `text_placeholder` 색으로. placeholder 는 목록의 행이 아니다 — 열린 목록은 실제 옵션만, 체크 없음. 값을 지울 수 있어야 하는 Select 는 "None" 같은 실제 옵션을 둔다) | `prim_forms`(시안 "Select states": Mocha·Latte 칸마다 value · hover · focus/open · no value · disabled+value · disabled+no value 여섯 칸, 칸 폭 `field-width-lg`. 아래 surface-raised 줄은 ground="raised" 의 value · focus/open · no value · disabled 와 비교용 panel 칸) · `prim_nav::draw_menu_item_selected` 의 "no value" 패널 |
| `forms/MultiSelect` | `multi_select` / `multi_select_summary` / `multi_select_popup_id` (`select` 와 같은 트리거 토큰 + checkbox 행 팝업(프레임은 `multiselect-menu-bg`·`multiselect-menu-border`·`multiselect-menu-radius`·안쪽 여백 `multiselect-menu-padding`·`shadow-popover`, 트리거 아래 `multiselect-menu-gap`; 행 높이 `multiselect-row-height`·좌우 `multiselect-row-padding-x`, 행 사이 간격 0, 포인터가 올라간 행 전체 `multiselect-row-bg-hover`, 행 어디를 눌러도 그 행만 토글) + `CloseOnClickOutside` + 요약 라벨 3분기 + 메뉴 max-height 스크롤/max-width 클램프 + 행 단위 disabled 마스크 + 일괄 선택/해제 액션 행(opt-in, accent + separator, 스크롤 밖 고정) + 키보드 내비(↓/Enter/Space 열기 · ↑↓/Home/End active 행 이동(disabled 건너뜀) · Space/Enter 토글(안 닫힘) · Esc 닫기(포커스 유지) · Tab 닫고 이동, active 행은 `multiselect-row-bg-active`(→ surface-active) 배경)) | `prim_multiselect` |
| `forms/SearchableSelect` · `gallery/components/searchable-select.jsx` `#searchable-select` | `SearchableSelect`(builder: `pinned`·`loading`·`width`·`enabled`, `show(ui, theme, &mut Option<String>)` 은 값이 바뀌면 true, `show_response` 는 같은 동작에 트리거 상자의 `egui::Response` 를 더한 `SearchableSelectResponse` — 행 배치처럼 트리거 자리가 필요한 호출자용, 응답 rect 에 팝업은 들어가지 않는다) / `SearchableSelectOption`(value·label·disabled) / `SearchableSelectLabels`(placeholder·검색 안내·결과 없음·로딩 — 위젯은 번역을 소유하지 않는다) / `searchable_select_filter`(앞뒤 공백을 뺀 대소문자 무시 부분 문자열) / `searchable_select_popup_id`·`searchable_select_search_id`. 트리거는 Select 부품(높이 `select-height`, 쉴 때 `select-border`, 호버 `select-search-trigger-border-hover`, 열림·포커스 `select-border-focus` + 바깥 1px ring, chevron 은 열리면 위, disabled 는 `state-disabled-*`, 값 없음은 placeholder 색, 끝 말줄임). 목록에 없는 값과 검색으로 숨은 현재 값도 트리거에 그대로 보인다. 팝업은 메뉴 popover(`with_popover_frame_ring`, 둘레 `select-search-inset`, 트리거 아래 `select-search-menu-gap`, 폭은 트리거~`select-search-menu-max-width`, 전체 높이 `select-search-list-max-height`)이고 검색칸 · 구분선 · 고정 행(+구분선) · 필터한 옵션 행(MenuItem, 키보드 활성 `surface-active`) · 상태 행(결과 없음 · Spinner 14 + 로딩, `select-search-status-fg`, 누를 수 없음)을 쌓으며 옵션 목록만 스크롤한다. 열면 검색칸 포커스, ↑↓ 는 보이는 활성 행(고정 행 먼저)을 끝에서 멈추며 이동, Enter 확정·닫기, Esc 닫고 트리거 포커스, Tab 닫기, 열 때 활성 행 = 현재 값, 입력마다 첫 일치 행, 닫을 때마다 검색어 초기화. 옵션 클릭만 닫고 검색칸 클릭은 닫지 않는다(`CloseOnClickOutside`). 열린 채로 `enabled(false)`가 되거나 감싼 Ui가 비활성이 되면 자기 목록만 닫아 비활성인 동안 값이 바뀌지 않는다(다른 위젯의 팝업은 그대로). `show_specimen` 은 시안 specimen hook(`open`·`query`·`activeIndex`·`hoverIndex`·`focusSearch`·`hoverTrigger`) 상태를 같은 그리기 코드로 정적으로 그린다. 회귀 검사 `crates/tasty-ui-widgets/tests/searchable_select.rs` | `prim_searchable_select`(세 무대 × Mocha·Latte: 트리거 짝 비교 · 트리거 상태 6칸 · 열린 목록 9칸) · `prim_forms` 의 "Searchable Select" live 칸 · 본체 소비자는 Appearance 글꼴 행(아래 Appearance › General · Font override 행) |
| `forms/AutoComplete` | `AutoComplete` / `autocomplete_dropdown` (Input 트리거 + menu container + MenuItem 행 middle-ellipsis + substring 필터 + match highlight, 활성 행은 밑줄 + max-height 스크롤) | `prim_autocomplete` |
| `plugins.jsx/PathField`(:59) | `PathField` / `PathFieldOutcome` (AutoComplete 트리거 + Go IconButton, 편집/이동/원복 결정 = markdown `addr_outcome` 포팅, idle=secondary/editing=primary) | `prim_path_field` |
| `feedback/StatusDot` | `status_dot`(kind+pulse+`Ground`, 점과 라벨 사이 `space-xs`, 공용 표 선택 행과 `Ground::Active`는 `-active` 색) | `prim_status_dot` · `selected-row` |
| `components/feedback/LoadingOverlay.jsx` · `gallery/layouts/loading-overlay.jsx` | `paint_loading_overlay` / `LoadingPlacement`, `Spinner::still_arc`, `Tooltip::second_line`(로딩 둘째 줄) | `attach-loading` (Components): 시안 일곱 Spec을 Mocha·Latte로 그린다. 네 자리 on/off와 rect 점선 · 탭 상태(비활성·활성·hover·attention 둘·busy·move+표지·좁은 120·Ctrl 키캡·넘침 줄) · LNB 카드 상태(REMOTE·부제·설명·배지·Alt 키캡) · 레일 아바타 상태(모서리 칩 넷·attached ring·키캡) · surface(터미널·검색줄+move ring·빈 surface·WebView 의도) · 세 자리 동시·모션 감소·UI 배율 ×1/×1.25/×1.5 · 문구와 툴팁. 탭·카드·아바타는 갤러리 kit의 `tab_cell`·`paint_ws_row`·`paint_avatar`로 그리고, 시안의 markdown egui 예제는 Tasty에서 WebView이므로 빈 surface로 대신한다. 시안 a11y 항목의 `role=status`는 제공하지 않는다. 네이티브 앱은 AccessKit을 빌드하지 않아 오버레이에 이 역할을 붙일 경로가 없다([접근성](../../features/accessibility/index.md#목적)) |
| `components/feedback/PendingIndicator.jsx` | `PendingIndicator` (count/time은 호출자 문구, compact, reduced motion) | `attach-loading` (Components) |
| `gallery/layouts/loading-prelayout.jsx` | View 전용 생성 draft와 공용 loading 표시 | `attach-prelayout` (Components): 새 탭·surface/pane 양축 분할 |
| `feedback/Spinner` | `Spinner`(size/color, 모션은 `Theme` 이 결정 · reduced_motion 은 override) | `prim_spinner` |
| `feedback/CenterState` · `gallery/components/index.jsx` `CenterStateG`(Section `centerstate`) | `CenterState` / `CenterStateVariant` / `CenterStateOutput` / `CENTER_STATE_ERROR_GLYPH` (loading·empty·error, 글리프 24 · 제목 · 보조 줄 슬롯 항상 예약, 받은 영역 안 세로 가운데, 오류 글리프 alertTriangle 부품 소유, 선택 액션은 가운데 정렬 밖 보조 슬롯 아래 `center-state-action-gap`, 높이 없는 호스트는 대칭 자연 높이 — 액션이 있으면 위아래 48) | `prim_center_state` (Components `CenterState — empty · loading · error` 의 `center-state` · `center-state-action` · `center-state-unsized` spec) |
| `feedback/Tooltip` | `Tooltip`(text/placement/id_source · `placement_top_then_bottom` · 탭 스트립 규칙 `placement_clear_of_native` · painter 전용 호출부의 `show_in`) · 호버 지연 `tooltip_hover_delay_elapsed` | `prim_help_hint` · convert(잘린 제목) |
| `feedback/HelpHint` | `HelpHint`(text/placement/open/id_source) — `(?)` 글리프 painter 직접 드로잉 + `Tooltip` 조합. `open`은 글리프가 클립 안에 보일 때만 버블을 띄운다 | `prim_help_hint` |
| `navigation/MenuItem` | `menu_item` / `menu_separator`(`.tasty-menu-sep`, 색 `menu-separator` → border-strong. 모든 메뉴 구분선이 같은 색이다)(글자 `menu-item-fg`, 호버·선택 `menu-item-fg-hover`, 키보드 활성 행의 아이콘·단축키·danger 라벨 `menu-item-part-fg-active`. 넘친 라벨은 끝 말줄임). 포인터 없이 호버 모습을 보이는 상태 견본은 `menu_item_with_hover`(마지막 인자 `hovered`)로 같은 호버 배경·글자를 그린다 — Foundations 떠 있는 메뉴와 파일 피커 경로 `…` 메뉴의 첫 행. `selected` 는 `menu_option` / `menu_option_icon` / `menu_option_value`(선택 목록의 옵션 행 — 공용 Select, 본체 egui ComboBox 열린 목록, macOS 수식키 표시 목록): 글자 `menu-item-selected-fg` + 오른쪽 끝 `check`(`menu-item-check-size` 14 · `menu-item-check-fg`), 채움 없음. 옵션 행은 라벨을 줄이지 않는 폭(행 패딩 · 아이콘 · 라벨 · 체크 자리)만큼 목록의 최소 폭을 넓혀 열린 목록이 트리거 폭에 묶이지 않는다 — 갤러리 `menu-item-selected`(Mocha·Latte) | `prim_nav` |
| `navigation/TreeRow` | `tree_row` | `prim_nav` |
| `navigation/Tab` | `horizontal_tab_bar_with_arrows`(기존) | `prim_layout_shell` (Components `Layout shell widgets`) |
| `navigation/DrillDown` | `DrillDown` / `DrillDownView` / `DrillDownOutput` (controlled list⇄detail content-swap, back bar ←(ghost IconButton sm)+제목+actions 슬롯, 본문 내부 스크롤, 0ms 즉시 전환 — opt-in animate 는 장식이라 미전사) | `prim_drilldown` |
| `data/Table` | `Table`(컬럼 정의[제목·폭·정렬]·정렬 인디케이터·sticky 헤더·행 선택. 머리글은 대문자에 `table_header_tracking` 자간이고, 글꼴은 시안의 mono 가 아니라 비례 글꼴 `strong` 이다. 탐색기 상세 보기도 같은 머리글을 쓰고, 시안 `DetailHeader` 도 이 공용 머리글(UI 글꼴·`table-header-font-size`)이다) | `prim_table` (Components `Table · ListCtrl`) |
| `data/ListCtrl` | `ListCtrl` / `ListCtrlItem` / `ListCtrlOutput` (label+description+leading icon+trailing 슬롯+drill-in chevron, divided 헤어라인, selected surface-active+2px accent 좌측 바, 선택 행의 설명·아이콘·chevron은 `listctrl-part-fg-selected`, disabled, empty_label). disabled 행은 ink 규칙 — chevron 숨김, trailing은 `disabled_chip_scope` 안에서 그려 Tag·Badge가 disabled 변형이 되고, fade 없이 행 Sense만 hover로 둔다 | `prim_listctrl` · `prim_listctrl::draw_disabled_trailing`(Spec "Disabled row with a trailing marker — the ink rule", Mocha·Latte) |
| `feedback/Toast` | `crates/tasty-ui-widgets/src/toast.rs`(그리기, hint 키캡은 `chip.rs::kbd_text_parts_painted`) + `src/adapters/ui/toast.rs`(상태·레이어, `binding_hint`) | Components `Toast`(시안 Toast Spec 6장, hint 2장 · Toast stack의 hint 1장). agent·icon은 카탈로그 전용이라 갤러리 본체 카드에 없다 |

**primitive 케이스 커버리지**: 디자인 jsx 의 변형까지 specimen 에 포함 — Button
`leadingIcon`/`trailingIcon`(prim_button), Input `block`(width 미지정 시 가용폭 채움),
Select `block`(가용폭을 width 로 전달), MenuItem `disabled`(enabled=false).

시각 비교는 본체의 격리 인스턴스에서 `ui.screenshot`을 사용하거나 갤러리의 `TASTY_GALLERY_SHOT=<idx>:<png> ./target/debug/tasty-gallery`로 캡처한다. 갤러리는 선택한 예제가 안정되도록 4프레임 뒤에 캡처하고 종료한다. 키보드 내비게이션은 갤러리에 키 주입 경로가 없으므로 본체에서 `debug.inject_egui_key`로 확인한다. 토큰값뿐 아니라 열림·닫힘, 선택 유지, 긴 목록·라벨의 스크롤과 말줄임도 확인한다.

<a id="layouts-composition-specimens"></a>

## 레이아웃 예제 (Layouts)

상위 화면 idiom 데모. 본체 binary 의존 0 — layout·색·폰트·간격은 Theme 토큰, 상태는
thread-local mock. `crates/tasty-gallery/src/catalog/widgets/<name>.rs`.

### 1 depth (general list → detail)

`crates/tasty-gallery/src/catalog/widgets/layout_1depth.rs`(`onedepth`). **대응하는 본체
함수가 없다** — 특정 창이 아니라 좌측 고정 리스트(200) → 우측 detail 배치 관용구 자체를
보이는 데모다. Plugins 창의 미러는 이것이 아니라
`crates/tasty-gallery/src/catalog/components/plugins_window.rs` 이고, 그쪽은 목록 폭을 본체와
같은 접근자 `Theme::plugins_side_panel_width`(240)에서 읽고 행 높이도 40 이다(위
[Overlays — plugins window](#overlays--plugins-window) 절). 필터가 놓이는 자리도 다르다 —
본체 Plugins 창의 필터는 헤더 밴드 우측이고 이 idiom 데모는 목록 안이다.

### 2 depth (Settings idiom)

디자인 `ui_kits/terminal/overlays/settings_window.jsx` ↔ 본체
`src/view/settings/ui.rs`(+ `settings/ui/tabs/*`, `keybindings_tab.rs`) ↔ 갤러리
`components/settings.rs` (Overlays `settings` · `settings-controls` specimen). `settings` 는 시안 갤러리
`SettingsFrame` 의 620×380 전시 구성(L1 탭 넷, L2 168 · 항목 셋, Theme preset 카드 둘, 버튼 줄)을 그대로
옮긴다. 제품 창 1100×700 · L2 200 · L1 탭 일곱은 Meta 의 canonical 값과 본체에만 있다. L1 높이 44 는
본체 `SETTINGS_HEADER_HEIGHT`(44)와 같은 `titlebar_height + spacing_sm` 도출이다. `settings-controls` 는
시안 갤러리에 없는 갤러리 전용 예제로, 콘텐츠 열 최대 폭(620) 안에 행·스위치·색 override·언어 선택을 모았다.
Layouts 의 `widgets/layout_2depth.rs`(`twodepth`)는 이 미러가 아니라 특정 창에 매이지
않는 일반 2단계 레이아웃(168/40, 토큰으로 계산)이다.

| 디자인 jsx 컴포넌트 | 본체 (`src/view/settings/ui.rs`) | 갤러리 (`components/settings.rs`) | 비고 |
|---|---|---|---|
| `SettingsWindow`(container, 824×472) · 갤러리 `SettingsFrame`(620×380) | `draw_settings_panel` | `draw` | 본체 창 크기는 `settings-window-width`/`-height`(1100×700)다. 갤러리는 시안 갤러리처럼 620×380 전시 크기로 그린다 |
| L1 top tabs (underline) | `draw_l1_tab_band` | `l1_band` / `l1_tab` | 밑줄 스타일을 별도 탭 구현으로 복제하지 않는다. **공유 위젯을 쓰지 않는다** — 양쪽 다 자기 `Frame` 으로 밴드를 그린다. 좌측 타이틀·세로 구분선이 탭과 같은 줄에 들어가야 해서 탭만 담는 컨테이너에 안 맞는다 |
| L2 sidebar(필터+리스트, 200) | `draw_l2_sidebar` | `l2_sidebar` / `l2_item` | 필터 Input + sub-section 리스트. 본체 200, 갤러리는 시안 전시 크기 168 이고 공유 위젯을 쓰지 않는다 — 본체는 모달 셸이 소유하는 `SidePanel`(오른쪽 1px vline), 갤러리는 `Frame`. `tasty_ui_widgets::two_depth_layout_filtered` 는 콘텐츠 안에 놓이는 둥근 테두리 패널(`SUB_TAB_PANEL_WIDTH` 150)이라 **다른 idiom** 이다 |
| `Row`(라벨 열 + 컨트롤) · `RowCaption` | `tasty_ui_widgets::SettingsRow` — 열 폭 `settings_label_column`(서브탭 최장 라벨을 `settings-label-width` 150 … `settings-label-max-width` 240 으로 clamp, 넘치면 열 안 줄바꿈), 행을 직접 짜는 화면은 `settings_label_cell` + `settings_label_gap`. 컨트롤이 라벨 옆에 들어가지 않으면 행마다 쌓는다(`settings_stack_row` — 라벨 전폭, 컨트롤은 행 시작 x 에 `settings-row-stack-gap`, 되돌릴 때 `settings-row-stack-hysteresis` 여유. 갤러리 `settings-general-grid` 끝의 시안 b12 "Settings rows at narrow widths" 창 폭 700 · 520 · 420 견본) | `row`(`settings-controls`) · 각 서브탭 specimen 의 `SettingsRow` | gap 16(`settings-label-gap`)·min-h 32(`--tasty-settings-row-min-height`)·라벨 본문 크기 text-secondary. `hint` 있는 행은 라벨 뒤 `HelpHint`(placement Bottom, gap `help-hint-gap`)를 라벨 열 안에 둔다 — 아래 caption 과 중복 금지. 행에 딸린 caption·callout 은 행 아래 `settings-row-caption-gap`(4)·`measure-md`(남은 폭이 더 좁으면 그 폭에서 줄바꿈). 본체 적용: General 의 모든 서브탭 · Terminal(General·TUI·Mouse Capture·Performance) · Appearance(General 글꼴 격자·합자·배경 투명도, 글꼴 override, plugin 기여 페이지) · Misc › Task pipeline · Misc › Anomaly detection · Keybindings General ~ Scripts(같은 150 … 240 clamp 를 서브탭의 엔트리·quick-switch 라벨 또는 스크립트 이름으로 잰다, 행은 `settings_stack_row`(`align_top`)로 그린다 — 갤러리 `settings-keybinding-rows`) |
| `KeyRow`(Keybindings General ~ Scripts 녹화 버튼 행) | `keybindings_tab/entries.rs` `draw_keybinding_entries` · `quick_switch.rs` `draw_quick_switch_section` · `entries_scripts.rs` `draw_script_bindings` | `settings_keybinding_rows` (`settings-keybinding-rows` spec) | 라벨 열은 위 `Row` 와 같은 규칙(`settings_label_cell` 높이 = 버튼 높이). 녹화 버튼 `kb-record-width`(→ `kb-ie-slot-min-width` 140, 글자가 길면 넓어진다) × `kb-record-height`(→ `kb-ie-slot-height` 24), mono caption 글자, 채움 surface-raised, 1px `kb-record-border`(→ border-default) 테두리(Import / Export 녹화 슬롯과 같은 모양). 호버하면 채움은 그대로 두고 테두리만 `kb-record-border-hover`(→ border-strong)다. 추가(+) 버튼 `kb-record-add-width`(→ size-32)는 채움 없이 테두리와 text-muted plus 아이콘(icon-size-sm)만 그린다. 바인딩이 없는 행은 + 없이 140 폭 None 슬롯 하나(채움 없음, caption 글자 `kb-record-empty-fg` → text-muted)이고 누르면 첫 바인딩을 녹화한다. quick-switch 슬롯과 스크립트 바인딩의 빈 슬롯도 같은 None 슬롯이다. 세 모양은 공용 `tasty_ui_widgets::kb_record_slot` 이 그린다. quick-switch 수식키 행(Tab·Workspace·Category)은 공용 `select`(폭 `field-width-md`, 트리거 28)이고 caption 이 없다 — 갤러리 `settings-keybinding-rows` 둘째 무대(Workspace 서브탭 수식키 행 + 슬롯 행 둘). quick-switch의 각 행은 고정 녹화 슬롯 하나이며 추가(+) 버튼이 없다. 일반 바인딩 행은 기존 추가(+) 버튼을 유지한다. 행 안 버튼 사이·줄바꿈된 버튼 줄 사이 `space-xs`, 행 사이 `kb-row-gap`(→ space-sm 8), 구분선 없음. quick-switch 수식키 행과 첫 슬롯 행 사이도 `kb-row-gap` 이다. 회귀 검사 `keybindings_tab/entries_tests.rs` · `quick_switch.rs` `select_row` |
| "Settings · control metrics" Spec(단축키 행 · 드래그 반전 modifier · font combo · 열린 Select) | `keybindings_tab/entries.rs` `draw_keybinding_entries` · `keybindings_tab/drag_flip.rs` `draw` | `settings_control_metrics` (`settings-control-metrics` spec, Mocha·Latte) | 무대는 본체 General 서브탭의 실제 세 필드(Open file picker · Command palette · Toggle sidebar)와 기본 바인딩, 구분선, 드래그 반전 행이다. 행·슬롯·반전 행 그리기는 `settings_keybinding_rows` 와 공유한다. 시안 무대의 Category switch modifier 행은 본체에서 Workspace 서브탭에 있어 넣지 않고, 반전 행은 본체처럼 General 엔트리 끝에 둔다. font combo(SearchableSelect, `font-combo-*` 는 `select-search-*` 별칭)·열린 Select 는 무대 없이 meta·토큰 칩으로만 적는다 |
| `Mono`(섹션 헤딩) | — | `mono` | micro(10)·uppercase·text-muted |
| `Note` | — | `note` | `measure-md`(400) 폭·text-muted |
| Theme preset 카드(색 띠 34 + 라벨) | — | `theme_swatch` | 시안 `SettingsFrame` 카드 — 띠 34 · 라벨 여백 6/9 · 선택 시 accent 테두리 + 1px ring, 카드 사이 10 |
| 색 행 hex 칸(Colors·Tasty·Terminal surface 배경) | `tabs/appearance.rs` 의 세 행 → `color_row_line`(높이 control-height = `input_height`, 세로 가운데) + Default 칸 `default_hex_field`(`Input::mono().read_only(true)`) | `settings_appearance_colors` (`settings-appearance-colour-rows` spec, Mocha·Latte) | Default = 읽기 전용(`input-readonly-*`, 값 text-secondary, 1px focus 테두리·선택·복사), override = 일반 Input · 폭 `field-width-xs` · 패널 바깥 폭 `--tasty-size-360` |
| Colors 헤더(설명 + Reset all) | `tabs/appearance.rs` `draw_appearance_colors` 헤더 줄 — 오른쪽 `Button` ghost sm, override 0 이면 비활성 "Reset all", 있으면 "Reset all (N)" | `settings_appearance_colors::draw_header` (`settings-appearance-colors-header` spec, Mocha·Latte) | 시안 `ui_kits/terminal/overlays/settings_window.jsx` `ColorOverridePicker` 헤더. 갤러리는 본체 헤더 줄의 미러이고 패널 바깥 폭 `--tasty-size-360` |
| Colors › Terminal-specific 5행 | `tabs/appearance.rs` `color_groups` — `selection_bg`·`vi_cursor_bg`·`search_match_bg`·`search_match_active_bg`·`search_match_active_fg` 순서, 활성 매치 글자색은 그 바탕색 바로 뒤 | `settings_appearance_colors::draw_terminal_rows` (`settings-appearance-terminal-rows` spec, Mocha·Latte) | 행 이름은 키 이름 그대로(번역 없음), 견본은 다른 행과 같은 평범한 견본. 기본값은 각 테마 파일에서 읽는다. Reset all 이 이 행도 지운다. 전체 6그룹 47행 |
| 글꼴 override 격자(Terminal·Explorer·plugin surface) | `tabs/appearance.rs` `font_override_grid` → 행마다 `tasty_ui_widgets::override_row`(기본값 행의 컨트롤은 적용 값을 담은 읽기 전용 Input, 적용 값이 없으면 빈 칸) · 미리보기 `draw_font_preview` 를 격자 아래에 | `settings_font_override` (`settings-appearance-font-override` spec, Mocha·Latte, 설정 본문 최대 폭 짝 + `--tasty-size-360` 긴 문구 짝) | 행 = 라벨 열(위 `Row` 와 같은 열 폭 규칙, `SettingsRow::show_label`) · 컨트롤(family 는 기본 글꼴 행과 같은 `SearchableSelect`, family·file `field-width-lg`, size·line height `field-width-xs`, DPI Select `field-width-md`) · 뒤따르는 Checkbox "Use default"(켜면 컨트롤 disabled) · 칸 사이 `settings-label-gap`, 위아래 `space-xs`, 최소 높이 `settings-row-min-height` · 체크박스가 남은 폭에 들어가지 않으면 컨트롤 아래 줄(줄 사이 `space-xs`) · 미리보기는 창 폭과 관계없이 격자 아래 — caption "Preview", Focused·Unfocused 를 나란히 두고 남은 폭을 반씩 나눈다. 한 칸이 `font-preview-min-width`(→ `field-width-lg` 200)보다 좁으면 Unfocused 가 아래로 내려간다(사이 `space-md`) — 칸마다 1px 테두리(Focused `border-strong`, Unfocused `separator`)·`radius`, 안쪽 여백 `font-preview-padding-y`(space-sm)·`font-preview-padding-x`(space-md), 줄 높이 `line-height-ui` × 글꼴 크기. 칸 내용은 표본 네 줄(라틴·한글·숫자·가나)이다. 본체는 실제 surface 의 focused/unfocused 배경·효과 글꼴·focused 전경색을, 갤러리는 대역으로 `bg-app`/`bg-panel`·mono `font-size-term`·`text-primary` 를 쓴다 · mono caption 요약 줄 |
| Appearance › General 기본 글꼴(글꼴 행 · 미리보기 · 합자 · 배경 투명도) | `tabs/appearance.rs` `draw_appearance_general` → `default_font_section`(행마다 `SettingsRow` · 미리보기 `draw_font_preview` 를 글꼴 행 아래에) | `settings_font_override::draw_general` (`settings-appearance-general` spec) | 단일 열이다. 글꼴 다섯 행(라벨 열은 다른 설정 행과 같은 150 … 240 clamp, 예외 없음) → `space-lg` → 콘텐츠 전폭 미리보기(글꼴 override 와 같은 배치·토큰) → 구분선 → 합자 · 배경 투명도 행. 행 사이 `settings-row-gap`. 글꼴 행은 공용 `SearchableSelect`(`font_family_picker`)다 — 폭 `field-width-lg` 200, 남은 폭이 더 좁으면 그 폭으로 줄이고 긴 이름은 말줄임, 트리거 높이 `select-height` 28(같은 화면 DPI Select 와 같다)·chevron ⌄. 기본값 행 "monospace (default)"(`settings.appearance.font_family_default`, ko "monospace (기본)", 값 `""`)는 검색어와 관계없이 맨 위 고정 행이고, 목록을 받는 중이면 로딩 행, 일치가 없으면 결과 없음 행(`font_search_placeholder`·`font_no_match`·`font_loading`). 검색어는 위젯이 소유해 닫을 때마다 비운다. 저장된 이름이 목록에 없거나(미설치·로딩 중) 검색으로 숨어도 트리거에 그대로 보이고 값은 바뀌지 않는다. 글꼴 override 행은 같은 컨트롤이고 "Use default" 가 켜지면 Select disabled 토큰(`state-disabled-*`)으로 그린다. 열린 목록 치수는 `font-combo-*`(→ `select-search-*` 별칭). 회귀 검사 `default_font_section_tests`(트리거 rect 가 `field-width-lg` × `select-height` · 미리보기 위 · chevron 자리 클릭으로 열림 · 목록에 없는 값 보존 · 숨은 값 보존과 고정 행 · override disabled 토큰). 갤러리 두 견본(General · Font override)도 같은 위젯이다 · 커스텀 글꼴 경로 입력칸은 override 행과 같은 mono `Input`, 폭 `field-width-lg` 이다 |
| 거절된 글꼴(파일·family 행, 토스트) | `tabs/appearance.rs` 와 `tabs/font_rows.rs` 의 글꼴 행 → `SettingsRow::show_with_error`·`override_row_with_error`, `tasty_ui_widgets::field_error` · 토스트는 `App::report_rejected_fonts` | `settings_font_rejected::{draw_file, draw_family, draw_toast}` (`appearance-font-file-rejected`, `appearance-font-family-rejected`, `font-rejected-toast` spec, Mocha·Latte, en·ko·ja) | 파일 행은 Input `invalid` 테두리 + 컨트롤 열에서 시작하는 한 줄(caption · `accent-danger-raised` · `measure-md`, 입력과 사이 `settings-row-caption-gap`). family 행은 같은 줄만 있고 테두리는 그대로다. 포커스 중이나 목록이 열린 동안에는 글자를 지우고 칸을 유지한다. 칸의 높이는 활성이 아니고 포인터 버튼이 눌려 있지 않은 프레임에만 바뀌고(`FieldErrorState::step`), 눌림과 판정 대기의 단계는 `settings_font_rejected/held.rs`의 프레임이 보인다. 체크박스가 내려가면 Input, 줄, 체크박스 순서다. 폭이 모자라면 컨트롤도 행 시작으로 내려간다(`OverrideLayout`). 긴 값은 입력 끝에서 잘리고 hover Tooltip 으로 전체를 보인다. 토스트는 `Warning` 카드에 값을 적고 버튼이 없다. live 프레임의 판정은 확장자만 보는 데모다 |
| footer Cancel/Save | `draw_settings_footer` | `footer` | ghost/primary, gap 8. 갤러리는 시안 `SettingsFrame` 처럼 `transfer-footer-pad-y` · `transfer-pad-x` 여백 |

form-control 폭: `field-width-{xs,color,md,range,lg}` = 90/110/160/180/200 (디자인
`tokens/semantic.css` 미러). 다섯 중 **`range`(180)만 `Theme` 필드로 안 이어져 있다** —
`dtcg.rs` 가 xs/color/md/lg 넷만 잇는다(집합을 셀 때 그 넷으로 세지 마라). `settings` content 는
시안처럼 Appearance › Theme 한 화면만 보여준다 (전 7탭 전수 구현 아님 — skeleton).

### Components 재분류

디자인 gallery `components.html` 구조에 맞춰 갤러리 `Components` = primitive 전용으로 정리.
통팝업/컴포지션 데모(Dialog/Convert/Port Scanner/Approval)는 `Overlays` 로 이동.

<a id="plugin-settings-page-16-b--tasty-design-system-3"></a>

## 플러그인 설정 페이지

디자인 `ui_kits/terminal/overlays/settings_window.jsx:240-248`(Appearance › HTML viewer 페이지) ↔
본체 `src/view/settings/ui/tabs/appearance.rs` `draw_plugin_settings_page`(+ `plugin_setting_row` /
`draw_plugin_toggle` / `draw_plugin_select` / `draw_plugin_number`) ↔ 갤러리
`components/plugin_settings.rs::draw` (Components › `Plugin settings page`).

**미러 방식**: 갤러리는 main 바이너리에 비의존이므로 본체 렌더러(`Settings` 저장소 read/write 포함)를
그대로 호출할 수 없다. 따라서 행 레이아웃·토큰만 공유 위젯(`tasty_ui_widgets::{switch,select}`)으로
**미러**한다(렌더러 공유크레이트 이전 불필요 — `prim_forms`/`settings` specimen 과 동일 패턴).

| 디자인 jsx | 본체 함수 | 갤러리 미러 | 비고 |
|---|---|---|---|
| `Row`(라벨 열 + 컨트롤) | `plugin_setting_row` — 페이지의 Toggle·Select·Number 라벨로 정한 열 폭의 `SettingsRow` | `row`(`SettingsRow`) | `add_space spacing_sm` 뒤 설정 행 격자 |
| `Mono`("HTML viewer") | 페이지 헤더 | mono micro · text-muted | |
| `Default zoom:` `Input`(mono)+`%` | `draw_plugin_number` → `number::number_field` | `plugin_settings.rs` 의 number 행 | 셋 다 **숫자 한 모양**이다 — mono `Input`(width xs, 우측 정렬) + 필드 밖 정적 suffix + 확정 때만 clamp. 상태 셋은 `settings-number` specimen |
| `Color scheme:` `Select` | `draw_plugin_select` | `select`(width `field_width_md`) | follow/light/dark |
| `Allow remote content:` `Switch` | `draw_plugin_toggle` | `switch`(28×16) off | |
| `Sandbox scripts:` `Switch` | `draw_plugin_toggle` | `switch`(28×16) on | |
| `Note` | Note 라벨 | caption · text-muted | |

<a id="banner-banner-02--galleryoverlaysjsx-banner-section"></a>

## 배너

디자인 `gallery/overlays-banners/index.jsx` 의 `#banner` Section(3 Spec) ↔ 갤러리
`crates/tasty-gallery/src/catalog/widgets/banner.rs` (Overlays › `Banner — the floating
top notice`). 배너의 상태별 예제를 제공한다.

**전사 방식**: 디자인 Spec 의 정적 레이아웃(shell chrome · 행 구성 · 우상단 슬롯 ·
스택 z-order)을 1:1 전사한다. hover/카운트다운/큐 같은 시간·상호작용 상태는 egui
immediate-mode 정적 specimen 이므로 **각 상태를 나란히 노출**(toast 스택 데모와 동일
관습 — 라이브 상호작용은 kit `banner.html` 담당).

| 디자인 jsx 함수 | 갤러리 함수 | 비고 |
|---|---|---|
| `BannerShellG` | `tasty_ui_widgets::banner_shell`(본체 `BannerManager::draw`도 같은 함수) | surface-raised fill + 1px border-strong + radius-8 + popover shadow, padding 12/8. `opacity`<1 → 전 색 디밍(recessed) |
| `BannerScope` | `faux_scope` | 탭 스트립(28, 비워둠) + 디밍 콘텐츠 + 배너 존(탭바 아래 8px, 양옆 8px). 배너가 탭바를 덮지 않는 위치 관계 전사 |
| `MouseCaptureBannerG` (Spec 1) | `draw` | 예시 배너: mouse 글리프 + 제목 + 본문 + `Shift` kbd hint + action 2(Secondary/Ghost). × 는 기본 숨김 |
| Spec 2 plain/TTL | `draw_dismiss` | plain(× 노출 상태) + TTL(check 글리프 + 우상단 카운트다운 `6`) 두 행을 Column 으로 |
| `TtlBannerG` countdown | `countdown` | mono micro(10)·text-muted·tabular 숫자 |
| `StackDemoG` (Spec 3) | `draw_stack` | 하위(Pane, 40% 디밍, 후면) + 상위(Workspace, warn 글리프, 전면) 두 shell 을 overlap child Ui 로 |
| `BannerMoreMenuG` · `MoreLabel` (Elastic width) | `banner_mouse_capture::draw_elastic` → 행은 `tasty_ui_widgets::banner_more_row`(본체 `mouse_capture_menu` 도 같은 함수) | 짧은 이름 · 중간(active) · 긴 이름(288 상한, 이름 말줄임) · ja 줄바꿈(288 유지, 둘째 행 active) · 기각된 danger. 이름은 mono 이고 고정 문구와 같은 행 글자색(`banner-more-app-fg` → menu-item-fg, `-fg-hover`). 고정 문구 + 이름 한 글자 + 말줄임표가 안 들어가면 그 행만 줄을 바꾼다(최소 28, 위아래 `menu-item-wrap-padding-y`, 줄 높이 `line-height-ui`, 아이콘 세로 가운데) — 반영됨 |
| `BannerMoreMenuG` `lang` (Row copy — en / ko / ja / zh-Hans) | `banner_mouse_capture::draw_row_copy` → 행은 `tasty_ui_widgets::banner_more_row` | 예제 메뉴는 en·ko·ja 짧은 이름, en·ja 긴 이름. 아래 Meta 표는 모든 내장 언어의 두 행 앞·뒤 문구를 보인다. 문구는 `t_lang`으로 본체와 같은 `lang/{en,ko,ja,zh-Hans}.toml`의 `[popup.mouse_capture_banner_menu]` 키를 읽는다. en 은 이름 앞 문구만, ko·ja 는 이름 뒤 문구만, zh-Hans 는 앞뒤 문구를 모두 쓴다 — 반영됨 |
| `BannerMoreMenuUnknownG` · `MouseCaptureBannerG app={null}` (Program name unknown — rows shown disabled) | `banner_mouse_capture::draw_unknown_name` → 메뉴 본문은 `tasty_ui_widgets::banner_more_unknown`(본체 `mouse_capture_menu` 도 같은 함수) | 이름 미상·메뉴 열림 배너(en, ⋯ active, 예약 56) + 모든 내장 언어(en·ko·ja·zh-Hans)의 메뉴. 메뉴는 288 고정, 사유 줄(info 14·caption·text-muted, 글리프 top `banner-more-reason-glyph-offset` 1) → 구분선(`menu-separator`) → 비활성 두 행(`state-disabled-fg`, wrap). 문구는 `t_lang`으로 `unknown_reason`·`notice_unknown`·`capture_unknown` 을 읽는다. mirror 별도 예제는 없다 — 반영됨 |

글리프: mouse/check 는 `crates/tasty-icons` 의 `MOUSE`/`CHECK` 글리프(warn = 기존
`ALERT_TRIANGLE`, × = 기존 `CLOSE`). 카탈로그 등록은 Overlays 페이지에 `banner`
Section 1개(6 Spec) — `fullscreen-stage` 다음.

### HTML script notice (inset 배너)

디자인 `gallery/overlays-banners/index.jsx`의 `#htmlscript` Section(5 Spec)과
`ui_kits/terminal/overlays/html_script_banner.jsx` ↔ 갤러리
`crates/tasty-gallery/src/catalog/widgets/html_script_banner.rs` (Overlays › `HTML script notice — the first
inset banner`, `banner` Section 다음). 배너와 마커는 본체가 호출할 공용 함수를 그대로 쓰고,
탭 스트립·WebView 자리·터미널 surface만 갤러리가 흉내 낸다.

| 디자인 jsx 함수 | 공용 함수 / 갤러리 함수 | 비고 |
|---|---|---|
| `HtmlScriptBanner` | `tasty_ui_widgets::html_script_banner`(`HtmlScriptBannerView`) | 셸 > 행 [lock 글리프 `icon_glyph_size_md` · `html_script_banner_glyph()` \| 제목 `banner_title_font_size` · 본문 `banner_body_font_size` · `banner_body_fg`, 줄 수 제한 없이 감쌈 \| Secondary/Sm 액션] + 우상단 닫기 슬롯(top `banner_padding_y`, right `spacing_sm`). 행 gap `banner_gap`, 오른쪽 예약 `icon_button_size_sm + space_sm + space_xs`. reloading은 스피너 `icon_glyph_size_sm` + 라벨이 액션 자리를 대신하고 제목·본문 잉크는 그대로이며 닫기를 숨긴다. loading은 액션을 비활성 Button(중립 상자·disabled ink)으로 그리고 hover 지연 뒤 위쪽 툴팁(`loading_tooltip`)을 보이며 닫기는 blocked와 같다. narrow는 액션이 본문 왼쪽 가장자리에 맞춰 다음 줄 |
| `HtmlScriptMarker` | `tasty_ui_widgets::html_script_marker`(`HtmlScriptMarkerKind`) | Blocked = `LOCK` · `html_script_marker_fg()`, 클릭 가능 / Allowed = `SCRIPT`(design `scriptFile`) · `html_script_marker_allowed_fg()`, 툴팁만. 크기 `html_script_marker_size()`. 툴팁은 `placement_clear_of_native`(위 → 아래, 호출자가 넘긴 WebView 영역 회피) |
| `HtmlSurfaceG` | `html_surface` | 탭 스트립 → `inset_banner_zone` 안의 배너 → `inset_content_rect`의 페이지 자리 |
| `HsTab` | `tab` | `tab_height` · padding `spacing_sm` · gap `spacing_xs` · caption 라벨 · 활성 bg-panel + 하단 `tab_indicator_width` accent-primary · 오른쪽 separator |
| `HsPage` | `page_stand_in` | 터미널 focused bg, mono micro 라벨 + surface-raised 막대 3개 |
| `TermSurfaceG` | `term_surface` | 터미널 unfocused bg, 배너 없음 |
| `HsFailed` | `failed_stand_in` | 페이지 자리 가운데에 `ALERT_CIRCLE` `icon_glyph_size_md` accent-danger → "Failed to load"(`webview.error`, body, accent-danger) → URL(mono caption, text-disabled, 한 줄). 사이 간격 `spacing_sm`, 안쪽 여백 `spacing_md` |
| Spec 1 · 2 · 3 · 4 · 5 | `draw_placement` · `draw_states` · `draw_banner_button` · `draw_markers` · `draw_load_failed` | Spec 5(로드 실패)는 테마마다 현재(배너·lock이 실패 위에 남은 모양)와 결정(실패 상태만)을 나란히 그리고, 두 테마 패널은 시안의 flex-wrap처럼 세로로 쌓는다. Spec 1(배치)·Spec 3(배너 셸 위 버튼)·Spec 4(마커)는 고정 테마(`mocha_fallback`·`latte_theme`)로 Mocha·Latte를 나란히 그린다. Spec 2(상태)는 시안과 같이 페이지 테마를 따른다. Spec 3의 버튼 상자는 `banner_shell`이 여는 배너 문맥이 정한다 |

제목의 semibold는 egui에 굵기 family가 없어 재현하지 않는다([디자인 정합 지침](design-parity-notes.md)).
글리프 nudge 1과 제목↔본문 간격 2는 디자인이 primitive `size-1`·`size-2`를 직접 써서 역할 토큰이 없으므로
위젯의 이름 붙은 상수에 배율만 적용한다. 재로드 커밋 뒤 120ms 페이드는 정적 예제에서 재현하지 않는다.

### HTML remote content notice (inset 배너 · 주소 칸 표지)

디자인 `gallery/overlays-banners/index.jsx`의 `#htmlremote` Section과
`ui_kits/terminal/overlays/html_remote_banner.jsx`·`html_address_bar.jsx`(`trailing` 슬롯) ↔ 갤러리
`crates/tasty-gallery/src/catalog/widgets/html_remote_banner.rs` (Overlays › `HTML remote content notice`,
`htmlscript` 다음). 배너와 표지는 본체가 호출하는 공용 함수를 그대로 쓰고, 주소 띠는 공용
`html_address_bar`에 `remote_marker`를 넘겨 그린다. 페이지 자리만 갤러리가 흉내 낸다.

| 디자인 jsx 함수 | 공용 함수 / 갤러리 함수 | 비고 |
|---|---|---|
| `HtmlRemoteBanner` | `tasty_ui_widgets::html_remote_banner`(`HtmlRemoteBannerView` = 스크립트 배너 view) | 스크립트 배너와 같은 셸·행·단계·narrow·닫기 슬롯(`html_notice_banner` 공유). 글리프만 `GLOBE_OFF` · `html_remote_banner_glyph()`(accent-info) |
| `HtmlRemoteMarker` | `tasty_ui_widgets::html_remote_marker`(`HtmlRemoteMarkerKind`) | 칸 `html_remote_marker_hit()`(20), 글리프 `html_remote_marker_size()`(14). Blocked = `GLOBE_OFF` · `html_remote_marker_fg()`, hover 채움 `html_remote_marker_hover_bg()` + `corner_radius_sm`, 클릭 가능 / Allowed = `GLOBE` · `html_remote_marker_allowed_fg()`, 툴팁만. 툴팁은 `placement_clear_of_native`(위쪽 우선) |
| `HtmlAddressField` `trailing` | `HtmlAddressView::remote_marker` | 입력란 끝, 건너뛴 파일 Tag 뒤. 표지 칸 오른쪽 끝은 입력란 오른쪽 안쪽 여백에서 `spacing_xs`만큼 바깥. 주소 말줄임 예산에 표지 칸을 먼저 뺀다 |
| Spec states · marker · variants | `draw_states` · `draw_markers` · `draw_variants` | variants는 디자인 1–10(차단, 로드 중, 재로드, 허용 표지, 닫은 뒤 차단 표지, 원격+스크립트 6a–6c, 원격 스크립트만 7·7′, 스캔 누락 8a·8b, 좁은 폭 9a·9b, 전역 켬 10)과 mirror 3단을 Mocha·Latte로 그린다. 열 폭이 좁아 두 테마 패널은 세로로 쌓는다. mirror 3단은 시안 그대로 그리지만 본체는 mirror view에 원격 안내·표지를 두지 않는다 |
| Spec `http-s-page-opened-in-the-address-field-remote-then-scripts` · `http-s-page-the-flow-step-by-step` (`HtmlRemoteBanner`·`HtmlScriptBanner`·`HtmlRemoteMarker`·`HsTab`의 `web` prop, `HtmlRemoteSurfaceG` `web`·`loading`·`sandboxOff`) | `widgets/html_web_page.rs`의 `draw_states` · `draw_flow`. surface는 `html_remote_banner.rs`의 `remote_surface`에 `Variant::web`·`loading`·`sandbox_off`를 넘겨 그린다 | 셸·글리프·단계·표지·토큰은 로컬 문서와 같고 문구만 `_web` 키다(`title_web`·`body_web`, sandbox 꺼짐은 `body_web_sandbox_off`, 스크립트는 `action_web`, allowed 툴팁은 `marker_allowed_web`). web 주소 띠는 `HtmlAddressKind::Web`, `loading`이면 새로고침이 진행 중이다. 흐름의 툴팁 여섯 프레임(표지 넷, 두 안내의 loading 툴팁)은 `Tooltip`을 위로 열어 그린다. 안내의 loading 툴팁은 위젯 출력의 `action_rect`(비활성 버튼 자리)를 기준으로 연다. 흐름 4a·4b의 페이지 영역은 본체 webview chrome과 같은 `tasty_ui_widgets::webview_page_tile`의 Loading 타일이다(갤러리 `html_address::page_placeholder`가 부른다). 본체 타일과 같은 배치라 정본 `HtmlPageState`와 세로 배치·페이지 영역 테두리가 다르다(디자인 요청 중) |

원격 안내의 문구 키는 디자인과 앱 모두 `banner.html_remote.*`다. 같은 성격의 `banner.html_script.*` 옆에
두는 기존 규칙을 따르고, mirror 원격 수신 문구인 `[html_remote]`와 헷갈리지 않게 한다. leaf 이름도 스크립트 안내에 맞춰
`action`·`action_loading`을 쓴다. 재로드 커밋 뒤 페이드는 정적 예제에서 재현하지 않는다.

## ui-code (Components)

디자인 `gallery/components/index.jsx`의 Hint text Section 두 번째 Spec "CLI runs inside UI copy" ↔ 위젯
`crates/tasty-ui-widgets/src/ui_code.rs::ui_copy` ↔ 갤러리 `catalog/widgets/ui_code.rs::draw`.
UI 글꼴은 하이픈 두 개를 대시 하나로 이어 그리므로, UI 문장 안의 CLI 명령·옵션·인자는 code run 으로 그린다.
번역 문자열은 그 구간을 백틱으로 감싸고 옵션 글자는 그대로 둔다.

| 디자인 jsx (css) | 토큰 | 위젯/갤러리 |
|---|---|---|
| `font-family: var(--tasty-ui-code-font)` · `font-size: 1em` | `ui-code-font` → `font-mono`, 크기는 문장과 같다 | 손 접근자 `ui_code_font()`(egui Monospace family) · 크기는 문장 크기 |
| `background: var(--tasty-ui-code-bg)` | `ui-code-bg` → `surface-raised` | 줄 높이 전체를 칠하는 사각형 |
| `background: var(--tasty-ui-code-bg-on-raised)`(b12 Spec 의 토스트 카드) | `ui-code-bg-on-raised` → `bg-panel` | 채움은 용기와 한 단계 다른 색이다. 호출자가 `UiCodeContainer`(`Panel`·`Raised`)로 용기를 알린다. 토스트 카드는 `Raised`, 그 밖의 현재 호출처는 `Panel` |
| `color: var(--tasty-ui-code-fg)` | `ui-code-fg` → `text-primary` | run 글자 색 |
| `padding: 0 var(--tasty-ui-code-padding-x)` | `ui-code-padding-x` → `space-xs`(4) | 앞뒤 구간의 `leading_space`, 세로 여백 없음 |
| `border-radius: var(--tasty-ui-code-radius)` | `ui-code-radius` → `radius-sm`(2) | 채움 사각형 반경 |
| `white-space: nowrap` | — | run 안의 공백을 줄을 나누지 않는 공백으로 바꾼다 |

토큰은 `UiCodeTokens::on(theme, container)`가 `Theme::ui_code_bg`(또는 `ui_code_bg_on_raised`)·`ui_code_fg`·`ui_code_padding_x`·`ui_code_radius`로 읽고, `UiCodeTokens::of`는 `Panel` 용기다. fontFamily 토큰은 생성기가 접근자를 만들지 않으므로 `ui_code_font()`가 대신한다.
egui 글자 배치의 배경색은 여백과 반경을 줄 수 없어 위젯이 run 의 글리프 범위로 사각형을 계산해 먼저 칠하고 그 위에 글자를 그린다.
줄 머리에서 시작하는 run 은 글자를 글 단 왼쪽 끝에 두고 채움만 `ui-code-padding-x` 만큼 단 바깥으로 낸다. egui 는 줄을 나눈 뒤 글자를 옮길 수 없고, 디자인은 이 모양을 허용했다(글을 담는 용기의 안쪽 여백이 4 보다 넓다).
가운데 정렬 안내는 왼쪽 정렬로 배치한 뒤 `center_ui_copy_rows` 로 줄마다 가운데에 옮긴다. 줄 폭은 줄 끝 공백을 빼고 run 채움을 넣어 재므로 CSS `text-align: center` 처럼 채움까지 보이는 모양이 가운데에 온다. 이동 거리는 픽셀에 맞춘다.
run 하나가 줄 폭보다 길면 egui 가 run 안에서도 자른다. 갤러리는 시안처럼 13 body(text-secondary)와 11 caption(text-muted) 두 예문을 그리며,
Spec 설명 문장의 `--webhook-port`도 같은 위젯으로 그린다. 세 번째 Spec "Code runs — on raised containers · info modal chips · line start · centred hints"(`draw_followups`)는
토스트 카드(`toast-bg` 위 `Raised` run), 안내 모달 문단(두 명령, 두 번째가 줄 머리), 가운데 정렬 caption 안내를 그린다. 가운데 안내는 DAG 빈 화면처럼 `measure-sm` 폭에서 줄을 바꾼다(시안 예문은 260). 시안 예문, 본체 문구(`dag.empty.none_hint`), 갤러리 예문은 모두 추천 표기 `--workspace 1` 을 쓴다. 카탈로그 등록은 Hint text Section 의 spec 하나가 세 Spec 을 이어 그린다.

## warning-callout (Components)

디자인 `ui_kits/terminal/overlays/settings_window.jsx:623-632` (Settings › Terminal ›
TUI 섹션의 OSC 52 경고 박스) ↔ 위젯 `crates/tasty-ui-widgets/src/warning_callout.rs::warning_callout`
↔ 갤러리 `catalog/widgets/warning_callout.rs::draw` (Components › `Warning callout`).
플레인 경고 텍스트(`accent-warning` + `.small()`)를 대체하는, 아이콘 + caption 을
보더 + 틴트 배경으로 감싼 bordered callout.

| 디자인 jsx (css) | 토큰 | 위젯/갤러리 |
|---|---|---|
| `border: 1px solid color-mix(accent-warning 40%, transparent)` | `accent-warning`.gamma_multiply(0.4) + `border_width` | `warning_callout` stroke |
| `background: color-mix(accent-warning 12%, transparent)` | `accent-warning`.gamma_multiply(0.12) | `warning_callout` fill |
| 라운드 박스 | `corner_radius` | Frame corner_radius |
| padding | `spacing_md`(x) / `spacing_sm`(y) | Frame inner_margin |
| 삼각 경고 아이콘 | `icon_glyph_size_sm` + `accent-warning` | `IconPainter` 주입(`ALERT_TRIANGLE`) |
| 본문 문구 | `font_size_caption` + `text-secondary` | wrapping `Label` |

아이콘은 crate 경계상 위젯이 직접 못 그린다 → `IconPainter` 클로저로 외부 주입(본체
`icons::ALERT_TRIANGLE`, 갤러리 `catalog::icons::ALERT_TRIANGLE`). color-mix 는
`gamma_multiply` 알파 감쇠 근사(chip/banner 전례). 카탈로그 등록은 Components 페이지
Hint text Section 바로 다음에 `Warning callout` Section 1개.

## clipboard-viewer (Plugins)

디자인 `ui_kits/terminal/overlays/clipboard_viewer.jsx` ↔ plugin
`crates/tasty-plugin-clipboard-viewer/src/view.rs::draw`(egui-mesh 자가 렌더, B4) ↔ 갤러리
`catalog/components/clipboard_viewer.rs` (Plugins › `Clipboard viewer popup`). 좌측 rail
master-detail 레이아웃은 폐기됐다 — header→type-bar→body→footer 4단 수직 스택으로
구조 전사. 갤러리는 plugin crate 비의존이라 그 *구성*을 Theme 토큰 painter mock 으로
전사 — 픽셀 동일성 비목표.

| plugin view.rs | 토큰 | 갤러리 함수 |
|---|---|---|
| header(아이콘+타이틀+snapshot 뱃지+close) | `text-muted`/`font-size-max`/`tag` Default | `header_row` |
| type-bar(≤1: 뱃지, ≥2: 세그먼트) | `bg-sidebar` 행 + `tag` Accent(≤1) / `border-default`+`accent-primary`(≥2) | `type_bar_row`(text) / `type_bar_segmented_row`(Text/Files) / `type_bar_compact_row`(다섯 타입) / `image_type_bar_row`(image, 우측에 meta 텍스트) / `type_bar_row_html`(우측 Pretty print 체크박스) / `other_type_bar_row`(Other 뱃지) |
| body well(text/html) | `bg-app` fill + `separator`+`border-width` + `corner-radius`, mono 스크롤 | `body_row` / `body_row_text`(임의 문자열) |
| body well(files) | 위와 동일 + 아이콘(`text-muted`)+mono 경로 한 줄씩 | `files_body_row` |
| body well(image — 인라인 렌더 없음) | 위와 동일 fill/border, 콘텐츠는 중앙 정렬(아이콘 30px 고정 + `text-muted` + mono caption 메타 + `text-disabled` italic 안내) | `image_body_row` |
| body well(other) | 위와 동일 fill/border, 포맷 블록마다 이름(`text-secondary` 굵게)+크기(`text-muted`) 같은 줄 + 미리보기(`text-primary`), 블록 사이 `separator` 1px | `other_body_row` |
| footer(mime+Close) | `font-size-caption` mono + `Button` Secondary mock | `footer_row(ui, theme, mime)`(text · files · image `image/rgba8` · other — 호출부가 mime 자리 문구를 넘긴다) / `footer_row_html`(`{mime} · {meta}`) |
| CenterState(empty/read-failed/already-open) | 아이콘(28px) + `font-size-body` 굵은 타이틀 + `font-size-term-sm` 옅은 부제 | `center_popup` |

화면 전용 고정값 480×360 은 용도를 명시한 모듈 상수. 10 상태(data-text/data-files/
compact/image/html-raw/html-pretty/other/empty/read-failed/already-open) 를
`StageVariant::Column` 으로 노출.

**세그먼트(≥2)는 공용 view 다.** plugin `type_switch` 와 갤러리 `type_bar_segmented_row`·`type_bar_compact_row`
는 같은 `tasty_ui_widgets::draw_type_segments`(`crates/tasty-ui-widgets/src/clipboard_viewer.rs`)를 부른다. 세그먼트
크기·간격·구분선·채움·글자색, `SEG_COMPACT_AT` 과 `seg_shows_label` 규칙이 그 함수 하나에 있다. 호출자는
라벨·툴팁 목록과 아이콘 그리기만 넘긴다 — plugin 은 baked 아이콘, 갤러리는 SVG 글리프. 타입 순서와 아이콘
짝(갤러리 `COMPACT_TYPES`)은 갤러리 데이터로 남는다.

**압축 세그먼트**는 지원하는 5개 타입을 모두 표시할 때 사용한다. `SEG_COMPACT_AT` 은 5 이고 `ClipboardType`
도 다섯(Text/Files/Image/Html/Other)이며 `read_available()` 이 다섯 리더의 결과를 이어 붙이므로,
다섯이 동시에 살아 있으면 그대로 compact 다 — 브라우저 복사가 text·html·image 를 한 번에 올리는
흔한 출발점이다. `compact` specimen 이 그 상태를 보인다.

## git-viewer (Plugins)

디자인 `ui_kits/terminal/overlays/git_viewer.jsx` ↔ plugin `crates/tasty-plugin-git-viewer/src/render.rs`
(egui-mesh 자가 렌더) ↔ 갤러리 `catalog/components/git_viewer.rs` (Plugins › `Git worktree viewer
popup`). git-viewer 팝업은 UiNode tree 가 아니라 **egui-mesh** 로 그린다(docs/dev-guide/egui-mesh-channel.md의 egui-mesh 렌더링) — plugin 이
자기 egui Context 에서 새 디자인을 직접 페인트하고 host 는 셸(scrim/border/Esc/outside-click)만
소유한다. 갤러리는 plugin crate 비의존이라 같은 구성을 Theme 토큰 mock 으로 전사한다. [갤러리 정책](../policies/gallery-completeness.md)에 따라 예제를 유지한다. 토큰·구조 정합 목표, 픽셀 동일성 비목표.

| 디자인(jsx) | plugin render.rs | 갤러리 함수 |
|---|---|---|
| `Header`(Git + `Refresh` secondary) | `header` | `header` |
| context strip(worktree · branch · oid pill · path) | `context_strip` | `context_strip` |
| `PaneHead`(uppercase 섹션 strip + count) | `pane_head` | `section_head` |
| `WtRow`(2줄: name+type pill / oid+state pill) | `wt_row` | `wt_row` |
| `ChRow`(status pill + dir/file) | `ch_row` | `ch_row` |
| `CmRow`(oid + refs + summary + author + time) | `cm_row` | `cm_row` |
| `DiffLine`(거터 + 부호 + ± tint / hunk band) | `diff_line`(+`draw_diff` well) | `diff_line`(+`diff_pane`) |
| oid·refs·`main`·hunk = sky | `accent_info` (Tag `Info` 톤) | 동일 |
| current·added·`+` / locked·modified / invalid·deleted·`-` | `accent_success`/`-warning`/`-danger` | 동일 |

`normal`(rail \| Changes/Commits) / `diff` 두 cluster(`StageVariant::Column`)로 하단 pane 의
Commits↔Diff 교체를 함께 노출. Tag `Info`(sky) 톤은 `tasty-ui-widgets` `chip.rs` 에 추가되어
host gallery Tag specimen(prim_chips)에도 노출된다.

## surface viewers (Plugins)

egui-mesh surface(`image`) + webview surface/chrome(`markdown`/`html`) 의 Plugins 페이지
specimen 묶음(각 surface 가 독립 Section). plugin crate 비의존 — plugin render 경로의 토큰·구성만
painter/egui 로 전사. markdown 은 [WebView 호스트 계약](webview.md)로
Stage B 부터 image 와 다른 채널(webview)로 이동했지만, html 과 달리 (콘텐츠가 없는 chrome-only
specimen 이 아니라) 실제 CSS 출력 내용까지 손으로 전사한다 — plugin 이 아직 host chrome 을 얹지
않는 대신 문서 자체(주소창 포함)를 통째로 생성하기 때문.

| surface | plugin draw | 갤러리 specimen | 핵심 토큰 |
|---|---|---|---|
| markdown | `crates/tasty-plugin-markdown/src/render.rs` (`pulldown-cmark` → `ammonia` sanitize → CSS custom property 주입, native OS WebView 가 렌더) | `components/markdown_viewer.rs`. 본문 색 대응(`--md-*` → 토큰, highlight.js 역할 → 팔레트)과 제목 단계는 `markdown_viewer/css_path.rs`의 `markdown-content-colour` · `markdown-heading-hierarchy` spec | 문서 바탕 `surface-bed-focused`·`surface-bed-unfocused` · 표 row·zebra는 `md-table-row-bg*`와 `-unfocused` 짝 · 본문 `text-secondary`(=override subtext1) · 링크 표본 `accent-primary-raised` · 코드 `surface-raised` · 헤딩 `font-size-prose-h1`(h1)↔`font-size-body`(h6) CSS 5단계 선형보간(`prose-h2`·`line-height-prose` 은퇴 유지 — CSS custom property `--md-h1`..`--md-h6` 로 대체) |
| image | `crates/tasty-plugin-image/src/render.rs` | `components/image_viewer.rs` (`image-viewer` spec, 편집 모드는 `image-paint` spec) | 캔버스 표본 `surface-bed-focused`·`surface-bed-unfocused` · 상태 칸 `bg-sidebar` · 도구 모음·되돌리기/다시 실행·zoom +/−·찾아보기 아이콘 버튼 `IconButton` sm(ghost, disabled는 잉크만) · 편집 바 Save `Button` Secondary·Cancel Ghost · zoom Fit `Button` Secondary · New Image·Save As 확인 `Button` Primary·Cancel Ghost · 파일명·zoom `text-muted` · zoom % mono `image-zoom-font-size`(11) · 최소 폭 `image-zoom-min-width`(40) · fallback `IMAGE` glyph · floating selection·손잡이 `accent-primary`, 손잡이 한 변 `image-handle-size`(6, 변 위 가운데) · 기본 붓 색 `accent-danger` · 붓 슬라이더 트랙 `surface-active` · 팝업 카드 `image-popup-width`(300) · `image-popup-pad-top`/`-pad-x`/`-gap`(12/14/10) · 제목 `image-popup-title-font-size`(14, semibold 는 egui 글꼴이 없어 보통 굵기) · 버튼·Width 줄 간격 `image-popup-btn-gap` · 크기 입력 `image-size-input-width`(64, 공용 `Input`) · 경로 줄 간격 `image-path-row-gap`(6) · 팝업 `shadow-modal` |
| explorer | 본체 `src/adapters/ui/surface/explorer.rs`의 `draw_explorer` (plugin 아님) | `components/explorer_surface.rs` (`explorer-surface` spec). 탭 줄 `explorer_tab_bar::strip` · 보기 전환 `explorer_toolbar::seg_toggle` · 사이드바 `explorer_sidebar::two_region` · 상세 표 `explorer_view_cells::detail_table` 를 760×420 한 장으로 조립. 탭 줄·Up 글리프·보기 전환·상세 행은 본체 값을 그리며, 시안 값과의 차이는 Spec Note에 적는다. 보기 전환이 세그먼트 채움을 따른다는 결론은 `components/explorer_view_toggle.rs` (`explorer-view-toggle` spec — Mocha·Latte 상자마다 켜짐 · 꺼진 칸 hover · grid 켜짐 세 줄). 내용 영역 상태 화면은 `components/explorer_states.rs` (`explorer-states` spec — 빈 폴더 · 권한 거부(`accent-warning`) · 불러오는 중(Spinner) 세 칸과 `explorer_states/popups.rs`의 `favorite` · `rename` 두 팝업. 팝업은 시안 Spec 무대의 구성(Path·Name 라벨, Kbd 안내 줄, 확장자 보존 줄, sm 버튼, 여백 12/14/10)을 따르며 본체를 따르는 단독 팝업 예제와 따로 그린다) | 면 `bg-panel` · 파일 영역 표본 `surface-bed-focused`·`surface-bed-unfocused` · 탭 줄·사이드바 `bg-sidebar` · 보기 전환 `segtoggle-on-bg` · 선택 행 `surface-active` · 즐겨찾기 별 `accent-warning` |
| html | OS native WebView overlay (`src/runtime/surface_registry/webview_kind.rs`) | `components/html_chrome.rs` | 콘텐츠 토큰 무관 — chrome 만: 포커스는 위쪽 2px 띠(`surface-focus-edge-*`) · `bg-panel`/`border-default` 빈 경계 타일(글리프·라벨·URL 없음) · placeholder `HTML` glyph · `Spinner` 로딩 · `ALERT_CIRCLE`+`accent-danger-raised` 에러 |

glyph: `crates/tasty-icons` 의 `IMAGE`(image fallback) · `HTML`(webview) — 갤러리 아이콘 페이지 SURFACES 그룹에 전시. image 는
`viewer`/`no-image` 2 cluster, image 편집 모드(`image-paint`)는 시안 Spec 무대(paint bar · floating selection · New Image · Save As 팝업)를 공용 `Button`·`IconButton`으로 전사한 Wrap 무대, html 은 `boundary`/`placeholder`/`loading`/`error` 4 cluster,
markdown 주소 띠는 시안 `gallery/plugins/markdown-address.jsx` 의 네 Spec 을 `components/md_address.rs` 의 `markdown-address-bar`(Mocha·Latte 띠 상태 일곱 장: 로컬 편집+최근 목록, 로컬·원격 idle, 원격 편집·stale·끊김·이동 실패) · `markdown-address-folder`(폴더 버튼 상태와 세 언어 tooltip) · `markdown-address-failures`(원격 이동 실패 종류별 문장 다섯 가지와 30초 대기 행, en 과 영어 외 내장 언어 전부) · `markdown-address-narrow`(좁은 폭에서 Go 숨김, 필드 하한) 로 옮긴다. 쉬는 경로 글자는 `md-address-value-fg`, 대기 행 글자는 `md-address-pending-fg` 다. 본체 `render/address.rs` 와 같은 `md-address-*`·`input-*`·`icon-button-*` 토큰을 쓰고, 버튼은 공용 `IconButton` sm, 필드는 공용 `Input`(테두리 고리는 egui 위젯 규칙)이다. 플러그인 문구는 본체 번역표에 없어 시안 문구를 상수로 둔다.
markdown 본 spec 은 Column · Solo · Wrap 세 무대이고, 제목 단계 견본은 `markdown-heading-hierarchy` spec 의 Column 무대에 있다. 화면 전용 고정값(560/360/300, control 버튼 24×20/30×20)은
용도를 명시한 모듈 상수.

<a id="misc--scripts-lua-script-manager--05-docs/features/file-handler/index.md"></a>

<a id="misc--scripts-lua-script-manager--05"></a>

## Lua 스크립트 관리

설정 modal Misc 탭 › Scripts 관리 창. 디자인: `ui_kits/terminal/overlays/settings_window.jsx`
(`ScriptManager`/`ScriptRow`/`ScriptPath`/`ScriptChangedBadge`). 갤러리 미러:
`gallery/parts/overlay-frames.jsx` `ScriptManagerFrame({empty})`.

| 디자인 컴포넌트 | 본체 draw | 갤러리 specimen | 핵심 토큰 |
|---|---|---|---|
| `ScriptManager` (헤더+add card+list/empty) | `view/settings/ui/tabs/misc.rs::draw_scripts_subtab` | `catalog/components/script_manager.rs::draw`(list와 empty를 한 Spec에 나란히, `frame(empty)`) | 제목 `font-size-max`/semibold · 설명 caption `text-muted`/`measure-md` |
| `ScriptRow` (glyph/name/path/kbd/actions) | `draw_script_row` | specimen 내 `Row` | 행 하단 `separator` 보더 · name 13/600 `text-primary` |
| `ScriptChangedBadge` | inline | inline | `accent-warning` color-mix(40% border/12% bg) · `font-label` `font-size-micro`(10) + warn glyph 12 |
| `TriggerRow` (Auto-run) | `draw_trigger_row` / `trigger_chip` | `trigger_row` / `trigger_chip` | "Auto-run:" caption `text-muted` · 칩 높이 16 · 안쪽 여백 0 `space-xs` · `border-default` · mono `font-size-micro` `text-secondary` + close 12 `text-muted` · Add trigger… 는 본체와 갤러리 모두 공용 `script_trigger_add_control`(`crates/tasty-ui-widgets/src/script_trigger.rs`): 점선 `border-default`(`border_dash` 4 / `border_dash_gap` 4, 곧은 변만, 모서리 실선) + chevronDown 12, `text-muted`. 메뉴가 열려 있으면 `overlay-active` 채움, 모든 이벤트가 걸려 있으면 숨기지 않고 disabled(`state-disabled-fg`, 툴팁 `settings.scripts.trigger_all_bound`). 남은 이벤트 메뉴는 `script_trigger_menu` — 폭 하한 `trigger_menu_min_width()` 200, `trigger_menu_max_height()` 220 넘으면 스크롤, 행은 `menu_item_height()` · 좌우 `space-sm` · mono micro `text-secondary`(hover `overlay-hover` · `text-primary`). 본체는 `popup_below_widget` + `with_popover_frame` 안에, 갤러리는 펼친 모습을 `script_trigger_menu_frame`(menu-bg · menu-border · menu-radius · shadow-popover · `popup_content_margin()`) 안에 그린다 |
| `ScriptPath` (중간생략) | `draw_script_path` | inline `Path` | dir=`text-muted` ellipsis-first / file=`text-secondary` full · mono caption 11 |
| Add card | inline | (list variant만) | `surface-raised` bg + `border-default` + `radius` · 라벨폭 100 · row `settings-row-min-height` |
| Empty state | `draw_empty` → 공용 `CenterState` | `frame(empty)` (같은 위젯) | `SCRIPT` 글리프 `center-state-glyph-size`(24) + 제목 body 13 `center-state-title-fg` + 보조 caption `center-state-sub-fg` · `center-state-max-width`(300) 줄바꿈, 자연 높이 |

**전사 스펙 (jsx inline style → LogicalPx / Theme)**:
- ScriptRow: `align-items:flex-start`, `gap: space-md`(12), `padding: space-sm space-xs`(8/4), 하단 `1px separator`. glyph 16 `text-muted` `margin-top:2`. 중앙 flex1 `min-width:0` col `gap:2`. 우측 `flex:none` `gap: space-sm`(8).
- 우측: 바운드=`Kbd`, 미바운드=이탤릭 "Unbound" `text-disabled`(overlay1) 12. IconButton sm ×3(bind kbd 16 / edit 16 / trash 16).
- rename: inline Input + Save(primary sm)/Cancel(ghost sm), Enter=commit/Esc=cancel.
- remove: inline "Remove?" `text-secondary` 12 + Cancel(ghost sm)/Remove(secondary sm, `accent-danger` 톤).
- 헤더: 좌 "Scripts" `font-size-max` semibold + muted 설명(`measure-md`/`line-height-ui`), 우 "Add script"(secondary sm, plus leadingIcon).

**glyph** (`crates/tasty-icons`): `SCRIPT`(file+lines: `M14 3v4a1 1 0 0 0 1 1h4` / `M17 21H7a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h7l5 5v11a2 2 0 0 1-2 2z` / `M9 13h4M9 17h6`), `KEYBOARD`(`rect x2 y6 w20 h12 rx2` + `M6 10h.01…M8 14h8`). 기존 재사용: PLUS/EDIT/TRASH/FOLDER/ALERT_TRIANGLE.
**신규 토큰 없음** — `script_manager.rs`/`draw_scripts_subtab`가 쓰는 토큰은 전부 기존
`spacing_*`/`font_size_*`/`text_*`/`accent_warning`/`border_default` 등 범용 접근자다.
i18n 12키(`settings.misc.scripts` · `settings.scripts.{description,add,file,display_name,browse,unbound,changed_badge,changed_help,empty_title,empty_body,remove_confirm}`).

## Settings › Keybindings › Preset drill-down (settings-preset-drilldown)

디자인 `ui_kits/terminal/overlays/settings_window.jsx` `PresetSubtab`/`PresetDiffTable` ↔ 본체
`src/view/settings/ui/keybindings_tab/preset.rs`. 구 좌(120px)/우 split 을
공용 위젯 [`DrillDown`+`ListCtrl`](#primitive-컴포넌트-레이어-components) 소비로 재작성 —
두 위젯의 첫 본체 소비처.

| 디자인 jsx | 본체 | 비고 |
|---|---|---|
| `PresetSubtab` (DrillDown 루트, `view` controlled) | `draw_preset_subtab` | 뷰 상태 = `selected_preset: Option<String>` (None=List / Some=Detail) |
| list wrapper (`padding: space-md space-lg`, gap space-sm) | list 클로저 `Frame::inner_margin(symmetric(lg, md))` | 인트로 `<p>`(12/muted/measure-md) = `intro_note` |
| `<ListCtrl items selectedId={activeId}>` | `ListCtrl::show(..., active_idx)` | Active(사용 중) = draft 와 전 일반 바인딩 일치 프리셋. trailing `Tag`(success·dot) "Active" |
| back bar `actions` = Apply(primary sm, disabled=Applied) | `DrillDownActions` 클로저 + `Button` | 클릭 신호는 `Cell` 로 회수 (`&dyn Fn` 불변 계약) |
| `PresetDiffTable` (grid `minmax(0,1.6fr) 1fr 1fr`) | `draw_preset_diff_table` (수동 갤리 페인트) | 헤더 `font-label` micro(10) uppercase muted + separator 헤어라인. 셀 padding space-sm/space-md. Action=body(13) text-secondary, 현재 바인딩 열=mono caption(11) muted, 바뀔 바인딩 열=mono term-sm(12), 변경=`accent-primary`(색상만, bold 없음) |
| `fullBleed` (Keybindings›Preset 만 표준 래퍼 우회) | `ui.rs` content 디스패치 `full_bleed` 분기 | DrillDown 이 자체 패딩+내부 스크롤 소유 |

헤더에는 별도 close ✕를 두지 않는다. 하단 Cancel과 OS 타이틀바로 닫으며, 갤러리 `components/settings.rs`의 L1 밴드도 같다.
**신규 토큰 없음** (위젯 토큰은 [design-token-mapping §drilldown/listctrl](design-token-mapping.md) 참조).
i18n: `settings.keybindings.preset_*` 신규 10키 + `select_preset_label`/`preset_col_before` 문구 갱신,
`preset_col_after` 제거 (3열 헤더 = 프리셋 이름).

## Settings › Keybindings › Plugins (kbplugins)

디자인 `ui_kits/terminal/overlays/kb_plugins_subtab.jsx` + `gallery/overlays-windows/index.jsx` Spec
"Keybindings › Plugins" + `gallery/overlays-windows/kb-plugin-records.jsx` Spec "Keybindings › Plugins — Custom = record slots"
↔ 갤러리 `catalog/components/kb_plugins.rs`(Overlays › `kbplugins`, 견본 다섯:
기본 · 초안과 해석 실패(OS 키 이름 포함) · 녹화 중 · 좁은 폭 460 줄바꿈 · OS 예약 조합(Windows 표기) · 빈 상태). 화면은 공용 view
`tasty_ui_widgets::kb_plugins_subtab`(`crates/tasty-ui-widgets/src/kb_plugins.rs`)이 그리고 본체 wrapper
`src/view/settings/ui/keybindings_tab/plugins.rs` 의 `draw_plugins_subtab` 과 갤러리가 함께 부른다. 본체 wrapper 는 초안과
저장된 override 를 표시값으로 풀고 결과를 `plugin_shortcuts_draft` 에 쓴다. 갤러리는 시안
`KBP_PLUGINS`·`KBP_RESOLVED` 고정 데이터와 시안 초안 규칙(mode 전환 시 시작값, Reset 이 override 를 지움)으로
props 를 채운다.

| 디자인 jsx | 공용 view | 비고 |
|---|---|---|
| `KbPluginsSubtab` | `kb_plugins_subtab` | picker 행 · `kb-plugin-list-gap` · 명령 행. 플러그인이 없으면 muted caption 한 줄만 두고 picker 는 없다 |
| picker 행 | `row_grid` + `select_rect` | 제목 열 `kb-plugin-title-width` · `kb-plugin-title-gap` · Select `kb-plugin-picker-width`. 행 최소 높이 `kb-plugin-row-min-height` 안에서 세로 가운데 |
| `KbpCommandRow` | `command_row` | 위아래 `kb-plugin-row-padding-y`, 명령 사이에만 1px `kb-plugin-separator`. 제목은 text-secondary 본문 크기로 열 안에서 줄바꿈하고 말줄임하지 않는다 |
| 컨트롤 줄 | `control_line` | mode Select `kb-plugin-mode-width` · `kb-plugin-control-gap` · slot `kb-plugin-slot-width` · Reset(ghost md). 셋 모두 `kb-plugin-control-height`, 줄 최소 높이 `kb-plugin-row-min-height`. 폭이 모자라면 한 흐름으로 줄을 바꾼다 — 다음 줄은 mode Select 의 x 에서 시작하고 앞 줄 컨트롤 아래 `space-xs`, Reset 은 늘 마지막이다. 녹화 슬롯 묶음은 키 수만큼 넓어져 Reset 을 오른쪽으로 민다 |
| slot | `KbPluginSlot::{Inherit, Custom, Unassigned}` | 상속 소스 Select / 녹화 슬롯 줄 / "(Unassigned)" `kb-plugin-none-fg` |
| `recordAlt` | `KbPluginSlot::Custom` → `record_slots` | 사용자 결정으로 Custom 은 녹화 방식이다. 시안의 slot 폭 Secondary 버튼 대신 다른 단축키 서브탭과 같은 `kb_record_slot_sized`(`kb-record-width`·`kb-record-add-width`, 슬롯 사이 `space-xs`, 키마다 슬롯 + 추가 슬롯, 키가 없으면 None 슬롯)을 쓴다. 높이는 줄의 다른 컨트롤과 같은 `kb-plugin-record-height`(→ `kb-plugin-control-height`)다 |
| caption | `caption` | 줄 아래 `kb-plugin-caption-gap`, caption 크기. Inherit 은 `kb-plugin-caption-fg` "Inherited (…)" 또는 "None", 해석 실패는 `kb-plugin-error-fg` 에 키를 mono 로, OS 키 이름은 `kb-plugin-error-fg` 의 `keys.os_key_name`(저장 토큰은 code run), OS 예약 조합은 `alertTriangle` `icon-glyph-size-xs` · `space-xs` · caption, 둘 다 `kb-os-reserved-fg` |
| 해석 실패 슬롯 | `KbRecordSlot::Invalid` | 설정 원문을 그대로 보이고 테두리만 `kb-plugin-error-fg`. 호버해도 테두리를 바꾸지 않는다 |
| 초안 점 | `title_cell` | 제목 뒤 `space-sm`, `kb-plugin-draft-dot-size` 원 `kb-plugin-draft-dot`. hover tooltip |
| Reset `disabled={!overridden}` | `Button::enabled(overridden)` | override 가 없으면 disabled. tooltip 은 켜져 있을 때만(시안 disabled 버튼은 `title` 이 뜨지 않는다) |

키 해석 실패 판정은 시안 `kbpParse` 를 옮기지 않고 키 매칭 규칙(`tasty_key_match::binding_key_recognized`)을 쓴다.
시안은 `cmd`·`super` modifier 도 받지만 매칭 규칙에 그 토큰이 없어 해석 실패로 보인다. `f13`~`f24` 는 매칭 규칙이 받는다.

시안 `overlays-windows-b12.jsx` 의 "Win · Super as option" 중 OS 별 표기(조합 슬롯 · 키캡 낱말)는 갤러리
Icons › `keys-in-use`(`catalog/icons_keys.rs`)의 "option per OS" 묶음에 있다. macOS 줄은 시안 기호 대신
글자 표시 스타일로 적는다(기호는 벡터 글리프로만 그린다). 같은 시안의 예약 경고·OS 키 이름 caption 은 위 kbplugins 견본에 있다.

## Settings › Keybindings › Import / Export (kbimportexport)

디자인 `ui_kits/terminal/overlays/kb_import_export.jsx` + `settings_window.jsx`(`KB_L2_SEPARATED` ·
창 자체 toast) + `gallery/overlays-windows/index.jsx` Section `kbimportexport` ↔ 갤러리
`catalog/components/kb_import_export.rs`(Overlays › `kbimportexport` 섹션, Spec 4 종 — 하위 모듈은 본체와 같은 이름 `entry` · `diff_table` · `notices` · `paint`, 갤러리의 추가 상태 예제인 `open_values` 와 나머지 상태 예제인 `remaining_values`). 갤러리는
본체 미의존이라 같은 위젯·토큰으로 미러한다. 본체는 `src/view/settings/ui/keybindings_tab/import_export.rs`
와 그 하위 모듈(`import_export/` 의 `entry.rs` · `diff_table.rs` · `notices.rs` · `paint.rs`, 계산은 `labels.rs` · `view_model.rs` · `model.rs` · `bundle_notices.rs`.
`action_row` · `diff_table` · `group_header` · `action_cell` ·
`dropped_notice` · `parse_failure` 는 갤러리와 같은 이름. 갤러리의 `entry`·`detail_frame`·`notices`
자리는 `draw_import_export_subtab` 한 함수가 `DrillDown` 으로 짠다)와 `src/view/settings/ui.rs` 의 `l2_separator` 다.

| 디자인 jsx | 갤러리 | 비고 |
|---|---|---|
| `IeL2Tail` · `KB_L2_SEPARATED` | `l2_tail` · `l2_separator` · `l2_row` | **신규 축** — L2 행 위 1px separator(margin space-sm), 필터 활성 시 숨김 |
| `IeActionRow` ×2 (`IeEntry`) | `entry` · `action_row` | surface-raised + border-default + radius, padding `kb-ie-notice-inset`(→ space-md, 가로·세로 모두 12). Import primary · Export secondary. `notice` 자리(행 아래, gap space-sm) + trailing 버튼 비활성 축 |
| 창 toast(export 경로) | `export_toast`(`toast_card::draw_single_card`, Success) | 설정 창 자체 `ToastManager` 의 카드 |
| `DrillDown` detail + back bar actions | `detail_frame`(실제 `DrillDown`) | 우측 슬롯: `Show all {n}`/`Changed only` ghost · Apply primary |
| `IeDiffTable` (grid `size-32 minmax(0,1.6fr) 1fr 1fr`) | `diff_table` | **신규 축 둘** — 선두 선택 열(32) · 그룹 헤더 행(surface-raised, select-all · chevron · `font-label` micro caps 그룹명 · `N changed · M total`). 변경 = accent-primary |
| plugin 행 부제 · quick-switch 축 부제 | `action_cell` | agent 점 + mono micro plugin 이름 / micro 슬롯 수 |
| `IeNotices` | `dropped_notice` · `parse_failure` | 버린 plugin = helpCircle muted 정보 줄(경고 아님) · 파싱 실패 = 알림 블록(danger) + "Choose another file" |
| `IeBlockG` | `notice_block` | **공통 알림 블록** — tone 12% 채움 · 35% 테두리 · 헤더(glyph 16 · 제목 13 tone · 우측 mono caption 개수) · 본문 12 text-secondary · 액션 행(gap space-sm). 파싱 실패 · 내보내기 실패(danger) · 번들 경고(warning)가 모두 이것이다 |
| `IeExportFailG` · `IeBundleNoticesG` · `IeParseFailG` | `open_values`(Spec 3) · `export_failure_row` · `bundle_notices` · `notice_line` | 내보내기 실패 = Export 행 안 danger 블록(Try again secondary · Choose another location… ghost, 행 버튼 비활성) · 번들 경고 = warning 블록 하나(`{n} notices`, `·` 글머리 줄, 3 줄 뒤 `Show {n} more`) · 줄 번호 없는 파싱 실패. 본체는 `notices::{export_failure, bundle_notices}` · `bundle_notices.rs`(순서·접기) |
| `IeExportFailG reason="other"` · `IeBundleNoticesG one` | `remaining_values`(Spec 4) · `unknown_export_failure` · `os_reason_line` · `one_notice` | 사유를 모르는 내보내기 실패 — 가운데 구절은 고정 집합의 catch-all("the write didn't finish.") 이고 OS 가 낸 문장은 **문장 안에 안 들어간다**: 본문 아래 제 줄(mono caption · muted · 한 줄 말줄임 · 전문은 tooltip). 알림이 하나뿐이면 헤더와 그 줄이 모두 단수형이고 접기 링크가 없다. 본체는 `notices::{export_failure, os_reason_line, bundle_notices}` · `import_export::ExportFailReason` · `view_model::notice_line` |

**전사 노트**:
- `letter-spacing-caps` 는 `font-label` `font-size-micro` uppercase, `fontWeight: 600` 은 색 강조로
  둔다(Preset · Hook Handlers 관례). diff 표 머리글과 그룹 라벨의 자간은 생성 접근자
  `letter_spacing_caps(font_size_micro)` 를 `paint::truncated_tracked` 로 적용한다. `color-mix(tone X%, transparent)` 는 명명 const
  계수의 `gamma_multiply`.
- 그리드 밖 값(chevron gap 6 · plugin 점 gap 5)은 가까운 값으로 바꾸지 않고 용도를 명시한 상수로 둔다([테마 시스템 (운영 상세)](theme.md)).
- 선택 열 32 는 본체와 갤러리가 모두 Theme 의 `kb_ie_select_column_width` 를 읽는다. 나머지 `kb-ie-*` 토큰
  (`kb-ie-action-column-width` · `kb-ie-from-column-width` · `kb-ie-slot-min-width` · `kb-ie-notice-inset` ·
  `kb-ie-slot-height`)은 이 화면이 읽지 않는다. 그중 slot 둘은 녹화 슬롯의 `kb-record-*` 가 참조한다.
  `kb-ie-action-column-width` · `kb-ie-from-column-width` 는 옵션 이전 카드가 사라져 디자인이 deprecated 로
  표시했다. 읽는 곳이 없지만 디자인이 지울 때까지 토큰과 생성 접근자를 그대로 둔다.
- Import/Export 는 full-bleed 서브탭이라 620 상한을 받지 않는다. specimen 폭은 시안 `IE_W` 와 같은 868
  (기본 1100 창의 본체 설정 창 콘텐츠 컬럼)이다. 진입 화면 컬럼만 620 을 따른다.

<a id="settings--handler-탭-서브탭-콘텐츠-s13"></a>

## Settings › Handler 하위 탭

L1 "File Handler" 를 **Handler** 로 일반화(내부 key `FileHandler` 유지)하고 Hook Handlers
서브탭을 추가한 개편. 디자인: `ui_kits/terminal/overlays/settings_window.jsx`
(`L1_LABEL`·`L2.FileHandler`·`HookHandlers`/`HookRow`/`SEED_HOOKS`/`HOOK_ORIGIN`).

| 디자인 컴포넌트 | 본체 draw | 갤러리 specimen | 핵심 토큰 |
|---|---|---|---|
| `ExtensionMapping` (File Extension Mapping) | `view/settings/ui/file_handler_tab/extension_mapping.rs` | `catalog/components/settings_handler.rs::draw_extension_mapping` (Mocha·Latte 짝 여섯 — 빈 입력·`.toml` · custom+missing · 긴 번역 문구 · Save 전 대기 · 대기+긴 번역 문구, 시안처럼 세 Stage, 패널 바깥 폭 `--tasty-size-360`) | 위 mono `Input` + Add(`Button` secondary sm — 입력이 비었거나 그 확장자를 지원하는 detector가 없으면 disabled, Enter도 추가) · 확장자 머리줄(본체 `group_header`, 갤러리 `ext_group_header`): 위·아래 `space-xs`, 안쪽 최소 높이 `button-height-sm`, `.ext` mono `font-size-caption` `text-secondary`(미설치면 `text-disabled` + `tag_disabled` "not installed" + 아래 `separator`) · 오른쪽 끝 ghost `Button` sm — 사용자 순서가 있으면 Reset(툴팁), 미설치면 Remove · Save 전 대기(시안 `pendingRemove`·`pendingReset`): 누른 버튼 자리에 Undo, `tag_disabled` "reset on save"/"removed on save", Remove 대기는 `.ext` 취소선(색 `text-disabled` 유지) — 본체는 `extension_priority_pending` 에 누르기 전 초안 값을 맡겨 Undo 가 그 확장자만 되돌린다 · detector 행: 순번 mono caption `text-muted`(폭 `space-lg`) + 이름 `font-size-body` `text-secondary` + ▲▼ `IconButton` sm `chevronUp`/`chevronDown` · row `settings-row-min-height` + 하단 `separator` · ▲는 맨 위 행, ▼는 마지막 후보에서 disabled(숨기지 않음) · 비후보(꺼짐·미설치) 행은 이름 `text-disabled` + `tag_disabled` "off" + ▲▼ 모두 disabled |
| `FileDetectorsPane` (`ui_kits/terminal/overlays/file_handlers.jsx`) | `view/settings/ui/file_handler_tab/detectors.rs` | `catalog/components/settings_file_handlers.rs::draw_detectors` (Mocha·Latte 짝 — 목록 · 빈 카드 · 검증 오류 · 여러 출처 · 좁은 열 접힘 · 사용자 기여 삭제 · 기존 id patch 네 가지 + patch 대기 · 추가/삭제 대기 · 빈 목록 · 독일어 좁은 열 · 좁은 열 대기 행 셋(markdown·notebook 사용자 기여 삭제, log 사용자 규칙 추가)과 같은 행의 넓은 열) | 공용 `tasty_ui_widgets::fh_*`(`file_handler_form.rs`) — 머리 `fh_intro`(caption `text-muted`, 설명 열 `measure-md`, 출처 불릿 key mono `text-secondary`, Add Button secondary sm plus 글리프 — 설명 열이 `primitive.size-240` 보다 좁아지면 아래로) · 행 `fh_row`(위아래 `space-sm` 좌우 `space-xs`, 1줄 `button-height-sm` · id mono body · 출처 Tag(plugin 은 agent variant) · 대기 기본 Tag · Switch · 끝 칸. 1줄이 좁으면 `fh_fit_step` 순서로 줄인다 — id 가 말줄임될 때만 ① 여러 출처 Tag 를 "N sources" 하나로 접고 ② 대기 Tag 를 1줄 아래 자기 줄(`tag_disabled_truncated`, 말줄임 + 툴팁)로 내린 뒤 ③ id 를 하한 min(`file-handler-id-min-width`, id 폭)까지 말줄임한다. 단계마다 밟을 때의 폭을 행별로 기억해 그 폭으로 돌아와야 되돌린다. Switch · 끝 칸 · `prio` 는 줄지 않는다 Remove/Undo ghost sm 또는 자물쇠 `icon-size-sm` `glyph-dim`, 2줄 mono `font-size-term-sm` `file-handler-summary-fg`, 사용자 조각 취소선 `file-handler-summary-removed-fg`, 하단 `separator`) · 카드 `fh_card`(`file-handler-form-bg` · `file-handler-form-border` · `file-handler-form-padding` · `file-handler-form-row-gap`, 제목 `font-label` micro caps `file-handler-form-label-fg`, Cancel ghost `Ground::Raised`, 카드 안 Input · Select 는 모두 `Ground::Raised`) · 필드 `fh_field`(라벨 `settings-label-width` + `settings-label-gap`, 카드가 라벨+gap+`field-width-lg` 보다 좁으면 아래로) · 아래 줄 `fh_line`(오류 alertCircle `file-handler-form-error-fg` > 안내 info `file-handler-form-info-fg` > caption `file-handler-form-caption-fg`) · 빈 목록 `fh_empty`(`settings-row-min-height`, `file-handler-empty-fg`) |
| `FileHandlersPane` (같은 파일) | `view/settings/ui/file_handler_tab/handlers.rs` | `::draw_file_handlers` (목록 · 실행 순서 tie 장면과 독일어 좁은 열 · 빈 카드 · raised 카드 상태(Input 넷 · Select 셋 · ghost Cancel, 오류 줄) · 검증 오류 · 동작 종류 셋 · 같은 이름 교체 · 대기 · 빈 목록 · 독일어 · 좁은 열) | 위 공용 부품 + `prio N` `font-label` micro `file-handler-prio-fg` · 목록은 실행 순서(`tasty_file_handler::run_order_key` — priority → owner user > plugin > host → id) · Detector `select_or_placeholder_on` 폭 `file-handler-select-width` · Action `select_on` 폭 `file-handler-kind-width` · Priority Input 폭 `file-handler-priority-width` · 오류 줄 `file-handler-form-error-fg`(→ `accent-danger-raised`) |
| `HookHandlers` (intro+add card+list) | `view/settings/ui/file_handler_tab/hook_handlers.rs::draw_hook_handlers` | `::draw_hook_handlers` · 열린 add 카드 Mocha·Latte 고정 예제 `::draw_hook_add_card_raised` | intro caption 11 `text-muted`/`measure-md` · add card `surface-raised`+`border-default`+`radius`, 라벨폭 100, 라벨 `text-secondary-raised` · Input `Ground::Raised` · Cancel ghost `Ground::Raised`(File Handlers 카드와 같은 raised 처리) |
| `HookRow` (2줄 행) | `hook_handlers.rs::draw_hook_row` | specimen 내 `draw_hook_row` | id mono 13/600 `text-primary` · origin `Tag`(`host` · `you` · plugin id = `agent` variant) · `prio N` `font-label` `font-size-micro` · 우측 끝은 user 행이면 휴지통 IconButton, 아니면 **자물쇠 글리프**(`glyph-dim` + tooltip) · 꺼진 행은 불투명도 없이 id만 `text-muted` · 하단 `separator` · Shell cmd 라벨폭 74/`font-size-caption` + mono `Input`(IpcSequence 는 mono `font-size-term-sm` `text-secondary` 한 줄 요약 — 갤러리 `HookOverrideG` 행도 같다) |

**전사 노트**:
- jsx `headStyle`(`font-label` 10 uppercase `letter-spacing-caps`)은 기존 관례(`font-label`
  `font-size-micro` uppercase `text-muted`)로 전사하고 자간은 생성 접근자
  `letter_spacing_caps(font_size_micro)` 로 적용한다.
- **별도 토큰 없음** — `hook_handlers.rs`/`settings_handler.rs`가 쓰는 토큰은 전부 기존
  `spacing_*`/`font_size_*`/`text_*`/`border_*` 등 범용 접근자이며 이 기능 전용으로
  추가된 Theme 필드가 없다. 화면 전용 고정값(라벨폭 74/100, priority step 10)은
  용도를 명시한 모듈 상수.
- **레지스트리 정책에 따른 행 표시**: 제거 버튼은
  user-origin 행만 달고(host/plugin base 는 finalize 가 되살린다), 나머지 행에는 같은
  자리에 자물쇠 글리프가 온다(`glyph-dim` + "Provided by host — can't be removed").
  제거할 수 없는 항목이므로 비활성 버튼 대신 자물쇠로 표시한다. 출처 Tag 는 **모든 행**이 달고 `host` · `you` ·
  **그 plugin 의 id**(mauve `accent-agent`)를 찍는다. `IpcSequence` 행은 인라인 편집
  대신 mono 한 줄 요약(스텝을 `→` 로 이음)이다. intro copy 의 priority 방향은 엔진
  규약(낮을수록 먼저)으로 기술.
- **사용자 patch 가 걸린 host/plugin 행**(시안 `HookOverrideG`, 갤러리
  `settings_hook_override.rs` = `settings-hook-edited-default` Spec, Mocha·Latte): 병합 결과의 owner 는
  patch 를 단 User 이므로 본체는 `HookHandlerRegistry::patched_defaults`(사용자 기여분을 뺀 병합)에서
  원 출처와 기본값을 읽는다. 출처 Tag·자물쇠는 원 출처를 따르고, 출처 Tag 바로 뒤에 기본 `Tag`
  "edited"(툴팁 "Changed in your settings. Updates to the default no longer apply.")를 단다. 둘째 줄에는
  `Edit` 왼쪽(셸 행은 Input 오른쪽)에 Revert(ghost sm, 툴팁 "Go back to the default from {origin}.")가
  온다. 누르면 확장자 연결의 대기 모양을 따른다 — 같은 자리 Undo, 기본 Tag "reverts on save",
  요약·Switch 는 기본값을 보인다. 대기 중에는 Switch(`switch` disabled, 툴팁
  `hook_handlers.pending_locked_tip`)와 `Edit`(`enabled(false)`)을 Undo 나 Save 전까지 잠근다. 그 행의 초안 편집도 함께 버리고, Save 가 `RemoveHook`(patch 삭제)을
  다른 편집보다 먼저 적용한다. Cancel 은 초안을 버리므로 patch 가 그대로 남는다. user 행에는 달지 않는다.
- **`IpcSequence` 행**: 요약 한 줄(줄어드는 항목) · 오른쪽 끝 `Edit`(ghost sm). 한 줄 형식으로
  쓸 수 없는 시퀀스(`sequence_text::format_sequence` 가 거절)는 `Edit` 대신 caption
  "Edit with CLI"(text-muted, 툴팁 = `tasty hook-handler get --id <id>`) + 복사 `IconButton` sm 이다.
  갤러리 Spec 은 시안 `on_webhook` CLI 행을 `ci-notify` 행으로 옮겼다.
- **IpcSequence 문자열 편집기**: 본체 `hook_handlers::draw_seq_editor` 와 갤러리
  `settings_hook_seq.rs`(시안 `HookSeqEditorG` normal · error · empty, Mocha·Latte, 카드 폭
  `--tasty-size-460`)가 같은 `tasty_ui_widgets::sequence_editor` 를 그린다 — `CodeArea`
  (minRows 4) · 오류 한 줄(`alertCircle` `icon-glyph-size-xs` + accent-danger caption 문장 + mono
  caption text-muted serde 원문) 또는 빈 안내(text-muted caption) · 도움말(text-muted caption) ·
  위 `space-xs` 뒤 오른쪽 정렬 Cancel(ghost sm) · Apply(secondary sm, 오류면 disabled). 세로 간격
  `space-xs`, 캡션 줄 높이 `line-height-ui`. 본체는 행 둘째 줄 자리에 편집기를 두고, 갤러리는
  시안처럼 머리줄(`on_webhook` · `ci-notify` mono `font-size-term-sm` · Tag `you`)이 있는 카드에
  둔다. 갤러리는 해석기가 없어 error 상태의 문장·이유·줄(3)을 시안 값으로 고정한다.

## Settings › General › Permissions (macOS)

General L1의 마지막 L2 서브탭이다. 디자인은 원격 킷의
`ui_kits/terminal/overlays/macos_permissions.jsx`(권한 화면)와
`ui_kits/terminal/overlays/info_modal.jsx`(부팅 안내 모달과 모든 안내 메시지의 셸)에 있다.
요청 버튼과 진행 표시는
[macOS 권한 (TCC)](../../features/macos-permissions/index.md)로 생긴
요소다. 본체와 갤러리는 `tasty_ui_widgets`의 같은 함수를 호출한다.

| 디자인 jsx 컴포넌트 | 본체 함수 | 갤러리 항목 |
|---|---|---|
| `MacPermissionsPane`(콘텐츠 컬럼) | `src/view/settings/ui/tabs/macos_permissions.rs::draw_macos_permissions_tab` → `tasty_ui_widgets::mac_permissions` | `components/settings_macos_permissions.rs::draw` (`settings` 섹션 `settings-macos-permissions` spec) |
| `PermRow`(라벨·보조 설명 / 상태 / 행 버튼, 행 사이 1px `border-default` 선) | `mac_permissions`의 `PermRow` 행. 최소 높이 `perm-row-height`, 좁으면 상태와 버튼이 다음 줄 오른쪽으로 내려감 | 동 |
| `PermStatus`(글리프 `icon-size-sm` + 단어, 간격 `perm-status-gap`) | `PermState::{Granted,Missing,Unknown,NotObservable}` → `perm-*-fg` 색과 글리프 check / alertCircle / helpCircle / eyeOff | 동 |
| `Button primary`(Request all permissions) | `Button::Primary/Md`, 요청 중 `enabled(false)` | 동 |
| FDA 행 `Button secondary sm`(Open System Settings) | `Button::Secondary/Sm` → `open_full_disk_access_settings()` | 동(열지 않고 그리기만) |
| `mpNote`(설명 줄) | caption · `line-height-ui` · `text-muted` · `measure-xl`에서 줄바꿈. 요청 중에는 스피너 + `text-secondary` 진행 문구로 교체 | 동 |
| `Tag`("debug", 손쉬운 사용 행) | debug 빌드에서만 행을 넣음 | E 시나리오 |
| `InfoModalShell`(440 × 140..360, 본문 스크롤, 버튼 줄 위 스크롤 경계) | `src/adapters/ui/info_modal.rs::draw_info_modal` → `tasty_ui_widgets::info_modal` | `components/info_modal.rs::draw` (`info-modal` spec) |
| `PermissionNoticeModal`(강조 표기가 있는 본문) | 같은 셸, `InfoModal.emphasis = true` | `components/info_modal.rs::draw_permissions` (`info-modal-permissions` spec) |
| `PermNoticeBody({ fda })` | `tasty_platform::macos_permission_notice::permission_notice_body`(문단 조립, 본체·갤러리 공용). 서명 문단은 모든 갈래의 맨 끝 | `draw_permissions`의 never 4장 + stale·revoked 각 1장(맨 위) |
| `mpSteps`(stale 번호 2단계 목록) | `info_modal` 번호 항목(문단 머리 `1. `): 본문 크기 · `text-primary` · 들여쓰기 `space-xl` · 항목 간격 `space-xs` | `draw_permission_branches`(`info-modal-permission-branches` spec)와 `draw_permissions`의 stale 장 |
| `mpAside`(서명 보조 문단) | `info_modal` 보조 문단(문단 머리 `> `): caption · `text-muted`, 명령은 caption 크기 code run | `draw_permission_branches`의 never / stale / stale Latte / revoked(끝까지 스크롤) |
| `PERM_SCENARIOS.fdaStale`·`fdaRevoked`·`PermRow sub` | FDA 행 `PermRow.detail` = `fda_settings_detail_key`(Stale → `full_disk_access_stale_detail`, Revoked → `full_disk_access_revoked_detail`). 상태는 Missing 그대로, 칩 없음 | F 시나리오 Mocha·Latte, G 시나리오 |

갤러리는 본체보다 많은 것을 보여준다. 시안의 A~G 조합(아무것도 허용하지 않음, 모두 허용,
Full Disk Access 확인 불가, 업데이트 뒤 잃은 Full Disk Access, 밖에서 꺼진 Full Disk Access, 요청 중, debug 빌드)을 나란히 놓는데, 특히 `Unknown`은 추정에
쓰는 경로가 하나도 없는 macOS에서만 나와 실제 장비에서 재현하기 어렵다. 본체는 그 장비의
TCC 상태 하나만 그린다. 손쉬운 사용 행은 본체에서 debug 빌드에서만
보인다([플러그인 권한 모델](../../dev-guide/plugin-permissions.md)).

안내 모달은 제목을 공용 팝업 타이틀바에 둔다. 시안도 이 모양이다: 채움 `bg-sidebar`,
높이 `control-height`, 한 줄 제목(넘치면 말줄임하고 잘렸을 때만 호버 Tooltip 으로 전체 제목, 위 → 아래 배치), 오른쪽 닫기 ×. 제목은 양쪽에 버튼 예약 폭을 대칭으로 비운 스트립 전체 기준 가운데에 온다(`tasty_ui_widgets::popup_title_text_rect`, 본체와 갤러리 공용). 예약 폭은 버튼 하나면 32, 전체화면 버튼까지 둘이면 60이고, 버튼은 IconButton sm(24 칸, `close`·`fit` 글리프, 호버·누름 표시)으로 그린다. 갤러리 Overlays "Info modal shell" 절의 `popup-title-bar` spec(`components/info_modal.rs::draw_title_bar`)이 시안 `gallery/overlays-dialogs/index.jsx` 의 두 버튼 Spec(알림 popup 머리, 짧은/긴 제목, Mocha·Latte)을 옮긴다. 제목은 모든 popup 에서 `font-size-max` 크기다(`tasty_ui_widgets::popup_title_font`, 본체와 갤러리 공용). 디자인의 semibold 는 굵기를 재현하지 않는다. 이 spec 의 머리는 본체 알림 popup 처럼 `border-frame` 선으로 그린다. 안내 모달만
아래 선에 `info-modal-title-edge`를 쓴다. ×는 dismiss 버튼·Enter·Esc와
같은 동작이고, 바깥 클릭으로는 닫히지 않는다.

디자인이 확정한 근사가 두 가지 있다. 제목과 권한 안내의 도입부·경로 강조는 굵기 없이
`text-primary` 색으로만 본문(`text-secondary`)과 구분한다. egui UI에 굵은 글꼴을 등록하지 않기
때문이며, 디자인은 이 색 근사를 받아들였다. 명령은 [ui-code](#ui-code-components) code run 이다(문단 크기 mono,
`ui-code-bg` 채움, 좌우 `ui-code-padding-x`, `ui-code-radius`). 안내 모달 셸은 `bg-panel` 위라 채움은 `ui-code-bg` 다. `PermRow`의 라벨 줄과 부연 줄 사이는 semantic
`label-detail-gap`(Theme `label_detail_gap`)이다.

## Settings › General › General — 행 격자와 웹훅 외부 수신 행

General L1 의 첫 L2 "General". 디자인: `ui_kits/terminal/overlays/settings_window.jsx`(General/General 의
`Row` · `RowCaption` · `WarnCallout`) · `gallery/overlays-windows/index.jsx` "General › General — row grid, row captions, webhook row" spec.

| 디자인 jsx 컴포넌트 | 본체 함수 | 갤러리 항목 |
|---|---|---|
| `Row`(라벨 열 · gap 16 · 컨트롤) | `src/view/settings/ui/tabs/general.rs::draw_general_tab` — 행마다 `tasty_ui_widgets::SettingsRow`, 열 폭은 `settings_label_column` | `components/settings_general_grid.rs::draw` (`settings` 섹션 `settings-general-row-grid` spec) — Restore layout · Close behavior · Wheel scroll distance · Language · 웹훅 다섯 행 |
| `RowCaption`(Wheel scroll distance · Language) | 같은 함수 — `SettingsRow::caption` 로 휠 거리 설명과 언어 재시작 안내를 각 행 아래에 | 동 |
| `WarnCallout`(웹훅 행) | 같은 함수 — `SettingsRow::warning` 이 `measure_md` 폭 안에서 `warning_callout` 을 그린다 | 동 |

**전사 노트**:
- 라벨 열: 서브탭의 가장 긴 라벨 폭을 `settings-label-width`(150) … `settings-label-max-width`(240)로 clamp 한다. 더 긴 라벨은 열 안에서 줄을 바꾼다(en·ja 웹훅 라벨). 열은 언어마다 그 화면을 그릴 때 잰다. 라벨은 본문 크기 `text-secondary` 다.
- 행 아래 caption 과 callout 은 행과 `settings-row-caption-gap`(4) 만큼 떨어지고 폭은 `measure-md` 까지다. 창이 그보다 좁으면 남은 폭에서 줄을 바꾼다. callout 은 스위치 상태와 관계없이 항상 그린다. caption 글자는 `font-size-caption` · `text-muted` 다(언어 재시작 안내도 같은 caption 이다).
- 행 사이는 `settings-row-gap`(12)이다. 섹션 헤딩·행·행 사이 구분선이 한 세로 흐름에 있는 서브탭(Remote transfer · Task pipeline)은 그 셋 사이도 모두 이 간격이다.
- 토큰: `settings-label-width` · `settings-label-max-width` · `settings-label-gap` · `settings-row-gap` · `settings-row-caption-gap` · `settings-row-min-height` · `measure-md` · `accent-warning`.

## Settings › General › Overlay — toast duration

General L1 의 4번째 L2 "Overlay" 의 한 행. 디자인: `gallery/parts/overlay-frames.jsx` `ToastDragValue`(= `ToastDurationField`) ·
`SettingsGeneralOverlayFrame` + `gallery/overlays-windows/index.jsx` "General › Overlay — toast duration" spec.

| 디자인 jsx 컴포넌트 | 본체 함수 | 갤러리 항목 |
|---|---|---|
| `ToastDragValue`(설정 숫자 모양) | `src/view/settings/ui/tabs/overlay.rs::draw_overlay_tab` → `number::number_field`(`NumberSpec::int(1.0, 10.0).step(0.5).decimals(1)`, 단위 `s`) — mono `Input` `field_width_xs`(90) · 오른쪽 정렬 · 단위 caption text-muted · 확정(blur/↵) 때 범위 제한과 0.5 눈금 · 범위 밖이면 danger 테두리와 범위 한 줄 | `components/settings_number.rs::draw_toast_duration`(`settings-overlay-toast-duration` spec) — 평소 `2.0` · 범위 밖 `14` 두 상태와 Meta. 편집 중 상태는 공용 `Input` 의 포커스 테두리라 갤러리에서는 칸을 눌렀을 때 보인다. `SettingsGeneralOverlayFrame` 창 틀 전체는 옮기지 않았다 |

## Settings › General › Remote transfer

General L1 에 5번째 L2 서브탭 "Remote transfer" 추가 — 원격 mirror 파일 전송(bulk, [원격 attach (Remote attach)](../../features/remote-attach/index.md))
수신측 저장 정책(`RemoteTransferSettings{dir, max_mb}`) 편집. 디자인:
`gallery/parts/overlay-frames.jsx` `SettingsRemoteTransferFrame` + `gallery/overlays-windows/index.jsx`
"Settings · General › Remote transfer" spec. 저장 정책을 편집하는 화면이다.

| 디자인 jsx 컴포넌트 | 본체 함수 | 갤러리 항목 |
|---|---|---|
| `SettingsRemoteTransferFrame`(콘텐츠 컬럼) | `src/view/settings/ui/tabs/remote_transfer.rs::draw_remote_transfer_tab` | `components/settings_remote_transfer.rs::draw` (`settings` 섹션 `settings-remote-transfer` spec) |
| `Mono`("Received files") | `mono` 헤딩(micro uppercase muted) | `mono_head` |
| `Row`(Save folder) | `SettingsRow` + right_to_left(Browse→Input) | `SettingsRow` |
| `Input block mono` + `Button secondary sm folder`(Browse…) | `Input::mono` + `Button::Secondary/Sm/FOLDER` + View-local Tasty Folder chooser | 동(선택 화면은 filepicker-folder-mode 견본) |
| `Row`(Maximum size) + `Input mono width90` + 정적 `MiB` | `SettingsRow` + `number::number_field`(`NumberSpec{suffix: Some("MiB")}`). 입력 칸과 단위는 감싸는 가로 줄이라, 남은 폭에 단위가 들어가지 않으면 단위가 입력 칸 아래 줄로 내려간다(모든 `number_field` 공통) | `SettingsRow` + 동 |
| `Note`(행별 muted 설명) | `SettingsRow::caption` | `SettingsRow::caption` |
| 행 사이 `borderTop separator` | `row_separator`(`th.separator` hline) | `separator_line` |

**전사 노트**:
- 행은 설정 공용 행 격자다(라벨 열 = 가장 긴 라벨을 150 … 240 으로 clamp, gap 16, 행 높이
  `settings_row_min_height`(32), 설명은 행 아래 caption). 콘텐츠 wrapper 패딩은 공유 `tab_content_frame`(space-lg)
  가 제공(형제 탭 관례 — 재패딩 안 함).
- 최대 크기 입력은 정수 칸이라 값에 맞춰 자라며 하한이 `number-field-min-width`(→ `field_width_xs` 90)다(아래 Performance 절의 공용 규칙). 하한 90 은 승인된 값이다. field-width 토큰 세트는 90/110/160/180/200이다. 2026-09-17 결정에서 예제의 88px 대신 90px를 승인했다. 2026-09-20 확인한 `gallery/parts/overlay-frames.jsx`의 `style={{ width: 88 }}`에는 아직 반영되지 않았으므로 그 값으로 되돌리지 않는다.

- **"MiB" 는 필드 밖 정적 `font-label` suffix**(Toast 의 " s" 와 동형, addon/Tag 아님). i18n 단위
  기호 예외로 리터럴.
- **별도 Theme 필드 없음** — 전부 기존 접근자(`settings_row_min_height`/`field_width_xs`/
  `separator`/`text_muted`/`font_size_micro`/`font_size_caption`)·기존 위젯(`Input`/`Button`).
  i18n 8키(`settings.tab.remote_transfer` + `settings.remote_transfer.{section,dir,dir_placeholder,dir_desc,browse,max_capacity,max_capacity_desc}`).
- 갤러리는 본체 미의존이라 host `draw_remote_transfer_tab`(Settings 저장소 의존)을 직접
  못 부르고 같은 위젯·토큰으로 미러(settings_handler 서브탭 specimen 전례). 찾아보기의 폴더 선택 화면은 공용 footer를 쓰는 filepicker-folder-mode specimen으로 대조한다.

## Settings › General › Remote attach

General L1 의 6번째 L2 서브탭이다(Remote transfer 다음, macOS 전용 Display·Permissions 앞). `[remote_attach] heartbeat_timeout_secs`(`Disconnect after`)와 `reconnect_timeout_secs`(`Keep reconnecting for`)를 편집하는 행 두 개를 담는다. 디자인: `gallery/parts/overlay-frames.jsx` `SettingsRemoteAttachFrame` + `gallery/parts/settings-attach-section.jsx` `AttachConnectionsSection` + `gallery/overlays-windows/index.jsx` "Settings · General › Remote attach" spec.

| 디자인 jsx 컴포넌트 | 본체 함수 | 갤러리 항목 |
|---|---|---|
| `SettingsRemoteAttachFrame`(콘텐츠 컬럼) | `src/view/settings/ui/tabs/remote_attach.rs::draw_remote_attach_tab` | `components/settings_remote_attach.rs::draw` (`settings` 섹션 `settings-remote-attach` spec) |
| 헤딩 "Attach connections" | `settings.remote_attach.section`, `font-label` micro uppercase muted, `letter-spacing-caps` | 동(같은 토큰) |
| 행 + 설명 | `SettingsRow` + `SettingsRow::caption` | 동 |
| `Input block mono`(오른쪽 정렬) + 정적 `font-label` `s` | `number::integer_field`(`NumberSpec::int` 에 각 행의 범위 `HEARTBEAT_TIMEOUT_SECS_RANGE` 10~600 · `RECONNECT_TIMEOUT_SECS_RANGE` 0~3600, `suffix`, `settings.number.unit_s`). 칸과 단위 사이는 `number-field-unit-gap` | `seconds_control`(같은 `Input`·토큰) |
| 범위 밖 `role="status"` 줄 | 공용 숫자 칸의 기본 danger 문장 `settings.number.range_between`. `range_note` 없음. 칸 아래 `space-xs` | 동 |

**전사 노트**:

- 설명은 행 전체의 폭을 쓰는 caption 이며 `measure-md` 에서 줄을 바꾼다.
- 갤러리는 본체 미의존이라 host 함수를 직접 부르지 않고 같은 위젯·토큰으로 미러한다. 시안처럼 Mocha·Latte 의 평소·`5` 입력·`1000` 입력, ko·ja, 좁은 폭의 쌓인 행을 나란히 둔다. 시안은 쌓인 행을 `stacked` 속성으로 강제하고 칸 폭이 320 이지만, 앱 행은 남은 폭으로 쌓임을 정해 320 에서는 쌓이지 않으므로 갤러리 칸은 280 이다. 시안의 창 틀(`SettingsRemoteFrameShell`)과 L2 목록은 옮기지 않았다.
- **별도 Theme 필드 없음** — 기존 접근자와 기존 위젯만 쓴다. i18n 4키(`settings.tab.remote_attach` + `settings.remote_attach.{section,heartbeat_timeout,heartbeat_timeout_desc}`), 재사용 2키(`settings.number.unit_s`, `settings.number.range_between`).

## 포트 스캐너 (Overlays) — Table 열 정의 공유

디자인 `ui_kits/terminal/overlays/port_scanner.jsx` `PortsWindow` + `gallery/parts/overlay-frames.jsx` `PORTS_COLUMNS`·`PortsFrame` + `gallery/overlays-windows/index.jsx` Listening ports Section ↔ 갤러리 `catalog/components/port_scanner.rs`(`draw`·`draw_process_column`) ↔ 본체 `src/adapters/ui/popup/port_scanner.rs`의 `draw_table`.

- 시안의 `PORTS_COLUMNS`처럼 본체와 갤러리가 같은 열 정의를 쓴다. 갤러리는 본체에 의존하지 않으므로 정의는 `tasty-ui-widgets`의 `PortsColumn`·`ports_table`·`ports_process_cell`에 있다. 본체 `ColumnId`는 열 선택 상태와 정렬 키를 맡고 `ColumnId::shared`로 이 정의를 찾는다.
- 열 폭은 본체 값이다(Port 84 · Proto 76 · Address `port-addr-col-min-width` · Process `port-process-col-min-width` · Workspace 120 · Tab 80 · State 140). 시안 갤러리 세트(72·64·104·132, Tab 없음)와 시안 popup 세트(84·76·120·140, Tab 포함)가 다르며, 회신은 앱 갤러리가 popup 정의를 미러하도록 정했다.
- Process 열은 시안 Table 의 `minWidth` 대응인 `TableColumnWidth::Flex`로 하한을 가진다. 고정 열은 폭이 곧 하한이다. 하한 합보다 좁으면 본문이 가로 스크롤하고 열을 숨기거나 줄이지 않는다. Process 이름은 말줄임하고 PID Tag 는 유지한다.
- `Process column` 예제는 시안처럼 공용 Table 로 860·660 두 폭을 그린다. 열은 시안 세트와 같은 별 · Port · Proto · Address · Process · Workspace · State(Tab 숨김)다. 캡션의 열 예산은 공용 정의에서 계산한 값이다. 시안의 예제 전용 숫자(머리줄 24 · 행 26 · mono 11 · PID 열)는 없다. 두 예제 사이는 `space-lg`(갤러리 Column stage 간격), 캡션 아래는 `space-sm`(cluster 간격)이며 시안도 같은 토큰을 쓴다.
- 남는 폭은 모두 Process 가 받는다. Address 는 `port-addr-col-min-width` 에 고정돼 늘지 않는다(시안 Address `width` = 이 토큰, Process `width: 100%` + `minWidth`). 갤러리 860 캡션과 Meta(Address · spare width · stage gaps 행, Address 토큰 칩)도 시안과 같다.
- 반영 상태: batch 7(Table 기반 specimen)과 batch 8(남는 폭·Address 토큰·stage 간격) 회신이 갤러리와 본체에 모두 반영됐다.

<a id="file-picker-overlays--gallery-first-반영-완료-본체-배선됨"></a>

## Settings › Misc › Task pipeline

Misc L1 의 두 번째 L2 서브탭 "Task pipeline" — 작업 그래프 report 의 두 크기 상한
(`TaskPipelineSettings{report_append_bytes, report_block_bytes}`) 편집. 디자인 카탈로그 프레임은
`gallery/overlays-windows/index.jsx` 의 "Settings · Misc › Task pipeline — Report limits" Spec 이다
(갤러리 spec id `settings-task-pipeline`, Remote transfer 뒤). 새 시각 값 없이 Remote transfer 의 숫자 행
(`SettingsRow` + `number_field`)을 그대로 쓰고 단위만 `B` 다.

| 디자인 jsx 컴포넌트 | 본체 함수 | 갤러리 항목 |
|---|---|---|
| Spec 의 콘텐츠 컬럼(헤딩 REPORT LIMITS `font-label` micro uppercase muted) | `src/view/settings/ui/tabs/task_pipeline.rs::draw_task_pipeline_tab` | `components/settings_task_pipeline.rs::draw` (`settings` 섹션 `settings-task-pipeline` spec) — Mocha 기본 짝 · Latte 범위 밖 짝 |
| 숫자 행 두 개(mono Input, 값에 맞춰 자라고 하한 `number-field-min-width` 90, 오른쪽 정렬 + 정적 `B`) | `SettingsRow` + `number::integer_field`(`NumberSpec{suffix: Some("B")}`), 두 칸의 범위가 서로의 현재 값으로 좁혀진다(`tasty_settings::REPORT_*_BYTES_RANGE`) | `SettingsRow` + `bytes_control`(같은 범위 상수) |
| 범위 줄(범위 밖일 때만, `accent-danger`, caption) | `number_field` 의 범위 줄 | `bytes_control` 의 범위 줄 |
| 행별 muted 설명 · 행 사이 separator | `SettingsRow::caption` · `row_separator`(Remote transfer 와 공유) | `SettingsRow::caption` · `separator_line` |

**전사 노트**:
- 헤딩·행·구분선 사이는 모두 `settings-row-gap`(12)이고 라벨 열은 다른 설정 행과 같은 150 … 240 clamp, 라벨 → 컨트롤 `settings-label-gap` 16 이다.
- 값은 그룹 구분 없는 원시 바이트이며 KiB 로 바꾸지 않는다.
- 범위 줄의 자리: 모든 숫자 칸이 공유하는 `number_field` 구조대로 입력칸 아래(컨트롤 칸 안, 라벨 열 + 라벨 간격만큼 들어간 자리), 설명 caption 위다. 시안 Task pipeline·Numbers in settings 예제도 같은 자리다. 갤러리 Numbers in settings·토스트 표시 시간 예제(`settings_number.rs` `range_line`)는 입력칸 왼쪽 끝에서 시작한다.

## Settings › Misc › Anomaly detection

`ui_kits/terminal/overlays/settings_anomaly.jsx`와 `gallery/overlays-windows/settings-anomaly.jsx`의 설정 본문이다. Misc의 Task pipeline 뒤에 둔다.

| 디자인 구성 | 본체 | 갤러리 |
|---|---|---|
| info 안내 · 세 섹션 · 아홉 행 | `settings_anomaly` 공용 view와 `tabs/anomaly_detection.rs` | `settings_anomaly_detection`의 같은 공용 view |
| 헤딩 `font-label` micro uppercase muted · 알림 caption 끝 내부명 mono secondary | `AnomalySection` props, 번역된 사람 이름과 caption | 같은 props 구조 |
| 숫자 여섯 칸 · 단위 · 범위와 확정값 안내 | `number::integer_field`, i128 편집, tasty-settings의 여섯 범위 상수 | mono Input과 동일 범위 상수, Enter/blur 확정 |
| responsive 행 | `SettingsRow`와 `settings_label_column`, `settings_stack_row`의 자연 폭 판정과 기존 히스테리시스 | 같은 함수, 수동 stacked 플래그 없음 |

스위치 caption은 실제 Ui 폭을 measure-md와 남은 폭 중 작은 값으로 제한한 뒤 줄을 바꾼다. 내부명은 한 번만 mono secondary로 그린다. 좁은 견본은 영어 SlowLoop 1→2, 한국어 기본 상태, 일본어 CallBurst 5→10을 보여 준다. 견본 액자의 폭은 영어·일본어 320, 한국어 440의 이름 있는 LogicalPx 전시 치수다. 대응하는 Theme 프레임 역할이 없어 같은 숫자의 다른 역할 토큰으로 대체하지 않는다.

안내→섹션과 섹션 사이는 space-xl, 섹션 안은 settings-row-gap이다. 숫자 행 사이는 separator, 설명은 settings-row-caption-gap과 measure-md를 따른다. 범위 줄은 number-field-note-fg이며 입력칸 아래에 범위·확정값을 함께 보여준다. 갤러리도 본체와 같은 `tasty_i18n::fill_args`로 번역 템플릿의 최솟값·최댓값·확정값을 채운다. 기본 알림은 on/off/on이다. 알림을 꺼도 숫자는 보통 잉크로 편집 가능하다. SlowLoop는 응답 지연이 아닌 동일 호출 반복 횟수다. 각 caption에 기본값을 적고 별도의 복원 버튼은 두지 않는다. 저장은 다음 판정에 적용하며 기존 관측 이력을 초기화하지 않는다.

## Settings › Terminal › Performance

성능 탭의 적용 시점 안내와 세 행이다. 디자인 시안은 `gallery/overlays-windows/settings-performance.jsx`(`#perfapply`)와
`ui_kits/terminal/overlays/settings_window.jsx` 의 Performance 본문이다. 갤러리 spec id 는 `settings-performance`(Task pipeline 뒤)다.

| 구성 | 본체 함수 | 갤러리 항목 |
|---|---|---|
| 적용 시점 안내(탭 맨 위 한 번, info 글리프 `icon-size-sm` `settings-apply-note-glyph` + caption `settings-apply-note-fg`, 간격 `settings-apply-note-gap` 8, `measure-md` 에서 줄바꿈·말줄임 없음) | `tasty_ui_widgets::settings_apply_note` | 같은 함수 |
| 스위치 두 행(PTY 선택 폴링 · 스크롤백 디스크 스왑) | `src/view/settings/ui/tabs/performance.rs::draw_performance_tab` — `SettingsRow::hint` + `tasty_ui_widgets::switch` | `components/settings_performance.rs::draw` — 기본값 · 스왑 끔 · -1 · 큰 설정 값 · 좁은 폭 360, Mocha·Latte |
| 디스크 보관 줄 수(mono Input, 값에 맞춰 자람, 왼쪽 정렬, 칸 뒤 단위 `settings.number.unit_lines`, -1 이면 단위 자리에 `settings.performance.no_limit`) | `number::integer_field`(`NumberSpec::int(-1, 100000000)` + `align(Left)` · `unit` · `no_limit` · `range_note`, 디스크 스왑이 꺼져 있으면 `enabled(false)`) | `lines_control`(같은 범위·단위·비활성 조건) |
| 범위 줄(범위 밖일 때만, caption `number-field-note-fg`, `measure-md` 에서 줄바꿈, 칸 줄과 space-xs) — 문구 `settings.performance.disk_lines_range` | `tasty_ui_widgets::number_field_note` | 같은 함수 |

- 안내 → 첫 행, 행 → 행 간격은 모두 `settings-row-gap`(space-md 12)이다. 킷 Settings 본문과 같은 값이며, `#perfapply` 견본의 행 사이 space-sm 은 견본 카드 안의 축약이다.
- 행 라벨과 도움말은 본체 문자열을 쓴다. 시안 견본의 라벨·도움말 문장은 견본 표현이며, 재시작이 필요하다는 표현만 쓰지 않는다는 지시다.

### 값에 맞춰 자라는 숫자 칸

시안 `gallery/overlays-windows/settings-performance.jsx` `PfLinesSpec`(`PfNumField`)이다. 갤러리 spec id 는 `settings-performance-number`(`settings_performance::draw_number_field`)다.

| 구성 | 본체 함수 | 갤러리 항목 |
|---|---|---|
| 칸 폭 = max(`number-field-min-width`, (글자 수 + 1) × 칸 mono 글꼴 `0` 너비 + 2 × `input-padding-x` + 2 × 테두리), 상한은 남은 폭. 글자 수는 편집 중이면 친 글자, 아니면 저장 값(부호 포함) | `tasty_ui_widgets::number_field_width` | 같은 함수 |
| 칸 뒤 단위(`font-label` caption `number-field-unit-fg`, 간격 `number-field-unit-gap`, 비활성 `state-disabled-fg`) · -1 의 "no limit"(UI 글꼴 caption, 단위 자리) | `tasty_ui_widgets::number_field_unit` | 같은 함수 |
| 범위 줄(caption `number-field-note-fg`, `measure-md`) | `tasty_ui_widgets::number_field_note` | 같은 함수 |

- **공용 규칙**: 소수 자리가 없는 모든 설정 숫자 칸(`number::number_field`·`integer_field`, `decimals == 0`)이 값에 맞춰 자란다. 대상은 디스크 보관 줄 수, 스크롤백 줄 수, 알림 묶음 ms, 휠 스크롤 거리(pt), 원격 전송 최대 크기(MiB), 작업 파이프라인 바이트 두 칸, 글꼴 크기(기본·surface·override), 정수 값인 plugin 숫자 설정이다. 소수 칸(토스트 표시 시간, 줄 높이)은 `field-width-xs` 고정이다. File Handlers priority 는 `number_field` 를 쓰지 않으며 `file-handler-priority-width` 90 고정이다(시안 예외).
- 원격 전송·파이프라인·그 밖의 칸은 폭 규칙만 더한다. 단위 색(`text-muted`)·오른쪽 정렬·범위 줄(`accent-danger`, 범위와 확정될 값)은 그대로다. 시안 Meta 의 단위·no limit·범위 문장·왼쪽 정렬은 디스크 보관 줄 수 칸에만 쓴다.
- 정수 칸은 `i128` 로 읽고 쓴다. `f64` 를 거치면 2^53 을 넘는 값(예: 9223372036854775807)이 다른 수로 보이기 때문이다. 읽은 값은 범위 밖이어도 보정하지 않고 그대로 보이며, 사용자가 확정할 때만 범위로 맞춘다.
- 편집 중 폭은 그 프레임이 시작할 때의 글자 수로 정한다. 커서 한 칸 여유가 있어 한 글자를 친 프레임도 값 전체가 칸에 들어가고, 다음 프레임에 칸이 자란다. 칸이 남은 폭 상한에 닿을 때만 egui 기본 한 줄 스크롤이 생긴다.
- 시안의 "before"(110 + 칸 안 단위) 견본은 갤러리에 옮기지 않았다. 지난 모양이며 본체에 없다.

## 파일 피커 (Overlays)

디자인 `gallery/parts/overlay-frames.jsx` `FilePickerFrame`/`FpRow`/`FpCrumbs`/`FpCrumbMenu`/`FpHostBadge`
+ `gallery/overlays-windows/index.jsx` `#filepicker` Section ↔ 갤러리
`catalog/components/file_picker.rs`.
본체는 `src/adapters/ui/popup/file_picker.rs`의 `draw_file_picker`를 사용한다. `FILE_PICKER_POPUP_ID = "file_picker"`인 egui `PopupDef`가 `defs.rs`에 등록돼 있다. 디자인처럼 헤더는 하나다 — `PopupDef`가 `headless`라 셸 타이틀바가 없고, 뷰가 그린 헤더 줄을 이동 손잡이(`DragHandle::Region`)로 보고한다. 셸 공통 내부 여백을 두지 않는 popup 이라 구역의 배경·구분선(path bar 의 `bg-sidebar` 띠 포함)은 창 좌우 끝까지 닿는다.

구역 안쪽 여백은 갤러리와 본체 모두 `component.fp-*` 토큰을 쓴다.

- 모든 구역의 시작선은 `fp-inset-start`(12)다. 헤더 글리프·첫 crumb·목록 머리·행 아이콘·푸터 라벨이 한 열에 선다. 헤더와 path bar 의 끝 IconButton 쪽은 `fp-inset-end`(8)이고, 푸터는 양쪽 모두 `fp-inset-start`다.
- 구역 안 간격은 `fp-section-gap`(8)이다.
- 헤더에는 고정 높이가 없다. `fp-header-pad-y`(8) + sm IconButton(24) + 8 = 40이고, 원격 host 배지(22)는 이 안에 들어간다. path bar 는 `fp-path-pad-y`(4) + 24 + 4 = 32다.
- 목록 머리는 `fp-list-head-pad-y`(4)와 `font-size-micro` 라벨 한 줄, 행은 `fp-row-pad-y`(4)와 아이콘(16)·이름 줄 중 높은 쪽으로 높이가 정해진다. 줄 높이는 디자인 기본 줄 높이 `line-height-ui`(1.4)를 글꼴 크기에 곱한 값이라 행은 4 + 13 × 1.4 + 4 = 26.2다. 글꼴 행 높이로 재지 않으므로 갤러리와 본체의 글꼴 구성이 달라도 높이가 같다.
- 푸터는 위아래 `fp-footer-pad-y`(8)다. 라벨 폭은 `fp-footer-label-width`(64)다. 필터 칩 높이는 `fp-filter-height`(28), 최대 폭은 `fp-filter-max-width`(160)다.

본체도 목록 머리(NAME/SIZE/MODIFIED)와 선택 행 좌측 bar를 그린다. 열 위치는 행과 목록 머리가 같은 계산(`cols`)을 쓴다. 선택 bar 폭은 `selection-edge-width`, 색은 `accent-primary`다. 헤더 제목은 `font-size-max`(14)에 `strong`이다(egui는 semibold를 고르지 못한다). 필터 칩은 본체와 갤러리가 공용 `tasty_ui_widgets::filter_readout`(`filter_readout_label` · `filter_readout_width`)을 부른다. 호출자 필터가 없으면 칩을 그리지 않고, 이름 칸이 칩 폭과 간격을 뺀 나머지를 갖는다.

640×480 단일 컴포넌트가 로컬/원격 두 모드를 겸한다 — 차이는 헤더 host indicator 와
브레드크럼 root 뿐, 레이아웃은 불변. §6.1 열린 결정(원격 표시 A 배지 / B 글리프 /
C 프레임보더) 중 **A 배지가 사용자 확정**되었다. 본체와 갤러리의 다른 예제는 A만
쓰고, B/C 는 미채택 대안으로 `filepicker-remote-indicator` spec 의 비교 그림에만 그린다.

| 디자인 jsx 컴포넌트 | 갤러리 함수 (`file_picker.rs` · `file_picker/{path_bar,footer}.rs`) | 비고 |
|---|---|---|
| `FilePickerFrame`(container) | `card` | 640×480 · bg-panel · border-strong · modal shadow |
| header(glyph·title·host indicator·✕) | `header` → `indicator::{header_glyph, after_title}` | 로컬과 A안은 `FILE` 글리프. B안은 `remote` 글리프(`accent-info`) + mono 호스트 글자, C안은 `remote` 글리프(muted) |
| `FilePickerFrame indicator` prop · `overlays-windows.jsx` "Remote indicator — three candidates" | `Variant::indicated` · `indicator::top_strip` · `draw_remote_indicator`(Spec `filepicker-remote-indicator`) | A 배지 · B 글리프 + 호스트 · C 프레임 테두리(`accent-info` 테두리 + 위 2px 띠) 세 카드 + Meta + Don't |
| host 배지(§6.1 A안, 채택) | `host_badge` | mono `user@host` · `accent-info` 14%/45% 배경/보더 |
| path bar(`FpCrumbs`+Up+refresh) | `path_bar` → `crumbs` | `bg-sidebar` 전폭 띠 · 아래 `separator` 1px. 경로 · Up(`chevronUp`) · refresh 를 `fp-section-gap` 간격으로 놓는다. root=mono, 조상=accent 링크, 현재 폴더=`text-primary` 보통 굵기 비클릭, 글자 `font-size-caption` |
| list header(NAME/SIZE/MODIFIED) | `list_header` | loaded/multi 상태만, `cols()` 좌표 공유 |
| `FpRow` | `row` | selected=surface-active+2px accent 좌측바, focus=1px accent outline(선택과 구분) |
| 로딩/빈폴더/에러(권한·연결끊김) | `body` → `center` → 공용 `CenterState` | Spinner · folderOpen · 부품 소유 오류 글리프(alertTriangle). Retry/Reconnect 는 refresh 아이콘을 단 위젯 액션으로, 가운데 정렬 밖 보조 슬롯 아래에 매달린다 |
| footer(name field+filter chip+Cancel/Open) | `footer` + 공용 `filter_readout` | `kit::field` 재사용, Open 은 loaded 상태에서만 활성. 라벨·칩·버튼 flex:none, 이름 칸만 준다 |
| `FilePickerFrame filters` prop · `overlays-windows.jsx` "File-type filter chip — a read-only readout" | `Variant::filtered` · `Variant::sized` · `draw_filter_chip`(Spec `filepicker-filter-chip`) | 4 프레임(필터 없음 · 하나 · 둘+저장 · 여섯=상한 말줄임), 디자인과 같은 480×300 카드. 읽기 전용 표시라 chevron·채움·hover 없음, 툴팁 "Showing …" |
| footer overwrite line(`save="picked"`) | `overwrite_line` · `footer_height` | alertTriangle + 이름 mono · `accent-warning`. footer 가 커지면 본문이 준다 |
| `FilePickerFrame mode/save` prop | `Variant` · `Mode` · `SaveState` | Save file 제목 · Save/Overwrite 라벨 · 저장 모드 선택 행 |
| `FilePickerFrame remote/deep/pathKind` prop | `Variant::path` · `path_bar::PathKind`(`Local` · `Remote` · `Deep` · `LongTwo` · `LongRoot`) | 디자인 crumbs seed 를 그대로 쓴다 |
| `FilePickerFrame folderSel` prop | `Variant::folder_selected` · `footer::folder_line`(Spec `filepicker-gesture-table`) | 고른 것이 폴더인 상태 — 저장은 "저장 대상이 아니다", 열기는 "확정하면 들어간다". 톤 없는 muted caption + `folder` 글리프, 열기 문구는 확정 버튼 이름을 부른다. 본체는 `file_picker::selected_folder` · `footer::folder_line` |
| `FpCrumbs`(배분 · `elide` · `single`) | `crumbs` → 공용 `tasty_ui_widgets::crumb_alloc::plan` | 본체와 같은 배분을 카드의 실제 폭에 돌린다. 가용 폭은 막대에서 Up · refresh 와 간격을 뺀 폭(`crumbs_width`). 상한·바닥·여유는 `fp-crumb-max-width`(180) · `fp-crumb-min-width`(64) · `fp-crumb-current-min-width`(96) · `fp-bar-hysteresis`(8). 조상은 꼬리, 현재 폴더는 앞(`…-bbbb`)에서 말줄임. `…` 는 hover 시 "Show N hidden folders"(1 이면 단수) |
| `FpCrumbMenu`(`crumbMenu` prop) | `Variant::crumb_menu_open` · `path_bar::crumb_menu` | 숨긴 조상을 경로 순서로, `folder` 글리프 + `menu_item`. 틀은 `menu-bg` · `menu-border` · `menu-radius`, 안쪽 여백 `popup-content-margin`. 폭은 가장 긴 줄에 틀(여백·테두리)을 더한 바깥 폭을 `fp-crumb-menu-min-width`(180)~`fp-crumb-menu-max-width`(320) 밴드에 맞춘다(본체 `menu_width` 와 같은 식). 첫 줄 `menu-item-bg-hover` |
| `overlays-windows.jsx` "Path bar — what gives way when the folded path still doesn't fit" | `draw_path_fit`(Spec `filepicker-path-bar`) | 측정 카드 폭 사다리 — longtwo 640 · 520 · 440 · 360 · 320, longroot 320(높이 300). 라벨은 그 카드 폭에서 `plan` 이 돌려준 단계 + Meta + Do/Don't + Note. 디자인의 "stage diagram (forced)" 카드는 접힘을 강제한 그림이라 옮기지 않는다 |
| `overlays-windows.jsx` "Save mode — one confirm, in the footer" | `draw_save_mode` | 4 프레임(new · picked · edited · deep) + Meta + Note |

본체 `src/adapters/ui/popup/file_picker.rs::entry_row`도 `FpRow`의 오른쪽 고정 열과
이름 가변 열 배치를 따른다. 이름은 남은 열 폭에서 egui 단일 행 galley로 말줄임하며,
선택·확정에 사용하는 원래 이름은 보존한다.

**갤러리 vs 디자인 차이**: 긴 파일명 말줄임은 jsx `text-overflow:ellipsis`(CSS 네이티브)
대신 `elide()`(문자 단위 폭 측정 후 컷 + `…`)로 근사한다. 브레드크럼은 jsx 처럼 `elide`·`single` prop 으로
접힘 단계를 정하지 않고 본체와 같은 `crumb_alloc::plan` 이 카드 폭으로 정한다. 카드 라벨은 갤러리가 실제로 그린
단계를 적는다. `…` 메뉴 예제는 깊은 경로가 접히는 피커 바닥 폭(`fp-popup-min-width`, 320×420) 카드에 둔다. 갤러리
카드는 크기가 고정이라 직전 단계(히스테리시스)를 넘기지 않는다. 디자인 · 갤러리 · 본체의 path bar 는 모두 Up 버튼을 두고,
경로 가용 폭은 막대에서 Up · refresh 와 간격을 뺀 폭이다. `…` 메뉴(`FpCrumbMenu`) 행은 갤러리와 본체 모두 공용
`menu_item`(높이 28 · padding `menu-item-padding-x` 12 · 아이콘 `icon-size-md` 16 · 글자 `menu-item-fg`, 호버 `menu-item-fg-hover`)이다.
행은 다른 메뉴처럼 간격 없이 붙어 28 간격으로 놓인다.
메뉴 폭의 180~320 밴드는 테두리를 포함한 바깥 폭(border-box)이다. 현재 폴더 crumb 은 보통 굵기 `text-primary`, crumb 글자는
`font-size-caption`(11)이다. 색·간격은 전부 기존 semantic 접근자
(`accent_info`/`surface_active`/`accent_primary`/`text_placeholder`/`bg_sidebar` 등)와
기존 위젯(`kit::field`/`checkbox`/`Spinner`/`Button`/`IconButton`)으로 해소.

## Remote file transfer 팝업 (Overlays) — 진행 + 실패

디자인 `gallery/parts/overlay-frames.jsx` `TransferProgressFrame` / `TransferErrorFrame`
↔ 본체 `src/adapters/ui/popup/transfer.rs`(PopupDef `transfer_progress` / `transfer_error`)
↔ 갤러리 `catalog/components/transfer.rs`. bulk 파일 전송 + mirror 터미널 이미지 붙여넣기 업로드에 대한
사용자 피드백 UI. 진행률 막대는 완료 비율을 표시하며, 비율을 알 수 없는 작업의 `Spinner`와 구분한다.

| 디자인 jsx | 본체 함수 (`popup/transfer.rs`) | 갤러리 함수 (`components/transfer.rs`) |
|---|---|---|
| `TransferProgressFrame`(container) | `draw_transfer_progress` (PopupDef draw_fn) | `progress_card` |
| 헤더(download glyph + "Receiving file" + mono pct) | `header_band` | `header_band` |
| 파일 행(file glyph + mono ellipsis name) | `progress_row` | `progress_row` |
| determinate 4px bar(track+fill) | `progress_bar` | `progress_bar` |
| done/total · rate (mono muted, space-between) | `progress_row` 내 | `progress_row` 내 |
| ghost Cancel(footer) | `footer_buttons` + `Button::Ghost` | `footer_buttons` + `Button::Ghost` |
| `TransferErrorFrame`(container) | `draw_transfer_error` (PopupDef draw_fn) | `error_card` |
| 헤더(warn glyph + "Transfer failed") | `header_band`(ALERT_TRIANGLE·accent-danger) | `header_band` |
| prose(`<b>name</b> could not be received.`) | `horizontal_wrapped` mono bold + 산문 | 동 |
| reason well(command-well: bg-app+separator, mono danger) | `reason_well` | `reason_well` |
| Dismiss / (mid-transfer)Retry (danger-fill 금지) | `footer_buttons`(Secondary/Ghost) | 동 |
| 배율 비교 Stage(ui_scale 0.85 · 1 · 1.2) + inset Meta | 해당 없음(본체는 현재 배율 하나로 그린다) | `draw` 두 번째 stage·meta |
| 배율 비교 Stage 배치(시안 `flexWrap: wrap`) | 해당 없음 | 세로 `StageVariant::Column`으로 쌓는다. egui `horizontal_wrapped`는 크기를 미리 모르는 cluster를 다음 줄로 넘기지 못해 1.2 카드가 Stage 경계를 넘는다 |

**본체 vs 갤러리 차이**: 갤러리는 main 바이너리 비의존이라 `draw_transfer_*`(DialogState 의존)을
직접 못 부르고 같은 구조·토큰으로 미러(정적 seed 데이터). scrim dim 은 본체 `draw.rs` 가 그리므로
갤러리 specimen 은 프레임을 클러스터에 **직접** 렌더한다(scrim 스테이지 미사용 — file_picker 관례,
[design-parity-notes](design-parity-notes.md) "transfer — scrim_backdrop 스테이지…" 참조). 진행
determinate bar 는 `Spinner` 처럼 위젯화하지 않고 painter 인라인(track `bg_app` + fill `accent_primary`,
0ms). 헤더 아래·푸터 위 구분선과 reason well 테두리는 `separator`(Mocha bg-panel 위 1.264:1)이고, 카드 바깥 경계만 `border-strong`이다. 디자인이 의도한 대비로 확인한 값이라 더 강한 토큰으로 바꾸지 않는다. 폭과 여백(`transfer-*` 토큰)은 양쪽이 같은 Theme 접근자를 읽어 UI 배율을 따르고, 줄 높이는 글자 크기 ×
`line_height_ui`로 계산한다. **별도 Theme 필드 없음** — 전부 생성 접근자([design-token-mapping §transfer](design-token-mapping.md#remote-file-transfer-progresserror-09) 참조).
i18n 6키(`transfer.progress.{title,cancel}` · `transfer.error.{title,body_suffix,dismiss,retry}`).

본체의 `upload_file_over_bulk`는 청크마다 `on_progress(sent,total)`을 호출한다. 이미지 업로드 워커가 `transfer_progress` 채널로 보내고 `drain_transfer_progress`가 화면 행을 갱신한다. `drain_image_upload_results`의 실패는 팝업으로 알린다. `BULK_REJECT_PREFIX`인 원격 거절에는 Dismiss만 제공하고, 그 밖의 실패에는 다시 큐에 넣는 Retry를 제공한다. 자세한 동작은 [원격 attach](../../features/remote-attach/index.md)를 따른다.

## 다운로드 후 실행 팝업 (Overlays)

디자인 `gallery/parts/overlay-frames.jsx` `DownloadRunFrame` / `DownloadRunLargeFrame`, 그리고 다운로드 용도의
`TransferProgressFrame`(title·action) / `TransferErrorFrame`(`reasonKind="sentence"`), 시안 `gallery/overlays-dialogs/index.jsx`
Section `#downloadrun` ↔ 본체 `src/adapters/ui/popup/download_run.rs`(PopupDef `download_run` / `download_run_large` /
`download_run_progress`)와 `transfer.rs`의 실패 팝업 다운로드 변형 ↔ 갤러리 `catalog/components/download_run.rs`.

| 디자인 jsx | 본체 함수 | 갤러리 함수 (`components/download_run.rs`) |
|---|---|---|
| `DownloadRunFrame`(container) | `draw_download_run` | `dialog_card` |
| 헤더(제목 font-size-max semibold · `FhRemoteMark` · space-sm · 경로 mono caption muted) | `draw_download_run` 머리 Frame, 경로 줄은 `remote_path_line`(공용 `tasty_ui_widgets::paint_fh_remote_mark` + 남은 폭에서 `elide_front_mono`) | `dialog_card` 머리, 같은 형상의 `remote_path_line`(같은 공용 위젯, 표본 호스트 `gx10`) |
| 본문(설명 body secondary · 한 파일 caption muted) | `wrapped` | `wrapped` |
| 옛 서버 well(surface-raised · info glyph accent-info-raised · caption secondary) | `old_server_well` | `dialog_card` 안 |
| footer ghost Cancel · primary Download and run(download glyph, 옛 서버면 비활성) | `footer_buttons` | `dialog_footer` |
| `DownloadRunLargeFrame` | `draw_download_run_large`(`header_band` attention glyph · `file_name_line` · 크기 문장) | `draw_large` |
| `TransferProgressFrame` title="Downloading to open locally" action="Stop download" | `draw_download_run_progress`(`transfer.rs`의 `header_band`·`progress_row`·`footer_buttons`) | `draw_progress` |
| `TransferErrorFrame` title="Download failed" reasonKind="sentence" | `draw_transfer_error`의 `TransferErrorKind::DownloadRun`, `reason_well` sentence | `failed_card`, `reason_well_kind` |

경로 줄 앞 호스트 표지는 핸들러 picker 와 같은 공용 위젯과 `fh-remote-*` 토큰으로 그리고, 디자인의 connected 모양만
쓴다. 디자인의 재연결 중 모양(`state="reconnecting"`)은 공용 위젯에 없어 picker 와 마찬가지로 그리지 않는다. 대화상자·큰 파일 확인·실패의 높이는 그린 내용을 재서 맞춘다(첫 프레임만
대략의 높이). i18n 은 `download_run.dialog.*`·`download_run.large.*`·`download_run.reason.*`와
`transfer.progress.{title_download_run,stop}`·`transfer.error.title_download_run`이다. 동작은
[원격 attach](../../features/remote-attach/index.md#원격-파일-다운로드-후-실행)를 따른다.

<a id="attention-kind--needsinput-배지dot테두리탭-제목-surfaces-docs/dev-guide/agent-runner.md"></a>

## Attention kind — NeedsInput 배지/dot/테두리/탭 제목

디자인 `components/core/Badge.jsx`(variant `warning`) + `components/feedback/StatusDot.jsx`
(status `needs-input`/`completion`), 시안 `gallery/layouts/index.jsx` Section `attention` ↔ 본체
`src/adapters/ui/{divider,egui_panels,tab_bar/tab,sidebar/view}.rs` ↔ 갤러리 Layouts ›
Attention kinds(`catalog/layouts_attention.rs`). 순위와 색 토큰의 판정은 공용
`tasty_ui_widgets` attention 함수(`crates/tasty-ui-widgets/src/attention.rs`)에 있고 본체와 갤러리가
같은 함수를 부른다. 요청·확정 절차는 [Surface Highlight (주의 환기)](../../features/surface-highlight/index.md)
가 정한 kind-aware 모델을 그대로 따르며, 토큰 값은 [design-token-mapping §attention
kind](design-token-mapping.md#attention-kind--needsinputcompletion-surface-highlight-docs/dev-guide/agent-runner.md)
참조.

| 시안 Spec / 컴포넌트 | 공용 함수 | 본체 호출부 | 갤러리 spec |
|---|---|---|---|
| The scale — kind → color → rank | — | `src/core/state/attention.rs` `AttentionLevel` | `attention-scale` (`layouts_settled::draw_attention_scale`) |
| Workspace row — `BadgeGroup` + `Badge variant="warning"`/`"primary"` | `workspace_attention_badges` · `attention_count_label`(99+) | `sidebar/view.rs::draw_workspace_card` 행 끝 right-to-left 칸 | `attention-rows` — 시안 다섯 경우(Completion only · NeedsInput only · Both · Overflow 99+ · Quiet). 행 카드의 여백·점 슬롯은 본체 카드 구조를 갤러리에서 다시 쌓는다 |
| Collapsed rail — `StatusDot` 점 하나 | `RailDot::resolve` · `paint_rail_dot` | `sidebar/view.rs::draw_collapsed_avatar` | `attention-rail` — 시안 여섯 경우(busy · completion · needs-input · completion+busy · needs-input+completion · all three) |
| Tab title — 제목 색 사다리 | `tab_title_color` | `tab_bar/tab.rs` `text_color` | `attention-ladder` 탭 줄 — needs-input · completion · active · rest 네 탭(활성 탭에는 attention 색을 얹지 않는다) |
| Surface border — 한 선만 남기는 사다리 | `surface_edge_attention` · `attention_edge_stroke` · `occupancy_edge_stroke` | `divider.rs::regions_from_state`/`draw_surface_highlights_view` · `egui_panels.rs` 점유 테두리 | `attention-ladder` 테두리 줄 — needs-input 2px · occupied soft 1px · completed 2px |

**별도 Theme 필드 없음** — 전부 기존 component 접근자(`tab_fg*`·`surface_highlight_*`·
`surface_occupied_*`·`status_dot_*`·`badge_group_gap`)로 해소([design-token-mapping
§attention kind](design-token-mapping.md#attention-kind--needsinputcompletion-surface-highlight-docs/dev-guide/agent-runner.md)
참조). `AttentionKind`(host, `src/core/state/attention.rs`)는 `divider.rs`의 `From` 변환으로 공용
`Attention`이 된다. 갤러리는 라이브 attention 상태에 연결되지 않고 경우 데이터를 공용 함수에 넘긴다.
레일 점은 compact 지름(`status-dot-size-compact` 6)이고 실행 중 점을 포함한 모든 점을
`status_dot_ring()`(→ bg-sidebar) 색, `status_dot_ring_width()` 1.5px 고리로 두른다. 고리 두께는
hairline이라 UI 배율을 적용하지 않는다.

## 부팅 오류 화면 (Chrome)

디자인 `gallery/loading/index.jsx` Section `booterror`의 `BootErrorFrame`(b12) ↔ 갤러리 `catalog/chrome_loading/boot_error.rs::draw`(Chrome › First-run shell setup 구역의 두 번째 Spec "Boot error screen". 구역 목록 `catalog.rs`는 동결 파일이라 Spec 행을 늘리지 않고 앞 Spec 뒤에 이어 그린다) ↔ 본체 `src/gfx/gpu/boot_error.rs` `render_boot_error`. 본체와 갤러리는 공용 view `tasty_ui_widgets::boot_error_screen`을 함께 호출한다. 시안처럼 카드 없이 락업과 `boot-form-width` 폼을 쓰고 Quit은 공용 `Button` Secondary md다. 시안의 네 장(엔진·데이터 폴더·웹훅 포트 Mocha, 데이터 폴더 Latte)을 같은 순서로 그린다. 동작은 [부팅 순서](../../architecture/boot-sequence.md)에 있다.

## 첫 실행 셸 설정 (Chrome)

디자인 `gallery/loading/index.jsx` Section `shellsetup`의 `ShellSetupFrame` ↔ 갤러리 `catalog/chrome_loading.rs::draw_shell_setup`(Chrome › First-run shell setup, `shell-setup-form` spec) ↔ 본체 `src/gfx/gpu/shell_setup.rs` `render_shell_setup`. 본체와 갤러리는 공용 view `tasty_ui_widgets::shell_setup_screen`을 함께 호출한다. 동작은 [부팅 순서 §첫 실행 셸 설정 화면](../../architecture/boot-sequence.md)에 있다.

| 디자인 요소 | 공용 view | 비고 |
|---|---|---|
| 부팅 화면 채움(bg-app) + `Lockup` | `shell_setup_screen` — `brand::draw_wordmark`(로딩 화면과 같은 `loading_screen_wordmark_*` 크기와 `loading_lockup_tracking` 자간) | 락업과 폼 묶음을 세로 가운데에 둔다. 높이는 지난 패스에서 잰 값을 쓰고 바뀌면 패스를 다시 돈다 |
| 폼(`size-360` 폭, 항목 gap `space-sm`, 락업과 `space-xl`) | `draw_form` | 폭은 이름 붙은 상수 `SHELL_SETUP_FORM_WIDTH`(역할 토큰 없음) |
| 제목 14/600 text-primary · 부제 body text-muted line-height-ui | `draw_form` | 600 굵기는 크기와 색으로 근사([디자인 정합 지침 §타이포그래피](design-parity-notes.md)) |
| Windows Git Bash 안내(caption, accent-warning, `alertTriangle` icon-size-sm, 글리프 위 `size-1`, gap space-xs) | `notice_line` | 호출부가 Windows에서만 문구를 넘긴다. 기본 위치 탐색 결과에 따라 `git_bash_missing` 또는 `git_bash_required`다 |
| mono 경로 `Input`(block, control-height) | 공용 `Input` | |
| 검증 줄(caption, gap space-xs, 높이 = caption × line-height-ui 예약) | `check_line` — `ShellSetupCheck` 4판정 | 판정은 본체 `shell_check`가 한다 |
| 버튼 줄(오른쪽 정렬, gap space-sm, 위 `space-sm` 추가) Quit secondary · Use this shell primary md | `draw_form` | 확인은 판정이 유효할 때만 활성 |

갤러리는 시안 첫 Spec의 8장(Windows empty·missing·notShell·valid, macOS missing·valid, Latte Windows missing·macOS valid)에 시안에 없는 macOS 디렉터리·실행 권한 없음 2장을 더해 640×480 창에 그리고, 시안의 `windows-saved-shell-refused-git-bash-in-default-location` 4장을 같은 카드의 다음 Spec으로 그린다. Meta 행과 토큰 칩은 시안 문구를 옮긴다.

## 탭 스트립 툴팁 — 네이티브 콘텐츠 위로 (Layouts)

디자인 `gallery/layouts/tab-strip.jsx` Spec "Tooltips in the strip open upward — native content below" ↔ 갤러리 `catalog/widgets/html_script_banner.rs::draw_strip_tooltips`(Layouts › Tab strips, `tab-strip-tooltips` spec) ↔ 본체 `src/adapters/ui/tab_bar/tab.rs`의 표지 툴팁. 규칙은 [배너 시스템 §탭 스트립 툴팁](banner.md#탭-스트립-툴팁)에 있다.

| 디자인 요소 | 갤러리 | 본체 | 비고 |
|---|---|---|---|
| 창(`size-320` 폭, `border-frame` 테두리, `radius`) | `strip_tooltip_window` | Tasty 창 | 폭과 WebView 자리 높이 `size-96`은 예제 전용 이름 붙은 상수 |
| 제목 영역(`titlebar-height`, bg-app, mono micro text-muted) | 같은 함수 | 타이틀바 | |
| 탭 스트립(surface-raised, separator 아래 선) · 활성 html 탭의 lock(hover 채움) + 비활성 shell 탭 | `tab` 두 번 + hover 채움 | `html_script_marker` | |
| 위로 뜬 툴팁 | `Tooltip::placement_clear_of_native`(WebView 자리를 피할 영역으로, html 탭 칸을 앵커 칸으로 넘김) | 같은 함수, 피할 영역은 `MainViewState::native_content_rects`, 앵커 칸은 표지가 든 탭 칸 | Mocha·Latte 두 장 |
| WebView 자리(bg-panel, 가운데 mono micro text-muted) | 같은 함수 | 네이티브 WebView | |
| html pane 아래 html pane(위 pane WebView `size-64`(아래 separator 포함, border-box) → 탭 스트립 → 자기 WebView `size-64`) · 스트립 안에 뜬 툴팁 | `strip_tooltip_window(.., stacked = true)` | 같은 함수 | 앱 기하대로 자기 WebView가 스트립 바로 아래에서 시작한다(간격 0, separator 없음). 버블은 활성 탭 칸 오른쪽 `tooltip-offset`, 스트립 행 세로 가운데이며 행보다 1 높아 변마다 `border-width` 허용치로 통과한다. 높이 64는 예제 전용 이름 붙은 상수 |
| Stage(bg-app, 여백 `space-lg`, 오른쪽만 `size-120`) · 테마 카드 `Themed`(bg-app, `border-default` 테두리, `radius`, 여백·간격 `space-md`)를 세로로 쌓음 | `draw_strip_tooltips` · `themed_card` | 없음(예제 배치) | 쌓인 예제의 버블은 예제 창 밖 Stage 오른쪽 여백까지 나갈 수 있다. 앱은 앱 창 기준으로 clamp한다. 오른쪽 여백 120은 예제 전용 이름 붙은 상수 |

## 탭 스트립 스크롤 화살표 — disabled ink (Layouts)

디자인 `gallery/foundations/index.jsx` Spec "Disabled ink — no contrast target, Latte one step up"의 C4 행과 순서 사다리 ↔ 갤러리 `catalog/components/tab_bar.rs::draw_scroll_arrows`(Layouts › Tab strips, `tab-scroll-arrows` spec) ↔ 본체 `src/adapters/ui/tab_bar/view.rs`의 스크롤 화살표.

| 디자인 요소 | 갤러리 | 본체 | 비고 |
|---|---|---|---|
| C4 스트립(`control-height-tab` 높이, `size-288` 폭, surface-raised, radius-sm) | `scroll_strip` | 탭이 넘치는 pane의 탭 바 | 폭 288은 예제 전용 이름 붙은 상수 |
| 화살표 칸(`tab-scroll-arrow-width` 정사각, 자체 채움 없음, chevron `tab-scroll-arrow-glyph-size`) | `arrow_cell` | 같은 모양(공용 `paint_tab_scroll_arrow`) | 모양은 아래 "페인 탭 스트립 — 스크롤 화살표 모양"을 따른다 |
| `<` disabled · `>` enabled 잉크 | `tab_scroll_arrow_fg_disabled()` · `tab_scroll_arrow_fg()` | 같은 접근자 | 둘 다 component role. 값은 text-disabled · text-muted |
| 순서 사다리 placeholder < disabled < muted < secondary < primary | `ink_ladder` | 없음(규칙 전시) | 순서 규칙은 [theme 문서](theme.md) 대비 행 |

C3(port scanner 푸터) 행은 이 specimen에 넣지 않는다. Mocha·Latte는 고정 테마(`mocha_fallback`·`latte_theme`)로 나란히 그린다. C4 비율 문구(`(was …)` 포함)와 Meta 행·토큰 칩은 시안 문구를 그대로 옮긴다. 시안의 "pixels" 행은 시안 쪽 이전 가정(enabled = n800) 기준이라, 본체는 이미 text-muted였으므로 enabled 화살표 픽셀이 바뀌지 않았다.

## 페인 탭 스트립 — 스크롤 화살표 모양 (Layouts)

디자인 `gallery/layouts/tab-strip.jsx` Spec "Scroll arrows — chevron icon, square cell, no own fill" ↔ 갤러리 `catalog/components/tab_bar/kit_strip.rs::draw_scroll_shape`(Layouts › Tab strips, `tab-scroll-arrow-shape` spec) ↔ 본체 `src/adapters/ui/tab_bar/view.rs`의 스크롤 화살표.

| 디자인 요소 | 갤러리 | 본체 | 비고 |
|---|---|---|---|
| `Strip`(`size-560` 폭, 포커스 surface-raised · 비포커스 bg-sidebar, 아래 separator) | `strip` | 탭이 넘치는 pane의 탭 바 | 폭 560은 예제 전용 이름 붙은 상수 |
| `Arrow`(`tab-scroll-arrow-width` × `tab-height`, 채움 없음, chevron `tab-scroll-arrow-glyph-size`) | `arrow` → `paint_tab_scroll_arrow` | `tasty_ui_widgets::paint_tab_scroll_arrow`. 칸 폭은 zoom을 적용하지 않는 스트립 높이 `tab_bar_height` | enabled 쪽에만 `tab-scroll-arrow-hover-bg` |
| 도달한 끝은 disabled(`tab-scroll-arrow-fg-disabled`) | `arrow`의 `disabled` | `can_left`·`can_right` | disabled 쪽은 hover 채움·응답이 없다 |
| `TabCellS` 최소형(아이콘 · 제목 · 닫기 칸) | `tab_cell` | `tab_bar/tab.rs::draw_tab` | 두 번째 탭이 활성이다 |

Mocha·Latte는 고정 테마(`mocha_fallback`·`latte_theme`)로 위아래에 그린다. 행 다섯 개와 Meta 행·토큰 칩·Don't 문구는 시안을 그대로 옮긴다.

### 탭 칸 오른쪽 상태 묶음

디자인 같은 파일의 Spec "Tab cell — the right-hand status cluster" ↔ 갤러리 `kit_strip.rs::draw_status_cluster`(`tab-status-cluster` spec) ↔ 본체 `src/adapters/ui/tab_bar/tab.rs::draw_tab`.

| 디자인 요소 | 갤러리 | 본체 | 비고 |
|---|---|---|---|
| `TabCellS` 오른쪽 묶음 [표지][move][busy][close], 간격 `tab-status-gap` | `tab_cell`의 `slot` | 탭 칸 오른쪽 묶음 | 오른쪽 끝 `space-xs` 안쪽에서 왼쪽으로 칸을 잡는다 |
| 제목 → 묶음 간격 `tab-gap`, 제목이 먼저 말줄임 | `LayoutJob` 한 줄 `…` | `layout_tab_label` | 묶음 폭은 제목 길이와 무관하다 |
| close 칸 `tab-close-size`(활성·hover 전에도 자리 유지) | `CellCfg.active`·`hover` | 같은 조건 | 글리프 `icon-size-xs` |
| lock 칸 hover 채움 | `marker: Some((Blocked, true))` | `tasty_ui_widgets::html_script_marker` | lock hover도 탭 hover라 close가 보인다 |
| 좁은 탭 폭 `size-120` | `NARROW_TAB_W` | 설정의 탭 폭 | 예제 전용 이름 붙은 상수 |

행 여덟 개와 Meta 행·토큰 칩은 시안을 그대로 옮긴다. 탭 칸 툴팁 위치 Spec은 이 예제에 넣지 않았다.

### 숨은 이동 대상 화살표

디자인 같은 파일의 Spec "Move source scrolled out of view — the arrow on that side turns pink" ↔ 갤러리 `kit_strip.rs::draw_move_cue`(`tab-move-cue` spec) ↔ 본체 `view.rs`의 스크롤 화살표 잉크.

| 디자인 요소 | 갤러리 | 본체 | 비고 |
|---|---|---|---|
| 대상이 가려진 쪽 화살표 `tab-scroll-arrow-move-fg` | `StripCfg.mv` → `TabScrollArrowInk::Move` | 대상 탭 칸의 노출 판정 결과 | 잉크 우선순위 disabled > move > fg는 시안 `Arrow`와 같다 |
| 행 네 개(오른쪽 밖 · 왼쪽 밖 · 오른쪽 밖 hover · 비포커스 왼쪽 밖) | `MOVE_ROWS` | — | 시안 행을 그대로 옮긴다 |

화살표는 두 예제 모두 본체와 같은 `tasty_ui_widgets::paint_tab_scroll_arrow`로 그린다.

## Move source highlight (Layouts)

디자인 `gallery/layouts/move-source.jsx`(Layouts › Move source highlight, `#movesource`) ↔ 본체 `src/adapters/ui/{move_source,tab_bar,sidebar/view}.rs` ↔ 갤러리 `catalog/components/move_source.rs`(`movesource` 섹션). 링·글리프·레일 칩은 본체와 갤러리가 같은 `tasty_ui_widgets` painter를 호출한다. 주변 화면(사이드바·탭 바·서피스)은 갤러리가 정적 데이터로 흉내 낸다.

| 디자인 컴포넌트 | 본체 함수 | 갤러리 함수 | 비고 |
|---|---|---|---|
| `MoveRing` | `tasty_ui_widgets::paint_move_source_ring` ← `move_source::draw_move_source_ring`(서피스·페인), `tab_bar/view.rs`(탭 칸) | `move_source.rs::paint_surface`·`paint_pane`·`paint_tab` | rect 안쪽 2px 대시 4/4. 대상 rect에서 마지막에 그린다 |
| `MoveGlyph`(탭 칸) | `tab_bar/tab.rs::paint_move_glyph` → `paint_move_source_glyph` | `move_source.rs::paint_tab`(`cue`) | 제목 뒤 |
| `MoveGlyph`(워크스페이스 행) | `sidebar/view.rs::paint_move_source_row_glyph` | `move_source.rs::paint_ws_row` | 이름 뒤, 배지 묶음 앞 |
| `RailAvatar` 칩 | `sidebar/view.rs::draw_collapsed_avatar` → `paint_move_source_chip` | `move_source.rs::off_screen`(4c) | 왼쪽 아래 12px 칩, 글리프 8px |

색·치수는 `move_source_*` component 접근자와 `accent_move()`에서 읽는다([토큰 대응](design-token-mapping.md)). 대상 해석·수명·겹침 규칙은 [이동 기능 문서](../../features/surface-move/index.md#이동-대기-표시)에 있다.

## Task DAG — surface · canvas · node (Layouts)

디자인 `gallery/dag/index.jsx` (카탈로그 페이지) + `ui_kits/terminal/overlays/dag_view.jsx`
(상태 어휘 · 노드 카드 · 러너 배지 · 크롬 · 빈 상태) + `dag_surfaces.jsx` (캔버스 · 노드
상세 · 풀탭 서피스 · 워크스페이스 popup) ↔ 본체 `src/adapters/ui/surface/dag_graph/` +
`src/adapters/ui/popup/dag_list.rs` ↔ 갤러리 `catalog/components/dag/` (Layouts 페이지
`dag-graph` · `dag-phase` · `dag-shell` · `dag-list` 네 섹션).
디자인 회신(2026-10-07)의 부분 실패 rollup(`◒`·`dag-status-partially-failed*`·필터 7번째),
실행 중 세부 단계와 이유 줄(`nodeLook`·`nodeTitle`·상세의 입력 대기 알림과 `Why unknown`·목록 행의
`! n needs input`), 탭 머리글 숫자(고정폭 caption·`dag-header-count-fg`)를 갤러리 spec 과 본체에 반영했다.

| 디자인 jsx 컴포넌트 | tasty 함수 | 갤러리 항목 |
|---|---|---|
| `DagCanvas` | 공통 `tasty_ui_widgets::draw_graph_canvas` + `canvas::DagPainter` | `dag/canvas.rs::paint` → 같은 `draw_graph_canvas` + `SamplePainter` (`dag-canvas` spec). 상태별 모습은 `dag-graph-view` spec |
| `dagLayout()` | `tasty_dag_layout::layout_dag` | 동 crate 직접 호출 (미러 아님 — 아래) |
| `elbow()` | 공통 `graph_orthogonalize` + `graph_round_corners` | `dag/edges.rs::specimen` → 같은 함수 |
| `DagNode` | `node::paint_node` → 공통 `paint_dag_card` | `dag/node.rs::paint_card` → 같은 `paint_dag_card` (`dag-node`/`dag-kinds`/`dag-lod` spec) |
| `DAG_STATUS` / `DAG_KIND` / `DAG_REL` | `model::{DagStatus, DagRelation}` | `dag.rs::{Status, Kind, Rel}` |
| `DAG_PHASE` / `nodeLook` (실행 중 세부 단계) | `model::NodePhase` + `DagNodeData::{status_label, glyph}` + `node::node_colors` | `dag/phase.rs` 의 `Phase` + `Node::{status_label, glyph, accent, bg, label_fg, border}` (`dag-running-phase` spec) |
| `nodeTitle` (호버 이유 줄) | `DagNodeData::hover_text` (`DagPainter::hover_text` → 공통 캔버스의 노드 툴팁) | `Node::hover_text` (`dag-why` spec 에 펼쳐 전시) |
| `DagDetail` 의 입력 대기 알림 · `Why unknown` | `detail::awaiting_notice` · `detail::unknown_reason`. 세션 열기는 `DetailAction::OpenSession` → `draw_dag_graph` 반환값 → `RequestContext::reveal_surface` | `dag/detail.rs::draw_body` (`dag-why` spec 의 두 상세) |
| 전이 선택·미선택 노드(`routes` 섹션) | `model::{DagRelation, EdgeSelection}` + `node::paint_node` 의 skip 라벨·툴팁 | `dag/routes.rs::draw` (`dag-routes` spec) |
| `RunnerBadge` | `chrome::runner_badge` + `resume_hint` (헤더 우측) | `dag/runner.rs::paint_badge` + `row` (`dag-runner` spec) |
| 재개 힌트 캡션 | `chrome::resume_hint` — lead 비례폭 + 명령 mono 2 조각 | `dag/runner.rs::row` (동일 2 조각) |
| `ZoomCluster` | 공통 `tasty_ui_widgets::draw_zoom_cluster` (캔버스 우하단) | 같은 `draw_zoom_cluster` (`dag-chrome` spec) |
| `Minimap` | 공통 `paint_graph_minimap` (줌 클러스터 바로 위, 흐린 노드는 `dag-minimap-node-dim`) | 같은 `paint_graph_minimap` (`dag-chrome` spec) |
| `CycleBanner` | `chrome::draw_cycle_banner` | `dag/chrome.rs::paint_cycle_banner` (`dag-states` spec) |
| LOD 힌트 칩 | 공통 `graph_paint_lod_chip` (캔버스 안) | 같은 함수 (공통 캔버스 안) |
| `DagEmpty` | surface `chrome::draw_empty`, 목록 popup `popup/dag_list.rs::draw_empty` → 공용 `tasty_ui_widgets::paint_dag_empty` | `dag/chrome.rs::paint_empty` → 같은 `paint_dag_empty` (`dag-states` spec — surface는 포커스·비포커스 바탕 두 장, 사라진 DAG 한 장, popup·search·filter 변형은 `bg-panel` 위 세 장. 글리프는 모두 `glyph-dim`. 모두 본체 문구 키) |
| `DagDetail` / `DetailRow` / `LogBlock` | 틀은 공통 `split_graph_drawer` + `drawer_body`, 내용은 `detail::draw_detail` (`drawer_row` / `drawer_text_block`) | 틀은 같은 `drawer_body` + `paint_drawer_divider`, 내용은 `dag/detail.rs::draw_body` (`dag-detail` spec) |
| `DagSurface` | `render::draw_dag_graph` + `DagChrome::Surface` (헤더는 `chrome::draw_header`) | `dag/surface.rs::paint` (`dag-surface` spec). `focused`를 넘기면 캔버스가 `surface_bed`를 쓰고, 워크스페이스 팝업은 `dag-canvas-bg`를 유지한다 |
| `DagWindow` 디테일 back bar actions | `chrome::draw_detail_backbar_actions` (줌 클러스터 + 러너 배지) | `dag/window.rs::detail_view` (`dag-window-detail` spec) |
| `dagRowItems` (DAG 목록 행) | `popup::dag_list::draw_row_trailing` | `dag/rows.rs::trailing` (`dag-rows` spec) |
| `DagWindow` (워크스페이스 popup) | `popup::dag_list::draw_dag_list_popup` | `dag/window.rs::paint` (`dag-window` · `dag-window-detail` spec) |

**공통 뷰와 남은 미러**: 캔버스·카드·미니맵·줌 클러스터·LOD 칩·Drawer 틀은
[공통 그래프 뷰](graph-view.md)로 옮겨 본체와 갤러리가 같은 함수를 부른다. `render::draw_dag_graph`
는 `(ui, DagTarget<'_>, &mut DagGraphView, DagChrome)` 로 호스트 상태(폴링 스냅샷)와 본체 문구에
의존하고 갤러리는 main 바이너리를 의존할 수 없으므로, DAG 헤더 · 러너 배지 · 사이클 배너 · 상세
내용 · 간선 잉크 규칙은 갤러리가 같은 모양으로 다시 그린다. 다만 DAG 는 **좌표 계산만은 미러가 아니라
같은 코드**다: `tasty-dag-layout` 이 egui/Theme 를 모르는 순수 계산 crate 라 갤러리가 그대로
의존한다(`crates/tasty-gallery/Cargo.toml`). 디자인 jsx 의 `dagLayout()` 은 시안용 최단 구현
(longest-path + 중앙정렬)이라 sugiyama 결과와 좌표가 다르고, 갤러리는 **본체가 실제로 그리는
좌표**를 보여야 하므로 엔진 쪽을 따른다.

**디자인과 구현의 차이**

- **상태 글리프**: 시안의 `❯`(U+276F) `✓`(U+2713) `✗`(U+2717) 은 Dingbats 블록이라 UI 비례
  폰트에서 tofu 로 떨어진다. 본체는 기하·수학 기호(`◦ ▷ ◑ ● × − ⊘ ?`)로 치환했고 — 시안과 같은 `−`(cancelled)·`⊘`(skipped, 두 건너뜀 이유 공통)는 그대로 쓴다 —
  `crates/tasty-doc-guards/tests/design_token_adherence.rs::no_raw_pictographic_glyph` 가 그 블록을 host UI 소스에서
  금지한다. 갤러리도 같은 치환 세트를 쓴다 — 렌더되지 않는 글자를 전시하면 정합 판정 자체가
  무의미하기 때문이다.
- **건너뜀 이유 툴팁**: 시안 안에서 `routes` Spec 메타는 옛 문구("Not taken — another branch was
  selected." · "Skipped — {source} {state}.")를 적고, `nodeTitle` 코드와 2026-10-07 회신은 why 줄
  ("Why: Not selected by the upstream result" · "Why: An upstream task did not succeed")을 쓴다. 본체와
  갤러리는 `nodeTitle` 을 따른다.
- **대기 시간 표기**: 시안의 `{since}` 예시는 `2m` 이다. 본체는 기다린 시간을 노드 소요 시간과 같은
  형식(`format_duration_ms`, 예 `2m 4s`)으로 적는다.
- **상세 패널 캡션 대소문자**: 시안은 상세 캡션 전부를 CSS 대문자로 그린다(`WHY UNKNOWN`). 본체 상세는
  다른 캡션(`Command`·`Dependencies`)과 같이 번역 문구 그대로(`Why unknown`) 그리고, 갤러리 상세는
  시안대로 대문자로 그린다.
- **탭 머리글 숫자 문구**: 시안은 `{done}/{total} done`, 본체는 기존 번역 문구 `{} / {} done`
  (`dag.header.progress`)을 유지하고 글꼴·색만 시안에 맞췄다.
- **러너 재개 힌트 문구**: 시안은 `tasty dag runner start` 를 적지만 그런 CLI 는 없다. 본체와
  갤러리 모두 실제 명령(`tasty agent task-run --workspace <N> --action start`)을 쓴다.
- **기본 방향**: 시안 기본은 top-down, 본체 기본은 left-right(`DagDirection::LeftRight` —
  `agent.task_graph --format dot` 의 `rankdir=LR` 과 멘탈 모델 일치, 168×48 카드가 가로로
  길어 화면 폭을 아낌). 갤러리 specimen 은 시안대로 top-down 으로 전시한다.
- **줌 클러스터의 방향 아이콘**: fit 셀은 시안·specimen 과 같은 `move` 를 쓴다. 디자인의
  `fit` 글리프는 popup 전체화면 무대용이라 이 셀에 쓰지 않는다. 방향 셀은 시안의 `swap` 대신
  방향을 그대로 비추는 `arrow-right`/`arrow-down` 을 쓴다. `swap` 은 방향이 바뀐다는 것만
  말할 뿐 **지금** 어느 방향인지를 못 보여준다 — 방향 버튼은 눌러서 바뀔 결과가 아니라 현재
  상태를 읽는 쪽이 그래프와 대조하기 쉽다. 클러스터의 위치 · 크기 · 셀 구성(`− % + | fit dir`)
  · 토큰은 시안 그대로다.
- **popup 디테일 뷰의 크롬**(차이 아님 — 같은 함수의 두 갈래): 디테일에는 헤더가 없다.
  `DrillDown` 의 **back bar 가 그 화면의 크롬**이고, 그 actions 슬롯이 줌 클러스터(읽는
  순서로 줌 → 러너이므로 슬롯이 오른쪽부터 채워지는 것을 뒤집어 넣는다)와 러너 배지를
  든다. 캔버스 우하단 줌 클러스터도, DAG 선택기도 디테일에서는 안 그린다 — 한 DAG 를 이미
  고르고 들어온 화면이라 고를 것이 없고, 같은 조작을 한 화면에 두 번 두지 않는다.

  렌더는 여전히 한 벌이다(`render::draw_dag_graph`). 크롬을 누가 드는가만
  [`DagChrome`](../../../src/adapters/ui/surface/dag_graph/render.rs)(공통 `GraphFrame` 의 DAG 판)로 갈라, 탭 surface 는
  `Surface`(헤더 + 캔버스 오버레이), popup 디테일은 `Popup(이미 눌린 조작)` 을 받는다. back bar
  는 본문보다 먼저 그려지므로 조작은 그 프레임 안에 답이 나와 있고, popup 쪽이 그 값을 본문
  호출로 넘긴다.

  그래서 두 specimen 은 **차이가 아니라 두 갈래**를 전시한다 — `dag-surface`
  (`dag/surface.rs::paint`)가 `Surface`, `dag-window-detail`(`dag/window.rs::detail_view`)가 `Popup`
  이고, 둘 다 본체와 1:1 이다.

## 알림 요약과 상세 목록

| 디자인 | 공용 위젯·본체 | Rust 갤러리 |
|---|---|---|
| `feedback/NotificationBadge`, `terminal/notification_summary` | `tasty_ui_widgets::notification_summary`, sidebar full/collapsed 하단 | Layouts `notification-summary`: empty, mixed, 각 레벨, overflow, open 및 rail |
| `feedback/NotificationItem`, `terminal/overlays/notifications` | `tasty_ui_widgets::notification_panel`, notifications popup/fullscreen | Overlays notification panel 및 fullscreen stage |

배지는 error→warning→info 순서이고 0을 숨기며 `99+`로 제한한다. 요약의 최신 미확인 제목은 full에서만 표시한다. popup은 352×400, toolbar는 28이며 본문은 popup 2줄/fullscreen 3줄을 넘길 때 더 보기를 제공한다. 크기·색은 notification component 토큰을 사용한다. 정보 배지의 semantic 색은 completion attention과 같다. 알림 수와 surface attention 수는 합치지 않는다. 본체와 갤러리는 같은 위젯으로 행 펼치기·읽음·닫힌 출처를 표시하며 host가 읽음과 Jump를 도메인 동작으로 연결한다.

### 폴더 선택 모드

`gallery/parts/overlay-frames.jsx`의 `FilePickerFrame mode="folder"`는 기존 open/save의 `folderSel`과 별개다.
갤러리 `filepicker-folder-mode`(`components/file_picker/folder.rs`)는 현재 폴더·선택 폴더·비활성 파일·
빈 폴더·읽기 오류·로딩·Latte와 en/ko/ja의 긴 경로를 표시한다. 기본640×480, 설정 견본480×360,
최소폭320(일본어 견본 높이420)을 유지한다. `FolderPickerFooter`(`folder_picker` 모듈, crate 루트 재수출)를 실제 view와 공유한다.

읽기 전용 경로·안내·버튼의 3행이며 필터 칩은 없다. 경로는 앞 말줄임/전체 경로 툴팁이다.
경로/안내/파일 잉크는 각각 `fp-folder-path-fg`/`fp-folder-hint-fg`/`fp-folder-file-fg`이며 opacity를
낮추지 않는다. 로딩·오류에는 안내를 숨기고 확정을 막으며 경로는 `state-disabled-fg`로 그린다.
파일 행은 hover/선택 없이 툴팁만 제공한다. 확정 대상은 선택 폴더, 선택이 없으면 현재 폴더다.
Enter는 확정, 더블클릭/오른쪽 화살표는 진입, 재클릭/빈 곳 클릭은 선택 해제다.
