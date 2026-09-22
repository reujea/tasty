#![forbid(unsafe_code)]

//! Tasty markdown plugin — **webview** markdown viewer surface (ADR-0028, Stage B).
//!
//! The plugin owns the markdown document (reads the `.md` file delivered via `surface.create`,
//! watches it for external changes) and renders it by generating a complete, sanitized HTML
//! document (`render::render_document`) that the host's native OS WebView displays — the
//! plugin no longer tessellates its own egui mesh for the document body (that was the former
//! `rendering = "egui-mesh"` design, ADR-0028 / B1). Address-bar navigation and content link
//! clicks are captured via the host's `webview.navigation_attempt` event (Stage A) and routed
//! back through `file_handler.dispatch` (files) or `webview.open_external` (external URLs, opened
//! by the host — this plugin never calls an OS opener itself) — see `render.rs`'s
//! module doc for why link destinations are rewritten into an internal URL-fragment scheme
//! rather than left as plain `href`s.
//!
//! Two egui-mesh popups remain unchanged: the large-file confirmation and the file-open form
//! (`[[contributes.popup]]` in the manifest) are still self-rendered by the plugin — only the
//! main document surface moved to the webview channel.

// 이유: 테스트 본문의 `let _ =` 는 정책이 사유를 요구하지 않는 자리라
// `clippy::let_underscore_must_use` 명부에 섞이면 안 된다 — 그 명부는 프로덕션에서
// 값을 버리는 자리의 목록이고, 테스트가 늘 때마다 숫자만 흔들리면 새 프로덕션
// 자리가 그 안에 묻힌다(docs/dev-guide/error-handling.md). `cfg_attr(test, ..)` 라
// 라이브러리 타깃의 판정은 그대로다 — 프로덕션 자리는 여전히 명부에 오른다.
#![cfg_attr(test, allow(clippy::let_underscore_must_use))]

mod popup;
mod render;

/// 빌드타임 SVG 베이크 산출물 (방식 B). `build.rs` 가 `tasty-icons` 의 canonical
/// `<svg>` 를 usvg 로 파싱·평탄화해 `pub const <NAME>: &[&[[f32; 2]]]`(viewBox 0..24
/// 좌표)를 생성한다. 런타임은 이 점배열을 [`tasty_plugin_sdk::baked_icon::draw`] 로
/// 그릴 크기에 스케일해 벡터 stroke 로 그린다(텍스처 없음, DPI 독립). 확인 팝업
/// (large-file/file-open) 전용 — 본문은 더 이상 이 plugin 이 그리지 않는다.
mod baked_icons {
    include!(concat!(env!("OUT_DIR"), "/plugin_icons.rs"));
}

use serde_json::{Value, json};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc;
use tasty_plugin_protocol::ThemeWire;
use tasty_plugin_sdk::file_watch::{self, ContentDigest, WatchCmd};
use tasty_plugin_sdk::{
    BusHandle, EventDispatchCtx, EventScope, HostHandle, IpcMethodCtx, IpcMethodError, Plugin,
    PluginEnv, PopupClosedCtx, PopupOpenCtx, PopupOpenResult, PopupSetContextCtx, SurfaceCreateCtx,
    SurfaceRestoreCtx, SurfaceResult, Translator, WebviewNavigationAttemptCtx,
};
use tasty_type_appearance::theme::Theme;

use popup::{
    FILE_PICKER_RESULT_EVENT, FileOpenState, FilePickerResultWire, LargeFileConfirm, PickerStart,
    basename, format_size,
};
#[cfg(any(unix, windows))]
use tasty_plugin_sdk::EguiMeshPopup;

const PLUGIN_ID: &str = "com.tasty.markdown";
const PLUGIN_VERSION: &str = env!("CARGO_PKG_VERSION");

/// 대용량 파일 확인 게이트 임계값 (1MB). 이 크기를 *초과* 하는 파일은 읽기 전에 확인
/// 팝업을 띄운다. **크기 감지는 plugin in-process** — host 는 파일 크기를 stat 하지
/// 않는다(불가침 원칙: markdown 크기게이트는 plugin 소유). 이름도 plugin-local 이다.
const LARGE_FILE_LIMIT_BYTES: u64 = 1024 * 1024;

/// 대용량 감지 시 plugin 이 발행하는 이벤트 key. 매니페스트 `event_publish` 패턴 +
/// `[[contributes.popup]]` event trigger 가 이 key 로 매칭돼 확인 팝업을 연다.
const LARGE_FILE_EVENT_KEY: &str = "com.tasty.markdown.large_file_confirm";

/// host 가 발행하는 전역 테마 변경 이벤트 key(`event_subscribe`). webview-kind surface 는
/// `surface.set_context` 를 받지 않아 Theme 변경이 자동으로 밀리지 않으므로, 이 이벤트를
/// 받을 때마다 살아있는 모든 문서를 재생성한다.
const THEME_CHANGED_EVENT: &str = "theme.changed";

/// attach mirror 문서의 원문 조회를 host 에 거는 메서드. host 는 `request_id` 만 즉시
/// 돌려주고 원문은 [`MIRROR_CONTENT_RESULT_EVENT`] 로 나중에 온다
/// (`docs/adr/0255-markdown-attach-mirror-forwards-content-not-pixels.md`).
const MIRROR_CONTENT_REQUEST_METHOD: &str = "markdown_mirror.content_request";

/// 원격 원문 요청을 누가 일으켰나. host 는 에이전트가 일으킨 요청의 회신에서 사용자
/// toast(원문 잘림)를 띄우지 않는다 — 에이전트 행동의 부수효과가 사용자 시각 상태에 닿지
/// 않게 하는 것이다(identity 원칙 1, ADR-0503).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RemoteRequester {
    /// 최초 열기 · 원격 변경 신호 재조회 · 새로고침 버튼 — 이 plugin 이 스스로 또는 사용자
    /// 클릭으로 건 요청.
    Plugin,
    /// `markdown.reload` IPC — 바깥 호출자(에이전트)가 건 요청.
    Agent,
}

/// `markdown_mirror.content_request` 의 params. 에이전트 요청에만 `agent_origin: true` 를
/// 싣는다 — 그 칸이 없는 요청은 종전 그대로 읽힌다.
fn mirror_content_request_params(surface_id: u32, requester: RemoteRequester) -> Value {
    match requester {
        RemoteRequester::Plugin => json!({ "surface_id": surface_id }),
        RemoteRequester::Agent => json!({ "surface_id": surface_id, "agent_origin": true }),
    }
}

