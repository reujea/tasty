//! 헤드리스 전용 PluginManager 부트스트랩 + pump (attach mesh mirror 선행조건 —
//! `docs/dev-guide/attach-behavior.md` "mesh mirror 채널").
//!
//! `App::new_headless` 는 `plugin_manager: None` 으로 뜬다 — 헤드리스는 "GUI 없음"을
//! 전제한 코드 경로가 넓어 상시 초기화는 회귀 위험이 크다. 대신 attach 세션이 실제로
//! (mesh mirror 후보를 포함할 수 있는) workspace 를 mirror 하려 할 때만 lazy 초기화하고,
//! 이후로는 프로세스 수명 동안 유지한다(GUI 도 PluginManager 를 재생성하지 않는 것과
//! 동일한 정책 — 세션당 attach 종료 시 tear-down 하지 않음, 스코프 결정).
//!
//! pump 트리거 (busy-poll 편승 없이 이벤트 기반):
//! - plugin 프로세스의 수신 스레드는 매 라인마다 `waker.make_default_waker()()`
//!   를 호출한다(`tasty-host-plugin` `process.rs`). 이 waker 는 `PluginManager` 를
//!   만들 때 넘긴 `SharedWakerFactory` — 즉 `CoreState::waker_factory` 와 **동일
//!   인스턴스**를 공유한다. 헤드리스에서 default waker 는 `AppEvent::TerminalOutput(None)`
//!   을 발화하므로, plugin 이벤트(hello 응답, `PaintFrame` 등)는 이미 이 이벤트로 host 를
//!   깨운다 — 별도 wake 채널이 필요 없다("PaintFrame 도착 시 즉시 wake" 요구도 이
//!   경로가 충족한다).
//! - plugin 자체 주기 작업(ping/healthcheck/RSS/auto-reload/retire)은 `PluginManager` 가
//!   소유한 타이머 허브가 스케줄한다 — headless 메인 루프가 그 데드라인을 자기
//!   대기 계산에 합성하므로(`docs/dev-guide/timer-hub.md`) plugin 소켓이 조용해도
//!   제때 깨어난다. **깨어난 바퀴가 그 데드라인을 거두는 자리는 [`pump_plugins_if_due`]
//!   다** — 깨우기만 하고 안 거두면 데드라인이 과거에 남아 `recv_timeout(0)` 이 다음
//!   `Tick::Busy`(1 Hz)까지 헛돈다(실측: `PluginTick::Ping` 15 s 마다 약 1 s 동안
//!   루프 160 만 회). 그래서 `Tick::Busy` 편승은 안전망으로만 남는다.

use crate::app::App;
use crate::core::CoreState;
use crate::state::AppState;

/// 매니저를 **디스크를 읽기만 해서** 세운다 — plugin 프로세스를 띄우지 않고,
/// 번들 plugin 을 설치하지도 권한을 grant 하지도 않는다.
///
/// 조회 메서드(`plugin.list` 등)가 부르는 층이다. 조회가 자기 관측 대상을 바꾸면
/// 에이전트가 상태를 **관찰하려고** 부른 명령이 그 상태를 만들어버린다. 그래서
/// 여기서 하는 일은 `refresh_packages` 뿐이고, 그것은 `~/.tasty/plugins/` 를 스캔해
/// `packages`/`rejected` 를 채우는 읽기 연산이다.
///
/// 특히 [`crate::plugin::install_builtins_if_needed`] 는 이 층에 **없다** — 그것은
/// 번들에서 파일을 복사하고 매니페스트 권한을 `plugins.toml` 에 자동 grant 한다.
/// 설치와 권한 부여는 조회의 부수효과일 수 없다.
///
/// 그 결과 아직 아무것도 설치되지 않은 홈에서는 목록이 빈다. 그것은 거짓이 아니라
/// 그 시점의 사실이며, 매니저가 아예 없을 때의 `-32000` 응답과 **구분되는 답**이다.
///
/// `engine.waker_factory` 가 없으면(불변식 위반 — headless 는 부팅 시 항상 설정)
/// 경고만 남기고 스킵한다. 이미 초기화돼 있으면 no-op.
pub(crate) fn ensure_plugin_manager_metadata(app: &mut App, engine: &CoreState) {
    if app.plugin_manager.is_some() {
        return;
    }
    let Some(factory) = engine.waker_factory.clone() else {
        tracing::warn!(
            "headless plugin manager bootstrap skipped: engine has no waker_factory (invariant violated)"
        );
        return;
    };
    let mut mgr = crate::plugin::PluginManager::with_registries(
        factory,
        engine.file_format.clone(),
        engine.file_handler.clone(),
    );
    mgr.set_surface_registry(engine.surface_registry.clone());
    // gui 경로(`build_plugin_manager`)와 같은 묶음을 주입한다 — 두 조합이 같은 게이지를 채운다.
    let gauges = app.core.plugin_gauges();
    mgr.set_plugin_wait(gauges.plugin_wait);
    mgr.set_slow_requests(gauges.slow_requests);
    mgr.set_i18n_registrar(std::sync::Arc::new(crate::i18n::BinI18nRegistrar));
    mgr.set_hook_handler_registry(std::sync::Arc::new(
        crate::hook_handler::HostHookHandlerPort,
    ));
    mgr.set_completion_strategy_registry(std::sync::Arc::new(
        crate::completion_strategy::HostCompletionStrategyPort,
    ));
    mgr.refresh_packages();
    tracing::info!("headless plugin manager bootstrapped (metadata only — no plugin started)");
    app.plugin_manager = Some(mgr);
}

