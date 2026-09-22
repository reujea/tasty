# ADR-0272: URL 대상은 핸들러 picker 와 실행 계층에만 들어가고 형식 식별에는 들어가지 않는다

- **Status**: Accepted
- **Date**: 2026-09-14
- **Tags**: file-handler, terminal-link, dispatch, url
- **Group**: file-handler

## Context

핸들러 흐름의 대상 타입은 `FileTarget(PathBuf)` 한 겹이었다. 형식 식별(`FileFormatRegistry::identify`)도,
picker(`FileHandlerPickerData.target`)도, 액션 실행(`execute_handler_action`)도 이 타입 하나를 썼다.
그래서 아키텍처는 "파일은 핸들러, URL 은 OS" 로 나뉘어 있었다 — 터미널 링크 좌클릭은
`LinkKind::External` 을 곧장 `terminal_link::open_uri` 로 보내고, plugin 프로토콜의
`webview.navigation_attempt` 주석도 같은 이분법을 적는다.

터미널 링크 우클릭 메뉴의 "연결 동작"은 링크를 핸들러 picker 에 넘기고, 사용자는 `http(s)://`
링크에서도 그 항목이 동작하기를 원했다. URL 을 받을 목적지는 이미 있다 — html 핸들러가
`open_surface { surface_kind = "html", param_key = "url" }` 로 webview 를 띄운다.

URL 문자열을 `PathBuf` 에 그대로 담으면 겉보기엔 동작하지만(`to_string_lossy` 가 원문을
보존한다) 네 자리가 조용히 틀린다.

1. 식별의 확장자 fast path 가 `https://example.com/a.md` 의 `md` 로 markdown detector 를 고른다.
2. `PathGlob` 이 `file_name()` 으로 마지막 세그먼트(`a.md`)를 매칭한다.
3. `System` 액션이 `path_to_file_uri` 로 `file:///https://example.com/a.md` 를 만들어 OS opener 에 넘긴다.
4. `Ipc` 액션이 plugin 에 `{"path": "https://…"}` 를 보낸다 — plugin 은 그 키를 파일 경로로 해석한다.

`OpenSurface` 만 `{param_key: 원문}` 이라 우연히 왕복한다.

## Decision

dispatch 계층에 `DispatchTarget::{File(FileTarget), Url(String)}` 을 두고 **picker 와 액션 실행이
그것을 공유한다.** 형식 식별 계층(`FileTarget`, registry, evaluator)은 파일 전용으로 남고, URL 은
detector 없이 핸들러 목록으로 직행한다. 각 액션이 URL 을 받는지는 판정 하나
(`handler_accepts_target`)가 정한다.

- `System` 은 받는다 — URL 원문을 그대로 OS opener 에 넘긴다(`file://` 로 감싸지 않는다).
- `OpenSurface` 는 `param_key = "url"` 을 선언한 핸들러만 받는다. 그 키 이름이 곧 "이 surface 는
  URL 을 받는다" 는 선언이다. `file`/`path` 키는 로컬 경로를 기대한다는 선언으로 읽는다.
- `Ipc` 는 받지 않는다. 큐(`pending_handler_ipc`)는 `FileTarget` 만 담아 URL 이 `path` 키로 갈
  길이 타입상 없다.

이 판정은 picker 의 후보 열 · recent 열(저장 파일에서 따로 읽힌다) · 최종 실행 세 자리에 똑같이
걸린다. 실행에서 거절된 선택은 recent 에 기록하지 않는다.

`Url` 은 `http`/`https` 만 만든다. mailto/ssh/ftp 등은 열 핸들러가 없어 기존 OS opener 경로에 남는다.

형식 식별 계층에는 방어를 하나 더 둔다. 경로 문자열로 들어온 URL 이 `FileTarget` 에 담기는
입구(IPC 경로 문자열 등)가 남아 있으므로, `identify` 와 `evaluate_cheap`/`evaluate_deep`/`DeepCtx`
는 `<scheme>://` 모양(scheme 두 글자 이상 — Windows 드라이브 제외)의 경로를 매칭하지도 읽지도
않는다.

공개 IPC `file_handler.dispatch` 는 URL 을 받지 않는다. 경로 자리에 URL 이 오면 `-32602` 로
거절한다. 이 메서드의 권한 토큰(`FsRead`)은 URL 을 여는 능력을 덮지 않고, URL 을 받게 하려면
권한 축부터 다시 정해야 한다. 터미널 링크 **좌클릭**의 동작도 바꾸지 않는다(`External` →
`open_uri`). URL 이 핸들러 흐름에 들어오는 입구는 사용자 조작인 링크 우클릭 메뉴 하나다.

