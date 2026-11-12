//! 원격 프로필·Attach·Passkey를 편집한다. CLI·IPC와 같은 저장 로직을 사용한다.
//! tasty-attach 프로필은 Attach 탭에서만 다루며 SSH 프로필 참조 또는 직접 입력을 지원한다.

mod forms;
#[cfg(test)]
mod hint_color_tests;
mod receipts;
mod reveal;
#[cfg(test)]
mod reveal_tests;
pub(crate) use forms::{AttachForm, PasskeyForm, ProfileForm};
use receipts::enqueue;
pub(crate) use receipts::{accept_file_result, take_file_requests};

use crate::app::remote_tool_files::{FileAction, FileRequest, FileValue, PendingToken};
use std::collections::HashSet;
use std::sync::{
    Arc, Weak,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};

use tasty_remote_profiles::{
    KNOWN_PASSKEY_KINDS, PORT_MODES, Passkey, Passkeys, RemoteProfile, RemoteProfiles, SHELLS,
    SshConfigHost, config_availability, enumerate_hosts_at, imported_as, is_builtin_kind,
    user_config_path,
};

use crate::adapters::ui::icons;
use crate::adapters::ui::popup::PopupAction;

use crate::i18n::t;
use crate::state::MainViewState;
use crate::theme;
use crate::theme::Theme;
use tasty_type_geometry::length::LogicalPx;
use tasty_ui_widgets::{
    Button, ButtonVariant, ControlSize, IconButton, IconButtonVariant, LocalSshHost,
    LocalSshSectionData, ProtocolFilterItem, ProtocolFilterLabels, RemoteRowChip, TabStripData,
    TextWrap, draw_local_ssh_section as ssh_section_view, draw_protocol_filter_body,
    draw_protocol_filter_button, draw_tab_strip, filter_dropdown_content_width, ghost_button,
    primary_button, remote_list_row, remote_row_title, selectable_label, selectable_text,
    warn_badge, warn_badge_width,
};

pub const REMOTE_TOOL_POPUP_ID: &str = "remote_tool";

/// 헤더·본문의 좌측 안쪽 여백. 디자인 전사값 14 로 4px 그리드 밖이다
/// (`spacing_md`=12 와 2px 차). `egui::Margin` 필드가 `i8` 이라 타입을 맞춰 둔다.
const PANEL_PAD_L: i8 = 14;
/// 헤더 상하 여백. 디자인 전사값 11 로 그리드 밖이다(`spacing_md`=12 와 1px 차).
const HEADER_PAD_Y: i8 = 11;
/// 본문 상단 여백. 디자인 전사값 10 으로 그리드 밖이다(하단은 `spacing_sm`=8 로
/// 비대칭 — 디자인 전사 그대로다).
const CONTENT_PAD_TOP: i8 = 10;
/// 헤더 아이템 가로 간격. 디자인 전사값 9 로 그리드 밖이다(`spacing_sm`=8 과 1px 차).
const HEADER_GAP_X: LogicalPx = LogicalPx(9.0);

const KNOWN_TYPES: &[&str] = &["ssh", "smb", "http"];

const UI_MEMORY_ID: &str = "remote_tool.ui";

/// 적용된 필터는 팝업 상태와 별도 키로 보관해 다시 열어도 유지한다. 앱을 종료하면 사라진다.
const FILTER_MEMORY_ID: &str = "remote_tool.filter";

/// 프로토콜 필터 드롭다운 egui popup id. Escape/바깥클릭 닫힘 판정에 사용.
const FILTER_POPUP_ID: &str = "remote_tool.filter_popup";

/// attach 레코드의 kind (같은 레지스트리 안의 예약 kind, docs/features/remote-profiles/index.md).
const ATTACH_KIND: &str = "tasty-attach";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
enum Tab {
    #[default]
    Profiles,
    Attach,
    Passkeys,
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
enum Sub {
    #[default]
    List,
    Form,
    ConfirmDelete(String),
}

#[derive(Clone, Debug)]
struct DetectJob {
    name: String,
    id: tasty_remote::profile_detection::DetectionId,
}

#[derive(Clone, Default)]
struct UiState {
    pending: Arc<AtomicUsize>,
    identity: Arc<()>,
    view: Weak<()>,
    revealed_values: std::collections::HashMap<String, (Passkey, String)>,
    key_edit: Arc<AtomicBool>,
    reveal_owners: std::collections::HashMap<String, Arc<AtomicBool>>,
    tab: Tab,
    profile_view: Sub,
    attach_view: Sub,
    passkey_view: Sub,
    pform: ProfileForm,
    aform: AttachForm,
    kform: PasskeyForm,
    perr: Option<String>,
    aerr: Option<String>,
    kerr: Option<String>,
    revealed: HashSet<String>,
    detecting: Option<DetectJob>,
    /// 적용 버튼을 누르기 전의 필터. 팝업이 닫히면 버린다.
    filter_draft: HashSet<String>,
    /// 팝업을 여는 동안 한 번 읽는 SSH config 캐시. 닫으면 지우며 파일 변경 감시는 하지 않는다.
    local: Option<LocalSshCache>,
}

/// 로컬 ssh config 스냅샷. 목록 렌더가 파일 I/O 없이 돌 수 있는 최소 정보다.
#[derive(Clone, Debug, Default)]
struct LocalSshCache {
    hosts: Vec<SshConfigHost>,
    /// 표시용 config 경로(`~/.ssh/config`).
    path: String,
    /// 파일 자체가 있는지. "설정이 없다" 와 "있는데 alias 가 0 건" 은 사용자가 할 일이
    /// 다르다.
    exists: bool,
    /// 경로는 있지만 읽을 수 없는 상태. 권한 거부뿐 아니라 디렉터리이거나 UTF-8이 아닌
    /// 경우도 포함하므로 오류 원인을 권한으로 단정하지 않는다.
    unreadable: bool,
}

/// 로컬 섹션의 빈 상태 문구 키 — 원인 3 갈래를 가른다(없음 / 못 읽음 / 정말 0 건).
fn local_ssh_empty_key(local: &LocalSshCache) -> &'static str {
    if !local.exists {
        "remote_tool.local_ssh_missing"
    } else if local.unreadable {
        "remote_tool.local_ssh_unreadable"
    } else {
        "remote_tool.local_ssh_empty"
    }
}

/// 캐시가 없을 때 SSH config를 읽는다. 다시 읽으려면 팝업을 닫고 열어야 한다.
fn load_local_ssh() -> LocalSshCache {
    local_ssh_cache_at(user_config_path())
}

/// 테스트용 경로를 받을 수 있는 캐시 로더. 존재·읽기 가능 여부는 CLI·IPC와 같은 config_availability로 판별한다.
fn local_ssh_cache_at(path: Option<std::path::PathBuf>) -> LocalSshCache {
    if path.is_none() {
        tracing::warn!("ssh_config: cannot resolve home directory — skipping enumeration");
    }
    let avail = config_availability(path.as_deref());
    LocalSshCache {
        hosts: path.as_deref().map(enumerate_hosts_at).unwrap_or_default(),
        path: path
            .as_deref()
            .map(tasty_utils::path::tilde_abbreviate)
            .unwrap_or_else(|| "~/.ssh/config".into()),
        exists: avail.exists,
        unreadable: avail.exists && !avail.readable,
    }
}

/// Host 블록에 직접 적힌 값만으로 접속 요약을 만든다. 없는 user/port를 기본값으로 채우지 않는다.
/// Host *·Match 등의 실제 접속 설정을 모두 해석한 결과는 아니며 가져오기에는 alias만 사용한다.
fn local_target_hint(h: &SshConfigHost) -> String {
    // HostName 이 없으면 ssh 가 alias 를 호스트 이름으로 쓴다.
    let host = match (&h.hostname, &h.user, h.port) {
        (None, None, None) => return "—".into(),
        (Some(name), ..) => name.as_str(),
        (None, ..) => h.alias.as_str(),
    };
    let mut out = String::new();
    if let Some(u) = &h.user {
        out.push_str(u);
        out.push('@');
    }
    out.push_str(host);
    if let Some(port) = h.port {
        out.push(':');
        out.push_str(&port.to_string());
    }
    out
}

/// 가져오기 클릭 시 여는 폼의 프리필. **alias 만 `host` 에 넣고 user/port 는 비운다** —
/// 채우면 ssh config 위임이 깨지고(그 값이 config 를 덮어쓴다) config 수정 시 어긋난다.
/// 이름은 alias 를 기본값으로 주되 폼에서 바꿀 수 있다.
fn import_prefill(alias: &str) -> ProfileForm {
    ProfileForm {
        kind: "ssh".into(),
        name: alias.to_string(),
        host: alias.to_string(),
        shell: "auto".into(),
        ..Default::default()
    }
}

/// 프로필 목록이 비었을 때의 문구. 로컬 SSH config 목록은 이와 별개로 계속 표시한다.
fn profile_empty_key(has_non_attach: bool, any_visible: bool) -> Option<&'static str> {
    match (has_non_attach, any_visible) {
        (false, _) => Some("remote_tool.profile_empty"),
        (true, false) => Some("remote_tool.profile_filter_empty"),
        (true, true) => None,
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
    ctx.memory_mut(|m| {
        let previous = m
            .data
            .get_temp_mut_or_default::<UiState>(egui::Id::new(UI_MEMORY_ID));
        let mut next = ui;
        reveal::retire_changed(previous, &mut next);
        *previous = next;
    });
}
fn clear_ui(ctx: &egui::Context) {
    ctx.memory_mut(|m| m.data.remove::<UiState>(egui::Id::new(UI_MEMORY_ID)));
}

/// 적용된 필터 제외 집합(hidden) 읽기. 미설정=빈 set=필터 없음(전체 표시).
fn read_filter(ctx: &egui::Context) -> HashSet<String> {
    ctx.memory(|m| {
        m.data
            .get_temp::<HashSet<String>>(egui::Id::new(FILTER_MEMORY_ID))
            .unwrap_or_default()
    })
}
fn write_filter(ctx: &egui::Context, hidden: HashSet<String>) {
    ctx.memory_mut(|m| m.data.insert_temp(egui::Id::new(FILTER_MEMORY_ID), hidden));
}

/// 현재 프로필들의 `kind` 집합(=프로토콜). KNOWN_TYPES 순서 우선, 나머지(플러그인/
/// unknown)는 알파벳. 디자인 `protocols` 도출 로직 전사. 프로필 0개인 kind 는 없음.
/// `tasty-attach` 는 Attach 탭 전담이라 프로토콜이 아니다 — 항상 제외.
fn protocol_set(profiles: &[RemoteProfile]) -> Vec<String> {
    let mut seen: Vec<String> = Vec::new();
    for p in profiles {
        let k = p.kind.trim();
        if !k.is_empty() && k != ATTACH_KIND && !seen.iter().any(|s| s == k) {
            seen.push(k.to_string());
        }
    }
    let mut out: Vec<String> = KNOWN_TYPES
        .iter()
        .filter(|t| seen.iter().any(|s| s == *t))
        .map(|t| t.to_string())
        .collect();
    let mut extra: Vec<String> = seen
        .into_iter()
        .filter(|s| !KNOWN_TYPES.contains(&s.as_str()))
        .collect();
    extra.sort();
    out.extend(extra);
    out
}