/// attach 세션이 mesh mirror 후보를 mirror 하려 할 때 호출한다.
/// namespace forward는 이 전량 기동 경로 대신 공통 manager의 owner 준비를 쓴다.
///
/// [`ensure_plugin_manager_metadata`] 위에 번들 설치와 프로세스 기동을 얹는다.
/// 조회 경로에서 부르지 않는다(위 함수의 주석 참조).
pub(crate) fn ensure_plugin_manager(app: &mut App, engine: &CoreState) {
    if app.plugin_started {
        return;
    }
    ensure_plugin_manager_metadata(app, engine);
    let Some(mgr) = app.plugin_manager.as_mut() else {
        return;
    };
    crate::plugin::install_builtins_if_needed(mgr);
    mgr.discover_and_start();
    app.plugin_started = true;
    tracing::info!("headless plugin manager started (attach mesh mirror session)");
}

/// GUI `about_to_wait()` plugin 블록의 헤드리스 등가. hello 마무리(surface_kind
/// 등록) + pending plugin IPC 호출의 최소 처리(shared_buffer.create 인터셉트 +
/// 나머지는 기본 라우터로 dispatch)만 수행한다 — popup/banner 인터셉트, namespace
/// forward, host event bus 브로드캐스트 등 GUI 전용/부가 통지는 이 스코프(attach mesh
/// mirror 렌더에 필요한 최소 집합)에서 의도적으로 생략한다(스코프 결정, Gate4 검토 대상).
/// `plugin_manager` 가 `None` 이면 no-op.
pub(crate) fn pump_plugins(app: &mut App, state: &mut AppState, engine: &mut CoreState) {
    if app.plugin_manager.is_none() {
        return;
    }
    let hello_pairs = {
        let mgr = app.plugin_manager.as_mut().expect("checked Some above");
        mgr.pump(std::time::Instant::now())
    };
    if !hello_pairs.is_empty() {
        finalize_plugin_hello_headless(app, engine, hello_pairs);
    }
    dispatch_plugin_ipc_calls_headless(app, state, engine);
    forward_mesh_frames(app, engine);
}

/// plugin 허브의 데드라인이 지났으면 [`pump_plugins`] 를 부른다. 불렀는지를 돌려준다.
///
/// headless 루프는 대기를 두 허브의 `min` 으로 계산하므로(`crate::app::timers::min_deadline`)
/// plugin 데드라인에 깨어난다. 그 바퀴가 plugin 허브를 안 돌리면 데드라인이 그대로
/// 과거에 남고, 다음 대기가 0 이 되어 루프가 헛돈다 — gui 는 `about_to_wait` 가 깨어날
/// 때마다 `mgr.pump` 를 불러 이 형태가 없다. 앱 허브의 due 판정(`drain_due`)과 같은
/// 규칙(`at <= now`)을 쓴다.
pub(crate) fn pump_plugins_if_due(
    app: &mut App,
    state: &mut AppState,
    engine: &mut CoreState,
    now: std::time::Instant,
) -> bool {
    let deadline = app.plugin_manager.as_ref().and_then(|m| m.next_deadline());
    run_if_due(deadline, now, || pump_plugins(app, state, engine))
}

/// 데드라인이 지났으면 `pump` 를 한 번 부른다 — 판정을 [`pump_plugins_if_due`] 에서 떼어
/// `App` 없이 시험한다.
fn run_if_due(
    deadline: Option<std::time::Instant>,
    now: std::time::Instant,
    pump: impl FnOnce(),
) -> bool {
    let due = deadline.is_some_and(|at| at <= now);
    if due {
        pump();
    }
    due
}

/// `CoreState::mesh_mirror`(구독 상태)를 읽어 plugin 을 구동하고, 새 frame 을 attach
/// client 에 chunk forward 한다(상세
/// `docs/dev-guide/egui-mesh-channel.md#attach-mesh-mirror-소비-경로`).
/// `pump_plugins` 호출 tick 마다 실행돼
/// `PaintFrame` 도착 즉시(또는 1Hz busy-poll 안전망 tick 에) 반응한다 — 별도 wake
/// 채널 불필요(모듈 문서 §pump 트리거 참조).
///
/// 구독당 두 가지 독립 동작:
/// 1. **구독 상태가 dirty**(신규 구독/geometry·theme·focus 변경) — plugin 에
///    `surface.set_context` 재전송(첫 호출이면 `surface.create` bootstrap 선행).
/// 2. **아직 이 client 에 안 보낸 새 generation 의 frame 존재** — `SharedBuffer`에서
///    바이트를 읽어 chunk 로 쪼개 client 에 push.
/// 두 동작은 서로 독립이다 — geometry 변경 없이도 plugin 이 새 frame 을 밀 수 있고
/// (markdown 내부 애니메이션 등), 반대로 이번 tick 에 새 frame 이 없어도 geometry 변경은
/// 즉시 반영해야 한다.
fn forward_mesh_frames(app: &mut App, engine: &mut CoreState) {
    let Some(mgr) = app.plugin_manager.as_ref() else {
        return;
    };
    crate::plugin_bridge::mesh_forward::forward_mesh_frames_for_engine(
        engine,
        mgr,
        &app.stream_hub,
    );
}

