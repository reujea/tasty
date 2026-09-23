# ADR-0558: push 범위 안쪽의 커밋에는 초록을 요구하지 않는다 — 초록의 단위는 push tip 과 착지 tip 이다

- **Status**: Accepted
- **Date**: 2026-09-23
- **Tags**: ci, git-hooks, pre-push, push-range, landing, bisect, unmeasured, adr-0192, adr-0142
- **Group**: ci-gates

## Context

자동 채널 셋이 보는 모수는 서로 다르다.

- **pre-commit** — staged 트리(plugin 버전 `P.1` 은 `main` 과의 merge-base 대비).
- **pre-push** — `B.9`·`B.10` 은 git 이 stdin 으로 건넨 **원격 tip 과 로컬 tip**. 컴파일 스텝
  `B.4`~`B.8` 은 tip 도 아니고 훅이 도는 **작업 트리**다(`cargo` 를 레포 루트에서 부른다).
- **CI** — push 된 커밋 하나(tip)를 체크아웃한다. 범위를 쓰는 워크플로는
  `plugin-version-check.yml` 하나이고 그것도 `github.event.before` 와 tip 의 **두 끝점**이다.

그래서 push 범위의 **안쪽 커밋은 어느 채널도 안 본다.** 한 회차(2026-09-23)에 이 형태가 세 번
났다 — clippy 빨강이 다음 커밋에서 고쳐진 것, 상한 래칫의 사본이 네 커밋 동안 낡았다가 맞춰진
것, 명부 개수 가드가 한 커밋에서 빨갛다가 tip 에서 초록이 된 것. 셋 다 tip 은 초록이었다.
그 회차의 착지는 이력을 다시 쓰지 않고 그대로 두었다.

그 결정을 규율로 세울지는 **중간 커밋의 초록을 소비하는 자리가 있는가**에 달렸다. 소비한다는
것은 누군가 그 커밋을 체크아웃해 그 위에서 무언가를 판정한다는 뜻이다. 실측(2026-09-23):

| 소비 후보 | 모수 | 값 |
|---|---|---|
| `git bisect` — 추적 파일 | `git grep -i bisect`(vendored JS·자산 제외) · `CHANGELOG.md` | 0 · 0 |
| `git bisect` — 커밋 메시지 | `git log --all -i --grep=bisect` | 0 |
| `git bisect` — 실제 실행 | 이 레포 작업 세션 기록 632 개(2026-08-12 부터)에서 `git bisect start/run/good/bad` 를 담은 셸 명령 | 0 |
| 기계 revert(`git revert <sha>`) | `main` 5831 커밋 중 `This reverts commit` 본문 | 0 |
| 손 revert | 제목에 revert 를 단 커밋 | 5 — 전부 tip 위의 전진 커밋. 특정 중간 커밋을 집은 것 없음 |
| CI 가 범위 안쪽을 순회 | 워크플로 11 개 | 0 |
| lane 이 중간 커밋 위에 선다 | lane 브랜치 101 개의 merge-base 중 push tip 이 아닌 것 | 98(고유 커밋 73). 그 73 중 72 가 어느 lane 브랜치의 tip 과 같은 제목 = **착지 tip** |
| 빨간 중간 커밋이 base 로 쓰였다 | 위 회차의 빨간 중간 커밋 중 `main` 에 사본이 있는 것 셋 | 0 |

세션 기록에서 `bisect` 가 나온 자리는 전부 산문이었다 — "bisect 를 깨지 않으려 별도 커밋 대신
amend 했다" 는 판단 서술, 그리고 CI 빨강의 원인 커밋을 좁히는 절차 안의 "bisect 로(또는 그
커밋들을 임시 worktree 로 떠서)" 라는 처방이다. 앞의 것은 **쓰인 적 없는 소비자를 위해 비용을
치른 것**이고, 뒤의 것은 실제로는 worktree 쪽으로 수행됐다.

중간 커밋을 체크아웃하는 **실재하는** 소비자는 lane base 하나이고, 그것은 착지 tip 에 선다.
착지 tip 은 착지하는 쪽의 통합 회차가 병합 트리에서 잰다([ADR-0192](0192-a-repo-wide-ratchet-is-judged-at-the-merge-tree-not-per-lane.md)).