/// 코어/플러그인이 모르는 kind(필터 목록에서 ⚠ 표식 대상). row 배지 로직과 동일.
fn is_unknown_kind(kind: &str) -> bool {
    !is_builtin_kind(kind) && !KNOWN_TYPES.contains(&kind)
}

/// 닫을 때 폼·조회 슬롯·필터 초안을 버린다. 적용된 필터는 별도 키에 남긴다.
pub fn on_close_remote_tool_popup(
    ctx: &egui::Context,
    _state: &mut MainViewState,
    _engine: &crate::runtime::engine_read::EngineRead<'_>,
) {
    clear_ui(ctx);
}

/// PopupDef.draw_fn 진입점.
pub fn draw_remote_tool_popup(
    ui: &mut egui::Ui,
    state: &mut MainViewState,
    _engine: &crate::runtime::engine_read::EngineRead<'_>,
) -> PopupAction {
    let th = theme::theme();
    let ctx = ui.ctx().clone();
    let mut st = read_ui(&ctx);

    st.view = state.webview_identity.clone();

    let mut profiles = RemoteProfiles::load();
    let passkeys = Passkeys::load();

    // Escape: Form/Confirm 은 뒤로, List 면 닫기.
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        // 드롭다운이 열려 있으면 Escape로 드롭다운만 닫는다.
        if ctx.memory(|m| m.is_popup_open(egui::Id::new(FILTER_POPUP_ID))) {
            ctx.memory_mut(|m| m.close_popup());
            write_ui(&ctx, st);
            return PopupAction::None;
        }
        let sub = match st.tab {
            Tab::Profiles => &mut st.profile_view,
            Tab::Attach => &mut st.attach_view,
            Tab::Passkeys => &mut st.passkey_view,
        };
        if *sub == Sub::List {
            clear_ui(&ctx);
            return PopupAction::Close;
        }
        *sub = Sub::List;
        write_ui(&ctx, st);
        return PopupAction::None;
    }

    let mut close = false;
    // 구역마다 여백을 주고 구역 사이 세로 간격만 없앤다. 콘텐츠 내부에서는 간격을 복원한다.
    let full = ui.max_rect();
    let saved_spacing = ui.spacing().item_spacing;
    ui.spacing_mut().item_spacing.y = 0.0;
    let sep = egui::Stroke::new(th.border_width.value(), th.border_strong());

    let header_ir = egui::Frame::NONE
        .inner_margin(egui::Margin {
            left: PANEL_PAD_L,
            right: th.spacing_md.value() as i8,
            top: HEADER_PAD_Y,
            bottom: HEADER_PAD_Y,
        })
        .show(ui, |ui| draw_header(ui, &th));
    if header_ir.inner {
        close = true;
    }
    ui.painter()
        .hline(full.x_range(), header_ir.response.rect.bottom(), sep);
    // 실제 헤더 영역을 드래그 손잡이로 보고한다.
    super::report_header_drag_rect(
        ui.ctx(),
        REMOTE_TOOL_POPUP_ID,
        egui::Rect::from_x_y_ranges(
            full.x_range(),
            full.top()..=header_ir.response.rect.bottom(),
        ),
    );

    draw_tab_bar(ui, &th, &mut st, full.x_range());

    // 폼은 자체 여백과 고정 푸터를 그리므로 외곽 여백을 두지 않는다.
    let is_form = matches!(
        match st.tab {
            Tab::Profiles => &st.profile_view,
            Tab::Attach => &st.attach_view,
            Tab::Passkeys => &st.passkey_view,
        },
        Sub::Form
    );
    let content_margin = if is_form {
        egui::Margin::ZERO
    } else {
        egui::Margin {
            left: PANEL_PAD_L,
            right: PANEL_PAD_L,
            top: CONTENT_PAD_TOP,
            bottom: th.spacing_sm.value() as i8,
        }
    };
    egui::Frame::NONE
        .inner_margin(content_margin)
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing = saved_spacing;
            match st.tab {
                Tab::Profiles => draw_profiles_tab(ui, &th, &mut st, &mut profiles, &passkeys),
                Tab::Attach => draw_attach_tab(ui, &th, &mut st, &mut profiles, &passkeys),
                Tab::Passkeys => draw_passkeys_tab(ui, &th, &mut st, &passkeys),
            }
        });

    if close {
        clear_ui(&ctx);
        PopupAction::Close
    } else {
        write_ui(&ctx, st);
        PopupAction::None
    }
}

/// 오른쪽 행 버튼 위를 스크롤바가 가리지 않도록 막대를 숨긴다.
/// 남은 스크롤 영역은 가장자리 페이드로 알리며 휠·드래그·키보드 스크롤은 유지한다.
/// 근거: docs/architecture/ui-widgets-crate.md.
fn scroll_list_with_fade<R>(
    ui: &mut egui::Ui,
    th: &Theme,
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    let out = egui::ScrollArea::vertical()
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
        .drag_to_scroll(false)
        .show(ui, add_contents);
    let (up, down) = fade_edges(
        out.state.offset.y,
        out.content_size.y,
        out.inner_rect.height(),
    );
    if up {
        paint_edge_fade(ui, th, out.inner_rect, FadeEdge::Top);
    }
    if down {
        paint_edge_fade(ui, th, out.inner_rect, FadeEdge::Bottom);
    }
    out.inner
}

/// `(위쪽 페이드, 아래쪽 페이드)` 판정. 위/아래로 각각 남은 스크롤 여지가 있는지만 본다.
fn fade_edges(offset: f32, content_h: f32, view_h: f32) -> (bool, bool) {
    // 끝부분의 반올림 오차로 페이드가 남지 않도록 여유를 둔다.
    const EPS: LogicalPx = LogicalPx(1.0);
    let max_offset = (LogicalPx(content_h) - LogicalPx(view_h)).max(LogicalPx(0.0));
    let offset = LogicalPx(offset);
    (offset > EPS, offset < max_offset - EPS)
}

/// 페이드를 그릴 가장자리.
#[derive(Clone, Copy, PartialEq, Eq)]
enum FadeEdge {
    Top,
    Bottom,
}

/// 위·아래 페이드가 겹치지 않도록 각각 뷰포트 높이의 절반으로 제한한다.
fn fade_height(max: f32, view_h: f32) -> f32 {
    max.min(view_h * 0.5)
}

/// 정점 색을 보간한 메시로 페이드를 그린다. Color32는 premultiplied이므로
/// 불투명 배경색과 TRANSPARENT 사이를 보간한다.
fn paint_edge_fade(ui: &egui::Ui, th: &Theme, rect: egui::Rect, edge: FadeEdge) {
    let height = fade_height(th.spacing_xl.value(), rect.height());
    let (solid_y, clear_y) = match edge {
        FadeEdge::Top => (rect.top(), rect.top() + height),
        FadeEdge::Bottom => (rect.bottom(), rect.bottom() - height),
    };
    let solid: egui::Color32 = th.bg_panel().into();
    let clear = egui::Color32::TRANSPARENT;
    let mut mesh = egui::epaint::Mesh::default();
    mesh.colored_vertex(egui::pos2(rect.left(), solid_y), solid);
    mesh.colored_vertex(egui::pos2(rect.right(), solid_y), solid);
    mesh.colored_vertex(egui::pos2(rect.left(), clear_y), clear);
    mesh.colored_vertex(egui::pos2(rect.right(), clear_y), clear);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(2, 1, 3);
    ui.painter().add(egui::Shape::mesh(mesh));
}

fn draw_header(ui: &mut egui::Ui, th: &Theme) -> bool {
    let mut close = false;
    // 헤더 글자 선택이 창 드래그를 가로채지 않도록 헤더 안에서만 텍스트 선택을 끈다.
    ui.style_mut().interaction.selectable_labels = false;
    ui.horizontal(|ui| {
        // 폰트의 실제 높이가 디자인 헤더보다 낮아 최소 높이와 보더 여유를 확보한다.
        ui.set_min_height(th.remote_tool_header_min_height().value());
        ui.spacing_mut().item_spacing.x = HEADER_GAP_X.value();
        ui.add(icons::TERMINAL_PROMPT.image(th.icon_glyph_size_md.value(), th.text_muted().into()));
        ui.label(
            egui::RichText::new(t("remote_tool.heading"))
                .color(th.text_primary())
                .size(th.font_size_heading.value())
                .strong(),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if IconButton::new()
                .variant(IconButtonVariant::Ghost)
                .size(ControlSize::Sm)
                .show(ui, th, &|ui, rect, c| {
                    icons::CLOSE.image(rect.height(), c).paint_at(ui, rect)
                })
                .on_hover_text(t("remote_tool.close"))
                .clicked()
            {
                close = true;
            }
        });
    });
    close
}

fn draw_tab_bar(ui: &mut egui::Ui, th: &Theme, st: &mut UiState, x_range: egui::Rangef) {
    // 공용 탭 위젯을 쓰며, 탭을 바꾸면 하위 화면을 목록으로 돌리고 폼 오류를 지운다.
    let labels = [
        t("remote_tool.tab_profiles"),
        t("remote_tool.tab_attach"),
        t("remote_tool.tab_passkeys"),
    ];
    let active = match st.tab {
        Tab::Profiles => 0,
        Tab::Attach => 1,
        Tab::Passkeys => 2,
    };
    let clicked = draw_tab_strip(
        ui,
        th,
        &TabStripData {
            labels: &labels,
            active,
            x_range,
        },
    );
    if let Some(i) = clicked {
        st.tab = match i {
            0 => Tab::Profiles,
            1 => Tab::Attach,
            _ => Tab::Passkeys,
        };
        st.profile_view = Sub::List;
        st.attach_view = Sub::List;
        st.passkey_view = Sub::List;
        st.perr = None;
        st.aerr = None;
        st.kerr = None;
    }
}

fn draw_profiles_tab(
    ui: &mut egui::Ui,
    th: &Theme,
    st: &mut UiState,
    profiles: &mut RemoteProfiles,
    passkeys: &Passkeys,
) {
    match st.profile_view.clone() {
        Sub::List => draw_profile_list(ui, th, st, profiles, passkeys),
        Sub::Form => draw_profile_form(ui, th, st, passkeys),
        Sub::ConfirmDelete(name) => {
            if let Some(act) = draw_confirm_delete(
                ui,
                th,
                t("remote_tool.noun_profile"),
                &name,
                st.perr.as_deref(),
                st.pending.load(Ordering::Relaxed) != 0,
            ) {
                if act {
                    enqueue(ui.ctx(), st, FileAction::DeleteProfile(name.clone()));
                } else {
                    st.profile_view = Sub::List;
                }
            }
        }
    }
}

