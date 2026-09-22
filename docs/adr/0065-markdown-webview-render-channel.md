# ADR-0065: markdown surface 는 EguiMesh 대신 Webview(HTML+CSS) 로 렌더한다 — ADR-0028 의 markdown B1 선례 조항 개정

- **Status**: Accepted
- **Date**: 2026-08-10
- **Tags**: plugin, render-channel, webview, html, markdown, egui-mesh, typography, mermaid, surface-kind, host-rendered-removal, adr-0028
- **Group**: plugin-render

## Context

[ADR-0028](0028-plugin-egui-mesh-render-channel.md) 은 markdown 을 EguiMesh 채널로의 "첫 마이그레이션 선례(B1)" 로 명시했고, 그 조항의 "채널 일원화 완료 시 plugin-content 채널은 `EguiMesh`(+image 비트맵의 Canvas) 와 html 의 `Webview` 만 남는다" 는 문구에 따라 markdown 은 EguiMesh 로 확정된 상태였다(`crates/tasty-plugin-markdown/src/render.rs:1-4`, 커밋 f91bfd6b). 이 결정을 재검토해야 하는 근거가 이후 두 축에서 쌓였다.

**타이포그래피 격차 (egui_commonmark 의 구조적 한계).** markdown 은 egui_commonmark 0.20 (`egui_commonmark::CommonMarkViewer`, `crates/tasty-plugin-markdown/Cargo.toml:29`, 내부적으로 pulldown-cmark 0.12 사용 — 같은 파일 22-43행 주석)로 렌더한다. 실제 문서를 VSCode 마크다운 프리뷰와 나란히 비교하면 줄간격(line-height)·리스트 항목 간 문단 여백이 눈에 띄게 좁아 가독성이 크게 떨어진다. 이는 버그가 아니라 라이브러리가 line-height override 자체를 노출하지 않는 구조적 한계다 — `crates/tasty-plugin-markdown/src/render.rs:8-11` 이 이를 "library-driven constraint" 로 이미 명시하고 있고, `tokens/semantic.css` 의 `prose-h2`·`line-height-prose` 토큰은 "라이브러리가 헤딩 보간·본문 leading 을 소유해 은퇴 확정"으로 문서화돼 있다(`docs/design/systems/design-token-mapping.md:49`, 커밋 aaec7394/bd35a476). 즉 본문 leading·per-heading 픽셀 크기 제어권을 이미 라이브러리에 넘긴 상태이며, 추가 패치로는 VSCode 수준 타이포그래피에 도달할 수 없다.

**확장 불가 구조 (mermaid 미지원).** egui_commonmark 는 CommonMark 파싱 자체는 표준 준수이지만, fenced code block 렌더 지점에 언어별 확장 훅이 없어 mermaid 같은 커스텀 렌더를 라이브러리 안에서 끼워 넣을 방법이 없다(vendor 크레이트 `egui_commonmark_backend` 의 `CodeBlock::end` 렌더 경로 확인). 이는 별개로 요청됐던 mermaid 다이어그램 지원 문제이기도 하다.

**업계 관행과의 괴리.** CommonMark 스펙의 레퍼런스 구현은 HTML 을 출력 타깃으로 삼고, GitHub/VSCode/GitLab 등은 예외 없이 브라우저 엔진(HTML+CSS)으로 마크다운을 렌더한다. egui immediate-mode 툴킷으로 이 관행을 흉내내는 현재 접근이 비주류다.

**과거 렌더러 교체 이력.** hand-rolled pulldown-cmark → egui 레이아웃 렌더러(커밋 950d1bb1, 2a030365)로의 전환 목적은 "표/체크박스/링크 등 CommonMark 표준 기능을 유지보수 부담 없이 얻는 것"이었고, 그 대가로 타이포그래피 세부 제어를 잃었다. Webview 전환은 그 표준 기능을 유지한 채로 이 트레이드오프 자체를 제거한다.

**스키마 제약.** `SurfaceKindRendering`(`crates/tasty-plugin-manifest/src/types.rs:516-536`) 은 `Remote`/`Webview`/`EguiMesh` 가 상호 배타적인 단일 값이고, `[[surface_kinds]] rendering = "..."` 도 필드 하나뿐이다 — 한 surface_kind 가 "egui-mesh chrome + webview body" 하이브리드로 선언될 스키마가 없다. 또한 `crates/tasty-plugin-html/src/main.rs`(약 239줄)는 egui mesh paint 로직이 전혀 없는 순수 IPC 트램폴린(`webview.set_url` 호출만)이고, `src/adapters/ui/surface/webview_chrome.rs:1-28` 은 webview kind surface 에서 host 가 그리는 chrome 이 overlay 가 아직 안 붙었거나(placeholder) 일시적으로 가려질 때(boundary)만 노출되는 fallback 이라고 명시한다 — `rendering = "webview"` 로 선언된 surface 는 plugin 이 커스텀 egui-mesh 콘텐츠를 그릴 채널 자체가 없다.

