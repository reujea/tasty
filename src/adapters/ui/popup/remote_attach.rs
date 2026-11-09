//! 원격 워크스페이스 조회·연결의 요청 ID와 표시 값. 워커와 조회 터널은 Remote가 소유한다.
//! Connect는 BrowserRequest로 App에 전달하고 원 engine/View에 사용자 활성화 의도를 고정한다.
//! App의 연결 입구는 조회 터널을 재사용하며 자기 인스턴스로의 연결을 거절한다.

mod nav;

use tasty_remote::browser::BROWSE_DEADLINE;
use tasty_type_geometry::length::LogicalPx;
use tasty_ui_widgets::{RowParent, RowState};

use crate::app::remote_browser::BrowserRequest;
use tasty_remote::browse::RemoteWorkspace;
use tasty_remote_profiles::{AttachView, RemoteProfiles};
use tasty_ui_widgets::{
    Button, ButtonVariant, CenterState, StatusKind, TagVariant, status_dot, tag, tag_with_icon,
};

use crate::adapters::ui::icons;
use crate::adapters::ui::popup::PopupAction;

use crate::i18n::t;
use crate::state::MainViewState;
use crate::theme;
use crate::theme::Theme;

pub const REMOTE_ATTACH_POPUP_ID: &str = "remote_attach";

const UI_MEMORY_ID: &str = "remote_attach.ui";

/// attach kind(같은 레지스트리의 예약 kind, docs/features/remote-profiles/index.md). remote_tool Attach 탭이 편집하고
/// 이 팝업은 **소비만** 한다.
const ATTACH_KIND: &str = "tasty-attach";

const LEFT_W: LogicalPx = LogicalPx(240.0);
const HEADER_H: LogicalPx = LogicalPx(47.0);
const FOOTER_H: LogicalPx = LogicalPx(49.0);

// 중앙 블록 글리프 크기는 `tasty-ui-widgets::tokens` 가 단일 출처다.
use tasty_ui_widgets::tokens::STRUCT_GAP_2;
const CAPS_H: LogicalPx = LogicalPx(30.0);
const PROFILE_ROW_H: LogicalPx = LogicalPx(50.0);
const WS_ROW_H: LogicalPx = LogicalPx(34.0);
const HEADER_PAD_L: LogicalPx = LogicalPx(14.0);

/// 조회 화면 상태. 목록이 비어도 새 워크스페이스를 만드는 행은 표시한다.
#[derive(Clone, Default)]
enum Conn {
    #[default]
    Initial,
    Connecting,
    Error(String),
    Loaded(Vec<RemoteWorkspace>),
}

/// 기존 워크스페이스와 새로 만들기 행을 하나의 선택값으로 다룬다.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum WsSel {
    New,
    Existing(u32),
}

/// 생성 진행·실패는 행 안에서 표시해 기존 목록을 계속 볼 수 있게 한다.
#[derive(Clone, Default, PartialEq, Eq, Debug)]
enum NewWsPhase {
    #[default]
    Rest,
    Creating,
    Failed(String),
}

#[derive(Clone, Default)]
struct UiState {
    /// 선택된 attach 프로필명.
    attach_sel: Option<String>,
    conn: Conn,
    /// 선택된 목록 행(기존 원격 ws 또는 "+ 새 워크스페이스").
    ws_sel: Option<WsSel>,
    /// "+ 새 워크스페이스" 행의 진행 상태.
    phase: NewWsPhase,
    request: Option<u64>,
    ready: bool,
    created: Option<u32>,
}

impl UiState {
    /// 생성 왕복 중인가 — Connect 재클릭 차단과 행 렌더가 같이 본다.
    fn creating(&self) -> bool {
        self.phase == NewWsPhase::Creating
    }

    /// 목록이 있고 행을 골랐으며 생성 중이 아닐 때 확정할 수 있다. 빈 목록의 새로 만들기도 포함한다.
    fn can_connect(&self) -> bool {
        matches!(&self.conn, Conn::Loaded(_)) && self.ws_sel.is_some() && !self.creating()
    }

    /// 새 행이 선택돼 있는가 — footer 라벨이 `Create & connect` 로 바뀌는 조건.
    fn create_mode(&self) -> bool {
        self.ws_sel == Some(WsSel::New)
    }
}

fn read_ui(ctx: &egui::Context) -> UiState {
    ctx.memory(|m| {
        m.data
            .get_temp::<UiState>(egui::Id::new(UI_MEMORY_ID))
            .unwrap_or_default()
    })
}
fn write_ui(ctx: &egui::Context, ui: UiState) {
    ctx.memory_mut(|m| m.data.insert_temp(egui::Id::new(UI_MEMORY_ID), ui));
}
fn clear_ui(ctx: &egui::Context) {
    ctx.memory_mut(|m| m.data.remove::<UiState>(egui::Id::new(UI_MEMORY_ID)));
}

