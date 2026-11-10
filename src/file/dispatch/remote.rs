//! mirror(원격 attach) origin의 파일 열기. 경로는 원격 파일이라 client 로컬에서 식별·실행하지 않는다.
//! 식별은 파일 이름만 보는 DetectDepth::Name으로 끝나며, 원격 NewTab으로 보낼 수 있는 핸들러만 실행한다.
//! 규칙은 [원격 attach (Remote attach)](../../../docs/features/remote-attach/index.md)를 따른다.

use crate::app::services::AppServices;
use crate::file::dispatch::{DispatchTarget, FileDispatchOrigin};
use crate::file::format::{DetectorId, FileTarget};
use crate::file::handler::{FileHandler, HandlerAction};
use crate::runtime::engine_access::{EngineMut, EngineRef};
use crate::runtime::engine_read::EngineRead;
use crate::state::RequestContext;

/// 원격에 열 수 있는 핸들러인지 확인한다.
/// OpenSurface이면서 client가 그 kind의 콘텐츠를 mirror하는 경우만 대상이다.
/// System·Ipc는 원격 경로를 client의 OS나 plugin이 로컬 파일로 열게 되어 제외한다.
pub(crate) fn is_remote_openable(
    engine: &EngineRead<'_>,
    handler: &FileHandler,
    origin_surface_id: Option<u32>,
) -> bool {
    match &handler.action {
        HandlerAction::OpenSurface { surface_kind, .. } => {
            mirrors_content(engine, surface_kind, origin_surface_id)
        }
        HandlerAction::Ipc { .. } | HandlerAction::System => false,
    }
}

/// mirror origin의 경로는 원격 파일이다. picker 선택 등 어느 경로로 와도 원격에 열 수 없는 핸들러는
/// 실행하지 않는다. 막았으면 true다.
pub(crate) fn rejects_handler_for_origin(
    engine: &EngineRef<'_>,
    handler: &FileHandler,
    origin_surface_id: Option<u32>,
) -> bool {
    let rejected = origin_surface_id.is_some_and(|sid| engine.is_mirror_surface(sid))
        && !is_remote_openable(&engine.read(), handler, origin_surface_id);
    if rejected {
        tracing::warn!(
            handler_id = %handler.id,
            "file handler cannot open a remote file; not executed",
        );
    }
    rejected
}

/// client에 등록한 kind가 원문 전달 허용 목록에 있거나 egui-mesh인지 확인한다.
/// egui-mesh는 서버가 plugin ID와 무관하게 mesh mirror로 보낸다(docs/dev-guide/attach-behavior.md).
/// HTML은 원래 workspace의 viewer 협상과 소유 plugin을 추가로 확인한다.
fn mirrors_content(engine: &EngineRead<'_>, kind: &str, origin: Option<u32>) -> bool {
    let Some(def) = engine.surface_registry.get_live(kind) else {
        return false;
    };
    let Some(plugin_id) = def.source.plugin_id() else {
        return false;
    };
    if kind == "html" {
        return plugin_id == "com.tasty.html"
            && origin
                .and_then(|id| engine.find_workspace_index_for_surface(id))
                .and_then(|(index, _)| engine.workspace_at(index))
                .is_some_and(|workspace| engine.html_entry_available(workspace.id));
    }
    crate::remote::server::is_attach_content_allowed(kind, plugin_id)
        || def.rendering == crate::runtime::surface_registry::RegisteredRendering::EguiMesh
}

/// mirror origin의 식별 결과를 적용한다. 로컬 핸들러 전체 picker는 띄우지 않는다.
/// 1순위가 원격에 열 수 있으면 바로 실행한다. 아니면 사용자 요청과 사용자 입력을 증명하지 못한 plugin 중계 요청에는
/// 원격에 열 수 있는 핸들러만 담은 picker를 띄우고, 외부 IPC 요청은 사용자 화면에 팝업을 만들지 않도록
/// 그중 첫 핸들러를 실행한다.
pub(crate) fn apply_remote_identify_result(
    core: &mut AppServices,
    state: &mut RequestContext,
    engine: &mut EngineMut<'_>,
    target: FileTarget,
    detector: Option<DetectorId>,
    origin_surface_id: u32,
    policy: crate::file::dispatch::FileDispatchPolicy,
) {
    let dispatch_origin = policy.dispatch_origin;
    let handlers = detector
        .as_ref()
        .map(|d| engine.runtime.file_handler.handlers_for(d))
        .unwrap_or_default();
    let first_is_openable = handlers
        .first()
        .is_some_and(|h| is_remote_openable(&engine.read(), h, Some(origin_surface_id)));
    let openable: Vec<FileHandler> = handlers
        .into_iter()
        .filter(|h| is_remote_openable(&engine.read(), h, Some(origin_surface_id)))
        .collect();
    let Some(first) = openable.first() else {
        let engine = engine.read();
        let workspace = mirror_workspace(&engine, origin_surface_id)
            .map(|id| (id, engine.mirror_host_text(origin_surface_id)));
        report_unsupported(state, &target, dispatch_origin, workspace);
        crate::file::dispatch::fail_run(
            policy.run,
            format!(
                "no file handler can open the remote file {}",
                target.display()
            ),
        );
        return;
    };
    if first_is_openable || dispatch_origin == FileDispatchOrigin::Agent {
        let first = first.clone();
        crate::file::dispatch::execute_handler_action(
            core,
            state,
            engine,
            &first,
            &DispatchTarget::File(target),
            Some(origin_surface_id),
            crate::file::dispatch::FileDispatchPolicy {
                ignore_size_limit: false,
                ..policy
            },
        );
        return;
    }
    open_remote_picker(
        state,
        &engine.read(),
        target,
        detector,
        openable,
        origin_surface_id,
        policy,
    );
}

