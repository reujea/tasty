# tasty-gallery

Tasty의 UI 컴포넌트를 따로 실행해 비교하는 갤러리다.
공용 위젯은 본체와 같은 함수를 호출한다. 일부 화면은 props와 레이아웃을 복제하므로
본체가 바뀌면 시각적 일치를 직접 확인해야 한다.

## 실행

```sh
cargo run -p tasty-gallery
```

상단 툴바에서 테마와 UI 배율을 바꾸고, 왼쪽에서 페이지를 선택한다.
각 페이지의 예제를 스크롤하거나 구역 링크로 이동할 수 있다.

네이티브 Wayland(`DISPLAY` 없음, 또는 X 서버 연결 실패)에서 갤러리 입력칸은 외부 앱과 클립보드를 주고받지 않는다(앱 안 복사·붙여넣기만). X11·XWayland에서는 외부 앱과 주고받는다.

## 카탈로그

- Foundations: 색, 글꼴, 간격, 형태, 배율
- Components: 버튼, 입력 필드, 표 등 위젯
- Icons: 공통 아이콘
- Overlays: 팝업과 대화상자
- Layouts: 사이드바, 탭바, 분할 화면
- Plugins: 플러그인 화면
- Chrome: 부팅·종료 화면

## 카탈로그 항목 추가하기

1. `src/catalog/<group>.rs` 또는 `src/catalog/widgets/<name>.rs`에
   `pub fn draw(ui: &mut egui::Ui, theme: &Theme)`를 작성한다.
2. `src/catalog.rs::pages()`의 해당 페이지에 `section(...)` 또는 `spec(...)`을 추가한다.

새 위젯은 본체와 갤러리가 공유할 수 있게 작성한다.
자세한 절차는 [갤러리 우선 개발](../../docs/dev-guide/gallery-first.md)을 따른다.