/// attach 프로필의 표시 요약.
struct ProfileSummary {
    name: String,
    label: String,
    target: String,
    inactive: bool,
}

/// attach 프로필의 연결 상태. 참조한 ssh 프로필이 없으면 `missing`, 그 프로필이 꺼졌거나
/// 인라인 접속의 감지에 실패했으면 `inactive`다. `target`은 표시용 접속 대상이다.
pub(super) struct AttachStatus {
    pub(super) missing: bool,
    pub(super) inactive: bool,
    pub(super) target: String,
}

pub(super) fn attach_status(v: &AttachView<'_>, profiles: &RemoteProfiles) -> AttachStatus {
    let (missing, inactive) = match v.ssh_ref() {
        Some(r) => {
            let referenced = profiles.get(r).filter(|rp| rp.kind == "ssh");
            let disabled = referenced
                .and_then(|rp| rp.as_ssh())
                .map(|s| s.is_disabled())
                .unwrap_or(false);
            (referenced.is_none(), disabled)
        }
        None => (false, v.detect_failed()),
    };
    let target = match v.ssh_ref() {
        Some(r) => format!("→ {}", if r.is_empty() { "?" } else { r }),
        None => {
            let mut s = v.ssh_destination();
            if s.is_empty() {
                s = "?".into();
            }
            if let Some(port) = v.port()
                && port != 22
            {
                s = format!("{s}:{port}");
            }
            s
        }
    };
    AttachStatus {
        missing,
        inactive,
        target,
    }
}

fn attach_summaries(profiles: &RemoteProfiles) -> Vec<ProfileSummary> {
    profiles
        .profiles
        .iter()
        .filter(|p| p.kind == ATTACH_KIND)
        .filter_map(|p| {
            let st = attach_status(&p.as_attach()?, profiles);
            Some(ProfileSummary {
                name: p.name.clone(),
                label: p.label.clone().unwrap_or_default(),
                // dangling ref(missing) 도 연결하면 실패하므로 inactive 로 표시.
                inactive: st.inactive || st.missing,
                target: st.target,
            })
        })
        .collect()
}

fn enqueue(state: &mut MainViewState, request: BrowserRequest) {
    state.dispatch_intent(crate::intent::Intent::RemoteBrowser(request).from_user_context_menu());
}
fn connect(state: &mut MainViewState, st: &mut UiState, name: String) {
    if let Some(id) = st.request.take() {
        enqueue(state, BrowserRequest::Cancel { id });
    }
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    let id = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    st.request = Some(id);
    st.attach_sel = Some(name.clone());
    st.ws_sel = None;
    st.phase = NewWsPhase::Rest;
    st.ready = false;
    st.created = None;
    st.conn = Conn::Connecting;
    enqueue(
        state,
        BrowserRequest::Browse {
            id,
            profile: name,
            view: state.webview_identity.clone(),
        },
    );
}
fn start_create(state: &mut MainViewState, st: &mut UiState) {
    if st.creating() || !st.ready {
        return;
    }
    if let Some(id) = st.request {
        st.phase = NewWsPhase::Creating;
        enqueue(state, BrowserRequest::Create { id });
    }
}
fn push_attach(state: &mut MainViewState, st: &mut UiState, workspace: u32) {
    if let Some(id) = st.request.take() {
        st.ready = false;
        enqueue(state, BrowserRequest::Connect { id, workspace });
    }
}
fn cancel_browse(state: &mut MainViewState, st: &mut UiState) {
    if let Some(id) = st.request.take() {
        enqueue(state, BrowserRequest::Cancel { id });
    }
    *st = UiState::default();
}
fn cleanup(ctx: &egui::Context, state: &mut MainViewState) {
    cancel_browse(state, &mut read_ui(ctx));
    clear_ui(ctx);
}
pub fn on_close_remote_attach_popup(
    ctx: &egui::Context,
    state: &mut MainViewState,
    _engine: &crate::runtime::engine_read::EngineRead<'_>,
) {
    cleanup(ctx, state);
}
/// App applies values only to the still-open ticket. No worker handle or tunnel enters egui storage.
pub(crate) fn receive_update(
    ctx: &egui::Context,
    id: u64,
    update: tasty_remote::browser::BrowserUpdate,
) {
    let mut state = read_ui(ctx);
    if state.request != Some(id) {
        return;
    }
    match update {
        tasty_remote::browser::BrowserUpdate::Listed(rows) => {
            state.ws_sel = rows.is_empty().then_some(WsSel::New);
            state.conn = Conn::Loaded(rows);
            state.phase = NewWsPhase::Rest;
            state.ready = true;
        }
        tasty_remote::browser::BrowserUpdate::Created(id) => {
            state.created = Some(id);
            state.phase = NewWsPhase::Rest;
        }
        tasty_remote::browser::BrowserUpdate::Failed { creating, message } => {
            if creating {
                state.phase = NewWsPhase::Failed(message);
            } else {
                state.conn = Conn::Error(message);
                state.ready = false;
            }
        }
    }
    write_ui(ctx, state);
    ctx.request_repaint();
}

