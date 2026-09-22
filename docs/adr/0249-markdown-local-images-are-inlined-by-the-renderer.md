# ADR-0249: markdown 의 로컬 이미지는 렌더러가 문서 안에 싣는다 — 그 자리가 읽기 범위이기도 하다

- **Status**: Accepted
- **Date**: 2026-09-08
- **Tags**: markdown, plugin, webview, images, sanitizer, security, scope, cross-platform, adr-0065
- **Group**: bundled-plugins

## Context

markdown surface 는 **로컬 이미지를 한 번도 로드하지 못했다.** 여섯 가지 `src` 형태를 실측한
결과 원격(`https://`)만 떴고 나머지는 전부 실패했다.

이유는 하나다. plugin 이 만든 HTML 은 host 의 `PlatformWebView::load_html` 로 실린다. 그
문서에는 **주소는 있어도 권한이 없다** — `<base href="file:///<base_dir>/">` 는 상대 경로를
푸는 규칙일 뿐이고, 문서 origin 은 `about:blank` 라 어떤 파일도 못 읽는다. 그래서 경로는
올바르게 풀리고 fetch 만 거부된다. 세 백엔드가 같은 형태다: Linux 는 `load_html(html, None)`,
macOS 는 baseURL 로 `about:blank`, Windows 는 `NavigateToString`.

백엔드에 읽기 범위를 주는 길이 각각 있긴 하다(macOS `loadFileURL:allowingReadAccessToURL:`,
Linux `load_html` 의 base_uri 인자). 그러나 그 길은 **범위가 백엔드마다 다르다.** 실측
(2026-09-08, Linux/WebKitGTK): `load_html` 에 `file:///<base_dir>/` 를 넘기면 상대 경로·스킴
없는 절대 경로·raw HTML `<img>` 가 모두 떴지만, `../밖/x.svg` 와 트리 밖 절대 경로도 **같이**
떴다. 즉 그 길로 가면 "이미지가 뜬다" 는 얻고 "문서 트리 밖은 안 읽는다" 는 못 얻는다.

## Decision

**렌더러가 로컬 이미지를 문서 안으로 싣는다.** sanitize 를 마친 본문에서 원격이 아닌 모든
`<img src>` 를 파일로 풀어 `data:` URI 로 바꾸고, 못 푸는 것은 `src` 를 통째로 지운다
(`render::inline_local_images`). 그래서 이 자리를 지나면 **`http(s)` 도 `data:` 도 아닌
`src` 는 하나도 안 남는다** — 그것이 이 함수의 계약이고, 시험이 그 문장을 그대로 건다.

한 자리가 두 가지를 동시에 준다.

1. **이미지가 뜬다.** 바이트가 문서 안에 있으니 읽기 권한이 필요 없다. 세 플랫폼이 같은
   경로로 동작하고, 백엔드마다 다른 "읽기 범위" API 를 쓰지 않는다.
2. **읽기 범위가 문서 디렉토리 트리다.** 트리 밖은 여기서 거절된다. 판정은 `canonicalize`
   뒤에 하므로 `..` 도 심볼릭 링크도 같이 막힌다.

범위 밖에 더해 이런 것도 거절한다: 확장자 허용목록(PNG·APNG·JPEG·GIF·WebP·AVIF·BMP·ICO·SVG)
밖, 한 장 4 MiB 초과, 문서 총합 16 MiB 초과, base_dir 이 없는 문서. 거절은 `src` 제거이고
그 자리는 이미 있는 실패 placeholder 가 채운다.

**sanitizer 의 스킴 허용목록은 `http`·`https`·`mailto` 셋 그대로 둔다.** `data:`·`file:` 을
넣지 않는 이유는 ammonia 의 `url_schemes` 가 **속성별로 나뉘지 않기** 때문이다 — `img src` 를
열면 `a href` 도 같이 열린다. 로컬 이미지는 그 스킴 없이도 뜬다(스킴 없는 경로는 ammonia 의
상대 URL 정책으로 통과해 위 인라이너까지 도달한다). 그래서 넓히면 얻는 것 없이 `href` 표면만
넓어진다. 이 결정은 `sanitize_keeps_only_http_https_mailto_schemes` 가 값으로 고정한다.

## Consequences