비용 쪽도 쟀다. push 범위의 크기는 origin/main reflog 의 연속 push tip 쌍 194 개에서 중앙값 8 ·
p90 54 · 최대 311 커밋이다. 각 커밋에 pre-push 의 빠른 게이트를 돌리는 비용은 아래
Alternatives 의 ⒜ 에 적었다.

## Decision

**초록을 요구하는 단위는 push tip 과 착지 tip 둘이다. push 범위 안쪽의 커밋에는 초록을 요구하지
않고, 그것을 재는 채널을 두지 않는다.**

- 빨간 중간 커밋이 나중에 발견돼도 **이력을 다시 쓰지 않는다.** 그 빨강은 회귀가 아니라 미측정
  구간이고, 고친 커밋이 이미 같은 범위 안에 있다.
- lane 이 착지 전에 자기 커밋을 amend·fixup 으로 다듬는 것은 **허용하되 요구하지 않는다.**
  "bisect 를 깨지 않으려" 는 그 근거가 되지 않는다 — 그 소비자는 이 레포에 없다.
- 원인 커밋을 좁혀야 할 때(CI 빨강의 귀속 등)는 **push tip 과 착지 tip 위에서** 좁힌다. `git bisect` 를 쓴다면
  push tip 도 착지 tip 도 아닌 커밋은 `git bisect skip` 으로 넘긴다. 초록을 요구하는 단위가 그 둘뿐이기 때문이다.
- 이것은 **안 재기로 한 결정**이다 — 조용히 안 재는 것과 다르다. 그래서 채널 문서
  [ci-gates](../dev-guide/ci-gates.md) 가 "범위 안쪽은 아무 채널도 안 본다" 를 그 좌표와 함께 적는다.

## Consequences

- **얻은 것**: pre-push 가 커밋 수만큼 느려지지 않는다. 기능 단위 즉시 커밋 정책(루트 `CLAUDE.md`
  "커밋 정책")이 "커밋마다 전량 초록" 이라는 추가 조건 없이 유지된다.
- **얻은 것**: 빨간 중간 커밋을 발견했을 때의 처방이 정해져 있다 — 고치지 않는다. 회차마다 같은
  판단을 다시 하지 않는다.
- **잃은 것**: `git bisect` 를 쓰는 날, 커밋 단위 해상도가 push tip·착지 tip 단위로 떨어질 수 있다. 그날은
  `skip` 이 늘어난다.
- **잃은 것**: 범위 안쪽 커밋을 체크아웃해 무언가를 재는 사람은 그 커밋이 빌드되리라 가정할 수
  없다. 대조군(base)을 고를 때는 push tip 이나 착지 tip 을 고른다.
- **운영 비용 / 유지 부담**: 없음. 채널이 없으므로 유지할 것도 없다. 대신 아래 재검토 조건을
  사람이 본다.

## Alternatives Considered

- **⒜ pre-push 가 `--range` 의 각 커밋에 빠른 게이트(fmt · clippy correctness · doc-guards)를
  돌린다** — 비용이 범위 크기에 선형이다. 실측(2026-09-23, 지금 push 대기 중인 범위의 끝 6 커밋,
  커밋마다 체크아웃해 같은 target 디렉토리로 워밍된 상태, 머신 load average 35~66):

  | 게이트 | 커밋당 벽시계 |
  |---|---|
  | `cargo fmt --check` | 2.7 ~ 10.9 s |
  | `cargo test -p tasty-doc-guards --locked` | 47 ~ 240 s (6 중 넷이 63 ~ 77 s) |
  | `cargo clippy --workspace --all-targets -- -D clippy::correctness` | **미측정** — 6 커밋 전부 번들 plugin 한 크레이트에서 rc=101 로 일찍 멈춰 워크스페이스를 다 돌지 않았다 |

  clippy 를 빼고도 커밋당 약 1 분이 하한이다. 중앙값 범위 8 이면 약 9 분, p90 범위 54 면 약 1
  시간, 이 측정 시점에 push 를 기다리던 범위 194 면 3 시간을 넘는다 — 전부 clippy 를 뺀 하한이다.
  소비자가 0 인 성질에 그 비용을 매 push 마다 치를 이유가 없다. 고른다면 판정 불가(원격 tip 이 로컬에 없음·체크아웃 실패)는
  `B.9` 처럼 실패로 떨어져야 했다.