/// PopupDef.draw_fn 진입점.
pub fn draw_remote_attach_popup(
    ui: &mut egui::Ui,
    state: &mut MainViewState,
    _engine: &crate::runtime::engine_read::EngineRead<'_>,
) -> PopupAction {
    let th = theme::theme();
    let ctx = ui.ctx().clone();
    let mut st = read_ui(&ctx);

    // 목록 글자 선택이 행 조작을 가로채지 않도록 이 팝업에서 텍스트 선택을 끈다.
    ui.style_mut().interaction.selectable_labels = false;

    let mut close = false;
    if let Some(new_ws) = st.created.take() {
        push_attach(state, &mut st, new_ws);
        close = true;
    }

    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        cleanup(&ctx, state);
        return PopupAction::Close;
    }

    let profiles = RemoteProfiles::load();
    let summaries = attach_summaries(&profiles);

    let full = ui.max_rect();
    ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);

    let mut do_connect = false;

    let header_rect =
        egui::Rect::from_min_size(full.min, egui::vec2(full.width(), HEADER_H.value()));
    if draw_header(ui, &th, header_rect) {
        close = true;
    }

    let footer_rect = egui::Rect::from_min_size(
        egui::pos2(full.left(), full.bottom() - FOOTER_H.value()),
        egui::vec2(full.width(), FOOTER_H.value()),
    );

    let body_rect = egui::Rect::from_min_max(
        egui::pos2(full.left(), header_rect.bottom()),
        egui::pos2(full.right(), footer_rect.top()),
    );
    let left_rect = egui::Rect::from_min_size(
        body_rect.min,
        egui::vec2(LEFT_W.value(), body_rect.height()),
    );
    let right_rect = egui::Rect::from_min_max(
        egui::pos2(body_rect.left() + LEFT_W.value(), body_rect.top()),
        body_rect.max,
    );
    ui.painter().rect_filled(left_rect, 0.0, th.bg_sidebar());
    ui.painter().vline(
        left_rect.right(),
        left_rect.y_range(),
        egui::Stroke::new(
            th.border_width.value(),
            th.separator.to_egui_premultiplied(),
        ),
    );

    if let Some(name) = draw_left_pane(ui, &th, left_rect, &summaries, st.attach_sel.as_deref()) {
        connect(state, &mut st, name);
    }
    let (scroll_to_sel, enter) = nav::handle_keys(&ctx, &mut st);
    do_connect |= enter;
    match draw_right_pane(ui, &th, right_rect, &mut st, scroll_to_sel) {
        RightAction::RetryBrowse => {
            if let Some(name) = st.attach_sel.clone() {
                connect(state, &mut st, name);
            }
        }
        RightAction::RetryCreate => start_create(state, &mut st),
        RightAction::None => {}
    }

    let can_connect = st.can_connect();
    let connecting = matches!(&st.conn, Conn::Connecting);
    match draw_footer(
        ui,
        &th,
        footer_rect,
        can_connect,
        connecting,
        st.create_mode(),
    ) {
        FooterAction::Cancel if connecting => cancel_browse(state, &mut st),
        FooterAction::Cancel => close = true,
        FooterAction::Connect => do_connect = true,
        FooterAction::None => {}
    }

    // 새로 만들기는 생성 완료 뒤 연결하므로 지금 닫지 않는다.
    if do_connect && can_connect {
        match st.ws_sel {
            Some(WsSel::New) => start_create(state, &mut st),
            Some(WsSel::Existing(ws)) => {
                push_attach(state, &mut st, ws);
                close = true;
            }
            None => {}
        }
    }

    if close {
        cancel_browse(state, &mut st);
        clear_ui(&ctx);
        PopupAction::Close
    } else {
        write_ui(&ctx, st);
        PopupAction::None
    }
}