## Decision

**markdown surface 의 `rendering` 을 `"egui-mesh"` 에서 `"webview"` 로 전환한다.** plugin 은 pulldown-cmark(또는 comrak) 의 HTML writer 로 markdown 을 HTML 로 변환하고, 번들 CSS(VSCode 프리뷰 수준 타이포그래피 + mermaid 렌더용 스크립트)를 적용해 host 의 native webview overlay 에 올린다. mermaid fenced block 은 HTML 출력 시 표준 `<div class="mermaid">` 로 변환되어, 별도 plugin 확장 없이 번들 mermaid.js(또는 동등 라이브러리)가 브라우저 엔진 안에서 렌더한다.

이 결정은 [ADR-0028](0028-plugin-egui-mesh-render-channel.md) 의 "markdown 은 EguiMesh 마이그레이션의 첫 선례(B1)" 조항과 "최종 채널은 EguiMesh(+image 비트맵의 Canvas) 와 Webview(html) 둘" 조항 중 **markdown 에 한해** 개정한다 — [ADR-0030](0030-image-egui-mesh-bitmap-texture.md) 이 image 의 Canvas-하이브리드 조항만 부분 개정하고 나머지("egui-mesh 채널 일원화, popup/banner, host-rendered 전면 제거 방향")는 그대로 유효하다고 명시한 것과 동일한 패턴이다. ADR-0028 의 popup·banner EguiMesh 방향, image 의 mesh-only 결정(ADR-0030), host-rendered(`Host`/`Remote` UiNode) 전면 제거 방향은 이 ADR 로 바뀌지 않는다 — **surface-kind 렌더 채널 중 markdown 하나만** EguiMesh 군에서 Webview 군으로 이동한다.

## Consequences

- **얻은 것**:
  - VSCode/GitHub 수준 타이포그래피 — line-height·문단 여백을 CSS 로 완전히 제어(라이브러리 제약 소멸).
  - mermaid 다이어그램 지원 — 별도 요청이었던 기능이 채널 전환의 부수 효과로 해결된다.
  - CommonMark 확장 어휘(footnote, 커스텀 code-block 렌더 등)가 필요해질 때 HTML/CSS/JS 생태계 전체를 재사용 가능 — pulldown-cmark HTML writer 는 egui_commonmark 보다 표준에 더 가깝다.
  - 표·코드 하이라이팅이 오히려 쉬워진다 — CSS 테이블 레이아웃과 웹 기반 syntax highlighter(예: highlight.js)를 그대로 붙일 수 있어, egui `Grid::striped` 로 흉내내던 제약(헤더 밴드·불투명 베이스 fill·셀 패딩 미지원, `render.rs:13-18`)이 사라진다.
- **잃은 것**:
  - **주소창 chrome(PathField) 재구현 필요.** webview kind 는 plugin 에 egui-mesh 페인트 채널을 주지 않으므로(`webview_chrome.rs` 는 host 소유 fallback 일 뿐), markdown surface 가 지금 갖고 있는 주소창(`PathField`, 경로 편집·히스토리 드롭다운·Go 버튼, `crates/tasty-plugin-markdown/src/main.rs:1228-1289`)을 그대로 유지하려면 (a) 웹뷰 bounds 를 주소창 높이만큼 줄여 host 가 별도 egui chrome 을 얹거나, (b) 주소창 자체를 HTML 로 재구현해야 한다. 어느 쪽이든 후속 구현의 설계 대상이다 — 이 ADR 은 결정만 기록하고 구현 방식을 선택하지 않는다.
  - **링크 클릭 라우팅 재구현 필요.** 현재 `LinkClick::File`/`External` 판정 로직(`render.rs:36-39`)은 egui_commonmark 의 클릭 콜백을 통해 얻는다. webview 로 전환하면 페이지 내 링크 클릭이 native webview 의 navigation 이벤트로 오므로, file link 를 가로채 host `file_handler.dispatch` 로 보내는 경로를 webview navigation 인터셉트 기반으로 다시 짜야 한다.
  - **텍스트 선택 방식 변경.** 현재 egui `TextEdit`/CommonMarkViewer 내부 selection 대신 native webview 의 브라우저 텍스트 선택으로 바뀐다 — 선택 상태를 host 가 관찰·제어하던 경로(있었다면)가 사라지고 OS webview 의 selection API 에 의존하게 된다.
  - EguiMesh 채널의 이점(DPI 선명도, 격리된 텍스처 파이프라인 재사용, mesh 프레임 invalidate-only 전송)을 markdown 은 더 이상 누리지 않는다 — 대신 webview 자체의 native 렌더 해상도·리소스 모델을 따른다.
