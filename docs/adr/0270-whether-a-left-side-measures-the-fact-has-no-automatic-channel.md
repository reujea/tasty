# ADR-0270: 좌변이 그 사실을 재는가에는 자동 채널을 안 붙인다 — 기계가 읽는 두 모양만 가드가 본다

- **Status**: Accepted
- **Date**: 2026-09-14
- **Tags**: docs, guards, channels, mutation-testing, false-positive, observability, adr-0142, adr-0151, adr-0220
- **Group**: guard-design

## Context

"이 가드가 본다 / 이 잡이 강제한다 / 배선돼 있다" 는 서술이 거짓인 형태가 회차마다
반복된다. [ADR-0142](0142-channel-claims-are-written-against-the-working-tree.md) 가 채널
주장의 층을 명령 · 발화 · 원격 · 결과로 갈랐고, [ci-gates](../dev-guide/ci-gates.md) 가
"돈 적이 있는가" 를 더해 다섯으로 운영한다. 반복해서 걸리는 것은 그 다섯보다 **한 층
앞**이다 — 그 잡 · 그 트리거 · 그 가드가 **내가 말한 그 사실을 애초에 재는가.**

관측된 모양은 넷이다: 재검토 조건이 안 깨지는 층을 트리거로 삼은 것, ADR 이 인가 집합을
실제보다 좁게 적은 것, 가이드가 발신자 없는 event 를 현재형 배선으로 적은 것, 가드가 그
구멍이 닫혀 있다고 단정한 것. 넷 다 소스를 읽어서는 안 보였고 변이를 심었을 때 드러났다.

그 규율(변이로 확인하고, 못 붙이면 채널이 없다고 적고 재는 법을 남긴다)은 lane prompt 에만
있었다. 규율의 정본은 이제 [documentation-model](../documentation-model.md) §6 에 있다. 남은
물음은 **그 규율 자신에 채널이 붙는가**다 — 규율이 "채널이 없으면 없다고 적어라" 를
말하므로 이 물음을 비워 둘 수 없다.

## Decision

**이 층 전체에는 자동 채널을 안 붙이고, 그 부재를 규칙 본문에 적는다.** 한 문장이 가리키는
사실이 무엇인지는 문장 밖(쓴 사람의 의도)에 있어, 지목된 것이 그 사실을 재는지는 기계가
못 읽는다. 이 층의 값은 변이로만 나온다.

그 층 안에서 **기계가 읽을 수 있는 모양 둘**은 이미 가드가 본다. 이 결정은 그 둘을 채널로
인정하고 그 이상으로 넓히지 않는다.

- 지목한 좌표가 **실재하는가** — `crates/tasty-doc-guards/tests/cited_coordinates_exist.rs`
  ([ADR-0151](0151-cited-coordinates-are-judged-as-literals-not-by-context.md)).
- 이름이나 경로로 지목한 시험이 **자동으로 도는가** —
  `crates/tasty-doc-guards/tests/ci_channel_claims_match_workflows.rs`.

뒤쪽이 이 결정의 논거로 쓰이므로 결정 시점에 변이로 확인했다(2026-09-14, base `2939832ea`).
추적 문서 하나에 〈자동 잡이 안 도는 통합 타깃의 경로 + 강제 어휘〉 한 문장을 심자
`no_file_claims_ci_enforces_an_integration_target_no_automatic_job_runs` 가 죽었다(rc=101).
음성 대조로 같은 문장의 경로만 자동으로 도는 `tasty-doc-guards` 통합 타깃으로 바꾸자
통과했다(rc=0). 두 번 다 `cp -p` 백업으로 되돌리고 `cmp` 로 바이트 동일을 확인했다.

## Consequences

- **얻은 것**: 이 층을 "가드가 본다" 고 부풀려 적지 않는다. 규율을 읽는 사람은 자기가 변이를
  안 심으면 아무도 안 심는다는 것을 안다.
