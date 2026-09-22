# ADR-0268: 끊긴 mirror markdown 문서는 옛 원문 대신 끊김을 보이고, 재연결은 변경 신호 한 번으로 되돌린다 — ADR-0255 의 항목 5·6 개정

- **Status**: Accepted
- **Date**: 2026-09-13
- **Tags**: markdown, attach, mirror, remote, reconnect, disconnect, surface-kind, deferred-plugin, adr-0255
- **Group**: remote-attach

## Context

[ADR-0255](0255-markdown-attach-mirror-forwards-content-not-pixels.md) 가 mirror 의 markdown
surface 를 원문 조회 채널로 채웠다. 그 채널을 GUI 두 인스턴스의 loopback attach 로 실제로 돌려
보니(2026-09-13) 문서 표시·변경 신호·새로고침은 설계대로 돌았고, 연결 수명의 두 끝에서 결정이
사실과 어긋났다.

**끊김.** anchor 매핑이 있는 세션은 끊겨도 mirror 워크스페이스를 살려 둔 채 `Reconnecting` 으로
들어간다. host 는 그때 markdown leaf 마다 abandon sentinel(`request_id = 0`)을 보내지만, plugin 은
**기다리던 요청이 있을 때만** 그것을 봤다. 원문을 이미 받아 표시 중인 문서 — 끊김의 대부분이 이
경우다 — 는 아무 변화가 없었다. 실측: 서버를 죽이자 client 에는 toast 만 뜨고 문서는 옛 원문 그대로
남았으며 plugin 로그에도 재렌더가 없었다. 끊긴 동안의 원격 변경은 신호가 오지 않으므로, 이 화면은
최신이 아닌데 최신처럼 보인다.

**재연결.** ADR-0255 항목 5 는 "점유가 없어 사라진 신호는 다음 attach 의 핸드셰이크가 최신 상태를
싣고 오므로 쌓아 둘 이유가 없다" 고 적었다. 새 attach 에는 맞지만 **재연결에는 틀리다.**
`reconnect_session` 은 survivor leaf 의 plugin surface 를 `RemoteSurface::share_handles` 로 이어
받으므로 plugin 은 문서를 다시 만들지 않고, 따라서 원문도 다시 요청하지 않는다. 끊긴 사이에 바뀐
문서는 재연결 뒤에도 옛 원문으로 남는다.

**plugin 이 attach 시점에 없을 때.** 항목 6 은 "client registry 에 `markdown` kind 가 없으면 현행대로
`EmptySurface`" 로 정했다. 그런데 kind 가 없는 이유는 둘이다 — plugin 이 **아예 없거나 다른 plugin 이
그 이름을 가졌다**, 또는 **번들 plugin 이 아직 안 떴다**(부팅 때 꺼 두었다가 세션 중에 켜는 경우).
뒤엣것도 같은 빈 surface 가 되고, 그 leaf 는 plugin 이 뜬 뒤에도 영영 빈 채로 남는다. 실측: 부팅 때
plugin 을 끄고 자동 attach 한 뒤 켜도 surface 목록의 type 이 `Empty` 로 남았다. host 의 surface
요청 송신은 plugin 프로세스가 없으면 조용히 버리므로, 뒤늦게 kind 로 다시 만드는 경로가 따로 없는
한 이 상태는 복구되지 않는다.

## Decision

ADR-0255 의 **항목 5 의 재연결 서술과 항목 6** 을 아래로 개정한다.

1. **abandon 은 "연결이 끊겼다" 는 사실이다.** plugin(`apply_remote_result`)은 `request_id = 0` 을
   받으면 기다리던 요청이 있든 없든 문서를 **끊김 상태**로 둔다. 끊김 상태의 렌더는 로딩·실패·원문
   어느 것보다 앞서 끊김 문구 하나를 그린다 — 옛 원문을 함께 두지 않는다. 끊김은 다음 성공 회신이
   지운다. 같은 abandon 이 거듭 와도 다시 그리지 않는다.
2. **재연결은 survivor markdown 문서마다 변경 신호를 한 번 보낸다.** `reconnect_session` 이 세션을
   `Connected` 로 되돌린 직후, 그 세션의 로컬 markdown id 전부에 기존
   `markdown_mirror.changed { surface_id }` 를 보낸다. 새 이벤트 이름은 만들지 않는다.
