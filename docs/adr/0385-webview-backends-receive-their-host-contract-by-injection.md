# ADR-0385: webview 백엔드는 호스트 계약을 주입받는다 — 탐색 상태는 도메인 모델, 키 정책은 콤보 목록, 키 접점은 trait

- **Status**: Accepted
- **Date**: 2026-09-21
- **Tags**: architecture, webview, host-api, keybindings, layering, injection, cross-platform, adr-0102, adr-0320
- **Group**: plugin-render

## Context

native webview 백엔드(`src/host_api/webview/{linux,macos,windows}.rs`)와 그 키 모듈
(`src/host_api/webview/keys.rs`)은 호스트의 **배치**를 세 갈래로 역참조하고 있었다. 결정
시점 기준 좌표다.

1. **탐색 상태의 경로.** `NavState` 는 `src/plugin_bridge/remote_surface.rs` 에 정의되고
   `src/host_api/webview.rs` 가 `pub use crate::plugin_bridge::remote_surface::NavState` 로
   재수출했다. 백엔드는 `super::NavState` 로 불렀지만 그 이름이 가리키는 정의는 plugin 연결
   계층에 있었다. 그 자리에 둔 이유는 주석에 있었다 — `RemoteSurface` 는 비-gui 빌드에도
   컴파일되므로 gui 게이트 뒤의 webview 모듈에 두면 참조할 수 없다. 이유는 옳았지만 해법이
   "항상 컴파일되는 다른 소비자의 파일" 이라, 세 소비자(`RemoteSurface` mirror · native 백엔드
   · egui chrome) 중 하나가 나머지의 경로를 정하는 모양이었다.
2. **키 정책의 입력.** `HostShortcutPolicy::from_sources(&KeybindingSettings, plugin_combos)`
   가 `KeybindingSettings::GENERAL_BINDING_FIELDS` 를 순회하고 quick-switch 세 축을
   합성하고, `PAGE_RESERVED_FIELDS`(`find`·`copy`·`cut`·`paste`·`select_all`) 라는 **설정
   필드 id** 를 키 모듈 안에 두었다. 즉 "어느 설정 필드가 host 액션인가" 라는 단축키 계층의
   지식이 webview 모듈에 있었다. 그래서 `keys.rs` 는 `crate::settings` 없이는 컴파일되지
   않았고, 그 판정 규칙(modifier 필터·예약 동등성)은 설정을 통째로 만들지 않고는 시험할 수
   없었다.
3. **키 접점의 타입.** `PlatformWebView::new` 는 세 백엔드 모두 `Rc<WebViewKeyBridge>` 라는
   **구체 타입**을 받았다. 백엔드가 실제로 부르는 것은 `capture_key` 와 `note_focus` 둘뿐인데,
   받는 값은 정책 교체(`set_policy`)·큐 비우기(`take_pending` · `take_focus_requests`)까지
   가진 호스트 쪽 객체였다.

키 쪽의 gui 역참조(`adapters/ui/input/shortcuts` 심볼)는 매칭 규칙이 `tasty-key-match`
크레이트로 올라가면서 이미 끊겼고, 플랫폼 helper 는 `tasty-platform` 크레이트로 옮겨져
의존 방향이 본체→leaf 로 정리돼 있었다. 남은 것이 위 셋이다.

## Decision

webview 백엔드와 키 모듈은 호스트 계약을 **주입받는다.** 셋 모두 외부 동작(IPC 응답 ·
CLI 출력 · plugin wire · 어떤 키를 host 가 가져가는가)은 바꾸지 않는다.

1. **`NavState` 는 `tasty-model` 이 소유한다**(`crates/tasty-model/src/nav_state.rs`,
   루트 재수출 `tasty_model::NavState`). `RemoteSurface` 는 `crate::model::NavState` 를,
   `host_api::webview` 는 `pub use crate::model::NavState` 를 쓴다 — 어느 쪽도 다른 쪽의
   경로를 거치지 않는다. 네 값짜리 `Copy` enum 이고 OS·webview·egui 타입을 하나도 담지 않으므로
   도메인 경계에 둬도 새는 것이 없다.