/// 기존 핸들러 picker를 원격에 열 수 있는 후보만으로 연다. 최근 목록도 같은 조건으로 거른다.
/// 1순위는 원격에 열 수 없어 목록에 없으므로 기본 핸들러 표시는 두지 않는다.
/// 식별 결과(detector)가 없으면 식별을 건너뛴 전체 핸들러 목록이라 fallback 표시를 쓴다.
fn open_remote_picker(
    state: &mut RequestContext,
    engine: &EngineRead<'_>,
    target: FileTarget,
    detector: Option<DetectorId>,
    openable: Vec<FileHandler>,
    origin_surface_id: u32,
    policy: crate::file::dispatch::FileDispatchPolicy,
) {
    let candidates_are_fallback = detector.is_none();
    crate::file::dispatch::open_picker_from_read(
        state,
        engine,
        DispatchTarget::File(target),
        detector,
        openable,
        candidates_are_fallback,
        crate::file::dispatch::FileDispatchPolicy {
            ignore_size_limit: false,
            ..policy
        },
    );
    let Some(picker) = state.dialogs.file_handler_picker.as_mut() else {
        return;
    };
    crate::file::dispatch::set_picker_origin(picker, engine, Some(origin_surface_id));
    picker.default_handler = None;
    picker.remote_host = Some(engine.mirror_host_text(origin_surface_id));
    picker.recent.retain(|summary| {
        engine
            .file_handler
            .handler(&summary.id)
            .is_some_and(|h| is_remote_openable(engine, &h, Some(origin_surface_id)))
    });
}

/// 터미널 링크 Ctrl+클릭이 만드는 파일 열기 요청. mirror surface의 경로는 원격 경로라 그 surface를 origin으로 실어
/// 원격 판정([`apply_remote_identify_result`])으로 보낸다. 로컬 surface는 origin 없이 기존 로컬 열기를 따른다.
pub(crate) fn terminal_link_file_intent(
    target: FileTarget,
    surface_id: u32,
    is_mirror: bool,
) -> crate::intent::DispatchedIntent {
    crate::app::command::DomainIntent::DispatchFile {
        target,
        depth: crate::file::format::DetectDepth::Deep,
        origin_surface_id: is_mirror.then_some(surface_id),
        dispatch_origin: FileDispatchOrigin::User,
        ignore_size_limit: false,
        run: None,
    }
    .from_user_menu("terminal_link_click")
}

/// 터미널 링크 우클릭 "연결 동작"의 원격 경로. 식별·자동 실행 없이 원격에 열 수 있는 핸들러 전체를 picker로 연다.
/// 로컬 링크의 "연결 동작"이 전체 핸들러를 보여 주는 것과 같은 계약이다. 열 수 있는 핸들러가 없으면
/// 빈 picker 대신 Ctrl+클릭과 같은 "다운로드 후 실행" 대화상자로 묻는다.
pub(crate) fn open_remote_link_picker(
    state: &mut RequestContext,
    engine: &EngineRead<'_>,
    target: FileTarget,
    origin_surface_id: u32,
) {
    let openable: Vec<FileHandler> = engine
        .file_handler
        .all_handlers()
        .into_iter()
        .filter(|h| is_remote_openable(engine, h, Some(origin_surface_id)))
        .collect();
    if openable.is_empty() {
        let workspace = mirror_workspace(engine, origin_surface_id)
            .map(|id| (id, engine.mirror_host_text(origin_surface_id)));
        report_unsupported(state, &target, FileDispatchOrigin::User, workspace);
        return;
    }
    open_remote_picker(
        state,
        engine,
        target,
        None,
        openable,
        origin_surface_id,
        crate::file::dispatch::FileDispatchPolicy {
            dispatch_origin: FileDispatchOrigin::User,
            ignore_size_limit: false,
            run: None,
        },
    );
}

/// origin surface 가 속한 mirror workspace 의 로컬 ID.
fn mirror_workspace(engine: &EngineRead<'_>, origin_surface_id: u32) -> Option<u32> {
    engine
        .find_workspace_index_for_surface(origin_surface_id)
        .and_then(|(index, _)| engine.workspace_at(index))
        .map(|workspace| workspace.id)
}

/// 원격에 열 핸들러가 없다. 사용자 요청과 사용자 입력을 증명하지 못한 plugin 중계 요청은 진입 경로와
/// 관계없이 "다운로드 후 실행" 대화상자로 묻는다(핸들러 picker를 띄우는 범위와 같다). 외부 IPC 요청은
/// 사용자 화면에 팝업을 만들지 않으므로 로그로 끝낸다.
fn report_unsupported(
    state: &mut RequestContext,
    target: &FileTarget,
    origin: FileDispatchOrigin,
    workspace: Option<(u32, String)>,
) {
    match (origin, workspace) {
        (FileDispatchOrigin::Agent, _) => tracing::warn!(
            target = %target.display(),
            "no file handler can open this remote file; not executed",
        ),
        (_, Some((workspace, host))) => crate::adapters::ui::popup::download_run::ask(
            state,
            workspace,
            target.display(),
            host,
            origin,
        ),
        (_, None) => tracing::warn!(
            target = %target.display(),
            "the origin surface left its mirror workspace; the remote file was not offered for download",
        ),
    }
}

#[cfg(test)]
#[path = "remote_tests.rs"]
mod tests;