3. **변경 신호에 대한 plugin 의 반응은 문서가 무엇을 보여 주는가로 갈린다**(`MdDoc::on_remote_changed`).
   - 원문을 보여 주는 중이고 끊기지 않았으며 실패 표시가 없다 → **stale 표시만** 켠다(항목 5 그대로).
   - 원문을 못 보여 주는 중이다(끊김·실패·아직 로드 전)이고 요청이 진행 중이 아니다 → **다시 요청**한다.
   - 요청이 진행 중이다 → 무시한다(그 회신이 곧 온다).
   그래서 2 의 신호 한 번이 "원문을 보던 문서는 stale, 끊김을 보던 문서는 재요청" 을 plugin 한 곳의
   판단으로 해낸다. 1 에 의해 재연결 시점의 survivor 문서는 모두 끊김 상태이므로, 실제로는 재연결
   뒤 전부 다시 받는다.
4. **항목 6 개정 — kind 가 아직 없으면 기다린다.** `merge_survivor_mapping` 은 `role: "markdown"` leaf
   를 만들 때 `markdown` kind 가 **등록돼 있지 않으면** layout 복원과 같은 kind 대기 placeholder
   (`EmptySurface::new_deferred_plugin`)로 만든다. 표시 시점의 reify(`CoreState::reify_plugin_surface`)가
   kind 등록 뒤 registry 의 `restore` 로 실제화하고, 그 복원 data 는 생성 params 와 같은 모양
   `{display_name, remote: {file}}` 이다 — plugin(`restore_surface` → `remote_file_of`)은 `remote` 키로
   mirror 문서임을 알아 로컬 snapshot 복원과 가른다. kind 를 **다른 plugin 이 이미 등록했으면** 기다리지
   않고 `EmptySurface` 로 남는다(reify 는 소유자를 가리지 않아 그 plugin 으로 실제화되기 때문이다).

**개정하지 않는 것**:

- 항목 1(role), 2(조회 채널과 "attach 점유 = 신뢰"), 3(예산), 4(파일 크기 게이트), 7(스코프 밖 셋),
  8(plugin↔host 다리의 이름) 전부.
- 항목 5 중 **원문을 보여 주는 문서는 원격 변경 신호로 다시 받지 않고 stale 표시만 한다**는 결정과 그
  이유(읽던 자리를 말없이 갈아치우지 않는다), 신호원(`webview.set_url`), 수신자 집합, "상위 집합"
  논거. 이 ADR 이 자동 재요청을 여는 것은 **원문을 못 보여 주는 문서**뿐이다 — 거기에는 지킬 읽던
  자리가 없다.
- 점유가 없어 서버에서 사라지는 신호를 쌓아 두지 않는 것. 새 attach 에는 핸드셰이크가 여전히 최신
  상태를 싣고 오고, 재연결은 2 가 메운다.
- anchor 가 없는 세션의 끊김은 mirror 워크스페이스째 정리되고 toast 가 뜬다 — 이 ADR 의 끊김 상태는
  mirror 문서가 살아남는 `Reconnecting` 에만 보인다.

## Consequences

- **얻은 것**: 끊긴 동안 옛 원문이 최신처럼 보이는 상태가 없어진다. 재연결 뒤 사용자가 누르지
  않아도 원문이 돌아온다. attach 시점에 plugin 이 안 떠 있어도 뜬 뒤 문서로 채워진다. host 는
  plugin 의 문서 상태를 모른 채 신호 한 번만 보내면 되고, 판단은 plugin 한 곳에 있다.
- **잃은 것**: 끊김 동안에는 옛 원문조차 읽을 수 없다. 재연결 순간 원문을 보고 있던 사용자의 읽던
  자리는 재요청으로 사라진다 — 다만 1 로 그 순간 화면은 이미 끊김 문구라 읽던 자리가 남아 있지
  않다.
- **운영 비용 / 유지 부담**: `markdown_mirror.changed` 의 의미가 "원격이 다시 그렸다" 에서 "원문이
  바뀌었을 수 있다" 로 넓어졌다 — 보내는 자리가 둘(원격 신호 전달, 재연결)이다. plugin 의 문서 상태가
  `loaded`·`stale`·`pending`·`disconnected`·실패의 조합이 되어 반응 표(3)를 함께 유지해야 한다.