fn draw_header(ui: &mut egui::Ui, th: &Theme, rect: egui::Rect) -> bool {
    ui.painter().hline(
        rect.x_range(),
        rect.bottom(),
        egui::Stroke::new(
            th.border_width.value(),
            th.separator.to_egui_premultiplied(),
        ),
    );
    let inner = egui::Rect::from_min_max(
        egui::pos2(rect.left() + HEADER_PAD_L.value(), rect.top()),
        egui::pos2(rect.right() - th.spacing_sm.value(), rect.bottom()),
    );
    let mut close = false;
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(inner)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    child.spacing_mut().item_spacing.x = th.spacing_sm.value();
    child.add(icons::REMOTE.image(th.icon_glyph_size_md.value(), th.text_muted().into()));
    child.label(
        egui::RichText::new(t("remote_attach.heading"))
            .color(th.text_primary())
            .size(th.font_size_heading.value())
            .strong(),
    );
    child.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        if ui
            .add(
                egui::ImageButton::new(
                    icons::CLOSE.image(th.icon_glyph_size_md.value(), th.text_muted().into()),
                )
                .frame(false),
            )
            .on_hover_text(t("remote_attach.close"))
            .clicked()
        {
            close = true;
        }
    });
    close
}

/// 제목 줄 아래에 목록 영역을 둔 열. 목록 영역의 Ui를 돌려준다.
fn titled_column(
    ui: &mut egui::Ui,
    th: &Theme,
    rect: egui::Rect,
    title: &str,
    suffix: Option<&str>,
) -> egui::Ui {
    let mut col = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    col.set_clip_rect(rect);
    col.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);
    caps_header(&mut col, th, title, suffix);
    let list_rect = egui::Rect::from_min_max(
        egui::pos2(rect.left(), rect.top() + CAPS_H.value()),
        rect.max,
    );
    let mut list = col.new_child(
        egui::UiBuilder::new()
            .max_rect(list_rect)
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    list.set_clip_rect(list_rect);
    list
}

fn draw_left_pane(
    ui: &mut egui::Ui,
    th: &Theme,
    rect: egui::Rect,
    profiles: &[ProfileSummary],
    selected: Option<&str>,
) -> Option<String> {
    let mut clicked: Option<String> = None;
    let mut list = titled_column(ui, th, rect, t("remote_attach.attach_profiles"), None);
    egui::ScrollArea::vertical()
        .id_salt("remote_attach.profiles")
        .drag_to_scroll(false)
        .show(&mut list, |ui| {
            ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);
            if profiles.is_empty() {
                ui.add_space(th.spacing_md.value());
                let mut inner = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(egui::Rect::from_min_size(
                            egui::pos2(
                                ui.max_rect().left() + th.spacing_md.value(),
                                ui.cursor().top(),
                            ),
                            egui::vec2((LEFT_W - th.spacing_md.scaled(2.0)).value(), 40.0),
                        ))
                        .layout(egui::Layout::top_down(egui::Align::Min)),
                );
                inner.label(
                    egui::RichText::new(t("remote_attach.no_profiles"))
                        .color(th.text_muted())
                        .italics()
                        .size(th.font_size_caption.value()),
                );
                return;
            }
            for p in profiles {
                if profile_row(ui, th, p, Some(p.name.as_str()) == selected) {
                    clicked = Some(p.name.clone());
                }
            }
        });
    clicked
}

fn profile_row(ui: &mut egui::Ui, th: &Theme, p: &ProfileSummary, selected: bool) -> bool {
    let w = ui.available_width();
    let (rect, resp) =
        ui.allocate_exact_size(egui::vec2(w, PROFILE_ROW_H.value()), egui::Sense::click());
    let state = RowState::from_response(selected, &resp);
    state.paint_fill(ui.painter(), rect, 0.0, th);
    if selected {
        let bar = egui::Rect::from_min_size(
            rect.min,
            egui::vec2(th.selection_edge_width.value(), rect.height()),
        );
        ui.painter()
            .rect_filled(bar, 0.0, th.accent_primary_on_active());
    }
    let inner = egui::Rect::from_min_max(
        egui::pos2(
            rect.left() + th.spacing_md.value(),
            rect.top() + th.spacing_sm.value(),
        ),
        egui::pos2(
            rect.right() - th.spacing_md.value(),
            rect.bottom() - th.spacing_sm.value(),
        ),
    );
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(inner)
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    // 두 번째 줄의 아래가 잘리지 않도록 세로는 행 전체로, 가로만 내부 영역으로 자른다.
    child.shrink_clip_rect(egui::Rect::from_min_max(
        egui::pos2(inner.left(), rect.top()),
        egui::pos2(inner.right(), rect.bottom()),
    ));
    child.spacing_mut().item_spacing.y = STRUCT_GAP_2.value();
    let meta_c = state.meta(th, RowParent::Base);
    child.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = th.spacing_sm.value();
        let name_c = state.label(th, RowParent::Base, th.text_secondary());
        ui.add(
            egui::Label::new(
                egui::RichText::new(&p.name)
                    .size(th.font_size_body.value())
                    .strong()
                    .color(name_c),
            )
            .truncate(),
        );
        if !p.label.is_empty() {
            ui.add(
                egui::Label::new(
                    egui::RichText::new(format!("({})", p.label))
                        .size(th.font_size_body.value())
                        .color(meta_c),
                )
                .truncate(),
            );
        }
        if p.inactive {
            tag_with_icon(
                ui,
                th,
                t("remote_attach.inactive"),
                TagVariant::Warning,
                icons::ALERT_TRIANGLE,
            );
        }
    });
    child.label(
        egui::RichText::new(&p.target)
            .monospace()
            .size(th.font_size_caption.value())
            .color(meta_c),
    );
    resp.clicked()
}