2. **`HostShortcutPolicy` 는 콤보 목록을 받는다.** 생성자는
   `HostShortcutPolicy::new(ShortcutSources { host, page_reserved, plugin })` 하나이고, 키 모듈이
   하는 판정은 두 축뿐이다 — modifier 가 있는 콤보만, 그리고 페이지 예약 콤보와 **동등한**
   plugin 콤보는 제외. 어느 설정 필드가 host 액션이고 어느 것이 페이지 예약인지는
   `src/adapters/ui/input/shortcuts/webview_claims.rs` 의 `webview_shortcut_policy` 가
   `KeybindingSettings` 에서 도출해 넘긴다(quick-switch 합성은 디스패치 `numeric.rs` 와 같은
   규칙). `keys.rs` 는 `crate::settings` 를 더 이상 참조하지 않는다.
3. **백엔드는 `Rc<dyn WebViewKeySink>` 를 받는다.** `WebViewKeySink` 는 `capture_key` 와
   `note_focus` 두 메서드의 trait 이고, 호스트 구현이 `WebViewKeyBridge` 다. 정책 교체와 큐
   비우기는 브리지의 고유 메서드로 남아 백엔드가 볼 수 없다. 스레드 친화성(세 백엔드의
   콜백이 모두 winit main thread 에서 발화)은 그대로 `Rc` + `RefCell` 의 근거다.

**ADR-0320 과의 관계.** 0320 은 세 `PlatformWebView` 를 묶는 trait(호스트가 백엔드를 부르는
방향)을 기각했다. 이 결정의 trait 은 반대 방향 — 백엔드가 호스트를 부르는 접점 — 이고,
0320 이 기각한 이유(이름·시그니처 일치는 이미 공유 호출부와 세 OS 컴파일이 강제한다)가
여기에는 적용되지 않는다. 여기서 trait 이 사는 것은 **일치가 아니라 좁힘**이다: 백엔드가
호스트에 대해 아는 것을 두 메서드로 줄인다. 0320 의 결정은 바뀌지 않는다.

## Consequences

- **얻은 것**:
  - 백엔드 세 파일이 호스트에 대해 아는 것은 `WebViewKeySink` 두 메서드와 `NavState` 값뿐이다.
    정책 판정이나 큐 모양이 바뀌어도 백엔드 본문은 안 바뀐다.
  - 키 모듈의 판정 규칙을 설정 없이 콤보 문자열로 시험할 수 있다. 가로채기·교체·무정책·
    중복 제거·예약 동등성(대소문자·modifier 순서 변형)·예약 필터가 plugin 콤보에만 걸린다는
    사실이 `keys.rs` 의 단위 시험으로 고정되고, 설정 → 정책 도출은 `webview_claims.rs` 의
    단위 시험이 따로 고정한다(기본값 끝과 끝 · 콤보를 바꾸면 claim 이 옮겨 간다 · 예약 ·
    창 chrome · `ctrl+z` · quick-switch).
  - `keys.rs` 가 `crate::settings` 를 안 보므로, 나중에 webview 모듈을 크레이트로 뽑을 때
    키 모듈이 본체 의존 없이 따라갈 수 있다.
- **잃은 것**:
  - 키 콜백 한 번에 동적 디스패치 한 번이 붙는다. 사람의 키 입력 빈도라 잴 만한 비용이 아니다.
  - "어느 액션을 host 가 가져가는가" 가 두 파일에 나뉜다 — 필터 규칙은 `keys.rs`, 필드 목록은
    `webview_claims.rs`. 두 머리 주석이 서로를 가리킨다.
- **운영 비용 / 유지 부담**: 새 설정 필드가 `GENERAL_BINDING_FIELDS` 에 들어가면 자동으로
  정책에 오른다(종전과 같다). 새 **페이지 예약** 액션은 `webview_claims.rs` 의
  `PAGE_RESERVED_FIELDS` 에 더한다.

## Alternatives Considered