fn draw_profile_list(
    ui: &mut egui::Ui,
    th: &Theme,
    st: &mut UiState,
    profiles: &RemoteProfiles,
    passkeys: &Passkeys,
) {
    let ctx = ui.ctx().clone();
    let protocols = protocol_set(&profiles.profiles);
    let applied_hidden = read_filter(&ctx);

    let mut add_clicked = false;
    let mut new_filter: Option<HashSet<String>> = None;
    ui.horizontal(|ui| {
        add_clicked = add_button(ui, th, t("remote_tool.profile_add")).clicked();
        if protocols.len() >= 2 {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                new_filter = draw_protocol_filter(ui, th, st, &protocols, &applied_hidden);
            });
        }
    });
    if let Some(h) = &new_filter {
        write_filter(&ctx, h.clone());
    }
    let applied_hidden = new_filter.unwrap_or(applied_hidden);

    if add_clicked {
        st.pform = ProfileForm {
            kind: "ssh".into(),
            shell: "auto".into(),
            ..Default::default()
        };
        st.perr = None;
        st.profile_view = Sub::Form;
        return;
    }
    ui.add_space(th.spacing_xs.value());
    let has_non_attach = profiles
        .profiles
        .iter()
        .any(|p| p.kind.trim() != ATTACH_KIND);
    let any_visible = profiles
        .profiles
        .iter()
        .any(|p| p.kind.trim() != ATTACH_KIND && !applied_hidden.contains(p.kind.trim()));
    let detecting = st.detecting.as_ref().map(|j| j.name.clone());
    let known: Vec<String> = passkeys.passkeys.iter().map(|k| k.name.clone()).collect();
    // SSH config는 캐시에서 읽고 프로필과 같은 스크롤 영역에 표시한다.
    // 프로필 목록이 비어도 가져올 호스트를 볼 수 있도록 로컬 섹션은 남긴다.
    let local = st.local.get_or_insert_with(load_local_ssh);
    let mut action: Option<(usize, ProfileRowAction)> = None;
    let mut local_import: Option<String> = None;
    scroll_list_with_fade(ui, th, |ui| {
        if let Some(key) = profile_empty_key(has_non_attach, any_visible) {
            let msg = t(key);
            ui.vertical_centered(|ui| {
                ui.add_space(th.spacing_lg.value());
                selectable_text(
                    ui,
                    msg,
                    th.text_muted(),
                    th.font_size_body.value(),
                    false,
                    true,
                    TextWrap::None,
                );
                ui.add_space(th.spacing_lg.value());
            });
        } else {
            for (i, p) in profiles.profiles.iter().enumerate() {
                if p.kind.trim() == ATTACH_KIND || applied_hidden.contains(p.kind.trim()) {
                    continue;
                }
                if let Some(a) = draw_profile_row(ui, th, p, detecting.as_deref(), &known) {
                    action = Some((i, a));
                }
            }
        }
        local_import = draw_local_ssh_section(ui, th, local, profiles);
    });
    if let Some(alias) = local_import {
        st.pform = import_prefill(&alias);
        st.perr = None;
        st.profile_view = Sub::Form;
        return;
    }
    if let Some((i, a)) = action {
        let p = &profiles.profiles[i];
        match a {
            ProfileRowAction::Edit => {
                st.pform = form_from_profile(p, passkeys);
                st.perr = None;
                st.profile_view = Sub::Form;
            }
            ProfileRowAction::Delete => {
                st.profile_view = Sub::ConfirmDelete(p.name.clone());
            }
            ProfileRowAction::Redetect => {
                if st.detecting.is_none() {
                    enqueue(ui.ctx(), st, FileAction::Detect(p.name.clone()));
                }
            }
        }
    }
}

/// 프로필 아래에 읽기 전용 SSH config 목록을 표시한다. 가져오기만 허용하며
/// 프로토콜 필터는 적용하지 않는다. 공용 위젯에 번역·빈 상태·가져오기 여부를 전달한다.
fn draw_local_ssh_section(
    ui: &mut egui::Ui,
    th: &Theme,
    local: &LocalSshCache,
    profiles: &RemoteProfiles,
) -> Option<String> {
    let targets: Vec<String> = local.hosts.iter().map(local_target_hint).collect();
    let rows: Vec<LocalSshHost<'_>> = local
        .hosts
        .iter()
        .enumerate()
        .map(|(i, h)| LocalSshHost {
            alias: &h.alias,
            target: &targets[i],
            in_profiles: imported_as(profiles, &h.alias).is_some(),
        })
        .collect();
    ssh_section_view(
        ui,
        th,
        &LocalSshSectionData {
            heading: t("remote_tool.local_ssh_heading"),
            path: &local.path,
            in_profiles_tag: t("remote_tool.local_ssh_in_profiles"),
            add_label: t("remote_tool.local_ssh_add"),
            empty_message: t(local_ssh_empty_key(local)),
            hosts: &rows,
        },
    )
    .and_then(|i| local.hosts.get(i).map(|h| h.alias.clone()))
}

/// 필터 초안을 편집하고 적용 때 현재 프로토콜과 교집합을 취한 제외 목록을 반환한다.
/// 공용 위젯을 사용하며 열기·초안 초기화·닫기·영역 보고는 이 함수에서 처리한다.
fn draw_protocol_filter(
    ui: &mut egui::Ui,
    th: &Theme,
    st: &mut UiState,
    protocols: &[String],
    applied_hidden: &HashSet<String>,
) -> Option<HashSet<String>> {
    let popup_id = egui::Id::new(FILTER_POPUP_ID);
    let total = protocols.len();
    let active_hidden = protocols
        .iter()
        .filter(|p| applied_hidden.contains(*p))
        .count();
    let filtered = active_hidden > 0;
    let selected = total - active_hidden;
    let label = if filtered {
        format!("{} · {}/{}", t("remote_tool.filter"), selected, total)
    } else {
        t("remote_tool.filter").to_string()
    };

    let btn = draw_protocol_filter_button(ui, th, &label, filtered);
    if btn.clicked() {
        if !ui.memory(|m| m.is_popup_open(popup_id)) {
            st.filter_draft = applied_hidden.clone();
        }
        ui.memory_mut(|m| m.toggle_popup(popup_id));
    }

    let items: Vec<ProtocolFilterItem<'_>> = protocols
        .iter()
        .map(|p| ProtocolFilterItem {
            name: p,
            unknown: is_unknown_kind(p),
        })
        .collect();
    let labels = ProtocolFilterLabels {
        title: t("remote_tool.filter_title"),
        select_all: t("remote_tool.filter_select_all"),
        deselect_all: t("remote_tool.filter_deselect_all"),
        reset: t("remote_tool.filter_reset"),
        apply: t("remote_tool.filter_apply"),
        unknown: t("remote_tool.filter_unknown"),
        unknown_hint: t("remote_tool.type_unknown_hint"),
    };
    let mut applied: Option<HashSet<String>> = None;
    // 본문의 구획이 프레임 끝까지 닿도록 popup 프레임의 안쪽 여백을 없앤다. 래퍼에도 둘레 0 을 넘긴다.
    ui.scope(|ui| {
        ui.spacing_mut().menu_margin = egui::Margin::ZERO;
        // 프레임은 이 scope 의 스타일로 그려지고 popup 안쪽 Ui 는 컨텍스트 스타일을 받으므로,
        // 테두리 + 안쪽 여백은 여기서 잰다. 토큰은 테두리를 포함한 폭이다.
        let frame_x = egui::Frame::popup(ui.style()).total_margin().sum().x;
        tasty_egui_theme::with_popover_frame_ring(ui, th, egui::Margin::ZERO, |ui| {
            egui::popup::popup_above_or_below_widget(
                ui,
                popup_id,
                &btn,
                egui::AboveOrBelow::Below,
                egui::PopupCloseBehavior::CloseOnClickOutside,
                |ui| {
                    ui.set_min_width(filter_dropdown_content_width(th, LogicalPx(frame_x)).value());
                    if draw_protocol_filter_body(ui, th, &items, &labels, &mut st.filter_draft) {
                        applied = Some(
                            st.filter_draft
                                .iter()
                                .filter(|p| protocols.iter().any(|x| x == *p))
                                .cloned()
                                .collect(),
                        );
                    }
                },
            )
        });
    });
    // 부모 밖으로 나온 드롭다운도 안쪽 클릭으로 인식하도록 영역을 보고한다.
    super::report_dropdown_overlay(ui, REMOTE_TOOL_POPUP_ID, FILTER_POPUP_ID, popup_id);
    if applied.is_some() {
        ui.memory_mut(|m| m.close_popup());
    }
    applied
}

enum ProfileRowAction {
    Edit,
    Delete,
    Redetect,
}

fn draw_profile_row(
    ui: &mut egui::Ui,
    th: &Theme,
    p: &RemoteProfile,
    detecting: Option<&str>,
    passkey_names: &[String],
) -> Option<ProfileRowAction> {
    let mut out = None;
    let ssh = p.as_ssh();
    let is_ssh = ssh.is_some();
    let disabled = ssh.as_ref().map(|v| v.is_disabled()).unwrap_or(false);
    let detecting_now = detecting == Some(p.name.as_str());
    let kind_chip = if is_builtin_kind(&p.kind) || KNOWN_TYPES.contains(&p.kind.as_str()) {
        RemoteRowChip::TagMono(&p.kind)
    } else {
        RemoteRowChip::Warn {
            text: &p.kind,
            tooltip: t("remote_tool.type_unknown_hint"),
        }
    };
    let action_count = if is_ssh { 3 } else { 2 };
    remote_list_row(
        ui,
        th,
        action_count,
        |ui, w| {
            remote_row_title(
                ui,
                th,
                &p.name,
                p.label.as_deref(),
                disabled,
                &[kind_chip],
                w,
            );
            selectable_text(
                ui,
                &profile_summary(p),
                th.text_muted(),
                th.font_size_caption.value(),
                true,
                false,
                TextWrap::Truncate(w),
            );
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = th.spacing_sm.value();
                match &p.passkey_ref {
                    Some(pr) if !pr.is_empty() => {
                        selectable_label(
                            ui,
                            &format!("passkey: {pr}"),
                            th.text_muted(),
                            th.font_size_caption.value(),
                            false,
                        );
                        if !passkey_names.contains(pr) {
                            warn_badge(
                                ui,
                                th,
                                t("remote_tool.passkey_missing"),
                                t("remote_tool.passkey_missing_hint"),
                            );
                        }
                    }
                    _ => {
                        selectable_label(
                            ui,
                            "passkey: —",
                            th.text_muted(),
                            th.font_size_caption.value(),
                            false,
                        );
                    }
                }
                if let Some(v) = &ssh {
                    selectable_label(
                        ui,
                        &format!("shell: {}", v.shell()),
                        th.text_muted(),
                        th.font_size_caption.value(),
                        false,
                    );
                    if detecting_now {
                        ui.add(egui::Spinner::new().size(th.font_size_caption.value()));
                        selectable_label(
                            ui,
                            t("remote_tool.detecting"),
                            th.text_muted(),
                            th.font_size_caption.value(),
                            false,
                        );
                    } else if disabled {
                        selectable_label(
                            ui,
                            t("remote_tool.detect_failed"),
                            th.accent_danger_raised(),
                            th.font_size_caption.value(),
                            false,
                        );
                    }
                }
            });
        },
        |ui| {
            // 오른쪽부터 배치하므로 삭제 버튼을 먼저 그린다.
            if row_icon_button(ui, th, icons::TRASH, true)
                .on_hover_text(t("remote_tool.delete"))
                .clicked()
            {
                out = Some(ProfileRowAction::Delete);
            }
            if row_icon_button(ui, th, icons::EDIT, true)
                .on_hover_text(t("remote_tool.edit"))
                .clicked()
            {
                out = Some(ProfileRowAction::Edit);
            }
            if is_ssh
                && row_icon_button(ui, th, icons::REFRESH, !detecting_now)
                    .on_hover_text(t("remote_tool.refresh_tooltip"))
                    .clicked()
            {
                out = Some(ProfileRowAction::Redetect);
            }
        },
    );
    out
}