/// 워커 시작을 호출부에 요청하는 동작.
enum RightAction {
    None,
    /// error center-state 의 Retry — 선택 프로필로 **재조회**.
    RetryBrowse,
    /// 실패한 "+ 새 워크스페이스" 행의 "다시 시도" — **재생성**.
    RetryCreate,
}

/// 우측 pane 렌더.
fn draw_right_pane(
    ui: &mut egui::Ui,
    th: &Theme,
    rect: egui::Rect,
    st: &mut UiState,
    scroll_to_sel: bool,
) -> RightAction {
    let sel_name = st.attach_sel.clone().unwrap_or_default();
    match &st.conn {
        Conn::Initial => {
            CenterState::empty(icons::REMOTE, t("remote_attach.select_profile"))
                .sub_line(Some(t("remote_attach.select_profile_hint")))
                .show_in(ui, th, rect);
            RightAction::None
        }
        Conn::Connecting => {
            let hint = t("remote_attach.connecting_hint")
                .replace("{name}", &sel_name)
                .replace("{secs}", &BROWSE_DEADLINE.as_secs().to_string());
            CenterState::loading(t("remote_attach.connecting"))
                .sub_line(Some(&hint))
                .show_in(ui, th, rect);
            RightAction::None
        }
        Conn::Error(msg) => {
            let msg = msg.clone();
            let out = CenterState::error(t("remote_attach.cant_connect"))
                .sub_line(Some(&msg))
                .action(t("remote_attach.retry"), Some(icons::REFRESH))
                .show_in(ui, th, rect);
            if out.action_clicked {
                RightAction::RetryBrowse
            } else {
                RightAction::None
            }
        }
        Conn::Loaded(ws) => {
            let ws = ws.clone();
            let phase = st.phase.clone();
            match draw_ws_list(
                ui,
                th,
                rect,
                &ws,
                &sel_name,
                st.ws_sel,
                &phase,
                scroll_to_sel,
            ) {
                Some(ListAction::Select(sel)) => {
                    st.ws_sel = Some(sel);
                    if sel == WsSel::New && matches!(st.phase, NewWsPhase::Failed(_)) {
                        st.phase = NewWsPhase::Rest;
                    }
                }
                Some(ListAction::RetryCreate) => {
                    st.ws_sel = Some(WsSel::New);
                    return RightAction::RetryCreate;
                }
                None => {}
            }
            RightAction::None
        }
    }
}

/// 목록에서 나온 사용자 동작.
enum ListAction {
    /// 행 선택(기존 ws 또는 "+ 새 워크스페이스").
    Select(WsSel),
    /// 실패한 새 행의 "다시 시도".
    RetryCreate,
}

#[allow(clippy::too_many_arguments)] // reason: 목록 그리기 입력은 popup UiState 의 서로 다른 필드라 묶을 구조체가 없다
fn draw_ws_list(
    ui: &mut egui::Ui,
    th: &Theme,
    rect: egui::Rect,
    ws: &[RemoteWorkspace],
    profile_name: &str,
    ws_sel: Option<WsSel>,
    phase: &NewWsPhase,
    scroll_to_sel: bool,
) -> Option<ListAction> {
    let mut action = None;
    let mut list = titled_column(
        ui,
        th,
        rect,
        t("remote_attach.remote_workspaces"),
        Some(profile_name),
    );
    egui::ScrollArea::vertical()
        .id_salt("remote_attach.workspaces")
        .drag_to_scroll(false)
        .show(&mut list, |ui| {
            ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);
            let top = ui.cursor().top();
            if let Some(a) = new_ws_row(ui, th, phase, ws_sel == Some(WsSel::New)) {
                action = Some(a);
            }
            nav::scroll_to_row(ui, top, scroll_to_sel && ws_sel == Some(WsSel::New));
            if ws.is_empty() {
                empty_line(ui, th, profile_name);
                return;
            }
            // 생성 중에는 목록을 읽을 수 있게 두고(행 이름 text-muted) 다른 행 선택만 막는다.
            // egui disabled fade가 잉크에 겹치지 않도록 입력은 행의 Sense로만 막는다.
            let creating = *phase == NewWsPhase::Creating;
            for w in ws {
                let top = ui.cursor().top();
                let selected = ws_sel == Some(WsSel::Existing(w.id));
                if ws_row(ui, th, w, selected, !creating) {
                    action = Some(ListAction::Select(WsSel::Existing(w.id)));
                }
                nav::scroll_to_row(ui, top, scroll_to_sel && selected);
            }
        });
    action
}

