# ADR-0297: e2e 하네스는 디스플레이를 격리하지 않고 이름을 요구한다

- **Status**: Accepted
- **Date**: 2026-09-20
- **Tags**: testing, e2e, harness, isolation, linux, adr-0127, adr-0090
- **Group**: testing

## Context

`tests/common/mod.rs` 와 `tests/webhook_common/mod.rs` 는 자식 인스턴스를 여러 축으로
격리한다 — `HOME` · `TASTY_HOME` · `ZDOTDIR` · 포트 파일(`--port-file` 로 명시 전달) ·
`TASTY_SURFACE_ID` 제거. 그 축을 전수로 세면 **디스플레이만 비어 있었다**: 두 하네스
어디에도 `DISPLAY` 라는 문자열이 없었고, 그래서 `Command` 가 부모의 값을 그대로
물려주었다.

그 상태가 무엇을 바꾸는지를 값으로 쟀다(2026-09-20, 이 개발 박스).

- **물려받는다.** 전용 Xvfb `:77` 위에서 `shared_instance_harness` 를 돌리고 자식
  `/proc/<pid>/environ` 을 읽으면 `DISPLAY=:77` 이다. 부모가 준 값 그대로다.
- **창이 실제로 뜬다.** 같은 완주 중 그 디스플레이의 창 목록에
  `0x200002 "Tasty (Debug)" 1280x720+0+0` 이 있었다. 부모가 사람이 보는 화면이면
  창은 거기 뜬다.
- **홈 격리가 그것을 막지 못한다.** 격리 `HOME` 에서는 `~/.Xauthority` 가 안 보이지만,
  X 서버가 로컬 사용자를 인증하면(`SI:localuser:`) 쿠키 없이 붙는다 — 격리 홈으로
  `xdpyinfo` 를 돌려 확인했다.
- **비용은 이미 이 레포가 쟀다.** 실제 GPU·컴포지터가 붙은 X 서버를 여러 인스턴스가
  공유하면 `surface.list` 한 왕복의 최악값이 5.79 s 까지 늘어난다
  ([e2e-tests.md](../dev-guide/e2e-tests.md) §0-1 의 표). 전용 Xvfb 에서는 같은 부하에서
  그 정체가 사라진다.

그러면 격리하면 되는가 — **안 된다.** 같은 날 같은 바이너리를 `DISPLAY` 와
`WAYLAND_DISPLAY` 없이 띄우니 winit 이
`neither WAYLAND_DISPLAY nor WAYLAND_SOCKET nor DISPLAY is set` 로 즉사했고 port file 이
안 써졌다. gui 조합에서 디스플레이는 **격리해야 할 누수가 아니라 필요한 입력**이다.
다른 축과 성질이 다르다: `HOME` 은 지워도 자식이 자기 것을 만들지만, 디스플레이는
지우면 부팅이 없다.

## Decision

디스플레이를 격리하지 않는다. 대신 **누가 그것을 정했는지를 값으로 남기게** 한다.

`spawn_diag::apply_display_policy` 가 두 하네스의 spawn 직전에 `TASTY_E2E_DISPLAY` 를
읽는다. 값이 디스플레이 이름이면 자식의 `DISPLAY` 로 **명시 전달**하고 `WAYLAND_DISPLAY`
를 함께 지운다(두 축이 같이 있으면 winit 의 백엔드 선택이 결과를 정해, 지정한 값이
실제로 쓰였는지를 말할 수 없다). 값이 `inherit` 이면 오늘 동작 그대로 물려받는다.
**값이 없으면 spawn 을 세우고**, 실패 문구가 두 길(전용 디스플레이를 만드는 법 ·
자기 화면을 선언하는 법)을 다 찍는다.

요구는 세 조건의 곱일 때만 선다 — `target_os = "linux"`(디스플레이를 환경변수로 고르는
플랫폼) · `feature = "gui"`(창을 만드는 데몬) · `TASTY_E2E_BIN` override 가 안 먹히는
스위트(override 의 용도는 미리 지어 둔 헤드리스 바이너리다). 그래서 `check-headless` 가
돌리는 헤드리스 전체 스위트는 이 변수를 요구받지 않는다.

## Consequences

