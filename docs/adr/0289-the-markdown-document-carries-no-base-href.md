# ADR-0289: markdown 문서는 `<base href>` 를 싣지 않는다 — 그것이 문서 안 앵커를 문서 밖으로 보낸다

- **Status**: Accepted
- **Date**: 2026-09-19
- **Tags**: markdown, plugin, webview, navigation, anchors, toc, footnotes, cross-platform, adr-0249, adr-0065
- **Group**: bundled-plugins

## Context

markdown surface 의 **목차 항목을 눌러도 아무 데로도 가지 않는다**(사용자 보고, macOS).

생성 HTML 은 `<head>` 에 `<base href="file:///<base_dir>/">` 를 싣고 있었고, 목차 항목은
`<a href="#<슬러그>">` 였다. HTML 규칙상 **fragment-only href 도 base URL 에 상대적으로
풀린다** — 즉 `#target` 은 이 문서 안의 `id` 가 아니라 `file:///<base_dir>/#target` 이라는
**다른 문서**를 가리킨다. 제목 `id` 는 제대로 붙어 있었고 sanitizer 도 `id` 를 지우지 않았다.
목적지가 없었던 것이 아니라 **주소가 딴 데로 풀린** 것이다.

실측(2026-09-19, Linux/WebKitGTK 2.50, 이 렌더러가 실제로 생성한 문서를 host 와 같은 방식으로
`load_html(html, None)` 에 실어 재현):

| | base 있음(옛 형태) | base 없음(현재) |
|---|---|---|
| `document.baseURI` | `file:///…/mddir/` | `about:blank` |
| 목차 항목의 raw href | `#target` | `#target` |
| 그 항목의 해석된 href | `file:///…/mddir/#target` | `about:blank#target` |
| 목차 클릭 후 `scrollY` | 0 | 2307 |
| 목차 클릭 후 대상 제목 가시성 | 보이지 않음 | 보임(뷰포트 상단 48px) |

그 base 가 원래 하던 일은 **이미 남아 있지 않았다.** 로컬 이미지는 [ADR-0249](0249-markdown-local-images-are-inlined-by-the-renderer.md)
이후 렌더러가 `data:` URI 로 문서 안에 싣고(못 실으면 `src` 를 지운다), **마크다운** 링크
destination 은 anchor-only 가 아닌 한 전부 `#tasty-nav:` fragment 로 rewrite 된다. KaTeX 폰트도
`data:` 다.

상대 URL 이 그래도 한 형태로는 남는다 — **저자가 마크다운에 직접 쓴 raw HTML**(`<a href="x.md">`)
이다. 그것은 pulldown-cmark 를 `Event::Html` 로 통과해 `rewrite_link_event` 가 보지 못하고,
ammonia 의 `url_relative` 기본값이 pass-through 라 sanitize 도 지나 살아남는다. 그러나 그 형태에
base 는 **도움이 아니라 해**였다: base 가 있으면 그 href 가 실제 파일로 풀려 클릭 시 렌더된 문서가
통째로 교체됐다(실측). base 가 없으면 그 href 는 파일로 풀리지 않는다(실측) — 다만 **그것을 클릭했을
때 엔진이 무엇을 하는지는 세 백엔드 어디에서도 측정되지 않았다**(아래 재검토 트리거). 그래서 base 가
실제로 **지켜 주던** 것은 하나도 없고, base 가 결정하던 것은 **fragment-only href 하나뿐**이며 그
방향은 파괴적이다.

## Decision

**`<base href>` 태그를 생성 문서에서 없앤다.** 그리고 문서 안 앵커의 이동을 엔진의
fragment navigation 에 맡기지 않고 **신뢰 스크립트가 직접 수행한다** — `nav_script` 의
위임된 `click` 리스너 하나가 fragment-only `href`(목차 항목 · 본문의 `[텍스트](#슬러그)` ·
각주 참조/복귀)를 잡아 기본 navigation 을 취소하고 목적지 `id` 를 `scrollIntoView` 한다.
`#tasty-nav:` 로 시작하는 href 는 가로채지 않는다 — 그쪽은 host 신호 채널이라
`decide-policy` 까지 도달해야 한다.

