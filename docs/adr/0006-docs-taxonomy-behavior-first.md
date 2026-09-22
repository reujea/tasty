# ADR-0006: 문서 분류체계 — 동작 우선(behavior-first), 화면 종속

- **Status**: Accepted
- **Date**: 2026-06-16
- **Tags**: docs, taxonomy, headless, screen-spec, design-system, behavior-first
- **Group**: docs-adr

## Context

tasty 의 화면/기능 명세 문서가 `docs/features/` 단일 층에 임의 구조로 쌓여 왔고(옛 단일 `features.md` 를 H2 단위로 분할한 반쯤 끝난 마이그레이션 상태), 화면 정의와 내부 동작 정의가 한 문서에 섞여 있었다. 이를 정리하려면 "무엇이 무엇의 상위인가" 를 먼저 정해야 한다.

세 가지 제약이 방향을 강제한다:

- **tasty 는 headless 로 동작한다.** 한 기능의 진실은 *내부 동작* 이고, *화면* 은 그 동작을 사람 사용자에게 투영한 결과일 뿐이다.
- **사용자/에이전트 행동 분리** 가 정체성이다. 에이전트 행동(CLI/IPC)은 headless 표면, 사용자 행동(키/마우스)은 화면 표면 — 동작 축과 화면 축이 그대로 대응한다.
- **시각의 진실은 외부 산출물이다.** 디자인은 claude design 이 만든 `design-system/` vendor 가 소유하며 claude code 는 직접 수정하지 않는다. docs 가 픽셀/토큰을 재서술하면 진실이 둘로 갈라진다.

## Decision

문서를 **동작 우선** 으로 분류한다.

- **기획문서(내부 동작, headless-valid)를 1순위 부모**, **화면정의서를 그 자식(2순위)** 으로 둔다. 폴더 중첩: `docs/features/<f>/index.md` + `docs/features/<f>/screens/<s>.md`. 기획 1 : 화면 0..N.
- **시각 진실은 `design-system/` vendor 가 소유**하고 docs 는 링크만 한다. 화면정의서는 요소 인벤토리와 "동작 상태 → 시각" 매핑만 적는다.
- **합성 화면은 다른 문서를 언급(링크)으로만** 잇는다 (임베드/복제 금지).
- **design ↔ code 는 claude design 협업으로 연계한다**: 디자인 변경은 소스를 먼저 고치지 않고 claude design 에 요청해 변경된 디자인을 받아 재적용하며, 그 변경 내용을 docs 가 조각별로 라우팅해 흡수한다. 요청·회수의 구체 워크플로는 [`docs/dev-guide/design-change-workflow.md`](../dev-guide/design-change-workflow.md) 에 둔다.

운영 규칙 상세는 [`docs/documentation-model.md`](../documentation-model.md).

## Consequences

- **얻은 것**: headless 정체성과 문서 구조가 정합한다. 시각이 단일 진실(design vendor)로 모인다. 합성 화면의 중복이 사라진다. 다음 세션의 에이전트가 중앙 규칙 문서 하나로 "이 내용이 어디 가야 하나" 를 판단할 수 있다.
- **잃은 것**: 폴더 중첩 때문에 단순 1:1 기능도 폴더 + 2파일이 된다. 기존 33개 feature 문서 마이그레이션 비용이 든다.
- **운영 비용 / 유지 부담**: 디자인 sync 때마다 changelog 흡수 라우팅을 사람이 수행해야 한다. "어디까지 반영했나" watermark 는 별도 장부 없이 커밋 규약(`chore(design): sync @date`, 구현 커밋의 `Design:` 줄)에 의존한다.

## Alternatives Considered

- **화면 우선(screen-first)**: UI 를 진실로, 동작을 부속으로. — headless 에서 화면이 없으므로 진실이 사라진다. 기각.
- **단일 허브(기획=화면 한 문서)**: feature 당 한 문서에 동작+화면 통합. — 1:N 투영과 합성 화면에서 비대해지고 두 층이 섞인다. 기각.
- **design-system 을 docs 로 흡수·복제**: 시각을 docs 가 직접 보유. — 두 진실 drift. 기각(링크만 유지).
- **changelog 영구 보관**: tasty 레포에 외부 산출물 로그를 누적. — 외부 소유 산출물이 본 레포 히스토리를 오염. 기각(휘발 + docs 흡수).

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

- 디자인을 claude design 외부 산출물이 아니라 레포 내부에서 직접 편집하게 되면 시각 소유 규칙을 재검토한다.
- `features/` 폴더 수가 관리 불가 수준으로 늘어 flat 모델이 더 단순해지면.

## References

- [`docs/documentation-model.md`](../documentation-model.md) — 운영 규칙 본체
- [`docs/dev-guide/design-change-workflow.md`](../dev-guide/design-change-workflow.md) — claude design 협업(디자인 변경) 워크플로
- ADR-0003 (CSD), ADR-0005 (memory secret) — 동일한 trust/소유 경계 사고의 연장
