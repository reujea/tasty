# 디자인 변경 워크플로 — 요청문서 → 시안 → 정합 루프

디자인에 **없는** UI 요소를 새로 만들거나, 디자인과 **다른** 형태로 바꿔야 할 때 따르는 절차다.
"소스부터 고치지 않는다" — 먼저 디자인을 *확보*하고, 받은 디자인을 갤러리·본체에 반영한다.

> 디자인에 *이미 있는데* 소스만 못 따라간 경우(구현 누락/불일치)는 이 워크플로가 필요 없다 —
> 디자인 변경이 아니므로 바로 [gallery-first](gallery-first.md) 의 구조 전사(1단계)로 간다.

디자인 원본은 Design System Artifact `Tasty Design System`이고 파일은 `project/` 아래에 있다.
Artifact 주소와 `designer` Agent 정의는 커밋하지 않는 로컬 전용 지침에 있다.
정합 순서와 그 근거·재검토 조건은 [테마 시스템 (운영 상세)](../design/systems/theme.md) 에 있다.

<a id="뱅글뱅글-도는-루프"></a>

## 요청부터 반영까지

```
   ┌───────────────── 재요청 (부족·불일치 발견 시) ─────────────────┐
   ▼                                                               │
[1] 구현 세션 ──작성·게시──▶ 요청문서 ──[2] 호출──▶ [3] designer Agent  │
                         project/design-request/<slug>.md    (Artifact를 고침)
                                                                │  │
                                                 결정표·변경 목록 ▼  │
                 [4] 정합: 갤러리 specimen → 본체 구현 ─┘
                                                                │
                                              [5] 원격 요청·완료 문서·changelog 정리
```

| 단계 | 행위자 | 하는 일 | 산출물 |
|------|--------|---------|--------|
| 1 | 구현 세션 | 무엇을 어떻게 보이게 할지 정의한 요청문서를 Artifact에 게시 | `project/design-request/<slug>.md` |
| 2 | 구현 세션 | `designer` Agent를 Artifact 주소와 요청문서 경로로 호출. **사용자 허락을 따로 받지 않는다** | |
| 3 | `designer` Agent | 디자인을 결정하고 범위 안의 파일을 게시 | 결정표, 게시 경로, `project/design-tasks/*.DONE.md`, changelog |
| 4 | 구현 세션 | 보고를 게시본과 대조한 뒤 갤러리 specimen → 본체 순서로 반영. 사이트 사본은 릴리스할 때 갱신한다 | 코드 |
| 5 | 구현 세션 | 반영이 끝난 요청문서·완료 문서·changelog 토픽 삭제 | |

시안이 요청문서에 없던 구조를 드러내거나 4단계에서 부족분이 나오면 다음 요청문서로 다시 돈다.

## 책임 경계

| 대상 | 정하는 쪽 | 예 |
|------|----------|-----|
| 제품 동작 | 사용자. 요청문서에 확정 사항으로 적는다 | 언제 풀리는지, 기능을 넣을지, 어떤 동작을 제공할지 |
| 동작의 표현 | designer | 레이아웃, 상태별 모양, 문구, 비활성 표시 방식 |
| 정해진 동작 안의 흐름 표현 | designer. 동작이 달라지면 멈춘다 | 버튼 위치, 안내 순서, 포커스 표시 |
| 기술 사실 | 구현 세션 | 키 이름, 현재 구현 동작, 토큰 사용 범위 |

구현 세션은 디자인 값을 스스로 정하지 않는다. designer는 저장소 소스를 읽지 않으므로 요청문서에 필요한 구현 사실을 실제 코드에서 확인해 적는다.

### `.NEEDS-INPUT` 처리

designer가 답을 정할 수 없는 항목은 `project/design-tasks/<YYYYMMDD-hhmm>-<slug>.NEEDS-INPUT.md`로 게시하고 질문마다 종류를 표시한다. 호출한 구현 세션이 먼저 처리한다.

