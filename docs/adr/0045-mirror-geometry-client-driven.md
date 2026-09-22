# ADR-0045: mirror grid geometry 는 client 가 구동하고 remote 는 reflow 메커니즘으로 확정한다

- **Status**: Accepted
- **Date**: 2026-07-11
- **Tags**: attach, remote, mirror, geometry, resize, protocol, client-driven, backward-compat, headless, adr-0007, adr-0040
- **Group**: remote-attach

## Context

원격 attach 로 headless 서버의 워크스페이스를 로컬 GUI 에 mirror 로 붙이면, mirror 가 로컬 pane 크기(예: ~200×55)를 쓰지 않고 **원격 기본 grid 80×24 로 pane 좌상단에 작게** 그려지고 나머지는 배경 레터박스로 남았다. 사용자가 실사용 후 "불편하다, 방향을 바꿔야 한다"(2026-07-11)고 확정했다.

이 동작은 우연이 아니라 **의도된 설계**였다: mirror grid 를 **원격이 authoritative** 로 두고, 로컬 창/pane 크기가 mirror 에 절대 반영되지 않게 했다. 프로토콜(`StreamControl::Resize` 의 "local window/pane size never drives a mirror")·dev-guide·features 문서에 그렇게 명시돼 있었다. 근거는 desync 회피였다: 원격 PTY 는 자기 grid 로 콘텐츠를 래핑(reflow)하므로, 로컬이 mirror grid 를 마음대로 바꾸면 줄바꿈·커서가 어긋난다.

제약:
- 원격 PTY 의 reflow 는 **원격 grid 에 종속**이다 — 로컬이 grid 만 바꾸고 원격을 그대로 두면 콘텐츠가 깨진다.
- attach 점유는 **배타적**(ADR-0040 hard 점유): 한 워크스페이스의 holder 는 항상 정확히 1명이라, geometry 를 구동하는 client 도 유일하다(다중 구동 충돌 없음).
- 기존 attach 세션과의 **하위호환**: 프로토콜 variant 추가가 구버전 상대를 깨면 안 된다.
- **크로스플랫폼/headless**: 서버 수신 로직은 `--no-default-features`(headless)에서도 컴파일·동작해야 한다.

## Decision

mirror grid geometry 를 **client-driven** 으로 뒤집는다: mirror 를 띄운 **로컬 pane 의 크기가 grid 를 정하고**, 원격 PTY 를 그 크기로 reflow 시킨다. 단 "remote authoritative" 를 **메커니즘으로는 유지**한다 — 원격 PTY 가 실제 크기의 단일 진실원(reflow 담당)이고, 그 settled 크기를 client 에 되돌린다. 즉 **의도(intent)는 client 가 구동, 확정(confirm)은 remote 가 echo** 하는 요청→확정 협상이다.

구현은 기존 인프라의 방향만 바꾼다:
- 새 프로토콜 variant `StreamControl::ClientResize{surface_id, cols, rows}`(client→server) 하나를 추가한다. 기존 client→server `StructuralOp` 채널과 동형이다.
- client 의 로컬 레이아웃 리사이즈 스윕은 detached(mirror) 터미널을 **skip 하던 것을**, 목표 grid 를 로컬에 적용하지 않고 **forward 큐에 넣는 것으로** 교체한다.
- 서버는 두 drain(gui `event_handler` / headless `boot`)에서 holder 를 검증한 뒤 원격 **실제 PTY** 를 `Terminal::resize` 한다.
- 로컬 mirror grid 는 **server 의 `Resize` echo 로만 갱신**한다(낙관적 로컬 적용 금지). 이 echo 경로(resize tap → forwarder → reader → `t.resize`)는 **무변경 재사용**이다.
- 서버가 **GUI 인스턴스**(창 보유)로 그 워크스페이스를 렌더 중이면, host 창의 레이아웃 리사이즈 sweep(`Core::resize_all_terminals`)이 **hard-점유된 surface 를 skip** 한다(`OccupancyRegistry::is_hard_occupied`). 이 skip 이 없으면 host 창이 매 sweep 마다 점유 surface 를 자기 창 grid 로 되돌려 client 의 `ClientResize` 를 무력화하고, mirror 가 host 창 크기에 고정된다(레터박스). 점유 중 그 surface 의 grid 는 **오직 `apply_attached_workspace_resize`(holder 검증 후 client 요청 크기 적용)** 만 설정하며, detach 로 lock 이 풀리면 다음 sweep 부터 host 창이 다시 구동한다(원복). headless 서버는 창이 없어 이 sweep 자체가 없으므로 무해하지만, **GUI-hosted 서버**(원격 tasty GUI 를 attach)에선 필수 불변식이다 — 초기 설계 Context 가 headless 서버만 상정해 이 케이스를 놓쳤다.
- **단일 authority (필수)**: 이 skip 이 유효하려면 터미널 grid 리사이즈 sweep 이 **`Core::resize_all_terminals` 한 곳뿐이어야** 한다. 과거 View 레이어에 중복 sweep(`AppState::resize_all` 자체 구현)이 남아 gpu 렌더 경로에서 매 프레임 돌며 occupancy 가드 없이 점유 surface 를 창 grid 로 되돌려, 정본의 skip 을 무력화했다(레터박스 재발). 중복 sweep 은 정본으로 위임(delegation)해 제거했다 — `AppState::resize_all` 은 이제 `Core::resize_all_terminals` 로 forward 하는 얇은 진입점이다. detached-forward·occupancy-skip 같은 attach 의미론이 정본 한 벌에만 존재해야 divergence 로 인한 회귀가 없다.