/// host 가 이 plugin 에 unicast 하는 원문 조회 회신 이벤트.
const MIRROR_CONTENT_RESULT_EVENT: &str = "markdown_mirror.content_result";

/// host 가 이 plugin 에 unicast 하는 원격 파일 변경 신호. 받아도 원문을 다시 받지 않고
/// 새로고침 버튼 색만 바꾼다 — 다시 받는 것은 사용자가 버튼을 눌렀을 때다.
const MIRROR_CHANGED_EVENT: &str = "markdown_mirror.changed";

/// host 가 대기 중인 요청을 버리라고 알릴 때 쓰는 `request_id`. host 는 이 값을 발급하지
/// 않는다(attach 연결이 끊겨 회신이 영영 안 올 때 보낸다).
const MIRROR_ABANDON_REQUEST_ID: u64 = 0;

/// Per-surface markdown document state owned by the plugin (content, load outcome, base
/// dir for relative paths). Tracking to *decide when* to reload lives in the SDK's `file_watch`,
/// not here — `MdDoc` only knows how to (re-)read once asked.
struct MdDoc {
    file_path: Option<String>,
    base_dir: Option<PathBuf>,
    content: String,
    load_error: Option<String>,
    /// 대용량 확인 대기 중이면 true — 파일을 아직 읽지 않았다(빈 콘텐츠). 확인 팝업의
    /// [열기] 확정 시 [`MdDoc::resume_load`] 가 실제 read 를 재개한다.
    pending_large: bool,
    /// attach mirror 문서면 `Some` — 원문은 로컬 파일이 아니라 host 가 원격에서 가져다
    /// 준다. 이 문서는 파일을 **한 번도 읽지 않는다**(`file_path` 는 원격 경로라 표시 전용).
    remote: Option<RemoteDoc>,
}

/// attach mirror 문서의 원격 조회 상태.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
struct RemoteDoc {
    /// 회신을 기다리는 요청. 새 요청이 나가면 덮어써져 늦게 온 옛 회신은 버려진다.
    pending: Option<u64>,
    /// 원문을 한 번이라도 받았는가 — 받기 전에는 본문 자리에 로딩 상태를 그린다.
    loaded: bool,
    /// 받은 뒤 원격 파일이 바뀌었다는 신호가 왔는가.
    stale: bool,
    /// attach 연결이 끊겼다는 통지(abandon)를 받은 뒤 아직 원문을 다시 받지 못했는가. 받은
    /// 원문은 버리지 않지만 화면은 끊김을 그린다 — 옛 원문을 그대로 두면 연결이 살아 있는
    /// 화면과 구분되지 않는다.
    disconnected: bool,
}

/// [`MdDoc::on_remote_changed`] 의 결정 — 변경 신호 하나에 plugin 이 할 일.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RemoteChange {
    /// 할 일이 없다(로컬 문서 · 이미 stale · 이미 받는 중).
    Ignore,
    /// stale 표시를 켰다 — 다시 그린다.
    Redraw,
    /// 원문을 보여 주지 못하는 문서다 — 다시 요청한다.
    Refetch,
}

/// `markdown_mirror.content_result` 페이로드. host 가 surface_id 를 로컬 id 로 바꿔 보낸다.
#[derive(Debug, serde::Deserialize)]
struct MirrorContentResultWire {
    surface_id: u32,
    request_id: u64,
    ok: bool,
    #[serde(default)]
    source: Option<String>,
    #[serde(default)]
    reason: Option<String>,
}

impl MdDoc {
    fn new(file: Option<String>) -> Self {
        let base_dir = file
            .as_ref()
            .and_then(|f| PathBuf::from(f).parent().map(|p| p.to_path_buf()));
        let (content, load_error) = match &file {
            Some(f) => match std::fs::read_to_string(f) {
                Ok(text) => (text, None),
                Err(e) => (String::new(), Some(e.to_string())),
            },
            None => (String::new(), None),
        };
        Self {
            file_path: file,
            base_dir,
            content,
            load_error,
            pending_large: false,
            remote: None,
        }
    }

    /// attach mirror 문서 — 파일을 읽지 않고 `base_dir` 도 두지 않는다. 상대경로 이미지·
    /// 링크가 이 머신의 파일로 풀리면 원격 문서가 로컬 파일을 끌어다 쓰게 된다.
    fn new_remote(file: String) -> Self {
        Self {
            file_path: (!file.is_empty()).then_some(file),
            base_dir: None,
            content: String::new(),
            load_error: None,
            pending_large: false,
            remote: Some(RemoteDoc::default()),
        }
    }

    /// 원문 조회 회신을 반영한다. 다시 그려야 하면 true.
    ///
    /// 대기 중인 요청과 id 가 다르면 버린다(새로고침을 연달아 눌러 옛 회신이 늦게 온 경우).
    /// abandon id 는 연결이 끊겼다는 통지다 — 기다리던 요청을 끝내고, 원문을 이미 받은
    /// 문서도 끊김 상태로 둔다. 받은 원문은 지우지 않는다(다시 받으면 그대로 갈아끼운다).
    fn apply_remote_result(&mut self, reply: MirrorContentResultWire) -> bool {
        let Some(remote) = self.remote.as_mut() else {
            return false;
        };
        if reply.request_id == MIRROR_ABANDON_REQUEST_ID {
            remote.pending = None;
            let changed = !remote.disconnected;
            remote.disconnected = true;
            return changed;
        }
        let Some(pending) = remote.pending else {
            return false;
        };
        if reply.request_id != pending {
            return false;
        }
        remote.pending = None;
        if reply.ok {
            self.content = reply.source.unwrap_or_default();
            self.load_error = None;
            remote.loaded = true;
            remote.stale = false;
            remote.disconnected = false;
        } else {
            self.load_error = Some(reply.reason.unwrap_or_default());
        }
        true
    }