| 종류 | 처리 |
|------|------|
| 사실 확인 | 구현 세션이 코드에서 확인해 답하고 designer를 다시 호출한다 |
| 디자인 판단 | 요청문서를 보강해 designer를 다시 호출한다 |
| 제품 결정 | 사용자에게 묻는다. 분류가 애매하면 제품 결정으로 본다 |

## 디자인 요청문서란

구현 세션이 작성하는 입력 산출물이다. *왜/구현 내부 구조*가 아니라 **무엇을 어떻게 보이게 할지**와 designer가 알 수 없는 사실을 담는다.

- **위치**: `project/design-request/<slug>.md`. 일부 값이 비어 있어도 게시하며 여러 요청을 동시에 둘 수 있다.
- **기준 구성**: 아래 §0~§8 구성을 그대로 따른다.

### 요청문서 구성 (§0~§8)

| 섹션 | 내용 |
|------|------|
| 헤더 메타 | 작성일 · 요청자 · 수행자(designer) · **상태** · 범위 |
| §0 한 줄 요약 | 이 요청이 만들려는 것 한 줄 |
| §1 맥락 / 제약 | 확인한 구현 사실(확인 날짜 포함) · 토큰 규칙 · 기존 구현과의 관계 |
| §2 인벤토리 | 다룰 화면/컴포넌트/상태 목록 표 (신규/기존 구분) |
| §3 화면별 요구사항 | 화면 단위 레이아웃·영역 구성 |
| §4 컴포넌트별 요구사항 | 컴포넌트 단위 상태·변형 |
| §5 인터랙션 / 동작 | 사용자가 확정한 동작과 상태 전이 규칙 |
| §6 열린 결정 | designer가 판단할 항목 |
| §7 산출물 형식 | 결정표, 토큰 변경표, 영향 목록처럼 보고받을 것 |
| §8 참고 | 표본 경로, 토큰 안내 |

### 요청문서가 강제하는 디자인 규칙 (시안 단계부터)