/// 원격이 닿기는 하는데 ws 가 없을 때 새 행 아래 붙는 muted 한 줄. 이름 열은 위 행과
/// 같은 정렬선에서 시작한다(선행 dot 슬롯 폭 스페이서).
fn empty_line(ui: &mut egui::Ui, th: &Theme, profile_name: &str) {
    let width = ui.available_width();
    let h = th.spacing_xs.value() * 2.0 + th.font_size_caption.value() * th.line_height_ui;
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, h), egui::Sense::hover());
    let x =
        rect.left() + th.spacing_md.value() + th.status_dot_size().value() + th.spacing_sm.value();
    ui.painter().text(
        egui::pos2(x, rect.center().y),
        egui::Align2::LEFT_CENTER,
        t("remote_attach.no_workspaces_line").replace("{name}", profile_name),
        egui::FontId::proportional(th.font_size_caption.value()),
        th.text_muted().into(),
    );
}

/// 새로 만들기도 다른 행처럼 선택 후 Connect로 확정한다.
/// 기존 워크스페이스와는 아이콘·라벨 색·구분선으로 구별한다.
fn new_ws_row(
    ui: &mut egui::Ui,
    th: &Theme,
    phase: &NewWsPhase,
    selected: bool,
) -> Option<ListAction> {
    let creating = *phase == NewWsPhase::Creating;
    let failed = matches!(phase, NewWsPhase::Failed(_));
    let sense = if creating {
        egui::Sense::hover()
    } else {
        egui::Sense::click()
    };
    let (resp, mut child) = ws_row_frame(ui, th, sense, selected, !creating);
    let state = ws_row_state(selected, !creating, &resp);
    // 새 행의 글리프는 색이 뜻을 담는다. 실패와 생성 중은 뜻이 바뀌므로 hue를 따로 고른다.
    let glyph_c: egui::Color32 = if creating {
        state.glyph(th).into()
    } else if failed {
        state
            .hue(
                th,
                th.accent_danger(),
                th.accent_danger_raised(),
                th.accent_danger_on_active(),
            )
            .into()
    } else {
        state
            .hue(
                th,
                th.accent_primary(),
                th.accent_primary_raised(),
                th.accent_primary_on_active(),
            )
            .into()
    };
    dot_slot_glyph(&mut child, th, creating, failed, glyph_c);
    // 선택 배경에서 대비를 확보하도록 라벨 색을 바꾼다. 아이콘·구분선도 행 구분에 사용한다.
    let label_c = if state.is_selected() {
        state.label(th, RowParent::Base, th.text_on_active())
    } else if creating {
        state.meta(th, RowParent::Base)
    } else {
        th.accent_primary_raised()
    };
    child.add(
        egui::Label::new(
            egui::RichText::new(if creating {
                t("remote_attach.creating_workspace")
            } else {
                t("remote_attach.new_workspace")
            })
            .size(th.font_size_body.value())
            .strong()
            .color(label_c),
        )
        .truncate(),
    );
    child.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        if !creating && !failed {
            ui.label(
                egui::RichText::new(t("remote_attach.new_workspace_on_remote"))
                    .size(th.font_size_caption.value())
                    .color(state.meta(th, RowParent::Base)),
            );
        }
    });
    let resp = resp.on_hover_text(t("remote_attach.new_workspace_hint"));

    let mut action = resp.clicked().then_some(ListAction::Select(WsSel::New));
    if let NewWsPhase::Failed(msg) = phase
        && new_ws_error(ui, th, msg)
    {
        action = Some(ListAction::RetryCreate);
    }
    row_separator(ui, th);
    action
}

/// 기존·새 행의 아이콘 공간을 같게 잡아 이름 시작 위치를 맞춘다.
fn dot_slot(ui: &mut egui::Ui, th: &Theme) -> egui::Rect {
    let (slot, _) = ui.allocate_exact_size(
        egui::vec2(th.status_dot_size().value(), th.icon_glyph_size_sm.value()),
        egui::Sense::hover(),
    );
    slot
}

/// 새 행의 글리프 — 슬롯 중심의 `plus`(실패 시 `alertTriangle`, 생성 중이면 Spinner).
fn dot_slot_glyph(
    ui: &mut egui::Ui,
    th: &Theme,
    creating: bool,
    failed: bool,
    color: egui::Color32,
) {
    let size = th.icon_glyph_size_sm.value();
    let g = egui::Rect::from_center_size(dot_slot(ui, th).center(), egui::vec2(size, size));
    if creating {
        let mut c = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(g)
                .layout(egui::Layout::top_down(egui::Align::Center)),
        );
        tasty_ui_widgets::Spinner::new()
            .size(size)
            .color(color)
            .show(&mut c, th);
    } else {
        let glyph = if failed {
            icons::ALERT_TRIANGLE
        } else {
            icons::PLUS
        };
        glyph.image(size, color).paint_at(ui, g);
    }
}