    /// 원격 파일 변경 신호에 어떻게 반응할지 정한다.
    ///
    /// 원문을 보여 주고 있는 문서는 다시 받지 않고 stale 표시만 켠다 — 사용자가 읽던 자리를
    /// 말없이 갈아치우지 않기 위해서다(ADR-0255 항목 5). 이미 stale 이면 다시 그리지 않는다
    /// (같은 파일이 연달아 저장될 때 문서를 매번 통째로 다시 싣지 않게).
    ///
    /// 원문 대신 끊김·실패를 보여 주는 문서는 **다시 받는다** — 지킬 읽던 자리가 없고, 그
    /// 상태로 두면 원격이 되살아나도 사용자가 누르기 전까지 끊김 화면이 남는다. host 가
    /// 재연결 직후 이 신호를 보내는 이유가 그것이다. 이미 받는 중이면 그 회신을 기다린다.
    fn on_remote_changed(&mut self) -> RemoteChange {
        let showing_error = self.load_error.is_some();
        let Some(remote) = self.remote.as_mut() else {
            return RemoteChange::Ignore;
        };
        if remote.loaded && !remote.disconnected && !showing_error {
            if remote.stale {
                return RemoteChange::Ignore;
            }
            remote.stale = true;
            return RemoteChange::Redraw;
        }
        if remote.pending.is_some() {
            return RemoteChange::Ignore;
        }
        RemoteChange::Refetch
    }

    fn remote_view(&self) -> Option<render::RemoteView> {
        self.remote.as_ref().map(|r| render::RemoteView {
            loading: !r.loaded,
            stale: r.stale,
            disconnected: r.disconnected,
        })
    }

    /// 대용량 파일을 **읽지 않고** 경로만 보관한 문서를 만든다(확인 팝업 대기). 확인
    /// 시 [`MdDoc::resume_load`] 가 read 를 재개한다.
    fn new_deferred(file: Option<String>) -> Self {
        let base_dir = file
            .as_ref()
            .and_then(|f| PathBuf::from(f).parent().map(|p| p.to_path_buf()));
        Self {
            file_path: file,
            base_dir,
            content: String::new(),
            load_error: None,
            pending_large: true,
            remote: None,
        }
    }

    /// 대용량 확인 [열기] 후 실제 read 를 재개한다.
    fn resume_load(&mut self) {
        self.pending_large = false;
        if let Some(f) = self.file_path.clone() {
            self.read_now(&f);
        }
    }

    /// Force a re-read (`markdown.reload` IPC, idle watch — SDK `file_watch` owns its own
    /// tracking to decide *when* to call this; `MdDoc` itself no longer tracks mtime since
    /// `paint_surface` never fires for a webview-kind surface, so there's no per-frame
    /// throttled poll to gate anymore — see the module doc on the removed `poll_reload`).
    fn force_reload(&mut self) {
        // mirror 문서의 경로는 원격 호스트의 것이다 — 로컬에서 읽으면 엉뚱한 파일이 뜬다.
        if self.remote.is_some() {
            return;
        }
        let Some(f) = self.file_path.clone() else {
            return;
        };
        self.read_now(&f);
    }

    fn read_now(&mut self, path: &str) {
        match std::fs::read_to_string(path) {
            Ok(text) => {
                self.content = text;
                self.load_error = None;
            }
            Err(e) => self.load_error = Some(e.to_string()),
        }
    }
}

struct MarkdownPlugin {
    /// surface_id → markdown document state.
    docs: HashMap<u32, MdDoc>,
    /// large-file 이벤트 발행용 Event Bus 핸들(`on_start` 에서 저장).
    bus: Option<BusHandle>,
    /// host IPC 호출용 핸들(`on_start` 에서 저장) — `create_surface`/`on_event`/
    /// `on_webview_navigation_attempt` 컨텍스트에는 host 필드가 없다(SDK 계약). idle 감시
    /// worker 로 옮기는 클론과는 별개(그쪽은 감시 worker 전용).
    host: Option<HostHandle>,
    /// popup instance_id → 대용량 확인 대상.
    confirm: HashMap<u64, LargeFileConfirm>,
    /// popup instance_id → 파일열기 팝업 상태(경로 입력 버퍼).
    file_open: HashMap<u64, FileOpenState>,
    /// `file_picker.trigger`(ADR-0058) 로 보낸 요청의 `request_id` → 그
    /// 요청을 낸 파일열기 팝업 instance_id. `"file_picker.result"` 이벤트 수신 시
    /// 이 맵으로 상관관계를 맞춰 `path_input` 을 채운다.
    pending_file_picker: HashMap<u64, u64>,
    /// popup instance_id → egui-mesh popup 렌더 상태(폰트 atlas·shared buffer 소유).
    /// 대용량/파일열기 확인 팝업 전용 — 본문은 더 이상 이 채널을 쓰지 않는다.
    #[cfg(any(unix, windows))]
    popups: HashMap<u64, EguiMeshPopup>,
    /// CJK fallback 폰트를 이미 설치한 popup instance_id — set_fonts 재업로드 방지.
    #[cfg(any(unix, windows))]
    popup_fonts_installed: std::collections::HashSet<u64>,
    /// plugin lang 카탈로그 (state.failed / state.empty / addr.* 등 UI 문자열).
    tr: Translator,
    /// idle auto-reload 감시 worker(SDK `file_watch::run`) 로 등록/해제 명령을 보내는 채널
    /// (단계 06). `on_start` 에서 worker 를 spawn 하며 채워진다 — 그 전에는 감시가
    /// 비활성(사실상 도달하지 않음, worker_loop 이 on_start 를 먼저 호출).
    watch_tx: Option<mpsc::Sender<WatchCmd>>,
}

impl MarkdownPlugin {
    fn new(tr: Translator) -> Self {
        Self {
            docs: HashMap::new(),
            bus: None,
            host: None,
            confirm: HashMap::new(),
            file_open: HashMap::new(),
            pending_file_picker: HashMap::new(),
            #[cfg(any(unix, windows))]
            popups: HashMap::new(),
            #[cfg(any(unix, windows))]
            popup_fonts_installed: std::collections::HashSet::new(),
            tr,
            watch_tx: None,
        }
    }
}

impl Plugin for MarkdownPlugin {
    fn id(&self) -> &str {
        PLUGIN_ID
    }

    fn version(&self) -> &str {
        PLUGIN_VERSION
    }