- **토큰 기준** — 색·간격·치수·보더는 토큰 안에서만 정한다. 토큰은 3계층이고 추가만 한다. ([theme UI 규칙](../design/systems/theme.md#ui-디자인-규칙-필수))
- **레이아웃 규칙** — 4px 그리드 · 폰트 14px 상한 · 보더 1px · 호버/액티브 오버레이는 자동 도출한다.
- **대비** — 텍스트 4.5:1, 테두리·포커스·아이콘 3:1을 Mocha와 Latte 양쪽에서 지킨다.
- **gallery-first 부품 재사용** — 보편 부품은 [공용 위젯](../architecture/ui-widgets-crate.md#무엇을-공용-위젯으로)·기존 카탈로그와 시각 일관을 유지한다. ([gallery-first](gallery-first.md))
- **i18n 가변폭** — 문구는 번역 키마다 en, ko, ja를 모두 정하고 길이 차이를 감안한다. ([`t()`](i18n.md))

## 토큰이 바뀌는 요청

앱은 `project/tokens/tasty.tokens.json` 한 파일만 받아 Rust 상수와 Theme 접근자를 생성하므로, CSS만 고치면 앱에 들어오지 않는다.

1. designer는 토큰 CSS와 같은 게시에서 JSON을 재생성한다. 도구는 `scripts/design-tokens/gen_tokens_json.py`와 `verify_tokens_json.py`이고, 값을 손으로 옮겨 적지 않는다. 절차는 [`crates/tasty-design-tokens/README.md`](../../crates/tasty-design-tokens/README.md)에 있다.
2. Latte만 값이 달라지는 새 semantic 역할(`-raised` 등)은 단순 별칭이 아니므로 구현 세션이 `crates/tasty-design-tokens/src/dtcg/accessor.rs`의 `SEMANTIC_COLOR_TO_THEME_ACCESSOR`와 `SEMANTIC_COLOR_HAND_WRITTEN`에 Theme 접근자를 함께 추가한다. 대응이 없으면 생성기가 그 토큰을 쓰는 component 접근자를 건너뛴다.
3. `components/bundle.js`는 Artifact 안에서 재빌드할 도구가 없다. 갤러리 페이지는 `.jsx` 소스를 번들 뒤에 불러오므로 구현 기준은 `.jsx`다.

<a id="정합-대상--사이트-사본-필수"></a>

## 정합 대상 — 사이트 사본 (필수)

레포에는 같은 원격 원본의 vendored 사본이 **둘** 있다. 앱 사본은 디자인을 반영할 때 갱신하고, 사이트 사본은 **릴리스할 때** 갱신한다. 사이트는 사용자가 받을 수 있는 버전을 보여 주므로 릴리스 사이에는 앱 사본보다 뒤처지는 것이 정상이다. 절차는 [릴리스 절차](release.md#3-사이트-갱신)에 있다.

| 사본 | 무엇이 받는가 | 갱신 절차 | 안 따라오면 |
|------|--------------|-----------|-------------|
| 앱 — `crates/tasty-design-tokens/dtcg/tasty.tokens.json` | 토큰(DTCG) | [`crates/tasty-design-tokens/README.md`](../../crates/tasty-design-tokens/README.md) "vendor 갱신 절차" | `crates/tasty-design-tokens/tests/freshness.rs` 의 목록 검사에 실패한다 |
| 사이트 — `site/vendor/` | 킷 전부(토큰 · 컴포넌트 · UI kit · 갤러리 · 가이드라인) | [`site/vendor/README.md`](../../site/vendor/README.md) "vendor 갱신 절차" | **릴리스 후에도 공개 사이트가 결정 이전 UI 를 현재형으로 전시한다** |

갤러리 파일은 ES 모듈이다. 표본은 `gallery/<페이지>/` 아래에 두고 필요한 것을 `import`하며 전역 객체와 페이지별 `.html`을 쓰지 않는다. 표본의 제목과 순서, 상태 표시, 표 형식은 `gallery/README.md`를 따르고, 새 계약의 칸은 해당 페이지의 `states.jsx`와 `parts/contract-cells.jsx`에 넣는다. 게시 전에 사본을 `site/vendor/`에 맞춰 `cd site && npm run build`로 확인한다.

사이트는 오래된 사본도 정상 빌드할 수 있으므로 빌드 통과만으로 최신 여부를 판단하지 않는다.
`site_vendor_tokens_track_the_app_export`는 두 사본의 토큰 이름을,
`site_vendor_icons_match_the_app_transcription`은 아이콘 기하·채움·viewBox·선 굵기·
cap/join을 비교한다. `site_vendor_manifest_matches`는 사이트 사본이 `site/vendor/MANIFEST`에
기록한 원격 해시(또는 이유를 적은 변형 해시)와 같은지 확인한다. 세 검사는 `crates/tasty-doc-guards/tests/`에 있다. 앱과 사이트를 비교하는 앞의 두 검사의 해당 시험은 `#[ignore]`이며 릴리스 절차에서 `--ignored`로 실행한다.
문구·구성·컨트롤 삭제처럼 그 값들을 바꾸지 않는 변경은 검사하지 않으므로 원격 디자인과
직접 대조해 사이트 사본도 갱신한다.

이 정합 대상을 세운 근거·대안·재검토 조건은 [테마 시스템 (운영 상세)](../design/systems/theme.md).

재-vendoring 은 Artifact 읽기 권한이 필요하다. 그 권한이 없는 세션에서는 이 단계를 **미완으로 남기고 그 사실을 적는다** — 조용히 건너뛰지 않는다.

## 상태 라이프사이클

요청문서 헤더 메타의 `상태:` 필드로 추적한다.

| 상태 | 의미 |
|------|------|
| `requested` | 요청문서를 게시함. designer 호출 전이거나 실행 중. |
| `received` | designer가 게시를 마치고 보고함. 정합 작업 대기/진행. |
| `reconciled` | 시안을 갤러리 specimen + 본체 + **사이트 사본(`site/vendor/`)** 에 반영 완료. 이 시점에 정리한다(아래 [정합 완료 후 정리](#정합-완료-후-정리-필수)). |
| `re-requested` | 정합 중 부족·불일치가 드러나 추가/갱신 요청문서로 루프 재진입. |

<a id="정합-완료-후-요청문서-삭제-필수"></a>

## 정합 완료 후 정리 (필수)

`reconciled`에 도달하면 요청문서, designer가 남긴 완료 문서(`project/design-tasks/*.DONE.md`), changelog 토픽 파일(`project/changelog/<날짜>-<토픽>.md`)을 삭제한다. `project/changelog/README.md`는 남기고 목차 표의 해당 행만 지운다. 이력이 필요한 결정은 이미 ADR·docs 에 남는다.

- **삭제 조건**: 상태가 확정적으로 `reconciled`이고, 삭제할 파일을 게시본 목록에서 확인했다. `re-requested`로 루프가 도는 중이면 지우지 않는다.
- **삭제 방법**: 한 게시 호출의 `files`에 `"project/<경로>": null`을 넣는다. 같은 호출에서 `project/design-system.json`의 `lastChange`를 갱신한다.
- **남기는 것**: 완료 문서의 요청 중 반영되지 않은 것이 있으면 그 완료 문서와 changelog 토픽은 남긴다. `.FAILED`·`.NEEDS-INPUT` 문서는 처리가 끝나기 전까지 지우지 않는다. 토큰·갤러리·ui_kits 같은 확정 디자인은 삭제하지 않는다.

## Artifact 게시 규칙

- 게시가 낡은 판이라고 거부되면 `url`만 넘겨 다시 읽는다. 파일 경로 읽기로는 풀리지 않는다. `force`는 쓰지 않는다.
- 게시할 파일은 한 폴더 아래 `project/<경로>`로 두고 `root`, `file_path`, `files`를 함께 넘긴다. `.jsx`는 `files`에 `{from, contentType: "text/plain"}`으로 넣는다.
- 인덱스 `project/design-system.json`은 작업의 마지막 호출에서 직전에 다시 읽어 `lastChange`만 고친다.

## 누가 무엇을 하나 (역할 분리)

| 역할 | 누가 | 책임 |
|------|------|------|
| **사용자** | 사람 | 제품 동작 결정. 디자인 작업 시작을 따로 승인하지 않는다. |
| **구현 세션** | Claude Code 메인 세션 | 요청문서 작성·게시, designer 호출, 보고 대조, [gallery-first](gallery-first.md) 순서의 반영, 원격 정리. |
| **designer** | `designer` Agent | 디자인 결정과 Artifact 수정. 지정된 Artifact 외에는 쓰지 않는다. |

> 레포 안에 근거로 남겨야 하는 시안은 HTML 을 `docs/design/` 에 보존한다(docs/design/systems/theme.md).

## 관련

- [테마 시스템 (운영 상세)](../design/systems/theme.md) — 정합 순서·갤러리 완전성·사이트 사본 결정 근거와 재검토 조건.
- [gallery-first](gallery-first.md) — 확보한 디자인을 갤러리→본체로 내리는 순서(0단계 "디자인 확보"가 이 워크플로).
- [design/policies/gallery-completeness](../design/policies/gallery-completeness.md) — cut 금지 시 디자인 보강(=이 워크플로로 재요청).
- [design/systems/design-parity-notes](../design/systems/design-parity-notes.md) · [design-gallery-mapping](../design/systems/design-gallery-mapping.md) — 시안→소스 구조 전사 원칙·매핑.
- [theme.md "UI 디자인 규칙"](../design/systems/theme.md#ui-디자인-규칙-필수) — 토큰 정합 규칙(시안부터 적용).
