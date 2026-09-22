# ADR-0299: 스크롤 영역은 드래그 패닝 여부를 선언한다

- **Status**: Accepted
- **Date**: 2026-09-20
- **Tags**: ui, scroll, input, guard, egui
- **Group**: ui-theme-gallery

## Context

`egui::ScrollArea` 는 `drag_to_scroll` 기본값이 `true` 다. 켜져 있으면 ScrollArea 가
**콘텐츠를 그리기 전에** 자기 영역을 `Sense::drag()` 로 선점한다 — egui 소스의 주석이
그 순서를 그렇게 적는다("We must do this BEFORE adding content to the `ScrollArea`,
or we will steal input from the widgets we contain"). 안쪽 위젯이 `Sense::click()` 만
감지하면 드래그는 전부 ScrollArea 로 간다. 손을 뗀 뒤에는 관성(kinetic)까지 붙는다.
발동 조건은 `content_is_too_large` 라, 콘텐츠가 영역을 넘칠 때만 나타난다.

데스크톱 마우스에서 누른 채 끄는 동작은 텍스트 선택 · 행 선택 · 파일 드래그의 의도다.
그 자리에서 내용이 포인터를 따라 미끄러지면 의도와 충돌하고, 터미널 앱의 관례와도 다르다.
2026-05-03 에 레포는 이 판단을 이미 내려 당시 아홉 파일 전부에 `.drag_to_scroll(false)`
를 넣었다.

그 회차가 남기지 않은 것이 둘이다. **정책 문장이 어디에도 없었고**(`docs/` · `site/content/`
에 이 축의 문장 0 건, `docs/adr/` 에 ADR 0 건 — 근거가 살아 있던 유일한 자리가 커밋 본문
하나였다), **그 값을 지키는 판정기도 없었다.** 그래서 2026-06-28 의 explorer 네이티브
재작성이 세 자리를 기본값으로 되돌렸을 때 빌드도 clippy 도 테스트도 전부 초록이었다.
2026-09-20 실측으로 출하 58 자리 중 50 이 기본값이었다 — 비율이 뒤집혔다.

진입점은 `ScrollArea` 하나가 아니다. `egui_extras::TableBuilder` 도 같은 이름의 값을
가지고 기본이 `true` 이며 자기 안에서 `ScrollArea` 를 만들어 그 값을 넘긴다. explorer 의
Detail 뷰가 그 경로라, 바깥 `ScrollArea` 만 껐을 때 목록이 계속 패닝됐다(실측). `egui::Window`
도 `drag_to_scroll` 을 노출하지만 내부 기본이 `ScrollArea::neither()` 라 **축을 켜야만**
스크롤이 생긴다.

## Decision

출하되는 스크롤 영역은 드래그 패닝 여부를 **선언한다.** 기본 정책은 끄는 것(`false`)이고,
남기기로 한 자리는 `drag_to_scroll(true)` 를 명시한다 — 값이 아니라 **말했는가**를
`crates/tasty-doc-guards/tests/drag_to_scroll_is_declared.rs` 가 강제한다. 그래서
"아직 안 봤다" 와 "보고 남겼다" 가 소스에서 갈리고, 결정이 명부가 아니라 그 소스 줄에 남는다.

가드가 보는 진입점은 셋이다 — `ScrollArea`(생성 → `.show(`/`.show_viewport(`/`.show_rows(`),
`TableBuilder`(생성 → `.header(`/`.body(`), 그리고 **축을 켠** `egui::Window`
(`.scroll(`/`.scroll2(`/`.hscroll(`/`.vscroll(` 중 하나가 있는 것만). 축을 안 켠 `Window` 를
보지 않는 것은 그 자리에 스크롤이 없어 위반이 성립하지 않기 때문이다 — 보면 실재하지 않는
위반에 대한 처방이 된다.

이것은 on/off 정책이라 `Theme` 필드로 만들지 않는다. 조절 가능한 수치가 아니라 테마마다
달라지지 않는 상수이므로, [theme.md](../design/systems/theme.md) 의 "UI 디자인 규칙" 표와
이 ADR 이 단일 출처다(같은 표의 애니메이션 세 행과 같은 취급).

## Consequences

- **얻은 것**: 기본값 그대로의 스크롤 영역이 새로 들어오면 `doc-guards.yml` 이 main push ·
  PR 마다 경로 필터 없이 그 가드를 돌려 빨개진다. 정책이 커밋 본문이 아니라 문서와 판정기
  양쪽에 산다. 남기기로 한 자리는 그 이유가 소스 줄 옆에 남는다.
- **잃은 것**: 스크롤 영역을 새로 쓸 때 한 줄을 더 적어야 한다. 터치 장치에서 드래그 패닝이
  자연스러운 입력인데 이 레포는 그것을 기본으로 끈다 — 터치를 1 급으로 지원하게 되면 정책
  자체를 다시 봐야 한다(아래 재검토 조건).
- **운영 비용 / 유지 부담**: 가드는 소스 텍스트 스캔이라 구간 판정이 dataflow 가 아니라
  근접(생성 줄부터 40 줄)이다. 양쪽으로 샌다 — 종결자가 멀면 지나치고, 무관한 종결자가
  끼면 없는 위반을 보고한다. 둘 다 그 파일 모듈 doc 에 변이로 확인해 적어 두었다.
  `egui::ComboBox` 의 드롭다운처럼 egui 가 자기 안에서 만드는 스크롤은 호출부에
  `drag_to_scroll` 이 노출되지 않아 **레포 소스로 닫을 수 없고**, 그래서 좌변에 안 넣는다.

## Alternatives Considered

- **A. 문서에만 적는다** — 비용 0 이다. 그런데 **이미 한 번 실패한 방식이다**: 근거가 커밋
  본문에 있었는데도 재작성이 놓쳤고 넉 달 만에 50 자리가 됐다. 문서 문장은 이 결정에서도
  필요하지만(가드는 "빠뜨리지 마라" 만 강제하고 "왜" 를 말하지 않는다) 그것만으로는 부족하다.
- **B. 래퍼 위젯 하나로 모으고 직접 호출을 센다** — `tasty-ui-widgets` 에 래퍼를 두고 가드는
  래퍼를 안 거치는 직접 호출을 센다(공용 순회 `walk_with_floor` 와 같은 구조). (A) 보다 강하다
  — 기본값을 한 자리에서 정하므로 새 스크롤 영역이 정책을 자동으로 상속하고, 텍스트 스캔의
  사각 대부분이 무의미해진다. 안 고른 이유는 비용이다: 58 자리를 전부 이식해야 하고 `ScrollArea`
  의 빌더 메서드를 래퍼가 어디까지 노출할지 정해야 하며 `TableBuilder` 도 같은 래퍼 뒤로
  가야 일관된다. 그 이식 자체가 `tasty-ui-widgets` 를 크게 흔들어 plugin 의존 폐포
  ([ADR-0166](0166-the-plugin-version-gate-judges-the-artifact-not-the-directory.md))
  전체의 재발행을 부른다. **폐기가 아니라 미룬 것이다** — 아래 재검토 조건이 그 문턱이다.
- **C. 값까지 강제한다(`false` 만 허용)** — allowlist 가 생긴다. 예외가 명부로 가면 결정이
  소스에서 멀어지고, 명부는 자라기만 한다. "말했는가" 만 묻는 쪽이 같은 재발 방지력을 가지면서
  예외를 그 자리에 남긴다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

**채널이 붙는 것** — 판정 시점에 레포가 읽을 수 있는 사실이다.

- `egui` 가 `ScrollArea::drag_to_scroll` 의 기본값을 `false` 로 바꾼다. 그러면 좌변이 사라지고
  가드는 래칫이 아니라 소음이 된다. 좌변: `Cargo.lock` 이 잠근 `egui` 판의 `ScrollArea`
  기본값 초기화(상류 크레이트 소스 — 이 저장소 밖이다).
- `egui::ComboBox` 가 `drag_to_scroll` 을 호출부에 노출한다. 그러면 지금 "여기서는 못 닫는다"
  로 적어 둔 19~20 자리가 좌변 안으로 들어온다. 좌변: 잠긴 `egui` 의
  `ComboBox` 공개 메서드 목록.
- 명시 `drag_to_scroll(true)` 자리가 생긴다. 지금은 0 이다 — 하나라도 생기면 "기본 정책은
  끄는 것" 이라는 이 ADR 의 서술과 실제가 갈리기 시작하므로 그 사유를 여기 반영한다.
  좌변: 가드의 스캔 모수에서 `drag_to_scroll(true)` 를 담은 자리의 수.
- 대안 B 의 문턱 — 가드의 사각(별칭 import · 래퍼 함수 경유)이 레포에 실제로 나타난다.
  지금 둘 다 0 건이다. 좌변: 가드 모듈 doc 의 "못 잡는 것" 표에 적힌 형태의 실측 수.

**원리적으로 안 붙는 것** — 사람이 관측해야 한다.

- tasty 가 터치 입력을 1 급으로 지원하게 된다. 드래그 패닝은 터치에서 **기본 스크롤 수단**이라
  이 정책의 전제(데스크톱 마우스)가 무너진다. 재는 법: 터치 이벤트를 다루는 코드가 들어오는지
  (`winit` 의 `Touch` 이벤트 소비처)와 그 화면이 스크롤 영역을 쓰는지를 함께 본다 — 앞쪽은
  채널이 붙지만 "1 급 지원" 인지는 제품 결정이라 사람이 정한다.

## References

- [theme.md](../design/systems/theme.md) "UI 디자인 규칙" — 정책 행. on/off 정책을 `Theme` 로
  넓히지 않는 관례가 그 표 아래 적혀 있다
- [ci-gates.md](../dev-guide/ci-gates.md) — `doc-guards.yml` 의 트리거와 범위
- 코드 근거(결정이 실현된 현재 위치): `crates/tasty-doc-guards/tests/drag_to_scroll_is_declared.rs`
  의 `every_scroll_area_declares_drag_to_scroll` · `every_table_builder_declares_drag_to_scroll` ·
  `every_scrolling_window_declares_drag_to_scroll`
- [ADR-0166](0166-the-plugin-version-gate-judges-the-artifact-not-the-directory.md) — 대안 B 의
  비용이 plugin 의존 폐포로 번지는 근거