- **얻은 것**: 그 완주가 **어느 디스플레이에서 돌았는지**가 값으로 남는다. 격리의 한 축이
  비어 있어 "그 축이 원인인지 아닌지를 아무도 못 가르던" 상태가 닫힌다. 사람이 보는
  화면을 조용히 점거하는 일도 같이 사라진다.
- **잃은 것**: gui 조합의 로컬 완주에 환경변수 하나가 더 필요하다. 모르고 돌리면 인스턴스를
  띄우는 스위트가 전부 그 자리에서 멈춘다 — 그것이 이 결정의 의도된 비용이다.
- **운영 비용 / 유지 부담**: 자동 채널에는 부담이 없다(`check-headless` 는 헤드리스 조합이라
  요구가 안 선다). gui e2e 를 돌리는 관측 스텝 하나가 `xvfb-run` 이 만든 디스플레이를 그
  변수로 넘기도록 한 줄 바뀐다.
- **선언된 사각**: `TASTY_E2E_BIN` 이 **gui** 바이너리를 가리키면 창이 뜨는데 이 판정은
  요구하지 않는다. 경로만 보고 그 바이너리의 feature 조합을 알 방법이 없어서다. 그 경우는
  변경 전 동작(조용한 상속)으로 남는다.

## Alternatives Considered

- **A: `spawn()` 이 `DISPLAY` 를 제거한다** — 가장 싸 보였고 티켓의 첫 후보였다.
  실측으로 탈락했다: gui 조합의 데몬이 부팅하지 못해 이 하네스를 쓰는 스위트가 전부
  죽는다(위 Context 의 winit 즉사).
- **B: 하네스가 자기 Xvfb 를 띄운다** — 격리는 완전해지지만 세 값을 잃는다. 하네스는
  세 OS 에서 도는데 Xvfb 는 linux 전용이고, CI 는 이미 `xvfb-run` 으로 감싸 이중이 되며,
  X 서버의 생명주기(고아 회수)가 하네스 안으로 들어온다.
- **C: 문서에만 적는다** — 이 레포에는 같은 형태의 전례가 있다. 번들 plugin 선행 빌드
  전제가 세 자리에 글로 적혀 있었는데도 같은 실패가 반복됐고, 그것을 닫은 것은 네 번째
  글이 아니라 실패 자리에 붙은 판정이었다([e2e-tests.md](../dev-guide/e2e-tests.md) §0).
  글은 실패하는 순간에 읽히지 않는다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

**채널이 붙는 것** — 판정 시점에 레포가 읽을 수 있는 사실이다.

- `spawn_diag::display_required` 의 세 조건 중 하나가 사라진다 — 예를 들어 루트 패키지가
  `gui` feature 를 잃거나(헤드리스가 유일 조합이 되거나), 모든 스위트가
  `HEADLESS_OK_SUITES` 에 들어가 gui 데몬을 띄우는 자리가 0 이 된다. 그러면 요구가 설
  자리가 없다.
- 하네스가 창 없이 뜨는 진입점을 얻는다 — gui 빌드의 `--headless` 가 오늘은 경고만 내고
  GUI 로 폴백하지만(`src/boot.rs`), 그것이 실제 헤드리스 부팅이 되면 이 결정 대신
  "하네스가 그 플래그를 준다" 가 더 싸다.

**원리적으로 안 붙는 것** — 사람이 관측해야 한다. 재는 법을 함께 적는다.

- `TASTY_E2E_DISPLAY=inherit` 를 셸 rc 에 박아 두는 관행이 퍼져 게이트가 사실상 무력해지는
  것. 그러면 이 결정은 값을 안 남기고 마찰만 남긴다. 재는 법: gui 조합 e2e 완주 중
  자식 pid 의 `DISPLAY` 를 `tr '\0' '\n' < /proc/<pid>/environ | grep '^DISPLAY='` 로 읽어
  그것이 사람이 쓰는 화면인지 본다.

## References

- 절차와 값: [docs/dev-guide/e2e-tests.md](../dev-guide/e2e-tests.md)
- 하네스 바이너리 선택(같은 자리의 형제 결정): [ADR-0127](0127-e2e-harness-binary-selection.md)
- 인스턴스 공유·격리 원칙: [ADR-0090](0090-test-isolation-by-workspace-not-process.md)
- 코드 근거(결정이 실현된 현재 위치): `tests/spawn_diag/mod.rs` 의 `DISPLAY_ENV` ·
  `display_policy` · `display_required` · `apply_display_policy`
