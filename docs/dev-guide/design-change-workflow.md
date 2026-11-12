# 디자인 변경 워크플로 — 요청문서 → 시안 → 정합 루프

디자인에 **없는** UI 요소를 새로 만들거나, 디자인과 **다른** 형태로 바꿔야 할 때 따르는 절차다.
"소스부터 고치지 않는다" — 먼저 디자인을 *확보*하고, 받은 디자인을 갤러리·본체에 반영한다.

> 디자인에 *이미 있는데* 소스만 못 따라간 경우(구현 누락/불일치)는 이 워크플로가 필요 없다 —
> 디자인 변경이 아니므로 바로 [gallery-first](gallery-first.md) 의 구조 전사(1단계)로 간다.

디자인 정본은 공개 레포 [tasty-page](https://github.com/zilhak/tasty-page)의 `develop`에 있는 `vendor/`다.
그 안의 갤러리(tasty-page `vendor/gallery/`)가 UI 디자인의 기준이며 이 저장소에는 디자인 사본을 두지 않는다.
디자인은 구현 세션이 정하지 않는다. 디자인을 맡은 별도의 Claude 세션(디자이너 세션)이 tasty-page worktree에서
`vendor/`를 고쳐 커밋한다. 정합 순서와 그 근거·재검토 조건은 [테마 시스템 (운영 상세)](../design/systems/theme.md) 에 있다.

<a id="뱅글뱅글-도는-루프"></a>

## 요청부터 반영까지

```
   ┌──────────────── 재요청 (부족·불일치 발견 시) ────────────────┐
   ▼                                                            │
[1] 구현 세션 ──요청문서(§0~§8)──▶ [2] 디자이너 세션                 │
                                   tasty-page vendor/ 수정·검사·커밋 │
                                        │ 결정표·커밋 해시           │
                                        ▼                           │
              [3] 구현 세션: 커밋 대조 → tasty 갤러리 → 본체 ────────┘
```

| 단계 | 행위자 | 하는 일 | 산출물 |
|------|--------|---------|--------|
| 1 | 구현 세션 | 무엇을 어떻게 보이게 할지 정의한 요청문서를 작성해 디자이너 세션에 넘긴다. **사용자 허락을 따로 받지 않는다** | 요청문서 |
| 2 | 디자이너 세션 | 디자인을 결정하고 tasty-page `vendor/`(갤러리 포함)를 고친다. 그 레포의 디자인 검사를 통과시킨 뒤 `develop`에 커밋하고 보고한다 | tasty-page 커밋, 결정표 |
| 3 | 구현 세션 | 보고를 tasty-page 커밋의 실제 변경과 대조한 뒤 이 저장소의 갤러리(`crates/tasty-gallery`)에 같은 형상을 구현하고 본체에 반영한다 | 코드, `dtcg/SOURCE` 갱신 |

갤러리와 본체는 같은 컴포넌트 소스를 쓰므로 모양만 바꾸는 변경은 3단계의 갤러리 구현에서 본체에도 반영된다.
시안이 요청문서에 없던 구조를 드러내거나 3단계에서 부족분이 나오면 다음 요청문서로 다시 돈다.
디자인 변경의 기록은 tasty-page 커밋 본문이 맡는다. 바꾼 것, 근거, 구현 쪽이 알아야 할 것을 거기에 적는다.

디자이너 세션이 통과시키는 tasty-page 검사는 `npm run build`, `npm run check-contrast`, `npm run check-icons`,
`npm run check-tokens-copy`와 토큰 도구 `tasty-page/scripts/design-tokens/verify_tokens_json.py`다.
같은 검사를 그 레포의 CI(`design.yml`)가 push마다 실행한다.

## 책임 경계

| 대상 | 정하는 쪽 | 예 |
|------|----------|-----|
| 제품 동작 | 사용자. 요청문서에 확정 사항으로 적는다 | 언제 풀리는지, 기능을 넣을지, 어떤 동작을 제공할지 |
| 동작의 표현 | 디자이너 세션 | 레이아웃, 상태별 모양, 문구, 비활성 표시 방식 |
| 정해진 동작 안의 흐름 표현 | 디자이너 세션. 동작이 달라지면 멈춘다 | 버튼 위치, 안내 순서, 포커스 표시 |
| 기술 사실 | 구현 세션 | 키 이름, 현재 구현 동작, 토큰 사용 범위 |

구현 세션은 디자인 값을 스스로 정하지 않는다. 디자이너 세션은 이 저장소의 소스를 읽지 않으므로 요청문서에 필요한 구현 사실을 실제 코드에서 확인해 적는다.

디자이너 세션이 정할 수 없는 항목은 보고에 질문으로 남긴다. 사실 확인은 구현 세션이 코드에서 확인해 답하고, 디자인 판단은 요청문서를 보강해 다시 요청한다. 제품 결정은 사용자에게 묻는다. 분류가 애매하면 제품 결정으로 본다.

## 디자인 요청문서란

구현 세션이 작성하는 입력 산출물이다. *왜/구현 내부 구조*가 아니라 **무엇을 어떻게 보이게 할지**와 디자이너 세션이 알 수 없는 사실을 담는다.

- **전달**: 요청문서를 두는 위치와 디자이너 세션을 띄우는 방법은 커밋하지 않는 로컬 지침에 있다. 일부 값이 비어 있어도 넘기며 여러 요청을 동시에 둘 수 있다.
- **기준 구성**: 아래 §0~§8 구성을 그대로 따른다.

### 요청문서 구성 (§0~§8)

| 섹션 | 내용 |
|------|------|
| 헤더 메타 | 작성일 · 요청자 · 수행자(디자이너 세션) · 범위 |
| §0 한 줄 요약 | 이 요청이 만들려는 것 한 줄 |
| §1 맥락 / 제약 | 확인한 구현 사실(확인 날짜 포함) · 토큰 규칙 · 기존 구현과의 관계 |
| §2 인벤토리 | 다룰 화면/컴포넌트/상태 목록 표 (신규/기존 구분) |
| §3 화면별 요구사항 | 화면 단위 레이아웃·영역 구성 |
| §4 컴포넌트별 요구사항 | 컴포넌트 단위 상태·변형 |
| §5 인터랙션 / 동작 | 사용자가 확정한 동작과 상태 전이 규칙 |
| §6 열린 결정 | 디자이너 세션이 판단할 항목 |
| §7 산출물 형식 | 결정표, 토큰 변경표, 영향 목록처럼 보고받을 것 |
| §8 참고 | 표본 경로, 토큰 안내 |

위치(어느 탭·섹션·메뉴에 둘지)를 디자이너 세션에게 맡기는 요청이면 그 위치로 가는 탐색 경로의 사실(예: 설정 섹션 필터가 L2 라벨만 일치한다)을 코드에서 확인해 요청문서에 적는다.

### 요청문서가 강제하는 디자인 규칙 (시안 단계부터)

- **토큰 기준** — 색·간격·치수·보더는 토큰 안에서만 정한다. 토큰은 3계층이고 추가만 한다. ([theme UI 규칙](../design/systems/theme.md#ui-디자인-규칙-필수))
- **레이아웃 규칙** — 4px 그리드 · 폰트 14px 상한 · 보더 1px · 호버/액티브 오버레이는 자동 도출한다.
- **대비** — 텍스트 4.5:1, 테두리·포커스·아이콘 3:1을 Mocha와 Latte 양쪽에서 지킨다.
- **gallery-first 부품 재사용** — 보편 부품은 [공용 위젯](../architecture/ui-widgets-crate.md#무엇을-공용-위젯으로)·기존 카탈로그와 시각 일관을 유지한다. ([gallery-first](gallery-first.md))
- **i18n 가변폭** — 문구는 번역 키마다 en, ko, ja, zh-Hans를 모두 정하고 길이 차이를 감안한다. ([`t()`](i18n.md))

## 토큰이 바뀌는 요청

앱은 tasty-page `vendor/tokens/tasty.tokens.json` 한 파일만 받아 Rust 상수와 Theme 접근자를 생성하므로, CSS만 고치면 앱에 들어오지 않는다.

1. 디자이너 세션은 토큰 CSS와 같은 커밋에서 JSON을 `tasty-page/scripts/design-tokens/gen_tokens_json.py`로 재생성하고 `verify_tokens_json.py`로 대조한다. 값을 손으로 옮겨 적지 않는다.
2. 구현 세션은 [`crates/tasty-design-tokens/README.md`](../../crates/tasty-design-tokens/README.md)의 "토큰 갱신 절차"로 JSON과 `dtcg/SOURCE`를 함께 바꾼다.
3. Latte만 값이 달라지는 새 semantic 역할(`-raised` 등)은 단순 별칭이 아니므로 구현 세션이 `crates/tasty-design-tokens/src/dtcg/accessor.rs`의 `SEMANTIC_COLOR_TO_THEME_ACCESSOR`와 `SEMANTIC_COLOR_HAND_WRITTEN`에 Theme 접근자를 함께 추가한다. 대응이 없으면 생성기가 그 토큰을 쓰는 component 접근자를 건너뛴다.

## 앱이 따르는 디자인 판 (필수)

앱이 반영한 디자인 판은 `crates/tasty-design-tokens/dtcg/SOURCE`에 tasty-page 커밋 해시 한 줄로 기록한다.
디자인 커밋을 갤러리·본체에 반영하면 같은 커밋에서 SOURCE를 그 tasty-page 커밋으로 올린다.
tasty-page `develop`이 앞서 있는 것은 정상이다. 아직 반영하지 않은 디자인이 있다는 뜻이다.

| 대상 | 검사 | 안 따라오면 |
|------|------|-------------|
| 토큰 — `crates/tasty-design-tokens/dtcg/tasty.tokens.json` | `design_source_tokens_track_the_app_export`가 SOURCE 커밋의 JSON과 바이트를, 정본 CSS와 토큰 이름을 비교한다. `crates/tasty-design-tokens/tests/freshness.rs`가 생성물과 JSON을 비교한다 | 문서 검사가 실패한다 |
| 아이콘 — `crates/tasty-icons` | `design_source_icons_match_the_app_transcription`이 SOURCE 커밋의 아이콘 기하·채움·viewBox·선 굵기·cap/join을 비교한다. 앱 비교 기준은 `vendor/components/core/Icon.jsx`이고 SVG 파일과 Icon.jsx의 일치는 tasty-page `npm run check-icons`가 맡는다 | 문서 검사가 실패한다 |

두 `design_source_*` 검사는 `crates/tasty-doc-guards/tests/`에 있고, SOURCE 커밋으로 체크아웃한 tasty-page 경로를
`TASTY_PAGE_DIR`로 받는다. 경로가 없거나 체크아웃의 HEAD가 SOURCE와 다르면 실패한다.
`doc-guards.yml`과 pre-push 훅은 `scripts/fetch-design-source.sh`로 그 커밋을 받아 넘긴다.
문구·구성·컨트롤 삭제처럼 그 값들을 바꾸지 않는 변경은 검사하지 않으므로 SOURCE 커밋의 갤러리와 직접 대조한다.

이 정합 대상을 세운 근거·대안·재검토 조건은 [테마 시스템 (운영 상세)](../design/systems/theme.md).

## 누가 무엇을 하나 (역할 분리)

| 역할 | 누가 | 책임 |
|------|------|------|
| **사용자** | 사람 | 제품 동작 결정. 디자인 작업 시작을 따로 승인하지 않는다. |
| **구현 세션** | Claude Code 메인 세션 | 요청문서 작성, 디자이너 세션 호출, 보고와 tasty-page 커밋 대조, [gallery-first](gallery-first.md) 순서의 반영, `dtcg/SOURCE` 갱신. |
| **디자이너 세션** | 디자인을 맡은 별도 Claude 세션 | 디자인 결정과 tasty-page `vendor/` 수정·검사·커밋. 이 저장소에는 쓰지 않는다. |

> 레포 안에 근거로 남겨야 하는 시안은 HTML 을 `docs/design/` 에 보존한다(docs/design/systems/theme.md).

## 관련

- [테마 시스템 (운영 상세)](../design/systems/theme.md) — 정합 순서·갤러리 완전성 결정 근거와 재검토 조건.
- [gallery-first](gallery-first.md) — 확보한 디자인을 갤러리→본체로 내리는 순서(0단계 "디자인 확보"가 이 워크플로).
- [design/policies/gallery-completeness](../design/policies/gallery-completeness.md) — cut 금지 시 디자인 보강(=이 워크플로로 재요청).
- [design/systems/design-parity-notes](../design/systems/design-parity-notes.md) · [design-gallery-mapping](../design/systems/design-gallery-mapping.md) — 시안→소스 구조 전사 원칙·매핑.
- [theme.md "UI 디자인 규칙"](../design/systems/theme.md#ui-디자인-규칙-필수) — 토큰 정합 규칙(시안부터 적용).