/// plugin 이 선언한 surface kind 를 지목한 요청이 오면 **그 plugin 하나를** 기동한다.
///
/// **소속은 매니페스트가 답하고, 기동은 소속이 맞은 뒤에만 한다** — `plugin namespace
/// forward`(`boot/headless_dispatch.rs`)와 **같은 두 층**이고 근거도 같다
/// ([ADR-0173](../../docs/adr/0173-namespace-resolution-reads-the-manifest-not-the-process-table.md)).
/// 다른 것은 물음뿐이다: 그쪽은 "이 메서드 이름이 누구 것인가", 여기는 "이 kind 를
/// 누가 선언했는가".
///
/// 이 트리거가 없으면 `remote`/`webview` kind 는 헤드리스에서 **영영 등록되지
/// 않는다.** 등록은 hello 가 하고, 헤드리스에서 plugin 이 뜨는 자리는 attach 세션
/// (`boot/headless_stream.rs`)과 namespace forward 둘뿐인데, `tab.create` 는 그
/// 어느 쪽도 아니기 때문이다. 실측(2026-09-09): 등록만 열고 이 트리거가 없으면
/// `tests/attach_markdown_content_loopback.rs` 의 다섯이 그대로
/// `unknown surface kind: markdown` 으로 죽는다.
///
/// **소속 판정은 매니페스트와 `plugins.toml` 을 함께 읽는다.** 선언만 보고 기동하면
/// 사용자가 끈 plugin 의 kind 를 지목하는 것만으로 데몬이 아래 대기에 들어간다 —
/// 그 plugin 은 (`discover_and_start` 가 disabled 를 거르므로) 영영 안 뜨고, 대기는
/// 시한을 꽉 채운다. 실측(2026-09-10, 이 갈래를 고치기 전): `plugin disable
/// com.tasty.markdown` 뒤 `--type markdown` 한 번이 그 요청을 **5.34 s** 묶고, 그동안
/// 무관한 `list info` 가 **5.04 s** 걸렸다(데몬이 단일 루프라 IPC 전체가 선다).
/// 그래서 disabled 는 소속이 아예 안 맞은 것으로 친다 — 아래 `enabled_owner_of_kind`
/// 가 그 판정이고, 이 모듈의 `tests::a_disabled_plugin_does_not_own_its_kind` 가 그
/// 사실을 값으로 고정한다.
///
/// **띄우는 것은 지목된 하나뿐이다.** 예전에는 `ensure_plugin_manager`
/// (= `discover_and_start`)를 불러 설치된 것을 **전부** 띄웠고, 그래서 markdown 을
/// 한 번 지목하면 프로세스 여덟이 덤으로 남았다. 지금은 `start_one_enabled` 로
/// 소유자만 띄운다 — 요청이 자기가 부르지 않은 관측 대상을 만들지 않는다(ADR-0136 과
/// 같은 축). 설치·권한 grant 도 이 경로에 **없다**: 위 소속 판정이 이미 설치된
/// package 표를 보므로, 여기 닿았다는 것 자체가 설치가 끝났다는 뜻이다.
///
/// **여기서 기다리는 이유.** hello 는 비동기고 등록은 hello 가 한다. 반면 이 요청은
/// 동기라 handler 가 돌기 전에 registry 가 차 있어야 한다. 그래서 hello 를 등록의
/// 유일한 트리거로 두고(사본을 만들지 않는다) 그것이 도착할 때까지 pump 를 돌린다.
/// 기다리는 것은 **우리가 방금 띄운 프로세스**뿐이다 — 이미 떠 있는데 kind 가 아직
/// 없으면 기다려도 원인이 우리 손에 없으므로 그냥 돌아간다. 대기는 두 단계다: 연결 결과가
/// 날 때까지(상한 = 연결 한도, 기동이 연결 전에 돌아오므로 — ADR-0505), 그리고 연결이
/// 성사된 뒤부터 [`KIND_REGISTRATION_WAIT`] 동안 hello 를. 연결에 실패했거나 시한이
/// 지나도록 안 차면 handler 가 예전과 똑같은 `unknown surface kind` 를 답한다.
pub(crate) fn ensure_plugin_for_surface_kind(
    app: &mut App,
    state: &mut AppState,
    engine: &mut CoreState,
    request: &crate::ipc::protocol::JsonRpcRequest,
) {
    // surface 를 만드는 handler 셋(`tab.create` · `pane.split` · `workspace.create`)이
    // 모두 이 한 키로 kind 를 읽는다. 다른 키를 읽는 요청은 여기서 조용히 지나간다 —
    // 잘못 짚어도 하는 일이 없다(등록된 kind 면 아래 첫 검사에서 돌아간다).
    let Some(kind) = request.params.get("type").and_then(|v| v.as_str()) else {
        return;
    };
    if engine.surface_registry.get(kind).is_some() {
        return;
    }
    // 매니페스트와 `plugins.toml` 만 읽는다 — 프로세스는 아직 하나도 안 띄운다.
    ensure_plugin_manager_metadata(app, engine);
    let Some(owner) = app
        .plugin_manager
        .as_ref()
        .and_then(|mgr| enabled_owner_of_kind(mgr, kind))
    else {
        return;
    };
    let kind = kind.to_string();
    let started = app
        .plugin_manager
        .as_mut()
        .is_some_and(|mgr| mgr.start_one_enabled(&owner));
    if !started {
        // 우리가 안 띄웠으면 기다릴 근거가 없다 — 이미 떠 있거나(그럼 hello 는 이
        // 요청과 무관한 시점에 온다) spawn 이 실패한 것이다. 둘 다 시한을 채워도
        // 바뀌지 않으므로 데몬을 묶지 않는다.
        tracing::debug!(
            "surface kind '{kind}' is declared by '{owner}' but nothing was started here; \
             answering without waiting"
        );
        return;
    }
    let connect_limit = app
        .plugin_manager
        .as_ref()
        .map_or(std::time::Duration::ZERO, |mgr| mgr.connection_wait_limit());
    let outcome = wait_for_kind_registration(connect_limit, KIND_REGISTRATION_WAIT, || {
        pump_plugins(app, state, engine);
        if engine.surface_registry.get(&kind).is_some() {
            return OwnerPoll::Registered;
        }
        match app.plugin_manager.as_ref() {
            Some(mgr) if mgr.is_connecting(&owner) => OwnerPoll::Connecting,
            Some(mgr) if mgr.is_running(&owner) => OwnerPoll::Connected,
            _ => OwnerPoll::Gone,
        }
    });
    match outcome {
        OwnerPoll::Registered => {}
        OwnerPoll::Connecting | OwnerPoll::Gone => tracing::warn!(
            "surface kind '{kind}' is declared by '{owner}' but it did not connect or went \
             down; the request will be answered as an unknown kind"
        ),
        OwnerPoll::Connected => tracing::warn!(
            "surface kind '{kind}' is declared by '{owner}' but was not registered within {:?} \
             of its connection; the request will be answered as an unknown kind",
            KIND_REGISTRATION_WAIT
        ),
    }
}

