# ADR-0067: markdown webview 전환(ADR-0065) Stage B 구현은 mermaid 렌더링을 포함하지 않는다 — 스코프 정정

- **Status**: Accepted
- **Date**: 2026-08-10
- **Tags**: plugin, render-channel, webview, html, markdown, mermaid, sanitize, adr-0065, scope-correction
- **Group**: plugin-render

## Context

[ADR-0065](0065-markdown-webview-render-channel.md) 의 Decision/Consequences 는 markdown 을 EguiMesh 에서 Webview(HTML+CSS) 로 전환하면 "mermaid fenced block 이 표준 `<div class="mermaid">` 로 변환되어 번들 mermaid.js 가 브라우저 엔진 안에서 렌더한다" 고 서술했다 — 즉 채널 전환의 **부수 효과로 mermaid 지원이 곧바로 해결된다**는 전제였다.

Stage B(실제 구현, `crates/tasty-plugin-markdown`)는 이 전제를 확정하기 전에 두 가지가 먼저 필요하다는 것을 드러냈다:

1. **sanitize 파이프라인 확정.** webview 에 올리는 HTML 은 파일 콘텐츠에서 유도되므로 XSS 표면이다 — Stage B 는 `ammonia` 로 최소 GFM 허용목록 sanitize 를 적용했다(`javascript:` 링크 차단 포함). fenced code block 의 언어 태그(`class="language-<lang>"`)는 sanitize 를 거치며 통째로 날아가지 않도록 별도로 살려두는 정규화가 필요했다.
2. **mermaid.js 배선은 별도 결정 대상.** 번들에 mermaid.js(또는 동등 라이브러리)를 실제로 포함시키고, sanitize 허용목록에 `class="mermaid"` div 를 추가하고, 스크립트 실행을 sanitize 정책과 어떻게 공존시킬지는 ADR-0065 가 다루지 않은 별도의 보안/번들크기 트레이드오프다.

ADR-0065 는 Accepted 상태라 본문(Decision/Consequences)을 직접 고칠 수 없다([`template.md`](template.md) 작성 규칙) — 이 ADR 이 그 차이를 별도로 기록한다.

## Decision

**Stage B 는 fenced code block 의 언어 태그가 sanitize 를 거쳐도 살아남도록 정규화하는 것까지만 포함한다.** 이는 mermaid 를 그 자리에서 바로 렌더하기 위함이 아니라, **향후** mermaid fenced block(`class="language-mermaid"`)을 식별할 수 있는 최소 전제를 마련해 두는 것이다. `<div class="mermaid">` 변환 + 번들 mermaid.js 배선 자체는 이 ADR 의 스코프 밖이며 별도 후속 작업이다.

같은 이유로 ADR-0065 의 Consequences "잃은 것" 항목 중 주소창 chrome 재구현·링크 클릭 라우팅 재구현은, Stage B 가 실제로 고른 구현(주소창을 HTML/CSS/최소 JS 로 문서 자체에 내장, 링크는 nav-fragment 스킴으로 webview navigation 인터셉트 기반 재구현)으로 확정됐다 — ADR-0065 는 "구현 방식을 선택하지 않는다"고 명시했으므로 이 확정 자체가 그 ADR 의 결정을 뒤집지 않는다.

## Consequences

- **얻은 것**: mermaid 지원 여부에 대한 이해관계자 기대가 ADR-0065 본문만 읽었을 때 생기는 오해("전환하면 바로 mermaid 가 된다")와 어긋나지 않게 됐다 — Stage B 완료 시점에 mermaid 다이어그램이 실제로 렌더되지 않는다는 것이 문서로 명확하다.
- **잃은 것**: mermaid 요구가 있는 사용자는 이 ADR 이후에도 별도 후속 작업(번들 mermaid.js 배선 + sanitize 허용목록 확장)을 기다려야 한다 — ADR-0065 시점의 기대보다 지연된다.
- **운영 비용 / 유지 부담**: 없음 — 문서 정정이며 코드/동작 변경을 유발하지 않는다.

## Alternatives Considered

- **A. ADR-0065 본문을 직접 수정**: `template.md` 의 "Accepted 후 본문 불가변" 규칙 위반. Accepted ADR 의 References 는 링크 위생만 예외로 허용하고, Decision/Consequences 같은 결정 내용은 대상이 아니다 → **기각**.
- **B. ADR-0065 를 Superseded 로 전환하고 신규 ADR 로 전체 재작성**: mermaid 서술 정정은 ADR-0065 의 핵심 결정(EguiMesh→Webview 채널 전환) 자체를 뒤집지 않는다 — "결정이 바뀌었다"기보다 "구현 스코프가 결정 시점 서술보다 좁았다"는 사실 정정에 가깝다. Superseded 로 표시하면 채널 전환 결정 자체가 재검토 대상인 것처럼 오독된다 → **기각**.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

- mermaid.js 번들 배선 + sanitize 허용목록 확장이 실제로 구현되어 mermaid 다이어그램이 렌더되기 시작한다 (→ 이 ADR 의 "mermaid 미지원" 서술이 다시 stale 해지므로 갱신 또는 새 ADR 필요).

## References

- 정정 대상: [ADR-0065](0065-markdown-webview-render-channel.md) (Decision 의 mermaid 즉시 지원 서술, Consequences 의 주소창/링크 라우팅 구현 방식 미확정 서술).
- 코드 근거: `crates/tasty-plugin-markdown/src/render.rs`(`sanitize_html`(ammonia) + 언어 태그 정규화 + `addr_bar_html`/`nav_script` + `classify_link`/`parse_nav_fragment`), `crates/tasty-plugin-markdown/src/main.rs`(`on_webview_navigation_attempt` 핸들러).