- **운영 비용 / 유지 부담**:
  - 번들 CSS(+ mermaid 스크립트)의 유지 — 테마 변경 시 webview 쪽 CSS 도 함께 갱신해야 토큰 정합이 깨지지 않는다.
  - markdown surface 하나만 webview 렌더 경로로 남아, plugin-content 렌더 표면이 다시 두 채널(EguiMesh 대다수 + markdown 만 Webview)로 갈라진다 — ADR-0028 이 지향한 "일원화"에서 이 한 kind 만큼은 후퇴다. 이는 mermaid·타이포그래피 요구가 EguiMesh 로는 근본적으로 풀리지 않는다는 판단과 맞바꾼 의도적 트레이드오프다.

## Alternatives Considered

- **A. egui_commonmark 를 포크해 line-height 패치**: 상류 라이브러리가 leading·per-heading 크기를 노출하지 않는 구조이므로, 포크 유지 자체가 매 egui_commonmark 업데이트마다 리베이스 비용을 발생시킨다. mermaid 미지원 문제도 별도로 해결해야 해 두 문제 중 하나만 푼다 → **기각**.
- **B. `UiNode` DSL 확장(인라인 서식·테이블·code-block 언어 훅 노드 추가)**: ADR-0028 이 이미 "사실상 egui 재구현으로 DSL 이 비대해지고 표현력을 영원히 따라가야 함"으로 기각한 대안이며, 이 결론은 그대로 유효하다(재확인만) → **기각**.
- **C. 자체 hand-rolled egui prose 렌더러 부활**: markdown 이 egui_commonmark 로 전환되기 전 상태(커밋 950d1bb1 이전)로 되돌리는 길. line-height·mermaid 는 직접 구현 가능해지지만, CommonMark 표준 준수(표/체크박스/링크 등)를 처음부터 다시 구현·유지해야 해 950d1bb1/2a030365 가 없애려던 부담을 그대로 되살린다 → **기각**.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

- 다중 마크다운 탭을 동시에 열었을 때 native webview 인스턴스 수에 따른 리소스 비용(메모리·프로세스)이 체감 임계를 초과한다.
- 주소창 chrome 재구현(웹뷰 bounds 분할 vs HTML 재구현) 스파이크 결과 둘 다 markdown 의 focus-independence·에이전트 조작 가능성 원칙(`docs/identity.md` 원칙 2·3)과 충돌하는 것으로 드러난다.
- egui_commonmark 상류가 line-height override 와 언어별 code-block 확장 훅을 모두 노출하는 방향으로 바뀐다(→ EguiMesh 로 되돌아갈 근거 재발생).
- webview 링크 navigation 인터셉트가 플랫폼(WebView2/WKWebView/WebKitGTK) 간 동작 차이로 file-link 라우팅을 신뢰성 있게 구현할 수 없는 것으로 드러난다.

## References

- 개정 대상: [ADR-0028](0028-plugin-egui-mesh-render-channel.md) (markdown 을 EguiMesh 첫 선례(B1)로 명시한 조항, 최종 채널 구성 조항).
- 개정 패턴 선례: [ADR-0030](0030-image-egui-mesh-bitmap-texture.md) (image Canvas-하이브리드 조항만 부분 개정하고 나머지는 유효로 남긴 방식).
- 코드 근거: `crates/tasty-plugin-markdown/src/render.rs:1-18`(현재 egui_commonmark 렌더·library-driven constraint 주석), `crates/tasty-plugin-markdown/Cargo.toml:22-43`(egui_commonmark/pulldown-cmark 버전), `crates/tasty-plugin-markdown/src/main.rs:1228-1289`(PathField 주소창), `crates/tasty-plugin-manifest/src/types.rs:516-536`(`SurfaceKindRendering`), `crates/tasty-plugin-html/src/main.rs`(webview kind IPC 트램폴린 선례), `src/adapters/ui/surface/webview_chrome.rs:1-28`(webview host chrome fallback 정책), `docs/design/systems/design-token-mapping.md:49`(prose-h2/line-height-prose 은퇴 문서화).
- 이력 커밋: 950d1bb1(egui_commonmark 최초 도입), 2a030365(plugin 재도입), f91bfd6b(EguiMesh B1 전환), aaec7394/bd35a476(prose-h2/line-height-prose 토큰 은퇴).
- 실제 구현이 위 Decision/Consequences 의 mermaid·주소창·링크 라우팅 서술과 갈라지는 지점(Stage B 스코프 축소, HTML 내장 주소창 등)은 [ADR-0067](0067-markdown-webview-stage-b-scope-correction.md) 참고 — 이 ADR 은 Accepted 상태로 본문을 고정하고, 실제 구현과의 차이는 별도 ADR 로 기록한다.
