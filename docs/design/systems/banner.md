# 배너 시스템

배너는 소속 영역 상단에 안내와 즉시 할 수 있는 조치를 표시하는 오버레이다. TUI가 마우스를 캡처(DECSET 1000/1002/1003)해 드래그 선택이 막혔을 때 이유와 우회 방법을 알리는 것이 한 예다. Popup·Toast와 별도 관리자를 사용한다. 용어는 [통합 용어집](../../concepts/ubiquitous-language.md)을 따른다.

색과 치수는 아래 Theme 접근자로 읽는다. 숫자를 호출부에 직접 쓰지 않는다.

## 정체성 — 왜 별도 개념인가

배너는 안내와 바로 할 수 있는 조치를 함께 보여 준다. 마우스 입력과 버튼을 지원하지만 키보드 포커스를 가져가지 않는다.

| 동작 | Banner | Toast | Popup |
|---|---|---|---|
| 마우스 | 자기 카드 영역에서 소비 | 통과 | 소비 |
| 키보드 포커스 | 없음 | 없음 | 있음 |
| 내부 버튼 | 있음 | 없음 | 있음 |
| 위치·이동 | 부모 상단 고정 | 범위별 고정 스택 | 이동 가능 |
| 수명 | 동작이 없으면 TTL 뒤 자동 소멸, 동작이 있으면 TTL 없이 동작이나 ×까지 유지 | 자동 소멸 | 사용자가 닫음 |

이 차이를 유지하려고 별도 BannerManager를 둔다. popup의 타이틀바·이동·포커스 기능을 여러 옵션으로 끄거나 입력이 통과하는 toast에 버튼을 넣지 않는다.

## 포지셔닝 — Popup / Banner / Toast

Popup은 독립 기능, Banner는 안내와 조치, Toast는 짧은 정보 표시를 담당한다. 모두 부모 영역 위에 표시하지만 역할이 다르다. 배너에 단순 정보를 넣을 수도 있으나 내용이 짧고 조치가 없으면 Toast를 사용한다.

## 위치 규칙

배너 대상 스코프는 **View(최상위) / Workspace / Pane / Tab / Surface** 중 하나다. 공통 원칙은 하나다:

> **배너는 탭 바(탭 영역)를 가리지 않는다.** 가리면 탭 전환이 막히기 때문이다.

배치는 두 부류로 나뉜다.

### ① Workspace / Pane / Tab / Surface 배너 — "탭 바 바로 아래"

네 스코프 모두 **"탭 바 바로 아래 = 콘텐츠 영역 최상단"** 을 기준으로 상단 margin 을 두고 뜬다.

- Workspace / Pane 은 **탭 바 하단** 기준, Tab / Surface 는 **자기 영역 최상단** 기준이지만 — Tab/Surface 영역의 최상단이 곧 탭 바 아래이므로 **네 스코프 모두 사실상 같은 y 위치** 다.
- Workspace 영역은 그 워크스페이스에 보이는 pane 사각형 전체의 합이다. 사이드바와 제목 표시줄은 포함하지 않는다.
- 스코프 간 차이는 **가로 폭이 어느 영역의 100% 인지**(그리고 좌우 clamp 경계)뿐이다.

### ② View / Modal 배너 — 플레이스홀더

각 View 가 지정한 **배너 플레이스홀더** 위치에 뜬다.

- **View 배너**: 워크스페이스에 **종속되지 않고** View 자체에서 띄운다. **모든 View 구현체(`MainView`/`SettingsView`/…)는 배너 표시 위치 플레이스홀더를 가져야 하며**, 각 View 가 자기에게 알맞은 곳에 지정한다. 워크스페이스 전환과 무관하게 View 위에 유지된다.
- **Modal 배너**: Modal 은 View 의 한 형태(`SettingsView`/`QuitView`/`PluginsView`)이므로 위 플레이스홀더 규칙에 포함된다. Modal 이 전역 입력을 독점하는 상태에서도 그 Modal 의 플레이스홀더 위치에 배너가 떠야 한다 — 배너는 Modal 의 입력 차단보다 **위 레이어** 에서 자기 영역의 마우스를 소비한다.

## 형태

