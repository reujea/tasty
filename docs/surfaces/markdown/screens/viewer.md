# Markdown surface 화면

- **부모 기획**: [../index.md](../index.md)
- **시각 소스**: plugin 이 생성한 sanitize HTML 문서 — host native OS WebView 가 렌더. 마크다운 surface의 전용 vendor 시안은 현재 없다.

[작업 영역](../../../features/work-area/index.md#화면) 타일 안에 열리는 마크다운 렌더 surface. plugin 이 `pulldown-cmark`+`ammonia` 로 만든 HTML 문서를 host 의 native OS WebView overlay(WebKitGTK/WKWebView/WebView2)에 올려 렌더한다(webview, [WebView 호스트 계약](../../../design/systems/webview.md)) — host 는 문서의 픽셀에 관여하지 않는다.

## 트리거

마크다운 파일 열기 또는 `markdown` surface 생성/전환.

## UI 요소 인벤토리

- **렌더된 마크다운 본문** — 제목/문단/목록/코드블록/링크 등. heading 은 전부 GitHub 호환 자동
  슬러그 `id` 를 받는다(아래 "Heading id + 목차(TOC)" 절).
- **주소창** — 문서 자체에 내장된 `<input>`+`<button>`(host egui 위젯 아님).
- **목차(TOC)** — heading 이 하나 이상이면 주소창 바로 아래, 본문 위에 접을 수 있는 `<nav>` 로
  삽입된다(문서 자체에 내장, host egui 위젯 아님). heading 이 없으면 렌더되지 않는다.
- **검색 바** — 문서 자체에 내장(host egui 위젯 아님), 기본 숨김. 문서에 포커스가 있을 때
  `Ctrl+F`/`Cmd+F` 로 우상단에 뜬다(아래 "문서 내 검색(find-in-page)" 절).
- 탭 표시명은 파일명.

## 상태별 시각

- 파일 없음/로드 실패 등은 문서 내 `.tasty-state` div 로 표시. 본문 자리의 상태는 다음 순서로 하나만 고른다(`render_document`).

  | 순서 | 조건 | 표시 | 배치·색 |
  |---|---|---|---|
  | 1 | mirror 연결 끊김 | `markdown.remote.disconnected` | `.tasty-state-error` |
  | 2 | mirror 원문을 아직 못 받음(실패 없음) | `markdown.remote.loading` | `.tasty-state` |
  | 3 | 읽기 실패 | `markdown.state.failed` + 사유 | `.tasty-state-error` |
  | 4 | 대용량 확인 대기 또는 취소이고 보일 이전 내용이 없음(`shows_large_deferred`). 읽어 둔 파일이 커져 확인을 기다리는 동안에는 이전 내용을 그대로 보인다 | `markdown.state.large_deferred` | `.tasty-state` |
  | 5 | 원문이 공백뿐 | `markdown.state.empty` | `.tasty-state` |

  대용량 대기 상태는 빈 파일 상태와 같은 배치·색에 문구가 다르고, 문구 아래 space-sm 에 secondary sm 버튼 `<button class="tasty-state-action">`(`markdown.state.large_deferred_open`, "Open file…")을 둔다. 버튼의 높이·여백·글자 크기·색·hover/active 는 Theme 의 Button 토큰(sm 높이 `item_height_tab`, 좌우 space-sm, caption, button-secondary-bg·border, button-fg, overlay)을 CSS 로 주입한 값이다. 버튼은 파일을 바로 읽지 않는다. 새로고침과 같은 `#tasty-nav:refresh:<nonce>` 를 보내고, 플러그인은 그 문서가 대기 상태일 때만 명시적 reload 경로로 대용량 확인을 다시 띄운다. 확인 팝업이 아직 열려 있으면 겹쳐 띄우지 않는다. attach mirror 문서는 대용량 확인을 거치지 않으므로 이 상태가 없다.

## Mermaid 다이어그램

펜스드 코드블록 언어가 `mermaid` 인 블록(` ```mermaid `)은 `code.language-mermaid` 로 살아남아
[mermaid.js](https://github.com/mermaid-js/mermaid)(MIT, 오프라인 vendor —
`crates/tasty-plugin-markdown/assets/mermaid.min.js` + `NOTICE.md`)로 렌더된다. 문서에 mermaid
블록이 하나도 없으면 이 스크립트는 아예 삽입되지 않는다(수 MB 번들이라 불필요한 문서에 매번
인라인하는 낭비를 피함). 테마는 `Theme.is_light` 에 따라 mermaid 내장 `default`/`dark` 팔레트로
매핑되며, 테마 전환 시 문서 전체가 재생성되므로(`reload_all_webviews`) 별도 런타임 재테마
로직 없이 자동 반영된다. 초기화·렌더 실패는 console error로 기록한다. 실패한 다이어그램이 언제나 원본 코드로 복원된다고 보장하지 않는다.

## 수식(Math/LaTeX) 렌더링

인라인 `$...$`/블록 `$$...$$` 수식은 `pulldown-cmark`의 `Options::ENABLE_MATH`
(`parser_options()`)가 켜져 있으면 각각 `<span class="math math-inline">`/`<span
class="math math-display">`(원본 LaTeX 소스가 HTML-escape된 텍스트)로 파싱된다 — **이 구조는
라이브러리 기본 동작 그대로**다(GFM alert/footnote처럼 Rust 쪽에서 별도 AST 이벤트 rewrite를
할 필요가 없다). 실제 수식 렌더링은 mermaid와 동일한 클래스의 작업(브라우저 JS
라이브러리, 필요한 문서에만 신뢰된 스크립트 삽입)으로
[KaTeX](https://katex.org)(MIT, 오프라인 vendor —
`crates/tasty-plugin-markdown/assets/katex.min.{js,css}` + `assets/fonts/KaTeX_*.woff2` +
`NOTICE.md`)가 수행한다. 문서에 math span이 하나도 없으면 이 스크립트(JS+CSS+폰트 합쳐 약
1MB)는 아예 삽입되지 않는다 — mermaid/highlight.js와 동일한 조건부 삽입 절약 패턴.

- **sanitizer 확장 방식**: ammonia는 class *값* 단위 화이트리스트를 지원하지 않는다(태그·속성
  단위만) — `span` 태그 + `class` 속성 전체를 허용하는 쪽을 택했다(`div`+`class`가 이미 이
  crate에서 같은 방식으로 열려 있다 — `.tasty-state`/`.tasty-state-error` 등). math 이벤트만
  sanitizer를 우회한 별도 신뢰 HTML로 사후 조립하는 대안도 검토했으나, `class` 값 자체는
  스크립트를 실행하거나 URL을 열지 못해 사용자가 raw HTML로 `<span class="math
  math-inline">직접 쓴 LaTeX</span>`를 흉내내도 결과는 "자기 콘텐츠가 KaTeX로 렌더된다"뿐이고,
  pulldown-cmark 기본 동작을 그대로 통과시킬 수 있어 커스텀 이벤트 rewrite 코드가 전혀
  필요없다는 점에서 이 방식을 택했다.
- **폰트 오프라인 포함 — data URI**: 이 plugin의 다른 모든 vendored 자산(mermaid.js/highlight.js,
  그리고 렌더된 문서 자체)은 host WebView에 넘기는 단일 HTML 문자열 안에 완전히
  self-contained되어 있다 — 런타임에 상대 폰트 URL이 참조할 수 있는 "plugin assets 디렉토리"가
  디스크에 따로 존재하지 않는다(`include_str!`/`include_bytes!`가 바이너리에 포함하며, 아무것도
  디스크에 다시 써지지 않는다). 문서에는 `<base href>`가 아예 없어서(문서 안 앵커가 다른
  URL로 풀리기 때문 — 아래 기능 계약)
  상대 폰트 URL을 풀어 줄 기준 자체가 없다. 그래서
  `render.rs::katex_css_with_embedded_fonts`가 `katex.min.css`의 각 `@font-face`
  `src:`(원래 `woff2`/`woff`/`ttf` 3-format 상대경로 리스트)를 vendored `woff2` bytes를
  base64 인코딩한 단일 `data:font/woff2;base64,...` 엔트리로 치환한다(문자열 치환 — regex
  의존성 불필요, 폰트 20개 basename이 컴파일타임에 고정). `woff`/`ttf` 형제 파일은 vendor하지
  않았다 — 이 웹뷰가 임베드하는 엔진(WebKitGTK/WKWebView/WebView2)은 전부 evergreen이라
  레거시 브라우저 폴백이 불필요하다.
- **보안**: `throwOnError: false`(파싱 실패 시 예외 대신 원본 TeX을 에러색으로 렌더 —
  크래시·빈 화면 없음, 실패 전 span의 원본 escaped 텍스트도 손대지 않으므로 최악의 경우도
  "원본 텍스트 그대로")와 `trust: false`(`\includegraphics`/`\href`/`\url` 등 LaTeX 매크로를
  통한 임의 URL/마크업 삽입 차단, `sanitize_html`의 화이트리스트와는 별개의 방어선)를 **명시적으로**
  설정한다(`katex.render(tex, el, {throwOnError:false, trust:false, displayMode})`) — KaTeX
  자체 기본값에 기대지 않는다.
- **텍스트 추출**: `el.textContent`(`innerHTML`이 아님)로 원본 LaTeX을 복원한다 — DOM이
  `escape_html`이 인코딩한 HTML entity를 자동으로 원래 문자로 되돌려주므로, 저자가 `$...$` 사이에
  실제로 입력한 문자열을 그대로 얻는다(코드블록 복사 버튼의 `code.textContent` 사용과 동일한
  근거).
- **블록/인라인 구분**: `el.classList.contains('math-display')`로 `displayMode`를 결정한다.
  블록 수식의 가운데 정렬·여백은 KaTeX 자체 CSS(`.katex-display`)가 이미 처리하므로 이 plugin이
  별도 CSS를 얹지 않는다.
- **테마 연동**: vendored `katex.min.css`는 색을 하드코딩하지 않는다(`color:currentColor`만
  존재, 직접 확인) — 수식은 그냥 `body`의 `color:var(--md-fg)`를 상속하므로 다크/라이트 전환
  시 별도 런타임 재테마 로직 없이 자동으로 본문 텍스트 색을 따라간다(테마 변경 시 문서 전체가
  재생성되는 것은 다른 모든 요소와 동일).
- **중복 렌더 방지**: `data-tasty-math-rendered` 속성으로 같은 span에 `katex.render`가 두 번
  걸리는 것을 막는다(코드블록 복사 버튼/이미지 실패 UI와 동일한 idempotency 관례) — 재로드는
  항상 문서 전체를 새로 만들므로 구조적으로 필요하진 않지만 방어적으로 넣었다.

**검증 범위**: Linux/WebKitGTK에서 정상 인라인·블록 수식의 MathML 생성과 깨진 수식의
`.katex-error` 표시를 확인한 이력이 있다. 오류 표시의 텍스트는 원본 TeX와 같았다.
macOS·Windows에서 실제 실행한 결과는 없다.

## 코드블록 syntax highlighting

펜스드 코드블록(` ```rust ` 등)은 `sanitize_fence_lang` 이 언어 토큰을 `[A-Za-z0-9_+-]` 로 정규화한
`code.language-<lang>` class 를 그대로 유지한 채 렌더되고, 실제 토큰 강조는 **브라우저**에서
[highlight.js](https://highlightjs.org)(BSD-3-Clause, 오프라인 vendor —
`crates/tasty-plugin-markdown/assets/highlight.min.js` + `NOTICE.md`, 36개 언어를 포함하는
"common" 번들)가 수행한다. 서버(syntect 등) 대신 브라우저를 택한 이유는
`sanitize_html`(`render.rs`)의 태그/속성 화이트리스트를 건드릴 필요가 없고, mermaid 가 이미
증명한 "신뢰된 wrapper 에 조건부 스크립트 삽입" 패턴을 그대로 재사용할 수 있기 때문이다.

- **조건부 삽입**: 문서에 펜스드 코드블록이 하나도 없으면 highlight.js 스크립트 자체가 삽입되지
  않는다(mermaid 와 동일한 절약 패턴 — `render_document`).
- **언어 매핑**: highlight.js 가 인식하는 언어 식별자와 `sanitize_fence_lang` 의 정규화 결과가
  겹치는 경우(예: `rust`/`javascript`/`typescript`/`python`/`json`/`bash`/`yaml`/`markdown`,
  그리고 `toml` — highlight.js 에서는 `ini` 문법의 alias 로만 등록되어 있다) 별도 매핑 테이블
  없이 그대로 통한다. 초기화 스크립트는 각 코드블록의 `language-<lang>` class 에서 `<lang>` 을
  추출해 `hljs.getLanguage(lang)` 로 지원 여부를 먼저 확인하고, 미지원 언어(또는 애초에 언어가
  없는 코드블록)는 에러 없이 조용히 무하이라이팅으로 남는다 — `mermaid` 처럼 highlight.js 가
  모르는 "언어"도 이 경로로 자연히 스킵된다.
- **XSS 안전성**: highlight.js 의 `highlightElement` 는 요소의 plain-text 내용을 읽어 자체
  escape 된 `<span class="hljs-*">` 마크업으로 `innerHTML` 을 재작성할 뿐, 기존 마크업을
  재해석하지 않는다. 코드블록 안의 리터럴 `<script>` 텍스트는 `sanitize_html` 이 이미
  HTML-escape 해 둔 상태라 하이라이팅을 거쳐도 그대로 비활성 텍스트로 남는다.
- **테마 연동**: highlight.js 자체 내장 테마(github.css 등)는 vendor 하지 않는다 — 대신
  `hljs-*` token class 를 이 plugin 이 이미 쓰는 Catppuccin 스타일 `Theme` hue 필드(`mauve`/
  `blue`/`green`/`peach`/`teal`/`lavender`/`red` 등)에 매핑한 CSS 규칙(`render.rs::hljs_css`,
  `theme_css` 에 상시 포함)으로 직접 칠한다. highlight.js 는 색을 전혀 모르고 토큰 분류만
  하므로, mermaid 처럼 라이트/다크를 스크립트에 baked-in 할 필요가 없다 — 테마 전환 시 문서
  전체가 재생성되며 `theme_css` 가 다시 계산될 뿐이다. vendor 된 고정 팔레트(GitHub 색 등)
  대신 사용자가 고른 테마(mocha/latte/커스텀)를 그대로 따라가는 쪽을 택했다.

## 코드블록 복사 버튼

`#tasty-md-body pre > code`(본문 코드블록만 — 로드 에러 상세 `.tasty-state-detail` 은 `#tasty-md-body`
바깥의 별개 `<pre>` 라 대상이 아니다) 각각에 hover 로 노출되는 복사 버튼을 붙인다
(`render.rs::copy_button_script`). 문서에 `<pre><code` 가 하나도 없으면(대다수 비-코드 문서)
이 스크립트 자체가 삽입되지 않는다 — mermaid/highlight.js 와 동일한 조건부 삽입 절약 패턴.
언어 라벨이 없는 코드블록(펜스드 무언어/들여쓰기 블록 — `class` 자체가 없는 `<pre><code>`)도
대상에 포함된다.

- **mermaid/highlight.js 와의 실행 순서 조율**: `mermaid.run()` 은 비동기(promise 를 await 하지
  않음, `mermaid_script` 문서 참조)이므로 스크립트 삽입 순서(`nav_script` → `highlight_script` →
  `mermaid_script` → 이 스크립트, `render_document`)만으로는 진단 시점에 mermaid 의 DOM 치환이
  이미 끝났다고 보장할 수 없다. 그래서 순서에 기대지 않고 attach 루프 안에서
  `code.classList.contains('language-mermaid')` 를 직접 검사해 스킵한다 — mermaid 블록은 버튼을
  받지 않는다(원본 마크다운을 복사하는 대신 아예 버튼을 붙이지 않는 쪽을 택함, 다이어그램으로
  치환된 뒤의 코드는 어차피 원본과 다른 모양이라 "복사"의 의미가 모호해지기 때문). highlight.js
  는 반대로 순서 문제가 없다 — `highlightElement` 는 기존 문자를 `<span class="hljs-*">` 로
  감쌀 뿐 절대 바꾸지 않으므로(실제 vendor 번들을 WebKitGTK 엔진에 로드해 하이라이팅 전/후
  `textContent` 를 직접 비교 — 아래 "실제 실행 검증" 참조), 클릭 시점에 `code.textContent` 를 읽으면
  하이라이팅 완료 여부와 무관하게 항상 원본 코드 텍스트가 나온다.
- **클립보드**: 우선 `navigator.clipboard.writeText()`, 실패(구현 부재 또는 promise reject) 시
  오프스크린 `<textarea>` + `document.execCommand('copy')` 로 폴백한다.
- **접근성/터치**: 버튼은 `tabindex="0"`+`aria-label`(`markdown.copy.label`)로 키보드 포커스
  가능하고, 기본은 `opacity:0` 로 숨겨져 있다가 `pre:hover`/`:focus-visible` 시 나타난다. hover
  가 없는 터치 환경은 `@media (hover:none)` 로 상시 노출한다.
- **피드백**: 복사 성공/실패 시 버튼 텍스트가 `markdown.copy.copied`/`markdown.copy.failed` 로
  바뀌고 `data-state` 속성(CSS 색 변경)이 붙은 뒤 1.5초 후 원래
  라벨로 복원된다.
- **중복 부착 방지**: `reload_webview`(`main.rs`)는 재로드 때마다 `render_document` 를 다시 호출해
  문서 전체를 통째로 교체한다(부분 DOM 패치가 아님) — 그래서 리스너가 재로드마다 누적될 구조적
  경로 자체가 없다. attach 루프도 방어적으로 `pre.querySelector('.tasty-copy-btn')` 로 같은
  `<pre>` 안에 이미 버튼이 있으면 건너뛴다.

**검증 범위**: Linux/WebKitGTK에서 highlight.js 적용 전후의 `code.textContent`가
원문과 같고, 복사 버튼이 시스템 클립보드에 같은 텍스트를 쓰는 것을 확인한 이력이 있다.
이때 `navigator.clipboard.writeText()`가 성공해 `execCommand('copy')` 폴백은 실행하지 않았다.
macOS·Windows의 실제 실행 결과는 확인되지 않았다.

## Frontmatter 숨김

문서 최상단(첫 줄)에 오는 YAML(`---\n...\n---`) 또는 TOML(`+++\n...\n+++`) frontmatter 는
Jekyll/Hugo/Obsidian/Zettlr 등에서 흔히 붙이는 메타데이터 블록이다. `pulldown-cmark` 의
`ENABLE_YAML_STYLE_METADATA_BLOCKS`/`ENABLE_PLUSES_DELIMITED_METADATA_BLOCKS` 옵션으로 파싱 단계에서
`Tag::MetadataBlock` 이벤트로 인식되고, HTML 라이터가 이 블록을 항상 비출력(non-writing) 처리하므로
렌더된 본문에서 완전히 사라진다 — 별도의 메타데이터 표시 패널은 두지 않는다(파싱 의존성·i18n·
깨진 YAML 폴백 처리 비용을 늘리지 않기 위한 선택). 이 규칙은 CommonMark frontmatter 확장 사양대로
**문서의 맨 처음에 올 때만** 적용된다 — 문서 중간의 `---` 는 지금까지와 동일하게 `<hr>` 로 렌더된다.

## Smart punctuation

`pulldown-cmark` 의 `ENABLE_SMART_PUNCTUATION` 옵션이 항상 켜져 있다(설정 토글 없음). 직선따옴표
`"`/`'` → 곡선따옴표, `--`/`---` → en/em dash, `...` → ellipsis 로 자동 치환된다. Obsidian 등
대다수 문서 뷰어의 기본값과 맞춘 선택이며, GitHub 의 raw 렌더링과 완전히 동일하지 않다는 점은
감수한 트레이드오프다. 인라인 코드(`` `--` ``)와 펜스드 코드블록 내부 텍스트, 그리고 백슬래시로
이스케이프한 문장부호(`\"`, `\-\-`)는 이 치환의 영향을 받지 않고 원문 그대로 남는다.

## 인용 막대 두께

일반 인용문의 왼쪽 막대는 `md_quote_bar_width()`(2px, `selection_edge_width`를 가리키는 hairline)를
CSS 변수 `--md-quote-bar-w`로 받아 그린다. 접지 않는 콜아웃은 `<blockquote>`라 같은 규칙을 이어받아
2px다. 접는 콜아웃(`<details>`)도 `--md-quote-bar-w`를 읽어 같은 두께로 그린다. 막대 색은 콜아웃
종류별 강조색이다. 종류별 규칙(`alert_css`)은 `border-left-color`만 정하므로, details 규칙은 막대를
`border-left-width`·`border-left-style`로 나눠 쓴다. `border-left` 축약을 쓰면 색이 currentColor로
돌아가고, 선택자 특이성이 더 높아 종류별 색을 덮는다.

## 콜아웃 (GFM alert + Obsidian 확장 타입)

`> [!note]`류 blockquote 태그는 GFM alert 5종과 Obsidian 스타일 확장(타입 확장·접기·커스텀
제목)이 `render.rs::rewrite_callout_events`/`rewrite_callout_buffer` 하나의 통합 경로로
처리된다 — 두 문법이 서로 다른 함수로 나뉘어 있지 않다. `pulldown-cmark`의
`Options::ENABLE_GFM`은 문서 전체에 한 번만 적용되는 파서 옵션이라 GFM 5종만 켜고 Obsidian
확장만 끄는 식으로 나눌 수 없기 때문에 이렇게 설계했다.

### 지원 타입

- **GFM 5종**(기존 동작 그대로 유지): `note`/`tip`/`important`/`warning`/`caution`.
- **Obsidian 확장 10종**(공식 문서 [obsidian.md/help/callouts](https://obsidian.md/help/callouts)
  기준으로 확정 — GFM 5종과 겹치지 않는 타입만): `abstract`/`info`/`todo`/`success`/`question`/
  `failure`/`danger`/`bug`/`example`/`quote`.
- **Obsidian 문서상의 별칭**(충돌 없는 것만, `render/callout.rs::CALLOUT_ALIASES`): `summary`/`tldr`→
  abstract, `hint`→tip, `check`/`done`→success, `help`/`faq`→question, `fail`/`missing`→failure,
  `error`→danger, `cite`→quote.
- **의도적으로 제외한 별칭**: Obsidian 문서는 `important`/`caution`/`attention`을 각각
  tip/warning/warning 의 별칭으로 정의하지만, `important`와 `caution`은 이미 GFM 고유 타입으로 각각
  다른 아이콘·색을 사용한다 — "GFM 5종 기존 유지"가 우선이라 이 3개는 별칭 테이블에 넣지
  않았다. `[!important]`/`[!caution]`은 항상 기존 GFM 엔트리로 해석되고, `[!attention]`은
  아무 타입에도 매칭되지 않아 일반 blockquote 로 남는다.
- 목록에 없는 `[!아무거나]`는 콜아웃으로 인식되지 않고 대괄호 텍스트 그대로 일반 blockquote
  본문에 남는다(공식 문서에 없는 타입은 만들어내지 않는다는 지원 범위).

### 문법

`[!type]` 뒤에 붙는 `+`/`-`/제목은 서로 독립적이다(Obsidian 공식 문서 확인 — 마커 없이도
제목만 붙일 수 있고, 제목 없이 마커만 붙일 수도 있다):

- `> [!type]` — 마커/제목 없음. 기본 타입 라벨(번역됨)로 렌더.
- `> [!type] 제목` — 마커 없이 커스텀 제목만. **접기 UI 자체가 없다**(`<details>`를 아예 쓰지
  않는다 — GFM alert 와 동일한 `<blockquote>` 구조, `data-label` 속성만 제목으로 바뀐다).
- `> [!type]+ 제목`(제목 생략 가능) — `<details open><summary>...` 로 렌더, 초기 펼침.
- `> [!type]- 제목`(제목 생략 가능) — `<details><summary>...`(`open` 없음), 초기 접힘.

GFM 5종을 마커/제목 없이 bare 로 쓴 `> [!NOTE]`는 `pulldown-cmark` 파서 자체가
`Tag::BlockQuote(Some(kind))` AST 이벤트로 이미 인식한 상태로 넘어온다(`scanners.rs::
scan_blockquote_tag` — 태그 뒤에 공백 외 다른 내용이 있으면 이 인식 자체가 실패해 일반
blockquote 로 폴백). 이 경우를 포함한 모든 콜아웃 인식·렌더가
`rewrite_callout_buffer`에서 한 곳에 모여 처리된다. **결과적으로 GFM 5종에 마커/제목이
붙은 문법(`[!note]+ 제목`처럼)도 Obsidian 문법으로 자연히 인식된다**.

헤더 레이블(기본 타입 라벨 또는 커스텀 제목)은 `render/callout.rs::CALLOUT_KINDS`가 plugin 자신의
`Translator`로 UI 언어에 맞게 조회한다(`markdown.alert.<type>`, `lang/{en,ko,ja}.toml`) —
마커 없는 구조는 CSS 가 언어를 분기할 수 없으므로 `data-label` 속성 + `content:
attr(data-label)`로 반영하고, `<details>` 구조는 `<summary>` 안의 실제 텍스트 노드로
반영한다(어느 쪽이든 커스텀 제목이 있으면 그 텍스트가 기본 라벨을 대체). 아이콘은
`tasty-icons`의 canonical 글리프를 각 kind 의 accent 색으로 만든 SVG data URI 로
`background-image` 에 심는다(`render/callout.rs::alert_icon_data_uri`, 15개 타입이 7개 기존
semantic accent(`accent_primary`/`accent_info`/`accent_success`/`accent_warning`/
`accent_attention`/`accent_danger`/`accent_agent`)를 나눠 쓴다 — 전용 색 토큰 신설 없음,
겹치는 조합은 아이콘·라벨 텍스트로 구분). 상자 바탕은 종류 색을 `tint-fill-alpha`(0.12, CSS 알파 31)로 칠한다(`render/callout.rs::tint_fill_alpha`). highlight.js 의 diff 추가·삭제 줄 배경도 같은 알파로 `accent_success`·`accent_danger`를 칠한다.

콜아웃 머리는 접힘 마커 → 종류 아이콘 → 라벨 순서이고 사이 간격은 `md_callout_icon_gap()`(space-xs 4)이다. 종류 아이콘은 `icon_glyph_size_md`(16)이며, 마커 없는 콜아웃의 라벨은 아이콘과 간격만큼(`calc(var(--md-callout-icon) + var(--md-callout-icon-gap))`, 20) 들여 쓴다. 접히는 콜아웃은 브라우저 기본 `<details>` 마커를 숨기고(`list-style:none`, `::-webkit-details-marker{display:none}`) summary 배경에 종류 색 chevron을 `md_callout_marker_size()`(icon-size-sm 14) 크기로 그린다. 닫혀 있으면 `chevronRight`, 열리면 `chevronDown`이다(`render/callout.rs::alert_css`). 세 값은 `theme_css()`가 `--md-callout-icon`·`--md-callout-icon-gap`·`--md-callout-marker`로 쓴다.

태그가 없거나 지원하지 않는 종류의 Markdown 인용문은 일반 인용문으로 남는다.
콜아웃 변환은 파서 이벤트와 인용문 첫 줄을 처리한다. Raw HTML에 class만 넣었다고
자동으로 data-label을 추가하지는 않는다. 다만 sanitize가 class·data-label·summary를
허용하므로 작성자가 이 속성과 태그를 직접 넣어 콜아웃처럼 표시할 수 있다.

### 중첩

콜아웃 안에 다른 콜아웃/일반 blockquote 가 중첩되면(`> [!note]\n> > [!warning]\n...`)
`rewrite_callout_events` 가 재귀적으로 안쪽도 동일하게 처리한다 — 완벽한 시각적 스타일링은
스코프 밖이지만(예: 중첩 깊이별 들여쓰기 조정 없음), 크래시나 텍스트 유실 없이 안쪽 콜아웃도
자기 타입대로 렌더된다.

### sanitize

`<details>`/`<summary>` 태그가 `sanitize_html` 허용 목록에 있다 — `details` 는
`class`+`open`, `summary` 는 별도 속성 없음(제목이 실제 텍스트 노드로 들어가 별도
attribute 가 필요 없다).

## Bare URL autolink

본문에 `<...>` 꺾쇠나 `[text](url)` 마크다운 문법 없이 그냥 텍스트로 적은 `http://`/`https://` URL 도
클릭 가능한 링크로 변환된다. `pulldown-cmark` 0.12.2 는 CommonMark 코어의 꺾쇠 autolink 만 지원하고
GFM 의 확장 autolink(스킴 없는 bare URL 인식 포함)는 구현하지 않으므로(`Options::ENABLE_GFM` 는 alert
blockquote 태그만 켠다), `render.rs::autolink_bare_urls`/`split_bare_urls` 가 이벤트 스트림에서 직접
스캔해 `Tag::Link` 로 쪼갠다. **지원하는 스킴은 `http(s)://`뿐이다** — `www.`-prefix(스킴 없는
호스트)나 이메일 자동링크는 제외(필요성이 확인되면 따로 다룬다).

이 pass 는 상태를 가진 스캔이다(단순 무상태 `map()` 이 아님) — 다음을 명시적으로 제외한다:

- 이미 `Tag::Link` 안의 텍스트(`[https://x](https://x)`처럼 이미 명시적으로 링크된 경우) — 중복 링크
  방지.
- `Tag::CodeBlock`(펜스드/들여쓰기 코드블록) 내부 텍스트.
- 인라인 코드는 별도 추적이 필요 없다 — pulldown-cmark 는 인라인 코드 내용을 애초에 `Event::Text`
  가 아니라 별개의 `Event::Code` variant 로 표현하므로 구조적으로 이미 제외된다.

하나의 URL이 여러 `Event::Text` 조각으로 나뉠 수 있다 — URL 안에서 진짜 강조(emphasis)로 짝이 맞지 않는 `*`/`_` 하나가 단독 1글자 `Event::Text` 로
따로 토큰화된다(예: `.../foo*bar` → `Text("...foo")` + `Text("*")` + `Text("bar...")`). 그래서 이
pass 는 실제 마크업(다른 태그 시작/`SoftBreak` 등)이 끼어들기 전까지 연속된 `Event::Text` 를 하나의
버퍼로 병합한 뒤 그 위에서 URL 을 스캔한다 — 진짜 마크업 경계는 절대 이어붙이지 않는다.

경계 처리:

- **문장부호 트리밍**: URL 끝의 `.`/`,`/`;`/`:`/`!`/`?`/따옴표/단독 `*`/`_`/`~` 는 한 글자씩 제거해
  링크 밖으로 뺀다(`https://example.com.` → `.` 는 링크 밖).
- **괄호 균형**: 매치된 URL **자기 자신 안의** `(`/`)` 개수만 비교한다 — 위키 URL처럼 URL 내부에서
  괄호가 짝이 맞으면(`Rust_(programming_language)`) 끝의 `)` 를 유지하고, 문장이 URL 을 감싸는
  괄호(`(https://example.com)`)처럼 URL 자체엔 없던 `)` 만 남으면 링크 밖으로 뺀다.
- **HTML entity**: `&amp;` 같은 entity 는 pulldown-cmark 파서가 `Event::Text` 에 도달하기 전에 이미
  실제 문자로 디코드해두므로, autolink 여부와 무관하게 `push_html` 이 나가는 길에 동일하게
  재이스케이프한다 — 이 pass 에 별도 entity 처리가 필요 없다.

새로 만들어진 `Tag::Link` 의 `dest_url` 은 명시적 마크다운 링크와 동일한 규칙으로
`#tasty-nav:link:<enc>` 프래그먼트로 재작성된다 — `autolink_bare_urls` 는 원본 URL 을 그대로
`dest_url` 에 넣은 `Tag::Link` 만 만들고, 이벤트 스트림 전체를 마지막에 한 번 통과하는
`rewrite_link_event` 가 명시적 링크와 자동 생성 링크를 구분 없이 동일하게 nav-fragment 로 바꾼다
(순서는 `unsafe_content_html` 의 주석 참조).

## 위키링크

Obsidian 스타일 `[[문서명]]`/`[[문서명|표시텍스트]]` 문법을 인식해 링크로 바꾼다
(`render.rs::resolve_wikilinks`/`split_wikilinks`/`wikilink_events`). Bare URL autolink(위 절)와
정확히 같은 아키텍처 — 라이브러리가 모르는 문법이라, `Event::Text` 를 이벤트 경계를 넘어 하나의
버퍼로 병합한 뒤(`autolink_bare_urls` 와 동일한 이유: 텍스트 하나가 여러 `Event::Text` 조각으로
쪼개져 들어올 수 있음) 그 위에서 스캔해 `Tag::Link` 로 쪼갠다. `link_depth`/`code_block_depth`
추적도 동일 — 이미 명시적 링크 안이거나 코드블록 안의 텍스트는 건드리지 않는다. 파이프라인
위치는 `figurize_solo_image_paragraphs` 다음, `autolink_bare_urls` 바로 앞(`unsafe_content_html`
주석 참조) — 둘 다 `rewrite_link_event` 이전에 원본 상대경로를 그대로 담은 `Tag::Link` 를 만들어
넘기므로, nav-fragment(`#tasty-nav:link:<enc>`) 로의 최종 재작성은 마지막에 한 번만 일어난다(위키링크
전용 재작성 로직 없음).

### 지원하는 스코프

- **파일 해석**: 현재 마크다운 파일과 **정확히 같은 디렉토리**(`base_dir`)에서 `<문서명>.md` 를
  대소문자까지 정확히 일치하는 파일명으로만 찾는다 — 일반 상대링크(`classify_link`)와 동일한
  디렉토리 규칙이고, 차이는 위키링크가 확장자 없는 이름만 주므로 `.md` 를 자동으로 붙인다는 점뿐이다.
  존재 여부는 렌더링 시점에 동기 `std::path::Path::exists()` 한 번으로만 확인한다(이 crate 는 이미
  `fs.read` 권한을 갖고 있어 별도 권한 확대 없음).
- **찾은 경우**: 일반 상대링크 `[text](문서명.md)` 와 완전히 동일한 모양의 `Tag::Link` 를 만들어
  기존 파이프라인에 그대로 흘린다 — 클릭 시 nav-fragment → `main.rs::dispatch_file_link` 로 이어지는
  기존 파일 열기 경로를 그대로 탄다(위키링크 전용 클릭 처리 없음).
- **못 찾은 경우**: 링크 자체는 그대로 만든다(destination 을 지우지 않음) — 클릭하면
  `dispatch_file_link` 의 기존 "존재하지 않는 파일은 로그만 남기고 조용히 무시" 경로를 그대로 탄다.
  다만 `.tasty-wikilink-missing` 스팬으로 감싸 시각적으로만 구분한다(점선 밑줄 + `accent-danger`
  색). `span`/`class` 는 이미 `sanitize_html` 화이트리스트에
  열려 있어(수식 렌더링 절 참조) 이 기능을 위한 sanitizer 변경은 없다.
- **`base_dir` 자체가 없는 경우**(저장 안 된 버퍼 등) — 일반 상대링크가 `classify_link` 에서
  해석 불가로 처리되는 것과 동일하게, 항상 "못 찾음" 취급한다.
- **표시 텍스트**: `[[문서명|표시텍스트]]` 형태면 `|` 뒤 텍스트를 링크 텍스트로 쓴다(비어 있으면
  문서명으로 폴백).
- **경로 순회 방지**: `<문서명>` 부분에 `/`, `\`, `..` 가 하나라도 포함되면(정상적인 단일-세그먼트
  이름이 아님) 위키링크로 인식하지 않고 원본 `[[...]]` 텍스트를 그대로 남긴다(파싱 실패로
  조용히 통과 — 에러 없음).
- **각주(`[^name]`)와의 비충돌**: `Options::ENABLE_FOOTNOTES` 활성 상태에서 각주는 애초에
  `Event::Text` 가 아니라 별개의 `Event::FootnoteReference`/`Tag::FootnoteDefinition` 이벤트로
  파서를 나오므로, 이 위키링크 스캐너가 볼 수 있는 대상 자체가 아니다 — 구조적 논거뿐 아니라
  같은 문서에 각주와 위키링크를 함께 넣어 실제 이벤트 스트림으로 확인한 테스트
  (`wikilink_does_not_collide_with_footnote_reference`)로도 검증했다.

### 지원하지 않는 것 (명시적 스코프 제외)

- **Vault 전체 재귀 검색** — 같은 디렉토리 밖은 전혀 뒤지지 않는다.
- **대소문자 무시 매칭 / alias** — 정확히 같은 파일명만 인식한다.
- **`[[문서명#섹션]]`(heading-anchor 조합)** — heading id 자동생성으로 기술적으로는 쉽게
  구현 가능하지만, 현재 지원하지 않는다.
- **`![[문서명]]`(embed 문법)** — 구현하지 않는다.

## 각주 backlink · 접근성

`Options::ENABLE_FOOTNOTES`(각주 `[^name]`/`[^name]: ...`)의 `pulldown-cmark` 기본 HTML 라이터는
참조 지점에서 정의로 가는 링크만 만들고, 정의에서 참조 지점으로 되돌아가는 backlink 나
`aria-label` 은 전혀 생성하지 않는다. `render.rs::rewrite_footnote_event` 가 실제
`Event::FootnoteReference`/`Tag::FootnoteDefinition` AST 이벤트를 가로채(콜아웃의
`rewrite_callout_events` 와 동일한 패턴 — 완성된 HTML 문자열이 아니라 파서 이벤트 자체를
매칭해 raw HTML로 쓴 각주 모양의 마크업과 구분한다) 다음을 직접 심는다:

- **참조 지점 고유 id** — `fnref-<safe-name>`(첫 참조), 같은 각주가 여러 번 참조되면
  `fnref-<safe-name>-2`, `-3`... 순으로 순번이 붙는다. 참조 `<a>` 는 `#fndef-<safe-name>`(정의)로
  링크한다.
- **정의 끝 backlink** — 정의 `<div id="fndef-<safe-name>">` 안, 콘텐츠 뒤에 `<a href="#fnref-...">
  ↩</a>` 를 참조 **횟수만큼** 추가한다(정의 하나에 backlink 하나만 있으면 어느 참조 지점으로
  되돌아갈지 모호해진다). 총 참조 횟수는 렌더링 실행 전 `footnote_reference_totals` 가 `source` 를
  한 번 더 파싱해 미리 센다 — 정의가 소스 상 참조보다 **앞에** 올 수 있어(`[^a]: ...`가 `[^a]`
  참조들보다 먼저 오는 문서도 유효), 단일 순방향 패스만으로는 정의 종료 시점에 최종 참조 횟수를
  알 수 없기 때문.
- **`aria-label`** — 참조는 `markdown.footnote.ref_aria`("각주 N으로 이동"), backlink 는 참조가
  1회면 `markdown.footnote.backlink_aria`("각주 N에서 본문으로 돌아가기"), 2회 이상이면
  `markdown.footnote.backlink_aria_nth`("각주 N의 M번째 참조로 돌아가기")로 어느 참조인지 구분한다.
  `lang/{en,ko,ja}.toml` 세 파일에 키가 있다. `sanitize_html` 은 `a` 태그에 한해 `aria-label` 을
  허용한다(모든 태그가 아니라 실제 필요한 `a` 로 범위를 좁힘).
- **id 안전화** — 각주 이름의 공백을 피하고 유니코드를 일관되게 표현하도록
  `percent_encode_fragment`(nav-fragment/아이콘 data URI 와 동일한 헬퍼)로 인코딩한다 — 충돌 없는
  단사 함수라 서로 다른 이름이 같은 id 로 뭉개지지 않는다.
- **미정의 참조**(`[^missing]` 인데 매칭되는 정의가 없음)는 `pulldown-cmark` 파서 자체가
  `FootnoteReference` 이벤트를 만들지 않고 `[`/`^missing`/`]` 평문으로 그대로 남긴다 — 크래시 없이
  원본 텍스트로 자연 폴백.

## Heading id + 목차(TOC)

제목(`h1`–`h6`)에는 텍스트에서 만든 자동 ID를 붙인다. 명시적 `# 제목 {#custom-id}`
문법(`Options::ENABLE_HEADING_ATTRIBUTES`)은 사용하지 않는다. `collect_headings`가
코드·링크·강조·이미지 alt의 텍스트를 모으고 `Slugger`가 소문자로 바꾼다.
유니코드 문자·숫자·`-`·`_`를 남기고 공백은 `-`로 바꾸며, 결과가 비면 `heading`을 쓴다.

정규화한 기본 이름이 반복되면 `-1`, `-2`를 붙인다. 이 횟수는 기본 이름별로 관리하므로
문서 전체 ID의 유일성을 보장하지는 않는다. 예를 들어 `x`, `x`, `x-1`은
`x`, `x-1`, `x-1`이 될 수 있다. GitHub와 완전히 같은 알고리즘이라고 보지 않는다.

`assign_heading_ids`가 같은 원문을 다시 파싱해 제목에 ID를 순서대로 넣는다.
HTML writer는 이 ID를 출력하고 sanitize의 `generic_attributes(["id"])`가 유지한다.

heading 이 하나 이상 있으면 문서 최상단(주소창 바로 아래, 본문 위)에 접을 수 있는 `<nav id="tasty-toc">`
목차가 삽입된다(`render.rs::toc_nav_html`) — sticky 사이드 패널이 아니라 인라인 삽입이다(레이아웃
CSS/스크롤 동기화 복잡도를 늘리지 않기 위한 설계 결정). 각 항목은 `<a href="#슬러그">` 로,
`rewrite_link_dest` 의 anchor-only 예외 경로(`#` 로 시작하는 dest 는 rewrite 없이 그대로 통과)를
그대로 태운다 — `#tasty-nav:` 라우팅을 새로 만들지 않는다. **클릭 자체는 엔진이 아니라 `nav_script` 가 처리한다**
— 위임된 `click` 리스너 하나가 fragment-only `href`(목차 항목 · 본문의 `[텍스트](#슬러그)` · 각주
참조/복귀)를 잡아 기본 navigation 을 취소하고 목적지 id 를 `scrollIntoView` 한다. 그래서 같은 항목을
연달아 눌러도 매번 이동하고(같은 hash 재대입은 엔진에게 무동작이다), 세 백엔드가 fragment navigation
을 어떻게 다루든 결과가 같다. `#tasty-nav:` 로 시작하는 href 는 이 리스너가 건드리지 않는다 — 그쪽은
host 신호 채널이라 `decide-policy` 까지 가야 한다. 문서에 `<base href>` 가 없는 것도 같은 이유다
(아래 기능 계약). 레벨별 들여쓰기는 `.tasty-toc-l1`..`l6`
CSS 클래스(`--md-space-sm` 배수)로 표현된다. 접기/펼치기는 `nav_script`(신뢰된 스크립트, 사용자
콘텐츠 아님)의 최소 JS 가 `#tasty-toc-toggle` 클릭 시 `#tasty-toc` 에 `tasty-toc-collapsed` 클래스를
토글하는 것으로 구현되며, 목록은 `max-height:280px;overflow-y:auto` 로 heading 이 많은 문서에서도
패널이 무한정 길어지지 않는다. `#tasty-addr-bar` 는 `position:sticky` 이고 **문서 어느 위치에서도 상단에 붙어 있다.**
그 바의 높이는 `--md-addr-bar-h` 한 자리에만 적혀 있고 아래 세 소비처가 그 이름을 읽는다.
주소창을 문서 끝까지 고정하려면 `html`에는 `height:100%`, `body`에는 `min-height:100%`를
사용한다. `body`를 고정 높이로 제한하면 긴 문서에서 주소창의 고정 범위가 한 화면에 그친다.
반대로 `html` 높이가 확정되지 않으면 짧은 문서의 `body`가 화면 높이를 채우지 못할 수 있다.

`stylesheet_lets_body_grow_while_html_stays_definite`와
`bar_height_is_declared_once_and_read_by_four_rules`는 생성 CSS의 선언만 검사한다.
엔진의 실제 배치는 Linux 전용 시험 `render::webview_layout_tests::address_bar_stays_on_top_in_webkitgtk`가
화면 밖 WebKitGTK 창(본체와 같은 webkit2gtk 4.1)에서 `getBoundingClientRect()`로 잰다.
하네스는 같은 HTML을 같은 엔진의 기본 설정으로 배치한다. 호스트가 WebView에 주는 zoom·JS 게이트·콘텐츠 필터는 재현하지 않는다.
뷰포트 1000×800에서 긴 문서를 끝까지 스크롤한 뒤와 앵커 이동 뒤에 주소창 `rect.top`이 0인지
(`scrollTop`으로 실제 스크롤도 확인한다), 짧은 문서의 `body` 높이가 800인지를 본다.
GTK3는 디스플레이가 있어야 하므로 이 시험은 `#[ignore]`이고, `check-headless`의 관측용 단계가
xvfb-run 안에서 이름으로 실행한다([CI 가이드](../../../dev-guide/ci-gates.md#webview-레이아웃-시험)).
macOS(WKWebView)·Windows(WebView2)의 실제 레이아웃 결과는 확인되지 않았다.

모든 heading 에 `scroll-margin-top:calc(var(--md-addr-bar-h) + var(--md-space-sm))` 을 줘, 앵커
이동한 heading이 주소창 아래에 가려지지 않게 한다.
같은 값이 `.footnote-reference`/`.footnote-definition` 에도 걸린다 — 각주도 앵커 이동의
목적지다. heading 이 하나도 없는 문서는 TOC 영역 자체가 렌더되지 않는다(빈 nav 로 깨지지
않게 — `render_document` 이 heading 목록이 비면 호출을 아예 건너뜀).

## 이미지 캡션

한 문단에 이미지(`![alt](src "title")`) **하나만** 있으면 그 문단 전체가
`<figure><img/><figcaption>{alt}</figcaption></figure>` 로 승격된다(`render.rs::
figurize_solo_image_paragraphs`) — alt 텍스트가 `alt` 속성(이미지가 정상 로드되면 화면에
보이지 않음) 안에만 머물지 않고 캡션으로 상시 노출된다. **정책: alt 텍스트가 있으면 항상
자동으로 승격한다 — opt-in 문법 없음.** alt 가 없는 이미지(`![](img.png)`)는 캡션 붙일 게
없으므로 기존처럼 렌더된다(회귀 없음).

승격 조건은 의도적으로 엄격하다: 그 문단에 이미지 외 다른 인라인 콘텐츠(본문 텍스트, 두 번째
이미지, 이미지를 감싸는 링크 `[![alt](img.png)](url)` 등)가 조금이라도 섞여 있으면 승격하지
않고 원래대로(인라인 `<img>` 그대로) 렌더한다. 이유: `Tag::Image` 는 `<p>` 내부의 **inline**
요소인데, 그 이벤트 하나만 `<figure>`(block 요소)로 감싸면 `<p><figure>...` 처럼 block 이
inline 컨텍스트에 중첩되는 잘못된 HTML이 되어 브라우저가 `<p>` 를 예측 못한 방식으로 조기
종료할 수 있다 — 그래서 이미지 하나만 있는 **문단 전체**를 통째로 승격하는 방식을 택했고, 그
조건이 깨지면 캡션을 포기하는 쪽이 잘못된 HTML을 만드는 것보다 안전하다고 판단했다.

캡션 텍스트는 이미지 alt span 안의 `Text`/`Code` 이벤트만 이어붙인 순수 텍스트다(강조 등
마크업은 제거 — heading 텍스트 수집(`collect_headings`)과 동일한 방식). 이 텍스트는 일반
`Event::Text` 로 방출되므로 `push_html` 이 본문 텍스트와 동일하게 HTML-escape 한다 — alt 에
`<`/`>`/`&` 같은 문자가 있어도 캡션에 안전하게 이스케이프된 채로만 나타난다. `<figure>`/
`<figcaption>` 자체는 이 파일이 이미 쓰는 "신뢰된 plugin 작성 마크업" 패턴(`Event::Html`,
`wrap_static_callout` 의 `<blockquote data-label="...">` 삽입과 동일)으로
주입되므로, `sanitize_html` 화이트리스트에도 별도 속성 없이 bare tag 로 추가했다.

## 이미지 로드 실패 상태 UI

경로 오타·파일 이동 등으로 이미지 로드가 실패하면 브라우저 기본 "깨진 이미지" 아이콘 대신
경로가 보이는 tasty 자체 실패 UI(`.tasty-img-error`)로 교체된다(`render.rs::
image_error_script`). 문서에 `<img` 가 하나도 없으면(대다수 비-이미지 문서) 이 스크립트 자체가
삽입되지 않는다 — 코드블록 복사 버튼/mermaid/highlight.js 와 동일한 조건부 삽입 절약 패턴.

- **왜 `onerror=` 인라인 핸들러가 아니라 신뢰된 스크립트인가**: `sanitize_html` 은 모든 인라인
  이벤트 핸들러를 무조건 제거한다(XSS 방어의 핵심 축이라 예외를 두지 않음). 대신 코드블록 복사
  버튼과 동일한 패턴 — sanitize 이후 신뢰된 plugin 코드가 렌더 시점에 `addEventListener('error',
  ...)` 를 DOM에 직접 부착한다.
- **이미 실패가 끝난 이미지 보완**: 신뢰된 스크립트가 실행되는 시점은 문서 하단이라, 리스너를
  붙이기 전에 이미 `error` 이벤트가 발생해버린 이미지가 있을 수 있다. 그래서 리스너 부착 직후
  각 `<img>`에 대해 `img.complete && img.naturalWidth === 0`(로드는 끝났는데 실제 픽셀이 없음 =
  실패, `![alt]()`처럼 `src` 가 아예 빈 경우도 이 조건으로 자연히 잡힌다)도 함께 검사한다.
- **표시 경로**: `img.getAttribute('src')`(마크다운에 적힌 원본 상대경로)를 그대로 쓴다.
  `img.src`(프로퍼티)는 엔진이 문서 자신의 주소 기준으로 절대화해버린 값이라 사람이 읽기 나쁘다.
- **접근성**: 플레이스홀더는 `role="img"`이고, 원본 `alt`가 있으면 그대로 `aria-label`로
  보존한다(없으면 실패 안내 라벨 자체를 aria-label로 폴백).
- **원격 이미지 차단 정책과의 관계**: host의 `allow_remote_content` 정책으로 원격 이미지 요청이
  막히면 그 요청은 실제로 실패하므로 `<img>`의 `error` 이벤트가 정당하게 발생한다 — 오탐이
  아니라 정확한 실패 표시다. `sanitize_html`의 `url_schemes`가 `http`/`https`/`mailto`만 허용하고
  `loading` 속성 자체를 허용 attribute 목록에 넣지 않아(위 `sanitize_html` 절) `loading="lazy"`가
  살아남지 않으므로, 브라우저 네이티브 lazy-loading으로 인한 "아직 로드 시도 전"과 "로드
  실패"의 혼동 여지도 구조적으로 없다.
- **중복 부착 방지**: `reload_webview`가 재로드마다 문서 전체를 통째로 교체하는 것은 코드블록
  복사 버튼과 동일(리스너가 재로드마다 누적될 구조적 경로 없음). 스크립트 자체도 `<img>` 마다
  `data-tasty-img-checked`(리스너 중복 부착 방지)/`data-tasty-img-failed`(플레이스홀더 교체
  idempotent화, 리스너 경로와 즉시-검사 경로가 동시에 fire해도 안전) 두 데이터 속성으로
  방어한다.
- **테마 연동**: 아이콘은 `tasty_icons::IMAGE` 글리프를 `theme.accent_danger()`로 만든 data URI로
  심는다(GFM alert 아이콘과 동일한 `render/callout.rs::alert_icon_data_uri` 재사용). 테두리/라벨 색도
  같은 `danger` 토큰, 경로 텍스트는 `.tasty-state-detail`과 동일한 `muted` 토큰 — 별도 실패 UI
  전용 토큰 없이 기존 에러 상태 배색을 그대로 재사용한다.

**검증 범위**: Linux/WebKitGTK에서 정상 이미지와 실패 이미지를 함께 띄워 실패한 것만
`.tasty-img-error`로 바뀌고 alt·상대경로가 보존되는 것을 확인한 이력이 있다.
스크립트를 두 번 적용해도 placeholder가 늘지 않았다. 이 확인은 당시 `file://` base URI로
수행했으며, 현재 문서의 인라인 이미지 처리 전체를 검증한 결과로 확대하지 않는다.
macOS·Windows의 실제 실행 결과는 확인되지 않았다.

## 문서 내 검색(find-in-page)

문서(webview 콘텐츠)에 실제 키보드 포커스가 있는 상태에서 단축키 설정 `find` 의 조합(기본
프리셋 `Ctrl+F`·`Alt+F`, macOS 에서는 Control+F·Cmd+F)을 누르면 문서 내 텍스트 검색 바가 열린다
(`render.rs::find_bar_html`/`find_in_page_script`). 페이지는 host 설정을 받지 않으므로 plugin 이
문서를 만들 때마다 `settings.get_keybinding {action:"find"}` 로 바인딩을 읽어
`KeyboardEvent` 판정 값(`render/find_keys.rs` — US 물리 키 `code` 와 정확한 modifier, macOS 는
`alt`→Command·`option`→Option)으로 바꿔 스크립트에 넣는다. 설정이 저장한 바인딩은 modifier 유무와
관계없이 모두 넣는다. `ctrl`·`alt`·`option` 이 없고 F1~F24 가 아닌 바인딩(`f`·`shift+f`·`enter` 등)은
글자 입력과 겹치므로, 입력칸(input·textarea·select·contenteditable)에 포커스가 있거나 IME 조합 중일 때는
찾기로 가로채지 않는다. `F3`·`Shift+F3` 같은 기능 키는 어디서나 찾기를 연다. 조회에 실패하면 경고를 남기고 기본 프리셋 값(`ctrl+f`·`alt+f`)을
쓴다. 사용자가 설정을 저장해 `find` 가 바뀌면 host 가 `command.host_shortcut_changed`
(`action_ids` 에 `find`)를 발행하고 plugin 은 열린 문서를 모두 다시 만든다. 검색 자체는
markdown plugin 문서에 내장된 신뢰 스크립트만으로 동작한다(네이티브 find API
(`WebKitFindController`/`WKWebView.find`/WebView2 `Find`)는 세 엔진이 정규식/전체단어 지원이
서로 달라 채택하지 않았고, 매치 카운트를 host 로 되돌려받으려면 백엔드별 비동기 신호 처리가
추가로 필요해 이 markdown-plugin-local 기능치고는 host API 확장 비용이 과했다).

검색 바(입력창 + `n/m` 카운터 + ▲▼ prev/next + close)는 문서 우상단에 `position:fixed` 로
떠 있으며(갤러리 `search_bar` specimen 과 동일한 "sticky, non-modal, 우상단" 배치 — scrim 없음),
기본은 `hidden`. 매치는 DOM `TreeWalker`(`NodeFilter.SHOW_TEXT`)로 `#tasty-md-body` 의 텍스트
노드를 순회해 찾고, `<mark class="tasty-find-hit">` 로 감싼다 — 현재 매치는
`tasty-find-current` 클래스가 추가로 붙어 다른 색으로 구분된다. `<pre>`/`<code>` 조상을 가진
텍스트 노드는 스캔에서 제외된다(코드블록은 검색 범위 밖 — 정책 결정). 매 검색(입력 150ms
debounce, `compositionend` 시 즉시)마다 이전 `<mark>` 를 먼저 원문으로 복원(`Node.normalize()`)한
뒤 다시 스캔하므로 하이라이트가 누적되지 않는다. IME 조합 중(`compositionstart`~`compositionend`)
에는 검색을 트리거하지 않는다. `Esc` 는 바를 닫고 하이라이트를 완전히 제거(DOM 원상복구),
`Enter`/`Shift+Enter` 는 다음/이전 매치로 이동(래핑) + `scrollIntoView`.

**터미널 검색과의 구분**: 호스트 `kb.find`는 focused surface가 Terminal일 때만
터미널 검색 popup을 연다. 다른 kind에서는 키를 소비하지 않아 페이지의 검색 기능이
처리할 수 있다. markdown에서는 이 절의 문서 내 검색을 사용한다.

**host 키 포워딩과의 관계**: webview 자식 창이 키보드 입력을 받는 동안에도 tasty 전역
단축키는 동작한다 — 세 백엔드가 native 키를 가로채 host 로 올린다
([WebView 호스트 계약](../../../design/systems/webview.md),
[features/keybindings](../../../features/keybindings/index.md)). 그 포워딩은 **`find` 를
포함한 페이지 예약 액션(`find`·`copy`·`cut`·`paste`·`select_all`·`page_undo`·`page_redo`)을 애초에 가져가지
않으므로**, 사용자의 `find` 조합은 위 kind 게이트 이전에 이미 페이지로 간다. 페이지 스크립트도
같은 `find` 바인딩을 판정하므로 둘은 같은 조합을 본다. `find` 에서 뺀 조합(예: `ctrl+f`)을 다른 host
동작에 주면 host 가 그 조합을 가져가고 페이지는 더 이상 그 조합으로 검색 바를 열지 않는다. 한 조합을
두 동작이 갖는 상태는 단축키 탭의 충돌 확인을 거쳐야 저장되므로 조용히 겹치지 않는다. 검색 바 입력창에 커서가 있을 때의 타이핑도
안전하다: 포워딩 대상은 `ctrl`/`alt`/`option` 을 가진 콤보뿐이라 문자키·`shift+문자`·
`Esc`/`Enter`/`Shift+Enter` 는 전부 페이지가 그대로 받는다.

## 앱 문구의 한자 형태

주소 띠·검색 바·목차 라벨·상태 화면·콜아웃 라벨 같은 앱 문구와 문서 본문은 `lang` 속성 없이 같은 글꼴 스택으로 그려진다. 스택에 CJK 글꼴이 없어 한자는 WebView와 OS의 글꼴 대체에 맡겨지고, 앱 언어(`general.language`)는 한자 형태에 영향을 주지 않는다. 그래서 중국어 간체 UI에서도 앱 문구가 일본어 형태로 나올 수 있다.

Linux(WebKitGTK 2.50.4, Noto Sans CJK 전 변종 설치, 전용 Xvfb) 측정값이다. OS 로케일은 `LANG`만 바꿨고 앱 언어는 시험 언어팩이다. Windows(WebView2)와 macOS(WKWebView), 글꼴 구성이 다른 Linux는 측정하지 않았다.

| 대상 | `ko_KR`·`ja_JP`·`en_US` | `zh_CN` |
|---|---|---|
| 앱 문구와 문서 본문 | 일본어 형태 | 간체 형태 |
| 네이티브 tooltip(`title`) | 일본어 형태(`ko_KR`만 측정) | 간체 형태 |

같은 환경의 HTML 대조 페이지에서 요소에 `lang="zh-Hans"`를 붙이면 네 로케일 모두 간체 형태로 나왔다. 일반 글자, `::before`의 `content`, `input` placeholder가 같았다. 네이티브 tooltip은 `lang`을 붙여도 일본어 형태였다(`ko_KR`). 이 플러그인의 앱 문구에는 아직 `lang`을 붙이지 않았다. 예외는 원격 실패 사유의 영어 원문(`<span lang="en">`)이다. WebView 내용은 `ui.screenshot`에 담기지 않으므로 형태 확인은 [OS 화면 캡처](../../../ai-verification/screenshot-methods.md#os-화면-캡처)로 한다.

## 디자인 토큰 매핑

본문 스타일은 plugin의 CSS와 호스트 Theme 토큰을 사용한다. [시각 소스](#시각-소스) 참고.

## 갤러리 specimen

`crates/tasty-gallery/src/catalog/components/markdown_viewer.rs` — Layouts › `Content viewers` ›
`Markdown surface`. 갤러리는 live webview 를 띄우지 않으므로, 위 CSS 출력을 egui `Frame`/`Label`
로 **손으로 근사**한다(픽셀 동일성은 비목표) — 헤딩/문단/링크/리스트/코드블록/표(격자+zebra)/캡션
대표 문서 + 주소창 chrome 의 정적 근사 + TOC chrome 의 정적 근사(`toc_chrome` — 접기/펼치기·클릭
스크롤 같은 라이브 상태는 없고 항상-펼침 스냅샷 하나, 레벨별 들여쓰기만 CSS `.tasty-toc-l<N>` 과
동일 비율로 미러) + 시안 `Callouts` 표본 두 개(note · 접힌 warning, 아래 콜아웃 종류 카드와 같은 상자) + 코드블록(`code_block` —
`fn main() { format!("hi from tasty"); }` 를 highlight.js 의 rust 문법이 나눌 토큰 그대로 손으로
분할해 `hljs-*` scope 별 `Theme` hue 색을 입힌 `CodeToken` 런, 라이브 highlight.js 실행 결과의
정적 근사).
같은 섹션의 `Markdown callout kinds` 카드(`markdown_viewer/callout_kinds.rs`)는 렌더러가 그리는
콜아웃 다섯 종을 본체 모양 그대로 보인다 — 종류별 색 `tint-fill-alpha` 채움(egui 에 반투명 색을
넘기지 않고 문서 바탕 `md-doc-bg` 위에서 sRGB 로 미리 섞은 불투명 색), 왼쪽 `md_quote_bar_width`
막대, radius, 16px 종류 아이콘과 굵은 라벨(space-xs 간격). 시안 `Callouts` 도 같은 상자를 그리므로
문서 카드의 두 표본과 이 카드가 `callout_box` 하나를 함께 쓴다.
디자인·갤러리·제품 구현의 대응표: [design-gallery-mapping.md](../../../design/systems/design-gallery-mapping.md#surface-viewers-plugins).

## 시각 소스

plugin 이 host `theme.query` IPC 로 조회한 Theme 토큰을 CSS 로 문서에 주입해 자가 렌더(host 는
native WebView 로 그 문서를 표시만 함). 전용 vendor 시안은 현재 없다.