/// 공용 아이콘 공간 안에 상태 점을 그린다. 빈 라벨의 status_dot은 점 폭만 사용한다.
fn dot_slot_status(ui: &mut egui::Ui, th: &Theme, kind: StatusKind, pulse: bool, selected: bool) {
    let slot = dot_slot(ui, th);
    let mut c = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(slot)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    let ground = if selected {
        tasty_ui_widgets::Ground::Active
    } else {
        tasty_ui_widgets::Ground::Panel
    };
    status_dot(&mut c, th, kind, "", pulse, false, ground);
}

/// 생성 실패는 목록을 가리지 않고 행 아래 표시한다. 긴 오류는 줄바꿈하며 툴팁으로 전문을 제공한다.
fn new_ws_error(ui: &mut egui::Ui, th: &Theme, msg: &str) -> bool {
    let lead = th.spacing_md.value() + th.status_dot_size().value() + th.spacing_sm.value();
    let width = ui.available_width();
    let inner_w = (width - lead - th.spacing_md.value()).max(1.0);
    let galley = ui.painter().layout(
        msg.to_owned(),
        egui::FontId::proportional(th.font_size_caption.value()),
        th.accent_danger_raised().into(),
        inner_w,
    );
    let btn_h = tasty_ui_widgets::ControlSize::Sm.height(th);
    let h = th.spacing_xs.value() * 2.0 + galley.rect.height() + btn_h + th.spacing_sm.value();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, h), egui::Sense::hover());
    let inner = egui::Rect::from_min_max(
        egui::pos2(rect.left() + lead, rect.top() + th.spacing_xs.value()),
        egui::pos2(
            rect.right() - th.spacing_md.value(),
            rect.bottom() - th.spacing_sm.value(),
        ),
    );
    let mut col = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(inner)
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    col.shrink_clip_rect(inner);
    col.spacing_mut().item_spacing.y = th.spacing_xs.value();
    col.add(
        egui::Label::new(
            egui::RichText::new(msg)
                .size(th.font_size_caption.value())
                .color(th.accent_danger_raised()),
        )
        .wrap(),
    )
    .on_hover_text(msg);
    Button::new(t("remote_attach.try_again"))
        .variant(ButtonVariant::Secondary)
        .size(tasty_ui_widgets::ControlSize::Sm)
        .leading_icon(&|ui, rect, c| icons::REFRESH.image(rect.height(), c).paint_at(ui, rect))
        .show(&mut col, th)
        .clicked()
}

/// 행 아래 1px 구분선 + 위/아래 xs 마진.
fn row_separator(ui: &mut egui::Ui, th: &Theme) {
    let width = ui.available_width();
    let m = th.spacing_xs.value();
    let w = th.border_width.value();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, m * 2.0 + w), egui::Sense::hover());
    ui.painter().hline(
        rect.x_range(),
        rect.top() + m + w * 0.5,
        egui::Stroke::new(w, th.separator.to_egui_premultiplied()),
    );
}

/// 워크스페이스 행의 바탕(선택 막대·호버)을 그리고 안쪽 가로 배치 영역을 돌려준다.
/// 입력을 받지 않는 행은 선택 채움만 유지하고 hover·press를 그리지 않는다.
fn ws_row_state(selected: bool, hoverable: bool, resp: &egui::Response) -> RowState {
    RowState::of(
        true,
        selected,
        hoverable && resp.hovered(),
        hoverable && resp.is_pointer_button_down_on(),
    )
}

fn ws_row_frame(
    ui: &mut egui::Ui,
    th: &Theme,
    sense: egui::Sense,
    selected: bool,
    hoverable: bool,
) -> (egui::Response, egui::Ui) {
    let width = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(width, WS_ROW_H.value()), sense);
    ws_row_state(selected, hoverable, &resp).paint_fill(ui.painter(), rect, 0.0, th);
    if selected {
        let bar = egui::Rect::from_min_size(
            rect.min,
            egui::vec2(th.selection_edge_width.value(), rect.height()),
        );
        ui.painter()
            .rect_filled(bar, 0.0, th.accent_primary_on_active());
    }
    let inner = egui::Rect::from_min_max(
        egui::pos2(rect.left() + th.spacing_md.value(), rect.top()),
        egui::pos2(rect.right() - th.spacing_md.value(), rect.bottom()),
    );
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(inner)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    child.shrink_clip_rect(inner);
    child.spacing_mut().item_spacing.x = th.spacing_sm.value();
    (resp, child)
}

