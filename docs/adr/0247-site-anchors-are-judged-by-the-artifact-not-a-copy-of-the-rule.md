# ADR-0247: `site/content/` 의 앵커는 산출물을 읽는 판사에게 넘긴다 — ADR-0201 대체

- **Status**: Accepted
- **Date**: 2026-09-08
- **Tags**: documentation, anchors, slug, guards, site, astro, two-judges, adr-0201, adr-0139
- **Group**: docs-adr

## Context

[ADR-0201](0201-slug-rules-are-scoped-by-the-tree-that-renders-them.md) 은 이 저장소에
앵커 슬러그 규칙이 **둘** 있다는 사실 위에 서 있었다. GitHub 렌더러의 규칙(`_` 를 남기고
양끝 `-` 를 안 뗀다)과, `site/content/` 를 렌더하던 손으로 쓴 러스트 생성기의
규칙(`_` 를 `-` 로 접고 양끝 `-` 를 뗀다). 그 ADR 의 결정은 "통일하지 않는다 — 그 앵커를
실제로 푸는 렌더러가 그 트리의 규칙을 정한다" 였고, 첫째 재검토 조건이 **"사이트 생성기가
슬러그 규칙을 바꾼다"** 였다.

사이트가 Astro 로 옮겨 가면서 그 생성기가 **삭제됐다.** 재검토 조건이 발동한 것이고, 그
발동을 가드가 실패로 잡았다("사이트 렌더러 원문을 못 읽었다").

### 다시 잰 값

**두 규칙이 갈리던 두 자리를 오늘 렌더러로 재현했다.** `site/content/` 안의 440 개 헤딩은
두 규칙이 같은 답을 내는 것뿐이라(ADR-0201 의 표에서도 갈림 0) 그 트리로는 아무것도 못
가른다 — 갈리는 형태를 **넣어서** 쟀다.

| 넣은 헤딩 | 산출 `id` | GitHub 규칙 | 옛 사이트 규칙 |
|---|---|---|---|
| `probe set_context 송신 정책` | `probe-set_context-송신-정책` | **같다** | 다르다(`set-context`) |
| `probe 완료 판정 전략 (src/completion_strategy)` | `probe-완료-판정-전략-srccompletion_strategy` | **같다** | 다르다(`completion-strategy`) |
| `probe_a-b (c) -` | `probe_a-b-c--` | **같다** | 다르다(`probe-a-b-c`) |
| `` probe `--surface` 생략과 다중 윈도우 `` | `probe---surface-생략과-다중-윈도우` | **같다** | 같다 |
| `probe 가운데 — 긴 줄표` | `probe-가운데--긴-줄표` | **같다** | 같다 |

즉 **규칙이 하나가 됐다.** 갈리던 두 결정(`_` 보존 · 양끝 `-` 보존)이 둘 다 GitHub 쪽으로
수렴했다. ADR-0201 의 대안 A("사이트 `slugify` 를 GitHub 규칙에 맞춘다")가 그 ADR 이
피하려던 대가를 치르며 사실상 실행된 셈인데, 그 대가는 **이 결정으로 치른 것이 아니다** —
렌더러를 통째로 바꾼 마이그레이션의 부수효과다.

### 그런데 판사가 사라져 있었다

같은 마이그레이션이 조용한 구멍을 하나 냈다. ADR-0201 이 넘긴 쪽 판사는 옛 생성기의
`--strict` 였고 `pages.yml` 이 그것을 불렀다. Astro 로 옮기면서 그 스텝이 `npm run build`
로 바뀌었는데, 링크·앵커 판정은 별도 스크립트(`site/scripts/check-links.mjs`)로 나가
있었고 **부르는 곳이 없었다.** 스크립트는 멀쩡히 있었고 배선만 없었다 — 그 상태에서
`site/content/` 의 앵커를 보는 판사는 **하나도** 없었다.

## Decision

**넘김은 유지한다. 근거를 "규칙이 다르다" 에서 "실측이 사본보다 나은 판사다" 로 바꾼다.**

`crates/tasty-doc-guards/tests/cited_anchors_resolve.rs` 는 `site/content/` **안을
가리키는** 앵커를 계속 판정에서 뺀다. 넘기는 조건도 그대로다 — 두 끝이 모두 content 트리
안일 때만. 바뀌는 것은 셋이다.

1. **넘기는 근거.** 이 가드의 `slug` 는 GitHub 규칙을 옮겨 적은 **사본**이고, 사이트
   판사는 산출된 HTML 의 `id` 를 **그대로 읽는다**. 사본은 원본이 바뀌면 조용히 낡고,
   실측은 안 낡는다. 그리고 오늘 사이트 렌더러는 이 레포가 아니라 **의존 라이브러리**라
   그 규칙은 이 레포의 커밋 없이 바뀐다(`npm ci` 가 다른 판을 받아 오는 것만으로) — 사본
   으로 따라갈 수 있는 대상이 아니다.
2. **지키는 것.** 규칙의 사본을 대조하던 시험(`site_slug_still_mirrors_the_site_renderer`)
   을 없애고, **판사가 배선돼 있는가**를 재는 시험으로 바꾼다
   (`the_site_anchor_judge_is_still_wired`). 축 둘 — 판정기(`check-links.mjs` 가 `id` 를
   모으고 두 갈래에서 대조하는가)와 배선(`pages.yml` 이 그것을 부르는가). 이 자리가 실제로
   끊긴 방식이 "스크립트는 있고 배선만 없다" 였으므로 한 축만 보면 그 상태가 초록이다.