- **⒝ lane 이 착지 전에 `git rebase --exec` 로 자기 커밋을 스스로 훑는다** — ⒜ 와 같은 비용을
  lane 쪽으로 옮길 뿐이고, 병합 트리에서의 상호작용(두 lane 이 합쳐져야 나는 빨강)은 lane 이
  원리적으로 못 본다. 강제 채널도 없어 관행에 그친다.
- **⒞ 착지 때 lane 을 한 커밋으로 squash 한다** — 범위 안쪽이 착지 tip 만 남아 문제가 사라지지만,
  기능 단위 커밋이라는 커밋 정책과 커밋 본문의 측정 서사를 잃는다.
- **⒟ 지금처럼 아무것도 적지 않는다** — 안 재는 사실이 적히지 않으면 다음 회차가 같은 발견을
  같은 비용으로 다시 한다. 이번 결정은 그것을 막으려는 것이다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다. 불릿마다 괄호 안 기준 값은 그 값을 실제로 잰
시점으로 적는다 — 첫 불릿은 착지 tip 에서 다시 쟀고, 나머지 둘은 티켓 조사 때 잰 값이며
그 뒤로 다시 재지 않았다.

**채널이 붙는 것**

- 추적 파일(스크립트 · 워크플로 · 훅 · 문서 절차)이 `git bisect` 나 커밋 순회(`git rev-list`
  위를 도는 체크아웃)를 절차로 들인다. 재는 법: `git grep -n -E 'git bisect|rev-list' -- scripts
  .github .githooks docs/dev-guide`. 이 결정이 착지한 시점의 적중은 5 다(결정을 처음 적은
  커밋에서는 4 였고, 같은 lane 이 ci-gates 에 측정 명령을 한 줄 더하면서 5 가 됐다). 다섯 중
  셋은 `rev-list --count`(개수 세기), 하나는 ci-gates 의 47 측정 파이프(`rev-list` 출력을
  `grep -c` 로 세는 것 — 체크아웃 없음), 하나는 이 결정을 옮겨 적은 ci-gates 의 서술이다 —
  순회·실행은 0. 이 결정은 가드를 두지 않는다 — 위 명령을 사람이 돌린다.
- 착지 방식이 바뀌어 lane 이 착지 tip 이 아니라 lane 내부 커밋 위에서 출발한다. 재는 법: lane
  브랜치마다 `git merge-base <브랜치> main` 을 구해, 그것이 어느 브랜치 tip 과도 같지 않은 수를
  센다(티켓 조사 시점(2026-09-23) 73 중 1).

**원리적으로 안 붙는 것**

- 누군가 실제로 `git bisect` 를 돌렸고 빨간 중간 커밋이 원인 귀속을 틀리게 했다. 재는 법: 작업
  세션 기록에서 `git bisect start` 를 담은 셸 명령을 센다(티켓 조사 시점(2026-09-23) 0).
- 빨간 중간 커밋이 lane base 로 쓰여 그 lane 의 측정을 오염시켰다는 보고가 나온다.

## References

- 채널 문서: [dev-guide/ci-gates](../dev-guide/ci-gates.md) — "범위 안쪽 커밋" 절
- 훅: `.githooks/pre-push` (`B.9` 의 모수 규칙 — 모수를 못 정하면 실패)
- 선행 결정: [ADR-0192](0192-a-repo-wide-ratchet-is-judged-at-the-merge-tree-not-per-lane.md) (최종 판정은 병합 트리에서 — 착지 tip 이 초록의 단위인 근거)
- 선행 결정: [ADR-0142](0142-channel-claims-are-written-against-the-working-tree.md) (채널 주장은 작업 트리 기준 — 여기서 "안 본다" 를 적는 방식)
- 선행 결정 없음(같은 조항) — 탐색: `git grep -l -E 'bisect|중간 커밋|rebase --exec' -- docs/adr/`
