# ADR-0507: 새 ADR 은 결정 대상의 심볼로 기존 ADR 을 찾은 뒤에, 대안 사이의 선택일 때만 쓴다

- **Status**: Accepted
- **Date**: 2026-09-23
- **Tags**: adr-conventions, adr-duplication, documentation, search, adr-0506, adr-0243, adr-0244
- **Group**: docs-adr

## Context

**중복 쪽.** [ADR-0145](0145-typed-length-constructors-stay-open-for-now.md) 와
[ADR-0169](0169-the-tuple-constructor-of-length-types-stays-open.md) 가
[ADR-0128](0128-dpi-conversion-guarded-by-source-scan-not-sealed-types.md) 과 같은 조항("길이 타입
튜플 생성자를 봉인하지 않는다")을 인용 없이 다시 결정했다. 처리 절차는
[ADR-0506](0506-a-decision-recorded-twice-is-split-by-the-width-of-the-overlap.md) 이 정했고, 이
ADR 은 **작성 시점에 막을 수 있었는가**를 묻는다.

막을 수 있었다 — 재현했다(2026-09-23). 각 ADR 이 lane 에서 처음 쓰인 커밋의 **부모 트리**에서
결정 대상 심볼로 ADR 을 찾으면 선행 결정이 나온다.

```bash
git grep -l 'PhysicalPx(' 689bd5783^ -- docs/adr/   # 0145 원 커밋의 부모 → 0128
git grep -l 'PhysicalPx(' 65644206f^ -- docs/adr/   # 0169 원 커밋(당시 번호 0167)의 부모 → 0128 · 0145 · 0148
```

0145 와 0169 는 `main` 에 같은 롤업 커밋(`f438af4ab`)으로 함께 들어왔지만, 0169 를 쓰던 트리에는
0145 가 이미 있었다. "같은 롤업이라 서로 못 봤다" 는 설명은 틀렸다. `docs/adr/template.md` 에는
"쓰기 전에 기존 ADR 을 찾아라" 가 없었다.

**태그는 탐색 수단이 못 된다.** 서로 다른 태그 801 종 중 461 종이 한 번만 쓰였다(`adr-NNNN` 관계
태그 제외, 실측 2026-09-21). 같은 축의 ADR 이 같은 태그를 공유한다는 보장이 없다.

**쓸데없는 쪽.** 무엇이 ADR 이 될 자격인지 기준이 없었다. 루트 `CLAUDE.md` 는 "결정의 근거 /
대안 / 재검토 조건은 ADR 로 박는다" 뿐이고 `docs/documentation-model.md` 도 ADR 을 "왜 그렇게
결정했나" 로만 정의한다 — 결정이면 무엇이든 ADR 이 된다. 헤더 `Date` 기준 월별 수와 길이
중앙값(실측 2026-09-21): 2026-06 29 건·59 줄 · 07 28 건·61 줄 · 08 35 건·89 줄 · **09 248 건·117 줄**.
ADR 수가 늘수록 탐색 비용이 오르고 다음 중복의 확률이 오른다.

레포에는 이미 ADR 없이 "안 짓기로 한 결정" 을 남기는 자리가 있다 —
`crates/tasty-doc-guards/tests/adr_index_parity.rs` 모듈 주석의 "여기에 더 안 짓기로 한 것" 절이
판정 셋을 그 가드 옆에 이유·되돌아올 조건과 함께 적는다.

## Decision

**두 단계를 새 ADR 작성의 앞에 둔다.** 운영 규칙은 [`template.md`](template.md) "새 ADR 을 쓰기
전에" 가 싣고, 이 ADR 은 그 근거를 남긴다.

1. **자격.** 먼저 "이 결정은 이미 있는 문서 · 가드 모듈 주석 · 소스 주석 한 자리로 충분한가" 를
   묻는다 — 충분하면 ADR 을 안 만든다. 그다음 세 물음이 모두 예일 때 ADR 을 쓴다.
   - 서로 배타적인 **대안 중 하나를 골랐는가**. 측정 결과·사실 기록은 대안 선택이 아니다.
     보류·거부("하지 않는다")도 대안 선택이면 자격이 있다.
   - 그 선택이 **코드·설정·운영 절차에 남는가**.
   - 뒤집힐 수 있는 **재검토 조건이 있는가**.

   아니면 갈 곳: 현재 작업 규칙 → `docs/dev-guide/` · 한 가드에만 걸린 판단 → 그 가드의 모듈
   주석 · 측정 서사 → 커밋 본문.
2. **탐색.** 결정 대상의 **심볼·파일·설정 키**로 `git grep -l '<심볼>' -- docs/adr/` 를 돌린다.
   주제어 검색은 보조로만 쓴다. 찾은 ADR 과의 관계를 넷으로 가른다 — 같은 결정·같은 선택이면
   새 ADR 을 쓰지 않는다 · 같은 결정에 새 사실만 생겼으면 기존 ADR 을 보강한다(template 의 보강
   예외가 덮는 범위에서) · 선택이 바뀌었으면 조항은 부분 개정, 결정 전체는 Supersede · 다른
   조항이면 새 ADR 의 References 에 선행 결정으로 잇는다.
3. **흔적.** 새 ADR 의 References 에 탐색 결과를 한 줄 남긴다 — `- 선행 결정: [ADR-NNNN](<파일>)
   (<관계>)` 또는 `- 선행 결정 없음 — 탐색: <다시 돌릴 수 있는 명령>`. 명령을 적는 이유는
   리뷰어가 다시 돌려 보게 하기 위해서다. **앞으로 쓰는 ADR 에만 적용한다** — 기존 문서를 소급해
   고치지 않는다([ADR-0244](0244-the-trigger-split-is-marked-by-two-subheadings.md) 와 같은 판단).
4. **태그는 탐색 수단이 아니다.** 탐색은 인덱스 그룹과 심볼 grep 으로 한다. 태그 어휘를 통제하는
   규칙은 들이지 않는다.

**흔적의 존재를 보는 가드는 짓지 않는다 — [ADR-0243](0243-not-building-a-judge-has-three-reasons-and-the-left-side-is-the-last-one.md) 의 (ㄴ) 칸이다.**
그 가드가 볼 수 있는 것은 줄의 존재뿐이고, 실패문의 처방("한 줄 적어라")은 탐색 없이도 이행된다.
따르면 탐색의 질은 그대로인 채 줄만 는다.

## Consequences

- **얻은 것**: 0169 를 쓰려는 사람이 템플릿의 명령 한 줄로 0128 · 0145 를 만난다. 무엇이 ADR 이
  아닌지 갈 곳이 적혀 있어 측정 서사·작업 규율이 ADR 로 새지 않는다.
- **잃은 것**: 자격 판정이 사람 판단이라 경계 사례가 남는다 — 무작위 표본에서 한 가드 안에서만
  쓰이는 결정(예: [ADR-0123](0123-layering-guard-excludes-cfg-test-modules.md))이 모듈 주석으로
  충분한 쪽에 가깝게 읽혔다. 기존 ADR 을 이 기준으로 재분류하지 않는다.
- **운영 비용**: 새 ADR 마다 grep 한 번과 References 한 줄.

## Alternatives Considered

- **의미 중복 판정기** — 텍스트로 안 갈린다(ADR-0243). 안 짓는다.
- **흔적 존재 가드** — 위 Decision 끝 문단. 존재만 보고 질을 못 본다.
- **태그를 통제 어휘로 살린다** — `adr_index_parity` 가 본문 Tags ↔ 행 Tags 를 집합으로 대조하므로
  두 자리를 같이 고쳐야 하고, 801 종을 정리하는 일이 따른다. 인덱스 그룹이 같은 일을 더 싸게 한다.
- **자격 기준 없이 탐색만** — 중복은 줄지만 ADR 수의 증가(탐색 비용의 증가)는 그대로다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

**원리적으로 안 붙는 것** — 사람이 관측해야 한다. 재는 법을 함께 적는다.

- 흔적 줄을 가진 새 ADR 이 그런데도 기존 ADR 과 같은 조항을 다시 결정한 사례가 나오면 — 탐색
  절차가 심볼로 안 잡히는 형태가 있다는 뜻이다. 재는 법: ADR-0506 절차가 쓰인 사례의 원본들이
  흔적 줄을 가졌는지 본다.
- 자격 기준이 보류·거부 결정을 일괄로 "쓸데없다" 로 판정하는 쪽으로 읽히면. 재는 법: Deferred
  ADR 에 세 물음을 적용해 본다.

## References

- 운영 규칙: [`template.md`](template.md) "새 ADR 을 쓰기 전에"
- 중복 처리 절차: [ADR-0506](0506-a-decision-recorded-twice-is-split-by-the-width-of-the-overlap.md)
- ADR 없이 결정을 남기는 선례: `crates/tasty-doc-guards/tests/adr_index_parity.rs` 모듈 주석
- 선행 결정: [ADR-0506](0506-a-decision-recorded-twice-is-split-by-the-width-of-the-overlap.md) (다른 조항 — 발견 뒤 처리)