/// 방금 띄운 owner 를 한 번 pump 한 뒤 본 상태.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OwnerPoll {
    /// kind 가 등록됐다 — 기다림이 끝났다.
    Registered,
    /// 떠 있지만 연결 결과가 아직 안 났다.
    Connecting,
    /// 연결했고 hello 를 기다린다.
    Connected,
    /// 연결에 실패해 내려갔다.
    Gone,
}

/// [`ensure_plugin_for_surface_kind`] 의 두 단계 대기. 먼저 연결 결과를 `connect_limit` 까지
/// 기다리고, 연결이 성사된 **그 뒤부터** `registration_wait` 동안 등록을 기다린다 — 등록
/// 시한이 연결 시간을 떠안지 않게(ADR-0505). 기동이 연결까지 막히던 예전에는 연결이 시한
/// 밖에 있었으므로 그것과 같은 몫이다. 돌려주는 것은 마지막으로 본 상태다.
///
/// 판정을 `poll` 하나로 받는 이유는 시험이다 — pump 는 `App` 전체를 요구하므로, 시한을
/// 어디서부터 세는가를 `App` 없이 재려면 대기 규칙을 여기로 떼어야 한다.
fn wait_for_kind_registration(
    connect_limit: std::time::Duration,
    registration_wait: std::time::Duration,
    mut poll: impl FnMut() -> OwnerPoll,
) -> OwnerPoll {
    let connect_give_up = std::time::Instant::now() + connect_limit;
    let mut seen = poll();
    while seen == OwnerPoll::Connecting && std::time::Instant::now() < connect_give_up {
        std::thread::sleep(std::time::Duration::from_millis(5));
        seen = poll();
    }
    if seen != OwnerPoll::Connected {
        return seen;
    }
    let deadline = std::time::Instant::now() + registration_wait;
    while std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(5));
        seen = poll();
        if seen != OwnerPoll::Connected {
            return seen;
        }
    }
    seen
}

/// 이 kind 를 선언했고 **사용자가 끄지 않은** plugin 의 id.
///
/// 두 물음을 한 자리에서 답한다 — *누가 선언했나*(매니페스트)와 *그것이 지금 켜져
/// 있나*(`plugins.toml`). 앞엣것만 보면 `discover_and_start` 가 영영 안 띄울 plugin 을
/// 기다리게 되고, 그 대기가 데몬 전체를 세운다([`ensure_plugin_for_surface_kind`] 의
/// 실측). 뒤엣것만 보는 판정은 없다 — 켜져 있어도 이 kind 를 선언 안 했으면 남이다.
pub(crate) fn enabled_owner_of_kind(
    mgr: &crate::plugin::PluginManager,
    kind: &str,
) -> Option<String> {
    owner_of_kind(
        mgr.packages().iter().map(|pkg| {
            (
                pkg.manifest.id.as_str(),
                pkg.manifest.surface_kinds.as_slice(),
            )
        }),
        |id| mgr.config.is_disabled(id),
        kind,
    )
}

/// 위 판정의 알맹이 — 매니저 없이 값만 받는다.
///
/// 갈라 둔 이유는 시험이다. `PluginManager` 의 `packages` 는 밖에서 채울 수 없어서,
/// 매니저를 받는 채로는 "비활성이면 소속이 아니다" 를 **디스크에 홈을 만들지 않고는**
/// 못 잰다. 그러면 이 규칙의 채널이 실행 확인 하나가 되고, 그건 다음 사람이 안 돌린다.
fn owner_of_kind<'a>(
    packages: impl IntoIterator<Item = (&'a str, &'a [crate::plugin::manifest::SurfaceKindDecl])>,
    is_disabled: impl Fn(&str) -> bool,
    kind: &str,
) -> Option<String> {
    packages
        .into_iter()
        .find(|(id, kinds)| !is_disabled(id) && kinds.iter().any(|d| d.kind == kind))
        .map(|(id, _)| id.to_string())
}

