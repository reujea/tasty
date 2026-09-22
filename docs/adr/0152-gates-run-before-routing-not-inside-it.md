# ADR-0152: 게이트는 라우팅보다 먼저 돈다 — 조기 응답이 검사 자리를 건너뛴다

- **Status**: Accepted
- **Date**: 2026-09-05
- **Tags**: security, permissions, ipc, plugin, routing, guards, telemetry, audit
- **Group**: permission-security

## Context

권한 게이트가 라우터 **안쪽**(`handle_with_caller`)에만 있었다. 그런데 GUI 앱의
`App::dispatch_with_caller` 는 거기 도달하기 전에 끝나는 경로를 여럿 갖는다 —
`dispatch_list_global` 의 list 합산 응답, owner 해석 실패, 지목한 대상 없음, app state
없음. 그중 첫째가 `surface.list` · `workspace.list` · `pane.list` 를 게이트 앞에서 답했다.

셋 다 `plugin(&[SurfaceRead])` 로 선언돼 있고 [plugin-permissions](../dev-guide/plugin-permissions.md)
의 `surface.read` 행이 그것들을 싣는다. 표와 문서는 서로 맞았고 **집행만 안 됐다.** 표가
이름 수준에서 일치한다는 것과 그 권한이 실제로 걸린다는 것은 다른 명제다.

**진단을 확정한 것은 같은 회차 안의 대조다.** 권한을 하나도 선언하지 않은 번들 plugin 이
다섯 개를 연달아 불렀을 때, 같은 `surface.read` 를 요구하는 두 메서드가 서로 다르게
답했다.

    surface.list       -> OK   전체 surface 트리
    workspace.list     -> OK   전체 workspace 목록
    pane.list          -> OK   전체 pane 목록
    tab.list           -> ERR  permission_denied: missing 'surface.read'
    clipboard.set_text -> ERR  permission_denied: missing 'clipboard.write'

`tab.list` 은 **`surface.list` 과 같은 권한**을 요구하면서 거부됐다. 검사 로직이 없거나
망가졌다면 둘 다 통과했어야 한다. 하나만 새는 것은 **게이트가 없었다는 뜻이 아니라 게이트가
있는데 자리가 틀렸다는 뜻**이고, 결정이 아래 A 가 아니라 본 결정인 이유가 여기서 나온다.

**노출된 것**: 전체 workspace · pane · surface 트리에 각 surface 의 `foreground_pid`,
전경 프로세스 이름, 치수가 들어 있다. 권한 선언이 빈 plugin 이 사용자가 무엇을 어디서
돌리고 있는지를 읽을 수 있었다. 정보 노출이다.

건너뛰는 것은 권한만이 아니었다. 같은 자리에서 telemetry cap 과 rate limit 도 넘어가고
audit 기록도 남지 않았다 — 그 회차의 audit 은 거부된 둘만 1 행씩 갖고 샌 셋은 0 행이었다.
cap 이 Pause 로 모든 IPC 를 막은 plugin 도 이 셋은 통과한다는 뜻이다.

헤드리스에는 이 구멍이 없다. `headless_dispatch.rs` · `headless_plugins.rs` 는
`handle_with_caller` 직결이라 게이트를 탄다. GUI 앱 경로 전용이었다.

## Decision

**게이트 3종(권한 / telemetry cap / rate limit)을 `App::dispatch_with_caller` 진입부로
올린다.** 라우팅보다 먼저 돌므로, 그 아래에서 어떤 형태로 조기에 응답하든 검사를
건너뛸 수 없다.

라우터 안쪽 게이트는 **남긴다.** `handle_with_caller` 를 직접 부르는 진입점(headless ·
attach · routing)이 따로 있어 그쪽은 안쪽 게이트가 지킨다. 중복이 아니라 **경계가 둘**
이고, 거부는 바깥에서 단락되므로 안쪽이 다시 돌지 않는다. 게이트는 부수효과 없는 술어라
두 번 돌아도 판정이 같지만, **기록은 부수효과라 같은 축이 아니다** — 거부 한 건이 audit
행을 1 개 남기는지 2 개 남기는지를 따로 실측해 1 임을 확인했다.

`record_telemetry_and_audit`(Allow 기록)은 옮기지 않는다. 같은 이유에서다.

계약은 가드 `every_routing_entry_gates_before_it_answers` 가 소유한다. 판정은 **구문**이다
— caller 를 받아 라우팅하는 함수의 본문에서 게이트 호출 위치와 첫 `return` 위치를 비교하고,
게이트 결과가 곧바로 반환되는지까지 본다. 이름을 나열하지 않으므로 그런 함수가 새로
생기면 등재 없이 자동으로 대상이 된다.