fn profile_summary(p: &RemoteProfile) -> String {
    if let Some(v) = p.as_ssh() {
        let mut s = v.ssh_destination();
        if let Some(port) = v.port()
            && port != 22
        {
            s = format!("{s}:{port}");
        }
        s
    } else {
        let parts: Vec<String> = p
            .fields
            .iter()
            .filter_map(|(k, val)| val.as_str().map(|v| format!("{k}={v}")))
            .take(2)
            .collect();
        if parts.is_empty() {
            "—".into()
        } else {
            parts.join("  ")
        }
    }
}

pub(crate) fn form_from_profile(p: &RemoteProfile, _passkeys: &Passkeys) -> ProfileForm {
    let mut f = ProfileForm {
        kind: p.kind.clone(),
        name: p.name.clone(),
        label: p.label.clone().unwrap_or_default(),
        passkey_ref: p.passkey_ref.clone().unwrap_or_default(),
        editing_original: Some(p.name.clone()),
        original: Some(p.clone()),
        shell: "auto".into(),
        ..Default::default()
    };
    if let Some(v) = p.as_ssh() {
        f.host = v.host().unwrap_or("").to_string();
        f.user = v.user().unwrap_or("").to_string();
        f.port = v.port().map(|n| n.to_string()).unwrap_or_default();
        f.shell = v.shell().to_string();
    } else {
        f.fields = p
            .fields
            .iter()
            .filter_map(|(k, val)| val.as_str().map(|v| (k.clone(), v.to_string())))
            .collect();
    }
    f
}

/// 저장·취소 푸터와 여백을 둔 스크롤 본문으로 된 편집 폼. 눌린 버튼을 (저장, 취소)로 돌려준다.
fn edit_form(
    ui: &mut egui::Ui,
    th: &Theme,
    footer_id: &'static str,
    pending: bool,
    body: impl FnOnce(&mut egui::Ui),
) -> (bool, bool) {
    let full_x = ui.clip_rect().x_range();
    let sep = egui::Stroke::new(th.border_width.value(), th.border_strong());
    let pad_lg = th.spacing_lg.value() as i8;
    let pad_md = th.spacing_md.value() as i8;

    let mut do_save = false;
    let mut do_cancel = false;
    let footer = egui::TopBottomPanel::bottom(footer_id)
        .resizable(false)
        .show_separator_line(false)
        .frame(egui::Frame::NONE.inner_margin(egui::Margin {
            left: pad_lg,
            right: pad_lg,
            top: pad_md,
            bottom: pad_md,
        }))
        .show_inside(ui, |ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add_enabled_ui(!pending, |ui| primary_button(ui, th, t("remote_tool.save")))
                    .inner
                    .clicked()
                {
                    do_save = true;
                }
                ui.add_space(th.spacing_sm.value());
                if ghost_button(ui, th, t("remote_tool.cancel")).clicked() {
                    do_cancel = true;
                }
            });
        });
    ui.painter()
        .hline(full_x, footer.response.rect.top() + 0.5, sep);

    egui::CentralPanel::default()
        .frame(egui::Frame::NONE)
        .show_inside(ui, |ui| {
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .drag_to_scroll(false)
                .show(ui, |ui| {
                    egui::Frame::NONE
                        .inner_margin(egui::Margin {
                            left: pad_lg,
                            right: pad_lg,
                            top: pad_md,
                            bottom: pad_md,
                        })
                        .show(ui, |ui| {
                            body(ui);
                        });
                });
        });
    (do_save, do_cancel)
}

fn draw_profile_form(ui: &mut egui::Ui, th: &Theme, st: &mut UiState, passkeys: &Passkeys) {
    // 폼은 스크롤 본문과 고정 푸터를 나누고 자체 여백을 적용한다.
    let (do_save, do_cancel) = edit_form(
        ui,
        th,
        "remote_tool.profile_footer",
        st.pending.load(Ordering::Relaxed) != 0,
        |ui| {
            let editing = st.pform.editing_original.is_some();
            selectable_label(
                ui,
                if editing {
                    t("remote_tool.profile_form_edit")
                } else {
                    t("remote_tool.profile_form_add")
                },
                th.text_primary(),
                th.font_size_body.value(),
                false,
            );
            ui.add_space(th.spacing_md.value());

            let f = &mut st.pform;
            let is_ssh = f.kind.trim() == "ssh";
            let unknown = !f.kind.trim().is_empty()
                && !is_builtin_kind(f.kind.trim())
                && !KNOWN_TYPES.contains(&f.kind.trim());

            ui.spacing_mut().item_spacing.y = th.spacing_sm.value();

            // datalist 대신 텍스트 입력과 제안 콤보를 붙여 사용한다.
            form_row(ui, th, t("remote_tool.field_type"), |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = th.spacing_xs.value();
                    let combo_w = th.item_height_interactive.value();
                    let edit_w =
                        (ui.available_width() - combo_w - ui.spacing().item_spacing.x).max(0.0);
                    ui.add(
                        egui::TextEdit::singleline(&mut f.kind)
                            .desired_width(edit_w)
                            .font(egui::TextStyle::Monospace),
                    );
                    tasty_egui_theme::with_popover_frame(ui, th, |ui| {
                        egui::ComboBox::from_id_salt("remote_tool.type_suggest")
                            .selected_text("▾")
                            .width(combo_w)
                            .show_ui(ui, |ui| {
                                ui.spacing_mut().item_spacing.y = 0.0;
                                for kt in KNOWN_TYPES {
                                    tasty_ui_widgets::menu_option_value(
                                        ui,
                                        th,
                                        &mut f.kind,
                                        (*kt).to_string(),
                                        kt,
                                    );
                                }
                            })
                    });
                });
            });
            if unknown {
                indented_hint(
                    ui,
                    th,
                    t("remote_tool.type_unknown_hint"),
                    th.accent_warning_raised(),
                    false,
                );
            }

            if is_ssh {
                text_row(
                    ui,
                    th,
                    t("remote_tool.field_name"),
                    &mut f.name,
                    "prod-web",
                    false,
                );
                connection_rows(ui, th, &mut f.host, &mut f.user, &mut f.port);
                text_row(
                    ui,
                    th,
                    t("remote_tool.field_label"),
                    &mut f.label,
                    "us-east",
                    false,
                );
                shell_row(ui, th, "remote_tool.shell", &mut f.shell);
                passkey_dropdown_row(ui, th, &mut f.passkey_ref, passkeys);
                if f.shell == "auto" {
                    indented_hint(
                        ui,
                        th,
                        t("remote_tool.shell_auto_hint"),
                        th.text_muted(),
                        false,
                    );
                }
            } else {
                text_row(
                    ui,
                    th,
                    t("remote_tool.field_name"),
                    &mut f.name,
                    "media-nas",
                    false,
                );
                text_row(
                    ui,
                    th,
                    t("remote_tool.field_label"),
                    &mut f.label,
                    "lab",
                    false,
                );
                passkey_dropdown_row(ui, th, &mut f.passkey_ref, passkeys);
                ui.add_space(th.spacing_xs.value());
                ui.horizontal(|ui| {
                    selectable_label(
                        ui,
                        t("remote_tool.fields_section"),
                        th.text_muted(),
                        th.font_size_caption.value(),
                        true,
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ghost_button(ui, th, t("remote_tool.field_add")).clicked() {
                            f.fields.push((String::new(), String::new()));
                        }
                    });
                });
                if f.fields.is_empty() {
                    indented_hint(ui, th, t("remote_tool.fields_empty"), th.text_muted(), true);
                }
                let mut remove_idx = None;
                for (i, (k, v)) in f.fields.iter_mut().enumerate() {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = th.spacing_sm.value();
                        ui.add(
                            egui::TextEdit::singleline(k)
                                .desired_width(LABEL_COL_WIDTH.value())
                                .hint_text(tasty_egui_theme::hint_text(th, "key"))
                                .font(egui::TextStyle::Monospace),
                        );
                        let btn_w = th.item_height_interactive.value();
                        let val_w =
                            (ui.available_width() - btn_w - ui.spacing().item_spacing.x).max(0.0);
                        ui.add(
                            egui::TextEdit::singleline(v)
                                .desired_width(val_w)
                                .hint_text(tasty_egui_theme::hint_text(th, "value"))
                                .font(egui::TextStyle::Monospace),
                        );
                        if ghost_button(ui, th, "×").clicked() {
                            remove_idx = Some(i);
                        }
                    });
                }
                if let Some(i) = remove_idx {
                    f.fields.remove(i);
                }
            }

            if let Some(err) = &st.perr {
                indented_hint(ui, th, err, th.accent_danger_raised(), false);
            }
        },
    );

    if do_cancel {
        st.perr = None;
        st.profile_view = Sub::List;
        return;
    }
    if do_save {
        enqueue(ui.ctx(), st, FileAction::SaveProfile(st.pform.clone()));
    }
}

fn passkey_dropdown_row(ui: &mut egui::Ui, th: &Theme, value: &mut String, passkeys: &Passkeys) {
    form_row(ui, th, t("remote_tool.field_passkey"), |ui| {
        let sel = if value.is_empty() {
            t("remote_tool.passkey_none").to_string()
        } else {
            value.clone()
        };
        tasty_egui_theme::with_popover_frame(ui, th, |ui| {
            egui::ComboBox::from_id_salt("remote_tool.passkey_ref")
                .selected_text(sel)
                .width(ui.available_width())
                .show_ui(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    tasty_ui_widgets::menu_option_value(
                        ui,
                        th,
                        value,
                        String::new(),
                        t("remote_tool.passkey_none"),
                    );
                    for k in &passkeys.passkeys {
                        tasty_ui_widgets::menu_option_value(ui, th, value, k.name.clone(), &k.name);
                    }
                })
        });
    });
}