## Alternatives Considered

- **끊김 때 옛 원문을 두고 위에 배너만 얹는다** — 읽던 자리를 지키지만, 끊긴 동안 바뀐 문서를 최신처럼
  계속 읽게 둔다. 이 채널이 막으려던 "바뀐 줄 모른 채 낡은 화면을 보는 것"(항목 5 의 비대칭 논거)이
  그대로 남는다. 재연결 뒤에는 어차피 재요청이 원문을 갈아치우므로 지킨 자리도 오래가지 않는다.
- **재연결 때 host 가 원문을 직접 다시 요청해 결과를 밀어 넣는다** — host 는 plugin 이 어느
  `request_id` 를 기다리는지, 문서가 무엇을 보여 주는지 모른다. 모르는 채 밀어 넣으면 plugin 의 옛
  회신 폐기 규칙과 부딪치고, 판단이 host 와 plugin 두 곳으로 갈린다.
- **재연결 전용 이벤트(`markdown_mirror.reconnected`)를 새로 둔다** — plugin 이 그것을 받아 할 일이
  "원문을 못 보여 주면 다시 받는다" 로 3 의 반응과 같다. 같은 판단에 이름을 둘 두면 한쪽만 고쳐지는
  상태가 표현 가능해진다.
- **항목 6 을 그대로 두고 kind 등록 시점에 빈 leaf 를 찾아 다시 만든다** — registry 등록에 그런 훅이
  없고, "kind 가 뜨면 실제화되는 자리" 는 layout 복원이 이미 쓰는 `DeferredPlugin` 이 그 일을 한다.
  같은 일을 하는 두 번째 경로를 만들 이유가 없다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

**채널이 붙는 것** — 판정 시점에 레포가 읽을 수 있는 사실이다.

- `reconnect_session` 이 survivor leaf 의 plugin surface 를 이어 받지 않고 새로 만들게 된다
  (`RemoteSurface::share_handles` 를 부르지 않는다) — 그러면 plugin 이 문서를 새로 만들며 원문을
  요청하므로 2 의 신호는 중복이 된다.
- abandon 이 `request_id = 0` sentinel 이 아닌 다른 형태로 바뀐다 — 1 의 판정 재료가 사라진다.
- 서버가 끊긴 client 를 위해 점유를 붙들고 놓친 신호를 쌓아 두는 재연결 유예 창구가 생긴다 — 그러면
  재연결 때 놓친 신호를 그대로 받을 수 있어 2 가 필요 없다.

**원리적으로 안 붙는 것** — 사람이 관측해야 한다. 재는 법을 함께 적는다.

- 끊김 동안에도 옛 원문을 읽고 싶다는 사용자 보고가 반복된다. 재는 법: 원격 attach 관련 이슈·피드백에서
  끊김 화면에 대한 불만을 센다.

## References

- 개정 대상: [ADR-0255](0255-markdown-attach-mirror-forwards-content-not-pixels.md) (항목 5 의 재연결 서술, 항목 6)
- 개정 패턴 선례: [ADR-0030](0030-image-egui-mesh-bitmap-texture.md)
- [`docs/dev-guide/attach-behavior.md`](../dev-guide/attach-behavior.md) — markdown 채널의 현재 동작.
- [`docs/features/remote-attach/index.md`](../features/remote-attach/index.md) — 인수 조건과 실측.
- 코드 근거(결정이 실현된 현재 위치): `src/app/attach_client.rs::reconnect_session` ·
  `enter_reconnecting` · `merge_survivor_mapping` · `deferred_mirror_markdown_surface`,
  `crates/tasty-plugin-markdown/src/main.rs::MdDoc::on_remote_changed` · `apply_remote_result` ·
  `restore_surface` · `remote_file_of`,
  `crates/tasty-plugin-markdown/src/render.rs::render_document`,
  `crates/tasty-model/src/empty_surface.rs::EmptySurface::new_deferred_plugin`,
  `src/core/state/pty.rs::reify_plugin_surface`.