## Consequences

- **얻은 것**: 조기 응답이라는 **부류 전체**가 검사를 건너뛸 수 없게 된다. 권한 · cap ·
  rate 셋이 한 자리에서 복구된다.
- **잃은 것**: 게이트가 두 번 도는 경로가 생긴다(통과 시). 술어라 부수효과가 없고 비용은
  해시셋 조회 수준이다.
- **plugin 진입부에서 얻은 것**(후속 트랙): 인터셉트 갈래의 거부가 audit 에 남는다 —
  실측으로 같은 회차의 audit 이 `total 1` 에서 `total 3` 이 됐고, 새로 막힌 둘이 1 행씩
  중복 없이 늘었다. 같은 회차 안의 반대편 통제군도 있다: `ui.popup` 을 가진 `popup.close`
  는 게이트를 통과해 소유권 검증에서 떨어진다(막히는 것만 세면 "전부 막았다" 와 구별되지
  않는다).
- **남는 것**: 조기 응답 경로에는 Allow 텔레메트리가 기록되지 않는다.
- **운영 비용 / 유지 부담**: 가드가 구문 판정이라 유지할 목록이 없다. 면제는 하나이고
  그 근거를 테스트가 다시 검사한다.

### 측정 (2026-09-05 시점의 과거형 사실)

수정 전 audit `total 2 / deny 2 / allow 0`, 수정 후 `total 5 / deny 5 / allow 0` — 새로
막힌 3 건이 정확히 3 행 늘었다(메서드당 1 행, 중복 없음). 이 수는 메서드 구성이 바뀌면
낡는다. **다시 재는 방법**은 권한을 선언하지 않은 번들 plugin 으로 위 다섯 개를 부르고
`plugin audit-summary` 로 세는 것이다. 함정 셋:

- 격리된 `TASTY_HOME` 으로 인스턴스를 띄운다. 안 그러면 개발용 상태와 섞인다.
- GUI 는 `xvfb-run -a` 로 띄운다.
- CLI 는 부모 터미널의 `TASTY_SESSION_TOKEN` · `TASTY_SURFACE_ID` · `TASTY_AGENT_ID` ·
  `TASTY_PARENT_HOME` 를 상속하면 안 된다 — 상속하면 격리 인스턴스에 대해
  `session_token unknown/expired/revoked` 로 떨어지고, 그것을 결함으로 오독하게 된다.

### plugin 진입부 (후속 트랙에서 확정)

위 대안 B 를 실행했다. 실행으로 먼저 갈랐다 — 권한을 하나도 선언하지 않은 번들 plugin 으로
진입부의 갈래 다섯을 한 회차에 부른 결과, 넷은 실제로 막혔고 하나는 안 막혔다. **소스에서
"자기 게이트를 갖고 있다" 와 실행에서 "막힌다" 는 다른 명제라, 재기 전에는 넷이 안전한지도
확정되지 않았다.**

그런데 막히던 넷도 막히는 축이 하나뿐이었다. 그 넷은 `ensure_allowed` 만 부르고 telemetry
cap · rate limit · audit 를 타지 않아, **거부가 기록되지 않았고** cap 이 Pause 로 모든 IPC 를
막은 plugin 도 그 넷은 통과했다. 그래서 게이트 3종을 갈래 분기보다 먼저 돌리고, 인터셉트
핸들러는 자기 권한 검사를 버리고 소유권 검증만 남긴다.

막히지 않던 하나는 `host.shared_buffer.create` 였다. 게이트가 약한 것이 아니라 **`METHOD_TABLE`
에 항목 자체가 없었다** — 표에 없으면 표를 읽는 어떤 감사도 그 메서드를 보지 못하고, 게이트도
이름을 못 찾아 태울 수 없다. 표의 여집합을 세어 보니 그런 메서드는 그 하나뿐이었으므로(특례),
**현재 동작 그대로(요구 토큰 없음) 표에 등재한다.** 등재만으로 권한 이외의 두 축이 살아난다 —
cap 과 rate limit 이 걸리고 호출이 audit 에 남는다. 등재는 누가 부를 수 있는지를 바꾸지 않으므로
매니페스트 호환성 결정이 아니다.

그 결과 진입부에는 **면제가 하나도 없다.** 면제 목록을 남기지 않는 이유는 목록이 곧 다음
사람이 이름만 얹고 지나가는 자리가 되기 때문이다.

