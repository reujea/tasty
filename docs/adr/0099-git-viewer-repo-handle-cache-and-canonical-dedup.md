# ADR-0099: git-viewer 는 활성 worktree 의 repo 핸들을 하나만 들고, worktree 중복 검사는 미리 잰 정규화 경로로 한다

- **Status**: Accepted
- **Date**: 2026-09-03
- **Tags**: plugin, git-viewer, git2, cache, invalidation, performance, worktree
- **Group**: bundled-plugins

## Context

git-viewer plugin 은 조작 하나마다 `Repository` 를 새로 열었다. popup 최초 로드는
`load` 에서 한 번 열고 `bind_active` 가 **같은 repo** 를 또 열었고, Refresh 는 worktree
목록 재수집용으로 한 번 + 컬렉션 재바인딩용으로 또 한 번 열었으며, 파일 클릭
(`load_diff`)은 클릭할 때마다 열었다. `Repository::discover`/`open` 은 `.git` 을 위로
훑고 config·odb·refdb 를 초기화하는 syscall 다발이라, 이 비용이 클릭 순간의 멈칫으로
드러난다. 스크롤 프레임 비용(ADR-0095 가 다룬 축)과는 다른 축이다.

핸들을 상태에 들고 있어도 되는지가 전제였다. `git2::Repository` 는 `Send` 지만 `Sync`
는 아니다(`git2` 의 `unsafe impl Send for Repository`). plugin SDK 는 plugin 을 단일
`plugin-worker` 스레드로 옮겨 그 스레드에서 `&mut self` 로만 직렬 dispatch 하므로
(`tasty-plugin-sdk` 의 `worker_loop`), plugin 상태가 다른 스레드와 공유되는 경로가
없다. 전제는 성립한다.

두 번째 비용은 worktree 수집에 있었다. `collect_worktrees` 의 중복 검사가 이미 담긴
항목을 훑을 때마다 `std::fs::canonicalize` 를 다시 불렀다 — 항목 수의 제곱에 비례하는
디스크 syscall 이다. 중복 검사 자체는 비표준 레이아웃에서 main 합성분과 linked 항목이
같은 경로로 겹치는 것을 막는 방어라 없앨 수 없다.

세 번째로 `collect_log` 는 호출마다 전 ref 를 훑어 `oid → ref 이름` 맵을 만든다.
이것도 캐시 후보였다.

## Decision

**활성 worktree 의 `Repository` 핸들 하나만 plugin 상태에 캐시하고, 접근은
`take_repo`/`put_repo` 한 쌍으로 좁힌다.** 캐시 항목은 `(핸들이 바인딩된 workdir, 핸들)`
이며, 요청 경로가 캐시 키와 다르면 미스로 보고 옛 핸들을 그 자리에서 버린다.
`take_repo` 는 **항상 캐시를 비우고** 시작한다 — 호출자가 다 쓴 뒤 `put_repo` 로
돌려놓아야만 다음 호출이 재사용하므로, 중간에 에러로 빠져나가는 경로는 캐시를 빈 채로
남기고 다음 조작이 무조건 다시 연다. 낡은 핸들이 살아남는 코드 경로가 구조적으로 없다.

**무효화 조건은 세 가지이고 전부 명시적이다.**

| 조건 | 처리 |
|---|---|
| worktree 전환 | `select_worktree` 가 캐시를 비우고, 뒤이은 `bind_active` 도 키 불일치로 미스 |
| Refresh(외부 파일 편집 · 외부 worktree add/remove · 외부 커밋) | `refresh` 진입 즉시 캐시를 비운다 |
| repo 소실 | `take_repo` 의 재열기 실패 → 캐시는 이미 빈 상태, `error` 표시 |

**Refresh 는 캐시를 무조건 버린다.** Refresh 는 "지금 상태를 다시 읽어달라" 는 명시적
요청이므로 최신성이 캐시 적중률보다 우선한다. 외부 변경이 반영되는지가 핸들 수명에
좌우되면 안 된다. 대신 Refresh 안에서 worktree 목록 재수집용으로 연 repo 를 활성
worktree 와 같을 때 그대로 넘겨, 한 번의 Refresh 안에서 같은 repo 를 두 번 열던 것만
한 번으로 줄인다. **worktree 목록 재수집 자체는 Refresh 마다 계속 한다** — 외부
`git worktree add/remove` 를 반영하는 경로가 이것 하나뿐이다.

**worktree 중복 검사는 항목마다 어차피 한 번 재는 정규화 경로를 모아 두고 그것끼리
비교한다.** 검사 자체는 그대로 남고, `canonicalize` 호출만 항목 수에 선형이 된다.

**`collect_log` 의 ref 맵은 캐시하지 않는다.** ref 는 커밋/브랜치 조작 한 번으로 바뀌고,
이 함수가 불리는 시점이 곧 "최신 상태를 보여달라"(popup open / Refresh / worktree 전환)는
순간이라 캐시는 낡은 pill 을 띄울 위험만 만든다. 비용도 ref 개수에 선형이라 같은
함수 안의 revwalk(상한 200 커밋) 대비 미미하다. 대신 unborn HEAD 로 조기 반환하는
경우에는 ref 스캔을 아예 하지 않도록 순서만 바꿨다.