3. **배선 복구.** `pages.yml` 에 `npm run check-links` 스텝을 되살린다.

## Consequences

- **얻은 것**: 넘긴 자리에 판사가 실재하는지가 **시험으로** 지켜진다. ADR-0201 시절에는
  그것이 문장이었고(수동 회차 확인), 그래서 배선이 끊긴 것을 아무도 못 봤다. 그리고 이
  레포에서 사라진 규칙의 사본을 더 이상 안 들고 있는다.
- **잃은 것**: 사이트 쪽 앵커 판정이 **경로 필터 뒤에** 있다는 사실은 그대로다. `pages.yml`
  은 `site/**` 등을 담은 push 에서만 돈다 — 다만 그 트리의 앵커가 깨지는 길이 링크 수정과
  헤딩 수정 둘뿐이고 둘 다 `site/**` 라 사각이 안 생기는 것도 그대로다.
- **운영 비용**: `check-links` 스텝 하나. 산출 트리를 한 번 훑는다.

## Alternatives Considered

- **A: 넘김을 없애고 `site/content/` 를 이 가드의 좌변에 들인다** — 규칙이 하나가 됐으니
  판사도 하나면 된다는 쪽. 안 고른 이유: 그러면 그 트리를 **사본**이 판정한다. 사본이
  맞는지는 오늘 실측했지만 그 실측의 수명은 다음 `npm ci` 까지다. 판사가 둘이라 답이 둘이
  될 위험보다, 유일한 판사가 조용히 낡을 위험이 크다 — 앞쪽은 빨강이고 뒤쪽은 초록이다.
- **B: 규칙 사본을 그대로 두고 의존 라이브러리를 따라간다** — 안 고른 이유: 따라갈 좌표가
  없다. 사본을 지키던 시험의 원문 축은 삭제된 그 생성기, 즉 **우리 파일**이었다(경로는
  ADR-0201 이 이름으로 적어 뒀다). 그 자리에
  이제 `node_modules` 아래 남의 코드가 앉는데, 그것은 커밋되지 않아 CI 에도 갓 클론한
  트리에도 없다 — 못 읽은 채로 통과하면 "규칙이 안 바뀌었다" 가 아니라 "안 봤다" 다.
- **C: ADR-0201 을 그대로 두고 값만 갱신한다** — 안 고른 이유: 갱신할 값이 없다. 그 ADR 의
  판단은 "갈리는 헤딩 102 · 그 형태로 깨진 링크 0" 이라는 **저울** 위에 있었고, 좌변이 0 이
  되면 저울이 없어진다. 값이 아니라 전제가 바뀌었으므로 새 결정이다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

**채널이 붙는 것** — 판정 시점에 레포가 읽을 수 있는 사실이다. 앞엣것은
`the_site_anchor_judge_is_still_wired` 가, 뒤엣것은 두 가드의 본 판정이 잡는다.

- **`site/scripts/check-links.mjs` 가 앵커를 안 보게 된다** 또는 **`pages.yml` 이 그것을
  안 부르게 된다** — 넘긴 곳이 비면 넘김의 전제가 사라진다. 그때 할 일은
  `RENDERER_OWNED_PREFIX` 를 지우는 것이 아니라 판사를 되살리는 것이고, 되살릴 수 없으면
  그 트리를 이 가드의 좌변에 들이는 것이다(대안 A 를 그때 다시 잰다).
- **두 판사의 답이 실제로 갈린 사례가 한 건이라도 나온다** — 겹치는 자리가 0 이라는 서술이
  거짓이 된다. 어느 쪽이든 빨강으로 나오므로 조용히 안 지나간다.

**원리적으로 안 붙는 것** — 사람이 관측해야 한다. 재는 법을 함께 적는다.

- **사이트 렌더러가 이 레포 안으로 돌아온다** — 사본으로 따라갈 대상이 다시 생기므로
  "실측이 사본보다 낫다" 의 두 번째 논거(따라갈 좌표가 없다)가 낡는다. 재는 법:
  `site/package.json` 의 의존에서 마크다운 처리기가 빠지고 그 일을 하는 코드가 `site/src/`
  아래로 들어왔는가.

## References

- 개정 대상: [ADR-0201](0201-slug-rules-are-scoped-by-the-tree-that-renders-them.md) — 이
  ADR 이 통째로 대체한다.
- 판정기: `crates/tasty-doc-guards/tests/cited_anchors_resolve.rs` 의
  `RENDERER_OWNED_PREFIX` 와 `the_site_anchor_judge_is_still_wired` — 이 결정이 실현된
  현재 위치다(결정 당시의 기록이 아니라).
- 넘긴 쪽 판사: `site/scripts/check-links.mjs` 와 `.github/workflows/pages.yml` 의
  `npm run check-links` 스텝.
- 규칙 본문: [`documentation-model.md`](../documentation-model.md) 의 "인용한 앵커는
  풀려야 한다".
- 값의 성격 분류: [ADR-0139](0139-numbers-in-docs-are-classified-by-lineage-not-by-name.md).