- **floating overlay** — parent 영역을 나눠 차지하지 않고 그 **위에 떠서 덮는다**(Toast/Popup 과 동일). 배너 height 이외의 모든 공간이 그대로 하단 콘텐츠 공간이 된다.
- 너비: parent 폭 **100% − 좌우 margin**.
- margin: **상 8px / 좌 8px / 우 8px**, **하단 margin 없음**(`spacing_sm`).
- border-radius: **4px** — `banner_radius()`(`--tasty-banner-radius` → 시스템 기본 반경).
- 높이: **콘텐츠에 따라 가변** — 각 배너 구현체가 자체 결정. 시스템은 "프레임/셸"(`banner_shell`) 과 내부 패딩(좌우 `spacing_md` 12 / 상하 `spacing_sm` 8) 규칙만 정의.
- host 배너의 본문(제목 + 설명)은 줄 사이를 `banner_text_gap()`(2)로 두고, 앞 글리프는 `banner_glyph_offset()`(1)만큼 내려 제목 줄에 맞춘다.
- 제목의 디자인 굵기(semibold·600)는 재현하지 않는다. egui UI에 굵은 글꼴을 등록하지 않으므로 제목은 `banner_title_font_size()`(13)와 `banner_fg()`(text-primary)로, 본문은 `banner_body_font_size()`(11)와 `banner_body_fg()`(→ `text_muted_raised`)로 구분한다. Latte에서는 본문과 제목의 글자색이 같아 크기와 굵기로만 구분한다. 디자인이 받아들인 크기·색 근사([디자인 정합 지침 §타이포그래피](design-parity-notes.md#타이포그래피--font-weight-세분화-미지원-letter-spacing--line-height-는-지원된다))이며 HTML 스크립트 차단·자동 attach 거절·크기 동기 실패 배너 모두 같다.
- 배경 / 보더 / 그림자: **Theme 토큰** — `banner_bg()`(→ `surface_raised`/surface0) 배경 + 1px `banner_border()`(→ `border_strong`) 보더 + `shadow_popover()`(= `--tasty-shadow-popover`) 그림자. 본문 색은 `banner_fg()`(→ text_primary), leading 글리프 기본색은 `banner_icon_fg()`(→ text_muted, 심각도 배너는 override), 카운트다운은 `banner_countdown_fg()`(→ text_muted). 하위 스코프 디밍은 `opacity_recessed()`(0.4), 페이드 모션은 없다(`banner_fade()` 는 생성만 되고 소비처가 없다).

### 본문 줄 수

- Tasty 고정 문구의 배너(자동 attach 거절, 크기 동기화 실패, 마우스 캡처, 셸 통합 누락, HTML 스크립트 차단)는 제목과 본문의 줄 수를 제한하지 않는다. 문구가 길거나 스코프가 좁으면 카드가 늘어난다.
- plugin이 제공한 본문의 줄 수는 min(3, 콘텐츠 높이에 들어가는 줄 수)이다. 넘치면 마지막으로 보이는 줄 끝을 말줄임한다. 매니페스트 `size_hint`로 고정한 높이가 세 줄을 담지 못하면 담기는 줄까지만 보인다. 한 줄도 담지 못해도 한 줄은 그린다. plugin은 배너 콘텐츠를 자기 mesh로 그리므로 호스트가 이 규칙을 강제하지 못한다. 공용 위젯 `tasty_ui_widgets::plugin_banner_body`(상한 `PLUGIN_BANNER_BODY_MAX_ROWS` = 3, 디자인 규칙 상수이며 토큰 없음)를 쓰면 규칙을 따른다.
- 잘린 plugin 본문의 전문은 호스트가 툴팁으로 보인다. 포인터가 본문 위에 `tooltip_delay()`만큼 머물면 배너 카드 아래 `tooltip_offset()`(4)에, 왼쪽을 본문 왼쪽에 맞춰 공용 Tooltip(`tooltip_max_width` 240)을 그린다. plugin mesh 밖에 그리므로 콘텐츠 영역에 잘리지 않고 포인터를 덮지 않는다. 전달 경로는 [egui-mesh 채널](../../dev-guide/egui-mesh-channel.md#잘린-본문의-툴팁--호스트가-그린다)에 있다.
- 호스트 채널 밖에서 위젯을 쓰면 위젯이 본문 아래에 툴팁을 띄우고 위로는 띄우지 않는다. 화면 맨 위의 본문에서 위로 띄운 버블이 화면 안으로 당겨지면 포인터를 덮어 hover가 끊기기 때문이다.
- HTML 스크립트 차단 배너가 늘어나면 inset 배치라 WebView 자리가 그만큼 아래로 밀린다. 액션 버튼은 줄을 바꾸지 않는다.

### 배너 위 버튼

배너 배경은 Secondary 버튼의 기본 채움과 같은 surface-raised다. 그래서 배너 안의 Secondary 버튼은 한 단계 올린 박스를 쓴다. 채움은 `banner_button_bg()`(→ surface-hover), 테두리는 `banner_button_border()`(→ border-frame)이며 hover에서도 테두리를 유지한다. 이 규칙은 특정 배너가 아니라 배너 표면 전체의 문맥 규칙이다(디자인의 `data-surface="banner"`).

- `banner_shell`은 콘텐츠 클로저를 `tasty_ui_widgets::banner_surface` 안에서 그린다. 그 안에서 그린 `Button`의 Secondary 변형은 자동으로 배너 박스를 쓴다. 셸 밖에서 배너 콘텐츠를 그리는 위젯은 `banner_surface(ui, |ui| …)`로 직접 문맥을 켠다.
- 문맥은 클로저가 끝나면 이전 상태로 돌아가고 중첩할 수 있다. 다른 변형(Primary·Ghost 등)과 disabled 버튼은 영향을 받지 않는다.
- 배너 콘텐츠는 누가 그리든 항상 배너 문맥 안에서 그린다. plugin 배너는 플랫폼이 프레임 전체에 문맥을 연다. plugin SDK의 `EguiMeshBanner::run_frame`이 배너 전용 egui `Context`의 매 프레임을 `tasty_ui_widgets::banner_surface_ctx`로 감싸므로, 작성자가 감싸지 않은 공용 Secondary 버튼도 배너 박스를 쓴다. 같은 plugin의 surface·popup은 각자 다른 `Context`라 문맥이 적용되지 않는다. 문맥 키는 `tasty-ui-widgets`가 혼자 소유하고, SDK는 egui-mesh 기능에서만 이 크레이트를 의존한다.
- 디자인 원문: "A banner's content is always drawn inside the banner context. For plugin banners the platform opens it for the whole frame, so a plain Secondary button gets the banner box. The context never applies to the same plugin's surfaces or popups."

### 좁은 스코프

배너가 놓인 스코프의 폭이 `banner_narrow_below()`(440)보다 좁으면 액션 버튼을 본문 아래 줄로 내린다. 닫기(×)는 액션이 아니라 닫기 표지라 내려가지 않고 카드 오른쪽 위에 남는다.

- 판정은 `tasty_ui_widgets::banner_is_narrow(scope_width)`가 한다. 기준 폭은 배너 margin을 빼기 전 스코프 폭이다. Surface 배너는 surface 폭, Pane·Tab 배너는 그 pane의 탭 바 아래 콘텐츠 폭, Workspace 배너는 보이는 pane 전체를 합친 영역의 폭이다. 스코프 크기가 바뀔 때마다 다시 판정하고, 내용 길이로 임계값을 바꾸지 않으며, 배치 전환에 애니메이션을 두지 않는다.
- 줄 사이는 같은 `banner_gap()`이고, 내린 버튼은 본문 왼쪽 가장자리(글리프 + `banner_gap()`)에서 시작한다. 좁은 배치에서 글 열은 오른쪽 위 닫기 칸(`banner_gap()` + 닫기 버튼 폭)을 뺀 폭을 쓴다.
- 다시 시도 중(비활성 + 앞 Spinner, 라벨 유지)과 Retry all 도 내린 줄에서 같은 모양이다.
- 내용 안에 ×를 그리는 배너(자동 attach 거절, 크기 동기 실패)와 셸 모서리 닫기 슬롯을 쓰는 배너(HTML 스크립트 차단) 모두 액션 버튼만 내리고 ×는 오른쪽 위에 남는다. 앞의 두 배너는 공용 행 배치(`ActionRow`)가 넓을 때 버튼과 ×를 나란히, 좁을 때 따로 놓는다.
- 본체 BannerManager는 슬롯마다 스코프 폭으로 판정해 거절 배너와 크기 동기 실패 배너에 넘긴다. HTML 스크립트 차단 배너는 surface 폭으로 판정한다(아래 inset 배치).

### inset 배치

tasty가 위에 그릴 수 없는 네이티브 레이어(HTML surface의 WebView 등)를 콘텐츠로 가진 스코프는 floating 대신 inset 배치를 쓴다. floating 배너는 네이티브 레이어 아래에 가려지기 때문이다. inset은 일회성 예외가 아니라 배너 시스템의 정식 배치이며, 그 밖의 스코프는 floating이 기본값이다.

- 셸·패딩·닫기 슬롯은 floating과 같다. 같은 `banner_shell`을 쓴다.
- margin: **상·좌·우 8px(`banner_margin()`) + 하 8px(`banner_inset_gap()`)**. 네 변 모두 margin을 둔다.
- 콘텐츠 rect가 배너만큼 줄어든다. 네이티브 콘텐츠는 배너 카드 아래 `banner_inset_gap()` 뒤에서 시작하고 배너가 콘텐츠를 덮지 않는다.
- 배너를 표시하거나 치울 때 콘텐츠 rect는 한 번 바뀐다. rect 변화에 애니메이션을 두지 않는다.
- 기하는 `tasty_ui_widgets::inset_banner_zone`(배너를 둘 영역)과 `inset_content_rect`(카드 아래 콘텐츠 rect)가 계산한다.
- 첫 사용처는 HTML surface의 스크립트 차단 안내다. 스코프는 그 surface 하나이며 옆 surface에는 배너가 없다. 배너 view는 `tasty_ui_widgets::html_script_banner`, 닫거나 허용한 뒤 탭 라벨 뒤에 남는 표지는 `html_script_marker`다. 본체는 `src/adapters/ui/surface/html_script_banner.rs`가 webview chrome 위에 배너를 그리고, 카드 아래 `banner_inset_gap`까지의 높이만큼 WebView 영역을 줄인다. surface 폭이 `banner_narrow_below()` 미만이면 narrow 배치를 쓴다. 허용 버튼은 셸 안에서 그리므로 배너 버튼 박스를 쓴다. 닫기(×)는 셸 밖에 겹쳐 그리는 Ghost IconButton이라 이 규칙의 대상이 아니다. surface가 새 문서를 로드하는 동안(commit 전) 허용 버튼은 비활성이며 배너 버튼 박스가 아니라 공용 disabled 규칙(중립 상자, disabled ink, opacity 없음)을 따른다.
- 같은 surface의 원격 콘텐츠 차단 안내(`tasty_ui_widgets::html_remote_banner`)도 inset 배너다. 셸·단계·좁은 배치·닫기 규칙은 스크립트 안내와 같고 글리프만 globeOff(`html-remote-banner-glyph`)다. 한 surface의 inset 안내는 합치지 않고 크기 안내(mirror) → 원격 → 스크립트 순서로 따로 쌓는다. 카드마다 위 `banner_margin()`, 아래 `banner_inset_gap()`을 두며 카드 사이 간격도 8이다. 본체 배치는 `src/adapters/ui/surface/html_script_banner.rs::draw_stack`이다. 원격 안내를 닫거나 허용한 뒤 남는 표지는 탭이 아니라 주소 입력란 끝(`html_remote_marker`)에 둔다.

### 탭 스트립 툴팁

네이티브 WebView는 OS z-order상 항상 egui 렌더 표면 위에 있어 그 영역에 그린 egui 툴팁은 보이지 않는다. 그래서 pane 탭 스트립과 pane 머리에 붙은 툴팁은 surface 종류와 관계없이 같은 배치 규칙을 쓴다.

- 후보는 위 → 아래 → 스트립 안 순서다. 창 안에 들어가고 드러난 모든 네이티브 콘텐츠 영역(WebView)과 겹치지 않는 첫 후보를 쓴다.
- 스트립 안 후보는 위·아래가 모두 WebView에 막힐 때(html pane 위에 다른 html pane이 쌓인 경우 등) 쓴다. 탭 스트립은 Tasty가 그려 WebView가 덮지 않으므로, 버블을 스트립 행의 세로 가운데에 두고 앵커가 든 탭 칸의 오른쪽(안 되면 왼쪽)에 `tooltip-offset`(4)만큼 띄운다. 이웃 탭을 덮을 수 있다.
  - 이 후보는 네이티브 영역과 변마다 `border-width`(1)까지 겹쳐도 통과한다. 한 줄 버블의 실측 높이 25(egui caption 줄 높이 15 + 위아래 `space-xs` 4 + 위아래 테두리 1)가 스트립 행 `control-height-tab`(24)보다 1 크고, html surface의 WebView는 스트립 바로 아래에서 시작하기 때문이다. 버블 크기는 바꾸지 않는다.
- 셋 다 안 되면 스트립 안 배치를 그대로 쓴다. 오른쪽이 창 안에 들면 오른쪽, 아니면 왼쪽, 둘 다 창을 벗어나면 오른쪽 배치를 창 안으로 당긴다. 스트립보다 높은 두 줄 버블이면 WebView에 일부가 가려질 수 있다.
- 앵커 칸을 넘기지 않은 호출은 스트립 안 후보가 없으므로, 위·아래가 모두 안 되면 위에 두고 창 안으로 당긴다. 세로도 창 가장자리에서 `tooltip-offset`만큼 띄워 y = 0에 붙지 않는다.
  - 디자인의 최후 배치 "스트립 안"은 앵커 칸이 있는 스트립 툴팁에만 적용한다. 칸 없는 호출은 놓을 행이 없어 위 clamp 배치를 그대로 쓴다. 이 경우 버블이 WebView에 일부 가려질 수 있다. 지금 이 규칙을 쓰는 본체 호출(스크립트 표지)은 모두 칸을 넘긴다.
- 가로는 창 가장자리에서 `tooltip-offset`(4)만큼 안쪽으로 당긴다.
- 툴팁을 보이는 동안 WebView를 숨기지 않는다. hover마다 페이지가 깜박이기 때문이다. 대기 시간·문안·클릭 동작은 다른 툴팁과 같다.

위젯은 `Tooltip::placement_clear_of_native`이며 피할 영역과 앵커가 든 탭 칸은 호출자가 넘긴다. 칸을 넘기지 않으면 스트립 안 후보는 건너뛴다. 본체는 WebView 동기화가 드러낸 WebView의 영역을 `MainViewState::native_content_rects`에 기록하고 탭 바가 이를 읽는다. 지금 이 규칙을 쓰는 탭 스트립 툴팁은 스크립트 표지(`html_script_marker`)뿐이다. 주소 띠의 원격 콘텐츠 표지(`html_remote_marker`) 툴팁도 같은 위젯으로 WebView 영역을 피하지만 탭 칸을 넘기지 않으므로 스트립 안 후보는 쓰지 않는다. 다른 탭 스트립 요소(닫기·스크롤 화살표·분할 버튼)에는 툴팁이 없다. 갤러리 예제는 Layouts › Tab strips의 `tab-strip-tooltips` spec이다.

## 닫기 버튼 / 카운트다운 (우측 상단, 같은 자리)

우상단 같은 자리에서 상태에 따라 표현이 바뀐다.

- **동작이 있는 배너(TTL 없음)**: X(닫기) 버튼이 **평소 숨김**, **배너 위 hover 시에만 표시**.
- **동작이 없는 배너(TTL)**: 평소 그 자리에 **카운트다운 숫자(초 단위)** 표시 → **hover 시 X 로 전환**.
- X 클릭 시 배너 닫힘(사용자 행동).
- 닫기 버튼은 갤러리의 `dismiss_x()`와 같은 Ghost/Sm `IconButton`과 `icons::CLOSE` SVG를 사용한다. `"✕"` 문자는 UI 폰트에 없으면 빈 사각형으로 표시되므로 사용하지 않는다. 색은 IconButton의 기본 색을 따른다(ghost: text-secondary, hover: text-primary). 카운트다운은 `banner_countdown_fg()`를 사용한다.

## "더보기"(⋯) 컨텍스트 메뉴 — mouse-capture 배너 전용

mouse-capture 배너(`defs::BANNER_MOUSE_CAPTURE`)에 한해, X 왼쪽에 "더보기" ⋯ 트리거가
같은 버튼 열에 나란히 놓인다. 다른 배너 kind 는 이 트리거를 갖지 않는다. 트리거는 foreground 프로그램 이름을
모를 때(첫 foreground 조회 전, 조회 실패, 원격 attach mirror 의 첫 값 수신 전·구버전 서버)도 같은 모습으로 그린다. 그때 메뉴는 아래
[이름 미상 형태](#프로그램-이름을-모를-때)로 열린다.

- **노출 조건**: X 와 동일 — 배너 hover 시에만. 단 ⋯ 의 컨텍스트 메뉴가 열려 있는 동안은
  hover 여부와 무관하게 **계속 표시 + active(강조) 상태 유지** — 재사용하려면 ⋯ 재클릭.
- **배치**: ⋯ 가 X 왼쪽, 사이 `banner_more_column_gap()`(4). 두 버튼(Sm 24)의 열은 본문 흐름 밖에
  고정되어 셸 안쪽 위에서 `banner_padding_y()`(8), 오른쪽에서 `spacing_sm`(8) 떨어진다. 본문은 항상
  `banner_more_reserve()`(56 = 2×24 + gap 4 + gap 4)를 오른쪽에 예약한다 — hover 진입/이탈로 본문 폭이
  흔들리지 않도록, hover 전에도 예약 폭은 고정이다(다른 배너는 기존 1 슬롯 28px 흐름 배치 그대로).
- **트리거 아이콘**: SVG `icons::MORE` — 수평 3-dot(`M5 12h.01M12 12h.01M19 12h.01`).
- **메뉴**: host `PopupDef` 의 `headless: true` 컨텍스트 메뉴(`popup-implementation.md`).
  앵커는 트리거 버튼 아래 `banner-more-menu-offset`(4), 우측 정렬 — 뷰포트 하단 공간이 없으면 위로 flip. outside
  click/Esc 로 닫힘(scrim 없음), ↑↓/Enter/Esc 키보드 내비게이션은 기존 headless 메뉴와 동일.
- **폭**: 테두리까지 포함한 폭(시안의 border-box)이 두 행 중 넓은 쪽의 내용 폭(좌우 행 패딩 + 아이콘 + 간격 +
  고정 문구 + 프로그램 이름)에 메뉴 패딩·테두리를 더한 값이다. `banner-more-menu-min-width`(200)와
  `banner-more-menu-max-width`(288) 사이로 제한하며, 상한에 걸리면 프로그램 이름만 줄인다. 고정 문구에 이름 한
  글자와 말줄임표를 더한 폭도 들어가지 않는 행(ja)은 폭을 288 에 두고 그 행만 줄을 바꾼다. 폭과 셸 높이는 메뉴를 열 때
  포그라운드 프로그램 이름으로 한 번 잰다.
- **셸**: 내부 패딩 `banner-more-menu-padding`(4), 배경 `banner-more-menu-bg`(surface-raised), 테두리
  `banner-more-menu-border`(border-strong), radius·그림자는 다른 headless 메뉴와 같다.
- **항목 2개(순서 고정)**, 클릭 시 즉시 실행 + 메뉴 닫힘, 둘 다 neutral 톤(danger 아님 —
  파괴/유실 없고 Settings 에서 되돌릴 수 있음):
  1. **"{app}에서 다시 보지 않기"**(en "Don't show again for {app}", ja "{app} では今後表示しない")(`icons::BELL`) —
     `mouse_capture_banner_blacklist` 에 foreground 프로그램 이름 추가 + **배너도 즉시 함께 닫힘**.
  2. **"{app}에서 클릭은 Tasty가 처리"**(en "Let Tasty handle clicks in {app}", ja "{app} ではクリックを Tasty で処理")(`icons::MOUSE`) —
     `mouse_capture_blacklist` 에 추가. **배너는 남는다** — 캡처가 이미 풀렸음을 사용자가 읽고 직접 닫도록.
  - 두 행 모두 어느 프로그램에 무엇이 바뀌는지를 사용자의 말("다시 보지 않기", "클릭")로 적는다. 문구는
    `[popup.mouse_capture_banner_menu]` 의 prefix·suffix 키다. en 은 이름 앞 문구만, ko·ja 는 이름 뒤 문구만, zh-Hans 는 이름 앞뒤 문구를 모두 쓴다. 양쪽이 모두 빈 언어는 없다.
  - Settings › Terminal › Mouse Capture 의 목록 라벨도 같은 동사를 쓴다. 배너 목록은 "캡처 안내를 다시 보지 않을
    프로그램"(en "Don't show the capture notice for these programs"), 캡처 목록은 "클릭을 Tasty가 처리할
    프로그램"(en "Let Tasty handle clicks in these programs")이다.
- **라벨 렌더**: 공용 `tasty_ui_widgets::banner_more_row`. 고정 텍스트 + 프로그램 이름(mono) **두 조각**으로
  분리 렌더한다 — 하나의 문자열로 합쳐 ellipsis 하면 로케일에 따라(특히 en) 프로그램 이름부터 잘리기
  때문이다. 두 조각 모두 행 글자색이다(쉼 `menu-item-fg`, hover·active `menu-item-fg-hover`. 이름은
  `banner-more-app-fg`·`banner-more-app-fg-hover` 로 같은 값). 고정 텍스트는 truncate 하지 않고, 한 줄이면
  프로그램 이름 세그먼트만 축소+ellipsis 한다. 줄을 바꾼 행은 최소 `menu-item-height`, 위아래
  `menu-item-wrap-padding-y`, 줄 높이 `line-height-ui` × 본문 크기이고 아이콘은 세로 가운데다. 이름은 단어
  중간에서 끊지 않고 혼자 한 줄보다 길 때만 ellipsis 한다. 이름을 줄였으면 전체 이름을 항목 tooltip 으로 보완한다.
- 두 블랙리스트는 Settings › Terminal › Mouse Capture 탭과 데이터를 공유한다. 메뉴와 설정 화면은 같은 매칭 규칙을 사용한다([터미널 (Terminal)](../../surfaces/terminal/index.md)).
- **저장 방어**: 메뉴 항목이 보내는 설정 변경은 프로그램 이름을 trim 해 저장한다. trim 뒤 빈 이름과, 이미 있는
  패턴과 trim·ASCII 대소문자 무시 비교로 같은 이름은 목록을 바꾸지 않는다. 기존 목록의 빈 항목·중복은 자동으로
  정리하지 않는다.
- **메뉴 수명**: 메뉴는 클릭할 때 foreground 이름을 읽는다. foreground 이름이 바뀌거나 사라져 마우스 캡처 배너가
  닫히면, 그 배너에서 연 메뉴도 같은 시점에 닫고 대상 surface 를 비운다. 배너를 띄운 프로그램이 아닌 프로그램에
  설정을 저장하지 않기 위해서다. 다른 surface 의 배너가 닫혀도 열린 메뉴는 그대로다.

### 프로그램 이름을 모를 때

두 행은 foreground 프로그램 이름을 저장한다. 이름이 없거나 trim 뒤 비면(첫 1Hz 조회 전, OS 조회 실패, 원격 attach
mirror 가 서버의 이름을 아직 받지 못했거나 이름을 보내지 않는 구버전 서버에 붙은 경우) 동작은 있지만 지금 실행할 수
없으므로, 숨기지 않고 **비활성으로 보인다**. 원격 mirror 는 서버가 보낸 원격 foreground 이름을 쓴다.

- **트리거**: 이름을 알 때와 같다(hover 노출, 열린 동안 active, 본문 예약 `banner_more_reserve()` 56 유지).
- **구성**: 위에서부터 비대화형 사유 줄 → 구분선 → 같은 두 행(비활성). 공용 `tasty_ui_widgets::banner_more_unknown`
  이 그리며 갤러리 예제도 같은 함수를 쓴다.
  - 사유 줄: info 글리프(`icon_glyph_size_sm` 14) + caption 크기 문장. 둘 다 `banner_more_reason_fg()` 색이다. 글리프와 문장은 위쪽에 맞추고
    글리프만 `banner_more_reason_glyph_offset()`(1)만큼 내린다. 이 값은 배너 앞 글리프의 `banner_glyph_offset()` 과
    역할이 다른 별도 토큰이다. 줄 높이는 문장과 글리프(+오프셋) 중 높은 쪽이다. 줄을 바꾸며
    위아래 `spacing_xs`, 좌우 `spacing_md`, 글리프와 문장 사이 `spacing_sm` 이다. 포커스·hover 가 없다.
  - 구분선: 위아래 `spacing_xs`, 두께 `border_width`, 색은 모든 메뉴 구분선과 같은 `menu_separator()`(→ border-strong).
  - 행: MenuItem disabled 모습 — 글자·아이콘 `state_disabled_fg`, hover 채움 없음, 클릭·포커스 없음. wrap 을
    켜 최소 `menu-item-height`, 줄을 바꾸면 위아래 `menu-item-wrap-padding-y`, 줄 높이 `line-height-ui` 다.
    아이콘은 정상 행과 같은 bell·mouse 다. 툴팁은 없다(사유가 메뉴 안에 보인다).
- **문구**: `[popup.mouse_capture_banner_menu]` 의 `unknown_reason`·`notice_unknown`·`capture_unknown`. 이름 자리 없이
  문장 전체를 쓰고 이름 대신 "이 프로그램"이라고 적는다. "(unknown)" 같은 자리 표시는 그런 이름의 프로세스로
  읽히므로 쓰지 않는다.
- **폭**: `banner-more-menu-max-width`(288) 고정. 줄을 바꾸는 문장을 담으므로 내용 폭에 맞추지 않는다. 셸 높이는
  열 때 그 폭에서 잰 사유 줄·구분선·두 행의 합이다.
- **상호작용**: 비활성 행을 눌러도 아무 일도 없다 — 저장하지 않고 메뉴와 배너가 남는다. Esc·바깥 클릭은 평소처럼
  닫는다. 키보드 내비게이션은 비활성 행을 건너뛰며, 두 행이 모두 비활성이라 고를 행이 없다.
- **형태 고정**: 형태는 열 때 정한다. 열린 메뉴는 이름 미상 형태를 정상 행으로 바꾸지 않고, 다음에 열 때 정상 행을 보인다.
  로컬 surface 에서는 메뉴가 열린 동안 이름이 확인되면 다음 foreground 확인(약 1초 주기)에서 foreground 세대가 바뀌므로,
  위 "메뉴 수명" 규칙대로 배너와 메뉴가 함께 닫힌다. 원격 mirror 는 서버의 이름이 도착할 때 같다. 이름을 보내지 않는
  구버전 서버의 mirror 는 이름이 계속 없어 비활성 형태가 유지된다.

## TTL (살아있는 시간)

- 배너는 유효한 동안에만 존재한다. 유효한 동안 사용자 동작(본문 안 버튼, ⋯ 메뉴 행)이 **없는** 배너는 TTL 을 가지고 스스로 닫힌다. 동작이 **있는** 배너는 TTL 이 없고 사용자가 동작을 고르거나 닫을 때까지 남는다.
- 닫기(×)는 동작이 아니다. 모든 배너에 있으므로 × 때문에 TTL 을 빼지 않는다.
- 개발자는 특정 배너 id 에 이 규칙의 예외를 둘 수 있다. 예외와 그 이유는 해당 배너 정의 위치의 코드 주석에 적고, [원칙](../../principles/feedback-and-recovery.md#배너의-생명주기와-자동-닫힘)대로 개발자의 명확한 지시가 있었다는 사실과 지시 날짜를 함께 적는다.
- plugin 배너는 호스트가 내용을 볼 수 없어 이 규칙을 작성 지침으로 안내한다([egui-mesh 가이드](../../dev-guide/egui-mesh-channel.md)의 banner 절).
- 카운트다운은 **초 단위** 로 우상단에 표시, **0 이 되면 자동 소멸**.
- **정지 조건**: ① 배너 위에 마우스 hover 중 ② 백그라운드(자기 스코프가 현재 화면에 그려지지 않음). 정지 동안 남은 TTL 을 **보존** 하고, 재개 시 **이어서** 진행한다.

## 큐 (다중 배너)

한 스코프 상단에는 **한 번에 1개만 표시**, 나머지는 **큐** 에 대기한다.

- **한 종류(고유 id)당 하나만**:
  - 표시 중인 배너와 **동일 id** 가 다시 발생 → **카운트다운 초기화**(카운트다운 없는 배너면 무시).
  - 큐에 있는 배너와 **동일 id** → **무시**.
- 표시 중 배너가 닫히면 큐에서 **하나씩 꺼내** 표시한다.
- 큐 **최대 5개**. 꽉 찬 상태에서 새 배너 발생 시 **무조건 무시**.

## 계층 z-index / 투명도

서로 다른 스코프의 배너가 동시에 있을 때의 규칙이다.

- 계층(상위 → 하위): **View > Workspace > Pane > Tab > Surface**.
- 상위 배너의 z-index 가 하위보다 **높다**(상위가 앞에 명확히 보임).
- **상위 요소 배너가 뜨면 하위 요소 배너는 60% 투명**(잘 안 보이게).
- 높이 관계: 상위 배너가 더 크면 하위는 그 뒤에 가려져 안 보이고, **하위 배너가 더 커서 뒤로 삐져나온 부분만 60% 투명** 으로 비친다.

하위 배너 디밍은 `opacity_recessed()`를 사용한다. 별도 페이드 모션은 적용하지 않는다.

## 종류(kind)

- Info / Success / Warning / Error 같은 **범용 분류는 두지 않는다**(Toast 와 다른 점).
- **각 배너의 고유 id 자체가 kind** 역할을 한다.
- 경고/에러 같은 심각도 표현은 **그 배너 디자인이 자체적으로** 처리한다.

## 발화 정책 (불가침)

배너는 사용자 직접 조작에 대한 안내로 표시한다. release의 IPC·CLI·플러그인·시스템 작업만으로는 표시하지 않는다. 에이전트 행동이 사용자 상태를 바꾸지 않는다는 [포커스 정책](../policies/focus.md)을 따른다. 토스트에 허용된 원격 연결 상태 알림은 배너의 예외가 아니다.

## 호스트 배너 목록

| id | 범위 | 언제 | 내용 |
|---|---|---|---|
| `mouse-capture` | Surface | 마우스를 캡처한 TUI에서 사용자가 드래그 선택을 시도했을 때 | 정적 `content_fn` + 더보기(⋯)·닫기. 프로그램 이름을 몰라도 ⋯ 메뉴가 있어 TTL 없이 동작이나 ×까지 남는다 |
| `shell-integration-missing` | Surface | 사용자가 본 로컬 셸 surface가 출력은 있는데 PromptBoundary를 받지 못할 때. 로컬 PTY가 없는 원격 attach mirror에는 띄우지 않는다 | 정적 `content_fn` + 카운트다운·닫기. 동작이 없어 TTL 10초 뒤 닫힌다. TTL은 디자인 고정값이며 설정으로 노출하지 않는다 |
| `attach-refusal` | Workspace | 사용자가 활성화한 저장된 원격 행이 연결하지 못한 안내를 가질 때 | `BannerContentSource::AttachRefusal`(대상·이유)을 `tasty_ui_widgets::attach_refusal_banner_content`로 그린다. 매핑 지우기·닫기 버튼이 내용 안에 있어 모서리 닫기 슬롯을 쓰지 않는다. 스코프가 좁으면 매핑 지우기 버튼만 본문 아래 줄로 내리고 ×는 오른쪽 위에 남는다([좁은 스코프](#좁은-스코프)). ×는 그 창의 이번 활성화에만 숨긴다. 상세는 [원격 attach](../../features/remote-attach/index.md) |
| `attach-size-sync` | Workspace | 활성 워크스페이스가 mirror 이고 그 터미널 크기 동기화가 자동 재시도 뒤에도 실패했을 때 | `BannerContentSource::AttachSizeSync`(탭 이름·다시 시도 중 여부)를 `tasty_ui_widgets::attach_size_sync_banner_content`로 그린다. 다시 시도·닫기 버튼이 내용 안에 있어 모서리 닫기 슬롯을 쓰지 않으며, 좁으면 다시 시도만 아래 줄로 내린다. 버튼은 App 이 그 세션에 적용하고 × 는 실패 목록을 비운다. 상세는 [attach 동작](../../dev-guide/attach-behavior.md#크기-요청의-응답과-재시도) |

## IPC / debug

- `surface.read_since_mark` 등 터미널 읽기에는 배너 텍스트를 넣지 않는다. 배너는 egui의 `Order::Foreground`로 그려지며 termwiz 그리드와 분리돼 있다. 별도 텍스트 필터는 필요 없다.
- **debug 빌드 전용** 으로만 배너를 읽고 제어한다. debug 메서드는 사용자 입력 재현/내부 상태 덤프 격리 정책(`#[cfg(debug_assertions)]` + `feature="gui"`, [debug-ipc](../../dev-guide/debug-ipc.md))을 따르며, release 라우터에는 등록되지 않는다. IPC 메서드(= CLI `tasty debug banner <sub>`):
  - `debug.banner.list` (`list`) — 빌트인 def 목록 + 현재 표시/대기 상태 + 기하 덤프.
    표시 중인 배너마다 셸 rect(논리)와 plugin egui-mesh 콘텐츠 rect(물리)를 낸다.
    셸 rect 의 출처는 hover 판정이 쓰는 것과 같은 `card_rects` 라 **직전 프레임 실측**이고,
    그래서 뜬 직후 첫 프레임에는 없다 — 배너는 popup 과 달리 자기 좌표를 모델에 들고
    있지 않고 컨테이너가 매 프레임 배치한다.
  - `debug.banner.show` (`show --banner-id <id> --scope <token>`) — 배너 발생. `outcome`(`Shown`/`Queued`/`ResetCountdown`/`Ignored`) 반환.
  - `debug.banner.close` (`close --banner-id <id>`) — id 로 닫기(표시 중이면 큐 head 승격).
  - `debug.banner.set_countdown` (`set-countdown --scope <token> --seconds <n>`) — 표시 중 TTL 배너 남은 시간 강제 설정.
  - `scope` 토큰: `view` / `workspace:<i>` / `pane:<id>` / `tab:<pane>:<i>` / `surface:<id>` ([`BannerScope::from_token`]). 반환은 별도 구조 없이 "호출 함수 정보 + 인자값" 수준.

## 구조

배너 관리자는 `src/adapters/ui/banner.rs`에 있다. 셸 그리기(`banner_shell`)와 inset 기하는 본체와 갤러리가 함께 쓰도록 `crates/tasty-ui-widgets/src/banner.rs`에 둔다. GUI가 없어도 쓰는 `BannerId`·`BannerScope`는 `crates/tasty-model/src/banner_kind.rs`에 둔다([model-view-split](../../dev-guide/model-view-split.md)).

| 타입 | 역할 |
|---|---|
| `BannerDef` | 고유 ID, TTL 여부, 콘텐츠 그리기 함수의 정적 정의. `defs::all_defs()`·`defs::find(&str)`로 조회 |
| `BannerState` | id·scope·ttl_ms·remaining_ms·content를 담는 인스턴스. `persistent`·`with_ttl`·`plugin_mesh`로 생성 |
| `BannerContentSource` | Host의 `content_fn`과 PluginMesh의 egui-mesh 콘텐츠 구분 |
| `BannerKey` | `Host(id)`·`Plugin(instance_id)`로 정적 배너와 동적 plugin 배너를 같은 큐에서 구분 |
| `BannerManager` | 스코프당 1개 표시·최대 5개 대기, TTL·배치·겹침·디밍·마우스 입력 관리 |

큐와 TTL 처리인 `push`·`close_shown`·`advance`는 egui 없이 단위 테스트로 확인한다. plugin 배너도 같은 수명 관리를 사용하며 채널 규칙은 [egui-mesh 가이드](../../dev-guide/egui-mesh-channel.md)의 banner 절을 따른다.

`draw()`는 `LayoutContext`에서 배치 영역을 계산한다. 호출자가 현재 더보기 메뉴의 scope를 `more_menu_open_for: Option<&BannerScope>`로 전달하며, BannerManager가 팝업을 직접 조회하지는 않는다. 반환값은 `BannerDrawResult { hovered, more_clicked }`다.

- `hovered`는 `MainViewState.banner_hovered`를 통해 [입력 계층](../../architecture/input-layer.md)에 전달한다.
- `more_clicked: Option<(BannerScope, egui::Rect)>`는 버튼의 scope와 사각형이다. 호출자가 이 값으로 대상 필드를 채우고 컨텍스트 메뉴를 연다.

마우스는 scope 전체가 아니라 실제 카드 영역에서만 소비한다. 그렇지 않으면 배너 아래 터미널 클릭까지 막힌다. 배치 영역(`banner_zone`)과 입력 영역(`card_rects`)을 구분하며, 카드 영역은 직전 프레임 값을 사용해 1프레임 늦게 반영된다. 위치가 고정된 persistent 배너에서는 이 지연이 드러나지 않는다.

모든 배너 문자열은 `t("banner.*")` 키 — `lang/{en,ko,ja,zh-Hans}.toml` 네 파일 동시 추가([i18n](../../dev-guide/i18n.md)). 모든 색·치수는 Theme 토큰([theme.md](theme.md)).

## 관련

- [popup.md](popup.md) — 내부 팝업 시스템(독립 기능, 포커스 가짐)
- [toast.md](toast.md) — 휘발성 알림(info 만, 입력 통과)
- [concepts/ubiquitous-language](../../concepts/ubiquitous-language.md) — Modal/Popup/Toast/Banner 구분
- [포커스 정책](../policies/focus.md) · [사용자 통제와 데이터](../../principles/control-and-data.md#사용자에이전트원격-사용자) — 사용자/에이전트 행동 구분(발생 정책 근거)
- [내부 팝업 시스템](popup.md) — 배너의 입력과 수명 규칙

## HTML 원격 수신 카드

공용 `tasty_ui_widgets::html_size_banner`는 큰 파일의 Decline/Allow와 문서 누적 정지의 Stop/Continue를 그린다. 같은 inset 셸과 배너 버튼 박스를 사용하고 닫기 버튼이나 자동 포커스를 두지 않는다. Decline·Stop은 `Ground::Raised` ghost라 쉴 때 `button_ghost_raised_fg()`, hover 때 `button_ghost_raised_fg_hover()`를 읽는다. 파일 경로는 mono 왼쪽 말줄임, 크기는 warning Tag(`tag_bg()` 위 `tag_warning_fg()`), 추가 대기 수는 본문 글자색이다. 좁은 surface에서는 두 버튼을 본문 왼쪽 가장자리에 맞춰 다음 줄에 둔다. `html_receive_state`는 receiving 스피너·paused 다운로드 glyph·끊김·macOS 웹주소 미지원 안내를 같은 토큰으로 그린다. 갤러리의 HTML 주소 배치 예제가 두 위젯을 소비하며, native 전송 연결 여부는 [전송 계약](../../dev-guide/html-transfer.md)이 정한다.