둘을 함께 하는 이유는 각각이 다른 것을 막기 때문이다. base 제거는 앵커가 **다른 문서를
가리키는 것**을 막고, 스크립트 가로채기는 **엔진마다 다른 fragment navigation 동작**과
**같은 hash 재대입이 무동작인 성질**(같은 목차 항목을 두 번 누르면 두 번째가 사라진다)을 막는다.

목적지 조회는 raw fragment 로 `getElementById` 를 먼저 하고, 실패하면 percent-decode 한
철자로 한 번 더 한다 — 제목 `id` 는 원문 그대로(한글 제목이면 한글)이고 각주 `id` 는 양쪽이
모두 percent-encoded 라, 둘 중 한 철자가 반드시 맞는다.

## Consequences

- **얻은 것**: 목차 · 본문 내부 링크 · 각주 참조/복귀가 실제로 이동한다(실측 위 표 + 중복 제목 ·
  한글 제목 · 반복 클릭 · 각주 왕복 전부 대상 가시성 `true`). 앵커 클릭이 **navigation 을 아예
  만들지 않으므로** host 가 판정할 것도, 서피스가 로딩/오류 chrome 으로 깜빡일 것도 없다
  (실측: 앵커를 여러 번 클릭해도 `decide-policy` 에 도달한 URL 은 최초 `about:blank` 하나뿐).
- **얻은 것 2**: 세 백엔드가 같은 코드로 같은 결과를 낸다 — 문서의 base URL 을 각 엔진이 어떻게
  정하든(WebKitGTK 는 `load_html(html, None)`, macOS 는 `about:blank` baseURL, Windows 는
  `NavigateToString`) 이동이 그 값에 의존하지 않는다.
- **잃은 것**: 문서가 상대 URL 을 푸는 능력을 잃는다. 이 파이프라인이 내보내는 것 중에는 그
  능력을 쓰는 자리가 없고, 유일하게 남는 상대 URL(저자가 쓴 raw HTML `href`)은 base 가 있을 때
  오히려 문서를 날려 버리던 형태다. 다만 앞으로 `data:` 로 싣지 않는 새 상대 참조를 도입하면
  base 가 아니라 **절대화한 값**을 넣어야 한다.
- **잃은 것 2**: 앵커 이동이 JS 에 의존한다. 이 surface 는 신뢰 스크립트(주소창 · 찾기 · 복사
  버튼)가 이미 필수라 JS 는 항상 켜져 있고([`docs/plugins/markdown/index.md`](../plugins/markdown/index.md)
  의 "JS 는 기본 허용"), 스크립트가 없으면 이동만 안 될 뿐 문서는 그대로다.
- **운영 비용 / 유지 부담**: 리스너 하나(16 줄)와 CSS `scroll-margin-top` 한 줄. 각주 목적지에도
  제목이 이미 쓰던 것과 같은 값을 줘, 주소창 아래로 붙지 않게 한다. 그 값은 여기서 다시 적지
  않는다 — 주소창 높이는 `--md-addr-bar-h` 한 자리에 살고 `scroll-margin-top` 이 그것을 읽는다
  ([markdown 화면](../plugins/markdown/screens/markdown.md)). 이 ADR 이 쓰였을 때는 주소창이
  문서 상단 한 뷰포트에서만 붙어 있어 "화면에 있을 때" 라는 단서가 필요했는데, 지금은 바가 상시
  붙어 있어 그 단서도 없어졌다.

## Alternatives Considered

- **A: base 는 두고 스크립트로만 가로챈다** — 동작은 한다. 그러나 base 가 남아 있는 동안
  "이 문서의 fragment href 는 다른 문서를 가리킨다" 는 사실도 남아, 스크립트가 못 도는 자리
  (스크립트보다 먼저 일어나는 클릭, 앞으로 생길 다른 소비자)에서 되살아난다. 그리고 그 base 는
  ADR-0249 이후 **아무 일도 하지 않는다** — 없애지 않을 이유가 남지 않았다.
- **B: base 만 없애고 엔진의 fragment navigation 에 맡긴다** — Linux 에서는 그것만으로 이동했다
  (위 표의 오른쪽 열은 스크립트를 빼고 재도 `scrollY` 가 움직인다). 기각 이유는 두 가지다:
  같은 항목 반복 클릭이 무동작이고(과제 요구), 그 이동이 host 가 forward 하는 navigation 을
  만들어 plugin 이 무시해야 할 신호가 늘어난다. 세 엔진의 동작 일치도 확인되지 않았다.