fn draw_attach_tab(
    ui: &mut egui::Ui,
    th: &Theme,
    st: &mut UiState,
    profiles: &mut RemoteProfiles,
    passkeys: &Passkeys,
) {
    match st.attach_view.clone() {
        Sub::List => draw_attach_list(ui, th, st, profiles),
        Sub::Form => draw_attach_form(ui, th, st, profiles, passkeys),
        Sub::ConfirmDelete(name) => {
            if let Some(act) = draw_confirm_delete(
                ui,
                th,
                t("remote_tool.noun_attach"),
                &name,
                st.aerr.as_deref(),
                st.pending.load(Ordering::Relaxed) != 0,
            ) {
                if act {
                    enqueue(ui.ctx(), st, FileAction::DeleteProfile(name.clone()));
                } else {
                    st.attach_view = Sub::List;
                }
            }
        }
    }
}

fn draw_attach_list(ui: &mut egui::Ui, th: &Theme, st: &mut UiState, profiles: &RemoteProfiles) {
    if add_button(ui, th, t("remote_tool.attach_add")).clicked() {
        st.aform = AttachForm {
            mode_ref: true,
            shell: "auto".into(),
            remote_tasty: "tasty".into(),
            port_mode: "auto".into(),
            ..Default::default()
        };
        st.aerr = None;
        st.attach_view = Sub::Form;
        return;
    }
    ui.add_space(th.spacing_xs.value());
    let has_attach = profiles.profiles.iter().any(|p| p.kind == ATTACH_KIND);
    if !has_attach {
        ui.vertical_centered(|ui| {
            ui.add_space(th.spacing_lg.value());
            selectable_text(
                ui,
                t("remote_tool.attach_empty"),
                th.text_muted(),
                th.font_size_body.value(),
                false,
                true,
                TextWrap::None,
            );
        });
        return;
    }
    let mut action: Option<(String, AttachRowAction)> = None;
    scroll_list_with_fade(ui, th, |ui| {
        for p in profiles.profiles.iter().filter(|p| p.kind == ATTACH_KIND) {
            if let Some(a) = draw_attach_row(ui, th, p, profiles) {
                action = Some((p.name.clone(), a));
            }
        }
    });
    if let Some((name, a)) = action {
        match a {
            AttachRowAction::Edit => {
                if let Some(p) = profiles.get(&name) {
                    st.aform = form_from_attach(p);
                    st.aerr = None;
                    st.attach_view = Sub::Form;
                }
            }
            AttachRowAction::Delete => {
                st.attach_view = Sub::ConfirmDelete(name);
            }
        }
    }
}

enum AttachRowAction {
    Edit,
    Delete,
}

/// 이름·접속 요약·원격 Tasty 설정을 표시하는 Attach 행.
fn draw_attach_row(
    ui: &mut egui::Ui,
    th: &Theme,
    p: &RemoteProfile,
    profiles: &RemoteProfiles,
) -> Option<AttachRowAction> {
    let v = p.as_attach()?;
    // 참조가 없거나 감지에 실패한 프로필은 비활성으로 표시한다.
    let super::remote_attach::AttachStatus {
        missing,
        inactive,
        target,
    } = super::remote_attach::attach_status(&v, profiles);
    let mode_tag = if v.ssh_ref().is_some() {
        t("remote_tool.attach_tag_profile")
    } else {
        t("remote_tool.attach_tag_inline")
    };
    let mut out = None;
    let mut chips = vec![RemoteRowChip::Tag(mode_tag)];
    if inactive {
        chips.push(RemoteRowChip::Warn {
            text: t("remote_tool.attach_inactive"),
            tooltip: t("remote_tool.attach_inactive_hint"),
        });
    }
    remote_list_row(
        ui,
        th,
        2,
        |ui, w| {
            remote_row_title(ui, th, &p.name, p.label.as_deref(), inactive, &chips, w);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = th.spacing_sm.value();
                let missing_w = if missing {
                    warn_badge_width(ui, th, t("remote_tool.attach_profile_missing"))
                        + th.spacing_sm.value()
                } else {
                    0.0
                };
                selectable_text(
                    ui,
                    &target,
                    th.text_muted(),
                    th.font_size_caption.value(),
                    true,
                    false,
                    TextWrap::Truncate((w - missing_w).max(0.0)),
                );
                if missing {
                    warn_badge(
                        ui,
                        th,
                        t("remote_tool.attach_profile_missing"),
                        t("remote_tool.attach_profile_missing_hint"),
                    );
                }
            });
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = th.spacing_md.value();
                selectable_label(
                    ui,
                    &format!("tasty: {}", v.remote_tasty()),
                    th.text_muted(),
                    th.font_size_caption.value(),
                    false,
                );
                selectable_label(
                    ui,
                    &format!("port: {}", v.port_mode()),
                    th.text_muted(),
                    th.font_size_caption.value(),
                    false,
                );
            });
        },
        |ui| {
            if row_icon_button(ui, th, icons::TRASH, true)
                .on_hover_text(t("remote_tool.delete"))
                .clicked()
            {
                out = Some(AttachRowAction::Delete);
            }
            if row_icon_button(ui, th, icons::EDIT, true)
                .on_hover_text(t("remote_tool.edit"))
                .clicked()
            {
                out = Some(AttachRowAction::Edit);
            }
        },
    );
    out
}

/// 목록 머리줄의 추가 버튼. 세 탭이 같은 secondary sm + plus 형태를 쓴다.
fn add_button(ui: &mut egui::Ui, th: &Theme, label: &str) -> egui::Response {
    Button::new(label)
        .variant(ButtonVariant::Secondary)
        .size(ControlSize::Sm)
        .leading_icon(&|ui, rect, c| icons::PLUS.image(rect.height(), c).paint_at(ui, rect))
        .show(ui, th)
}

/// 목록 행의 아이콘 버튼 — 세 탭이 같은 ghost sm.
fn row_icon_button(
    ui: &mut egui::Ui,
    th: &Theme,
    glyph: icons::Icon,
    enabled: bool,
) -> egui::Response {
    IconButton::new()
        .variant(IconButtonVariant::Ghost)
        .size(ControlSize::Sm)
        .enabled(enabled)
        .show(ui, th, &|ui, rect, c| {
            glyph.image(rect.height(), c).paint_at(ui, rect)
        })
}

pub(crate) fn form_from_attach(p: &RemoteProfile) -> AttachForm {
    let Some(v) = p.as_attach() else {
        return AttachForm::default();
    };
    AttachForm {
        name: p.name.clone(),
        label: p.label.clone().unwrap_or_default(),
        mode_ref: v.ssh_ref().is_some(),
        ssh_ref: v.ssh_ref().unwrap_or("").to_string(),
        host: v.host().unwrap_or("").to_string(),
        user: v.user().unwrap_or("").to_string(),
        port: v.port().map(|n| n.to_string()).unwrap_or_default(),
        shell: v.shell().to_string(),
        passkey_ref: p.passkey_ref.clone().unwrap_or_default(),
        remote_tasty: v.remote_tasty().to_string(),
        port_mode: v.port_mode().to_string(),
        port_file: v.port_file().unwrap_or("").to_string(),
        editing_original: Some(p.name.clone()),
        original: Some(p.clone()),
    }
}

fn draw_attach_form(
    ui: &mut egui::Ui,
    th: &Theme,
    st: &mut UiState,
    profiles: &mut RemoteProfiles,
    passkeys: &Passkeys,
) {
    let (do_save, do_cancel) = edit_form(
        ui,
        th,
        "remote_tool.attach_footer",
        st.pending.load(Ordering::Relaxed) != 0,
        |ui| {
            let editing = st.aform.editing_original.is_some();
            selectable_label(
                ui,
                if editing {
                    t("remote_tool.attach_form_edit")
                } else {
                    t("remote_tool.attach_form_add")
                },
                th.text_primary(),
                th.font_size_body.value(),
                false,
            );
            ui.add_space(th.spacing_md.value());

            let f = &mut st.aform;
            ui.spacing_mut().item_spacing.y = th.spacing_sm.value();

            text_row(
                ui,
                th,
                t("remote_tool.field_name"),
                &mut f.name,
                "gb10",
                false,
            );
            text_row(
                ui,
                th,
                t("remote_tool.field_label"),
                &mut f.label,
                "us-east",
                false,
            );
            form_row(ui, th, t("remote_tool.field_connection"), |ui| {
                let selected = if f.mode_ref { 0 } else { 1 };
                if let Some(i) = tasty_ui_widgets::segmented(
                    ui,
                    th,
                    &[
                        t("remote_tool.attach_mode_ref"),
                        t("remote_tool.attach_mode_inline"),
                    ],
                    selected,
                ) {
                    f.mode_ref = i == 0;
                }
            });
            ui.add_space(th.spacing_xs.value());

            if f.mode_ref {
                form_row(ui, th, t("remote_tool.field_ssh_ref"), |ui| {
                    let sel = if f.ssh_ref.is_empty() {
                        t("remote_tool.ssh_ref_none").to_string()
                    } else {
                        f.ssh_ref.clone()
                    };
                    tasty_egui_theme::with_popover_frame(ui, th, |ui| {
                        egui::ComboBox::from_id_salt("remote_tool.attach_ssh_ref")
                            .selected_text(sel)
                            .width(ui.available_width())
                            .show_ui(ui, |ui| {
                                ui.spacing_mut().item_spacing.y = 0.0;
                                for sp in profiles.profiles.iter().filter(|sp| sp.kind == "ssh") {
                                    let display = match &sp.label {
                                        Some(l) if !l.is_empty() => {
                                            format!("{} ({})", sp.name, l)
                                        }
                                        _ => sp.name.clone(),
                                    };
                                    tasty_ui_widgets::menu_option_value(
                                        ui,
                                        th,
                                        &mut f.ssh_ref,
                                        sp.name.clone(),
                                        &display,
                                    );
                                }
                            })
                    });
                });
            } else {
                connection_rows(ui, th, &mut f.host, &mut f.user, &mut f.port);
                shell_row(ui, th, "remote_tool.attach_shell", &mut f.shell);
                passkey_dropdown_row(ui, th, &mut f.passkey_ref, passkeys);
            }

            ui.add_space(th.spacing_xs.value());
            selectable_label(
                ui,
                t("remote_tool.remote_tasty_section"),
                th.text_muted(),
                th.font_size_caption.value(),
                true,
            );
            text_row(
                ui,
                th,
                t("remote_tool.field_executable"),
                &mut f.remote_tasty,
                "tasty",
                true,
            );
            form_row(ui, th, t("remote_tool.field_port_mode"), |ui| {
                tasty_egui_theme::with_popover_frame(ui, th, |ui| {
                    egui::ComboBox::from_id_salt("remote_tool.attach_port_mode")
                        .selected_text(f.port_mode.clone())
                        .width(ui.available_width())
                        .show_ui(ui, |ui| {
                            ui.spacing_mut().item_spacing.y = 0.0;
                            for m in PORT_MODES {
                                tasty_ui_widgets::menu_option_value(
                                    ui,
                                    th,
                                    &mut f.port_mode,
                                    (*m).to_string(),
                                    m,
                                );
                            }
                        })
                });
            });
            text_row(
                ui,
                th,
                t("remote_tool.field_port_file"),
                &mut f.port_file,
                t("remote_tool.attach_port_file_ph"),
                true,
            );
            indented_hint(
                ui,
                th,
                t("remote_tool.attach_exec_hint"),
                th.text_muted(),
                false,
            );

            if let Some(err) = &st.aerr {
                indented_hint(ui, th, err, th.accent_danger_raised(), false);
            }
        },
    );

    if do_cancel {
        st.aerr = None;
        st.attach_view = Sub::List;
        return;
    }
    if do_save {
        enqueue(ui.ctx(), st, FileAction::SaveAttach(st.aform.clone()));
    }
}