    fn on_start(&mut self, host: HostHandle, bus: BusHandle) {
        // large-file 확인 이벤트 발행 + `theme.changed` 구독에 Event Bus 핸들이 필요하다.
        if let Err(e) = bus.subscribe(THEME_CHANGED_EVENT) {
            tracing::warn!("markdown: theme.changed subscribe failed: {e}");
        }
        self.bus = Some(bus);
        // `create_surface`/`on_event`/`on_webview_navigation_attempt` 는 host 를 받지
        // 않으므로(SDK 계약) 별도로 보관한다 — idle 감시 worker 로 옮기는 클론과는 독립.
        self.host = Some(host.clone());

        // idle auto-reload: paint 에 종속되지 않는 별도 스레드가 파일 내용 지문을
        // 폴링하다가 변경을 감지하면 이 plugin 자신의 `markdown.reload` IPC 를
        // self_invoke 한다(worker 는 read 하지 않음 — 상세는 SDK `file_watch` 모듈
        // 문서). webview kind 는 `paint`/`set_context` 를 전혀 받지 않으므로
        // `SurfaceInvalidated` 기반 idle-invalidate 경로는 쓰지 않는다.
        //
        // 판정자는 `ContentDigest` — markdown 은 읽기 비용이 문서 크기로 유계라
        // 매 폴 전량을 읽는 교환이 성립한다(근거는 그 타입의 문서).
        let (tx, rx) = mpsc::channel();
        self.watch_tx = Some(tx);
        if let Err(e) = std::thread::Builder::new()
            .name("markdown-watch".to_string())
            .spawn(move || file_watch::run::<ContentDigest>(host, rx, "markdown.reload"))
        {
            tracing::warn!("markdown watch worker spawn failed — idle auto-reload disabled: {e}");
        }
    }

    fn create_surface(&mut self, ctx: SurfaceCreateCtx) -> SurfaceResult {
        // SDK 는 surface.create 의 **전체 envelope** 을 `ctx.params` 로 넘긴다 — 실제 생성
        // params(file 등)는 `params.params` 아래에 중첩돼 있다.
        //
        // 대용량 파일(> LARGE_FILE_LIMIT_BYTES)은 **plugin in-process** 로 크기를 감지해
        // read 를 보류하고(확인 대기), large-file 이벤트를 발행한다. host 는 파일 크기를
        // stat 하지 않는다(크기게이트는 plugin 소유). 이벤트 → host `fire_popup_triggers`
        // → 이 plugin 의 `[[contributes.popup]]`(event trigger) 확인 팝업이 열린다.
        if let Some(remote_file) = surface_param_remote_file(&ctx.params) {
            return self.open_remote_surface(ctx.surface_id, remote_file);
        }
        let file = surface_param_file(&ctx.params);
        self.open_file_surface(ctx.surface_id, file)
    }

    // layout 재시작 복원 경로. preset apply 는 `surface.create` 를 타지만 layout
    // 재시작은 `surface.restore` 를 탄다 — SDK 기본 구현은 빈 `SurfaceResult` 라,
    // 구현하지 않으면 재시작 시 markdown 이 file 을 잃고 빈 채로 살아난다. create 가
    // 실어 둔 snapshot(`{"file": ...}`)을 그대로 받아 같은 문서를 연다.
    fn restore_surface(&mut self, ctx: SurfaceRestoreCtx) -> SurfaceResult {
        // attach mirror 문서가 plugin kind 등록을 기다렸다 실제화될 때도 이 경로를 탄다 —
        // host 는 생성 params 와 같은 모양(`{display_name, remote: {file}}`)을 data 로 싣는다.
        // 그 경로에는 생성 params 가 없어 탭 제목을 여기서 돌려준다.
        if let Some(remote_file) = remote_file_of(&ctx.data) {
            let mut result = self.open_remote_surface(ctx.surface_id, remote_file);
            result.display_name = ctx
                .data
                .get("display_name")
                .and_then(|v| v.as_str())
                .map(str::to_string);
            return result;
        }
        let file = ctx
            .data
            .get("file")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        self.open_file_surface(ctx.surface_id, file)
    }

    fn destroy_surface(&mut self, surface_id: u32) {
        self.docs.remove(&surface_id);
        self.watch_unregister(surface_id);
    }

    fn handle_ipc_method(&mut self, ctx: IpcMethodCtx) -> Result<Value, IpcMethodError> {
        match ctx.method.as_str() {
            "markdown.reload" => self.markdown_reload(&ctx.params),
            // host 가 구현한 이름이다(surface 를 열고 있는 창을 host 가 안다). 이
            // namespace 를 plugin 이 점유하는 순간 외부 호출은 전부 여기로 forward 되므로,
            // arm 이 없으면 host 구현이 **외부에서만** 안 닿는다 — plugin 이 설치돼 있으면
            // 막히고 빠지면 열리는, 설치 상태에 따라 흔들리는 표면이 된다.
            // 실측(2026-09-05): arm 이 없을 때 외부 `markdown.navigate` 는 plugin 의
            // not_found 로 끝났고, plugin 을 빼면 같은 호출이 host arm 에 닿았다.
            // image.open/list 와 같은 self-call trampoline 로 host 에 돌려준다.
            "markdown.navigate" => Ok(ctx.host.call(&ctx.method, ctx.params)?),
            // 최근목록 조회는 host 소유(AppState.recent_files) — plugin 은 저장소를 못 본다.
            // host 는 generic `recent.query {kind}` 만 알고 "markdown" 을 모른다. CLI/주소창
            // caller 가 이 plugin namespace 로 보낸 호출을 host 의 generic 메서드로 kind 를
            // 채워 trampoline 한다(host 무지 유지 — image.open/list 와 동형 host-adapter).
            "markdown.recent" => Ok(ctx
                .host
                .call("recent.query", json!({ "kind": "markdown" }))?),
            other => Err(IpcMethodError::not_found(other)),
        }
    }