- **C: 목차 항목을 `#tasty-nav:` 채널에 태워 plugin 이 스크롤을 지시한다** — host→plugin 왕복이
  생기고, 스크롤을 지시할 새 IPC(webview 에 JS 를 주입하는 채널)가 필요하다. 문서 안에서 끝나는
  일에 층을 셋 더한다.
- **D: 제목 `id` 대신 `<a name>` 앵커를 쓴다** — base 문제는 `name` 앵커에도 똑같이 걸린다
  (해석은 href 쪽에서 일어난다). 아무것도 고치지 못한다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

**채널이 붙는 것** — 판정 시점에 레포가 읽을 수 있는 사실이다.

- `render::inline_local_images` 가 사라지거나 그 계약 시험
  (`no_src_survives_that_is_neither_remote_nor_inlined`)이 사라진다 — 그러면 문서가 다시 상대
  URL 을 풀어야 하고, base 의 부재가 비용이 된다.
- `PlatformWebView::load_html` 이 base URI 를 받는 인자를 갖는다 — 문서의 base 가 plugin 이
  아니라 host 의 선택이 되므로 이 결정의 전제가 바뀐다.
- `render_document` 가 `<base` 를 다시 내보낸다 —
  `document_with_a_base_dir_still_carries_no_base_tag` 가 값으로 고정한다.

**원리적으로 안 붙는 것** — 사람이 관측해야 한다. 재는 법을 함께 적는다.

- macOS(WKWebView) · Windows(WebView2) 에서 앵커 이동이 실제로 일어나는지. 재는 법: 뷰포트보다
  긴 문서를 markdown surface 로 열고 화면 밖 제목의 목차 항목을 클릭해, 그 제목이 보이는지와
  문서가 그대로인지를 OS 캡처로 본다. 이 ADR 의 실측은 Linux/WebKitGTK 에서만 이뤄졌다.
- 엔진이 `scrollIntoView` 의 `scroll-margin` 을 무시해 대상이 뷰포트 맨 위에 붙는다. 재는 법:
  클릭 후 대상의 `getBoundingClientRect().top` 이 **주소창의 실측 높이보다 큰지** 본다 — 그
  높이를 이 자리에 값으로 적지 않는 이유는, 적으면 `--md-addr-bar-h` 가 바뀌었을 때 이 트리거가
  조용히 낡은 기준으로 재기 때문이다. 같은 자리에서 `#tasty-addr-bar` 의
  `getBoundingClientRect().height` 를 함께 읽어 그것과 견준다(실측 2026-09-20, WebKitGTK: 대상
  48 · 바 높이 40).
- 저자가 쓴 raw HTML 상대 `href`(`<a href="x.md">`)를 **클릭했을 때** 세 엔진이 무엇을 하는가 —
  위 Context 가 "풀리지 않는다" 까지만 재고 그 뒤는 안 쟀다. 재는 법: 그 링크를 담은 문서를
  markdown surface 로 열어 클릭하고, 문서가 그대로인지 OS 캡처로 본다. host 의 `decide-policy`
  가 그 navigation 을 막는지 forward 하는지도 같은 자리에서 함께 본다.

## References

- [ADR-0249](0249-markdown-local-images-are-inlined-by-the-renderer.md) — 로컬 이미지를 문서 안에
  싣는 결정. base 가 할 일을 없앤 것이 그 결정이고, 이 ADR 은 그 뒤에 남은 껍데기를 걷어낸다.
- [ADR-0065](0065-markdown-webview-render-channel.md) — markdown 본문을 webview 채널로 옮긴 결정.
- 코드 근거(결정이 실현된 현재 위치): `render::render_document`(base 태그 부재) ·
  `render::nav_script`(앵커 가로채기) · `render::rewrite_link_dest`(anchor-only 예외) ·
  `render::theme_css`(`scroll-margin-top`).
- 사용자에게 보이는 동작 서술: [`docs/plugins/markdown/index.md`](../plugins/markdown/index.md) ·
  [`docs/plugins/markdown/screens/markdown.md`](../plugins/markdown/screens/markdown.md) 의
  "Heading id + 목차(TOC)".
- MDN, `<base>` 의 in-page anchors 절 — fragment-only href 도 base URL 기준으로 풀린다는 규칙:
  https://developer.mozilla.org/en-US/docs/Web/HTML/Reference/Elements/base#in-page_anchors