fn draw_passkeys_tab(ui: &mut egui::Ui, th: &Theme, st: &mut UiState, passkeys: &Passkeys) {
    match st.passkey_view.clone() {
        Sub::List => draw_passkey_list(ui, th, st, passkeys),
        Sub::Form => draw_passkey_form(ui, th, st),
        Sub::ConfirmDelete(name) => {
            if let Some(act) = draw_confirm_delete(
                ui,
                th,
                t("remote_tool.noun_passkey"),
                &name,
                st.kerr
                    .as_deref()
                    .or(Some(t("remote_tool.passkey_delete_hint"))),
                st.pending.load(Ordering::Relaxed) != 0,
            ) {
                if act {
                    enqueue(ui.ctx(), st, FileAction::DeletePasskey(name.clone()));
                } else {
                    st.passkey_view = Sub::List;
                }
            }
        }
    }
}

fn draw_passkey_list(ui: &mut egui::Ui, th: &Theme, st: &mut UiState, passkeys: &Passkeys) {
    if add_button(ui, th, t("remote_tool.passkey_add")).clicked() {
        reveal::begin_form(st);
        st.kform = PasskeyForm {
            kind: "path".into(),
            ..Default::default()
        };
        st.kerr = None;
        st.passkey_view = Sub::Form;
        return;
    }
    ui.add_space(th.spacing_xs.value());
    if let Some(error) = &st.kerr {
        selectable_label(
            ui,
            error,
            th.accent_danger_raised(),
            th.font_size_caption.value(),
            false,
        );
    }
    if passkeys.passkeys.is_empty() {
        ui.vertical_centered(|ui| {
            ui.add_space(th.spacing_lg.value());
            selectable_text(
                ui,
                t("remote_tool.passkey_empty"),
                th.text_muted(),
                th.font_size_body.value(),
                false,
                true,
                TextWrap::None,
            );
        });
        return;
    }
    let mut action: Option<(String, PasskeyRowAction)> = None;
    scroll_list_with_fade(ui, th, |ui| {
        for k in &passkeys.passkeys {
            let revealed = st.revealed.contains(&k.name);
            if let Some(a) = draw_passkey_row(
                ui,
                th,
                k,
                revealed,
                st.revealed_values
                    .get(&k.name)
                    .filter(|(key, _)| key == k)
                    .map(|(_, value)| value.as_str()),
            ) {
                action = Some((k.name.clone(), a));
            }
        }
    });
    if let Some((name, a)) = action {
        match a {
            PasskeyRowAction::Reveal => {
                if st.revealed.contains(&name) {
                    st.revealed.remove(&name);
                    st.revealed_values.remove(&name);
                    st.reveal_owners.remove(&name);
                } else if let Some(key) = passkeys.get(&name) {
                    st.reveal_owners.insert(name.clone(), Arc::default());
                    st.revealed.insert(name);
                    st.kerr = None;
                    enqueue(
                        ui.ctx(),
                        st,
                        FileAction::Reveal {
                            key: key.clone(),
                            editing: false,
                        },
                    );
                }
            }
            PasskeyRowAction::Edit => {
                if let Some(k) = passkeys.get(&name) {
                    reveal::begin_form(st);
                    st.kform = PasskeyForm {
                        name: k.name.clone(),
                        kind: k.kind.clone(),
                        value: String::new(),
                        editing_original: Some(k.name.clone()),
                    };
                    st.kerr = None;
                    st.passkey_view = Sub::Form;
                    enqueue(
                        ui.ctx(),
                        st,
                        FileAction::Reveal {
                            key: k.clone(),
                            editing: true,
                        },
                    );
                }
            }
            PasskeyRowAction::Delete => {
                st.passkey_view = Sub::ConfirmDelete(name);
            }
        }
    }
}

enum PasskeyRowAction {
    Reveal,
    Edit,
    Delete,
}

fn draw_passkey_row(
    ui: &mut egui::Ui,
    th: &Theme,
    k: &Passkey,
    revealed: bool,
    revealed_value: Option<&str>,
) -> Option<PasskeyRowAction> {
    let mut out = None;
    let kind_chip = if KNOWN_PASSKEY_KINDS.contains(&k.kind.as_str()) {
        RemoteRowChip::TagMono(&k.kind)
    } else {
        RemoteRowChip::Warn {
            text: &k.kind,
            tooltip: t("remote_tool.kind_unknown_hint"),
        }
    };
    remote_list_row(
        ui,
        th,
        3,
        |ui, w| {
            remote_row_title(ui, th, &k.name, None, false, &[kind_chip], w);
            let val = if revealed {
                revealed_value.unwrap_or("••••••••").to_string()
            } else {
                "••••••••".into()
            };
            // 값 줄은 보일 때도 한 줄로 두고 끝을 말줄임한다.
            selectable_text(
                ui,
                &format!("{} · {}", k.kind, val),
                th.text_muted(),
                th.font_size_caption.value(),
                true,
                false,
                TextWrap::Truncate(w),
            );
        },
        |ui| {
            if row_icon_button(ui, th, icons::TRASH, true)
                .on_hover_text(t("remote_tool.delete"))
                .clicked()
            {
                out = Some(PasskeyRowAction::Delete);
            }
            if row_icon_button(ui, th, icons::EDIT, true)
                .on_hover_text(t("remote_tool.edit"))
                .clicked()
            {
                out = Some(PasskeyRowAction::Edit);
            }
            // 값이 보이는 동안은 active 상태로 그리고 글리프를 eyeOff 로 바꾼다.
            let reveal_icon = if revealed { icons::EYE_OFF } else { icons::EYE };
            if IconButton::new()
                .variant(IconButtonVariant::Ghost)
                .size(ControlSize::Sm)
                .active(revealed)
                .show(ui, th, &|ui, rect, c| {
                    reveal_icon.image(rect.height(), c).paint_at(ui, rect)
                })
                .on_hover_text(t("remote_tool.reveal_tooltip"))
                .clicked()
            {
                out = Some(PasskeyRowAction::Reveal);
            }
        },
    );
    out
}

/// 로컬 GUI 전용 값 노출. path kind 는 경로, inline kind 는 관리 파일 내용을 읽는다.
fn draw_passkey_form(ui: &mut egui::Ui, th: &Theme, st: &mut UiState) {
    let (do_save, do_cancel) = edit_form(
        ui,
        th,
        "remote_tool.passkey_footer",
        st.pending.load(Ordering::Relaxed) != 0,
        |ui| {
            let editing = st.kform.editing_original.is_some();
            selectable_label(
                ui,
                if editing {
                    t("remote_tool.passkey_form_edit")
                } else {
                    t("remote_tool.passkey_form_add")
                },
                th.text_primary(),
                th.font_size_body.value(),
                false,
            );
            ui.add_space(th.spacing_md.value());

            let f = &mut st.kform;
            ui.spacing_mut().item_spacing.y = th.spacing_sm.value();
            let mut changed = text_row(ui, th, t("remote_tool.field_name"), &mut f.name, "", false);
            form_row(ui, th, t("remote_tool.field_kind"), |ui| {
                for opt in KNOWN_PASSKEY_KINDS {
                    changed |= ui
                        .selectable_value(&mut f.kind, (*opt).to_string(), *opt)
                        .changed();
                }
            });
            form_row(ui, th, t("remote_tool.field_value"), |ui| {
                if f.kind == "inline" {
                    changed |= ui
                        .add(
                            egui::TextEdit::multiline(&mut f.value)
                                .desired_rows(3)
                                .hint_text(tasty_egui_theme::hint_text(
                                    th,
                                    t("remote_tool.value_inline_hint"),
                                )),
                        )
                        .changed();
                } else {
                    changed |= ui
                        .add(
                            egui::TextEdit::singleline(&mut f.value)
                                .desired_width(f32::INFINITY)
                                .hint_text(tasty_egui_theme::hint_text(th, "~/.ssh/id_ed25519")),
                        )
                        .changed();
                }
            });
            if changed {
                st.key_edit.store(true, Ordering::Relaxed);
            }
            selectable_label(
                ui,
                t("remote_tool.passkey_value_note"),
                th.text_muted(),
                th.font_size_caption.value(),
                false,
            );
            if let Some(err) = &st.kerr {
                selectable_label(
                    ui,
                    err,
                    th.accent_danger_raised(),
                    th.font_size_caption.value(),
                    false,
                );
            }
        },
    );

    if do_cancel {
        st.kerr = None;
        st.passkey_view = Sub::List;
        return;
    }
    if do_save {
        enqueue(ui.ctx(), st, FileAction::SavePasskey(st.kform.clone()));
    }
}

fn draw_confirm_delete(
    ui: &mut egui::Ui,
    th: &Theme,
    noun: &str,
    name: &str,
    hint: Option<&str>,
    pending: bool,
) -> Option<bool> {
    let mut out = None;
    ui.add_space(th.spacing_sm.value());
    selectable_label(
        ui,
        &format!("{}: \"{name}\"?", noun),
        th.text_primary(),
        th.font_size_body.value(),
        false,
    );
    if let Some(h) = hint {
        selectable_label(ui, h, th.text_muted(), th.font_size_caption.value(), false);
    }
    ui.add_space(th.spacing_sm.value());
    ui.horizontal(|ui| {
        if ui
            .add_enabled(
                !pending,
                egui::Button::new(
                    egui::RichText::new(t("remote_tool.delete")).color(th.accent_danger_raised()),
                ),
            )
            .clicked()
        {
            out = Some(true);
        }
        if ui.button(t("remote_tool.cancel")).clicked() {
            out = Some(false);
        }
    });
    out
}

/// 호스트·사용자·포트 입력 행.
fn connection_rows(
    ui: &mut egui::Ui,
    th: &Theme,
    host: &mut String,
    user: &mut String,
    port: &mut String,
) {
    text_row(ui, th, t("remote_tool.field_host"), host, "10.0.4.12", true);
    text_row(ui, th, t("remote_tool.field_user"), user, "deploy", false);
    text_row(ui, th, t("remote_tool.field_port"), port, "22", true);
}