    fn open_popup(&mut self, ctx: PopupOpenCtx) -> PopupOpenResult {
        // egui-mesh popup 이라 tree 를 반환하지 않는다(mesh 채널 paint_popup 로 그린다).
        // popup_id 로 두 팝업을 구분한다.
        match ctx.popup_id.as_str() {
            // large-file 확인 — event payload({surface_id, path, size})가 context 로 온다.
            "large-file-confirm" => {
                if let (Some(surface_id), Some(path), Some(size)) = (
                    ctx.context.get("surface_id").and_then(|v| v.as_u64()),
                    ctx.context.get("path").and_then(|v| v.as_str()),
                    ctx.context.get("size").and_then(|v| v.as_u64()),
                ) {
                    self.confirm.insert(
                        ctx.instance_id,
                        LargeFileConfirm {
                            surface_id: surface_id as u32,
                            file_name: basename(path),
                            size_label: format_size(size),
                        },
                    );
                }
            }
            // 파일열기 폼 — 경로 입력 버퍼는 빈 상태로 시작. context 에 `surface_id` 가
            // 있으면 그 surface 를 제자리 변환할 대상으로 기억한다(없으면 새 탭 열기).
            "file-open" => {
                let convert_surface_id = ctx
                    .context
                    .get("surface_id")
                    .and_then(|v| v.as_u64())
                    .map(|v| v as u32);
                self.file_open.insert(
                    ctx.instance_id,
                    FileOpenState {
                        path_input: String::new(),
                        convert_surface_id,
                        picker_start: PickerStart::from_context(&ctx.context),
                    },
                );
            }
            other => {
                tracing::warn!("markdown open_popup: unknown popup_id '{other}'");
            }
        }
        PopupOpenResult::default()
    }

    fn paint_popup(&mut self, ctx: PopupSetContextCtx) {
        // instance 가 어느 팝업 맵에 있는지로 분기한다(paint 시점엔 popup_id 가 없다).
        let iid = ctx.params.instance_id;
        if self.file_open.contains_key(&iid) {
            self.paint_file_open(ctx);
        } else {
            self.paint_confirm(ctx);
        }
    }

    fn on_popup_closed(&mut self, ctx: PopupClosedCtx) {
        let iid = ctx.instance_id;
        #[cfg(any(unix, windows))]
        {
            self.popups.remove(&iid);
            self.popup_fonts_installed.remove(&iid);
        }
        // 확인 없이 닫힘(취소/outside-click/Esc)이면 surface 는 대기(빈) 상태로 유지한다.
        self.confirm.remove(&iid);
        self.file_open.remove(&iid);
        // 이 팝업이 낸 file_picker.trigger 요청이 아직 응답 전이면 상관관계 항목을
        // 같이 정리한다 — 늦게 도착한 결과는 (그때 iid 를 못 찾으므로) 조용히 무시된다.
        self.pending_file_picker.retain(|_, v| *v != iid);
    }

    /// host 가 push 하는 이벤트: `"file_picker.result"`(ADR-0058)와 `"theme.changed"`.
    fn on_event(&mut self, ctx: EventDispatchCtx) {
        match ctx.envelope.key.as_str() {
            FILE_PICKER_RESULT_EVENT => {
                let Ok(reply) =
                    serde_json::from_value::<FilePickerResultWire>(ctx.envelope.payload)
                else {
                    tracing::warn!("markdown: malformed file_picker.result event");
                    return;
                };
                let Some(iid) = self.pending_file_picker.remove(&reply.request_id) else {
                    return;
                };
                if reply.cancelled {
                    return;
                }
                if let (Some(path), Some(st)) =
                    (reply.paths.into_iter().next(), self.file_open.get_mut(&iid))
                {
                    st.path_input = path;
                }
            }
            // 전역 테마가 바뀌면(라이트/다크 토글 등) 살아있는 모든 문서를 최신 CSS 로
            // 재생성한다 — webview-kind surface 는 `surface.set_context` 를 받지 않아
            // Theme 이 자동으로 밀리지 않는다(`host_api/webview.rs::handle_theme_query` 문서).
            THEME_CHANGED_EVENT => self.reload_all_webviews(),
            MIRROR_CONTENT_RESULT_EVENT => self.on_mirror_content_result(ctx.envelope.payload),
            MIRROR_CHANGED_EVENT => self.on_mirror_changed(&ctx.envelope.payload),
            _ => {}
        }
    }

    /// `webview.navigation_attempt`(Stage A) — 소유 webview surface 가 navigation 을
    /// 시도함(주소창 Go/Enter, 콘텐츠 링크 클릭). `render.rs` 가 생성한 문서는 모든
    /// 실제 목적지를 `#tasty-nav:{link,addr}:<enc>` fragment 로 감싸므로, 그 마커가
    /// 없는 navigation 시도는 이 plugin 이 낸 것이 아니라 조용히 무시한다.
    fn on_webview_navigation_attempt(&mut self, ctx: WebviewNavigationAttemptCtx) {
        let Some(intent) = render::parse_nav_fragment(&ctx.url) else {
            return;
        };
        let Some(host) = self.host.clone() else {
            tracing::warn!(
                "markdown surface {}: navigation attempt before host handle ready — dropping",
                ctx.surface_id
            );
            return;
        };
        let is_remote = self
            .docs
            .get(&ctx.surface_id)
            .is_some_and(|d| d.remote.is_some());
        match intent {
            render::NavIntent::Refresh if is_remote => {
                self.request_remote_content(ctx.surface_id, RemoteRequester::Plugin)
            }
            // 버튼은 mirror 문서에만 그려진다 — 로컬 문서에 온 것은 이 plugin 이 낸 것이 아니다.
            render::NavIntent::Refresh => {}
            // mirror 문서의 파일 링크와 주소창 경로는 원격 호스트의 것이라 이 머신에서 열 수
            // 없다. 외부 URL 만 연다.
            render::NavIntent::Link(dest) if is_remote => {
                if let Some(render::LinkClick::External(url)) = render::classify_link(&dest, None) {
                    dispatch_external_link(&host, ctx.surface_id, &url);
                }
            }
            render::NavIntent::Addr(_) if is_remote => {}
            render::NavIntent::Link(dest) => {
                let base_dir = self
                    .docs
                    .get(&ctx.surface_id)
                    .and_then(|d| d.base_dir.clone());
                if let Some(click) = render::classify_link(&dest, base_dir.as_deref()) {
                    dispatch_link(&host, ctx.surface_id, click);
                }
            }
            render::NavIntent::Addr(path) => navigate(&host, ctx.surface_id, &path),
        }
    }
}

impl MarkdownPlugin {
    /// `markdown_mirror.content_result` — 대기 중인 요청의 회신이면 문서에 반영하고 다시 그린다.
    fn on_mirror_content_result(&mut self, payload: Value) {
        let Ok(reply) = serde_json::from_value::<MirrorContentResultWire>(payload) else {
            tracing::warn!("markdown: malformed {MIRROR_CONTENT_RESULT_EVENT} event");
            return;
        };
        let surface_id = reply.surface_id;
        if self
            .docs
            .get_mut(&surface_id)
            .is_some_and(|doc| doc.apply_remote_result(reply))
        {
            self.reload_webview(surface_id);
        }
    }