/// [`ensure_plugin_for_surface_kind`] 가 hello 를 기다리는 시한 — **연결이 성사된 뒤부터**
/// 센다. 연결 대기는 그 앞 단계이고 상한이 따로 있다(연결 한도).
///
/// 아래 표가 잰 것은 프로세스 spawn + 연결 + hello **한 번** 전체다. 실측(2026-09-10, 갓 만든 격리
/// 홈의 헤드리스 데몬, `new workspace --type <kind>`):
///
/// | 호출 | 소요 | 그 뒤 running | 등록된 kind |
/// |---|---|---|---|
/// | 부팅 직후 | — | 0 | 4 (`dag_graph`/`empty`/`explorer`/`terminal`) |
/// | `--type nosuchkind` | 0.09 s | 0 | 4 |
/// | `--type markdown` 첫 번째 | 0.14 s | 1 (`com.tasty.markdown`) | 5 |
/// | `--type markdown` 두 번째 | 0.09 s | 1 | 5 |
/// | `--type image` 첫 번째 | 0.14 s | 2 (`+com.tasty.image`) | 6 |
///
/// 읽을 것 셋. ① 대기는 kind 마다 **처음 한 번**이고 그 뒤로는 첫 검사에서 돌아간다.
/// ② **없는 kind 는 이 대기에 안 들어간다** — 소속 판정이 먼저 답한다. ③ 지목한
/// plugin 만 뜬다: markdown 을 물었는데 image 가 딸려 오지 않는다.
///
/// 5 초는 위 0.14 s 의 30 배가 넘는 여유이면서, 뜨다 만 plugin 에 데몬을 오래 묶어
/// 두지 않는 선이다. 이 시한을 꽉 채우는 갈래는 **우리가 띄운 plugin 이 연결까지 했는데
/// hello 가 안 오는 경우** 하나로 좁혀져 있다([`ensure_plugin_for_surface_kind`] 의 `started`
/// 검사와 연결 단계) — 비활성 plugin 을 지목하던 옛 갈래는 소속 판정이 먼저 자르고, 끝내
/// 연결 안 하는 plugin 은 연결 한도에서 끝난다.
const KIND_REGISTRATION_WAIT: std::time::Duration = std::time::Duration::from_secs(5);

/// `src/app/plugin_glue/lifecycle.rs::finalize_plugin_hello` 의 헤드리스 등가.
/// surface_kind registry 등록(egui-mesh 포함) + hook_event 등록만 수행하고, GUI
/// 전용 CoreEvent cascade(toast/이벤트버스 브로드캐스트)는 생략한다 — 그 브로드캐스트는
/// PluginLoaded 등을 구독하는 *다른* plugin/UI 통지용이며, hello 를 마친 plugin 자신의
/// 렌더링에는 영향이 없다(레지스트리 mutation 은 이 함수 안에서 이미 동기 반영됨).
fn finalize_plugin_hello_headless(
    app: &mut App,
    engine: &CoreState,
    hello_pairs: Vec<(String, String)>,
) {
    let core_registry = engine.surface_registry.clone();
    let hook_event_registry = engine.plugin_hook_events.clone();
    let Some(mgr) = app.plugin_manager.as_mut() else {
        return;
    };

    register_hook_events(mgr, &hook_event_registry, &hello_pairs);

    let host_registry = mgr.surface_registry.is_some().then_some(core_registry);
    let Some(registry) = host_registry else {
        tracing::debug!(
            "headless plugin manager has no surface_registry; deferring registration of {} plugin(s)",
            hello_pairs.len()
        );
        for (plugin_id, _) in &hello_pairs {
            mgr.registered_plugins.insert(plugin_id.clone());
        }
        return;
    };

    register_surface_kinds(mgr, &registry, &hello_pairs);
}

/// hello 를 마친 plugin 이 선언한 `contributes.hook_events` 키를 공유 레지스트리에
/// 등록한다. 이 단계가 보는 것은 hook 레지스트리 하나뿐이라 surface_kind 등록과
/// 자원이 겹치지 않는다.
fn register_hook_events(
    mgr: &crate::plugin::PluginManager,
    hook_event_registry: &std::sync::Arc<crate::core::hook_event_registry::PluginHookEventRegistry>,
    hello_pairs: &[(String, String)],
) {
    for (plugin_id, _) in hello_pairs {
        if let Some(pkg) = mgr.packages().iter().find(|p| &p.manifest.id == plugin_id) {
            let keys: Vec<String> = pkg
                .manifest
                .contributes
                .hook_events
                .iter()
                .map(|h| h.key.clone())
                .collect();
            if !keys.is_empty() {
                hook_event_registry.register(plugin_id, keys);
            }
        }
    }
}

