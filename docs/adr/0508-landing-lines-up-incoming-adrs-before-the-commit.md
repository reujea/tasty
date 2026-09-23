# ADR-0508: 착지는 커밋 전에 들어오는 ADR 을 나란히 놓는다 — 도구는 보고만 하고 판정은 사람이 한다

- **Status**: Accepted
- **Date**: 2026-09-23
- **Tags**: adr-conventions, adr-duplication, landing, parallel-lanes, adr-0506, adr-0507, adr-0243, adr-0239
- **Group**: docs-adr

## Context

[ADR-0507](0507-an-adr-is-written-after-a-symbol-search-and-only-for-a-choice.md) 이 작성 시점의
탐색을 정했다. 그 탐색의 모수는 **작성하는 트리에 이미 있는 ADR** 이다. 두 lane 이 같은 base 에서
동시에 갈라져 서로의 ADR 을 트리에 갖지 않은 채 같은 조항을 쓰면, 작성 시점 탐색은 원리적으로
상대를 못 만난다. 두 문서가 처음 한 트리에 놓이는 순간이 착지다.

이 형태가 실제로 났는지는 확인하지 못했다. 실측 중복(0128·0145·0169)은 작성 시점 탐색으로
잡혔을 사례다 — 0169 를 쓰던 트리에 0145 가 이미 있었다. 그래서 착지 대조는 **구조적 가능성에
대한 보조 층**이다.

착지 절차가 ADR 에 대해 보는 것은 번호뿐이었다. 레포 가드
(`crates/tasty-doc-guards/tests/adr_index_parity.rs` 의 `an_adr_number_names_exactly_one_document`)도
번호만 본다. 내용이 겹치는 ADR 은 서로 다른 파일로 들어와 git 도 충돌로 보지 않고, lane 소스
교집합으로 재완주 생략을 판정하는 병합에서도 교집합 0 으로 통과한다.

## Decision

**착지마다, 착지 커밋을 만들기 전에 이번 착지로 들어오는 ADR 을 나란히 놓고 사람이 대조한다.**
절차는 [`docs/dev-guide/adr-index.md`](../dev-guide/adr-index.md#착지-때-새-adr-끼리-대조) 가 운영 규칙으로 싣는다.

- **입력은 `A`·`M`·`R` 전부다.** `A` 만 보면 같은 기존 ADR 을 두 lane 이 서로 다르게 고친 겹침이
  빠진다.
- **돕는 도구 `scripts/adr-landing-report.sh` 는 보고만 한다.** ADR 마다 제목과 Decision 절의 백틱
  인용 이름을 찍고 같은 뿌리 이름에 둘 이상 걸린 것을 후보로 모은다. **겹침 후보가 있어도 0 으로
  끝난다.** 겹침은 텍스트로 안 갈리므로
  ([ADR-0243](0243-not-building-a-judge-has-three-reasons-and-the-left-side-is-the-last-one.md)) 실패를
  내면 실재하지 않는 중복을 처방하게 된다.
- **조회가 안 된 것은 2 로 가른다.** 없는 rev · 얕은 clone · git 오류는 판정 불가다. 목록이 빈
  초록과 목록을 못 뽑은 초록이 같은 줄로 보이면 안 된다.
- **겹치면 착지 전에** [ADR-0506](0506-a-decision-recorded-twice-is-split-by-the-width-of-the-overlap.md)
  의 절차로 고친다. 착지 뒤에는 두 번호가 다 인용되기 시작한다.
- **자동 채널에 걸지 않는다.** 착지하는 사람이 직접 돌린다.

## Consequences

- **얻은 것**: 동시에 갈라진 lane 끼리의 겹침이 처음 한 트리에 놓이는 순간 사람의 눈에 든다.
  2026-09-05 롤업(`f438af4ab`)에서 도구가 `ALLOWED ← 0128 0145 0169` 를 후보로 낸다(실측 2026-09-23).
- **잃은 것**: 사람이 안 돌리면 아무것도 안 돈다. 흔한 이름(`#[cfg(test)]` · `src/`)은 우연한
  후보를 만든다 — 같은 롤업에서 후보 줄 대부분이 그렇다.
- **운영 비용**: 착지마다 명령 한 번과 후보 읽기.

## Alternatives Considered

- **겹침 후보가 있으면 실패하는 게이트** — 텍스트로 안 갈리는 판정에 실패를 달면 오탐마다 면제가
  쌓인다. 안 골랐다.
- **pre-push 훅이나 CI 에 보고만 거는 것** — 착지는 lane 이 아니라 conductor 가 하고, 보고만 하는
  잡은 아무도 안 읽는 로그가 된다. 착지 절차 안의 단계로 두는 쪽이 읽힌다.
- **`A` 만 보는 입력** — 짧지만 같은 기존 ADR 을 두 lane 이 고친 겹침이 빠진다.
- **착지 대조를 안 둔다(작성 시점 탐색만)** — 실측 사례는 그것으로 잡힌다. 그러나 동시에 갈라진
  lane 의 겹침은 모수 밖이라 남는다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

**채널이 붙는 것** — 판정 시점에 레포가 읽을 수 있는 사실이다.

- `scripts/adr-landing-report.sh` 가 없어지거나 종료 코드 규약이 바뀌면 — 이 ADR 의 도구 서술이
  낡는다.

**원리적으로 안 붙는 것** — 사람이 관측해야 한다. 재는 법을 함께 적는다.

- 착지 뒤에 겹침이 발견되는 일이 반복되면 — 절차가 안 돌고 있거나 후보 추출이 놓친다. 재는 법:
  ADR-0506 절차가 쓰인 사례의 원본들이 같은 착지로 들어왔는지 `git log --diff-filter=A` 로 본다.
- 후보가 너무 많아 아무도 안 읽으면. 재는 법: 착지 한 번의 `--candidates-only` 줄 수를 본다.

## References

- 운영 절차: [`docs/dev-guide/adr-index.md`](../dev-guide/adr-index.md#착지-때-새-adr-끼리-대조)
- 도구: `scripts/adr-landing-report.sh`
- 작성 시점 탐색: [ADR-0507](0507-an-adr-is-written-after-a-symbol-search-and-only-for-a-choice.md)
- 겹침 처리: [ADR-0506](0506-a-decision-recorded-twice-is-split-by-the-width-of-the-overlap.md)
- 선행 결정: [ADR-0239](0239-an-unused-adr-number-is-retired-not-recycled.md) (다른 조항 — 착지 때 번호 구간 재부여)