- **얻은 것**: 면제 목록이 새로 생기지 않는다(아래 Alternatives 의 첫째).
- **잃은 것**: 위 넷과 같은 모양은 다음에도 리뷰나 변이로만 잡힌다. 반복을 막는 수단은 규칙
  본문과 그것을 인용하는 lane prompt 뿐이다.
- **운영 비용 / 유지 부담**: 채널 주장을 새로 쓰거나 고칠 때마다 변이 한 쌍(양성 · 음성)을
  손으로 돌린다.

## Alternatives Considered

- **어휘 가드** — "강제한다 / 본다 / 잡는다 / 배선돼 있다" 류 서술 옆에 변이 좌표를 요구한다.
  기각: 그 동사들의 주어는 채널만이 아니다("파서가 강제한다" 는 제품 동작이다). 결정 시점에
  `git grep -F` 로 `docs/` 를 훑자 그런 동사 어구 하나하나가 열 자리 넘게 걸렸고, 채널 주장과
  제품 서술을 가를 표지가 문장에 없었다. 거짓 양성의 처방은 면제 등록이고 등록된 자리는
  영구히 헐거워진다 — 이 규율이 막으려는 결과를 가드가 만든다.
- **ADR-0142 에 한 층을 더하는 개정** — 기각: 0142 는 작업 트리와 원격의 층을 다루고, 그
  층들은 워크플로 파일이라는 읽을 수 있는 좌변이 있다. 이 층은 좌변이 문장 밖이라 판정
  방식이 다르다. 한 ADR 에 섞으면 "①② 는 기계가 잡게 한다" 는 0142 의 결정이 이 층에도
  적용되는 것처럼 읽힌다.
- **변이 기록을 커밋해 대조** — 서술마다 심은 변이와 rc 를 표로 남기고 테스트가 그 표의
  존재를 본다. 기각: 표가 있다는 것은 변이가 지금도 죽는다는 것을 말하지 않는다. 사람이
  갱신하는 스냅샷이라 [ADR-0133](0133-guard-scan-population-is-pinned-not-enumerated.md) 이
  막으려는 실패와 같은 형태다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

**채널이 붙는 것**

- 위 두 가드 중 하나가 삭제되거나 이름이 바뀐다 — 이 결정이 "기계가 읽는 모양" 으로 인정한
  채널이 사라진다. 인용 좌표이므로 `cited_coordinates_exist` 가 이 파일의 경로 인용을 깨뜨려
  발동시킨다.

**원리적으로 안 붙는 것**

- 서술이 가리키는 사실을 구조로 선언하는 규격(예: 채널 주장마다 대상 시험과 깨는 변경을
  기계가 읽는 형태로 적는 표기)이 도입된다 — 그러면 문장 밖에 있던 좌변이 문장 안으로
  들어온다. 재는 법: 그 규격을 쓰는 문서가 생겼는지 리뷰에서 본다.
- 이 층의 거짓 서술이 리뷰 · 변이로 잡힌 뒤에도 다음 회차에 같은 모양으로 계속 새로
  들어온다 — 규칙 본문이 반복을 못 막는다는 뜻이다. 재는 법: 회차 리뷰 보고에서 위 네 모양
  중 하나로 분류된 결함을 센다.

## References

- [documentation-model](../documentation-model.md) §6 — 규율의 정본. 이 ADR 은 그 규율에 채널이
  붙는가만 결정한다.
- [self-verification](../dev-guide/self-verification.md) — 변이 절차와 안 죽는 변이를 먼저
  의심하는 순서.
- [ADR-0142](0142-channel-claims-are-written-against-the-working-tree.md) — 이 층 뒤에 오는
  채널 주장의 층들.
- [ADR-0151](0151-cited-coordinates-are-judged-as-literals-not-by-context.md) — 좌표 실재 가드.
- [ADR-0220](0220-reconsideration-triggers-are-split-by-observability.md) — 채널을 붙일 수 있는
  조건과 없는 조건을 가르는 기준.