/// 선언된 surface_kind 를 등록하고 plugin 을 등록 완료로 표시한다.
/// 세 rendering 종류를 전부 등록한다 — gui 와 같은 집합이다([`register_one_surface_kind`]).
fn register_surface_kinds(
    mgr: &mut crate::plugin::PluginManager,
    registry: &std::sync::Arc<crate::core::surface_registry::SurfaceKindRegistry>,
    hello_pairs: &[(String, String)],
) {
    let host_cmd_tx = mgr.host_cmd_tx.clone();
    for (plugin_id, _version) in hello_pairs {
        if let Some(pkg) = mgr
            .packages()
            .iter()
            .find(|p| &p.manifest.id == plugin_id)
            .cloned()
        {
            for decl in &pkg.manifest.surface_kinds {
                if let Some(default) = &decl.default_colors {
                    tasty_themes::add_plugin_surface_default(&decl.kind, default.clone());
                }
                register_one_surface_kind(
                    registry,
                    plugin_id,
                    &pkg.manifest.api_version,
                    decl,
                    &host_cmd_tx,
                );
            }
        }
        mgr.registered_plugins.insert(plugin_id.clone());
    }
}

/// surface_kind 선언 하나를 rendering 종류에 따라 등록한다.
///
/// **세 rendering 을 전부 등록한다 — gui 와 같은 집합이다.** 한때 여기서
/// `remote`/`webview` 를 건너뛰었고 그 사유는 "실제 렌더가 창을 전제하는 surface 라
/// headless 에 재현할 대상이 없다" 였다. 그 사유가 [ADR-0255](../../docs/adr/0255-markdown-attach-mirror-forwards-content-not-pixels.md)
/// 로 무너졌다 — markdown mirror 가 나르는 것은 픽셀이 아니라 **원문**이고, 그리는
/// 것은 client 다. 서버가 하는 일은 파일 read 와 control 프레임 왕복뿐이라 창이
/// 필요 없다. 실제로 그 채널의 서버측 코드(`src/core/attach_runtime.rs`)에는 feature
/// 게이트가 하나도 없다 — 없던 것은 **이 kind 를 등록하는 한 줄**뿐이었고, 그것 때문에
/// 헤드리스 데몬에서는 `tab.create {type:"markdown"}` 이 `unknown surface kind` 로
/// 죽어 그 채널 전체에 닿을 방법이 없었다(`docs/identity.md` 원칙 2 — 에이전트 기능은
/// 조합에 따라 사라지지 않는다).
///
/// 탈 것도 이미 양쪽에 있다: `RemoteSurface`(`src/plugin_bridge/remote_surface.rs`)는
/// 비-gui 빌드에서도 컴파일되도록 **일부러** 게이트 밖에 두었고, `host_cmd` 도
/// 그래서 무조건 re-export 된다. gui 전용으로 남는 것은 그 surface 를 **그리는**
/// 쪽(`sync_webviews` · webview overlay)이지 등록이 아니다.
fn register_one_surface_kind(
    registry: &std::sync::Arc<crate::core::surface_registry::SurfaceKindRegistry>,
    plugin_id: &str,
    api_version: &str,
    decl: &crate::plugin::manifest::SurfaceKindDecl,
    host_cmd_tx: &std::sync::mpsc::Sender<crate::plugin_bridge::host_cmd::HostCmd>,
) {
    match decl.rendering {
        crate::plugin::manifest::SurfaceKindRendering::Webview => {
            // gui 의 `register_plugin_surface_kinds` 와 **같은 두 호출**이다. overlay
            // 플래그를 읽는 것은 gui 의 매 프레임 `sync_webviews` 뿐이라, 헤드리스에서
            // 세워 두어도 소비자가 없어 아무 일도 일어나지 않는다 — 대신 조합에 따라
            // 등록 사실이 갈리지 않는다.
            crate::core::surface_registry::webview_kind::register_webview_kind(
                plugin_id, &decl.kind,
            );
            crate::plugin_bridge::remote_kind::register_remote_kind(
                registry,
                plugin_id,
                decl,
                host_cmd_tx.clone(),
            );
        }
        crate::plugin::manifest::SurfaceKindRendering::Remote => {
            crate::plugin_bridge::remote_kind::register_remote_kind(
                registry,
                plugin_id,
                decl,
                host_cmd_tx.clone(),
            );
        }
        crate::plugin::manifest::SurfaceKindRendering::EguiMesh => {
            crate::core::surface_registry::egui_mesh::register_egui_mesh_kind(
                registry,
                plugin_id,
                decl,
                api_version,
            );
        }
    }
}

/// 헤드리스 진입부의 pre-gate. GUI 의 `App::gates_before_routing` 과 같은 3종을
/// 같은 순서로 돌린다. 헤드리스는 engine 이 항상 하나라 그쪽의 view 탐색이 필요 없다.
fn gates_before_intercept<'a>(
    app: &mut App,
    window: &mut dyn crate::ipc::window_port::IpcWindow,
    engine: &mut CoreState,
    request: &'a crate::ipc::protocol::JsonRpcRequest,
    caller: &'a crate::ipc::caller::CallerContext,
) -> Result<crate::ipc::handler::CheckedRequest<'a>, crate::ipc::protocol::JsonRpcResponse> {
    crate::ipc::handler::check_request(&mut app.core, window, engine, request, caller)
}