- **webview 전용 leaf 크레이트를 새로 만들어 `NavState` 를 거기 둔다** — 안 골랐다. 지금 그
  크레이트에 들어갈 수 있는 것은 `NavState` 하나다. `WebViewBounds` 와 키 모듈은 옮길 수
  있지만 백엔드 셋은 `gtk`/`webkit2gtk`·`objc2`·`webview2-com` 과 `raw-window-handle` 을 끌고,
  그것을 크레이트로 뽑는 것은 이 결정의 범위(주입)가 아니다. 크레이트 하나를 늘리면 크레이트
  수가 적힌 자리(`docs/architecture/index.md` · 루트 `CLAUDE.md` · 두 README)가 함께 움직이는데,
  얻는 것은 enum 하나의 위치다. `NavState` 는 이미 `RemoteSurface` 라는 도메인 소비자를 가졌고
  `tasty-model` 은 번들 plugin 의존 폐포 밖이라 plugin 버전에도 안 닿는다.
- **`NavState` 를 `plugin_bridge` 에 둔다(종전)** — 안 골랐다. 세 소비자 중 하나의 파일이
  나머지의 경로를 정하는 모양이 그대로 남는다.
- **정책이 `KeybindingSettings` 를 받되 예약 필드 목록만 주입한다** — 안 골랐다. 키 모듈의
  `crate::settings` 의존이 남아 목적(판정 규칙을 설정 없이 시험 · 추출 가능)이 절반만 된다.
- **예약 필터를 host 콤보에도 건다** — 안 골랐다. 종전 동작은 host 쪽을 **필드 id** 로만
  걸렀다. 콤보 동등성으로 다시 거르면 사용자가 host 액션과 예약 액션에 같은 콤보를 준
  설정에서 그 host 액션이 webview 위에서만 조용히 죽는다 — 호환이 깨지는 쪽이다.
- **`WebViewKeySink` 대신 제네릭 파라미터(`impl WebViewKeySink + 'static`)** — 안 골랐다.
  macOS 의 `KeyWebView` 는 `define_class!` ivar 에 이 값을 담는데 ivar 는 구체 타입이어야
  하므로 결국 `dyn` 이 필요하고, 세 백엔드의 시그니처가 갈라진다(ADR-0320 이 기대는 공유
  호출부가 셋에 같은 인자를 넘긴다).
- **trait 대신 클로저 두 개(`Rc<dyn Fn>` 쌍)를 넘긴다** — 안 골랐다. 두 입력이 한 호스트
  객체의 짝이라는 사실이 타입에서 사라지고, 계약의 이름·문서 자리도 없어진다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

**채널이 붙는 것**

- webview 모듈(`src/host_api/webview*`)이 본체 밖 크레이트로 추출된다. 그때는 `NavState` 를
  그 크레이트로 옮길지(그 크레이트가 `tasty-model` 에 의존하게 둘지)를 다시 정한다.
- `WebViewKeySink` 의 구현이 둘 이상이 되거나 메서드가 셋 이상이 된다 — 접점이 넓어지면 이
  trait 이 "좁힘" 이라는 근거가 약해진다.

**원리적으로 안 붙는 것**

- macOS 조합의 컴파일. 이 레포의 Linux 개발 머신에서는 macOS 타깃이 컴파일되지 않는다(clang
  부재). macOS 백엔드의 이 변경은 소스를 읽어 쓴 것이고 그 조합을 실제로 컴파일하는 것은
  `crossplatform-check` 의 `check-macos` 잡이다. 재는 법: 그 잡의 결론을 잡 단위로 읽는다
  (`gh run view <id> --json jobs`).

## References

- 계약 문서: [`docs/design/systems/webview.md`](../design/systems/webview.md)
- 키 포워딩 결정: [ADR-0102](0102-webview-key-forwarding.md)
- 백엔드 trait 기각: [ADR-0320](0320-the-webview-backends-are-held-together-by-shared-call-sites-not-a-trait.md)
- 코드 근거(결정이 실현된 현재 위치): `tasty_model::NavState` · `host_api::webview::keys::{HostShortcutPolicy, ShortcutSources, WebViewKeySink, WebViewKeyBridge}` · `adapters::ui::input::shortcuts::webview_claims::webview_shortcut_policy`