## 이 ADR 이 안 정한 것

**`host.shared_buffer.create` 가 어떤 권한을 요구해야 하는가.** 지금 요구 토큰이 없는 것은
그렇게 정해서가 아니라 **정한 적이 없어서**다 — 정책이 아니라 미결이고,
[plugin-permissions](../dev-guide/plugin-permissions.md) 의 "토큰 없이도 열려 있는 메서드" 표에서
앞의 두 군과 갈라 셋째 군으로 적어 두었다.

미결로 남기는 근거는 폭발 반경을 재 봤기 때문이다. 번들 plugin 아홉 중 **다섯**이
`EguiMeshSurface` / `EguiMeshPopup` / `EguiMeshBanner` 를 통해 이 메서드에 도달하고, 그 다섯이
공통으로 선언한 기존 권한 토큰은 **하나도 없다.** 즉 기존 variant 아무거나 골라 거는 것은
반드시 번들 plugin 을 끊는다. SDK 의 `HostHandle::create_shared_buffer` 가 public 이라 외부
plugin 도 같은 통로를 쓴다.

함께 열려 있는 질문이 하나 더 있다. 지금 있는 상한은 **호출당 1 GiB** 하나뿐이고 개수 상한도
총량 상한도 없다 — 회수는 surface/popup/banner 가 닫힐 때만 돌아서, 아무 UI 에도 붙지 않은
버퍼는 plugin 수명 내내 남는다. 실측으로 16 MiB × 64 회가 전부 성공했고(멈춘 이유는 프로브가
64 에서 끊었기 때문이다) 호스트에 memfd 67 개가 열린 채였다. 그래서 결정해야 할 것은 "어떤
토큰인가" 와 "개수·총량 상한을 함께 둘 것인가" 두 개다. 등재로 rate limit 경로가 열렸으므로
급한 쪽은 막혔지만, rate limit 은 운영자가 버킷을 걸었을 때만 동작한다(미등록 = 무제한 허용).

**남은 잔차**: namespace forward 의 거부는 여전히 audit 에 남지 않는다. 그 판정은
`validate_namespace_call` 안에 있고 audit 배선이 거기까지 닿지 않는다 — pre-gate 는 그 메서드를
`required=[]` 로 보고 통과시키므로 거부를 볼 자리가 아니다.

## Alternatives Considered

- **A: `dispatch_list_global` 에만 caller 를 넘겨 게이트를 태운다** — 기각. 그 인스턴스
  하나만 막고 **cap · rate · audit 는 여전히 스킵**된다. 같은 결함을 세 축 남기는 셈이고,
  다음 조기 응답이 추가되면 아무것도 해주지 않는다. 보안 수정의 기준은 *그 인스턴스가
  아니라 그 부류를 불가능하게* 만드는 것이다.
- **B: plugin 진입부(`process_plugin_ipc_calls`)에 agent 와 대칭인 pre-gate 를 둔다** —
  기각이 아니라 **후속이었고, 아래 "plugin 진입부 (후속 트랙에서 확정)" 에서 실행했다.**

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

- 게이트가 두 번 도는 경로의 비용이 실측으로 드러날 때 — 그때는 안쪽 게이트의 범위를
  다시 가른다.
- `plugin_ipc.rs` 의 인터셉트 갈래들이 갖는 자기 게이트가 바뀔 때.
- `host.shared_buffer.create` 의 요구 권한이 정해질 때 — 위 "이 ADR 이 안 정한 것" 이
  닫히고, 그 시점에 셋째 군은 문서에서 사라진다.
- 표의 여집합이 다시 1 을 넘을 때 — 하나면 특례라 등재로 닫히지만 여럿이면 구조이고,
  그때는 "왜 등재하지 않는가" 를 따로 결정해야 한다.

## References

- [plugin-permissions](../dev-guide/plugin-permissions.md) — 권한 토큰별 메서드 목록
- [ADR-0141](0141-host-key-namespace-is-reserved-in-raw-memory-kv.md) — 같은 부류(메서드
  단위로는 일관되는데 조합에서 새는 것)의 다른 인스턴스
- `src/app/dispatch/intents.rs` — `gates_before_routing` 과 가드
- `src/adapters/ipc/handler.rs` — 안쪽 게이트 3종
- `src/app/ipc/caller_gate.rs` — agent 경로의 대칭 지점

부분 개정: [0277](0277-ipc-admission-and-observation-run-once.md) (중첩 게이트·Allow 기록 위치 개정).