/// 원격 셸을 고르는 행.
fn shell_row(ui: &mut egui::Ui, th: &Theme, salt: &'static str, shell: &mut String) {
    form_row(ui, th, t("remote_tool.field_shell"), |ui| {
        tasty_egui_theme::with_popover_frame(ui, th, |ui| {
            egui::ComboBox::from_id_salt(salt)
                .selected_text(shell.clone())
                .width(ui.available_width())
                .show_ui(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    for sh in SHELLS {
                        tasty_ui_widgets::menu_option_value(ui, th, shell, (*sh).to_string(), sh);
                    }
                })
        });
    });
}

/// 입력의 무한 폭이 라벨을 밀어내지 않도록 라벨 고정 폭과 남은 입력 폭을 직접 나눈다.
fn form_row(ui: &mut egui::Ui, th: &Theme, label: &str, add_input: impl FnOnce(&mut egui::Ui)) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = th.spacing_md.value();
        field_label(ui, th, label);
        add_input(ui);
    });
}

/// 입력 행. placeholder는 host·port·경로 등의 기술 예시이며 번역하지 않는다.
fn text_row(
    ui: &mut egui::Ui,
    th: &Theme,
    label: &str,
    value: &mut String,
    placeholder: &str,
    mono: bool,
) -> bool {
    let mut changed = false;
    form_row(ui, th, label, |ui| {
        let mut edit = egui::TextEdit::singleline(value)
            .desired_width(f32::INFINITY)
            .hint_text(tasty_egui_theme::hint_text(th, placeholder));
        if mono {
            edit = edit.font(egui::TextStyle::Monospace);
        }
        changed = ui.add(edit).changed();
    });
    changed
}

/// 디자인 ProfileForm 의 라벨 컬럼 폭 (LogicalPx). grid `gridTemplateColumns: 112px 1fr`.
/// 화면전용 고정값이라 `Theme` 상수가 아니지만 길이이므로 `LogicalPx` 로 둔다.
const LABEL_COL_WIDTH: LogicalPx = LogicalPx(112.0);
/// hint/error 문구를 입력 컬럼에 맞춰 들여쓸 폭 = 라벨 컬럼(112) + columnGap(12).
/// 디자인 `marginLeft: 124px`.
const HINT_INDENT: LogicalPx = LABEL_COL_WIDTH.plus(LogicalPx(12.0));

// 아래 글자는 공용 selectable_text(선택·복사만 되는 표시 텍스트)로 그린다.
// RichText에서 스타일을 다시 꺼낼 수 없어 스타일 값을 별도 인자로 받는다.

/// 공통 고정 폭으로 오른쪽 정렬하는 폼 라벨.
fn field_label(ui: &mut egui::Ui, th: &Theme, label: &str) {
    ui.allocate_ui_with_layout(
        egui::vec2(LABEL_COL_WIDTH.value(), ui.spacing().interact_size.y),
        egui::Layout::right_to_left(egui::Align::Center),
        |ui| {
            selectable_text(
                ui,
                label,
                th.text_muted(),
                th.font_size_body.value(),
                false,
                false,
                TextWrap::Truncate(LABEL_COL_WIDTH.value()),
            );
        },
    );
}

/// 안내·오류를 입력 칸의 시작 위치에 맞춰 표시한다.
fn indented_hint(
    ui: &mut egui::Ui,
    th: &Theme,
    text: &str,
    color: impl Into<egui::Color32>,
    italic: bool,
) {
    ui.horizontal(|ui| {
        ui.add_space(HINT_INDENT.value());
        selectable_text(
            ui,
            text,
            color,
            th.font_size_caption.value(),
            false,
            italic,
            TextWrap::Wrap,
        );
    });
}

pub(crate) fn accept_detection(
    ctx: &egui::Context,
    update: &tasty_remote::profile_detection::DetectionUpdate,
) -> bool {
    let mut state = read_ui(ctx);
    if !state
        .detecting
        .as_ref()
        .is_some_and(|job| job.id == update.id)
    {
        return false;
    }
    state.detecting = None;
    write_ui(ctx, state);
    ctx.request_repaint();
    true
}

#[cfg(test)]
// 테스트는 의도적으로 무시하는 결과가 많아 let _ 사유 검사에서 제외한다.
#[allow(clippy::let_underscore_must_use)]
mod tests {
    use super::*;

    fn prof(name: &str, kind: &str) -> RemoteProfile {
        RemoteProfile::new(name, kind)
    }

    #[test]
    fn on_close_clears_ui_state_but_preserves_filter() {
        let ctx = egui::Context::default();
        write_ui(
            &ctx,
            UiState {
                tab: Tab::Attach,
                ..Default::default()
            },
        );
        write_filter(&ctx, ["ssh".to_string()].into_iter().collect());

        let (mut state, mut engine_session) = crate::state::tests::test_state();
        let engine = engine_session.borrow_mut();
        on_close_remote_tool_popup(&ctx, &mut state, &engine.read());

        assert!(
            ctx.memory(|m| m.data.get_temp::<UiState>(egui::Id::new(UI_MEMORY_ID)))
                .is_none()
        );
        assert_eq!(read_filter(&ctx), ["ssh".to_string()].into_iter().collect());
    }

    #[test]
    fn protocol_set_known_first_then_alpha() {
        let ps = vec![
            prof("a", "http"),
            prof("b", "zeta"),
            prof("c", "ssh"),
            prof("d", "smb"),
            prof("e", "ssh"), // 중복
            prof("f", "alpha"),
        ];
        assert_eq!(
            protocol_set(&ps),
            vec!["ssh", "smb", "http", "alpha", "zeta"]
        );
    }

    #[test]
    fn protocol_set_excludes_attach_kind() {
        let ps = vec![prof("a", "ssh"), prof("b", "tasty-attach")];
        assert_eq!(protocol_set(&ps), vec!["ssh"]);
    }

    #[test]
    fn protocol_set_dedup_and_skip_blank() {
        let ps = vec![prof("a", "ssh"), prof("b", "ssh"), prof("c", "  ")];
        assert_eq!(protocol_set(&ps), vec!["ssh"]);
    }

    #[test]
    fn filter_excludes_hidden_kinds() {
        let ps = [prof("a", "ssh"), prof("b", "smb"), prof("c", "http")];
        let none: HashSet<String> = HashSet::new();
        let all: Vec<&str> = ps
            .iter()
            .filter(|p| !none.contains(p.kind.trim()))
            .map(|p| p.name.as_str())
            .collect();
        assert_eq!(all, vec!["a", "b", "c"]);
        let hidden: HashSet<String> = ["ssh".to_string()].into_iter().collect();
        let vis: Vec<&str> = ps
            .iter()
            .filter(|p| !hidden.contains(p.kind.trim()))
            .map(|p| p.name.as_str())
            .collect();
        assert_eq!(vis, vec!["b", "c"]);
    }

    #[test]
    fn new_kind_visible_by_default() {
        let p = prof("x", "ftp");
        let hidden: HashSet<String> = ["ssh".to_string()].into_iter().collect();
        assert!(!hidden.contains(p.kind.trim()));
    }

    fn host(alias: &str, hostname: Option<&str>, port: Option<u16>) -> SshConfigHost {
        SshConfigHost {
            alias: alias.into(),
            source: std::path::PathBuf::from("/home/u/.ssh/config"),
            hostname: hostname.map(str::to_string),
            user: Some("zilhak".into()),
            port,
        }
    }

    fn cache(hosts: Vec<SshConfigHost>) -> LocalSshCache {
        LocalSshCache {
            hosts,
            path: "~/.ssh/config".into(),
            exists: true,
            unreadable: false,
        }
    }