    /// `markdown_mirror.changed` — 원문을 보여 주는 문서는 stale 표시만 켜고, 끊김·실패를
    /// 보여 주는 문서는 다시 요청한다([`MdDoc::on_remote_changed`]).
    fn on_mirror_changed(&mut self, payload: &Value) {
        let Some(surface_id) = payload
            .get("surface_id")
            .and_then(|v| v.as_u64())
            .map(|v| v as u32)
        else {
            tracing::warn!("markdown: malformed {MIRROR_CHANGED_EVENT} event");
            return;
        };
        let change = self
            .docs
            .get_mut(&surface_id)
            .map_or(RemoteChange::Ignore, MdDoc::on_remote_changed);
        match change {
            RemoteChange::Ignore => {}
            RemoteChange::Redraw => self.reload_webview(surface_id),
            RemoteChange::Refetch => {
                self.request_remote_content(surface_id, RemoteRequester::Plugin)
            }
        }
    }

    fn markdown_reload(&mut self, params: &Value) -> Result<Value, IpcMethodError> {
        let surface_id = params
            .get("surface")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| IpcMethodError::invalid_params("missing 'surface'"))?
            as u32;
        // mirror 문서는 원격 원문을 다시 요청한다 — 회신이 오면 그때 다시 그린다.
        // mirror 문서는 감시에 등록되지 않으므로 이 IPC 는 idle 감시의 self_invoke 가 아니라
        // 바깥 호출자(`tasty markdown reload` — 에이전트)만 부른다. 그래서 그 회신의 잘림
        // toast 를 host 가 사용자에게 띄우지 않도록 요청에 표시한다(identity 원칙 1).
        if self
            .docs
            .get(&surface_id)
            .is_some_and(|d| d.remote.is_some())
        {
            self.request_remote_content(surface_id, RemoteRequester::Agent);
            return Ok(json!({ "ok": true, "surface_id": surface_id }));
        }
        if let Some(doc) = self.docs.get_mut(&surface_id) {
            doc.force_reload();
        }
        self.reload_webview(surface_id);
        Ok(json!({ "ok": true, "surface_id": surface_id }))
    }

    /// 문서를 만든다. 파일이 임계값을 *초과* 하면 read 를 보류(`new_deferred`)하고
    /// large-file 이벤트를 발행해 확인 팝업을 띄운다(크기 감지는 plugin in-process).
    /// bus 가 없으면(초기화 전) 게이트를 건너뛰고 즉시 로드한다(fail-open).
    /// create/restore 공용 — file 로 문서를 열고, host 에 snapshot(`{"file": ...}`)을
    /// 올려 layout/preset round-trip 에 file 을 보존한다. host 는 이 snapshot 을
    /// `RemoteSurface.snapshot_cache` 로 캐시했다가 `SavedSurface::Generic.data` 로
    /// 저장하고, 다음 실행의 `surface.restore` 에 `data` 로 되돌려준다. file 이 없으면
    /// 저장할 것이 없어 `None`(호스트는 기존 캐시 유지).
    fn open_file_surface(&mut self, surface_id: u32, file: Option<String>) -> SurfaceResult {
        let doc = self.make_doc(file.clone(), surface_id);
        self.docs.insert(surface_id, doc);
        // idle 감시 등록(단계 06). `markdown.navigate` 제자리 이동도 같은 surface_id 로
        // create_surface 를 다시 호출하므로 여기서 자연스럽게 갱신된다.
        self.watch_register(surface_id, file.clone());
        // 문서를 HTML 로 렌더해 host WebView 에 싣는다 — 이 kind 는 mesh 를 그리지 않는다.
        self.reload_webview(surface_id);
        SurfaceResult {
            display_name: None,
            snapshot: file.as_ref().map(|f| json!({ "file": f })),
        }
    }

    /// attach mirror 문서를 연다. 파일 감시도 snapshot 도 없다 — 원문의 주인은 원격이고,
    /// mirror 워크스페이스는 저장되지 않는다. 원문 요청을 먼저 걸고 로딩 상태를 그린다.
    fn open_remote_surface(&mut self, surface_id: u32, file: String) -> SurfaceResult {
        self.docs.insert(surface_id, MdDoc::new_remote(file));
        self.request_remote_content(surface_id, RemoteRequester::Plugin);
        SurfaceResult {
            display_name: None,
            snapshot: None,
        }
    }

    /// mirror 문서의 원문을 host 에 요청하고 다시 그린다. 호출이 실패하면 그 사유를 문서의
    /// 실패 상태로 둔다 — 회신이 올 길이 없으므로 로딩 상태로 남기지 않는다.
    fn request_remote_content(&mut self, surface_id: u32, requester: RemoteRequester) {
        let Some(host) = self.host.clone() else {
            tracing::warn!(
                "markdown surface {surface_id}: no host handle yet — cannot request remote content"
            );
            return;
        };
        let outcome = host
            .call(
                MIRROR_CONTENT_REQUEST_METHOD,
                mirror_content_request_params(surface_id, requester),
            )
            .map_err(|e| e.to_string())
            .and_then(|v| {
                v.get("request_id")
                    .and_then(|id| id.as_u64())
                    .ok_or_else(|| format!("{MIRROR_CONTENT_REQUEST_METHOD}: no request_id"))
            });
        let Some(doc) = self.docs.get_mut(&surface_id) else {
            return;
        };
        let Some(remote) = doc.remote.as_mut() else {
            return;
        };
        match outcome {
            Ok(request_id) => remote.pending = Some(request_id),
            Err(e) => {
                tracing::warn!("markdown surface {surface_id}: remote content request failed: {e}");
                remote.pending = None;
                doc.load_error = Some(e);
            }
        }
        self.reload_webview(surface_id);
    }

    fn make_doc(&self, file: Option<String>, surface_id: u32) -> MdDoc {
        if let Some(path) = file.as_deref()
            && let Some(size) = file_exceeds_limit(path)
        {
            if let Some(bus) = self.bus.as_ref() {
                let payload = json!({
                    "surface_id": surface_id,
                    "path": path,
                    "size": size,
                });
                if let Err(e) =
                    bus.publish_fresh(LARGE_FILE_EVENT_KEY, payload, EventScope::Surface)
                {
                    tracing::warn!("markdown large-file event publish failed: {e}");
                }
                return MdDoc::new_deferred(file);
            }
            tracing::warn!("markdown large-file gate: event bus unavailable — loading anyway");
        }
        MdDoc::new(file)
    }

    /// idle 감시 worker(단계 06)에 surface 의 감시 대상 경로를 등록/갱신한다. worker 가
    /// 없으면(spawn 실패) 조용히 무시 — idle auto-reload 만 비활성화된다(webview kind 는
    /// paint 되지 않으므로 대체할 다른 자동 갱신 경로가 없다 — `markdown.reload` 를 명시
    /// 호출하거나 파일을 다시 열어야 갱신된다).
    fn watch_register(&self, surface_id: u32, path: Option<String>) {
        let Some(tx) = &self.watch_tx else { return };
        if tx.send(WatchCmd::Register { surface_id, path }).is_err() {
            tracing::warn!(
                "markdown watch: register send failed for surface {surface_id} (worker gone)"
            );
        }
    }

    /// idle 감시 worker(단계 06)에서 surface 를 해제한다.
    fn watch_unregister(&self, surface_id: u32) {
        let Some(tx) = &self.watch_tx else { return };
        if tx.send(WatchCmd::Unregister { surface_id }).is_err() {
            tracing::warn!(
                "markdown watch: unregister send failed for surface {surface_id} (worker gone)"
            );
        }
    }

    /// 문서를 (재)렌더해 host WebView 에 싣는다: 현재 Theme + recent 목록을 조회하고
    /// [`render::render_document`] 로 sanitize 된 HTML 문서를 만들어 `webview.set_url` 로
    /// 전달한다. host 의 `sync_webviews` 가 scheme 없는 문자열을 raw HTML 로 인식해
    /// `PlatformWebView::load_html` 을 호출한다(`src/view/main/redraw.rs`).
    fn reload_webview(&self, surface_id: u32) {
        let Some(host) = &self.host else {
            tracing::warn!(
                "markdown surface {surface_id}: no host handle yet (on_start not called?) — cannot load webview"
            );
            return;
        };
        let Some(doc) = self.docs.get(&surface_id) else {
            // 문서가 없으면 실을 것이 없다. surface 는 이미 만들어져 있으므로 host 는
            // 빈 webview 를 그대로 두고, 화면에는 아무 내용도 안 나온다.
            tracing::warn!(
                "markdown surface {surface_id}: no document registered — nothing to load"
            );
            return;
        };
        // 실패하면 렌더를 통째로 건너뛰어 webview 가 빈 채로 남는다. 그 사실과 사유는
        // `fetch_theme` 이 surface 를 밝혀 남긴다 — 여기서 또 남기지 않는다.
        let Some(theme) = fetch_theme(host, surface_id) else {
            return;
        };
        let recent = fetch_recent(host);
        let file_path = doc.file_path.as_deref().unwrap_or_default();
        let html = render::render_document(render::DocumentInput {
            theme: &theme,
            tr: &self.tr,
            file_path,
            source: &doc.content,
            load_error: doc.load_error.as_deref(),
            base_dir: doc.base_dir.as_deref(),
            recent: &recent,
            remote: doc.remote_view(),
        });
        push_html(host, surface_id, file_path, html);
    }

    /// 살아있는 모든 markdown 문서를 재렌더한다(`theme.changed` 수신 시).
    fn reload_all_webviews(&self) {
        for &surface_id in self.docs.keys() {
            self.reload_webview(surface_id);
        }
    }
}