/// `src/app/dispatch/plugin_ipc.rs::process_plugin_ipc_calls` 의 헤드리스 등가.
/// 게이트 3종을 인터셉트보다 먼저 돌리는 순서까지 같다(ADR-0152).
/// `host.shared_buffer.create` 는 egui-mesh 프레임 생성에 필수라 그대로 인터셉트한다.
/// popup.close/banner.open/banner.close 는 헤드리스에 대응하는 GUI 상태(popup/banner
/// overlay, view)가 없어 생략한다.
///
/// **namespace forward 는 그 근거가 아니다.** 여기서 빠져 있는 것은 plugin → plugin
/// 방향(`forward_namespace_call_from_plugin`)이고, 그건 GUI 상태와 무관하다 — 같은
/// 근거 문장에 묶여 있었을 뿐이다. host → plugin 방향은 이제 `headless_dispatch.rs`
/// 가 배선한다. plugin → plugin 방향은 아직 없다: 한 plugin 이 다른 plugin 의
/// namespace 를 부르는 시나리오가 헤드리스에서 관측된 적이 없어 남겨 두는 것이며,
/// 관측되면 그때 gui `app/dispatch/plugin_ipc.rs` 를 본떠 배선하면 된다.
fn dispatch_plugin_ipc_calls_headless(app: &mut App, state: &mut AppState, engine: &mut CoreState) {
    let calls = match app.plugin_manager.as_mut() {
        Some(mgr) => mgr.take_pending_plugin_calls(),
        None => return,
    };
    for call in calls {
        let caller = crate::ipc::caller::CallerContext::Plugin {
            plugin_id: call.plugin_id.clone(),
            permissions: call.permissions.clone(),
        };
        let request = crate::ipc::protocol::JsonRpcRequest {
            response_timeout_ms: None,
            idempotency_key: None,
            jsonrpc: "2.0".to_string(),
            id: Some(serde_json::Value::from(call.call_id)),
            method: call.method.clone(),
            params: call.params.clone(),
            session_token: None,
        };
        // 게이트 3종이 **인터셉트보다 먼저** 돈다 — GUI 진입부와 같은 순서다
        // (ADR-0152). 아래 인터셉트는 `handle_with_caller` 에 도달하지 않으므로,
        // 게이트가 그 함수 안에만 있으면 그 갈래만 권한·cap·rate·audit 를 통째로
        // 건너뛴다.
        let checked = match gates_before_intercept(app, state, engine, &request, &caller) {
            Ok(checked) => checked,
            Err(resp) => {
                let (msg, code) = match resp.error {
                    Some(e) => (Some(e.message), Some(e.code)),
                    None => (None, None),
                };
                if let Some(mgr) = app.plugin_manager.as_mut() {
                    mgr.send_ipc_result(&call.plugin_id, call.call_id, None, msg, code);
                }
                continue;
            }
        };
        if call.method == tasty_plugin_protocol::METHOD_HOST_SHARED_BUFFER_CREATE {
            let size = call
                .params
                .get("size")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            if let Some(mgr) = app.plugin_manager.as_mut() {
                let (result, error) =
                    match mgr.create_shared_buffer_for(&call.plugin_id, call.call_id, size) {
                        Ok(r) => (serde_json::to_value(&r).ok(), None),
                        Err(e) => (None, Some(e)),
                    };
                mgr.send_ipc_result(&call.plugin_id, call.call_id, result, error, None);
            }
            continue;
        }
        let response =
            crate::ipc::handler::handle_checked_request(&mut app.core, state, engine, &checked);
        // plugin 호출도 같은 IPC 핸들러를 타므로(예: Claude 플러그인 훅의
        // `surface.completion`) 결과 회신 전에 Intent 큐를 적용한다 —
        // `docs/adr/0111-headless-drains-the-intent-queue.md`.
        crate::intent::headless::drain_pending_intents(&mut app.core, state, engine);
        crate::intent::headless::drain_pending_host_events(&app.core, state, engine);
        // gui 갈래(`src/app/dispatch/plugin_ipc.rs`)와 같은 계약 — 코드를 함께 넘긴다.
        let (result, error, code) = match response.error {
            Some(err) => (None, Some(err.message), Some(err.code)),
            None => (response.result, None, None),
        };
        if let Some(mgr) = app.plugin_manager.as_mut() {
            mgr.send_ipc_result(&call.plugin_id, call.call_id, result, error, code);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{OwnerPoll, owner_of_kind, run_if_due, wait_for_kind_registration};
    use std::time::{Duration, Instant};

    /// 지난 데드라인은 거둔다 — 안 거두면 headless 루프가 다음 `Tick::Busy` 까지 헛돈다.
    /// 데드라인과 같은 시각도 지난 것이다(앱 허브 `drain_due` 와 같은 규칙).
    #[test]
    fn a_passed_plugin_deadline_is_pumped() {
        let now = Instant::now();
        for deadline in [now - Duration::from_millis(50), now] {
            let mut pumped = 0;
            assert!(run_if_due(Some(deadline), now, || pumped += 1));
            assert_eq!(pumped, 1, "지난 데드라인을 거두지 않았다 — 루프가 헛돈다");
        }
    }

    /// 아직 안 온 데드라인과 데드라인 없음은 pump 하지 않는다 — 이 판정이 늘 참이면 매
    /// 바퀴(IPC 명령마다) plugin 허브를 돌리게 된다.
    #[test]
    fn a_future_or_absent_plugin_deadline_is_not_pumped() {
        let now = Instant::now();
        for deadline in [Some(now + Duration::from_millis(50)), None] {
            let mut pumped = 0;
            assert!(!run_if_due(deadline, now, || pumped += 1));
            assert_eq!(pumped, 0);
        }
    }

    /// plugin 허브를 한 번 pump 하면 지난 데드라인이 **미래로 간다** — 그래야 위 판정이
    /// 루프를 끝낸다. 매니저가 부팅 때 등록하는 주기 타이머(`PluginTick::Ping` 등)로 잰다.
    #[test]
    fn one_pump_moves_a_passed_plugin_deadline_into_the_future() {
        let factory: tasty_terminal::waker_factory::SharedWakerFactory =
            std::sync::Arc::new(tasty_terminal::waker_factory::NoopWakerFactory);
        // `PluginManager::new` 는 그 크레이트 안 전용(`#[cfg(test)]`)이라 공개 생성자를 쓴다.
        let mut mgr = crate::plugin::PluginManager::with_registries(
            factory,
            std::sync::Arc::new(crate::file::format::FileFormatRegistry::new()),
            std::sync::Arc::new(crate::file::handler::FileHandlerRegistry::new()),
        );
        let first = mgr
            .next_deadline()
            .expect("매니저가 부팅 때 주기 타이머를 하나도 안 걸었다 — 대조군이 죽었다");
        let late = first + Duration::from_millis(1);
        assert!(run_if_due(mgr.next_deadline(), late, || {
            mgr.pump(late);
        }));
        let next = mgr.next_deadline().expect("pump 뒤 주기 타이머가 사라졌다");
        assert!(
            next > late,
            "pump 뒤에도 데드라인이 과거({next:?} <= {late:?})다 — 루프가 계속 헛돈다"
        );
    }

    fn decl(kind: &str) -> crate::plugin::manifest::SurfaceKindDecl {
        serde_json::from_value(serde_json::json!({
            "kind": kind,
            "display_name_i18n_key": format!("surface.kind.{kind}"),
        }))
        .expect("decl 을 만들지 못했다")
    }

    /// **비활성 plugin 은 자기 kind 의 소유자가 아니다.**
    ///
    /// 이 한 줄이 없으면 `ensure_plugin_for_surface_kind` 가 영영 안 뜰 plugin 을
    /// 기다린다 — 실측으로 그 대기가 데몬 IPC 전체를 5 초 세웠다(그 함수 doc).
    #[test]
    fn a_disabled_plugin_does_not_own_its_kind() {
        let md = [decl("markdown")];
        let pkgs = [("com.tasty.markdown", md.as_slice())];
        assert_eq!(
            owner_of_kind(pkgs, |_| false, "markdown").as_deref(),
            Some("com.tasty.markdown"),
            "대조군: 켜져 있으면 소유자다"
        );
        assert_eq!(
            owner_of_kind(pkgs, |id| id == "com.tasty.markdown", "markdown"),
            None,
            "비활성인데 소유자로 답했다 — 그 뒤 대기가 데몬을 세운다"
        );
    }

    /// 켜져 있어도 **선언 안 한 kind** 는 남의 것이다. 위 판정이 `is_disabled` 만
    /// 보게 뒤집히면 이쪽이 운다.
    #[test]
    fn an_enabled_plugin_does_not_own_a_kind_it_never_declared() {
        let md = [decl("markdown")];
        let pkgs = [("com.tasty.markdown", md.as_slice())];
        assert_eq!(owner_of_kind(pkgs, |_| false, "image"), None);
    }

    /// 첫 소유자만 답한다 — 뒤에 있는 비활성 동명 선언이 답을 가리지 않는다.
    #[test]
    fn the_first_enabled_declarer_answers() {
        let a = [decl("markdown")];
        let b = [decl("markdown")];
        let pkgs = [("com.first", a.as_slice()), ("com.second", b.as_slice())];
        assert_eq!(
            owner_of_kind(pkgs, |id| id == "com.first", "markdown").as_deref(),
            Some("com.second"),
            "앞엣것이 비활성이면 다음 선언자로 넘어가야 한다"
        );
    }

    /// 연결이 등록 시한보다 오래 걸린 owner 의 kind 도 등록된다 — 등록 시한은 연결이 성사된
    /// 뒤부터 센다(ADR-0505). 연결 대기를 빼고 시한을 기동부터 세면 연결하는 동안 시한이 다
    /// 지나 `Connected` 로 끝난다(= `unknown surface kind`).
    #[test]
    fn a_kind_is_registered_when_its_owner_connects_after_the_registration_wait() {
        let started = Instant::now();
        let connects_at = started + Duration::from_millis(1000);
        let registers_at = connects_at + Duration::from_millis(100);
        let outcome =
            wait_for_kind_registration(Duration::from_secs(10), Duration::from_millis(500), || {
                let now = Instant::now();
                if now < connects_at {
                    OwnerPoll::Connecting
                } else if now < registers_at {
                    OwnerPoll::Connected
                } else {
                    OwnerPoll::Registered
                }
            });
        assert_eq!(
            outcome,
            OwnerPoll::Registered,
            "연결 시간이 등록 시한을 갉아먹었다"
        );
    }

    /// 끝내 연결 안 하는 owner 는 연결 상한에서 끝나고 등록 시한을 더 기다리지 않는다.
    #[test]
    fn an_owner_that_never_connects_ends_at_the_connect_limit() {
        let started = Instant::now();
        let outcome =
            wait_for_kind_registration(Duration::from_millis(100), Duration::from_secs(30), || {
                OwnerPoll::Connecting
            });
        assert_eq!(outcome, OwnerPoll::Connecting);
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "연결 상한이 지난 뒤 등록 시한까지 기다렸다"
        );
    }
}
