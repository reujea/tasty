# ADR-0527: plugin 은 외부 링크를 host 를 거쳐 연다 — ADR-0511 의 "plugin 쪽 열기는 스위치 밖" 조항 개정

- **Status**: Accepted
- **Date**: 2026-09-23
- **Tags**: plugin, markdown, webview, os-open, browser, debug, verification, user-agent-separation, identity, adr-0511
- **Group**: bundled-plugins

## Context

번들 markdown plugin 은 문서 안의 외부 링크(`http(s)` · `mailto:` · `data:`)가 클릭되면 자기
프로세스에서 `webbrowser::open` 을 직접 불렀다. plugin 은 `tasty-platform` 을 링크하지 않으므로
[ADR-0511](0511-os-open-is-recorded-not-launched-under-a-debug-switch.md) 의 debug 스위치
(`TASTY_DEBUG_OS_OPEN_LOG`)가 이 열기에 닿지 않았다. e2e 하네스와 검증 절차는 가짜 `BROWSER` 로
그 자리를 막았지만, `webbrowser` 가 `BROWSER` 를 읽는 것은 Linux·BSD 뿐이라 macOS·Windows 의 검증
인스턴스에서 링크를 누르면 실행자의 기본 브라우저가 열린다 — 검증(에이전트 행동)의 부수효과가
사용자 상태에 닿는 형태다([정체성 원칙](../identity.md) 1).

사용자가 문서 안 링크를 눌렀을 때 확인 없이 기본 브라우저가 열리는 동작은 사용자가 이미 의존하는
동작이다.

## Decision

**plugin 은 외부 링크를 host 의 `webview.open_external { surface_id, url }` 로 보내고, host 가 자기 OS
열기 자리(`terminal_link::open_uri`)로 연다.** plugin 크레이트는 OS 열기를 직접 하지 않는다.

- 메서드는 `plugin_only(Mutate, &[SurfaceWrite])` 다. 외부 IPC 호출자에게는 arm 이 없다 — 사용자
  브라우저를 여는 것은 에이전트가 자기 작업에 쓰는 능력이 아니다.
- host 는 셋이 맞을 때만 연다: `surface_id` 가 살아 있는 창의 plugin surface 이고 그 소유 plugin 이
  호출자이며, URL 에 스킴(`https:` · `mailto:` 등, 두 글자 이상)이 있고 `javascript:` 가 아니다.
  스킴 없는 문자열은 파일 경로라 `file_handler.dispatch` 의 일이다. 거절은 `-32602`, OS 가 열기를
  거절한 것은 오류가 아니라 응답 `{ "opened": false }` 다.
- 확인 단계는 넣지 않는다. 클릭 → 기본 브라우저의 사용자 동작은 그대로다.

이것이 ADR-0511 의 "plugin 프로세스의 열기(번들 markdown plugin 의 `webbrowser::open`)는 이 스위치
밖이다" 조항을 개정한다 — markdown 링크 열기는 이제 스위치 안이고 `open_uri\t<url>` 로 기록된다.

**개정하지 않는 것**

- ADR-0511 의 스위치 자체: 변수 `TASTY_DEBUG_OS_OPEN_LOG` · 기록 형식 `<via>\t<대상>` · 가로채면
  성공으로 돌아가는 것 · 변수가 있기만 하면 억제하는 fail closed · opt-in 인 것.
- 판정 자리가 `crates/tasty-platform/src/debug_os_open.rs` 의 `intercepted` 하나이고 release 에 없는 것,
  그리고 그것을 묻는 host 호출부 넷.
- e2e 하네스가 격리 홈 아래 `os-open.log` 로 스위치를 늘 켜는 것과 가짜 `BROWSER` · 검증 절차의 가짜
  `PATH`/`BROWSER` — 이제 번들 markdown 몫은 아니지만 PTY 셸과 제3자 plugin 몫으로 그대로 남는다.

## Consequences

- **얻은 것**: markdown 링크 열기가 플랫폼과 무관하게 debug 스위치로 기록된다. OS 열기 자리가 host
  한 곳으로 모여, 무엇이 열리는지를 host 가 본다(소유 surface · 스킴 검사). markdown plugin 이
  `webbrowser` 의존을 잃었다.
- **잃은 것**: 링크 열기가 host 왕복 하나를 탄다(`host.call` 동기 대기). plugin 이 host 없이 링크를
  여는 길은 없다.
- **운영 비용 / 유지 부담**: 제3자 plugin 은 여전히 자기 프로세스에서 OS 열기를 부를 수 있다 —
  host 가 막을 수 없는 자리이고, 검증 절차의 가짜 `BROWSER`/`PATH` 가 그쪽 몫으로 남는다. 번들
  plugin 에 OS 열기 의존이 다시 들어오는 것을 잡는 가드는 두지 않았다(아래 대안).

## Alternatives Considered

- **plugin SDK 가 같은 debug 스위치를 읽게 한다** — plugin SDK 에 debug 분기를 들이고, 열기 자리는
  plugin 마다 흩어진 채로 남는다. host 가 무엇이 열리는지 못 본다.
- **클릭 시 확인 팝업을 띄운다** — 사용자가 의존하는 한 번 클릭 동작이 바뀐다. 이 결함은 검증
  인스턴스의 부수효과이지 사용자 클릭의 위험이 아니다. 호환을 깬다.
- **설정으로 노출한다(열기 · 확인 · 차단)** — 결함의 해결에 필요하지 않은 새 설정 표면이다.
- **`webview.open_external` 을 외부 IPC 에도 연다** — 에이전트가 사용자 브라우저를 여는 능력을
  만드는 일이다(원칙 1). 에이전트가 URL 을 열 필요는 이 ADR 의 과제가 아니다.
- **번들 plugin 크레이트에 OS 열기 의존(`webbrowser` 등)이 없음을 보는 가드** — 이번에는 두지 않았다.
  자리가 하나였고 이제 0 이며, 의존 추가는 리뷰에서 보인다. 아래 트리거가 붙으면 다시 본다.

## Reconsideration Triggers

**채널이 붙는 것**

- 번들 plugin 크레이트가 OS 열기를 다시 부른다. 재는 법: `git grep -nE 'webbrowser|open::that|opener::' -- 'crates/tasty-plugin-*/src' 'crates/tasty-plugin-*/Cargo.toml'`
  (에셋 `.js` 는 뺀다). 한 건이라도 생기면 가드를 만든다.
- 에이전트가 URL 을 사용자 브라우저로 열어야 하는 기능 요구가 생긴다 — 그때는 외부 IPC 표면을
  새 결정으로 다룬다.

**원리적으로 안 붙는 것**

- 제3자 plugin 의 자체 OS 열기가 검증 사고를 낸다. 재는 법: 검증 보고의 부수효과 절.

## References

- 개정 대상: [ADR-0511](0511-os-open-is-recorded-not-launched-under-a-debug-switch.md) (plugin 프로세스의 열기는 스위치 밖이라는 조항)
- 개정 패턴 선례: [ADR-0030](0030-image-egui-mesh-bitmap-texture.md)
- 탐색: `git grep -l 'webbrowser::open\|dispatch_external_link' -- docs/adr/` → 0511 하나
- [markdown plugin](../plugins/markdown/index.md) "링크 클릭 라우팅"
- [debug-ipc.md](../dev-guide/debug-ipc.md) "OS 열기를 띄우지 않고 기록하기"
- 코드 근거(현재 위치): `src/app/dispatch/plugin_webview_open.rs` 의 `authorize` ·
  `crates/tasty-plugin-markdown/src/main.rs` 의 `dispatch_external_link`