## Consequences

- **얻은 것**: popup 최초 로드가 repo 2 회 open → 1 회, Refresh 가 2 회 → 1 회, 파일 클릭이
  매번 1 회 → **0 회**(캐시 적중)가 됐다. 앞의 두 수치는 **활성 worktree 가 popup cwd 의
  worktree 일 때** 성립한다 — 활성이 다른 worktree 면 목록 수집용과 재바인딩용으로 2 회가
  남는다. 또한 이 수치는 plugin 자체 `discover` 만 센 것이고, `collect_worktrees` 가 항목마다
  HEAD 를 읽으려 여는 open(main + linked 각 1 회)은 이 결정의 대상이 아니라 그대로다.
  파일을 연달아 클릭하는 흐름에서 open 비용이 사라진다. worktree 수집의 `canonicalize` 가
  O(n²) → O(n).
- **잃은 것**: plugin 상태가 열린 fd 를 하나 붙들고 산다(활성 worktree 하나 분). 캐시 키가
  경로 비교라, 같은 worktree 를 다른 표기(Windows verbatim prefix 등)로 요청하면 미스가
  나 다시 연다 — 정확성 손실은 없고 최적화만 놓친다.
- **잃은 것 (Windows)**: 캐시된 `Repository` 가 popup 수명 동안 packfile mmap 을 붙든다.
  Windows 는 mmap 된 파일의 삭제가 실패하므로, popup 이 열린 채 외부에서 `git gc` /
  `git repack -d` 가 돌면 구 pack 삭제가 실패할 수 있다(Linux/macOS 는 열린 fd 와 unlink 가
  무관해 영향 없다). 핸들은 popup close 시 drop 된다.
- **운영 비용 / 유지 부담**: 새 조회 경로를 추가할 때 `take_repo`/`put_repo` 쌍을 지켜야
  한다. `put_repo` 를 빠뜨리면 성능만 예전으로 돌아가고 오동작하지 않는 방향으로
  설계했지만, 반대로 `self.repo` 에 직접 대입하면 이 안전성이 깨진다.

## Alternatives Considered

- **핸들을 `Rc`/`RefCell` 로 공유**: 단일 스레드라 가능은 하지만, 소유권이 흐려져
  "언제 버려지는가" 가 코드에서 안 보인다. take/put 쌍은 무효화 시점이 타입으로 강제된다.
- **worktree 마다 핸들을 맵으로 캐시**: 전환이 잦으면 이득이지만, 열린 fd 가 worktree
  수만큼 쌓이고 어느 항목이 언제 낡는지 판단이 항목별로 갈린다. 실제 사용 패턴은 한
  worktree 에 머무르며 파일을 클릭하는 쪽이라 활성 하나로 충분했다.
- **Refresh 에서도 캐시를 유지**: open 을 한 번 더 아끼지만, "Refresh 했는데 안 바뀐다"
  는 실패 모드를 감수해야 한다. 낡은 데이터를 보여주지 않는 것이 우선이다.
- **worktree 목록 재수집을 Refresh 에서 생략**: 외부 `git worktree add/remove` 가 영영
  반영되지 않는다. 목록 수집 비용은 위 canonicalize 수정으로 이미 선형이라 남길 이유가
  없었다.
- **`collect_log` ref 맵 캐시**: 위 Decision 대로 낡은 ref pill 위험 대비 이득이 없다.
- **중복 검사 제거**: `canonicalize` 를 통째로 없앨 수 있지만 비표준 레이아웃에서 같은
  worktree 가 두 번 나온다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

- plugin 이 여러 스레드에서 dispatch 되도록 SDK 가 바뀐다 — 핸들을 상태에 두는 전제가
  깨진다.
- git-viewer 가 read-only 를 벗어나 mutate(stage/commit 등) 를 갖는다 — 자기 조작 후의
  무효화 규칙이 새로 필요하다.
- worktree 전환이 잦은 사용 패턴이 확인된다 — 활성 하나 캐시를 맵으로 넓힐 근거가 된다.
- `git2` 가 `Repository` 를 `Sync` 로 만들거나 재열기 비용이 무시할 수준으로 바뀐다 —
  캐시 자체를 걷어낼 수 있다.

## References

- [`plugins/git-viewer/screens/git-viewer.md`](../plugins/git-viewer/screens/git-viewer.md) —
  이 결정이 적용된 조회 경로의 현재 동작
- [ADR-0095](0095-plugin-list-virtualization-and-fixed-content-width.md) — 같은 popup 의
  프레임당 렌더 비용(다른 축)
- [ADR-0028](0028-plugin-egui-mesh-render-channel.md) — plugin egui-mesh 자가 렌더 채널