fn ws_row(
    ui: &mut egui::Ui,
    th: &Theme,
    w: &RemoteWorkspace,
    selected: bool,
    interactive: bool,
) -> bool {
    let disabled = w.attached;
    let sense = if disabled || !interactive {
        egui::Sense::hover()
    } else {
        egui::Sense::click()
    };
    let (resp, mut child) = ws_row_frame(ui, th, sense, selected, !disabled && interactive);
    let state = ws_row_state(selected, !disabled && interactive, &resp);
    let kind = if w.busy_count > 0 {
        StatusKind::Running
    } else {
        StatusKind::Idle
    };
    dot_slot_status(&mut child, th, kind, w.busy_count > 0, selected);
    // 다른 클라이언트가 쓰는 워크스페이스도 이름은 읽으므로 text-muted다. 상태는 "in use" Tag가 알린다.
    let name_c = if disabled || !interactive {
        th.text_muted()
    } else {
        state.label(th, RowParent::Base, th.text_secondary())
    };
    let meta_c = state.meta(th, RowParent::Base);
    child.add(
        egui::Label::new(
            egui::RichText::new(&w.name)
                .size(th.font_size_body.value())
                .color(name_c),
        )
        .truncate(),
    );
    child.add(icons::SPLIT.image(th.font_size_caption.value(), meta_c.into()));
    child.label(
        egui::RichText::new(w.pane_count.to_string())
            .size(th.font_size_caption.value())
            .color(meta_c),
    );
    child.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        if disabled {
            tag(
                ui,
                th,
                t("remote_attach.in_use"),
                TagVariant::Attached,
                false,
            );
        } else if w.busy_count > 0 {
            ui.label(
                egui::RichText::new(t("remote_attach.busy"))
                    .size(th.font_size_caption.value())
                    .color(meta_c),
            );
        }
    });
    resp.clicked()
}

enum FooterAction {
    None,
    Cancel,
    Connect,
}

/// 조회 중에는 취소 버튼으로 창을 닫지 않고 조회만 중단한다.
fn draw_footer(
    ui: &mut egui::Ui,
    th: &Theme,
    rect: egui::Rect,
    can_connect: bool,
    connecting: bool,
    create_mode: bool,
) -> FooterAction {
    ui.painter().hline(
        rect.x_range(),
        rect.top(),
        egui::Stroke::new(
            th.border_width.value(),
            th.separator.to_egui_premultiplied(),
        ),
    );
    let inner = egui::Rect::from_min_max(
        egui::pos2(rect.left() + th.spacing_lg.value(), rect.top()),
        egui::pos2(rect.right() - th.spacing_lg.value(), rect.bottom()),
    );
    let mut action = FooterAction::None;
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(inner)
            .layout(egui::Layout::right_to_left(egui::Align::Center)),
    );
    child.spacing_mut().item_spacing.x = th.spacing_sm.value();
    let connect_label = if create_mode {
        t("remote_attach.connect_create")
    } else {
        t("remote_attach.connect")
    };
    if Button::new(connect_label)
        .variant(ButtonVariant::Primary)
        .enabled(can_connect)
        .show(&mut child, th)
        .clicked()
    {
        action = FooterAction::Connect;
    }
    let ghost_label = if connecting {
        t("remote_attach.stop")
    } else {
        t("remote_attach.cancel")
    };
    if Button::new(ghost_label)
        .variant(ButtonVariant::Ghost)
        .show(&mut child, th)
        .clicked()
    {
        action = FooterAction::Cancel;
    }
    action
}

/// caps 헤더 — font-label micro uppercase muted. `suffix` 있으면 "· {suffix}" 를 붙인다.
fn caps_header(ui: &mut egui::Ui, th: &Theme, label: &str, suffix: Option<&str>) {
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), CAPS_H.value()),
        egui::Sense::hover(),
    );
    let base_x = rect.left() + th.spacing_md.value();
    let y = rect.top() + th.spacing_md.value();
    let galley = ui.painter().layout_no_wrap(
        label.to_uppercase(),
        tasty_ui_widgets::label_font(th.font_size_micro.value()),
        th.text_muted().into(),
    );
    let w = galley.rect.width();
    ui.painter()
        .galley(egui::pos2(base_x, y), galley, th.text_muted().into());
    if let Some(s) = suffix {
        ui.painter().text(
            egui::pos2(base_x + w + th.spacing_sm.value(), y),
            egui::Align2::LEFT_TOP,
            format!("· {s}"),
            egui::FontId::proportional(th.font_size_caption.value()),
            th.text_muted().into(),
        );
    }
}
