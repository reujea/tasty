# ADR-0250: Linux 도 원격 서브리소스를 막는다 — macOS 와 같은 규칙을 WebKit content filter 로 컴파일해서

- **Status**: Accepted
- **Date**: 2026-09-08
- **Tags**: webview, linux, webkitgtk, security, remote-content, cross-platform, ffi, adr-0249
- **Group**: plugin-render

## Context

`allow_remote_content` 는 세 백엔드가 함께 지키기로 한 설정인데 **Linux 만 결론이 달랐다.**
macOS 는 `WKContentRuleList` 로, Windows 는 `WebResourceRequested` 403 으로 페이지 안의
서브리소스까지 막는다. Linux 는 `decide-policy` 하나뿐이었고 그 시그널은 최상위/프레임
네비게이션과 정책 협의 대상 응답에만 발화한다.

실측(2026-09-08, Linux/X11/WebKitGTK, 격리 홈, webview 자식 창 직접 캡처): 기본값
`allow_remote_content=false` 인 markdown 문서에서 원격 배지 이미지가 **그대로 떴다.**
같은 문서를 macOS·Windows 에서 열었을 때의 결론과 어긋난다.

코드에 남아 있던 사유는 "webkit2gtk 2.0.2 바인딩이 UserContentFilter 를 노출하지 않는다"
였다. 그 문장의 앞부분은 맞고 뒷부분이 틀렸다 — **안전한 바인딩에만 없고 `-sys` 에는 다
있다**(`webkit_user_content_filter_store_new/save/save_finish` ·
`webkit_user_content_manager_add_filter` · `webkit_user_content_filter_unref`). 안전한
쪽의 `add_filter` 는 주석 처리돼 있고(`//fn add_filter`), `remove_filter_by_id` 와
`remove_all_filters` 는 살아 있다.

## Decision

**macOS 와 같은 content-blocker 규칙을 Linux 에서 컴파일해 user content manager 에 붙인다.**
규칙 문자열은 두 플랫폼이 같다:

```json
[{"trigger":{"url-filter":"^https?://"},"action":{"type":"block"}}]
```

WebKit 은 content filter 를 **디스크 저장소에 컴파일해 두고** 쓰므로 이 경로가 비동기다.
`PlatformWebView::new` 가 tasty 홈 아래(`webkit-content-filters/`) 저장소를 열어 저장을
걸고, 완료 콜백이 그때의 차단 플래그를 다시 읽어 붙인다. 이후 토글은
`set_remote_content_allowed` → `apply_remote_block_filter` 가 **항상 먼저 지운 뒤 차단이면
다시 붙이는** 형태로 반영한다 — macOS 의 `apply_block_state` 와 같은 모양이라 중복 add 가
안 생긴다.

안전한 바인딩에 없는 세 호출(`store_new`/`store_save`/`add_filter`)만 `webkit2gtk::ffi` 로
직접 부른다. 새 의존은 없다 — `webkit2gtk` 가 `ffi` 를 재수출한다. 필터 핸들은 GObject 가
아니라 ref-count 되는 boxed 타입이라 `ContentFilter` 래퍼가 소유하고 Drop 이 unref 한다.

`decide-policy` 는 그대로 둔다. 두 자리는 다른 것을 막는다 — 하나는 네비게이션, 다른
하나는 페이지 안의 요청이다.

## Consequences

- **얻은 것**: 세 OS 의 결론이 같아졌다. 실측 — 기본(차단)에서 원격 배지가 실패
  placeholder 로 바뀌고, `plugin_settings."com.tasty.markdown".allow_remote_content = true`
  로 켜면 다시 로드된다(음성 대조군이 붙었다는 뜻이기도 하다: 차단이 네트워크 부재가
  아니라 이 필터의 결과다).
- **잃은 것**: 이 백엔드에 unsafe FFI 가 세 자리 늘었다. 안전한 바인딩이 그 셋을
  노출하면 지워야 하는 코드다(아래 재검토 조건).
- **잃은 것 2**: 필터 컴파일이 비동기라 `new()` 직후의 짧은 구간에는 필터가 없다. 문서
  로드가 그 뒤라 실사용에서 열리는 창은 관측되지 않았고, 컴파일이 실패하면 경고가 남는다.
- **운영 비용**: tasty 홈에 `webkit-content-filters/` 가 생긴다(WebKit 이 관리·재사용).

## Alternatives Considered

- **A: `WebResource` 의 `send-request` 시그널로 요청을 취소한다** — 이 환경에서 **안
  된다**. 실측 2026-09-08: `connect_local("send-request", …)` 가 그 자리에서 프로세스를
  죽였다 — `Signal 'send-request' of type 'WebKitWebResource' not found`. 그 시그널은
  `WebKitWebPage`(웹 프로세스 확장 API)의 것이지 UI 프로세스의 `WebKitWebResource` 것이
  아니다. 바인딩이 아니라 WebKit 자체의 자리 문제다.
- **B: `WebContext::set_network_proxy_settings` 로 http/https 를 죽은 프록시에 보낸다** —
  프록시는 **WebContext 단위**라 기본 컨텍스트를 공유하는 다른 webview(html plugin 이
  원격을 허용한 창 포함)까지 함께 막는다. webview 마다 컨텍스트를 나누면 막을 수는
  있지만 프로세스 모델을 바꾸는 일이라 대가가 훨씬 크다.
- **C: 상위 webkit2gtk 바인딩을 기다린다** — 결론이 갈린 상태가 그동안 유지된다. 필요한
  심볼은 이미 `-sys` 에 다 있어 기다릴 이유가 없다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

**채널이 붙는 것** — 판정 시점에 레포가 읽을 수 있는 사실이다.

- `webkit2gtk` 의 안전한 바인딩이 `UserContentManagerExt::add_filter` 와
  `UserContentFilterStore` 를 노출한다(그 크레이트의 `user_content_manager.rs` 에서
  `//fn add_filter` 주석이 사라진다). 그러면 이 파일의 unsafe 세 자리를 지운다.
- `webkit2gtk` 의존 버전이 `=2.0.2` 에서 올라간다.

**원리적으로 안 붙는 것** — 사람이 관측해야 한다. 재는 법을 함께 적는다.

- 필터 컴파일 지연이 실제로 창을 연다. 재는 법: 원격 이미지가 있는 문서를 기본값으로
  열어 첫 프레임에 원격 리소스가 그려지는지 본다(로그의 컴파일 실패 경고도 함께 본다).
- content filter 가 로컬 `data:` 이미지까지 막는 회귀. 재는 법: 로컬 이미지가 있는
  markdown 문서를 기본값으로 열어 이미지가 그대로 뜨는지 본다.

## References

- [ADR-0249](0249-markdown-local-images-are-inlined-by-the-renderer.md) — 같은 티켓의
  다른 절. 로컬 이미지를 `data:` 로 싣는 결정이라 이 필터의 `^https?://` 규칙과 서로
  간섭하지 않는다(인라인된 이미지는 원격 요청을 안 낸다).
- 코드 근거(결정이 실현된 현재 위치): `src/host_api/webview/linux.rs` 의
  `compile_remote_block_filter` · `remote_block_filter_saved` ·
  `PlatformWebView::apply_remote_block_filter`.
- 같은 규칙의 macOS 판: `src/host_api/webview/macos.rs` 의 `apply_block_state`.
- 설정 항목 서술: [`docs/features/settings/index.md`](../features/settings/index.md).