## Consequences

- **얻은 것**: mirror 가 로컬 pane 을 채운다(레터박스 제거). 원격 grid 가 로컬 크기를 따라가 `stty size`·앱 레이아웃이 실제 보이는 영역과 일치한다. 신규 코드는 작다(대부분 기존 forward/echo 인프라 복제·재사용).
- **잃은 것**: echo 왕복(약 1 RTT) 동안 mirror grid 가 즉시 반응하지 않는다. LAN 에선 무시할 수준이나 고지연 링크에선 체감될 수 있다. 초기 attach 순간(80×24 → 첫 forward reflow)에 짧은 깜빡임이 남는다.
- **운영 비용 / 유지 부담**: 프로토콜 variant 1 개 + 서버 수신 분류/적용 + client forward 큐. desync 를 막는 불변식("로컬 grid 는 echo 로만 갱신")을 유지해야 한다 — 낙관적 로컬 적용을 넣으면 이 ADR 의 전제가 깨진다. resize 폭주는 client 측 last-forwarded dedup + 서버측 `resize_grid=false` no-op 2 중으로 흡수한다.
- **불변식(GUI-hosted 서버)**: 점유 중인 surface 는 host 창 sweep(`resize_all_terminals`)이 skip 해야 한다 — 이 skip 을 빼면 host 창이 client-driven grid 를 되돌려 이 ADR 이 무효화된다. 부수 효과로 점유 중 GUI-host 의 로컬 창은 occupant 의 grid 를 따라 렌더되며(host 창이 더 크면 host 쪽에 레터박스), 이는 host 사용자가 점유 대상에 readonly 라는 ADR-0040 점유 의미와 정합한다.

## Alternatives Considered

- **remote-authoritative 유지(기존 설계)**: 로컬은 원격 grid 를 그대로 그리고 레터박스를 감수. — 사용자 실사용에서 불편이 확인돼 기각. mirror 가 pane 을 못 채우는 것이 핵심 불만이었다.
- **낙관적 로컬 적용(client 가 로컬 grid 를 먼저 바꾸고 원격에 통지)**: 반응성은 좋으나 원격 reflow 전 바이트가 잘못된 grid 에 재생돼 줄바꿈·커서가 어긋나는 desync 를 유발. 기존 설계가 경계한 바로 그 문제라 기각. echo-driven 을 기본으로 둔다.
- **멀티클라이언트 geometry 충돌 정책 도입**: 배타 점유(ADR-0040)라 holder 가 항상 1명 → 충돌이 원천적으로 불가능. 불필요해서 도입하지 않음.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

- 고지연/원거리 링크에서 echo 왕복 지연이 실사용에 체감돼 반응성이 문제가 된다 → 낙관적 로컬 적용(+ 재동기화 프로토콜) 도입을 재검토.
- attach 점유가 **비배타(멀티 holder)** 로 바뀐다 → geometry 를 누가 구동하는지의 충돌 정책이 필요해진다.
- 초기 attach 깜빡임이 문제가 되어 handshake 단계에서 client pane 크기를 먼저 협상하는 방식이 요구된다.

## References

- dev-guide: [`attach-behavior.md`](../dev-guide/attach-behavior.md) "리사이즈 전파 (mirror geometry)"
- features: [`remote-attach/index.md`](../features/remote-attach/index.md) "화면 동기화"
- 관련 ADR: [`0007`](0007-attach-targets-remote.md)(attach 대상=원격), [`0040`](0040-occupancy-soft-hard-tiers-agent-occupant.md)(hard 점유 = mutate/geometry 구동 권한)
- 프로토콜: `crates/tasty-ipc/src/stream.rs`(`StreamControl::ClientResize` / `Resize`)
