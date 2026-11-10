# 설정 창 화면

- **부모 기획**: [../index.md](../index.md)
- **트리거 위치**: [사이드바](../../sidebar/index.md#화면) 하단 **설정 버튼**
- **시각 소스**: tasty-page `vendor/ui_kits/terminal/overlays/settings_window.jsx`

## 트리거

사이드바 하단 **설정 버튼** 클릭 → 설정 모달 창이 열린다 (전역 1개, 활성 시 입력 차단).

## 레이아웃 (2-level IA)

```
┌──────────────────────────────────────────────────────────────────────┐
│ [General][Terminal][Appearance][Keybindings][FileHandler][Misc][Plugins]│  L1 탭바 (7탭, 폭 넘치면 화살표 스크롤)
├───────────────┬────────────────────────────────────────────────────────┤
│ 🔍 필터        │                                                        │
│ ▸ General     │   (선택된 L2 섹션의 설정 항목)                            │  콘텐츠
│   Notifications│                                                       │
│   Accessibility│                                                       │
│   …(L2 섹션)   │                                                        │
├───────────────┴────────────────────────────────────────────────────────┤
│                                              [ Cancel ] [ Save ]        │
└────────────────────────────────────────────────────────────────────────┘
```

## UI 요소 인벤토리

- **L1 탭바** (상단, 7탭, 이 순서): General / Terminal / Appearance / Keybindings / FileHandler(표시 라벨 **Handler**) / Misc / Plugins.
- **L2 섹션 목록** (좌측): 현재 L1 의 하위 섹션 + **필터 검색**. (L1 전환 시 필터 클리어.) L1 별 L2:
  - **General**: General / Notifications / Accessibility / Overlay(토스트 표시 시간 `Toast duration`, 1~10s · 0.5s 눈금) / Remote transfer / Remote attach(원격 attach 무응답 끊김 시간 `Disconnect after`, 10~600s) / Display(macOS 전용) / Permissions(macOS 전용)
  - **Terminal**: General(터미널 동작 설정) / Input(앱별 Shift+Enter 설정) / Mouse Capture(마우스 캡처 안내 배너 토글 + Shift 우회 Note + 캡처 비활성화 블랙리스트 + 배너만 억제하는 블랙리스트) / TUI(OSC 52 클립보드 읽기 허용 토글 + bordered warning callout) / Performance
  - **Appearance**: Theme / Colors(프리셋 색 개별 override picker) / General / Display(UI 스케일 전용) / Tasty(앱 크롬 색상) / Terminal / Explorer(내장 파일 관리자 폰트) / (플러그인 기여 페이지 동적 — 예: HTML)
  - **Keybindings**: General / Workspace / Pane / Tab / Surface / Clipboard / Zoom / Explorer / Scripts / Preset / Plugins / ─ / Import / Export — 마지막 항목 위에만 1px separator 가 붙고, 필터 검색 중에는 separator 를 숨긴다
  - **FileHandler**(표시 "Handler"): File Extension Mapping / File Detectors / File Handlers / Hook Handlers(공유 훅 핸들러 레지스트리 편집 — 리스너 설정은 CLI 전용, 여기 미노출)
  - **Misc**: Scripts (최상단) / Task pipeline / Anomaly detection.
  - **Plugins**: 플러그인 기여 설정 페이지 (동적)
- **콘텐츠** (중앙): 선택된 L2 섹션의 설정 항목. 도메인별 내용은 해당 기능 문서에서 설명:
  - Keybindings → [`features/keybindings/`](../../keybindings/index.md) / [`design/policies/key-mapping`](../../../design/policies/key-mapping.md)
  - Theme(Appearance) → [`design/systems/theme`](../../../design/systems/theme.md)
  - Notifications → [`features/notifications/`](../../notifications/index.md) · FileHandler(파일 서브탭) → [`features/file-handler/`](../../file-handler/index.md) · Hook Handlers → [`features/webhook/`](../../webhook/index.md)·[`features/hooks/`](../../hooks/index.md)
  - Plugins → [`features/plugin-system/`](../../plugin-system/index.md)
- **Save / Cancel** (하단): draft 커밋 / 폐기. plugin 단축키 draft(Plugins 서브탭 편집 · 가져오기 Apply)도 같은 규칙이다 — Save 로 닫혔을 때만 적용되고, Cancel · 타이틀바 close · `toggle_settings` 로 닫히면 버린다. 헤더 밴드에 close ✕ 는 없다 — 닫기/취소 진입점은
  footer **Cancel** · OS 타이틀바 close · `toggle_settings` 바인딩(기본 `Ctrl+,`) 셋이다. 마지막 것은
  타이틀바 close 와 같은 경로(`ViewAction::Close`)로 닫으므로 draft 처리가 같다. 바인딩 녹화 중에는
  닫지 않는다 — 그때 키는 캡처로 가야 한다. **Escape 는 넷째 진입점이 아니다** — 대응 바인딩
  필드가 없고, 이 화면에는 편집 가능한 텍스트 필드가 여러 탭에 있어 편집 중 Escape 가 "편집 취소"
  인지 "닫기" 인지가 아직 값으로 안 정해졌다. 메인 윈도우의 Escape 경로가 보는 `settings_open_requested`
  는 **열기 요청 래치**라 모달이 떠 있는 동안은 false 다 — 그 경로는 이 화면을 닫지 않는다.
- **콘텐츠 컬럼 상한**: 스크롤하는 콘텐츠 컬럼은 `Theme::settings_content_max_width`(620)로 막힌다(`tasty_ui_widgets::settings_content_column`). 상한은 **그 컬럼 한 자리**에만 건다 — full-bleed 가 아닌 L2 서브탭이 전부 물려받으므로 블록이 저마다 폭을 들 필요가 없고, 블록마다 걸면 블록끼리 값이 갈린다. 창이 그보다 좁으면 남은 폭에 맞춘다(상한이지 최소폭이 아니다). 본문 산문이 쓰는 `measure_md`(400)는 용도가 다르다 — 읽는 줄 길이라 이 상한보다 좁고, 그대로 둔다.
- **행 격자**: 서브탭의 설정 행은 라벨 열 · 컨트롤 격자다(`tasty_ui_widgets::SettingsRow`). 라벨 열은 그 서브탭에서 가장 긴 라벨 폭을 `settings_label_width`(150) … `settings_label_max_width`(240)로 clamp 한 값이며, 더 긴 라벨은 열 안에서 줄을 바꾼다. 라벨과 컨트롤 사이는 `settings_label_gap`(16), 행과 행 사이는 `settings_row_gap`(12)이다. 한 행에 딸린 설명과 경고 callout 은 그 행 아래 `settings_row_caption_gap`(4) 자리에 `measure_md` 폭으로 붙는다. Keybindings 의 General ~ Scripts 서브탭도 같은 라벨 칸과 같은 clamp 를 쓴다. 열 폭은 그 서브탭의 단축키 라벨과 quick-switch 행 라벨(Workspace·Tab), Scripts 는 스크립트 이름으로 잰다. 열보다 긴 라벨은 줄을 바꾸고 도움말 아이콘은 마지막 단어 뒤에 붙는다.
- **설명·빈 줄 크기**: 행 caption, 서브탭 안내문, 숫자 칸의 단위, 패널 안의 빈 줄(목록이 비었음 · 필터 결과 없음 · 이 OS 에 없는 서브탭)은 caption 11(`font_size_caption`) · `text_muted` 다. 화면 전체를 차지하는 `CenterState`(Scripts 의 빈 목록 등)는 제목을 본문 13 으로 둔다.
- **Keybindings › Preset · Import / Export**: 이 두 서브탭만 표준 패딩/스크롤 래퍼 없이 **full-bleed** drill-down(목록⇄상세 content-swap)으로 그려진다. 그래서 위 컬럼 상한도 안 받는다 — 컬럼 자체를 자기 레이아웃으로 대체한다. 상세: [`features/keybindings/`](../../keybindings/index.md#프리셋) · [가져오기 / 내보내기](../../keybindings/index.md#가져오기--내보내기).

### 숫자 입력 한 모양

설정 창과 plugin 기여 설정의 숫자 입력은 같은 형태를 쓴다. 드래그 입력은 스크롤과
충돌할 수 있고, 증감 버튼은 자주 바꾸지 않는 설정에 불필요한 공간을 차지하므로 쓰지 않는다.

- **mono `Input`** — 폭 `field_width_xs`(90), **자릿수 우측 정렬**(열을 내려가며 자리가
  맞아야 두 값을 눈으로 견준다). 새 컴포넌트가 아니라 기존 `Input` 그대로다.
- **단위는 필드 밖 정적 텍스트** — muted · caption 11(`font_size_caption`), 타이핑 대상이 아니다.
  아래 범위 경고 줄도 같은 caption 크기다. 단위가 없는 칸은 그 자리가
  비어 있다. 원격 전송의 `MiB` 만 mono 다.
- **clamp 는 확정 때만** — blur 와 `↵` 뿐이고, **치는 동안에는 값을 안 건드린다**. 25~200
  칸에 `150` 을 칠 때 첫 글자에서 값이 끌려가면 그 다음 글자를 못 친다.
- **범위 밖은 막지 않고 말한다** — danger 테두리 + 그 아래 한 줄. 그 줄은 범위와 **확정될
  값**을 함께 적는다(`Between 10 and 200. Commits as 200.`).
- **숫자로 안 읽히는 중간 상태**(빈 칸 · `-` · `1e`)는 경고가 아니다. 확정 때 마지막 값이
  그대로 남는다.

구현은 `src/view/settings/ui/tabs/number.rs` 한 자리이고, 확정 판정(`commit`)은 그리기와
무관한 순수 함수라 같은 파일의 단위 시험이 확인한다. 갤러리 `settings-number` specimen 이
상태 셋(default / out of range / disabled)을 전시한다.

## 상태별 시각

- **plugin page 유무**: Plugins 탭 / Appearance plugin sub-tab 은 등록된 plugin page 가 있을 때만.
- **Keybindings 녹화/충돌**: 키 녹화 중 표시 + 충돌 시 확인 팝업.

## 시각 소스

tasty-page `vendor/ui_kits/terminal/overlays/settings_window.jsx` — 창 치수·탭바·L2 목록·콘텐츠 배치의 단일 출처. 스크린샷: tasty-page `vendor/screens/settings_window-tabs.png`.
