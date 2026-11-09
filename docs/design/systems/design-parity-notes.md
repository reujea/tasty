<a id="design-parity-히스토리--디자인htmlcss--구현winitegui-구조적-차이"></a>

# 디자인과 구현의 차이

디자인의 HTML/CSS 구조를 winit·egui로 옮길 때 주의할 점과 현재 구현의 제약을 설명한다. UI와 갤러리를 디자인에 맞추는 작업에서 함께 확인한다. 수치 예시는 해당 조건에서 확인한 값이며 다른 글꼴·배율에 그대로 적용하지 않는다.

## 구조 전사 (structural transcription) — 핵심 원칙

디자인의 grid·컬럼·패딩·정렬·요소 경계를 egui 레이아웃에 맞춰 옮긴다. egui의 기본 간격·정렬·크기 축소만으로 비슷하게 보이도록 맞추지 않는다.

구조와 토큰은 함께 확인한다. 색·폰트 크기·선 굵기·간격은 Theme에서 읽고 [테마 규칙](theme.md#ui-디자인-규칙-필수)의 4px 그리드·14px 폰트 상한·1px 보더를 따른다. 작업 순서는 [gallery-first](../../dev-guide/gallery-first.md)를 참고한다.

<a id="일반--egui-item_spacing-자동-삽입이-dividergap-을-밀어낸다"></a>

## egui 자동 간격과 명시한 패딩

egui는 위젯 사이에 `item_spacing`을 추가하므로 명시한 간격과 합쳐질 수 있다. 구역을 직접 배치할 때는 `item_spacing.y = 0`으로 두고 `add_space`나 Frame의 inner_margin으로 간격을 정한다. 구분선은 변경된 커서가 아니라 구역 Frame의 `rect.bottom()`에 그린다.

가로 간격까지 0으로 만들면 행 안의 텍스트가 붙는다. y만 바꾸거나 콘텐츠에 들어갈 때 원래 spacing을 복원한다.

<a id="일반--픽셀-검증-시-터미널-unfocused-배경이-popup-base-와-동색"></a>

## 같은 배경색을 쓰는 영역의 픽셀 비교

터미널의 비포커스 배경과 패널 팝업은 모두 `base`(#1e1e2e)를 쓸 수 있다. `bg-sidebar`(mantle)도 사이드바와 겹치므로 색 하나의 전체 범위로 팝업 경계를 정하지 않는다.

팝업 안에서 구분되는 띠나 구분선을 기준으로 삼고 측정할 x 범위를 제한한다. remote_tool에서는 탭바의 mantle 띠를 사용할 수 있다. x>700 같은 제한은 특정 캡처의 예시일 뿐 공통 기준이 아니다.

<a id="일반--ui_scalezoom이-popup-default_size-에만-곱해진다-비균일"></a>

## 팝업과 내부 콘텐츠의 배율

팝업의 `default_size`만 확대하고 내부 길이를 리터럴로 두면 UI 배율에서 비율이 달라진다. 기본 크기 비교는 `appearance.ui_scale="medium"`(1.0)으로 하되, 지원 배율에서 내부 내용이 함께 커지는지도 확인한다.

본체는 egui 전역 zoom 대신 배율을 적용하는 Theme 접근자를 사용한다. 토큰으로 전환하지 않은 호출부 리터럴은 여전히 누락될 수 있다. 이유와 검사 한계는 [타입 있는 길이 (Typed length)](../../concepts/typed-length.md)를 따른다. 검증 설정은 격리 인스턴스에서 바꾸거나 검사 뒤 복원한다.

<a id="remote_tool--css-line-height-vs-egui-텍스트-박스-높이-헤더-8px-얕음"></a>

## remote_tool — CSS 줄 높이와 egui 텍스트 박스

같은 폰트 크기라도 CSS line-height와 egui 텍스트 박스 높이는 다르다. 제목이 14px여도 디자인의 줄 높이는 약 24px, egui 박스는 약 18px일 수 있다.

헤더 행의 최소 높이를 디자인 콘텐츠 높이에 맞추고 바깥 보더도 포함해 비교한다. remote_tool의 기록된 보정은 최소 높이 26(콘텐츠 24 + Outside 보더 보정 2)과 탭 높이 36이며 구분선 y는 48.0·84.0이었다. 이 값을 다른 헤더의 기본값으로 사용하지 않는다.

<a id="remote_tool--popup-border-가-stroke-outside--콘텐츠가-1px-위에서-시작"></a>

## remote_tool — 바깥 보더와 콘텐츠 시작점

팝업 외곽선을 `StrokeKind::Outside`로 그리면 콘텐츠는 `popup_rect.top`부터 시작하고 보더는 그보다 1px 바깥에 있다. CSS의 `box-sizing: border-box`와 비교할 때 이 차이를 반영한다.

정밀한 좌표 비교에서 나타나는 1~2px 차이는 콘텐츠 시작점이나 최소 높이 계산에 포함한다. 보더 위치를 확인하지 않고 모든 요소에 일괄 보정값을 더하지 않는다.

<a id="command_palette--surface0-배경은-popup-밖에도-쓰여-색-bbox-가-오염된다"></a>

## command_palette — 배경색만으로 경계를 찾지 않는다

`surface0`은 팔레트 밖의 비활성 탭·호버·스크롤바에도 쓰인다. 해당 색의 전체 경계 상자로 팝업 폭을 재면 다른 위젯까지 포함한다.

팔레트 안의 전체폭 `surf1` 구분선을 기준으로 검색·목록·footer 영역을 나누고, 해당 x 범위에서 팝업 테두리를 찾는다.

<a id="command_palette--카드-높이는-콘텐츠를-따른다-popupdefsizer-는-매-프레임-돈다"></a>

## command_palette — 콘텐츠에 맞춘 높이 계산

`popup::frame::draw_popup_layer`는 매 프레임 `PopupDef.sizer`를 호출한다. 사용자가 직접 크기를 바꾸지 않은 팝업(`size_user_overridden`이 아님)에 결과를 적용한다.

`command_palette_sizer`는 현재 검색의 매칭 개수만 세어 `palette_height(theme, n)`을 계산한다. 라벨·아이콘·키캡을 만드는 처리를 중복 호출하지 않는다. 그리기와 크기 계산은 같은 `palette_footer_height`·`palette_row_height`·`palette_chrome_height`와 목록 상한 `Theme::palette_list_max_height()`(`palette-list-max-height` = size-320)를 사용한다. 행 높이는 `Theme.item_height_interactive`다. 크롬 높이는 검색 구역(사방 `space-md`) + 목록 위아래 여백(`space-sm` × 2) + footer이며, 목록과 footer 사이 별도 간격은 없다. 행 안 치수는 디자인 MenuItem과 같은 토큰이다. 좌우 여백 `menu_item_padding_x()`, 아이콘 `icon_glyph_size_md`, 아이콘–라벨 간격 `spacing_sm`, 강조 배경 반경 `menu_item_radius()`이며 갤러리의 `menu_item_kbd`와 같다. 검색 아이콘도 디자인 Input과 같은 `icon_glyph_size_md`다.

sizer가 있는 팝업은 등록 시 default_size에 UI 배율을 곱하지 않는다. 폭·footer 크롬 상수는 `zoomed_px`를 거치고, 목록 상한과 여백처럼 이미 배율이 적용된 Theme 값에는 다시 곱하지 않는다. 카드는 열 때 `request_center`로 중앙에 놓고 이후 높이가 바뀌어도 위쪽 위치를 유지해 검색창이 움직이지 않게 한다.

`command_palette.rs`의 `painted()` 테스트는 그려진 구분선 y를 확인한다.

<a id="port_scanner--테이블-컬럼-floor-가-footer-잘림의-근본-원인-디자인-table-구조-미준수"></a>

## port_scanner — 열 최소폭과 가로 스크롤

본문 테이블이 팝업보다 넓어져도 footer가 밀려나서는 안 된다. 열별 최소폭과 가변 여부는 공용 `PortsColumn`(`crates/tasty-ui-widgets/src/ports_table.rs`)이 정한다. 가변 열은 Process 하나이며 공용 Table 의 `TableColumnWidth::Flex`로 넘겨 남은 폭을 모두 받는다. Address 는 `port-addr-col-min-width` 폭에 고정된다. 최소폭 합이 본문보다 크면 `Table::horizontal_scroll(true)`로 본문만 가로 스크롤한다. 시안 Table 의 열 옵션 `minWidth`(고정 열은 `width`가 곧 하한)를 옮긴 것이다.

footer는 팝업 전체폭 사각형에 고정하고 sticky 헤더는 스크롤 콘텐츠와 수평으로 함께 이동한다. 과거의 최소폭 제거·무조건 말줄임 방식은 현재 규칙이 아니다. 열별 값은 `ports_table.rs`, 배치는 `src/adapters/ui/popup/port_scanner.rs`를 따른다.

<a id="port_scanner--테이블-헤더-th-배경mantle은-painter-로-직접-칠한다"></a>

## port_scanner — 헤더 배경

egui_extras Table의 헤더 배경은 별도로 그린다. TableBuilder 전에 `cursor.top`부터 `header_h`까지 전체폭 사각형을 계산해 `bg-sidebar`(mantle)로 채우고, 그 위에 sticky 헤더 텍스트를 표시한다.

<a id="port_scanner--테이블-셀-정렬패딩--footer-가-ui-폭-확장에-밀린다"></a>

## port_scanner — 셀 정렬과 footer 폭

셀 패딩과 정렬은 공용 그리기 함수에서 적용한다. 왼쪽 정렬 열은 콘텐츠 앞에 12px를 두고 Port는 `right_to_left`와 12px 패딩을 사용한다. 헤더도 같은 규칙을 따른다.

테이블의 컬럼 간 `item_spacing.x`가 전체 UI 폭을 늘릴 수 있다. footer를 그 UI의 오른쪽 끝에 붙이지 말고, 팝업 전체폭의 `UiBuilder::max_rect` 안에 그린다.

<a id="port_scanner--tasty-monod2coding가-디자인-폰트보다-넓다-셀-말줄임"></a>

## port_scanner — 고정폭 글꼴의 말줄임

같은 폰트 크기라도 D2Coding과 디자인 미리보기 글꼴의 글자 폭은 다르다. 예를 들어 디자인의 88px 열에 맞는 `127.0.0.1`이 실제 폰트와 좌우 패딩 24px를 적용하면 잘릴 수 있다.

현재 열 너비는 `PortsColumn`의 최소폭과 가로 스크롤 규칙을 따른다. 과거 측정에서 사용한 Tab 열 62→56 보정은 현재 코드에 적용할 처방이 아니다. 긴 문자열은 실제 글꼴로 배치한 결과를 확인한다.

<a id="egui-ui-의-mono-한-칸은-6px-다--공칭-advance-가-아니라-깔리는-advance"></a>

## 문자 폭은 실제 레이아웃으로 측정한다

문자 수로 폭을 제한할 때는 `glyph_width`나 한 글자의 galley 폭보다 한 글자 추가 시 늘어나는 폭을 사용한다. `"00"`의 폭에서 `"0"`의 폭을 빼면 레이아웃의 픽셀 반올림을 반영할 수 있다.

D2Coding 11px에서 확인한 예에서는 공칭 advance가 5.5556px, 한 글자 galley가 5.5625px였지만 추가 글자의 폭은 6px였다. 헤더 390px의 예산은 65자이며 공칭값으로 계산한 70자는 419.56px가 되어 29.56px를 넘었다.

폰트 설치 경로도 확인한다. `GpuState::setup_egui_fonts`와 `font_registry::build_font_definitions` 모두 번들 고정폭 글꼴을 우선해야 한다. 첫 프레임의 `set_fonts`가 다른 글꼴로 덮어쓰면 앞서 측정한 폭을 사용할 수 없다.

## port_scanner — 테이블 행 구분선이 divider 자동 측정을 교란

테이블의 행 구분선과 구역 구분선이 같은 `surf1`을 사용하므로 색만으로 구분하기 어렵다. 행에는 없는 헤더의 mantle 띠를 먼저 찾고 그 위·아래의 구역 경계를 비교한다. 패딩 값은 디자인과 코드의 정의를 대조하고 화면 캡처로 확인한다.

## remote_tool — 컨테이너 패딩 0 + 구역별 패딩 (통짜 패딩 금지)

헤더(왼쪽 14/오른쪽 12), 탭바(8), 목록(14)은 서로 다른 패딩을 사용한다. 팝업 전체를 같은 inner_margin으로 감싸지 않고 content_margin을 0으로 둔 뒤 각 구역의 Frame에서 적용한다.

탭바 배경은 전체폭 `rect_filled(mantle)`로 그린다. 자식 크기에 맞춰지는 Frame.fill만으로는 전체폭을 채우지 못할 수 있다.

<a id="공용-위젯-레이어-2026-06-21--primitive-컴포넌트화에서-얻은-것"></a>

## 공용 위젯과 CSS 표현

<a id="위젯의-집--cratestasty-ui-widgets-신규-디렉토리-아님"></a>

### 공용 위젯 위치: `crates/tasty-ui-widgets` (신규 디렉토리 아님)
갤러리(`tasty-gallery`)는 별도 크레이트라 메인 바이너리(`src/`)를 의존할 수 없다. 공용
위젯을 `src/adapters/ui/` 에 두면 갤러리가 또 mirror 를 떠야 한다. `tasty-ui-widgets` 는
본체와 갤러리가 함께 의존하며 `&Theme`을 명시적으로 받는다.
팝업과 갤러리가 같은 함수를 호출하므로 그리기 코드를 복사하지 않아도 된다.

<a id="egui-세금-디자인--즉시모드-변환-시"></a>

### CSS와 egui의 표현 차이
- **폰트 weight**(medium/semibold/bold): egui 는 별도 bold family 없이는 굵기 재현 불가. 워크스페이스 행의 running 개수(medium)도 크기와 색으로만 근사한다.
  크기+색(또는 `.strong()` 색 보정)만 따른다 — 디자인 weight 차이는 시각상 미세 손실.
- **radius-pill**(완전 둥금): egui CornerRadius 로 `height/2` 사용.
- **`color-mix(accent 40%, transparent)`**(Tag 상태 보더): `accent.gamma_multiply(0.4)`.
- **`::after` 오버레이 틴트**(Button/IconButton hover/active): pseudo-element 없음 →
  `rect_filled(overlay_*.to_egui_premultiplied())` 수동.
- **focus ring**(box-shadow 0 0 0 1px): box-shadow 없음 → `rect_stroke(outer.expand(bw),
  border_focus, Outside)`. **Motion 계약상 즉시**(focus-ring/invalid/checked 는 기능 → fade 금지).
- **separator 토큰**: 디자인 `--tasty-separator`(어두운 테마 white 8%, 밝은 테마 black 8%)는 premultiplied 색 `Theme.separator`로 저장한다. hline·테두리에는 `to_egui_premultiplied()`로 넘긴다. unmultiplied로 바꾸면 어두운 테마에서 white 8%가 어두운 회색으로 칠해져 배경과 거의 같아진다. 카드 안 구획선은 separator, 팝업의 바깥 경계는 `border-strong`이다. 둘을 바꿔 쓰지 않는다.
- **icon 글리프**: 위젯이 색을 상태별로 정해 `IconPainter` 클로저에 전달(아이콘 시스템은
  호출측 소유 — 본체 `icons::Icon`, 갤러리 mock 모두 동일 인터페이스).

### Motion 계약
rest/hover/active/focus/disabled **정지 상태가 canonical** — 파리티는 정지상태로 판정.
상태 사이 트랜지션(hover 틴트 fade)은 **장식** → 즉시모드 **스냅 허용**. 단 **기능적 외형은
즉시**(focus-ring 가시성, invalid 보더, checked/selected/active) — fade 금지. 터미널 0ms 별개.

### 검증
갤러리는 `TASTY_GALLERY_SHOT`의 GPU readback 캡처를 사용한다. 본체는 [격리 인스턴스](../../dev-guide/self-verification.md#독립-검증--개발도-agent-가-스스로-확인할-수-있어야-한다)에서 `ui.screenshot`과 `debug.host_popup.open`으로 확인한다. 공용 위젯도 실제 팝업에 넣은 상태에서 입력과 배치를 대조한다.

<a id="팝업--egui-area-미등록--scrollarea-스크롤-불가--클립-누출-2026-06-21"></a>

## 팝업 — Area 등록과 입력·클립 범위

팝업 콘텐츠를 `Ui::new(layer_id)`만으로 만들면 egui Area 목록에 등록되지 않는다. `Memory::layer_id_at`이 해당 레이어를 찾지 못해 ScrollArea가 포인터를 받지 못할 수 있다. 위젯 클릭은 별도 검사라 클릭만 동작하는 현상도 가능하다.

같은 layer_id의 `egui::Area`에 `movable(false)`·`sense(hover)`를 적용한다. 별도 배치한 footer까지 입력 영역에 포함되도록 `set_min_size(content_rect)`로 크기를 확보하고 `set_clip_rect(content_rect)`로 내용 유출을 막는다. 자세한 구조는 [팝업 구현](../../dev-guide/popup-implementation.md)의 콘텐츠 레이어 절을 따른다.

## port_scanner — State 컬럼 140px 에 가장 긴 라벨(ESTABLISHED)이 들어간다

State 셀은 `status_dot`(점 `status_dot_size` 8 + `space-xs` 4 + caption 11px proportional 라벨)이고, 폭은
`PortsColumn` 의 **최소폭** 140 이다(공용 Table 이 남는 폭을 flex 열에 나누고, 최소폭
합이 넘치면 가로 스크롤 — 위 전환 항목). 가장 긴 상태값 `ESTABLISHED` 는 egui 기본 proportional
폰트(Ubuntu-Light) advance 로 11px 에서 66.6px, 셀 전체 약 79px 이다. 1.2 배율(caption 13, 점 10, 간격 5)에서도
약 94px 라 140 안에 들어간다. 재는 법: `epaint_default_fonts` 의 `Ubuntu-Light.ttf` 로 문자열 advance
를 잰다(예: PIL `ImageFont.truetype(path, px).getlength("ESTABLISHED")`). 셀 폰트나 라벨 크기가
바뀌면 다시 잰다.

<a id="spinner--egui-엔-prefers-reduced-motion-매체-질의-없음--theme-이-실어-나름"></a>

## Spinner — 모션 감소 설정

- **증상**: 디자인 Spinner 는 `prefers-reduced-motion` 에서 회전을 멈추고 3-dot fallback 을
  쓰는데, egui 엔 그 매체 질의가 없다.
- **처방**: 설정값(`accessibility.reduced_motion`)을 `Theme.reduced_motion` 에 실어 나르고,
  `Spinner` 의 **기본 동작이 그것을 읽는다**. true 면 정지 3-dot. 빌더
  `Spinner::reduced_motion(bool)` 은 남아 있지만 이제 **설정을 무시하는 override** 이며,
  두 상태를 나란히 보여야 하는 갤러리 specimen 전용이다.
- **왜 파라미터가 아닌가**: 종전엔 호출부 파라미터였고, 그러자 실제로 넘기는 자리가 레포
  전체에 하나도 없어 설정을 켜도 스피너가 돌았다([`tasty-ui-widgets` — 본체·갤러리 공유 UI primitive](../../architecture/ui-widgets-crate.md)).
- **근거**: `crates/tasty-ui-widgets/src/spinner.rs`.

## CenterState — 가운데 계산과 시안 밖 요소

- **구조**: 시안 DS 컴포넌트 `CenterState`(`components/feedback/CenterState.jsx`)는 글리프·제목·보조
  줄의 세 요소만 flex 로 영역 가운데에 두고, 액션은 그 블록 아래에 절대 위치로 매단다.
  `tasty_ui_widgets::CenterState` 는 세 요소의 높이를 먼저 재고 `center-state-*` 토큰으로 직접
  배치한다. 블록 높이 상수는 없고, 받은 영역(file picker 본문 · remote attach 우측 pane)에서
  위아래 `space-md` 안쪽의 가운데에 놓인다. Settings › Misc › Scripts 에는 고정 높이 목록
  영역이 없으므로 블록 높이에 위아래 `space-md` 를 더한 자연 높이로 그린다.
- **보조 줄 슬롯**: 보조 줄이 없어도 캡션 한 줄을 예약한다. 예약 높이는 실제 보조 줄과 같은
  `LayoutJob` 경로로 잰 한 줄 galley 높이다. `Fonts::row_height` 는 소수점 아래가 달라
  loading 과 empty 사이에서 글리프가 0.17px 움직였다.
- **액션 버튼**: 부품의 슬롯이다. Secondary·`sm`(높이 24) 버튼을 보조 줄 슬롯 끝에서
  `center-state-action-gap`(12) 아래, 가로 가운데에 매단다. 가운데 정렬 계산에서 빠지므로 액션이
  있든 없든 글리프 위치가 같다. 영역이 낮으면 버튼이 아래쪽 안쪽 여백까지 내려가거나 영역 밖으로
  잘릴 수 있지만 글리프를 밀어 올리지는 않는다.
- **높이를 받지 않는 경로(시안 `natural`)**: `show(…, None)` 은 블록 위아래에 같은 띠를 둔 자연
  높이를 할당한다. 액션이 없으면 띠는 `space-md`(12)다. 액션이 있으면 띠가
  `space-md` + `center-state-action-gap` + `button-height-sm` = 48 이라 블록은 가운데에 남고, 액션은
  할당 영역 아래 끝에서 `space-md` 위에서 끝난다. 자연 높이는 블록 + 2 × 12, 액션이 있으면
  블록 + 2 × 48 이고 다음 위젯으로 넘치지 않는다. 글리프 위 36 의 빈 공간은 시안이 받아들인 값이다.
  지금 이 경로를 쓰는 호출부(Settings › Misc › Scripts)는 빈 상태뿐이라 액션이 없다.
- **글리프 선택**: 크기와 색은 위젯이 정한다. 오류 글리프도 부품이 정한다 — 모든 호스트에서
  `alertTriangle`(`CENTER_STATE_ERROR_GLYPH`)이고 `CenterState::error` 는 글리프를 받지 않는다.
  호스트는 빈 상태 글리프만 고른다(file picker `FOLDER_OPEN`, Scripts `SCRIPT`, remote attach
  초기 상태 `REMOTE`).
- **굵기와 배율**: 제목은 시안에 굵기 지정이 없어 기본 굵기다. 글리프 크기는 `ui_scale`
  0.85 / 1 / 1.2 에서 20 / 24 / 29 다. 시안 표기 20.4 / 28.8 과의 차이는 Theme 배율 반올림이다.
- **근거**: `crates/tasty-ui-widgets/src/center_state.rs`, `crates/tasty-ui-widgets/tests/center_state.rs`.

## Button variant — egui 엔 CSS variant 없음 → fill/stroke 수동 조합

- **증상**: 디자인 Button 의 primary/ghost 같은 variant 는 CSS 클래스로 갈리는데 egui 엔
  variant 개념이 없다.
- **처방**: variant 별로 fill·stroke 를 수동 조합해 그린다. remote_tool 은
  `primary_button`(accent fill) / `ghost_button`(투명 fill + 보더) 헬퍼로 분리.
- **근거**: `crates/tasty-ui-widgets/src/remote_tool.rs`. (primitive 레이어는 `tasty_ui_widgets::
  Button` 의 `ButtonVariant` 가 동일 역할.)

## 폼 라벨 — egui Grid 컬럼폭 고정 미지원 → 고정폭 우측정렬 흉내

- **증상**: 디자인 ProfileForm 은 `gridTemplateColumns: 112px 1fr` 로 라벨 컬럼이 112px
  고정·우측정렬인데, egui `Grid` 는 컬럼 폭을 고정값으로 못 박는다(콘텐츠 맞춤).
- **처방**: `field_label` 이 `allocate_ui_with_layout(112px, right_to_left)` 로 고정폭 우측정렬
  컬럼을 흉내(`LABEL_COL_WIDTH=112`). hint/error 는 `112 + columnGap(12)` 만큼 들여써 입력
  컬럼에 정렬.
- **근거**: `src/adapters/ui/popup/remote_tool.rs`.

## 설정 라벨 열 — 왼쪽 정렬 자식 영역은 내용 폭으로 줄어든다

- **증상**: 디자인 `Row` 의 라벨 열은 폭이 정해진 칸이라 라벨 길이와 관계없이 컨트롤이 같은 x 에서
  시작한다. egui `allocate_ui_with_layout(vec2(W, h), Layout::left_to_right(..))` 는 부모 커서를 자식의
  실제 내용 폭만큼만 옮겨, 라벨이 짧은 행의 컨트롤이 왼쪽으로 당겨진다. `right_to_left` 는 폭을
  유지하지만 라벨이 오른쪽에 붙는다. 칸 안에 `ui.put` 으로 위젯을 놓으면 부모 커서가 그 rect 뒤로
  되돌아가 같은 일이 생긴다.
- **처방**: `tasty_ui_widgets::settings_label_cell` 이 열 폭을 `allocate_exact_size` 로 정확히 할당하고,
  라벨(열 폭에서 줄바꿈한 galley)과 도움말 아이콘은 커서와 무관한 자식 Ui 에 놓는다. 열 폭은
  `settings_label_column` 이 서브탭 라벨을 재서 정한다. 칸과 컨트롤 사이는 `settings_label_gap`
  (가로 줄의 `item_spacing.x` 를 빼고 16)이다.
- **근거**: `crates/tasty-ui-widgets/src/settings_row.rs`, 시험 `crates/tasty-ui-widgets/tests/settings_row_grid.rs`.

## 설정 행 아래 글 — `set_max_width` 는 남은 폭보다 넓힐 수 있다

- **증상**: 행 아래 caption·callout 을 `ui.set_max_width(measure-md)` 로 감싸면, 창이 `measure-md`
  보다 좁을 때 자식 영역이 부모보다 넓어져 글이 그 폭에서 줄을 바꾸고 오른쪽 끝이 잘린다.
  숫자 칸의 입력 칸 + 단위를 `ui.horizontal` 로 두면 같은 폭에서 단위가 오른쪽 밖으로 나간다.
- **처방**: caption·callout 의 줄바꿈 폭은 `measure-md` 와 `available_width()` 중 작은 값이다.
  `number_field` 는 입력 칸과 단위를 `horizontal_wrapped` 로 두어 단위가 다음 줄로 내려간다.
- **근거**: `settings_row.rs::below_width`, `src/view/settings/ui/tabs/number.rs`. 시험
  `a_caption_and_a_callout_wrap_inside_a_narrow_area`, `the_suffix_wraps_under_the_field_in_a_narrow_row`.

## 좁은 칸의 글꼴 콤보 — 트리거를 남은 폭 안에 묶는다

- **증상**: Appearance › General 의 글꼴 콤보 칸이 `field_width_lg` 보다 좁아지면(좁은 설정 창) 트리거
  오른쪽 끝(chevron)이 행 밖으로 나가고, 그 자리를 다른 위젯이 덮으면 클릭이 그쪽으로 가 목록이 열리지
  않는다. 예전 egui `ComboBox::width` 는 최소 폭일 뿐이라 선택된 이름 폭만큼 늘어났다.
- **처방**: 트리거 폭을 `field_width_lg` 와 남은 폭 중 작은 값으로 정한다. `SearchableSelect` 트리거는 긴
  이름을 말줄임하고 늘어나지 않는다. egui `TextEdit` 은 원래 남은 폭을 넘지 않아 같은 줄의 입력칸은 손대지
  않는다. 미리보기는 옆 열이 아니라 글꼴 행 아래에 있다.
- **근거**: `src/view/settings/ui/tabs/appearance.rs` 의 `font_family_picker`, 시험 모듈
  `default_font_section_tests`(트리거·입력칸이 미리보기 위에서 끝나고 트리거가 `field_width_lg` ×
  `select_height` 인지, chevron 자리를 누르면 목록이 열리는지).

## footer 우측정렬 — flex `justify-end` 흉내

- **증상**: 디자인 footer 버튼군은 `justify-content: flex-end` 로 우측에 붙는데 egui 엔
  flex justify 가 없다.
- **처방**: `Layout::right_to_left(Align::Center)` 로 우측부터 배치(먼저 add 한 위젯이 우측
  끝). port_scanner footer·remote_tool form 액션이 이 패턴.
- **근거**: `src/adapters/ui/popup/remote_tool.rs`, `crates/tasty-gallery/src/catalog/components/
  port_scanner.rs`. (우측 기준 레이아웃을 ui 폭 확장과 무관하게 고정하는 건 위 port_scanner
  footer 항목 참고.)

## 타이포그래피 — font-weight 세분화 미지원 (letter-spacing · line-height 는 지원된다)

- **증상**: 디자인 토큰엔 letter-spacing(ui 0 / caps 0.04em)·line-height(tight 1.0 / term 1.2
  / ui 1.4 / prose 1.6)·세분 font-weight 가 있다. 셋 중 **막힌 것은 weight 하나**다 — egui 는
  별도 bold family 없이 굵기를 재현하지 못한다(위 "공용 위젯 레이어 — 폰트 weight" 항목).
- **앞의 둘은 API가 있다**: `RichText::extra_letter_spacing` / `RichText::line_height` 와
  `TextFormat` 의 같은 이름 필드. 둘 다 px 를 받으므로 em·배수 토큰은 폰트 크기를 곱해
  넘긴다. em 자간 토큰은 생성 Theme 접근자(`generated_tracking.rs` 의 `letter_spacing_caps(font_size)`
  등)가 그 곱을 돌려준다. `LogicalPx` 상수로는 생성하지 않는다(`Skip::EmUnit`). 자간이
  빠진 자리가 남아 있다면 값이 아니라 그 자리에 아직 적용하지 않은 것이다.
- **처방**: weight 는 크기+색으로 근사한다. 자간은 생성 접근자로, line-height 는 값이 준비되면 해당 API로 적용한다.
- **디자인 확인**: 권한 안내 모달의 제목(600)·도입부(semibold)·경로(medium)는 굵기 없이
  `text-primary` 색으로만 본문과 구분하며, 디자인이 이 근사를 받아들였다. 굵은 UI 글꼴을
  등록하기 전까지 다른 자리도 같은 근사를 쓴다.
- **근거**: `crates/tasty-gallery/src/catalog/typography.rs` — specimen 은 지금 weight 축만
  기록하고 letter-spacing·line-height 토큰 값은 아직 싣지 않는다.

## settings_window — 디자인 flex Row 의 gap 은 모든 자식 사이에 적용된다

- **증상**: settings 폼 Row(`Accent:` 등)에서 Input↔색스와치 간격을 `add_space(space-sm)` 로
  주면 디자인보다 좁다(또는 row item_spacing 과 겹쳐 과넓음).
- **원인**: 디자인 `Row` 는 `display:flex; gap:16` 이고 children 이 fragment(`<Input/><span
  swatch/>`)면 **label·Input·swatch 가 전부 형제 flex 자식** → 셋 사이 간격이 모두 16. 한
  쌍만 좁게 본 것은 오독.
- **처방**: row 진입 시 `ui.spacing_mut().item_spacing.x = space-lg(16)` 한 번만 설정하고
  형제 위젯 사이엔 `add_space` 를 추가하지 않는다(item_spacing 이 곧 flex gap). label 은
  `allocate_exact_size(150)` 고정컬럼 + `painter().text` 로 그려도 다음 위젯에 item_spacing 이
  붙는다.
- **근거**: `crates/tasty-gallery/src/catalog/widgets/layout_2depth.rs` 2026-06-24. settings_window.jsx `Row`.

## gallery — tasty-gallery 는 i18n(t()) 을 쓰지 않는다 (specimen 하드코딩 라벨)

- **증상**: 갤러리 specimen 에 `t()` 를 적용하려다 의존성/관례 충돌.
- **원인**: `tasty-gallery` 는 `tasty-i18n` 에 의존하지 않는다(Cargo.toml). 모든 specimen 이
  하드코딩 mock 영문 라벨을 쓴다(Storybook 류 격리 시각 카탈로그). CLAUDE.md 의 `t()` 규칙은 *본체 shipping UI* 대상이며 갤러리
  specimen 은 범위 밖.
- **처방**: 갤러리 specimen 라벨은 대상 디자인 jsx 의 영문 라벨을 그대로 미러(하드코딩). i18n
  의존 추가는 단일 specimen 작업 범위 밖.
- **근거**: `crates/tasty-gallery/Cargo.toml`(i18n 미의존) + 30+ 기존 specimen 관례. 2026-06-24.
## Kbd 우측정렬 — `ui.horizontal` 이 부모 RTL 을 상속해 키캡 순서 역전

- **증상**: command 행에서 `Kbd`("Ctrl+Alt+G")를 `with_layout(right_to_left)` 안에서 그리면
  키캡이 "G + Alt + Ctrl" 로 뒤집혀 렌더된다.
- **원인(검증)**: egui `Ui::horizontal` 은 부모의 `prefer_right_to_left()` 를 상속한다. RTL
  부모 안에서 `kbd`(내부 `ui.horizontal` 사용) 를 호출하면 키캡 시퀀스가 RTL 로 배치된다.
- **처방**: RTL 레이아웃은 그대로 쓰되 **그 안에 들어가는 키캡을 하나로 제한한다.** 키캡이
  하나면 시퀀스가 없으므로 역전될 것이 없다. 여러 키캡의 조합(chord)은 RTL 밖에서 그린다.
- **근거**: 우측 Kbd 를 그리는 두 자리가 모두 이 형태다 —
  `crates/tasty-gallery/src/catalog/components/modifier_hint.rs`(`hint_row`, 행 키캡은
  `sec.chord` 접두를 뗀 leaf 한 개) 와 `src/adapters/ui/modifier_hint_overlay.rs`(`draw_row`,
  `binding_leaf` 가 modifier 를 벗긴 leaf 한 개). 조합 전체를 보여주는 자리는 RTL 밖의
  `chord_head` 이고, 그래서 `kbd_parts` 가 부모 RTL 을 상속해도 뒤집힐 시퀀스가 없다.
  폭을 재서 spacer 로 미는 방식은 채택되지 않았다(그런 헬퍼는 저장소에 없다).

## color-mix(in srgb …) 재현 — lerp / alpha 헬퍼

- **증상**: 디자인이 아바타·배너에 `color-mix(in srgb, C 18%, surface)`(불투명 블렌드)와
  `color-mix(in srgb, C 11%, transparent)`(알파 감소)를 쓴다. egui 엔 color-mix 가 없다.
- **처방**: 두 케이스를 분리. 불투명 블렌드 = srgb 바이트 선형보간 `mix_srgb(a, ratio, b)`
  (비율이 가운데 인자다 — `a` 를 `ratio` 만큼 `b` 에 섞고, 결과 알파는 배경 `b` 의 것을 따른다).
  transparent 믹스 = 색은 그대로 두고 알파만 얹는다(= C 를 알파 t 로). 후자는 Tag 위젯의
  `gamma_multiply(0.4)`(=border 40% transparent) 와 같은 의도.
- **근거**: 두 케이스가 각각 구현돼 있다. 불투명 블렌드는
  `crates/tasty-type-appearance/src/theme.rs` 의 `mix_srgb(a, ratio, b)` — 그 파일의 wash
  접근자들이 유일한 호출자다. 알파 감소는 `HexColor::with_alpha`(`crates/tasty-type-appearance/src/color.rs`)
  로 상수 알파를 얹는 형태이고, 최종 변환 `HexColor::to_egui`(같은 파일)가 sRGB 바이트에 알파를
  곱해 premultiplied 색을 만든다. egui 렌더러가 sRGB 바이트 공간에서 합성하므로 CSS
  `color-mix(in srgb …, transparent)` 를 배경에 올린 값과 같다. 선형 공간에서 곱하는
  `Color32::from_rgba_unmultiplied` 는 어두운 배경에서 진하게 나온다(accent 12% 가 (43,48,70) 대신
  (75,93,136)). Tag 의 `gamma_multiply(0.4)` 는
  `crates/tasty-ui-widgets/src/chip.rs` 의 `TagVariant::Info` 보더에 살아 있다.

---

<a id="일반--component-tier-디자인-토큰은-신규-theme-필드를-만들지-않는다-semantic-접근자-직접-매핑"></a>

## component 토큰과 Theme 접근자

기존 semantic 값의 별칭인 component 토큰은 해당 Theme 접근자를 사용한다. 예를 들어 `button-primary-bg`·`checkbox-bg-checked`·`switch-track-bg-on`은 `accent_primary()`를 공유한다. 같은 의미의 필드를 중복해서 만들지 않는다.

반면 고유 치수를 가진 component 토큰에는 자체 Theme 경로가 필요할 수 있다. `component.fp-crumb-max-width`는 `generated_component.rs`의 `fp_crumb_max_width()`에서 UI 배율을 적용한다. component라는 이유만으로 접근자 생성을 금지하거나 숫자가 같은 다른 토큰으로 대신하지 않는다.

[토큰 매핑](design-token-mapping.md)과 [테마 가이드](theme.md)의 생성 길이 상수 규칙에 따라 기존 매핑·생성 접근자·수기 접근자를 확인한다.

<a id="갤러리--inline-키캡-위젯은-좌표-slot-에-못-끼운다--painter-갈래를-공유-위젯에-둔다"></a>

## 갤러리 — 좌표 기반 키캡 그리기

- **증상**: switch-number overlay 는 탭 스트립/사이드바 행 중간의 *정해진 16px slot*(아이콘/
  dot 자리)에 키캡을 그려야 한다. 본체 `kbd()`(`crates/tasty-ui-widgets/src/chip.rs`)를 그대로
  부르고 싶지만, kbd 는 `ui.horizontal` + `allocate_exact_size` 로 **자체 레이아웃 흐름에 inline
  배치**하는 위젯이라 임의 좌표 slot 에 끼울 수 없다.
- **원인**: 갤러리 mock(tab_bar/sidebar specimen)은 `ui.painter_at(rect)` 로 좌표 painting 한다.
  inline 위젯(kbd)과 좌표 painting 은 배치 모델이 달라 섞이지 않는다.
- **처방**: 그리기를 좌표 기반 함수 `paint_num_keycap`(`chip.rs`)으로 뽑아 두고, inline 위젯
  `num_keycap` 은 자리를 할당해 그것을 부른다. 갤러리 `keycap_at` 과 본체 `paint_keycap` 이 같은
  함수를 부르므로 그리기를 공유한다. 색·치수는 `switch-overlay-*` component 토큰에서 온다.
- **구현**: `crates/tasty-gallery/src/catalog/components/switch_overlay.rs`와 본체의 `paint_keycap`이 같은 공용 함수를 사용한다.

---

## switch-number overlay — modifier 상태 소스 & 이벤트 구동 redraw

- **증상**: "modifier 를 누르고 있는 동안" 만 보이는 오버레이는 (a) 무엇으로 modifier 상태를
  읽을지, (b) 다른 키 입력 없이 modifier press/release 만으로 redraw 가 도는지가 관건.
- **원인/사실(검증)**:
  - tasty 는 `WindowEvent::ModifiersChanged` 를 egui 에 전달하고(`src/view/main.rs` 의 그 분기),
    egui 가 반환하는 `repaint` 가 true 면 `mark_dirty()` → RedrawRequested → `run_egui_frame`.
    즉 **bare Ctrl press/release 도 redraw 를 유발**한다(별도 배선 불필요). focus 상실 시
    `base.modifiers = empty()` (`src/view/main.rs`) 로도 정리되고 egui 도 동일.
  - draw 단계 modifier 소스는 **egui `ctx.input(|i| i.modifiers)`** 가 가장 깔끔. winit→egui
    raw_input 으로 들어온 **실제 사용자 입력만** 반영 → IPC/에이전트가 raw_input에 사용자 입력을 주입하는 release API는 없다. tasty `base.modifiers`(MainView) 를 draw 까지
    따로 전달할 필요는 없다.
- **처방**: 공통 모듈 `src/adapters/ui/switch_overlay.rs` 에 ① modifier↔대상 판정
  (`switch_target_for`, numeric.rs 규칙 1:1; 사이드바용 얇은 래퍼 `workspace_switch_held`) ②
  키캡 painter(`paint_keycap`) 를 모은다. wrapper 가 **순수 view props 로 전달**(view 는 settings
  비의존 유지, model-view-split).
- **focused pane 한정(탭)**: 탭 전환 단축키는 focused pane 의 탭만 전환하므로, 키캡도 focused
  pane 의 탭바에만 그린다. `tab_bar.rs` wrapper 는 `ctx.input` 대신 `state.switch_overlay()`
  스냅샷(`Tab` 대상일 때 focused pane id 동봉)에서 `switch_overlay_pane: Option<u32>` 를 뽑아
  `PaneTabBarsProps` 로 넘기고, view 는 `tab_keycap_for(switch_overlay_pane, pane_id, i)` 로 매칭되는
  pane 에서만 키캡(비-focused pane 은 held 여도 아이콘 유지). 사이드바는 워크스페이스 전역 전환이라
  pane 한정이 없어 `ctx.input` modifier → `workspace_switch_held` bool 직접 사용.
- **근거(2026-06-27)**: `tab_bar.rs` (`PaneTabBarsProps.switch_overlay_pane`), `accessors.rs`
  (`switch_overlay()`), `switch_overlay.rs` (`tab_keycap_for`). P2b 사이드바도 같은 모듈의
  `workspace_switch_held`/`workspace_digit`/`paint_keycap` 재사용.

---

## command_palette — 키캡은 본체·갤러리가 **같은 `Kbd` 함수**를 부른다

본체의 `draw_keycaps`와 갤러리의 `menu_item_kbd`는 공용 `tasty-ui-widgets::kbd_parts_at`을 호출한다. 이 함수는 `kbd_parts`와 같은 토큰 및 `kbd_item_widths` 계산을 사용한다.

일반 메뉴의 고정폭 단축키 텍스트는 `menu_item`, 키캡 조합은 `menu_item_kbd`로 구분한다. 기존 텍스트 호출자의 인자 형식을 바꾸지 않는다. 키캡은 `kbd-size` 16·`kbd-padding-x` 4·`kbd-gap` 3·`kbd-font-size` 10을 따르며 28px 행의 높이와 정렬을 유지한다.

<a id="sidebar-카테고리-헤더--패딩-대칭화--고아-구분선-제거-2026-07-02-디자인-변경-반영"></a>

## sidebar — 카테고리 헤더와 구분선

카테고리 헤더는 상하 `spacing_xs`(4)로 대칭 배치한다. 그룹 컨테이너의 위 패딩은 `spacing_sm`(8), 첫 그룹을 제외한 그룹 간 간격도 8이다. 그룹 목록은 헤더 아래의 상단 구분선만 그리고 평면 모드는 상·하 구분선을 유지한다. 레일은 이 규칙의 변경 대상이 아니다.

`draw_category_header`와 그룹 루프가 이를 적용한다. 섹션 시작 위치를 기록한 뒤 그룹 간 간격을 추가해 해당 간격도 드롭 영역에 포함한다. 스크롤 시작의 8px와 New Workspace 앞의 4px도 유지한다. 사이드바 헤더 패널은 위 `spacing_md`(12), 아래 `spacing_xs`(4)를 사용하며 첫 카테고리 헤더 위 간격은 12다.

갤러리 `full_categories`도 같은 헤더 패딩·그룹 간격·상단 1px 구분선을 표시한다.

## preset 편집기 — 정적 specimen 은 존/× hover·crosshair 를 재현 못 한다

- **증상**: 갤러리 `preset_editor` specimen 의 편집 직접조작(경계 split 존·mini tab close ×·
  add-tab +)이 본체 `demo_layout.rs` 의 live 동작과 100% 동형이 아니다.
- **원인**: 갤러리 specimen 은 binary 미의존 **정적**(Theme-only) 렌더라 마우스 hover·pointer
  추적·커서 아이콘이 없다. 경계 split 존은 커서 위치로 활성 변을 고르고(`pick_zone`) crosshair
  커서로 바뀌며, tab × 는 `active || hover` 일 때만, add-tab hover fill 도 실시간 pointer 로
  결정되는데 — 정적 캔버스엔 이 입력 축이 존재하지 않는다.
- **처방(전사)**: specimen 은 이 상태들을 **고정 상태 예시**로 전사한다 — `draw_edit_direct_mock`
  이 Left 존을 활성 예시로 항상 그리고(`draw_split_zone_overlay_mock`), 탭 하나는 active 의 ×
  rest 상태, 다른 하나는 hover 상태(overlay_active fill), add-tab 은 hover fill 상태로 굳혀
  보여준다. crosshair 커서는 정적에서 표현하지 않아 생략(밴드+2px 분할선 시각만 전사). 색·치수는
  본체와 **동일 토큰**(`preset_split_zone_bg/border`, `overlay_active/hover`, 14×14 ×, 22×20 +,
  30% 밴드)이라 구조·토큰 축은 정합하고, 오직 "입력 상태 전이"만 정적↔live 로 갈린다.
- **근거**: `gallery/preset_editor.jsx` (`SurfaceBox`/`pickZone`/`AddTabBtn`).

## preset 편집기 surface 설정 화면 — 시안과 다르게 둔 자리

시안은 `gallery/preset_editor.jsx` 의 `SurfaceSettings` 다. 구조(세 상자 · 한 열 폼 · 고정 footer)와
토큰(`preset-cfg-*` 8종)은 그대로 전사했고, 아래만 갈린다.

- **kind 전환은 값을 지우지 않는다.** 시안의 `switchKind` 는 새 kind 가 선언하지 않은 키를 비운다.
  본체 `LeafDraft::switch_kind` 는 값을 draft 에 남겨 둔다 — 두 kind 가 함께 선언한 키가 이어지는
  것은 같고, 원래 kind 로 돌아오면 원래 값이 다시 보인다. 정리는 확인 시점에 **최종 kind 가
  원본과 다를 때만** 한 번 한다(`DemoLayout::apply_leaf_draft` → `set_kind`). 시안처럼 전환마다
  지우면 kind 를 잠깐 바꿨다 되돌리는 것만으로 원본 params 가 사라진다. 비어 있고 `default` 가
  있는 필드는 전환 시 그 값으로 채워 보인다 — 확인 시 `set_kind` 가 채울 값과 같다.
- **kind 가 같은 확인은 선언되지 않은 params 를 보존한다.** 시안의 확인은 `normalize(draft)` 로
  surface 를 통째로 바꿔 선언되지 않은 키를 지운다. 본체는 선언 필드만 덮어쓴다 — 편집기의
  round-trip 계약(`set_field_writes_param_and_preserves_unknown_params`)이다. dirty 판정은 시안과
  같다(kind + 현재 kind 의 선언 키).
- **필드 타입은 본체가 선언하는 넷(text · file_path · dir · url)뿐이다.** 시안 데모 kind
  `plugin:portscan` 의 `number`(addon `ms`)·`select` 필드는 본체 `PresetFieldInput` 에 없는 타입이라
  갤러리 specimen ③ 도 text 입력으로 그린다. 두 타입을 들일지는 별도 결정이다. 같은 이유로 그
  plugin kind 의 accent 는 본체 `kind_accent` 의 중립 fallback(text-secondary)이다.
- **Kind 드롭다운 autofocus 없음.** 시안은 진입 시 Kind `Select` 에 autofocus 한다. 공용 `select`
  위젯은 키보드로 조작되지 않아(`Sense::click()` 뿐) 포커스를 받을 자리가 없다.
- **breadcrumb 말줄임은 조각별이 아니라 전체 꼬리다.** 시안은 각 조각이 `minWidth:0` 으로 함께
  줄어든다. 본체는 조각을 ` › ` 로 이은 한 라벨을 남은 폭에서 `truncate` 한다.
- **잠금 디밍은 리스트·L1 탭의 내용에만 걸린다.** 시안은 컨테이너(`bg-sidebar` 배경 포함)에
  opacity 를 준다. 본체는 배경을 먼저 칠하는 셸 구조라 그 위의 행·탭 글자에 `set_opacity` 를 건다.
  입력 차단은 시안의 `pointer-events: none` 대신 그 영역 위에 나중에 얹은 막(`block_input`)이
  hover·click 을 받고, 리스트 스크롤도 끈다.
- **4px 그리드로 맞춘 간격.** 헤더 unsaved 묶음, mini tab 아이콘↔라벨, 미리보기 pane 사이
  split 공백은 시안과 같이 `space-xs`(4)를 쓴다. unsaved 점은 `status-dot-size-compact`(6),
  라벨↔입력 `gap: 3` 은 `STRUCT_GAP_3`, 선택 leaf 핸들 사이 `gap: 2` 는 `STRUCT_GAP_2` 다.
  시안 편집 툴바는 "saved automatically" 앞에 `accent-success` 체크 글리프를 `space-xs` 간격으로
  그린다. 본체 툴바는 문구만 그리고 갤러리에는 이 툴바 예제가 없어 이 간격이 적용될 자리가 없다.
- **근거**: `gallery/preset_editor.jsx` (`SurfaceSettings`/`useSurfaceCfg`/`switchKind`/`normalize`).
- **결정 근거**: kind 전환 · default 선채움 · autofocus 없음 · 저장 실패 시 화면 유지 · 더블클릭 범위의
  근거 · 대안 · 재검토 조건은 [레이아웃 프리셋 (Layout presets)](../../features/layout-presets/index.md).

<a id="explorer-gridcell--아이콘-축소--파일명-3줄-wrap-말줄임-2026-07-09-디자인-확정-반영"></a>

## Explorer — 아이콘과 여러 줄 파일명

파일명은 폭 기준으로 최대 3줄을 표시하고 마지막 줄을 말줄임한다. `LayoutJob`의 `halign: Align::Center`, `TextWrapping { max_width: CELL_W - spacing_xs*2, max_rows: 3, overflow_character: Some('…') }`를 사용한다. `p.galley()`는 라벨 블록 위쪽에서 그려 각 행을 가운데 정렬한다.

`CELL_W`는 80, 유효 라벨 폭은 72다. 줄 높이 `round(11×1.3)`인 14를 3줄 예약해 짧은 이름도 같은 셀 높이를 사용한다. 아이콘과 라벨 간격은 `spacing_xs`(4), 블록 상하 간격은 `spacing_sm`(8)이다.

아이콘은 `icon_glyph_size_md`(16), 라벨은 `font_size_caption`(11)을 사용하고 사용자 Explorer 글꼴 크기도 caption 상한으로 제한한다. 선택한 라벨은 `text_on_active`, 나머지는 `text_secondary`다. 폴더·파일 글리프는 text-muted이고 선택 행에서는 행 잉크다. 이미지는 `accent_info_raised`이고 선택 행에서는 `accent_info_on_active`다. 호버·잘라내기 표시는 기존 규칙을 따른다.

`Galley::text()`는 원문이므로 말줄임 여부를 확인할 수 없다. 실제 배치 결과의 `galley.rows[].glyphs[].chr`와 캡처를 확인한다. 구현은 `src/adapters/ui/surface/explorer.rs`와 갤러리 `explorer_view_cells.rs`의 `grid_cell()`에 있다.

## Explorer — 검색 결과 메뉴

갤러리 `explorer-context`의 search result 메뉴는 시안대로 Open · 들어 있는 폴더에서 보기(`folderOpen` 글리프) · 구분선 · Copy path를 그린다. 본체의 검색 결과 메뉴는 같은 순서로 열기 · 들어 있는 폴더에서 보기 · 구분선을 맨 앞에 둔다(`src/view/main/explorer_show_in_folder.rs`). 차이는 둘이다.

- **글리프**: 본체 메뉴는 네이티브 메뉴라서 행에 글리프가 없다.
- **나머지 행**: 본체는 구분선 뒤에 Copy path부터 이어지는 일반 파일 메뉴의 행을 그대로 둔다. 시안 표본은 Copy path 한 행만 그린다. 네이티브 메뉴에서 나머지 행을 남길지는 디자인 요청 중이며, 답을 받기 전까지 본체는 행을 줄이지 않는다.

원격 탐색기에는 열기와 들어 있는 폴더에서 보기를 붙이지 않는다.

## Explorer — 원격 쓰기와 응답 대기

시안 `gallery/explorer-ops-remote.jsx` 의 D1 은 쓸 수 없는 원격의 이유를 둘로 나눈다. 옛 Tasty(`explorer.remote.write_needs_update`)와 화면 없는 server(`explorer.remote.write_headless`, "{host} can't change files (server without a window)")다. 본체와 갤러리는 앞의 것만 쓴다.

- **화면 없는 server**: headless 부팅도 attach holder 가 보낸 explorer 파일 작업을 GUI 와 같은 `remote_file_jobs` 로 실행한다(`src/boot/headless_stream.rs` 의 `apply_explorer_ops`). 그래서 attach descriptor 는 원격 쓰기 capability 를 늘 싣고, 화면 없는 server 라서 쓰기를 거절하는 상태가 없다. `write_headless` 키와 그 상태를 알릴 신호는 두지 않는다. 쓰지 않는 키는 `i18n_key_parity` 가 미사용으로 잡는다.
- **D2 이름 변경 팝업**: 시안은 rename 팝업을 dialog 로 보고 바탕을 dialog-bg(bg-panel)로 두며, 입력은 그 위 공용 Input 의 기본 바탕 모양이다(평소 input-bg·input-border, 대기 중 input-readonly-bg·border·fg). raised 바탕 변형은 surface-raised 위 칸에만 쓴다. 본체 rename 팝업(workspace·탭·카테고리·탐색기 항목·즐겨찾기 추가 공용)의 바탕도 dialog-bg 다. 콘텐츠는 본체와 갤러리 예제가 함께 부르는 공용 view(`tasty_ui_widgets::rename_popup`)가 그린다. 입력은 공용 Input(input-height 28 · 좌우 input-padding-x · 세로 가운데, 대기 중 readOnly, 검증 오류 시 invalid 테두리)이라 글자·선택·caret 이 상자 테두리와 안쪽 여백을 침범하지 않는다. 치수는 대상별 시안을 따른다. 워크스페이스·부제·탭·카테고리는 `RenameFrame`·`CategoryEditFrame`(네 변과 입력 → 버튼 사이 size-14, 버튼 md, 오류 줄 caption accent-danger), 탐색기 항목·즐겨찾기 추가는 `RenamePending`(위아래 space-md · 좌우 size-14, 행 사이 space-sm, 버튼 sm)이다. 폭은 두 시안 모두 `size-360` 으로, 역할 토큰이 없어 화면 전용 고정 치수에 UI 배율을 곱한다. size-14 는 디자인 토큰 `explorer-props-padding-x` 를 쓴다(토큰 주석이 이 popup 계열 — Properties · 충돌 · rename — 의 안쪽 여백으로 정의). `CategoryEditFrame` 의 오류 줄 위 6px(`overlays-shared.jsx` `CategoryEditFrame` 의 `marginTop: 6`)는 대응하는 역할 토큰이 없어 본체의 기존 space-xs(4)를 유지한다. 높이는 직전 프레임에 그린 콘텐츠 높이에 타이틀바를 더한다(첫 프레임은 오류·대기 줄이 없는 높이). 오류·대기 줄이 생기거나 사라지면 그 프레임에 다시 그려 저장/취소 버튼이 잘리거나 빈 자리가 남지 않는다. 공통 popup 내부 여백은 적용하지 않는다. 시안 `RenameFrame`·`RenamePending` 평소 팝업의 Kbd 안내 줄("Press ↵ to confirm, Esc to cancel.")은 본체 rename 팝업에 없으며, `explorer-rename-popup-ground` 예제도 본체를 따라 그리지 않는다.
- **D2 입력 줄**: List·Detail 은 입력 칸을 Spinner 자리(space-sm + icon-size-sm)만큼 줄이고 그 뒤에 Spinner 를 둔다. 폴더 안 만들기의 caption 은 Spinner 뒤로 밀린다. Grid 칸에는 Spinner 자리가 없어 입력 칸 오른쪽 밖에 그린다.

## Explorer — 남은 임시 폴더 카드

시안 `gallery/explorer-ops-staging.jsx`(#stagingleft)는 준비용 임시 폴더를 지우지 못한 카드의 규칙을 Meta 로 정한다: 종류는 항목 결과와 Warning 중 높은 쪽, 제목은 항목 결과 그대로, 폴더 줄은 항목 줄 뒤에 오고 "and N more" 에 센다, 동작은 Copy paths(폴더 포함)·허용될 때 Undo 이며 폴더를 Retry 하지 않는다. 본체와 갤러리는 이 규칙을 따르며 표본과 다른 점은 다음이다.

- **부분 실패 카드**: 실패 줄 셋이 줄 상한 3을 채우면 폴더 줄은 "and 1 more" 에 든다. 표본은 같은 제목("3 failed")에서 폴더 줄을 보이고 실패 하나를 줄였다. 규칙 문장과 표본이 다르므로 규칙을 따랐다.
- **취소 카드**: 본체 Retry 는 취소로 하지 않은 항목을 센다(`Report::retryable`). 그래서 표본에 없는 Retry 28 이 붙는다.
- **버튼 순서**: 본체 카드 순서(Retry · Copy paths · Undo)를 그대로 쓴다. 표본은 Undo 를 Copy paths 앞에 둔다.
- **언어**: 표본은 ko·ja 복사 카드 한 장씩만 보인다. 갤러리는 네 카드를 en·ko·ja 세 열로 모두 그리며, 문구는 본체와 같은 `lang/*.toml` 키를 읽는다(`crate::i18n::t_lang`). 경로와 OS 오류는 실제 카드처럼 번역하지 않는다.

<a id="pathfield--autocomplete--go-합성-공용-위젯-편집이동원복-결정-포팅-2026-07-09"></a>

## PathField — 편집·이동·원복

- **무엇**: 두 주소창(Explorer / Markdown)이 공유할 편집형 경로 필드를 `tasty-ui-widgets` 에
  신설(`path_field.rs`). 디자인 `plugins.jsx` `PathField`(:59) 전사 — 트리거 = `AutoComplete`
  (Input 언어 + 후보 드롭다운) + 우측 Go `IconButton`(sm, arrow-right). idle=mono text-secondary,
  editing=text-primary + focus ring + caret(Input 기본).
- **구조 축**: 디자인 `PathField` 는 `editing && candidates` 면 `<AutoComplete withGo …/>`, 아니면
  필드 div + `<IconButton Go/>` 두 브랜치다. 소스 `AutoComplete` 에는 `withGo` 가 없어(markdown 이
  Go 를 따로 그렸음) PathField 가 **AutoComplete + Go IconButton 을 `ui.horizontal` 한 행에** 합성
  한다: 필드폭 = 총폭 − control-height(sm 28) − `spacing_sm` 간격. 드롭다운은 트리거 rect 아래
  floating(AutoComplete 소유)이라 Go 버튼과 겹치지 않는다.
- **토큰 축**: 색·간격·행높이 전부 `theme.*` accessor — 필드 fill=`surface-raised`(input-bg),
  idle=`text-secondary`, editing=`text-primary`, match=`accent-primary`, Go 버튼=`IconButton`(sm)
  자체 토큰. raw px/`from_rgb` 0. 신규 Theme 필드 0(AutoComplete/IconButton 토큰 재사용).
- **결정 로직 포팅**: markdown `addr_outcome(action, lost_focus)` → 위젯 순수함수 `decide(action,
  lost_focus, go_clicked)`. 우선순위 **Esc(Cancel) > Pick(행 확정) > Submit(버퍼) > Go 클릭 >
  확정없는 blur(원복) > None**. Go 클릭은 같은 프레임 `lost_focus` 를 유발하지만 이동 확정이므로
  blur-원복보다 앞선다(이 순서가 회귀 방지 핵심 — 단위테스트 `decide_go_click_navigates_buffer_over_blur_revert`).
  상태(buffer/editing/active)는 호출측 소유, 위젯이 매 프레임 `&mut` 갱신(글로벌 상태 0).
- **갤러리 예제의 제한**: editing 의 focus ring/caret 은 실제 포커스에서만 Input 이 그린다 → 정적
  specimen 은 focus 테두리를 못 고정한다. `prim_path_field` 는 idle/editing+list 를 정적 전사(필드
  행 + `autocomplete_dropdown`)하되, 실제 편집·포커스링·키내비·이동/원복은 **라이브 `PathField`
  인스턴스**(context 별 click-to-edit)로 노출한다(gallery-first).
- **근거**: 디자인 `gallery/plugins.jsx` `PathField`(:59). 소스
  `crates/tasty-ui-widgets/src/path_field.rs`, specimen
  `crates/tasty-gallery/src/catalog/components/prim_path_field.rs`. 소비 화면에서는 상태와 이동 처리를 별도로 연결한다.

<a id="transfer-팝업--scrim_backdrop-스테이지가-카드보다-짧으면-클러스터가-겹친다-2026-07-23"></a>

## transfer — 가변 높이 카드의 갤러리 배치

`scrim_backdrop`은 고정 영역을 확보하고 카드를 오버레이로 그린다. 가변 높이 카드가 영역보다 크면 부모의 배치 커서가 늘어나지 않아 다음 예제와 겹친다.

갤러리 transfer 예제는 scrim 스테이지 없이 클러스터에 Frame을 직접 배치해 실제 높이만큼 공간을 확보한다. 본체의 scrim은 `draw.rs`가 담당한다. 파일 피커 예제도 같은 방식을 사용한다. 구현은 `crates/tasty-gallery/src/catalog/components/transfer.rs`다.

<a id="tab_bar--attention-kind-도입으로-옛-값-보존-divergence-가-해소됨-2026-08-10"></a>

## tab_bar — 주의 환기 종류별 색

탭 제목의 NeedsInput은 `tab_fg_needs_input()`, Completion은 `tab_fg_completion()`을 사용한다(각각 attention-needs-input·attention-completion). 입력 요청과 작업 완료를 구분하는 색이며 별도의 값 보존 예외를 두지 않는다.

구현은 `src/adapters/ui/tab_bar/tab.rs`의 `text_color`와 `AttentionKind` 분기다. 값과 대응은 [토큰 매핑](design-token-mapping.md#attention-kind--needsinputcompletion-surface-highlight-docs/dev-guide/agent-runner.md), [갤러리 대응표](design-gallery-mapping.md#attention-kind--needsinput-배지dot테두리탭-제목-surfaces-docs/dev-guide/agent-runner.md)를 따른다.

<a id="pluginavatar--색은-갈래가-하나뿐이고-글리프는-상한에서-잘린다-2026-09-08"></a>

## PluginAvatar — 카테고리와 글자 크기 제한

디자인 `plugins_window.jsx` 의 `PluginAvatar` 를 전사하면서 **의도적으로 갈린 두 자리**다.
전사 자체는 구조·토큰 두 축 모두 디자인을 따른다(사각 `size`, `radius`,
`color-mix` 배경/보더, mono 머리글자).

- **색 — 카테고리 갈래가 도달 불가**: 디자인은 배경·보더·글자를 `CAT_COLOR[plugin.cat]`
  으로 칠하고, 그 표에 없으면 `var(--tasty-accent-primary)` 로 떨어진다. tasty 매니페스트
  스키마(`crates/tasty-plugin-manifest/src/types.rs` 의 `Manifest`)에는 **카테고리 필드가
  없다** — `id` · `name` · `version` · `authors` · `description` · `homepage` 뿐이다. 그래서
  일곱 카테고리 색 중 어느 것도 도달할 수 없고 fallback 갈래 하나만 남는다. 위젯이 색을
  인자로 받지 않는 이유가 그것이다: 부를 수 있는 값이 하나뿐인 인자는 호출부 넷에서 같은
  상수를 다시 적게 만든다. 매니페스트에 카테고리가 생기면 바뀌는 것은 두 `Theme` 접근자와
  위젯 시그니처뿐이다.
- **글자 색**: 머리글자는 `plugin_avatar_fg()`(= `accent-primary` fallback 갈래)로 칠한다.
- **글리프 크기와 굵기**: 머리글자는 목록 `plugin-avatar-initial-font-size-sm`
  (→ `font-size-body` 13)과 상세 `plugin-avatar-initial-font-size-lg`(→ `font-size-max` 14)를
  토큰 접근자 `plugin_avatar_initial_font_size_sm()`·`_lg()` 로 읽는다. 둘 다 UI 폰트 상한 14
  안이라 상한 예외가 없다. 굵기 토큰 `plugin-avatar-initial-weight` 는 `font-weight-normal` 이고
  위젯도 보통 굵기 mono 로 그린다. 사각형 한 변 46 은 그대로다
  (docs/design/systems/theme.md 대로 44 · 48 로 스냅하지 않는다).
- **근거**: `crates/tasty-ui-widgets/src/plugin_avatar.rs` (위젯 · 두 갈래의 유일한 구현부),
  `crates/tasty-type-appearance/src/theme.rs` 의 `plugin_avatar_bg` · `plugin_avatar_border`
  (위 color-mix 절의 처방 그대로 — 불투명 블렌드는 `mix_srgb`, `transparent` 항은 알파만).