    /// 실제 그린 텍스트를 모아 로컬 섹션 표시를 검사한다.
    fn painted_text(profiles: &RemoteProfiles, st: &mut UiState, hidden: &[&str]) -> Vec<String> {
        let ctx = egui::Context::default();
        write_filter(
            &ctx,
            hidden.iter().map(|s| s.to_string()).collect::<HashSet<_>>(),
        );
        let th = theme::theme();
        let passkeys = Passkeys::default();
        let mut out = Vec::new();
        // 첫 프레임은 폰트/레이아웃이 확정되지 않아 galley 가 비는 경우가 있어 두 번 돈다.
        for _ in 0..2 {
            out.clear();
            let full = ctx.run(egui::RawInput::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    draw_profile_list(ui, &th, st, profiles, &passkeys);
                });
            });
            for shape in full.shapes.iter() {
                collect_text(&shape.shape, &mut out);
            }
        }
        out
    }

    fn collect_text(shape: &egui::epaint::Shape, out: &mut Vec<String>) {
        match shape {
            egui::epaint::Shape::Text(t) => out.push(t.galley.text().to_string()),
            egui::epaint::Shape::Vec(v) => {
                for s in v {
                    collect_text(s, out);
                }
            }
            _ => {}
        }
    }

    /// 헤드리스로 목록 스크롤 영역을 한 프레임 그리고, 칠해진 Mesh 개수를 센다.
    /// 가장자리 페이드는 이 파일에서 Mesh 를 쓰는 유일한 지점이라 개수가 곧 페이드 수다.
    fn painted_fades(view_h: f32, rows: usize) -> usize {
        let ctx = egui::Context::default();
        let th = theme::theme();
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(400.0, view_h),
            )),
            ..Default::default()
        };
        let full = ctx.run(input, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                scroll_list_with_fade(ui, &th, |ui| {
                    for i in 0..rows {
                        ui.label(format!("row {i}"));
                    }
                });
            });
        });
        let mut n = 0;
        for shape in full.shapes.iter() {
            count_mesh(&shape.shape, &mut n);
        }
        n
    }

    fn count_mesh(shape: &egui::epaint::Shape, n: &mut usize) {
        match shape {
            egui::epaint::Shape::Mesh(_) => *n += 1,
            egui::epaint::Shape::Vec(v) => {
                for s in v {
                    count_mesh(s, n);
                }
            }
            _ => {}
        }
    }

    #[test]
    fn fade_edges_none_when_content_fits() {
        assert_eq!(fade_edges(0.0, 100.0, 200.0), (false, false));
    }

    #[test]
    fn fade_edges_bottom_only_at_top_of_overflowing_list() {
        assert_eq!(fade_edges(0.0, 900.0, 200.0), (false, true));
    }

    #[test]
    fn fade_edges_both_when_scrolled_to_middle() {
        assert_eq!(fade_edges(350.0, 900.0, 200.0), (true, true));
    }

    #[test]
    fn fade_edges_top_only_at_bottom_of_list() {
        assert_eq!(fade_edges(700.0, 900.0, 200.0), (true, false));
    }

    #[test]
    fn fade_edges_keeps_both_just_inside_the_epsilon_band() {
        assert_eq!(fade_edges(698.5, 900.0, 200.0), (true, true));
        assert_eq!(fade_edges(1.5, 900.0, 200.0), (true, true));
    }

    #[test]
    fn fade_height_is_capped_at_half_the_viewport() {
        assert_eq!(fade_height(24.0, 400.0), 24.0);
        assert_eq!(fade_height(24.0, 40.0), 20.0);
        assert_eq!(fade_height(24.0, 0.0), 0.0);
    }

    #[test]
    fn fade_edges_tolerates_subpixel_overshoot() {
        assert_eq!(fade_edges(699.6, 900.0, 200.0), (true, false));
    }

    #[test]
    fn short_list_paints_no_fade() {
        assert_eq!(painted_fades(400.0, 2), 0);
    }

    #[test]
    fn overflowing_list_paints_bottom_fade() {
        assert_eq!(painted_fades(120.0, 40), 1);
    }

    #[test]
    fn import_prefill_puts_alias_in_host_and_leaves_user_port_empty() {
        let f = import_prefill("gx10");
        assert_eq!(f.kind, "ssh");
        assert_eq!(f.name, "gx10"); // 기본값 = alias (폼에서 바꿀 수 있다)
        assert_eq!(f.host, "gx10"); // alias 그대로 — 값 펼치기 없음
        assert_eq!(f.shell, "auto");
        assert!(f.user.is_empty());
        assert!(f.port.is_empty());
        assert!(f.label.is_empty());
        assert!(f.passkey_ref.is_empty());
        assert!(f.editing_original.is_none());
    }

    #[test]
    fn already_imported_alias_is_detected_by_host_field() {
        let mut profiles = RemoteProfiles::default();
        profiles.upsert(RemoteProfile::new("my-gpu", "ssh").with_field("host", "gx10"));
        profiles.upsert(RemoteProfile::new("other", "smb").with_field("host", "bastion"));
        assert_eq!(imported_as(&profiles, "gx10"), Some("my-gpu"));
        assert_eq!(imported_as(&profiles, "bastion"), None);
    }

    #[test]
    fn local_target_hint_falls_back_to_alias_and_dash() {
        let hint = |hostname: Option<&str>, user: Option<&str>, port: Option<u16>| {
            local_target_hint(&SshConfigHost {
                alias: "gx10".into(),
                source: std::path::PathBuf::from("/home/u/.ssh/config"),
                hostname: hostname.map(str::to_string),
                user: user.map(str::to_string),
                port,
            })
        };
        assert_eq!(
            hint(Some("10.0.0.5"), Some("maya"), Some(2200)),
            "maya@10.0.0.5:2200"
        );
        assert_eq!(hint(Some("10.0.0.5"), None, Some(2200)), "10.0.0.5:2200");
        assert_eq!(hint(Some("10.0.0.5"), Some("maya"), None), "maya@10.0.0.5");
        assert_eq!(hint(Some("10.0.0.5"), None, None), "10.0.0.5");
        assert_eq!(hint(None, Some("maya"), Some(22)), "maya@gx10:22");
        assert_eq!(hint(None, None, Some(22)), "gx10:22");
        assert_eq!(hint(None, Some("maya"), None), "maya@gx10");
        assert_eq!(hint(None, None, None), "—");
    }

    #[test]
    fn profile_empty_key_distinguishes_no_profiles_from_filtered_out() {
        assert_eq!(
            profile_empty_key(false, false),
            Some("remote_tool.profile_empty")
        );
        assert_eq!(
            profile_empty_key(true, false),
            Some("remote_tool.profile_filter_empty")
        );
        assert_eq!(profile_empty_key(true, true), None);
    }

    #[test]
    fn protocol_filter_does_not_hide_local_section() {
        // 다른 시험의 전역 번역 초기화와 경쟁하지 않도록 그리기 전에 초기화한다.
        crate::i18n::init("en");
        let mut profiles = RemoteProfiles::default();
        profiles.upsert(RemoteProfile::new("my-gpu", "ssh").with_field("host", "other"));
        let mut st = UiState {
            local: Some(cache(vec![host("gx10", Some("10.0.0.5"), Some(2200))])),
            ..Default::default()
        };
        let texts = painted_text(&profiles, &mut st, &["ssh"]);
        assert!(
            texts.iter().any(|s| s.contains("gx10")),
            "필터가 로컬 섹션까지 가렸다: {texts:?}"
        );
        assert!(
            texts
                .iter()
                .any(|s| s == t("remote_tool.profile_filter_empty")),
            "필터 빈 상태 문구가 없다: {texts:?}"
        );
    }

    #[test]
    fn empty_profiles_still_renders_local_section() {
        // 다른 시험의 전역 번역 초기화와 경쟁하지 않도록 그리기 전에 초기화한다.
        crate::i18n::init("en");
        let profiles = RemoteProfiles::default();
        let mut st = UiState {
            local: Some(cache(vec![host("gx10", Some("10.0.0.5"), Some(2200))])),
            ..Default::default()
        };
        let texts = painted_text(&profiles, &mut st, &[]);
        assert!(
            texts.iter().any(|s| s.contains("gx10")),
            "프로필 0 건에서 early return 해 로컬 섹션이 사라졌다: {texts:?}"
        );
        assert!(
            texts
                .iter()
                .any(|s| *s == t("remote_tool.local_ssh_heading").to_uppercase()),
            "섹션 헤더가 없다: {texts:?}"
        );
    }

    #[test]
    fn local_section_shows_empty_note_instead_of_hiding() {
        // 다른 시험의 전역 번역 초기화와 경쟁하지 않도록 그리기 전에 초기화한다.
        crate::i18n::init("en");
        let profiles = RemoteProfiles::default();
        let mut st = UiState {
            local: Some(LocalSshCache {
                hosts: Vec::new(),
                path: "~/.ssh/config".into(),
                exists: false,
                unreadable: false,
            }),
            ..Default::default()
        };
        let texts = painted_text(&profiles, &mut st, &[]);
        assert!(
            texts
                .iter()
                .any(|s| s == t("remote_tool.local_ssh_missing")),
            "{texts:?}"
        );
    }

    #[test]
    fn local_section_distinguishes_unreadable_config_from_empty_one() {
        // 다른 시험의 전역 번역 초기화와 경쟁하지 않도록 그리기 전에 초기화한다.
        crate::i18n::init("en");
        let profiles = RemoteProfiles::default();
        let mut st = UiState {
            local: Some(LocalSshCache {
                hosts: Vec::new(),
                path: "~/.ssh/config".into(),
                exists: true,
                unreadable: true,
            }),
            ..Default::default()
        };
        let texts = painted_text(&profiles, &mut st, &[]);
        assert!(
            texts
                .iter()
                .any(|s| s == t("remote_tool.local_ssh_unreadable")),
            "{texts:?}"
        );
        assert!(
            !texts.iter().any(|s| s == t("remote_tool.local_ssh_empty")),
            "빈 설정 문구가 함께 떴다: {texts:?}"
        );
    }

    #[test]
    fn empty_key_covers_three_causes() {
        let base = LocalSshCache {
            hosts: Vec::new(),
            path: "~/.ssh/config".into(),
            exists: true,
            unreadable: false,
        };
        assert_eq!(
            local_ssh_empty_key(&LocalSshCache {
                exists: false,
                ..base.clone()
            }),
            "remote_tool.local_ssh_missing"
        );
        assert_eq!(
            local_ssh_empty_key(&LocalSshCache {
                unreadable: true,
                ..base.clone()
            }),
            "remote_tool.local_ssh_unreadable"
        );
        assert_eq!(local_ssh_empty_key(&base), "remote_tool.local_ssh_empty");
    }

    /// 권한 없는 실제 파일(mode 000)에서 캐시가 "있는데 못 읽음" 으로 떨어지는지.
    /// root 는 권한을 무시하므로 그 환경에서는 판정 자체가 성립하지 않아 건너뛴다.
    #[test]
    #[cfg(unix)]
    fn cache_marks_mode_000_config_unreadable() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("config");
        std::fs::write(&path, "Host a\n").expect("write fixture");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o000)).expect("chmod 000");

        if std::fs::File::open(&path).is_ok() {
            // root(또는 CAP_DAC_OVERRIDE) — mode 000 이어도 열린다.
            return;
        }
        let cache = local_ssh_cache_at(Some(path.clone()));
        assert!(cache.exists);
        assert!(cache.unreadable);
        assert_eq!(
            local_ssh_empty_key(&cache),
            "remote_tool.local_ssh_unreadable"
        );

        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).expect("restore");
        let cache = local_ssh_cache_at(Some(path));
        assert!(!cache.unreadable);
        assert_eq!(local_ssh_empty_key(&cache), "remote_tool.local_ssh_empty");
    }

    /// 경로 부재는 읽기 실패와 구분한다.
    #[test]
    fn cache_marks_absent_config_missing_not_unreadable() {
        let dir = tempfile::tempdir().expect("temp dir");
        let cache = local_ssh_cache_at(Some(dir.path().join("nope")));
        assert!(!cache.exists);
        assert!(!cache.unreadable);
        assert_eq!(local_ssh_empty_key(&cache), "remote_tool.local_ssh_missing");

        let cache = local_ssh_cache_at(None);
        assert!(!cache.exists);
        assert!(!cache.unreadable);
        assert_eq!(local_ssh_empty_key(&cache), "remote_tool.local_ssh_missing");
    }

    /// config 경로가 디렉터리이면 읽을 수 있는 빈 설정으로 표시하지 않는다.
    #[test]
    fn cache_marks_directory_config_unreadable() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("config");
        std::fs::create_dir(&path).expect("디렉토리 픽스처");

        let cache = local_ssh_cache_at(Some(path));
        assert!(cache.exists, "디렉토리도 존재는 한다");
        assert!(
            cache.unreadable,
            "디렉토리를 '읽히는 빈 설정' 으로 오독하면 안 된다"
        );
        assert_eq!(
            local_ssh_empty_key(&cache),
            "remote_tool.local_ssh_unreadable"
        );
    }

    /// UTF-8이 아닌 config를 호스트 0건의 빈 설정으로 표시하지 않는다.
    #[test]
    fn cache_marks_invalid_utf8_config_unreadable() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("config");
        std::fs::write(&path, b"Host a\n\xff\xfe\n").expect("write fixture");

        let cache = local_ssh_cache_at(Some(path));
        assert!(cache.exists);
        assert!(cache.unreadable, "디코드 실패는 읽기 실패다");
        assert_eq!(
            local_ssh_empty_key(&cache),
            "remote_tool.local_ssh_unreadable"
        );
    }

    #[test]
    fn unknown_kind_detection() {
        assert!(is_unknown_kind("ftp")); // 코어/known 모두 모름
        assert!(!is_unknown_kind("ssh")); // builtin
        assert!(!is_unknown_kind("smb")); // builtin
        assert!(!is_unknown_kind("http")); // KNOWN_TYPES
    }
}
