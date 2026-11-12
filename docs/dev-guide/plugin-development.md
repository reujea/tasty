# 플러그인 제작 가이드

외부 Tasty 플러그인을 작성·빌드·설치하는 법. 개념(배포/통합 축·권한)은 [concepts/plugins](../concepts/plugins.md) 먼저. 권한 모델 상세는 [plugin-permissions](plugin-permissions.md), 민감 데이터는 [아래 "민감 데이터"](#민감-데이터--regular--secret--keyring-선택).

**번들 플러그인이 곧 reference 예제다** — 각 기여 타입을 만들 때 아래 표의 해당 플러그인 코드를 시작점으로 복사·수정하는 게 가장 빠르다.

## 기여 타입 → 예제 플러그인

| 만들고 싶은 것 | 보면 되는 번들 플러그인 | 난이도 |
|---------------|------------------------|--------|
| **egui-mesh surface** (자가 렌더 mesh 합성) | [image](../surfaces/image/index.md) · [mesh-demo](egui-mesh-channel.md)(최소 PoC) | ★★ |
| **webview surface** | [html](../surfaces/html/index.md) · [markdown](../surfaces/markdown/index.md)(+파일 핸들러·settings, docs/design/systems/webview.md) | ★★ |
| **도구 메뉴 항목 + popup** | [git-viewer](../plugins/git-viewer/index.md)(view/logic 분리) · [clipboard-viewer](../plugins/clipboard-viewer/index.md)(master-detail) | ★★ |
| **CLI + IPC namespace** | [codex](../plugins/codex/index.md) · [claude](../plugins/claude/index.md) | ★★★ |
| **이벤트 구독 / 훅 / 외부 설치** | [claude](../plugins/claude/index.md)(`surface.closed`·Claude 훅·install) | ★★★ |
| **agent task provider (턴 결과를 작업 결과로)** | [claude](../plugins/claude/index.md) · [codex](../plugins/codex/index.md) (namespace + `agent.turn_report`) | ★★★ |
| **CLI 호출자(다른 프로그램의 대화) 알아보기** | [codex](../plugins/codex/index.md)(`caller_context`) · [claude](../plugins/claude/index.md)(`requires_caller_context`) (매니페스트만) | ★ |
| **Tasty bash·zsh 셸에 넣는 셸 함수** | [codex](../plugins/codex/index.md)(`install`이 쓰는 `codex` 함수) | ★ |
| **Tasty 자식에서 지울 상속 환경변수** | [claude](../plugins/claude/index.md#tasty-자식에서-지우는-세션-변수) · [codex](../plugins/codex/index.md#tasty-자식에서-지우는-세션-변수) (매니페스트만) | ★ |
| **wasm 플러그인** (frozen POC) | `crates/tasty-plugin-sdk-wasm`(workspace-exclude harness) — [플러그인 권한 모델](plugin-permissions.md) | ★★ |

전부 `crates/tasty-plugin-<name>/` 에 있다.

## 개요

플러그인은 **별도 OS 프로세스**로 실행되어 호스트와 TCP+NDJSON 으로 통신한다. 호스트는 `~/.tasty/plugins/<id>/`의 매니페스트로 소유권을 등록한다. GUI 부팅과 headless 요청별 시작 정책은 아래 수명주기 절을 따른다. 작성자는 SDK(`tasty-plugin-sdk`)의 `Plugin` trait 을 구현하고 `run()` 을 호출하면 SDK가 핸드셰이크(토큰 인증, AuthAck 5초 대기)·NDJSON 직렬화·dispatch loop·ping/shutdown을 처리한다.

플러그인이 추가할 수 있는 기능은 [플러그인 통합 방식](../concepts/plugins.md#통합-축--host-에-무엇을-기여하나)을 참고한다. 다른 surface가 닫히는 이벤트만 구독하는 플러그인처럼, 매니페스트의 contribute 항목이 0개여도 된다.

## 1. 크레이트 골격 + 매니페스트

```
my-plugin/
  Cargo.toml          # [[bin]] + tasty-plugin-sdk 의존
  tasty-plugin.toml   # 매니페스트
  src/main.rs
```

매니페스트 필수 필드: `manifest_version` · `id`(reverse-DNS, 전역 유일) · `name` · `version`(semver) · `api_version` · `[entry]`. 실제 contribute 하는 항목만 추가 선언한다.

`com.tasty.`로 시작하는 id는 Tasty 공식 플러그인 전용이다. 공식 id 목록에 없는 `com.tasty.*` 플러그인은 debug·release 모두 설치·등록되지 않는다([플러그인 관리](../features/plugin-system/index.md#예약-id-comtasty)). 새 공식 플러그인은 [공식 id 목록](plugin-packaging.md#공식-id-목록)에 id를 먼저 넣는다.

```toml
manifest_version = 1
id = "com.example.myplugin"
name = "My Plugin"
version = "0.1.0"
api_version = "1"
permissions = ["fs.read", "surface.write"]
lang_dir = "lang"

[entry]
type = "process"
command = "my-plugin"

[[surface_kinds]]
kind = "myplugin_main"               # 소문자 + '_' + 숫자
display_name_i18n_key = "surface.kind.myplugin"
```

## 2. Plugin trait

```rust
use tasty_plugin_sdk::{Plugin, SurfaceCreateCtx, SurfaceResult};

struct MyPlugin;

impl Plugin for MyPlugin {
    fn id(&self) -> &str { "com.example.myplugin" }
    fn version(&self) -> &str { "0.1.0" }
    fn create_surface(&mut self, _ctx: SurfaceCreateCtx) -> SurfaceResult {
        SurfaceResult { display_name: Some("My Plugin".into()), ..Default::default() }
    }
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().with_env_filter("info").init();
    tasty_plugin_sdk::run(MyPlugin)
}
```

contribute 한 항목에 대응하는 콜백만 채우면 된다 — surface 가 없으면 `create_surface` 는 호출되지 않는다.

## 3. 기여 타입별 작성

각 타입은 매니페스트 선언 + (필요 시) trait 콜백. 자세한 매니페스트 스니펫은 해당 예제 플러그인의 `tasty-plugin.toml` 을 본다.

### Surface kind — `rendering` 3 종

- **`rendering = "egui-mesh"`** (번들 예: image/mesh_demo, 그리고 markdown 의 확인 팝업 2 개): 플러그인이 **자기 프로세스에서 egui 를 tessellate** 한 mesh 를 host 가 전용 `egui_wgpu::Renderer` 로 합성. SDK 를 `features=["egui-mesh"]` 로 받아 `paint_surface` 에서 `EguiMeshSurface::paint(...)` 호출. 커스텀 plugin 도 번들 plugin 과 같은 조건(api_version 일치, host 내장·다른 plugin 의 kind 이름과 충돌 없음)으로 등록한다. 조건과 이름 충돌 규칙은 [egui-mesh-channel "개방 정책"](egui-mesh-channel.md#개방-정책).
- **`rendering = "webview"`** (html, markdown — [WebView 호스트 계약](../design/systems/webview.md)): host 의 네이티브 WebView 오버레이로 그림. html 은 surface 의 URL 을 host 가 동기화하고, markdown 은 plugin 이 직접 sanitize 된 HTML 문서를 생성해 로드시킨다. WebView 의 zoom·JS·원격 콘텐츠·색 구성표는 kind 를 등록한 plugin 의 `plugin_settings` 에서 읽으므로 커스텀 plugin 도 설정 페이지에 같은 `storage_key`(`zoom`·`sandbox_scripts`·`allow_remote_content`·`color_scheme`)를 선언해 사용자 설정을 받을 수 있다. 설정 창에 두지 않을 기본값은 kind 선언의 `[surface_kinds.webview_defaults]`(같은 키, webview kind 전용)로 정한다. 복원된 탭이 많아 보지 않은 탭의 WebView 를 미리 만들지 않으려면 kind 선언에 `lazy_webview = true` 를 둔다. 그러면 처음 보일 때 만들고, 그 전의 `webview.post_message` 는 버려지므로 `surface.webview_created` 이벤트에서 문서 전체를 다시 실어야 한다([WebView 생성 시점](../design/systems/webview.md#생성-시점--미리-만들기와-처음-보일-때-만들기)). 우선순위와 html 전용 스크립트 게이트의 차이는 [WebView 호스트 계약 "설정 출처"](../design/systems/webview.md#설정-출처--kind-를-등록한-plugin).
- **`rendering = "remote"` (기본)**: webview 와 같은 `RemoteSurface` stand-in 등록만 하는 marker — host 는 이 kind 의 콘텐츠를 그리지 않는다. `snapshot_surface`/`restore_surface` 로 세션 복원.

remote·webview surface 를 연 채 플러그인 프로세스가 바뀌면(disable 뒤 enable, 무응답 재시작, `upgrade-builtins --restart-running` 교체) 호스트는 새 프로세스가 뜰 때 남은 surface 를 다시 게시한다. 마지막 snapshot 이 있으면 그 값으로 `restore_surface` 를 부르고, 없으면 처음 받은 `create_surface` 요청을 그대로 다시 보낸다. 그래서 `restore_surface` 는 세션 복원뿐 아니라 실행 중에도 불릴 수 있고, 이미 열린 host 쪽 surface(webview 포함)를 그대로 이어 써야 한다. egui-mesh surface 는 렌더할 때 새 프로세스에 `create_surface` 를 다시 보내는 bootstrap 경로로 이어진다. 호스트가 이미 접수한 닫기의 대상 surface 는 다시 보내지 않는다. 닫기가 그 surface 를 남긴 채 끝나면 그때 보낸다([닫기 시퀀스](../architecture/close-sequence.md)).

이 재게시가 되살리는 것은 snapshot 에 담긴 상태까지다. 스크롤 위치, 저장하지 않은 편집·그림처럼 옛 프로세스 메모리에만 있던 상태는 잃는다. 다시 보내지 못한 surface(요청 큐 포화 등)는 경고 로그만 남기고 다음 기동 때 다시 보낸다. 그 사이 surface 에 "재연결 필요" 같은 표시는 하지 않는다.

surface kind 선언에는 host 가 kind-agnostic 하게 소비하는 메타가 함께 실린다 — host 본체에 `if kind == "..."` 를 박지 않기 위한 것들이다:

- **`icon`** — 탭/프리셋 leading 아이콘의 **이름**. host 가 자기 아이콘 세트에서 `icons::from_name` 으로 glyph 에 매핑한다(현재 이름: `markdown`/`folder`/`image`/`html`/`terminal`/`file`; 미지의 이름은 `file` 로 fallback). 예: markdown 의 `icon = "markdown"`.
- **`preset_fields`** — 프리셋 편집기가 이 kind 를 편집할 때 노출할 입력 필드 스키마. `required = true` 인 `param_key` 는 surface 생성 IPC(`pane.split`/`workspace.new`)의 **필수 파라미터**로도 쓰인다(공통 선언). 예: markdown 은 `file` 필드 하나(required).
- **`param_aliases`** — 옛 caller 가 넘기는 alias 키 → canonical 키 매핑. host 가 convert 경로에서 정규화한다. 예: markdown 의 `{ file_path = "file" }`.
- **`default_params`** — surface 생성 시 params 에 없으면 host 가 주입하는 기본값(키 → 리터럴 또는 정책 토큰). 정책 토큰: `@settings.explorer_view_mode`(Settings 의 마지막 explorer view mode), `@home`(홈 디렉토리 — **새 탭 생성 fresh-context 에서만** 해석; split/preset/workspace 처럼 cwd 를 상속·carry 하는 경로에선 건너뛴다). 예: explorer(builtin)의 `{ view_mode = "@settings.explorer_view_mode", path = "@home" }`.
- **capability flags**(모두 기본 false) — host 의 입력/줌/복사/붙여넣기 게이트를 kind 하드코딩 없이 판정한다:
  - **`consumes_egui_input`** — host 가 이 kind 를 host egui 위젯으로 렌더해 winit 키/IME 를 host egui 로 흘린다(예: explorer). egui-mesh 렌더 kind 는 false(중앙 키 디스패처가 forward).
  - **`zoomable`** — 줌 in/out/reset 단축키로 폰트 크기 override 조절(예: markdown/explorer).
  - **`egui_copy`** — copy 단축키를 이 kind 의 egui-mesh surface 에 `Copy` wire 이벤트로 forward한다. plugin 자신의 egui `Context` 가 텍스트 선택(selectable label/`TextEdit`)을 복사하고, plugin 이 그 텍스트를 OS 클립보드에 직접 쓴다(docs/dev-guide/plugin-permissions.md — host round-trip 없음). markdown 이 webview 로 전환된 뒤([WebView 호스트 계약](../design/systems/webview.md)) 현재 이를 선언하는 번들 plugin 은 없다.
  - **`copy_path`** — select-all / copy-path 단축키(선택 항목 경로 복사) 소비(예: explorer).
  - **`egui_paste`** — paste 단축키를 이 kind 가 소비한다. host 는 terminal paste 로 흘리지 않고 포커스된 egui-mesh surface 에 `Paste` wire 이벤트(클립보드 내용 없음)를 보낸다. SDK 는 내용 없는 `egui::Event::Paste` 로 바꾸고, plugin 이 클립보드에서 필요한 형식을 직접 읽는다(예: image 는 이미지를 떠 있는 선택으로 붙인다).
- **`name_from_param`** — 자동 탭 명명 시 basename 을 파생할 params 키. 선언하면 그 키 값의 basename 을 탭 표시명으로 쓴다(예: markdown/image 는 `"file"`, explorer(builtin)는 `"path"` → `README.md`). 미선언이거나 params 에 그 키가 없으면 kind 표시명(`display_name_i18n_key`)으로 fallback. host 의 `kind == "markdown"` basename 명명 하드코딩을 대체. host 내장 kind 는 그 사이에 `SurfaceKindDef::tab_name_from_params`(create 에 넘길 cwd 와 params 를 받는 함수)를 둘 수 있다. explorer 는 이것으로 capture 한 preset 의 이름을 짓는다([explorer](../surfaces/explorer/index.md#저장복원)). 매니페스트 필드가 아니라 플러그인 kind 는 쓰지 않는다.
- **`records_recent`**(기본 false) — 이 kind 의 surface 를 파일로 열 때 host 가 "최근 연 파일" 목록에 kind 별로 기록할지. host 는 특정 kind 이름을 모르고 이 플래그로 기록 대상을 판정한다(generic per-kind). plugin 은 host 의 generic `recent.query {kind}` IPC 로 자기 최근 목록을 조회한다(예: markdown 주소창 드롭다운). 예: markdown 은 `true`.
- **`convert_requires_input`**(기본 false) + **`convert_input_popup`** — 이 kind 로 convert 하려면 host 가 먼저 "파일 입력 팝업"을 띄워야 하는지, 그리고 그때 열 이 plugin 의 팝업 **local id**. host 는 kind 이름·event key 하드코딩 없이 이 데이터만 따라 `<plugin_id>/<popup_id>` 팝업을 `open_popup_instance` 로 연다(payload 의 `surface_id` 로 제자리 변환 / 새 탭 분기). 예: markdown 은 `convert_requires_input = true`, `convert_input_popup = "file-open"`([파일 핸들러 (File handler)](../features/file-handler/index.md)). 미선언이면 빈 params 즉시 변환.

변환 입력 popup 요청은 pending popup queue로 전달해 렌더 중 직접 상태를 변경하지 않는다.

파일 surface가 열고 있는 파일 경로를 플러그인이 바꿨다면(다른 이름으로 저장 등) 호스트에도 알린다. 호스트는 그 surface의 탭 제목(`name_from_param`), 레이아웃 저장·복원 경로, `image.list` 같은 조회 결과를 자기 기록에서 읽는다. 전용 setter는 없다. 그 kind의 제자리 변환 메서드(image는 `image.open`, markdown은 `markdown.navigate`)를 같은 `surface_id`와 새 경로로 호출한다. 호스트는 surface ID를 유지한 채 플러그인 surface를 닫았다가(`destroy_surface`) 새 경로로 다시 만든다(`create_surface`). 그래서 감시 대상도 새로 등록되지만 undo·확대 같은 문서 상태는 이어지지 않는다.

이 호출은 플러그인 처리 스레드(`paint`·`handle_ipc_method`)에서 직접 하지 않는다. 자기 surface를 다시 만드는 변환을 처리 스레드에서 기다리면 호출이 `host.call` 시한(60초)까지 끝나지 않고, 그동안 다른 플러그인도 응답 없음으로 재시작된다(image surface에 `image.open`을 다시 호출해 확인했다). image 플러그인은 `HostHandle`을 복제한 별도 스레드에서 호출하고 결과는 경고 로그로만 남긴다.
등록 때 local popup ID에 plugin ID를 붙이며 payload의 surface_id가 제자리 변환과 새 탭 열기를 구별한다.

> 대용량 파일 확인 게이트는 SurfaceKindDef 필드가 아니라 **플러그인 소유**다. 플러그인이 자기 프로세스에서 크기를 감지(`std::fs::metadata`)해 event 를 publish 하고, event trigger `[[contributes.popup]]`(아래 "도구 메뉴 항목 + popup")로 확인 팝업을 자가 렌더한다(예: markdown). host 는 파일 크기를 알지 않는다.

### 파일 핸들러 (detector + handler)

확장자 → surface 매핑. `[[contributes.detector]]`(확장자 규칙) + `[[contributes.handler]]`(`action = open_surface{surface_kind}`). 권한: `file_handler.define`(신규 detector) / `file_handler.extend:<id>` / `file_handler.handle:<id>`. handler `id` 는 short name — install 단계가 `<plugin_id>/<id>` 로 자동 prefix. priority 동순위면 owner tiebreak `user > plugin > host`. 예: [image](../surfaces/image/index.md)·[markdown](../surfaces/markdown/index.md).

### 훅 핸들러 (webhook/hook 트리거)

`[[contributes.hook_handler]]` — 인바운드 웹훅 또는 내부 hook 이벤트가 발사됐을 때 실행할 **IpcSequence**(고정 IPC 호출들 + 페이로드 값 치환)를 선언한다. 권한: `hook_handler.define`. plugin 은 `action.kind = "ipc_sequence"` 만 쓸 수 있고 셸(`shell_command`)은 타입 레벨에서 배제된다(host/user 전용). `source` 로 트리거 출처를 게이트한다 — `webhook`(외부 HTTP) / `hook`(내부 이벤트) / `any`. handler `id` 는 short name(`[a-z0-9-]{1,32}`) → install 단계가 `<plugin_id>/<id>` 로 자동 prefix. priority 동순위면 owner tiebreak `user > plugin > host`.

plugin 이 자기 훅 핸들러를 웹훅에 붙이려면 `webhook.register` 를 호출한다 — 이때 `network` 권한이 필요하고, plugin 은 **인라인 sequence 를 못 쓰고 자기 소유(`<plugin_id>/…`) 핸들러 id 만** 바인딩할 수 있다(임의 시퀀스는 owner=Local 전용 채널).

### 완료 판정 전략 (agent task `Custom` dispatch 완료 판정)

`[[contributes.completion_strategy]]` — `agent.task_create` 의 `TaskCommand::Custom.poll` 이 이름으로 참조할 수 있는 완료 판정 전략을 선언한다(자세한 모델·결정 사항은 [agent-runner](agent-runner.md)의 "완료 판정 전략 레지스트리" 참고). 권한: `completion_strategy.define`. `spec.kind = "poll"`(`poll_method`/`state_field`/`terminal_states` 등, `PollSpec` 과 1:1) 또는 `"push"`(`notify_via`: 자기 자신 또는 host 소유 훅 핸들러 id + 필수 `timeout_ms`). `poll_method` 와 `default_for_methods`(이 전략을 기본으로 사용할 IPC 메서드 목록)는 plugin 소유면 자기 namespace(`<plugin_id>.*`) 만 가리킬 수 있다. id 는 short name → install 단계가 `<plugin_id>/<id>` 로 자동 prefix.

### 상속 환경변수 제거 (`contributes.strip_inherited_env`)

Tasty를 다른 도구의 세션 안에서 띄우면 그 도구가 자기 자식에 넣은 세션 표지·비밀 변수가 Tasty 프로세스에 남는다. 플러그인은 이 변수를 Tasty가 띄우는 자식에게 넘기지 않도록 선언한다. 권한 토큰은 필요 없다.

```toml
[contributes.strip_inherited_env]
names = ["VENDOR_SESSION_ID"]                                   # 이름이 같으면 지운다
prefixes = ["VENDOR_PLUGIN_OPTION_"]                            # 이름이 접두사로 시작하면 지운다
values = [{ name = "AI_AGENT", prefixes = ["vendor_", "vendor/"] }]  # 이름이 같고 값이 접두사로 시작할 때만 지운다
```

- 적용 대상: 터미널 셸, surface 훅, 전역 훅, hook_handler 직접 실행, 작업 자식(Run·후처리·reduce custom 셸). 다섯 경로가 `tasty_utils::process::is_stripped_inherited_env` 하나로 판정한다.
- 적용 조건: 플러그인이 설치돼 있고 사용자가 비활성화하지 않았으면 적용한다. 프로세스 기동·hello 완료나 기동 실패로 인한 자동 비활성화와는 관계없다. 호스트는 부팅 때 엔진이 첫 셸(복원 셸 포함)을 만들기 전에 설치 목록을 읽어 규칙을 반영하므로 첫 자식부터 적용된다. 번들 갱신도 그보다 먼저 끝나 갱신된 매니페스트의 선언이 쓰인다.
- 변경 시점: 비활성화·제거하면 그 플러그인의 규칙이 빠지고, 다시 활성화하면 규칙이 돌아온다. 이미 실행 중인 자식의 환경은 바뀌지 않으며 이후 만드는 자식부터 적용된다.
- 사용자가 직접 넣는 설정과 이름공간을 공유하면 `prefixes` 대신 `names`나 `values`로 좁힌다.
- `TASTY_`로 시작하는 이름·접두사는 매니페스트 검증에서 거부한다. 바깥 Tasty 인스턴스 신원(`OUTER_IDENTITY_ENV`) 제거와 셸의 자기 신원 재주입은 본체가 정한다. `CMUX_*`도 본체가 지운다.
- 이름은 영문자·숫자·밑줄이며 숫자로 시작하지 않는다(1..=128자). `values`의 값 접두사는 비어 있을 수 없다.
- 예: [Claude 플러그인](../plugins/claude/index.md#tasty-자식에서-지우는-세션-변수), [Codex 플러그인](../plugins/codex/index.md#tasty-자식에서-지우는-세션-변수).

### agent task provider

플러그인이 IPC namespace `<p>` 를 소유하고 `agent.turn_report` 를 선언·허가받았으며 활성이면 agent task 의 `"provider": "<p>"` 를 받을 수 있다. 별도 매니페스트 선언은 없다. 플러그인은 `<p>.spawn`·`<p>.tell`·`<p>.state` 를 구현하고 훅에서 `agent.task_turn_report` 로 턴 시작·끝을 보고한다. 판정은 설치 목록과 설정만 보므로 프로세스가 준비되기 전에도 같다. 계약과 오류 문구는 [작업 러너 §agent task](agent-runner.md#agent-task), 권한은 [플러그인 권한](plugin-permissions.md)에 있다. 예: [Claude 플러그인](../plugins/claude/index.md), [Codex 플러그인](../plugins/codex/index.md).

### CLI 호출자 문맥 (`contributes.caller_context`)

다른 프로그램(에이전트 CLI 등)의 도구 호출 안에서 실행한 `tasty`가 그 프로그램의 어느 대화에서 왔는지 플러그인 명령에 알린다. 본체 CLI는 이 선언을 해석해 dynamic plugin 요청의 `_tasty_context`에 싣는다. 해석 규칙과 필드 표는 [CLI 구조](cli-structure.md#호출자-환경-caller_contextrs)에 있다. 권한 토큰은 필요 없다.

```toml
[[contributes.caller_context]]
provider = "vendor"                 # _tasty_context.provider 값
id_env = "VENDOR_THREAD_ID"         # 있으면 이 provider의 호출자
id_format = "uuid"                  # "text"(기본, 비어 있지 않은 UTF-8) | "uuid"
id_field = "thread_id"              # ID를 실을 키
path_env = [{ env = "VENDOR_HOME", field = "vendor_home" }]  # 절대 경로로 실을 변수
executable = "vendor"               # PATH에서 찾아 execution으로 싣는다(선택)
suppress_env_surface = true         # 기본값. 호출자가 있으면 TASTY_SURFACE_ID로 surface를 채우지 않는다

[contributes]
requires_caller_context = ["vendor"]  # 다른 플러그인이 선언한 provider의 형식 오류를 이 플러그인 명령도 거절한다
```

- 적용 범위: 유효한 호출자의 문맥은 모든 플러그인의 dynamic 명령에 실린다. 형식 오류 거절은 선언한 플러그인 명령과 `requires_caller_context`에 그 provider를 적은 플러그인 명령만 한다. 무관한 플러그인 명령은 값이 없을 때처럼 동작한다.
- 적용 시점: CLI가 설치된 매니페스트를 직접 읽으므로 플러그인 프로세스가 준비되지 않아도 첫 호출부터 적용된다. 비활성화한 플러그인의 선언도 읽는다. 문맥을 받은 쪽이 provider가 꺼져 있음을 알리고 다른 surface로 대신하지 않게 하기 위해서다.
- 검증: `provider`는 소문자로 시작하는 소문자·숫자·`_`·`-`(64자 이하)이며 한 매니페스트 안에서 겹치지 않는다. 환경 변수 이름은 영문자·숫자·밑줄이고 `TASTY_`로 시작할 수 없다. 필드 이름은 소문자·숫자·`_`이며 `provider`·`implicit`·`execution`은 쓸 수 없고 서로 겹치지 않는다. `executable`은 경로가 아닌 프로그램 이름이다.
- 받는 쪽은 값을 다시 검증한다. 환경 변수일 뿐 인증된 신원이 아니다.
- 예: [Codex 플러그인](../plugins/codex/index.md#codex-부모에게-결과-반환), [Claude 플러그인](../plugins/claude/index.md).

### 셸 통합 확장 스크립트

Tasty가 주입하는 bash·zsh 프롬프트 함수는 OSC 133 D·A를 보낸 뒤 `$TASTY_PARENT_HOME/shell-integrations/`의 `*.sh`를 읽는다. 플러그인은 이 디렉터리에 스크립트를 써서 Tasty 터미널에만 셸 함수 같은 통합을 넣는다. 매니페스트 선언은 없다.

- 위치: 플러그인 프로세스의 `tasty_utils::path::tasty_home()` 아래 `shell-integrations/`. 셸은 자기를 띄운 인스턴스의 데이터 홈(`TASTY_PARENT_HOME`)을 읽으므로 debug·release 인스턴스의 스크립트가 섞이지 않는다.
- 파일 이름: `<플러그인 ID>.sh`(예: `com.tasty.codex.sh`). 다른 플러그인과 겹치지 않게 하며, 여러 개가 필요하면 `<플러그인 ID>.<이름>.sh`를 쓴다.
- 읽는 순서와 시점: 일반 파일만 C 로캘 바이트 순서로, 매 프롬프트마다 읽는다. 이미 열린 셸도 다음 프롬프트부터 새 파일을 읽으므로 스크립트는 여러 번 읽혀도 같은 결과가 나와야 한다(예: `if ! typeset -f name`).
- 오류: 한 스크립트가 0이 아닌 값으로 끝나거나 문법 오류가 나도 다음 스크립트와 프롬프트는 계속된다. 사용자가 errexit(bash `set -e`, zsh `setopt err_exit`)를 켰어도 같다. 각 스크립트는 errexit가 적용되지 않는 문맥(`source … || :`)에서 읽힌다. 따라서 스크립트 안의 실패한 명령도 셸을 끝내지 않는다. D는 스크립트를 읽기 전에 직전 명령의 종료 코드를 보낸다. 셸을 끝내는 `exit`은 막지 않는다.
- 셸 옵션: 목록을 모을 때만 glob 관련 옵션(bash nullglob·failglob·noglob, zsh는 `emulate -L zsh`)과 로캘을 바꾼다. 스크립트를 읽기 전에 원래대로 돌려놓는다. 사용자가 `set -f`(noglob)를 켰어도 스크립트는 모두 읽힌다. 스크립트는 사용자의 원래 옵션으로 실행된다.
- 사용자 설정을 덮어쓰지 않는다. 같은 이름의 사용자 함수·별칭이 있으면 정의하지 않는다.
- 설치·제거는 플러그인 명령이 맡는다. 호스트는 비활성화된 플러그인의 스크립트를 걸러내지 않으므로 스크립트가 실행하는 도구가 없을 때 원래 명령으로 넘어가게 작성한다.
- 매니페스트 선언 대신 파일 규칙을 쓰는 이유: 스크립트 내용이 설치 시점의 실행 파일 경로에 따라 달라지고, 셸 프롬프트가 호스트에 묻지 않고 바로 읽을 수 있어야 하기 때문이다.
- 예: [Codex 플러그인](../plugins/codex/index.md)의 `tasty codex install`.

### 도구 메뉴 항목 + popup

- `[[contributes.tool]]`(`ui.tool_item`) — [도구 메뉴](../features/tools-menu/index.md)에 항목. `action.kind`: `event`(Event Bus 발화) / `open_surface`(탭 추가) / `open_popup`(`popup_id = <plugin_id>/<id>`). `order_hint` 오름차순(빌트인 0..99).
- `[[contributes.popup]]`(`ui.popup`) — trigger `event`(자동 open) 또는 `ipc`(명시 호출). `scope` 로 소속 범위를 선언한다 — `window`(기본, 필드 생략 시) 또는 `surface`. `surface` 는 host 진입점(변환 입력 popup · 도구 메뉴)이 대상 surface 를 바인딩하거나, event trigger popup 을 연 이벤트가 `EventScope::Surface` + payload `surface_id` 로 **자기 plugin 의 surface** 를 가리킬 때 효과가 있다. 다른 plugin 의 surface·host surface 를 가리키거나 다른 plugin 이 발행한 이벤트면 host 가 popup 을 열지 않는다. plugin 이 IPC 로 연 popup 과 대상 없는 이벤트는 `window` 로 뜬다([design/systems/popup.md](../design/systems/popup.md) §plugin popup 의 스코프). SDK 콜백 `open_popup`/`paint_popup`/`on_popup_closed`(egui-mesh). 동일 `popup_id` 라도 `instance_id` 가 다르면 별개 인스턴스. 예: [git-viewer](../plugins/git-viewer/index.md)·[clipboard-viewer](../plugins/clipboard-viewer/index.md).

### CLI + IPC namespace

`[[contributes.ipc_namespace]]`의 prefix로 IPC를 받고 `[[contributes.cli]]`로 CLI 명령을 기여한다.
플러그인의 `handle_ipc_method`가 `<prefix>.*`를 처리한다.

- prefix는 소문자·숫자·`_`를 사용하고 `RESERVED_IPC_PREFIXES`의 호스트 예약어를 피한다.
  이 검사는 모든 매니페스트 load에 적용한다. `image`·`markdown`은 기존 번들이 공유하는 예외다.
- 공유 prefix 안의 호스트 메서드는 해당 번들의 inbound handler가 `host.call`로 되돌려 준다.
  다른 곳에서 같은 이름을 호출한다는 사실만으로 이 위임이 구현됐다고 판단하지 않는다.
- GUI 호스트가 직접 변환하는 이름(`host_converts`, 지금은 `image.open`)은 외부 호출이든 다른 플러그인 호출이든 번들을 거치지 않는다.
  번들의 trampoline은 이 변환이 없는 헤드리스용으로 남는다.
- 처리 스레드(`paint`·`handle_ipc_method`)에서 자기 surface를 다시 만드는 호스트 호출을 기다리지 않는다.
  호스트는 옛 surface의 회수 확인을 그 처리 스레드에서 받아야 하므로 서로 기다리다 `host.call` 시한(60초)까지 멈춘다.
  위 "Surface kind" 절의 경로 알림처럼 별도 스레드에서 부르거나, 그런 메서드를 trampoline하지 않고 호스트가 직접 받게 한다.
- CLI top-level 이름과 alias 충돌은 실제 clap 명령 집합으로 등록 때 검사한다.
  충돌한 CLI 이름만 건너뛰고 warning을 남긴다. 플러그인 전체를 거절하지 않는다.
- `[[contributes.cli]]`의 선택 필드 `help_group`은 루트 도움말에서 이 명령을 표시할 그룹이다.
  `basic`·`agents`·`background`·`automation`·`files`·`settings` 중 하나이며, 생략하거나 모르는 값이면
  "플러그인 명령" 그룹에 표시한다. 하위 명령의 선택 필드 `internal = true`는 훅·도우미가 호출하는
  내부 연동 명령을 그 plugin 도움말의 마지막 그룹으로 분리한다. 두 필드 모두 도움말 표시에만 쓰이며
  실행·권한·IPC에는 영향이 없다. 옛 매니페스트는 그대로 읽힌다([CLI 구조](cli-structure.md#도움말-표시-계약)).
- CLI의 `ipc_method`는 자기 prefix에 속해야 한다. `commands`는 팔레트·단축키 기여 목록이며 전체 IPC 허용 목록은 아니다.
- 관례 메서드 `<prefix>.lifecycle_busy`(선택): 호스트는 `plugin.disable`·`plugin.remove`·`plugin.upgrade_builtins`(`restart_running`)로
  떠 있는 플러그인을 내리기 전에 첫 prefix의 이 메서드를 `{}`로 묻는다. `{"busy": true, "reason": "<사유>"}`로 답하면 요청은
  `-32023`으로 거절되고 아무것도 바뀌지 않는다. 그 밖의 답·오류·모르는 메서드·1.5초 안에 답 없음은 바쁘지 않음이다.
  호출자가 `ignore_busy: true`(CLI `--ignore-busy`)를 주면 묻지 않는다. 답을 기다리는 동안 요청은 요청 게이트 줄에 머문다.

선언된 인자 타입은 문서용 힌트가 아니다. flag와 stdin에서 받은 잘못된 타입은 요청 전체를 거절한다.
실패한 변환을 None으로 바꾸어 현재 대상이나 기본값으로 실행하지 않는다.
CLI에 직접 준 key는 stdin 값보다 우선하고 오류는 인자명과 잘못된 값을 설명한다.

string 인자의 `path_kind`는 CLI가 호출한 디렉터리 기준으로 경로를 바꾼 뒤 보낸다. 플러그인 프로세스와 호스트는 사용자가 명령을 친 디렉터리를 모르기 때문이다.

- `directory` — 절대 경로로 바꾸고 디렉터리인지 검사한다.
- `file` — 절대 경로로 바꾸고 정규 파일인지 검사한다.
- `url_or_file` — `scheme://`로 시작하는 값(`https://…`, `file://…`)은 그대로 보내고, 그 밖의 값은 `file`처럼 다룬다. URL과 로컬 파일을 같은 인자로 받는 `tasty html open`이 쓴다.
- 검사에 실패하면 요청을 보내지 않고 인자명과 함께 오류를 낸다. 파일 내용은 검사하지 않는다.
- 예외: 아래 `remote_attach.methods` 에 선언한 메서드가 숫자 `--workspace` 나 `--pane` 으로 attach mirror 를 가리키면 경로 인자를 바꾸지 않고 그대로 보낸다. 경로는 실행할 서버 기준이라 서버 plugin 이 절대 경로인지·있는지 확인한다. CLI 는 보내기 전에 `attach.agent_target` 으로 mirror 인지 묻고, 묻지 못하면 로컬로 다룬다.

### 원격 attach 실행 (`contributes.remote_attach`)

attach holder 가 서버에서 이 plugin 의 메서드를 실행하게 하려면 선언한다. host 는 provider 목록을 갖지 않고 이 선언으로만 허용한다. 계약은 [Attach 메커니즘](attach-behavior.md#원격-자식-에이전트-ipcstreamagent-children)에 있다.

```toml
[contributes.remote_attach]
methods = ["spawn", "tell", "kill"]   # 서버에서 holder 대신 실행할 자기 namespace 메서드(접두사 없이)
receive = "remote_receive"            # client 에서 원격 결과와 late_spawn 을 받을 메서드
relay_meta = ["my-meta-key"]          # 원격 자식에서 client 의 같은 surface 로 옮길 메타 키
```

- `ipc_namespace` 가 정확히 하나인 plugin 만 선언할 수 있다. 메서드 이름은 그 namespace 안의 이름이다.
- 서버 host 는 원격 호출에 `parent_external = attach:o<n>`·`attach_context`·`attach_target_surface` 를 넣는다. 일반 namespace 요청이 이 값을 실으면 host 가 거절하므로, plugin 은 `attach:` 부모와 `attach_context` 가 함께 있을 때만 원격 출처로 본다. 같은 plugin 의 공개 메서드가 `attach:` 부모의 자식을 바꾸지 못하게 막는 것은 plugin 몫이다.
- 원격 자식에서 다시 발화할 훅 이벤트는 `[[contributes.hook_events]]` 선언으로 정한다. `process-exit` 는 선언 없이 옮긴다.
- plugin 고유 결과는 서버 plugin 이 `attach.child_notice{surface_id, key, payload}` 로 맡기고, client 의 `receive` 메서드가 `kind: notice` 로 받는다. `rejected` 는 다시 보내도 받을 수 없다는 뜻이므로 재시도를 멈춘다.

namespace 소유권은 설치 manifest에서 만든 NamespaceTable로 조회한다.
관리자와 IPC가 같은 Arc 핸들을 공유하며 부팅 때 한 번 설치한다. 조회 callback이나 별도 prefix 복사본은 두지 않는다.
표 갱신은 잠금 밖에서 계산한 뒤 한 번 교체하고 조회는 소유 여부를 값으로 돌려준다.
비활성화·재시작은 소유를 유지하며 패키지 제거가 소유를 없앤다.
허용된 호출은 소유자와 실제 매칭되는 활성 pre/post IPC extension만 준비한다.
self-loop·backoff로 건너뛸 hook은 시작하지 않고 소유자 기동 실패 때도 extension을 시작하지 않는다.

### 단축키 (commands)

`[[contributes.commands]]` — `id` · `default_keybinding` · `binding_mode`(`independent` 또는 `inherit:<host_action>`) · `scope`(`global`(기본) 또는 `surface`) · `action`(선택, `[[contributes.tool]].action` 과 동일한 `ToolAction`).

**scope 별 발화 조건**:

- `scope = "global"`(기본값) — **어디서나 동작한다.** 포커스된 surface 가 무엇이든(다른 plugin surface, 터미널 tab, 아무 surface 도 없는 상태 포함) 등록된 키를 누르면 발화. 단일 키는 다른 곳(터미널 입력 등)과 충돌하기 쉬우므로 **조합키만 권장** — `default_keybinding` 이 modifier 없는 단일 키면 매니페스트 validate 단계에서 거부된다(`scope = "surface"` 로 바꾸거나 조합키를 쓸 것).
- `scope = "surface"` — 이 플러그인이 만든 surface(`RemoteSurface`)에 포커스가 있을 때만 발화. 단일 키(F5 등)도 허용.
- 포커스된 plugin surface 가 있으면(어떤 plugin 이든) 그 plugin 의 커맨드가 scope 무관하게 최우선 후보가 된다 — "그 plugin surface 가 포커스되어 있다"는 조건 자체가 `surface` scope 의 조건을 이미 만족하기 때문. 포커스된 plugin surface 가 없을 때는 등록된 모든 plugin 의 `global` 커맨드만 후보가 된다. 호스트 `KeybindingSettings` 와 같은 키가 겹치면 **plugin 이 항상 우선**(자세한 우선순위 규칙은 [`key-mapping.md`](../design/policies/key-mapping.md#plugin-커맨드-단축키-우선순위)).

**동작 방식(`action` vs `handle_command`)**: `action` 을 선언하면 호스트가 `[[contributes.tool]]` 과 동일하게 그 액션(`event`/`open_surface`/`open_popup`)을 직접 실행하고, 옛 `command.invoke` IPC(SDK `handle_command`)는 이 커맨드에 대해 발사되지 않는다 — popup 을 여는 것뿐인 커맨드라면 `handle_command` 를 구현할 필요가 없다. `action` 을 선언하지 않으면 기존처럼 `command.invoke` → SDK `handle_command` 왕복 경로를 쓴다. Event Bus `command.invoked` owner-unicast 통지는 `action` 유무와 무관하게 항상 발사된다(관찰용, 구독 안 해도 무방).

### 설정 페이지

`[[contributes.settings_pages]]`(`ui.settings_page`) — [설정 창](../features/settings/index.md)에 sub-tab 동적 등록. `category`(appearance/general/keybindings/plugin/…). 플러그인 비활성 시 sub-tab 자동 소멸. 예: [markdown](../surfaces/markdown/index.md).

`[[contributes.settings_pages.items]]` 의 `kind` (공통 필드: `id` · `label_key` · `storage_key` — `group` · `text` 는 `storage_key` 가 선택):

- `font_override` — surface 폰트 override. host 가 `plugin_font_overrides.<storage_key>` 슬롯에 read/write (아래 generic 컨트롤과 **별개 전역 네임스페이스**).
- `toggle` — on/off. `default`(bool) · `caption_key`(선택, 설명 글 i18n 키). host 는 Switch 로 렌더, bool 저장. `caption_key` 가 있으면 행 아래에 설명을 둔다(설정 행 caption 과 같은 모양). 설명은 줄(`\n`)마다 나뉘고, `- ` 로 시작하는 줄은 목록(•), `  - ` 로 시작하는 줄은 한 단계 안쪽 목록(–)이다. 항목이 줄을 넘기면 글머리표 뒤 자리에서 이어진다. 목록 줄이 없는 설명은 한 덩어리 글로 그린다.
- `select` — 드롭다운. `options = [{ value, label_key }]` + `default`(반드시 options.value 중 하나). Select 로 렌더, 선택 value(문자열) 저장.
- `number` — 수치. `default`(f64) · `min`/`max`(선택; 주어지면 min≤default≤max) · `suffix_key`(선택, 단위 i18n 키). 설정 창의 [숫자 한 모양](../features/settings/screens/settings.md#숫자-입력-한-모양)으로 렌더(mono Input + 필드 밖 정적 suffix, **확정 때만** min/max 로 끌어온다), f64 저장.
- `text` — 글. `default`(문자열) · `default_key`(선택, 기본 글 i18n 키) — 둘 중 하나는 있어야 한다. `editable`(bool, 기본 `false`). 읽기 전용이면 host 는 라벨을 위에 맞추고 컨트롤 열에서 줄을 바꾼 고칠 수 없는 글(선택·복사는 된다)로 그린다. 보이는 값은 `storage_key` 의 저장 문자열(앞뒤 공백 제외, 공백뿐이면 없는 것), 없으면 `default_key` 번역, 그것도 없으면 `default` 다. 이때 host 는 이 칸에 쓰지 않는다. `editable = true` 면 host 는 컨트롤 열에 여러 줄 입력(CodeArea)을 그리고 고칠 때마다 `storage_key` 에 문자열로 저장한다. 고칠 글은 저장 문자열 그대로(빈 값 포함)이고 저장값이 없으면 기본 글에서 시작한다. 비운 칸은 기본 글을 자리표시로 보이며, plugin 은 빈 값을 기본 글로 읽어야 한다. 검증: `editable` 이면 `storage_key` 가 있어야 한다. 예: [agent-session](../surfaces/agent-session/index.md#설정)의 현행화 문구.
- `group` — 묶음 머리. `storage_key`(선택) · `default`(bool, 기본 `true`). `toggle`/`select`/`number`/`text` 항목은 `group = "<묶음 id>"` 로 그 묶음에 든다. host 는 묶음 머리 줄(위 구분선 · 진한 라벨)을 그리고 그 아래에 묶음 항목을 선언 순서로 모은다. 묶음 밖 항목과 묶음 머리는 선언 순서대로 놓는다. `storage_key` 가 있으면 머리에 Switch 를 두고 `toggle` 과 같은 칸에 bool 로 저장하며, 끄면 묶음 항목을 숨긴다. 스위치는 그 기능을 끄는 값이므로 plugin 이 직접 읽어 기능을 꺼야 한다 — 숨긴 항목의 저장값은 그대로 남는다. `storage_key` 가 없으면 스위치 없는 머리이고 항목은 늘 보인다. `caption_key`(선택)가 있으면 머리 줄 아래에 설명 글을 두고, 스위치를 꺼도 남는다. 검증: 항목의 `group` 은 같은 페이지의 `group` 항목을 가리켜야 하고, 스위치 없는 묶음은 항목이 하나 이상이어야 하며, 머리 스위치의 저장 키는 같은 페이지의 다른 항목과 겹치면 안 된다. 묶음 id 는 다른 항목 id 와 같은 형식 · 중복 규칙을 따른다. 묶음은 중첩하지 않고 `font_override` 는 묶음에 들지 않는다. 예: [agent-session](../surfaces/agent-session/index.md#설정).

`toggle`/`select`/`number` 값은 `plugin_settings.<plugin_id>.<storage_key>` 슬롯(`PluginSettingValue` = Bool/Text/Number)에 저장·영속된다 — `font_override` 의 전역 슬롯과 충돌하지 않는 plugin-scoped 네임스페이스. 예: [html](../surfaces/html/index.md) 이 HTML viewer 설정(zoom/color scheme/allow remote content/sandbox scripts)을 이 방식으로 노출. webview kind 를 등록한 plugin 이면 이 저장 키들을 host 가 WebView 설정으로 적용한다([WebView 호스트 계약 "설정 출처"](../design/systems/webview.md#설정-출처--kind-를-등록한-plugin)).

### 이벤트 구독 / 윈도우 / 확장

- **event_subscribe** — `event_subscribe = ["surface.closed"]` + `on_start` 에서 `bus.subscribe(...)`. `on_event` 로 envelope 수신(`reason`: user/ipc/crash). 예: [claude](../plugins/claude/index.md)/[codex](../plugins/codex/index.md).
  - **공통 새로고침 받기**: 사용자의 새로고침 키를 자기 surface 의 갱신에 쓰려면 `event_subscribe` 에 `surface.refresh_requested` 를 넣고 `on_start` 에서 구독한다. 신호는 구독자 모두에게 가므로 `on_event` 에서 payload 의 `surface_id` 가 자기가 연 surface 인지(필요하면 `kind` 도) 확인한 뒤에만 갱신한다. 신호는 사용자 요청이므로 사용자 출처로 처리한다. 예는 markdown plugin 의 `on_refresh_requested` 이고, 계약은 [공통 새로고침](../features/keybindings/index.md#공통-새로고침--refresh)에 있다. 단축키를 따로 `[[contributes.commands]]` 로 등록할 필요는 없다. 같은 키를 plugin 명령으로 등록하면 plugin 명령이 먼저 가져가 신호가 나지 않는다.
  - **`event.dispatch` 에 응답하라.** 호스트는 그 응답을 기다리지 않지만, 응답이 오기 전까지 그 dispatch 의 hop 을 기억해 그 사이 이 plugin 이 publish 하는 사건의 hop 하한으로 쓴다. 응답하지 않으면 그 기억이 재시작 전까지 남아, hop 이 높은 사건을 한 번 받은 뒤의 publish 가 무엇이든 `MAX_HOP` 으로 거절될 수 있다. SDK 는 `on_event` 를 마친 뒤 자동으로 응답한다 — SDK 없이 프로토콜을 직접 구현할 때의 계약이다. 근거는 [Event Bus 1.0 카탈로그](../reference/event-catalog.md).
- **window** — `[[contributes.window]]`(`window.spawn`). 현재는 schema + 등록 stub 까지(실 spawn 은 별도 영역).
- **extension** — 다른 플러그인의 IPC/event 흐름을 가로채기. `[extends]` + `ext:<target>` 권한 + `handle_extension_hook`. mode: `transform`/`filter`/`observe`. target 당 활성 1개(나머지 `Conflict`). fail-open(timeout/에러 시 원래 값 사용).

### 열린 파일의 외부 변경 감지 (SDK `file_watch`)

파일을 여는 surface 를 가진 plugin 은 그 파일이 **밖에서 바뀌었을 때** 스스로 갱신해야
한다. host 는 무조건 tick 을 주지 않는다 — webview kind 는 `paint`/`set_context` 를 아예
안 받고, egui-mesh kind 도 입력·geom·theme·focus·invalidated 중 하나가 있어야 forward
된다. 즉 **idle 상태에서는 감시 스레드가 유일한 자동 갱신 경로다.**

공용 감시 루프는 SDK의 `file_watch` 모듈에 있다. plugin 이 정하는 것은 둘뿐이다.

1. **변경 검사**(`EntryProbe`) — 파일의 어떤 값을 비교할 것인가.
2. **reload 메서드 이름** — 변경을 알릴 자기 네임스페이스 메서드.

```rust
use tasty_plugin_sdk::file_watch::{self, ContentDigest, WatchCmd};

// on_start 에서
let (tx, rx) = std::sync::mpsc::channel();
self.watch_tx = Some(tx);
std::thread::Builder::new()
    .name("myplugin-watch".to_string())
    .spawn(move || file_watch::run::<ContentDigest>(host, rx, "myplugin.reload"))?;

// create_surface / destroy_surface 에서
tx.send(WatchCmd::Register { surface_id, path })?;
tx.send(WatchCmd::Unregister { surface_id })?;
```

폴링 주기(`RELOAD_CHECK_INTERVAL_SECS`)는 **공용이다** — "밖에서 고친 것이 얼마 만에
보이나" 는 사용자에게 하나인 물음이라 plugin 마다 다른 답을 두지 않는다.

**감시 스레드는 파일을 읽어 상태를 고치지 않는다.** 변경을 감지하면 `self_invoke` 로
자기 reload 메서드를 부르고, 실제 read 와 재생성은 그 메서드 하나로 수렴한다 — 그래야
빠른 연속 편집에서 "stale read 가 최신 것을 덮어쓰는" 레이스가 생기지 않는다(쓰기 경로가
하나뿐). `host.call` 로는 안 된다 — 호스트는 caller 가 네임스페이스 owner 자신이면
forward 하지 않아 항상 `-32601` 이 떨어진다.

#### 판정자 고르기 — 기준은 "읽기 비용에 상한이 있는가"

- **`ContentDigest`(SDK 제공)** — 매 폴 전량을 읽어 내용 지문을 견준다. mtime 해상도 때문에 같은 시각의 변경을 놓치는 문제를 피한다. 64비트 해시이므로
  충돌 가능성까지 없애지는 않는다. **입력 크기에 상한이 있을 때만** 이 교환이 성립한다(markdown 문서가 그렇다).
- **시계(mtime)만** — 싸지만 두 쓰기가 같은 mtime 눈금에 떨어지면 뒤엣것을 **영구히**
  놓친다. 그 창은 파일시스템이 정한다(그 타입 문서에 실측값이 있다).
- **2 단 판정(`stat` 게이트 → 지문)** — 파일 크기에 상한이 없을 때. 싼 `stat` 으로 먼저 거르고
  움직였을 때만 읽는다. `EntryProbe` 가 값 반환이 아니라 trait 인 이유가 이것이다 —
  "안 읽고 통과" 를 반환값으로는 말할 수 없다.

## 4. Plugin UI 렌더 (egui-mesh 채널)

`rendering = "egui-mesh"` surface 와 popup/banner 는 egui-mesh 채널 하나로 통한다 —
plugin 이 자기 프로세스에서 egui 를 구동해 tessellate 한 `(ClippedPrimitive,
TexturesDelta, ppp)` 를 SharedBuffer 로 host 에 보내고 host 가 합성한다. 위젯 어휘
제한이 없고(egui 전부 사용 가능) 색·간격은 host 가 forward 한 `Theme` 토큰에서
가져온다. 상세·SDK 헬퍼(`EguiMeshSurface`/`EguiMeshPopup`/`EguiMeshBanner`)는
[egui-mesh-channel](egui-mesh-channel.md). 배너 콘텐츠는 SDK가 프레임 전체에 배너 문맥을 열어 주므로
공용 Secondary 버튼이 따로 감싸지 않아도 배너 박스로 그려진다. 같은 plugin의 surface·popup에는
적용되지 않는다. 배너 본문 글은 `tasty_ui_widgets::plugin_banner_body`로 그린다. 세 줄과 콘텐츠 높이에 맞는 줄 수 중
작은 값 뒤에서 말줄임하고, 잘렸을 때만 호스트가 배너 카드 아래 툴팁에 전체 문구를 보이는 디자인 규칙([배너 본문 줄 수](../design/systems/banner.md#본문-줄-수))을 따른다. 툴팁은 plugin이 그리지 않는다. (`rendering = "webview"` surface(html/markdown)
의 본문은 이 채널을 타지 않는다 — host native WebView 가 직접 렌더한다. 단 markdown
의 대용량/파일열기 확인 팝업 2 개는 여전히 egui-mesh 채널을 쓴다.)

**chrome 아이콘**(툴바·주소창 등)은 raw 유니코드 글리프로 그리지 말고 `tasty-icons`
canonical 아이콘을 쓴다. plugin `build.rs` 가 `[build-dependencies] tasty-icons`(egui off)
+ usvg 로 `Icon.svg` 를 평탄화해 점배열을 `OUT_DIR` 에 베이크하고, 런타임엔
`tasty_plugin_sdk::baked_icon::draw(painter, icon, center, size, color)` 로 텍스처 없이
DPI 독립·theme tint 벡터 stroke 로 그린다. 새 아이콘 = `tasty-icons` 에 const 추가 +
plugin `build.rs` 의 `ICONS` 목록에 한 줄. 근거·대안은 [테마 시스템 (운영 상세)](../design/systems/theme.md).

## 5. 호스트 IPC 호출

`HostHandle::call("surface.list", json!({}))` — 매니페스트에 해당 권한 선언 + grant 필요. `?` 한 번으로 `PluginError → IpcMethodError` 변환. 주요 에러 variant: `HostCall{message}`(permission_denied 등) · `HostCallTimeout` · `HandshakeRejected/Timeout` · `HostClosed`. 권한↔메서드 매핑은 [plugin-permissions](plugin-permissions.md).

## 6. 데이터 저장 위치

| 데이터 | 위치 | 비고 |
|--------|------|------|
| 정적 자산(아이콘/lang/README) | `TASTY_PLUGIN_DIR` | **읽기 전용** — 업그레이드 시 통째 교체 |
| 사용자 번역 | host의 `lang/` 아래 plugin 오버라이드([i18n](i18n.md#사용자-plugin-번역)) | 설치본 lang는 수정하지 않음, SDK는 `TASTY_PARENT_HOME` 사용 |
| 사용자 편집 설정 | `TASTY_PLUGIN_CONFIG_PATH` | 업그레이드 보존 |
| DB·캐시·로그 | `TASTY_PLUGIN_DATA_DIR` | **쓰기 OK**, 업그레이드 보존 |
| 작업 메타/진행 상태(≤1 MiB) | `memory.*` / `memory.secret.*` | host SQLite. cap 초과는 `ValueTooLarge` |
| **진짜 민감 데이터**(토큰/키/자격증명) | **OS keyring** | secret 영역 금지 — [아래 "민감 데이터"](#민감-데이터--regular--secret--keyring-선택) |

`memory.secret` 의 유일한 보장은 **플러그인 간 IPC 격리**다 — 디스크엔 평문. regular vs secret vs keyring 선택은 [아래 "민감 데이터"](#민감-데이터--regular--secret--keyring-선택).

**`TASTY_PLUGIN_DATA_DIR` 수명 계약** — 실경로는 `~/.tasty/plugin-data/<plugin-id>` 로 **설치 디렉터리(`TASTY_PLUGIN_DIR`)와 분리**돼 있고(`crates/tasty-host-plugin/src/process.rs`), 번들 plugin 재동기화(`upgrade-builtins`)는 설치 디렉터리 내용만 mirror 하므로(`crates/tasty-host-plugin/src/builtin.rs`) data dir 을 건드리지 않는다. 즉 §9.1 의 `disable` → `upgrade-builtins` → `enable` 절차나 plugin 업그레이드·재설치를 건너 **data dir 내용은 보존된다** — plugin 을 지우기 전까지 살아 있어야 하는 상태(claude plugin 의 checklist 라운드, 프로필 부착 기록 등)를 여기 두어도 안전하다. 반대로 설치 디렉터리에 쓴 것은 다음 업그레이드에 사라진다.

### 민감 데이터 — regular · secret · keyring 선택

플러그인 안에서 비밀번호 / OAuth refresh token / API 결제 key 같은 민감 데이터를 어떻게 저장해야 하는지. 저장 위치 전반은 위 [§6 표](#6-데이터-저장-위치).

#### 핵심: secret 영역은 "안전 보관소"가 아니다

`memory.secret.*` 는 이름이 오해를 준다. 실제 보호 수준:

| 상황 | 결과 |
|------|------|
| 플러그인 A 가 IPC 로 B 의 secret 요청 | **차단**(owner 분리, 존재조차 모름) |
| 사용자/host(CLI·GUI)의 secret 조회 | 허용(의도된 동작) |
| 플러그인이 `~/.tasty/memory.db` 직접 열기 | **평문 그대로 보임** |
| `~/.tasty/` 백업/cloud sync | **평문 그대로 들어감** |
| 디바이스 분실 + 디스크 암호화 없음 | **평문 노출** |

즉 secret 의 보장은 **"플러그인 간 IPC 격리" 한 가지**뿐.

#### 권고

##### ✅ secret 에 둬도 되는 것
다른 플러그인이 못 보게만 하면 충분한 것 — UI 옵션, API 응답 캐시, 작업 중 임시 컨텍스트. *디스크에 평문으로 있어도 사용자에게 큰 손해 없는* 데이터.

##### ❌ secret 에 두면 안 되는 것
**디스크에 평문으로 저장해도 되는가**로 판단 — master password, OAuth refresh/access token, 결제 정보·API 결제 key, 개인정보(의료/금융/식별), 타 서비스 자격증명.

이 종류는 플러그인이 **직접 OS keyring** 을 호출한다. Rust `keyring` 크레이트:

```rust
let entry = keyring::Entry::new("com.example.myplugin", "refresh_token")?;
entry.set_password(&token)?;
let token = entry.get_password()?;
```

- service 이름은 plugin id prefix, user 이름은 데이터 의미(`refresh_token`).
- 키체인 없는 환경(Linux headless 등)에서 실패 시 사용자에게 명시 알림 + 기능 disable. **평문 파일 폴백 금지.**

##### 큰 민감 데이터 — 외부 파일 + memory 링크
값이 큰 것(예: SSH 개인키): ① 파일은 적절한 외부 위치, ② memory 엔 *경로만*, ③ 파일 권한 OS-level 강하게(0600). memory 시스템 원칙("한계 넘는 데이터는 외부 파일 + 링크")과 동일.

#### sandbox 가 들어오면

OS 샌드박스는 아직 도입되지 않았다. 도입할 때는 파일·keyring 접근 제한과 기존
IPC 권한을 함께 검토해야 한다. 그 전까지 민감한 값은 위 keyring·파일 권한 규칙을 따른다.

## 7. 호스트 런타임 계약 (env · 생명주기 · 핸드셰이크)

플러그인 *작성* 과 별개로, 호스트가 플러그인 프로세스를 어떻게 띄우고 살려두는지 — SDK 가 의존하는 런타임 계약(`crates/tasty-host-plugin/`).

CLI 기여의 명령·하위 명령 `description_i18n_key`와 인자의 선택적 `help_i18n_key`는
plugin 카탈로그에서 해석한다. 키가 없으면 기존 `description`/`help`를 표시한다.
CLI도 설치 영어 → 선택 언어 → host 홈의 사용자 plugin 파일 순서로 읽으며, 매니페스트의
`lang_dir`을 따른다. [국제화](i18n.md#cli-도움말-clap-about--help) 참조.

### spawn 시 주입 환경변수

호스트가 자식 프로세스에 넘기는 env (`process.rs`). SDK 가 이걸로 자기 위치·로그·호스트 접속을 안다.

| 환경변수 | 값 |
|----------|-----|
| `TASTY_PARENT_HOME` | host 데이터 루트. 상대 홈은 host CWD 기준 절대경로로 확정한다. 같은 루트에서 파생하는 DATA_DIR·CONFIG_PATH도 절대경로이며, 확정 실패는 spawn 오류다 |
| `TASTY_PLUGIN_ID` | plugin id |
| `TASTY_PLUGIN_DIR` | host CWD 기준 절대 설치 디렉터리(읽기 전용). 자식의 초기 CWD도 같은 경로 |
| `TASTY_PLUGIN_DATA_DIR` / `TASTY_PLUGIN_CONFIG_PATH` / `TASTY_PLUGIN_LOG_PATH` | 데이터·설정·로그 경로 |
| `TASTY_HOST_IPC_PORT` | 호스트 listener 포트 |
| `TASTY_PLUGIN_TOKEN` | 핸드셰이크 토큰(1 회용) |
| `TASTY_HOST_API_VERSION` | 호스트 protocol 메이저 |
| `TASTY_PLUGIN_HANDLE_ENDPOINT` | handle 채널 엔드포인트(있을 때) |
| `TASTY_LOCALE` | 활성 로케일(`general.language`) — host 본 바이너리가 부팅 시 자기 프로세스 env 에 set 하고(`src/boot/locale.rs`) spawn 시 그대로 propagate 한다(host-plugin 은 `tasty-i18n` 비의존). SDK `Translator` 가 소비. spawn 시점 고정 — 언어 변경은 재시작 후 반영([국제화 (i18n)](i18n.md)) |
| `TASTY_LOCALE_FONT` | 언어팩이 제공하는 폰트 파일의 절대경로 — **언어팩 폰트가 resolve 됐을 때만** 주입(내장 폰트 · 미제공이면 미설정, 셸에서 상속된 값도 자식에서 제거). 출처와 고정 시점은 `TASTY_LOCALE` 과 같다 |
| `TASTY_LOCALE_FONT_FACE` | `TASTY_LOCALE_FONT` 파일 안에서 읽을 글꼴 번호. `TASTY_LOCALE_FONT` 와 함께만 주입한다 |
| `TASTY_HOST_PID` | 호스트 프로세스 PID (**macOS 만** — SDK watchdog 가 부모 사망 감지에 사용) |

설치 디렉터리는 실행 명령을 만들 때 host CWD 기준으로 확정한다. 설치본 entry 탐색,
자식 CWD, `TASTY_PLUGIN_DIR`가 같은 루트를 사용하므로 상대 `TASTY_HOME`에서도
실행파일과 설치본 `lang/`가 자식 CWD에서 다시 상대 해석되지 않는다. canonicalize를
쓰지 않아 존재 여부나 symlink 해소를 새 전제조건으로 삼지 않는다. 기존 entry 문법은
유지한다: 절대 command는 그대로, 설치 디렉터리에서 찾은 bare/상대 command는 그
설치본을 실행하며, 찾지 못하면 원 command의 OS 탐색/실패 의미를 유지한다.
SDK가 자기 CWD에서 절대화하여 이 경계를 대신하지 않는다.

### 생명주기 (healthcheck / 자동 재시작·비활성화)

번들 설치·패키지 등록·부팅 정리는 GUI와 headless의 명시적 부팅 경로에서 수행한다.
플러그인을 쓰지 않아도 필요한 작업을 첫 namespace 호출에만 의존시키지 않는다.
프로세스 시작은 다음 정책을 따른다.

| 상황 | 시작·대기 정책 |
|------|----------------|
| GUI 전체 기동 | 부팅 워커가 활성 plugin들을 시작하고 연결 결과를 받은 뒤 hello를 기다린다 |
| 허용된 namespace 호출 | 소유자와 실제 매칭되는 활성 IPC extension만 시작한다 |
| headless kind 생성 | `tab.create`·`pane.split`·`workspace.create`의 type 소유자가 활성일 때 그 하나만 시작한다 |
| headless attach 전체 기동 | 기존 전체 기동을 유지하며 메인 스레드에서 연결 결과까지 기다린다 |

headless도 hello의 webview·remote·egui-mesh kind를 등록한다. 새로 시작한 kind 소유자는 연결 한도까지,
연결 성공 후에는 `KIND_REGISTRATION_WAIT`(5 초)까지 hello 등록을 기다린다.
이미 실행 중인데 kind가 없으면 추가 대기를 하지 않는다. 이 경로는 설치·권한 승인을 수행하지 않는다.
Local 이외 호출자도 기존 생성 권한을 만족하면 같은 시작 정책을 적용받는다.

#### 기동과 연결

토큰을 listener에 등록한 뒤 자식을 spawn한다. 연결을 먼저 시도한 자식이 unknown token으로 거절되는 경합을 피한다.
연결 대기는 `plugin-connect-<id>`가 최대 `HANDSHAKE_TIMEOUT`(10 초) 동안 수행한다.
연결 전에도 running:true이며 송신 큐에 쌓은 요청은 연결 후 순서대로 보낸다.
요청 기한은 연결 완료부터 계산한다. 전체 기동의 여러 연결 대기는 겹쳐 진행된다.

`settle_connections`가 결과를 한 번 처리한다. 성공하면 실패 기록을 지우고 실패하면 자식을 kill·회수하며
`plugin.error`의 spawn_failed를 알린다. namespace 대기는 오류로 끝낸다.
연결되지 않은 extension에 보낸 hook은 실행되지 않은 것으로 건너뛰고 원래 흐름을 이어가며 hook 실패 횟수는 올리지 않는다.
pre-hook 우회 뒤에도 caller plugin ID를 target에 전달한다. 연결 스레드 생성 실패는 동기 대기로 돌아간다.
enable·swap 응답은 자식을 만들었다는 뜻이며 연결 완료를 보장하지 않는다. 대신 기동마다 세대(`start.generation`)를 붙여 결과를 그 세대에 귀속한다(`manager/start_status.rs`).

- `plugin.enable` 응답은 `start: {generation, state, failure}`를 싣는다. `state`는 `starting`·`connected`·`failed`·`cancelled`이고, 호출자는 같은 세대가 `starting`을 벗어날 때까지 `plugin.show`의 `start`를 읽는다. CLI `tasty plugin enable`은 이 대기를 15 초까지 하고, 연결하지 못하면 원인과 함께 0 이 아닌 상태로 끝난다. 3 초가 지나도 `starting`이면 기동 세대와 `tasty plugin show <id>` 확인 방법을 담은 진행 줄을 표준 오류에 한 번 쓴다. 표준 출력에는 최종 응답 JSON 하나만 쓴다. 후속 `plugin.show` 조회는 응답 한 줄을 읽는 전체를 남은 대기 시간의 절대 시각까지로 제한하고(`IpcConnection::send_reading_within`, 읽기마다 남은 시간을 다시 건다. 요청 쓰기는 이 상한 밖이다), 진행 안내 시점이나 상한에 걸린 연결은 버린다. 새 연결의 수립도 남은 시간으로 제한하므로 호스트가 답하지 않거나 응답을 조금씩 보내도 안내와 15 초 상한이 지켜진다. 대기하는 `enable`은 루프 명령이라 `--response-timeout-ms`를 거절한다(종료 코드 2). `--no-wait`는 `plugin.enable`을 한 번만 보내 접수 응답(`start`가 `starting`일 수 있다)을 출력하며 이때는 응답 시한을 그 요청에 싣는다. IPC 를 직접 쓰는 에이전트는 응답을 바로 받고 `plugin.show`로 진행을 본다. 이미 연결을 기다리는 프로세스가 있으면 새로 띄우지 않고 그 세대를 돌려준다. 회수를 기다리느라 아직 띄우지 않은 기동은 나중에 같은 세대로 띄운다.
- 연결 결과는 그 프로세스를 띄운 세대에만 기록한다. 앞 기동의 늦은 실패는 뒤 기동의 결과를 바꾸지 않는다.
- 결과가 나기 전에 disable·remove(disable 을 거친다)·swap 이 겹치면 그 세대는 `cancelled`로 끝난다. 대기 상태로 남지 않는다.
- `plugin.list`·`plugin.show`는 `auto_disabled`(자동 비활성화 여부), `last_failure: {kind, message, generation}`(마지막 기동 실패, 이후 연결해도 남는다), `start`(가장 최근 기동)를 함께 싣는다. `kind`는 `spawn_failed`(프로세스를 띄우지 못함)·`connect_failed`(핸드셰이크 전에 끝났거나 시간 안에 연결하지 않음)다. 기존 `enabled`·`running`의 뜻은 그대로다.
- 매니저는 실패를 호스트 알림 보고로 쌓고 GUI·헤드리스 pump 가 꺼내 `NotificationStore`에 남긴다(`app/plugin_failure_notice.rs`, gui feature 없는 헤드리스 빌드에도 들어간다, `source: "plugin"`). 서로 다른 사건이라 `NotificationStore::add_separate`로 넣어 병합 창 안에 이어 나와도 합치지 않는다. `notification.create`·OSC 알림의 병합(`add_with_level`)은 그대로다. 한 번의 실패는 경고, 자동 비활성화는 오류다. 같은 plugin 의 실패는 실패 창(10 초) 안에서 첫 실패 한 건만 경고로 남기고, 자동 비활성화는 따로 한 건 남긴다. 연결에 성공하거나 사용자가 다시 켜면 묶음이 끝난다. 알림을 남길 workspace 가 없으면 꺼내지 않으므로 부팅 중 실패는 첫 workspace 가 생긴 뒤 남는다. 사용자가 끈 plugin 은 기동하지 않아 보고가 없다. 알림은 plugin 을 ID 로 가리키고 매니페스트 문구를 싣지 않는다. Event Bus 의 `plugin.error`와 Plugins 창의 실행 오류 항목은 그대로다.

#### 무응답과 늦은 응답

- ping 간격은 15 초, healthcheck 무응답 기준은 60 초다. tick 판정까지 최대 75 초이며 실제 연결 종료는 별도로 감지한다.
- 10 초 안에 spawn 실패 3 회면 자동 비활성화한다. 10 초짜리 연결 실패만 반복해서는 이 창 안에 3 회가 쌓이지 않는다.
- namespace 요청 기한은 `2 × (HEALTHCHECK_TIMEOUT + PING_INTERVAL)`, 현재 150 초다. 만료는 모든 caller에 `-32004`로 답한다.
- namespace 연속 만료 3 회는 다음 ping tick에서 재시작한다. 실제 namespace 응답은 늦게 와도 계수를 지운다.
  최근 만료 ID와 일치하는 응답만 한 번 소비하며 pong·hook 응답은 이 계수를 지우지 않는다.
- 늦은 target 응답은 post-hook을 부르지 않고 늦은 pre-hook은 target을 다시 부르지 않는다. 늦은 post-hook도 두 번 회신하지 않는다.
  hook의 지연 실패 계수는 그대로다. `-32004`는 결과를 모른다는 뜻이며 실행 취소를 보장하지 않는다.

#### 종료와 재기동

disable·healthcheck 재시작은 shutdown 요청 후 전용 회수 스레드가 최대 2 초의 정상 종료 기회를 주고 필요하면 kill한다.
메인은 회수 중에만 50ms 타이머로 완료된 스레드를 정리하고 `plugin process retired`에 시간·이유를 남긴다.
새 인스턴스는 회수 후에 시작한다. 회수 중 disable은 재시작 예약을 지운다.
회수 스레드 생성 실패에는 기존 동기 대기가 남는다.

remove·swap·실제 쓰기가 있는 upgrade-builtins·명시적 enable은 해당 회수를 기다린다.
변경할 파일이 없는 upgrade는 기다리지 않는다. 쓰기 뒤 재시작 예약을 잃지 않아야 한다.
disable 응답은 종료 완료가 아니며 회수 중 호출에는 `-32002`가 올 수 있다.
호스트 종료는 회수 중 자식도 정리하고 다시 띄우지 않는다.

재시작·disable·swap·연결 실패는 `forget_plugin_runtime`으로 권한·subscription·pending·shared buffer·설정 페이지·등록 gate를 정리한다.
새 hello가 다시 등록되므로 plugin.loaded 등의 사건은 같은 plugin에 여러 번 올 수 있다.
회신 대상 없는 surface.create·restore·popup open pending도 보낸 plugin 기준으로 회수한다.

SDK shutdown은 ack 후 이미 worker 큐에 들어간 요청까지 처리한다. 백그라운드 HostHandle 때문에 큐가 닫히기를 기다리지 않는다.
수신 루프 종료 전후의 host.call 대기는 HostClosed로 끝낸다. main에서 run이 반환하면 자식 프로세스가 끝난다.

#### Surface kind 철회

disable·remove는 registry 정의를 삭제하지 않고 철회한다. 기존 surface의 저장·아이콘·이름은 get으로 읽고,
새 생성과 목록은 get_live·contains·kinds_snapshot에서 제외한다.
새 생성은 SurfaceKindWithdrawn으로 거절하며 사용자 동작에는 toast, Agent 동작에는 로그로 알린다.
거절된 변환은 최근 목록에도 넣지 않는다.
닫은 탭 복원·프리셋·markdown mirror는 대기 placeholder로 두고 새 hello가 오면 다시 만든다.
재시작·swap·연결 실패는 kind를 철회하지 않는다. 열린 surface 자체는 닫거나 바꾸지 않는다.

### 전송 지연 (Nagle 금지)

호스트와 plugin 사이 메인 채널(TCP · NDJSON)은 IPC · attach 소켓과 같은 **이중 방어**를 쓴다.

- **본문과 개행은 한 버퍼로 보낸다** — 양 끝의 모든 송신 자리가
  `tasty_plugin_protocol::write_line` 을 거쳐 본문과 개행을 한 버퍼로 `write_all` 1 회에 보낸다.
  단위 시험 `line::tests::write_line_emits_one_write_call` 이 시험 writer에 전달된 호출 수를 확인한다(각 호출자가
  이 함수를 쓰는지는 별도로 확인한다).
- **양 끝 소켓은 `TCP_NODELAY`** 다 — 호스트는 listener 가 연결을 받는 자리
  (`crates/tasty-host-plugin/src/listener.rs` `handle_incoming`), plugin 은 SDK `Connection::connect`.
  한 번에 써도 줄이 MSS 를 넘거나 직전 메시지가 아직 unACKed 면 다음 조각이 다시 Nagle 에 걸리므로
  이것이 없으면 안 된다.

보조 핸들 채널(Unix 도메인 소켓 · Windows named pipe)은 TCP 가 아니라 Nagle 이 없어 이 규칙의 대상이 아니다.

`handed_off_stream_has_nodelay`와 `connect_disables_nagle_on_the_host_channel`이
양 끝의 소켓 설정을 검사한다. IPC·attach의 같은 규칙은
[프레임 전송 지연](attach-behavior.md#프레임-전송-지연-nagle-금지)을 따른다.

### 채널 상한 (개수 · 바이트 · 합계)

호스트와 plugin 프로세스 하나 사이의 세 채널(요청 · 응답 · 이벤트)에는 상한이 셋 걸린다.

| 상한 | 값 | 무엇을 묶나 | 근거 |
|---|---|---|---|
| 개수 | 채널마다 1024 건 | 큐에 쌓인 메시지 수 | [IPC 서버](../architecture/ipc-server.md) |
| 큐 바이트 | 큐마다 16 MiB | 큐 하나에 쌓인 줄의 바이트 | [IPC 서버](../architecture/ipc-server.md) |
| 합계 바이트 | 프로세스 전체 64 MiB | 모든 plugin · 모든 채널을 더한 바이트 | docs/architecture/ipc-server.md |

- **포화의 답은 방향이 정한다** — 호스트 → plugin 요청은 **거절**(호스트 프레임이 서지 않게),
  plugin → 호스트 응답·이벤트는 **대기**(plugin 의 소켓 읽기가 선다 = backpressure). 세 상한 모두
  같은 규칙이다.
- **빈 큐는 한 건을 늘 받는다.** 상한은 누적에 걸린다 — 상한보다 큰 한 건도 큐가 비어 있으면
  들어간다. 그러지 않으면 그 한 건이 대기 방향을 영영 세운다. 그래서 실제 상한은 "상한 + 큐마다
  한 건" 이고, 한 줄의 크기는 따로 묶이지 않는다(plugin 소켓에 줄 길이 상한이 없다).
- **바이트는 소켓의 줄 길이로 잰다**(개행 포함). 추정하지 않는다.
- **제어(ping · shutdown)는 합계 상한을 면제한다** — 개수와 큐 바이트 상한은 그대로 받는다. 합계는
  다른 plugin 이 채울 수 있어서, 거기에 제어를 걸면 건강한 plugin 이 남의 포화로 무응답 재시작되거나
  graceful 없이 kill 된다(docs/architecture/ipc-server.md의 전송 거절 처리).
- **합계는 plugin 을 가로지른다.** 합계가 찬 동안에는 **다른** plugin 의 요청도 거절되고 다른
  plugin 의 reader 도 기다린다. 큐 상한이 합계보다 작아 plugin 하나가 혼자서는 합계를 못 채운다.
- 호스트 로그에서 구분된다 — `request queue full`(개수) · `request queue over its byte budget`
  (큐 바이트) · `plugin channels over their total byte budget`(합계).
- 렌더 데이터(egui-mesh 기하·텍스처)는 이 채널을 안 탄다 — 공유 메모리 버퍼 한 칸을 덮어쓰고
  host 는 surface 마다 마지막 프레임만 든다. 쌓이는 큐가 없다(docs/architecture/ipc-server.md의 공유 메모리 예외).
- 현재 누적은 `PluginManager::channel_bytes`(큐별 바이트·최댓값·거절·대기 누계)가 낸다. **이
  값을 읽는 IPC/CLI 는 아직 없다.**

`crates/tasty-host-plugin/src/manager/pump.rs`는 이벤트 처리와 생성·회수 FIFO를 소비한다. `pump/retirement.rs`는 원 surface binding·plugin process에 고정된 회수 요청과 ACK 추적을 담당하며, publication 장벽의 제어 전용 pump와 대기 바이트 예산은 `pump.rs`에 남는다.

### 큐 포화 통지 (호스트가 버린 요청을 plugin 이 안다)

호스트 → plugin 요청 큐는 유한하고, 차면(개수든 바이트든 — 위 "채널 상한") **기다리지 않고 거절**한다 — 그 방향에서
기다리면 호스트 프레임이 통째로 선다([IPC 서버](../architecture/ipc-server.md)).
거절된 요청은 소켓에 안 나가므로 plugin 은 그것이 있었다는 사실 자체를 모른다.

그래서 버린 수를 **다음으로 실제 큐에 들어가는 요청**에 얹는다 —
`PluginRequest.dropped_requests`. 별도 통지 메시지를 만들면 그 통지도 같은(찬) 큐를
써야 해서 자기모순이고, 자리가 났다는 것은 plugin 이 하나라도 소비했다는 뜻이므로
다음 요청이 실제로 전달되면 plugin이 이 값을 읽을 수 있다. 근거는
[IPC 서버](../architecture/ipc-server.md).

SDK 가 셋으로 노출한다.

- **`warn` 로그** — 값이 0 이 아닌 줄을 받으면 SDK 가 자동으로 남긴다
  (`host dropped N request(s) to this plugin before this one`). plugin 이 아무것도 안
  해도 로그에는 남는다.
- **`Plugin::on_host_dropped_requests(dropped)`** — worker 스레드에서 dispatch 직전
  1 회. 기본 구현 no-op 이라 기존 plugin 은 안 움직인다.
- **`HostHandle::dropped_by_host()`** — 누적값. `HostHandle` 은 `Clone` 이고 값을
  공유하므로 자체 background 스레드에서도 읽는다.

**무엇이 버려졌는지는 알 수 없다** — 호스트도 안 들고 있다. 그래서 처방은 재요청이
아니라 **자기 작업량을 줄이는 것**이다(polling 간격, 렌더 빈도, 사용자 통지).

### 프로세스 수명 결박 (3 OS — 크래시·강제종료 포함)

정상 종료에서는 `PluginProcess::shutdown`과 `Drop`이 자식을 정리한다.
Drop이 실행되지 않는 크래시·강제 종료에 대비해 `PluginReaper`가 아래 장치를 사용한다.
설정 실패 시에는 경고를 남기고 기존 kill 기반 정리로 돌아가므로 모든 종료 상황을
무조건 보장하는 것은 아니다.

| OS | 메커니즘 | 통합 지점 | 손자(node/chrome) |
|----|----------|-----------|--------------------|
| **Windows** | Job Object (`JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`). 호스트가 Job 핸들을 `PluginManager` 수명 동안 소유 → 호스트 사망 시 핸들 닫히며 OS 가 Job 내 전 프로세스 강제 종료. | 호스트 `adopt`(각 자식 assign) | **자동 커버**(Job 멤버십 자식 상속) |
| **Linux** | `prctl(PR_SET_PDEATHSIG, SIGKILL)` (자식 `pre_exec`). 부모 사망 시 커널이 직속 플러그인에 SIGKILL. **PDEATHSIG 는 fork 한 *스레드* 종료에 발화**하므로(man prctl 경고) 모든 spawn 은 `PluginReaper::spawn_bound` 가 프로세스 수명의 영속 spawner 스레드에서 fork 한다 — 단명 스레드(부트 워커 등)에서 직접 spawn 하면 그 스레드 종료 시 plugin 전원이 SIGKILL 된다. | 호스트 `prepare`(pre_exec) + `spawn_bound`(영속 스레드 fork) | 고아 허용(범위 밖) |
| **macOS** | PDEATHSIG 등가물 부재 → SDK 런타임 watchdog 이 `getppid` 폴링(500ms)해 부모 PID 변화 감지 시 self-exit. 호스트는 `TASTY_HOST_PID` env 만 주입. | 플러그인 SDK(`runtime.rs`) | 고아 허용(범위 밖) |

모든 결박 실패(Job 생성/assign 실패 등)는 `tracing::warn!` 으로 흡수하고 기존 kill 기반 정리로 degrade — 결박 실패가 기능이나 호스트를 죽이지 않는다.

Linux spawner가 자식을 생성한 뒤 요청자가 결과 채널을 닫았다면, 반환된 자식 핸들로
종료와 wait를 수행한다. 전달받은 호출자가 없다는 이유로 Child를 버리면 프로세스가 남는다.
결과 전달에 성공한 자식은 기존 호출자가 수명을 관리한다.

PTY 셸도 별도 정리 경로가 있다. Windows에서는 `tasty-reaper`의 호스트 Job Object에
`Terminal::new`가 셸 PID를 등록한다. plugin Job과는 별도 인스턴스다.
Unix에서는 PTY master가 닫히며 SIGHUP이 전경 프로세스 그룹에 전달된다.
신호를 무시하거나 분리된 자식까지 모두 종료한다고 일반화하지 않는다.
`portable-pty::CommandBuilder`는 `pre_exec`를 제공하지 않아 셸에 PDEATHSIG를 설치하지 않는다.

정상 종료 경로(surface 닫기/quit)에서는 `tasty-terminal` 의 `Pty` Drop 이 셸을 명시적으로 kill 해 PTY master HUP 에만 의존하지 않는다. 결정 배경·대안·재검토 조건은 [터미널 (Terminal)](../surfaces/terminal/index.md).

### 토큰 핸드셰이크 (보안)

호스트가 `127.0.0.1:0`(랜덤 포트) listen → 토큰을 listener 에 **먼저 등록**(`HostListener::register`)하고 spawn 하며 `TASTY_HOST_IPC_PORT` + `TASTY_PLUGIN_TOKEN` 전달 → 플러그인이 그 포트로 connect 후 **첫 줄에 `AuthMessage{plugin_id, token}`** 전송 → 토큰 일치해야 인증 통과(`HANDSHAKE_TIMEOUT` 10s 내 — 호스트는 이 대기를 메인 스레드 밖에서 한다, 위 "기동과 연결"), mismatch 면 즉시 끊음. 등록이 spawn 보다 앞서는 이유: 채널이 Nagle 을 끄므로 인증 줄이 지연 없이 도착해, spawn 뒤에 등록하면 plugin 이 그 틈에서 이겨 모르는 토큰으로 거절된다(시험 `a_connection_that_authenticates_before_the_wait_is_kept` 은 등록 뒤·대기 전에 도착한 인증을 listener 가 붙잡아 두는지, `a_dropped_registration_is_withdrawn` 은 버린 등록이 거둬지는지 본다). 등록이 spawn 보다 앞선다는 순서 자체는 `manager::tests_connect::the_connection_is_registered_before_the_child_starts` 가 고정한다 — 시험 전용 hook(`process::AFTER_CHILD_SPAWN_DELAY`)이 자식을 띄운 직후 호출 스레드를 1 s 세워, 등록이 그 뒤면 곧바로 연결한 가짜 plugin 이 모르는 토큰으로 거절돼 실패한다. 실행 중에는 GUI 부팅 로그의 `plugin auth with unknown/expired token` 줄 수로도 볼 수 있다(0 이어야 한다). SDK transport 가 이 핸드셰이크를 자동 수행하므로 작성자는 보통 신경 쓸 필요 없다.

## 8. 규약

- **이름**: crate `tasty-plugin-<name>` = binary 이름, id `com.x.<name>`(다어절 hyphen), IPC prefix = id 마지막 segment 의 `_` 변환, i18n key root = prefix.
- **i18n**: 매니페스트 `*_i18n_key` 는 host 가 lookup. 플러그인이 직접 그리는 텍스트는 `tasty_plugin_sdk::i18n::Translator`(`TASTY_LOCALE` 주입 — host 가 부팅 시 `general.language` 에서 set, §7 표). 키는 자기 prefix 안에만(`surface.kind.<own>` 만 예외).
- **권한 표기**: 실제 필요한 것만. 자기 namespace `ipc.invoke:<self>` 는 적지 않는다 — 자기 호출에는 필요 없고, 자식 agent 토큰에 넘길 때도 소유자라 쥐지 않고 넘긴다([plugin-permissions](plugin-permissions.md#agent-caller--session-token--temp-grants)).
- **모듈 분리**: `main.rs`가 약 300줄을 넘으면 역할에 따라 `state.rs`·`handlers.rs`·
  `install.rs` 등으로 나눈다. Claude·Codex·image plugin의 모듈 구성을 참고한다.
  파일별 현재 줄 수는 문서에 복제하지 않는다(docs/documentation-model.md).

- **Cargo**: `tasty-plugin-protocol` 직접 의존 금지 — SDK 가 re-export. `[lints] workspace = true`.

## 9. 빌드 & 설치

```bash
cargo build --release -p my-plugin
tasty plugin install <plugin-dir>    # 신뢰 판정 → 복사 → 매니페스트 권한 자동 grant → spawn
```

**서명과 공개 키**: 설치(`tasty plugin install`·GUI Add 탭)는 debug·release 모두 폴더를 복사하기 전에 매니페스트 서명을 판정한다([플러그인 시스템](../features/plugin-system/index.md)). 서명하지 않은 커스텀 plugin 은 미인증이며, CLI 는 원인·위험 경고와 함께 거부하고 `--allow-unverified` 로 다시 설치하라고 안내한다. GUI 는 같은 경고를 보이고 사용자가 `Add anyway` 를 고를 때만 설치한다. 선택한 뒤에는 목록의 미인증 표시 외에 실행 제한이 없다. 사용자가 서명 키를 확인하고 신뢰할 수 있도록 서명과 공개 키 파일을 함께 배포한다. 처음 설치하는 사용자에게는 모르는 키라 경고가 보이며, 사용자가 키를 신뢰한 뒤에는 같은 키로 서명한 다음 버전을 묻지 않는다.

```bash
openssl genpkey -algorithm Ed25519 -out my-plugin.pem                       # 1회. 비밀 키는 배포하지 않는다
openssl pkey -in my-plugin.pem -pubout -outform DER | tail -c 32 > <plugin-dir>/tasty-plugin.toml.pub
./scripts/sign-bundle.sh --key my-plugin.pem --manifest <plugin-dir>/tasty-plugin.toml   # → tasty-plugin.toml.sig
```

- `tasty-plugin.toml.sig` 는 매니페스트 SHA-256 digest 의 Ed25519 서명(raw 64 byte)이고, `tasty-plugin.toml.pub` 는 raw 32 byte 공개 키다. 형식은 [플러그인 패키징](plugin-packaging.md#서명)에 있다. 매니페스트를 한 줄이라도 고치면 다시 서명한다.
- 호스트에 내장된 키가 아니므로 처음 설치하는 사용자에게는 모르는 키다. GUI 는 fingerprint 를 보이고 `Trust key & add anyway` 로 그 키를 신뢰 목록(`known-plugins.toml`)에 넣으며, 다음 버전부터는 같은 키로 서명하면 묻지 않는다. 권한이 늘면 재승인을 묻는다. CLI `install` 은 키를 신뢰 목록에 넣지 않으므로 처음 설치는 `--allow-unverified` 또는 GUI 를 쓴다.
- `.pub` 가 없거나 서명 키와 다르면 각각 `missing-pubkey`·`signature-error` 미인증이다. 서명 판정은 매니페스트 한 파일이 그 키로 서명됐는지만 확인한다. 바이너리 내용이나 실행 중 동작은 확인하지 않으며, 원칙의 인증(공식 플러그인은 공식 릴리스 절차로 만든 산출물, 커스텀 플러그인은 마켓플레이스가 소스를 확인하고 직접 빌드한 산출물. 향후 기능)과 다르다.
- 부팅 탐색은 release 에서 신뢰했거나 사용자가 고른 plugin 만 등록하고, debug 에서는 서명을 건너뛴다. debug 에서 설치가 통과했다는 사실만으로 release 사용자 경험을 확인했다고 보지 않는다.

**워크스페이스 내 번들 플러그인 개발**: `BUILTINS`(`crates/tasty-host-plugin/src/builtin.rs`) 등록 플러그인은 debug 빌드에서만 호스트가 부팅에 workspace→bundle 자동 sync(`ensure_dev_bundle`)한다. release/dist 는 `just build --release` 또는 해당 프로필의 `just build-plugins` 로 미리 스테이징해야 한다. bundle→설치 디렉터리 sync(`install_builtins_if_needed`)는 모든 프로필에서 유지된다. 이 복사는 macOS의 `com.apple.quarantine` 같은 확장 속성을 지우지 않는다. 앱 번들 안 파일에 그 속성이 남아 있으면 복사본도 같은 속성을 가질 수 있고, 처리는 사용자가 정한다([본체와 기반 기술](../principles/control-and-data.md#본체와-기반-기술)). 단 루트 `cargo build` 는 본 바이너리만 빌드하므로 플러그인 변경은 `cargo build -p <crate>` 또는 `--workspace` 필요. **부팅 없이, 실행 중인 tasty 에 플러그인 변경만 반영하는 절차(호스트 재빌드·재시작 불필요)는 아래 §9.1.**

디버깅: `tasty plugin logs <id> --follow` / `~/.tasty/plugins-logs/<id>.log` / `RUST_LOG=debug`.

### 9.1 실행 중인 tasty 에 번들 플러그인만 반복 갱신 (호스트 재빌드·재시작 불필요)

**플러그인은 호스트(tasty 본체)와 별개의 자식 프로세스다.** 번들 플러그인의 코드/매니페스트/임베드 리소스만 고쳤다면 **본체를 재빌드·재시작할 필요 없이 그 플러그인만** 갱신하면 실행 중인 tasty 에 반영된다. (호스트 코드 `src/`·다른 크레이트를 고쳤을 때만 본체 재빌드+재시작이 필요하고, 그건 실행 중 exe 잠금 때문에 보통 사용자 도움이 필요하다.)

**0) 실행 중인 tasty 의 프로필을 먼저 확인** — 그 프로필로 플러그인을 빌드해야 한다. 번들 스테이징 경로가 `target/<profile>/builtin-plugins/` 라, 프로필이 어긋나면 엉뚱한(옛) 바이너리가 sync 된다.
```bash
# 어떤 exe 가 떠 있나: Windows  tasklist | findstr tasty  ·  Linux/macOS  ps aux | grep tasty
# 경로로 profile 판별 (…/target/release/tasty → release, …/debug/… → debug, 설치본 → dist).
# 개발 중이면 보통 target/release/tasty → 이하 예시도 --release.
```

**1) 그 프로필로 플러그인 빌드·스테이징**
```bash
PROFILE=release just build-plugin <name>       # 실행 중 tasty 와 같은 프로필
```

**2) 재서명 (release/dist 호스트는 매니페스트 서명을 검증)** — 안 하면 다음 단계가 `untrusted: UnknownKey` 로 skip. debug 호스트는 서명 안 보므로 불필요. 단일 `build-plugin` 은 서명을 복사하지 않으므로 서명 후 번들에도 명시적으로 배치한다. release/dist 의 `upgrade-builtins` 는 workspace 소스를 읽지 않는다.
```bash
./scripts/sign-bundle.sh --key ~/.tasty-keys/dev.pem --manifest crates/tasty-plugin-<name>/tasty-plugin.toml
cp crates/tasty-plugin-<name>/tasty-plugin.toml.sig target/release/builtin-plugins/<manifest-id>/tasty-plugin.toml.sig
```

**3) 정지 → 재동기화 → 재기동 (순서 중요)**
```bash
tasty plugin disable com.x.<name>     # 먼저 정지. 안 하면 실행 중 .exe 를 잠가 upgrade 가 'os error 5(액세스 거부)'
# disable은 보통 회수를 별도 스레드에 맡긴다. 다음 upgrade-builtins는 실제로 쓸 때만
# 기존 회수 작업을 기다린다. 정상 종료 기한은 2s이며 kill/wait까지 포함한 전체 상한은 아니다.
# 변경이 없어 건너뛰는 플러그인은 기다리지 않는다.
tasty plugin upgrade-builtins         # 번들→user dir(~/.tasty/plugins) 재sync. 매니페스트 version 올렸으면 upgraded
#   ※ version 을 안 올려도 반영된다 — 같은 버전 갈래는 **내용으로** 판정해 다른 파일만 옮긴다
#      보고문은 여전히 'skipped' 로 나오지만 사유가 갈린다 — 'content resync: files rewritten' 이면 옮긴 것이고
#      'nothing to write' 면 이미 같았다는 뜻이다. `--force` 는 **설치본 버전이 번들보다 높아** 건너뛰는 갈래에만 필요하다.
tasty plugin enable com.x.<name>      # 재기동 — 호스트가 새 매니페스트를 레지스트리에 재적재
# 기존 회수가 진행 중이면 enable이 그 작업의 완료를 기다린 뒤 새 프로세스를 시작한다.
# 이 대기도 전체 2s 상한은 아니다. enable 성공은 spawn 완료이며 연결·hello 완료는 아니다(docs/dev-guide/plugin-development.md).
# 새 프로세스가 뜨면 열어 둔 surface 를 다시 게시한다(§ Surface kind 의 재게시 문단).
```

내용 비교를 **해시가 아니라 바이트로** 하는 근거와 잰 값·대안·재검토 조건은
이 문서.

**4) 실행 중 tasty 에 대해 실동작 검증**
```bash
tasty <plugin-cli> --help             # 새 서브커맨드/매니페스트 반영 확인
tasty <plugin-cli> <cmd> ...          # CLI→IPC→실행 중 호스트→플러그인 경로로 실제 동작 확인
```

> **왜 호스트 재시작이 필요 없나**: `tasty <plugin> --help` 는 매번 새 CLI 프로세스가 **설치된 매니페스트**(`~/.tasty/plugins/<id>/`)를 읽으므로 upgrade 즉시 반영되고, IPC 디스패치는 `disable/enable` 이 플러그인 프로세스를 재기동하며 호스트 레지스트리를 갱신한다. 따라서 **본체 GUI 를 껐다 켤 필요가 없다.**

## 10. 한계 (현재 SDK)

- async 미지원 — 모든 콜백 동기(무거운 I/O 는 플러그인 내부 thread).
- HotReload 미지원 — 코드 변경은 재빌드 후 `disable && enable`(전체 반복 절차는 [§9.1](#91-실행-중인-tasty-에-번들-플러그인만-반복-갱신-호스트-재빌드재시작-불필요)). 단 플러그인만 갱신하면 되고 **호스트 재빌드·재시작은 불필요**.
- 권한 게이트는 **호스트 IPC 호출만** 막는다 — 플러그인 프로세스의 직접 `std::fs` 는 OS 샌드박스가 없는 한 강제 안 됨([plugin-permissions 한계](plugin-permissions.md#한계)).

## 관련

- [concepts/plugins](../concepts/plugins.md) — 분류 축·권한 개요
- [plugin-permissions](plugin-permissions.md)
- [plugins/](../plugins/index.md) — 번들 플러그인(= 예제) 카탈로그
- [features/plugin-system](../features/plugin-system/index.md) — 설치/관리 UI

## 실패 후 로컬 정리

hook callback 뒤에 반드시 할 로컬 정리가 있다면 호스트 호출 실패를 기록한 뒤 그 정리를 마친다.
예를 들어 surface가 이미 닫힌 Claude hook은 host 알림 실패 때문에 로컬 hook 상태 정리까지 건너뛰지 않는다.
호출 성공으로 위장하는 것이 아니라 warning과 로컬 정리를 함께 수행하는 규칙이다.
정리가 없는 경로는 기존 오류 전파를 유지할 수 있다. Codex hook 등과 모양만 맞추려고 일괄 변경하지 않는다.
