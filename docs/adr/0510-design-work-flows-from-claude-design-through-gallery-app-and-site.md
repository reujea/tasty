# ADR-0510: 디자인 작업은 Claude Design 시안을 갤러리 → 본체 → 사이트 사본 순으로 정합한다 — 갤러리는 본체 UI 의 완전한 단일 출처다

- **Status**: Accepted
- **Date**: 2026-09-23
- **Tags**: design-workflow, claude-design, gallery, gallery-first, design-parity, component-catalog, site, vendor, guards, adr-0138, adr-0506
- **Group**: ui-theme-gallery

## Context

디자인 작업 — 어떤 도구로 시안을 받고, 받은 시안을 어디에 어떤 순서로 반영하며, 무엇을 반영
완료로 치는가 — 에 관한 결정이 ADR 여러 편에 흩어져 있었다. 디자인 작업을 처음 하는 사람은 그
여러 편과 운영 문서를 함께 읽어야 했고, 그중 일부는 더는 쓰지 않는 단계를 현재형으로 적었다.
사용자 결정(2026-09-21)으로 그 결정들을 이 한 편에 모으고 원본 파일은 지운다
([ADR-0506](0506-a-decision-recorded-twice-is-split-by-the-width-of-the-overlap.md) 의 "흡수 후
삭제", 번호는 결번).

**도구.** 시안은 Claude Design(claude.ai/design) 에서 받는다 — 실제로 렌더되는 HTML/CSS 시안이라
색·간격·인터랙션을 살아 있는 채로 탐색할 수 있다. 구현(egui)·토큰 정합·갤러리는 로컬 claude code 가
맡는다. Figma 로 기획을 병행하려 했으나 Claude Design 으로 충분했다.

**갤러리의 누락.** `crates/tasty-gallery` 는 본체 UI 컴포넌트를 격리 렌더해 디자인 정합·토큰·시각을
검증하는 도구이고, demo=main 원칙([shared-widgets](../design/policies/shared-widgets.md))상 갤러리
specimen 은 본체와 같은 view-only 함수를 호출한다. 그런데 디자인 산출물의 갤러리 재구성이 본체에
실재하는 컴포넌트 다수(Convert · Port Scanner · File Handler Picker · Apply Preset · Toast Stack ·
Markdown Open · Update · Search Bar · Tools Menu · Divider · Multi-tier Tab · hint_text)를 카탈로그에서
의도적으로 뺐다. 갤러리가 컴포넌트를 빼면 그 컴포넌트는 디자인 정합을 검증할 단일 경로를 잃는다.

**도구 접근의 형태.** 한때 번들 plugin(`com.tasty.claude-design`)이 claude.ai/design 캔버스를
off-screen headful Playwright 브라우저로 자동화했다(`tasty design *` CLI 11 종, 세션 import, 동시성
lock 기반 chat 자동 전송). headful 브라우저 자동화·세션 관리·Cloudflare 우회는 본체의 관심사와
이질적이라 사용자 지시로 별도 프로젝트로 분리하고 본체에서 전면 제거했다. 그 plugin 의 세션
자격증명을 평문으로 저장한다는 결정도 대상과 함께 사라졌다.

**사이트 사본의 낡음.** 레포에는 같은 원격 Claude Design 프로젝트의 vendored 사본이 둘 있다 —
`crates/tasty-design-tokens/dtcg/tasty.tokens.json`(토큰, 절차와 census 판정기가 있다)과
`site/vendor/`(킷 전부, 절차도 판정기도 없었다). 뒤엣것이 낡았는데 사이트는 낡은 사본을 정상적으로
렌더하므로 빌드도 링크 검사도 CI 도 초록이었다. 공개 갤러리가 제거하기로 결정된 컨트롤을 현재형으로
전시하는 동안에도 그랬다. 두 사본의 토큰 이름 집합을 세면 앱 817 · 사이트 791 · 사이트에만 있는 것 0
이었다(2026-09-20 실측).

## Decision

**1. 흐름.** 디자인에 없는 UI 를 만들거나 디자인과 다르게 바꿀 때는 소스부터 고치지 않는다 —
요청문서(§0~§8) → Claude Design 시안 → **갤러리 specimen → 본체 → 사이트 사본** 순으로 정합하고, 부족·
불일치가 드러나면 새 요청문서로 다시 돈다. 넷을 다 반영해야 `reconciled` 다. 운영 절차는
[`dev-guide/design-change-workflow.md`](../dev-guide/design-change-workflow.md).

**2. 갤러리는 본체 UI 컴포넌트의 완전한 단일 출처다 — cut 금지.** 본체에 있는 modal/popup/공용 위젯/
레이아웃 idiom 은 빠짐없이 갤러리 specimen 으로 둔다. 디자인 산출물이 일부를 카탈로그에서 생략해도
그것을 근거로 갤러리에서 빼지 않는다 — 생략은 디자인 측 결함으로 보고 디자인 request 로 보강한다.
운영 상태는 [design/policies/gallery-completeness](../design/policies/gallery-completeness.md).

**3. 새 UI 컴포넌트는 gallery-first 로 들어온다.** 새 modal/popup/공용 위젯은 (a) 디자인을 먼저 받고
(b) 갤러리 specimen 에서 토큰·치수를 맞춘 뒤 (c) 본체에 반영한다. 절차는
[dev-guide/gallery-first](../dev-guide/gallery-first.md).

**4. 토큰의 정본은 코드(`Theme`)다.** 시안도 색·치수를 토큰 안에서만 고른다 — 요청문서가 토큰 팔레트를
입력으로 먼저 준다. HTML(flexbox)→egui 전사는 눈대중 흉내가 아니라
[구조 전사 원칙](../design/systems/design-parity-notes.md)으로 한다.

**5. 시안의 보존처.** 확정 시안은 원격 Claude Design 프로젝트에 파일로 남는다(요청문서만 정합 완료 뒤
지운다). 레포 안에 근거로 남겨야 하는 시안은 HTML 을 `docs/design/` 에 보존한다.

**6. 디자인 도구 접근은 plugin 이 아니라 세션 레벨 연결로 한다.** 원격 프로젝트 읽기·요청 인박스
쓰기는 Claude Code 세션의 DesignSync 연결이 하고, designer 에게 실제로 지시하는 것은 항상 사용자다.
tasty 본체에 디자인 도구 자동화 plugin 을 두지 않는다.

**7. 사이트 사본은 정합 대상이고, 절차 · 부분 판정기 · 화면 시점을 가진다.** 원격에서 실제로 받아오는
행위(재-vendoring)는 원격 접근 권한이 세션에 딸려 있어 이 결정의 범위 밖이다 — 권한 없는 세션은 그
단계를 **미완으로 보고**하고 조용히 건너뛰지 않는다.

- **절차** — `site/vendor/README.md` 의 "vendor 갱신 절차". 목록 회수로 구조 차분, 경로 단위 개별 수신,
  덮어쓰면 날아가는 로컬 변형 재적용.
- **판정기 (토큰)** — `crates/tasty-doc-guards/tests/site_vendor_tokens_track_the_app_export.rs` 가 두
  사본의 토큰 **이름 집합**을 대조한다. 차는 이름 명부(`LAGGING`)로 고정한 여유 0 의 양방향 래칫이다 —
  늘면 새 결정이 사이트를 건너뛴 것이고, 줄면 재-vendoring 이 일어난 것이니 명부에서 지운다. 사이트에만
  있는 이름은 0 이어야 한다.
- **판정기 (아이콘)** — `crates/tasty-doc-guards/tests/site_vendor_icons_match_the_app_transcription.rs` 가
  아이콘 **기하·채움과 그 그릇**(`viewBox` · `stroke-width` · `stroke-linecap` · `stroke-linejoin`)을 세
  사본(사이트의 `icons/*.svg` · `components/core/Icon.jsx` 의 `ICON_PATHS` · 앱 `crates/tasty-icons`)에서
  대조한다. 짝은 이름에서 도출하지 않고 명부로 적는다(이 킷의 `list` 가 앱의 `log` 다). 색은 갈리는 것이
  정상이라 사유와 함께 명부에 둔다.
- **화면 시점** — 사본에서 그리는 페이지 머리가 사본의 시점을 적는다. 빌드 시각에
  `git log -1 -- site/vendor`(README 제외)로 읽고, 날짜를 소스에 손으로 적지 않는다.

**판정기가 덮는 범위와 절차가 덮는 범위는 다르다.** 판정기는 토큰 이름과 아이콘(기하·채움·그릇) 두 층만
본다. 토큰을 안 여는 결정 — 문구 변경 · 구성 변경 · **컨트롤 삭제** — 은 양쪽 좌변을 똑같이 남겨 두므로
여전히 안 잡히고, 그 층은 사람이 도는 절차만 닫는다. 판정기의 초록은 "사이트 사본이 최신" 이 아니라
"차이가 기록된 그대로" 라는 뜻뿐이다. 이 경계 문장은 다섯 자리(판정기 모듈 주석 · 절차 문서 ·
워크플로 · 사이트 가이드 · 이 절)에 있고, 경계가 움직이면 다섯을 함께 고친다.

## Consequences

- **얻은 것**: 디자인 작업의 결정이 한 편에 있다. 갤러리 카탈로그 = 본체 컴포넌트 전수라 디자인이 cut
  해도 검증 사각이 안 생긴다. 본체가 system Playwright·Node·Chromium 런타임을 신경 쓰지 않는다. 토큰을
  여는 결정이 사이트를 건너뛰면 원격 접근 없이 그 자리에서 빨개진다.
- **잃은 것**: 디자인 카탈로그와 갤러리 항목 집합이 어긋날 수 있다(디자인이 cut 한 경우) — "디자인에
  다시 넣어 달라" 로만 해소한다. 디자인 도구 자동화가 본체에서 빠져 사용자가 Claude Design 을 직접 열어
  지시한다. 사이트 판정기가 덮는 층이 좁다. 흡수된 원본의 서술은 git 이력으로만 남는다.
- **운영 비용**: 본체 컴포넌트를 더할 때마다 갤러리 specimen 을 함께 유지한다. 재-vendoring 이 일어날
  때마다 `LAGGING` 명부에서 따라온 이름을 지운다(명부가 비면 판정을 집합 동등으로 바꾸고 명부를 지운다).

## Alternatives Considered

- **기획 전용 단계를 둔 3 단 흐름** — 시도했고, 시안 단계만으로 충분했다.
- **전부 코드 우선(시안 없이 구현부터)** — 탐색 비용이 비싸고(매번 빌드) 대안 비교가 어렵다.
- **디자인 카탈로그를 100% 추종(cut 수용) · 대표 컴포넌트만 노출** — 빠진 컴포넌트의 검증 경로가 사라지고,
  "대표" 의 기준이 임의적이다.
- **디자인 자동화 plugin 을 유지·개선하거나 안내만 하는 stub 을 남긴다** — 이질적인 운영 부담이 본체에
  남고, 죽은 CLI 표면은 혼란만 더한다. 0.x 정책상 deprecation 유예 없이 제거했다(CHANGELOG `(BREAK)`).
- **사이트 판정기를 `crates/tasty-design-tokens/tests/freshness.rs` 에 넣는다** — 그 크레이트가 자기 밖
  `site/` 를 읽게 되어 층이 뒤집힌다. 레포 전역 대조가 본업인 `tasty-doc-guards` 에 두고, 의존 0
  ([ADR-0138](0138-doc-guards-live-in-a-dependency-free-crate.md))이라 JSON·CSS 스캐너를 손으로 썼다.
- **사본 전체를 파일 해시로 대조** — 원격을 읽지 않고는 좌변이 안 선다.
- **시점을 소스에 적는다** — 또 하나의 낡을 사본이다. git 에서 읽는다.
- **갤러리 라우트를 발행에서 뺀다** — 랜딩의 제품 창 · 가이드의 UI 설명 · 디자인 섹션도 같은 사본을
  가져오므로 낡은 사본은 계속 공개되고, 시점이 붙은 자리만 사라진다. 그리고 완전한 단일 출처를 아무도
  못 보게 된다.
- **`site/vendor/` 를 손으로 고쳐 결정에 맞춘다** — 다음 재-vendoring 이 지우고, 무엇이 손 편집이었는지가
  안 남는다. "사이트에만 있는 이름 = 0" 조항이 이 방향을 막는다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

**채널이 붙는 것** — 판정 시점에 레포가 읽을 수 있는 사실이다.

- 사이트 판정기의 `LAGGING` 명부가 빈다 — 판정을 집합 동등으로 바꾸고 명부를 지운다.
- 사이트 판정기의 "사이트에만 있는 이름" 조항이 발동한다 — 사본이 정본을 흉내 내기 시작했다는 뜻이라
  "한 방향으로만 흐른다" 는 전제부터 다시 정한다.
- demo=main 구조가 폐기되어 갤러리가 더는 본체의 거울이 아니게 된다.

**원리적으로 안 붙는 것** — 사람이 관측해야 한다. 재는 법을 함께 적는다.

- 갤러리 specimen 유지 비용이 그 컴포넌트의 디자인 정합 가치를 명백히 넘는 컴포넌트 부류가 생긴다.
  재는 법: specimen 을 고친 커밋 중 본체 변경 없이 specimen 만 따라간 것의 비율을 본다.
- 시안과 토큰(`Theme`)의 정합이 반복해서 깨진다 — 요청문서의 입력 규약을 강화하거나 토큰 주입 도구를
  본다. 재는 법: 정합 단계에서 raw 값을 토큰으로 바꾼 수정이 몇 번 나왔는지 센다.
- HTML→egui 전사 갭이 구현 비용으로 누적된다 — 시안 출력 형식을 egui 친화 구조로 다시 정한다. 재는 법:
  정합 커밋에서 레이아웃 재작성 비중을 본다.
- 토큰을 안 여는 결정이 다시 사이트에만 안 반영된 채 공개된다 — 원격을 읽는 채널(파일 단위 대조)을 세울
  값이 있는지 본다. 재는 법: 원격 결정 하나가 바꾼 킷 파일을 DesignSync 로 받아 `site/vendor/` 의 같은
  경로와 diff 한다.
- 재-vendoring 을 사람 손으로 도는 비용이 그 가치를 넘는다. 재는 법: 한 회차의 소요 시간과 실제로 바뀐
  파일 수를 함께 적는다.
- 디자인 자동화가 별도 프로젝트에서 안정화되어 본체로 재통합할 명분이 생기거나, 자동 지시 수요가
  반복된다. 재는 법: 사용자 요청 기록.

## References

- 흡수: ADR-0018 · ADR-0020 · ADR-0025 · ADR-0027 · ADR-0057 · ADR-0294 (파일 삭제, 번호 결번)
- 운영 절차: [`dev-guide/design-change-workflow.md`](../dev-guide/design-change-workflow.md) · [`dev-guide/gallery-first.md`](../dev-guide/gallery-first.md) · [`site/vendor/README.md`](../../site/vendor/README.md) · [`dev-guide/site.md`](../dev-guide/site.md)
- 운영 상태: [design/policies/gallery-completeness](../design/policies/gallery-completeness.md) · [design/systems/design-gallery-mapping](../design/systems/design-gallery-mapping.md) · [design/systems/design-parity-notes](../design/systems/design-parity-notes.md)
- 코드 근거(결정이 실현된 현재 위치): `crates/tasty-doc-guards/tests/site_vendor_tokens_track_the_app_export.rs` 의 `LAGGING` · `crates/tasty-doc-guards/tests/site_vendor_icons_match_the_app_transcription.rs` · `site/scripts/vendor-to-esm.mjs` 의 `vendorStamp` · `src/source_guards/gallery_specimen_parity.rs` · `src/source_guards/gallery_widget_coverage.rs`
- 선행 결정 없음 — 탐색: `git grep -liE 'claude.?design|gallery-first|site/vendor' -- docs/adr/` 가 흡수 대상 여섯과 그것을 인용하는 ADR 만 낸다