/// surface.create envelope 에서 attach mirror 문서의 원격 경로를 꺼낸다(`params.remote.file`).
/// `remote` 객체가 있으면 mirror 문서다 — 경로가 비어 있어도(원격이 파일 없이 연 문서) 그렇다.
/// 최상위 `file` 과 자리를 나눈 것은 그 키가 로컬 읽기·감시·cwd 도출을 모두 켜기 때문이다.
fn surface_param_remote_file(envelope: &Value) -> Option<String> {
    remote_file_of(envelope.get("params")?)
}

/// 생성 params(또는 restore data) 객체에서 mirror 문서의 원격 경로를 꺼낸다(`remote.file`).
fn remote_file_of(params: &Value) -> Option<String> {
    let remote = params.get("remote")?.as_object()?;
    Some(
        remote
            .get("file")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
    )
}

/// surface.create envelope 에서 `file` 을 꺼낸다. SDK 가 `ctx.params` 로 넘기는 것은
/// `{surface_id, kind, cwd, params:{file, ...}}` 전체이므로 `params.file` 을 본다(중첩).
/// 혹시 flat 으로 온 경우(`file` top-level)도 fallback 으로 받는다.
fn surface_param_file(envelope: &Value) -> Option<String> {
    envelope
        .get("params")
        .and_then(|p| p.get("file"))
        .or_else(|| envelope.get("file"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

/// 파일이 대용량 임계값([`LARGE_FILE_LIMIT_BYTES`])을 *초과* 하면 그 크기(bytes)를 반환.
/// stat 실패/이하/경계값은 `None`(게이트 통과 — 즉시 로드). 경계값(정확히 limit)은 통과.
fn file_exceeds_limit(path: &str) -> Option<u64> {
    std::fs::metadata(path)
        .ok()
        .map(|m| m.len())
        .filter(|&len| len > LARGE_FILE_LIMIT_BYTES)
}

/// wire 스냅샷을 host 와 동일한 `Theme` 인스턴스로 재구성 (sizing 은 zoom 으로 재도출).
fn theme_from_wire(w: &ThemeWire) -> Theme {
    Theme::with_colors_and_zoom(w.colors.clone(), w.is_light, w.ui_zoom)
}

/// 렌더된 HTML 을 host webview 에 싣고 결과를 로그에 남긴다. `reload_webview` 에서
/// 떼어낸 것은 그 함수의 조기 반환 갈래가 이미 여럿이라, 결과 분기까지 함께 두면
/// 복잡도 게이트(`clippy::cognitive_complexity`)를 넘기 때문이다.
fn push_html(host: &HostHandle, surface_id: u32, file_path: &str, html: String) {
    let html_len = html.len();
    if let Err(e) = host.call(
        "webview.set_url",
        json!({ "surface_id": surface_id, "url": html }),
    ) {
        tracing::warn!("markdown surface {surface_id}: webview.set_url failed: {e}");
    } else {
        // file 이 없는 surface(빈 문서)는 `file=` 가 빈 채로 남는다 — 그것이 곧
        // "경로 없이 열린 문서" 라는 표시다.
        tracing::info!(
            "markdown surface {surface_id}: loaded {html_len} bytes of HTML (file={file_path})"
        );
    }
}

/// host `theme.query` IPC(webview.rs 참조)로 현재 Theme 을 동기 조회한다. webview-kind
/// surface 는 `surface.set_context` 를 받지 않아(egui-mesh 와 달리 host 가 mesh 프레임을
/// 합성하지 않으므로) 이 조회가 유일한 Theme 획득 경로다. 실패하면 `None` — 호출자는
/// 문서 재생성을 건너뛴다(다음 성공한 조회가 갱신할 때까지 이전 내용 유지).
/// `surface_id` 는 로그 전용이다 — 실패하면 그 surface 의 webview 가 빈 채로 남으므로,
/// 어느 자리가 비었는지 이 줄만으로 짚을 수 있어야 한다.
fn fetch_theme(host: &HostHandle, surface_id: u32) -> Option<Theme> {
    match host.call("theme.query", json!({})) {
        Ok(v) => match serde_json::from_value::<ThemeWire>(v) {
            Ok(wire) => Some(theme_from_wire(&wire)),
            Err(e) => {
                tracing::warn!(
                    "markdown surface {surface_id}: malformed theme.query response: {e} — \
                     skipping webview load"
                );
                None
            }
        },
        Err(e) => {
            tracing::warn!(
                "markdown surface {surface_id}: theme.query failed: {e} — skipping webview load"
            );
            None
        }
    }
}

/// 편집 진입 시 host 의 generic `recent.query {kind}` 로 최근목록을 조회한다 — 최신순
/// 최대 10개 경로. host 는 "markdown" 을 모르므로 plugin 이 kind 를 채운다. 문서 생성
/// 시점에 baked-in 되어 주소창 `<datalist>` 후보가 된다(render.rs 모듈 문서 — 웹뷰
/// surface 엔 JS↔plugin 메시지 브리지가 없어 반응형 fetch 를 할 수 없다). 실패하면
/// 빈 목록으로 폴백한다.
fn fetch_recent(host: &HostHandle) -> Vec<String> {
    match host.call("recent.query", json!({ "kind": "markdown" })) {
        Ok(v) => parse_recent(&v),
        Err(e) => {
            tracing::warn!("recent.query fetch failed: {e}");
            Vec::new()
        }
    }
}

/// `recent.query` 응답(`{ "recent": [{ path, file_name }] }`)에서 경로 목록을 추출한다
/// (순수 — 단위테스트로 격리). 응답 형태가 어긋나면 빈 목록.
fn parse_recent(v: &Value) -> Vec<String> {
    v.get("recent")
        .and_then(|r| r.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|e| e.get("path").and_then(|p| p.as_str()).map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// 링크 클릭 부수효과. `webview.navigation_attempt` 로 도착한, 이 plugin 이 생성한
/// `#tasty-nav:link:` fragment 에서만 도달한다(module doc). 파일은 host
/// `file_handler.dispatch`(같은 Pane 새 탭, origin_surface_id)로, 외부 URL 은 host
/// `webview.open_external` 로 OS 핸들러에 넘긴다 — 이 plugin 은 OS 열기를 직접 하지 않는다.
fn dispatch_link(host: &HostHandle, sid: u32, click: render::LinkClick) {
    match click {
        render::LinkClick::File(path) => dispatch_file_link(host, sid, &path),
        render::LinkClick::External(url) => dispatch_external_link(host, sid, &url),
    }
}

fn dispatch_file_link(host: &HostHandle, sid: u32, path: &std::path::Path) {
    if !path.exists() {
        tracing::debug!("markdown link target does not exist: {}", path.display());
        return;
    }
    let params = json!({
        "path": path.to_string_lossy(),
        "depth": "deep",
        "origin_surface_id": sid,
    });
    if let Err(e) = host.call("file_handler.dispatch", params) {
        tracing::warn!("markdown link file dispatch failed: {e}");
    }
}

/// 외부 URL 을 host 의 OS 열기 자리로 보낸다(ADR-0527). host 가 거기서 열기를 한 곳으로 모아
/// debug 스위치(ADR-0511)도 이 열기를 기록한다. `sid` 는 링크가 클릭된 이 plugin 의 surface 다
/// — host 는 자기 surface 에서 온 요청만 연다.
fn dispatch_external_link(host: &HostHandle, sid: u32, url: &str) {
    match host.call("webview.open_external", external_link_params(sid, url)) {
        Ok(v) if v.get("opened").and_then(Value::as_bool) == Some(false) => {
            tracing::warn!("markdown external link was not opened by the OS ({url})");
        }
        Ok(_) => {}
        Err(e) => tracing::warn!("markdown external link open failed ({url}): {e}"),
    }
}

/// `webview.open_external` 파라미터. 호출(`HostHandle`)과 떼어 두어 단위 테스트가 wire 모양을 본다.
fn external_link_params(sid: u32, url: &str) -> Value {
    json!({ "surface_id": sid, "url": url })
}

/// 주소창 확정 이동을 host `markdown.navigate` 로 보낸다 — 같은 surface 제자리 이동.
fn navigate(host: &HostHandle, sid: u32, path: &str) {
    let path = path.trim();
    if path.is_empty() {
        return;
    }
    let params = json!({ "surface_id": sid, "path": path });
    if let Err(e) = host.call("markdown.navigate", params) {
        tracing::warn!("markdown navigate failed: {e}");
    }
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();
    let env = PluginEnv::load()?;
    let tr = Translator::from_plugin_env(&env);
    tasty_plugin_sdk::run(MarkdownPlugin::new(tr))
}

#[cfg(test)]
// 테스트 본문은 `let _ =` 사유 주석 정책의 범위 밖이다(전수 가드가 제외한다) —
// 여기 경고는 조치 대상이 될 수 없어 프로덕션 신호만 가린다. error-handling.md.
#[allow(clippy::let_underscore_must_use)]
mod tests;