- **얻은 것**: 상대 경로·스킴 없는 절대 경로·raw HTML `<img>` 가 뜬다(실측). 트리 밖은
  안 뜬다(실측). host 코드는 한 줄도 안 바뀌었고 백엔드 분기도 안 생겼다 — 세 플랫폼이
  같은 코드로 같은 결과를 낸다.
- **잃은 것**: 문서 HTML 이 이미지 바이트의 약 4/3 만큼 커지고, 그만큼 IPC 한 줄에 실려
  간다. 그래서 상한이 있다. 이 렌더러는 이미 같은 방식으로 KaTeX 폰트(~254 KB)와 mermaid
  번들(~3.5 MB)을 문서에 싣고 있어 새로운 종류의 비용은 아니다.
- **잃은 것 2**: `![](file:///abs/x.png)` 형태는 여전히 안 뜬다 — sanitizer 가 `file:` 을
  지운다(위 결정). 저자가 쓰는 형태는 상대 경로거나 스킴 없는 절대 경로이고 그 둘은 뜬다.
- **운영 비용 / 유지 부담**: 확장자 허용목록과 두 상한이 이 파일에 상수로 있다. 늘릴 일이
  생기면 그 자리 하나만 만진다.

## Alternatives Considered

- **A: 백엔드에 읽기 범위를 준다** (Linux `load_html` base_uri · macOS
  `loadFileURL:allowingReadAccessToURL:` · Windows 는 미상) — Linux 로 재 보니 이미지는
  떴지만 트리 밖도 함께 떴다(위 Context). 범위 판정이 백엔드마다 갈리고, 그 갈림을 세
  플랫폼에서 각각 재야 한다.
- **B: 커스텀 URI 스킴 핸들러** — 범위를 정확히 줄 수 있지만 백엔드마다 등록 API 가 다르고
  구현량이 가장 크다. 그리고 plugin 이 만든 `<base href>` 와 host 가 정한 스킴이 서로를
  알아야 해서 층이 얽힌다.
- **C: 렌더 결과를 base_dir 안 임시 파일로 떨구고 `file://` 로 연다** — base_dir 에 쓰기를
  요구해 읽기 전용 위치의 문서를 못 열게 된다.
- **D: `<img>` 를 지우고 경로만 보여준다** — 결함을 결함으로 고정하는 것이다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

**채널이 붙는 것** — 판정 시점에 레포가 읽을 수 있는 사실이다.

- `PlatformWebView` 에 base URI/읽기 범위를 받는 API 가 생긴다(세 백엔드의 `load_html`
  시그니처가 인자를 하나 더 받는다). 그러면 A 를 다시 견줄 근거가 생긴다.
- `render::inline_local_images` 가 사라지거나 계약 시험
  (`no_src_survives_that_is_neither_remote_nor_inlined`)이 사라진다.

**원리적으로 안 붙는 것** — 사람이 관측해야 한다. 재는 법을 함께 적는다.

- 이미지가 많은 문서에서 열기가 눈에 띄게 느려진다. 재는 법: 이미지 총량이 상한 근처인
  문서를 markdown surface 로 열고 webview 자식 창을 캡처할 때까지의 벽시계를, 같은 문서에서
  이미지를 뺀 판과 견준다.
- 저자가 `file:` 스킴 이미지를 쓰는 문서가 실제로 문제가 된다. 재는 법: 그 형태가 담긴
  문서를 열어 실패 placeholder 가 뜨는지 본다.

## References

- [ADR-0065](0065-markdown-webview-render-channel.md) — markdown 본문을 webview 채널로 옮긴
  결정. 이 ADR 이 고치는 결함은 그 이동이 데려온 것이다(egui 텍스처 파이프라인에서는
  plugin 이 파일을 직접 읽어 그렸다).
- [ADR-0248](0248-webview-teardown-lets-gdk-finish-before-the-x-window-is-destroyed.md) —
  같은 티켓의 다른 절(창 닫기 abort). 무관한 결함이고 고침도 겹치지 않는다.
- 코드 근거(결정이 실현된 현재 위치): `render::inline_local_images` ·
  `render::read_inline_image` · `render::sanitize_html`.
- 사용자에게 보이는 동작 서술: [`docs/plugins/markdown/index.md`](../plugins/markdown/index.md)
  의 "인라인 이미지".
