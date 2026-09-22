# ADR-0042: native 파일 선택 다이얼로그는 host `fs.pick_file`(FsRead)로 위임한다

- **Status**: Superseded by ADR-0162 — 이 ADR 이 스스로 한정한 유스케이스(markdown plugin browse 버튼)가 ADR-0058 의 host 소유 popup 으로 옮겨가 비었고, 남은 메서드는 호출자 없이 호스트를 멎게 할 수 있어 제거됐다.
  원격 인지 + 비동기 파일 브라우징은 [ADR-0053](0053-native-file-picker-remote-attach-channel.md)
  이 별개 메커니즘으로 다룬다(본 ADR 의 개정이 아니다 — 전제 자체가 달라 분리).
- **Date**: 2026-07-09
- **Tags**: plugin, ipc, fs, native-dialog, rfd, permission, fs-read, host-delegation, markdown, focus-independence, adr-0028
- **Group**: file-handler

## Context

markdown de-pluginize 과정에서 "파일 열기" 팝업이 host 소유 PopupDef 에서 markdown plugin 의
egui-mesh `[[contributes.popup]]` 로 이동했다. 이 팝업의 browse 버튼은 native OS 파일 선택
다이얼로그(macOS `NSOpenPanel`, Windows `IFileDialog`, Linux portal)를 열어야 한다.

그러나 plugin 은 별도 프로세스에서 egui-mesh 를 tessellate 할 뿐(ADR-0028) winit 윈도우도
NSApplication main 스레드도 소유하지 않는다. native 파일 다이얼로그는 대개 앱의 UI 메인
스레드(특히 macOS 는 AppKit main thread)에서 열어야 하므로 **plugin 프로세스에서 직접 열 수
없다.** 반면 host 는 이미 winit main 스레드에서 IPC 를 drain 하고(`about_to_wait` →
`process_ipc`) rfd 를 gui feature 로 링크한다(구 host `file_open` 팝업이 rfd 를 그렇게 썼다).

필요한 것은 "plugin 이 host 에게 native 파일 선택을 대신 열어 달라고 요청하고, 선택된 경로를
회신받는" generic IPC 메서드다. host 는 이 메서드가 markdown 용이라는 사실을 알아서는 안 된다
(host 무지 원칙) — filters 는 caller 가 채운다.

## Decision

host 에 generic IPC 메서드 **`fs.pick_file {filters?, start_dir?} → {path?}`** 를 추가하고
권한을 **`FsRead`** 로 건다. 핸들러(IPC 핸들러 디렉터리의 전용 `fs` 모듈, gui feature 전용)는
winit main 스레드에서 동기 dispatch 되는 흐름 위에서 `rfd::FileDialog::pick_file()`(모달
블로킹)을 그 자리에서 호출하고, 사용자가 고른 경로(취소면 `path` 없음)를 응답으로 돌린다.
async/oneshot 회신 배관은 없다 — plugin 의 `host.call` 은 host 메인 루프에서 inline dispatch
되어 응답까지 동기로 반환되므로(`handle_ipc_default_dispatch` → `dispatch_with_caller` →
`send_ipc_result`), 모달이 열려 있는 동안 plugin 은 자신의 `host.call` 결과를 기다릴 뿐
데드락이 없다.

권한을 `FsRead` 로 정한 근거: 사용자가 임의 경로를 **고르는** 것은 파일시스템을 읽는(탐색하는)
관심사다. 다이얼로그는 파일을 생성/수정하지 않는다 — 선택 경로 문자열만 반환한다. 실제 파일
내용 read 는 뒤이어 별도 경로(`file_handler.dispatch`)로 일어나고 그 또한 read 관심사다.

## Consequences

- **얻은 것**: plugin 이 별도 프로세스·비-메인-스레드라는 제약을 깨지 않고 native 파일 선택을
  쓸 수 있다. host 는 `fs.pick_file` 이 어느 plugin·어느 kind 를 위한 것인지 모른 채 generic
  하게 제공한다(무지 유지) — filters/start_dir 는 전부 caller 가 채운다. 회신 배관이 동기라
  구현이 단순하다(별도 채널/상태 없음).
- **잃은 것**: `fs.pick_file` 처리 동안 host 메인 스레드가 모달로 블로킹된다(사용자가 다이얼로그를
  닫을 때까지). 이는 native 파일 다이얼로그의 본질적 성질이자 구 host `file_open` 팝업과 동일한
  거동이라 회귀가 아니다. 그동안 다른 IPC 는 drain 되지 않고 큐에 남는다.
- **운영 비용 / 유지 부담**: rfd 를 gui feature 에 계속 의존한다(이미 의존 중). 비-gui/headless
  빌드에는 이 메서드가 없다(`#[cfg(feature = "gui")]`) — 그 환경엔 native 다이얼로그 개념이
  없으므로 정상. `FsRead` 를 가진 plugin 은 모두 이 다이얼로그를 띄울 수 있다(권한 세분화 없음).

## Alternatives Considered

- **plugin 프로세스에서 rfd 직접 호출**: plugin 은 winit/AppKit main 스레드를 소유하지 않아
  macOS 에서 다이얼로그가 뜨지 않거나 크래시한다. 채택 불가(근본 제약).
- **전용 권한 `FsPickFile` 신설**: 파일 선택만을 위한 별도 권한. 선택은 read 관심사의 부분집합이라
  `FsRead` 로 충분하고, 권한 표면을 불필요하게 늘린다. 기각.
- **`Clipboard`/`UiPopup` 같은 다른 기존 권한 재사용**: 파일시스템 탐색 의미와 어긋난다. 기각.
- **async oneshot 회신 채널**: host 메인 루프가 IPC 를 동기 inline dispatch 하는 현 구조상
  불필요한 복잡도. 모달 자체가 동기 블로킹이라 async 로 얻을 이득도 없다. 기각.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

- host IPC dispatch 가 동기 inline 모델에서 벗어나(워커 스레드/async) `host.call` 이 더 이상
  같은 스레드에서 즉시 반환되지 않게 될 때 — 회신 배관 전제가 깨진다.
- native 파일 다이얼로그를 메인 스레드 블로킹 없이 여는 크로스플랫폼 비동기 방식이 필요해질 때
  (예: 모달 중 다른 IPC 를 계속 처리해야 하는 요구).
- 파일 **선택**과 파일 **읽기** 권한을 분리해야 하는 보안 요구가 생길 때(`FsRead` 세분화).

## References

- [`docs/dev-guide/plugin-permissions.md`](../dev-guide/plugin-permissions.md) — plugin 권한 모델
- [ADR-0028](0028-plugin-egui-mesh-render-channel.md) — plugin egui-mesh 렌더 채널(plugin 별도 프로세스)
- `fs.pick_file` 핸들러 모듈(스레드 모델 주석) — ADR-0162 로 제거됐다
- `crates/tasty-ipc/src/method_meta.rs` — `fs.pick_file` 권한 등록(FsRead)
- `crates/tasty-plugin-markdown/src/popup.rs` — caller(browse → `fs.pick_file`)