## Consequences

- **얻은 것**: URL 을 사용자가 고른 핸들러로 열 수 있다(html 핸들러 → tasty 안 webview). 위 네 오동작
  지점이 각각 단위 테스트로 고정된다. 식별·평가 계층의 시그니처와 `FileTarget` 호출부는 그대로다.
- **잃은 것**: URL 은 detector 가 없어 "이 URL 형식의 1순위 핸들러 자동 실행" 이 없다 — 늘 picker 를
  거친다. `param_key` 이름에 의미를 싣는 것은 암묵 규약이라, URL 을 다른 키로 받는 surface 는 후보에
  안 뜬다.
- **운영 비용 / 유지 부담**: 새 `HandlerAction` 종류가 생기면 `handler_accepts_target` 의 `Url` 분기에
  답을 적어야 한다(`match` 가 망라적이라 컴파일러가 요구한다). `file_handler.dispatch` 에 URL 을 넘기던
  호출자가 있었다면 이제 오류를 받는다.

## Alternatives Considered

- **`FileTarget` 을 enum 으로 확장**: 호출부가 타입 하나를 계속 쓰지만 `as_path()` 를 쓰는 식별·평가 자리가
  전부 "URL 일 때 무엇을 주는가" 에 답해야 한다. 그 답이 필요 없는 자리(식별)까지 흔드는 것이라
  고르지 않았다 — 식별은 URL 을 다룰 일이 없다.
- **URL 전용 타깃 + 별도 detector 축(registry·설정 UI·TOML 두 벌)**: 파일 파이프라인은 안 흔들리지만 정본이
  둘이 된다(ADR-0047 이 hook handler 쪽에서 이미 한 번 미러링했다). 대상 타입만 나누고 registry 를 한 벌로
  두는 지금 형태가 이 안의 장점을 비용 없이 가진다.
- **대상 타입은 그대로 두고 URL 은 picker 전용 얇은 경로**: 변경 폭은 가장 작지만 `System`·`Ipc` 를 액션별로
  개별 방어해야 하고, 빠뜨린 자리는 타입이 알려주지 않는다.
- **`Ipc` 에 URL 을 별도 키(`url`)로 보내기**: plugin 쪽 규약이 새로 생긴다. URL 을 받겠다는 plugin 이 아직
  없어 받는 쪽 없는 규약을 먼저 만드는 셈이라 제외했다.
- **manifest 에 `targets = ["file", "url"]` 같은 명시 선언**: 가장 정확하지만 plugin 매니페스트 스키마와 번들
  plugin 버전이 함께 움직인다. 지금 URL 을 받는 핸들러는 html 하나이고 그것이 이미 `url` 키를 선언한다.

## Reconsideration Triggers

**채널이 붙는 것**

- `HandlerAction` 에 새 variant 가 생긴다. `handler_accepts_target` 의 `match` 가 망라적이라 컴파일이 멈추고,
  그 자리에서 URL 을 받을지 정해야 한다.

**원리적으로 안 붙는 것**

- URL 을 `url` 이 아닌 파라미터 키로 받는 surface 핸들러가 생긴다. 재는 법: 매니페스트와 user TOML 의
  `param_key` 값을 모아 URL 을 받는 surface 인지 사람이 확인한다 — 그런 핸들러가 나오면 명시 선언으로 옮긴다.
- URL 을 처리하겠다는 `Ipc` 핸들러(plugin)가 나온다. 재는 법: plugin 요청·이슈로 들어온다. 그때 `Ipc` 의 URL
  규약(키 이름)과 `file_handler.dispatch` 의 권한 축을 함께 정한다.

## References

- 코드 근거(결정이 실현된 현재 위치): `src/file/dispatch.rs` 의 `DispatchTarget` · `handler_accepts_target` ·
  `picker_lists` · `execute_handler_action`, `crates/tasty-file-format/src/types.rs` 의 `looks_like_url`,
  `src/adapters/ipc/handler/file_handler.rs` 의 `handle_dispatch`
- [file-handler 기능 문서](../features/file-handler/index.md) · [terminal-link 기능 문서](../features/terminal-link/index.md)
- [ADR-0047](0047-shared-hook-handler-registry-source-gate.md) — registry 미러링 선례(별도 detector 축의 비용 근거)
