//! TCP 포트 조회. Tasty 프로세스 트리 또는 시스템 전체를 조회하고 상태·검색어로 거른다.
//! 처음 열기·범위 변경·새로고침 때 백그라운드 조회를 시작한다.
//! 행을 선택하면 주소를 복사할 수 있고 즐겨찾기는 별도 시스템 조회 결과와 함께 표시한다.
//! 화면 함수는 앱 상태 없이 입력을 받아 사용자 동작을 반환한다.

use crate::runtime::engine_read::EngineRead;
use std::collections::HashSet;
use std::net::IpAddr;
use std::sync::{Arc, Weak};
use tasty_type_geometry::length::LogicalPx;

use crate::adapters::ui::icons;
use crate::adapters::ui::popup::PopupAction;
#[cfg(test)]
use crate::core::port_favorites::PortFavorites;
use crate::core::state::SurfaceDisplayPath;
use crate::i18n::{t, t_count_template};
use crate::state::MainViewState;
use crate::theme;
use crate::theme::Theme;
use tasty_portscan::PortState;
use tasty_ui_widgets::tokens::{STRUCT_GAP_1, STRUCT_GAP_2, STRUCT_GAP_4, TAG_PILL_CORNER_RADIUS};
use tasty_ui_widgets::{
    Button, ButtonVariant, IconButton, IconButtonVariant, Input, PORTS_PANEL_PAD_X, PortsColumn,
    StatusKind, TableAlign, TableColumn, TableColumnWidth, TableSortDir, checkbox, hspace,
    in_selected_table_row, margin_sym, ports_favorite_detail_and_state, ports_process_cell,
    ports_star_column_width, ports_table, status_dot, table_cell_ink, vspace,
};

/// popup 좌우 안쪽 여백. 갤러리 예제와 같은 값을 쓰도록 공용 위젯 크레이트에 둔다.
const PANEL_PAD_X: i8 = PORTS_PANEL_PAD_X;
/// 푸터 상하 여백. 디자인 전사값 9 로 그리드 밖이다(`spacing_sm`=8 과 1px 차).
const FOOTER_PAD_Y: i8 = 9;

pub const PORT_SCANNER_POPUP_ID: &str = "port_scanner";

/// 조회 범위: Tasty 프로세스 트리 또는 호스트 전체 TCP 소켓.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScanScope {
    Tasty,
    System,
}

/// Which workspace/tab a row belongs to. `External` means the listening
/// process is not part of any Tasty shell tree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SourceTag {
    Tasty {
        workspace_name: String,
        tab_name: Option<String>,
    },
    External,
}

/// 소속 워크스페이스·탭 정보를 포함한 포트 행.
#[derive(Clone, Debug)]
pub struct PortRowView {
    pub port: u16,
    pub addr_display: String,
    pub pid: Option<u32>,
    pub process_name: Option<String>,
    pub source: SourceTag,
    /// TCP connection state, drives the STATE column dot color/pulse + label.
    pub state: PortState,
    /// Whether `(addr_display, port)` is in `engine.port_favorites`. Filled by
    /// the wrapper after the background scan returns (`run_scan` has no
    /// `CoreState` access) — always `false` fresh off the scan thread.
    pub favorited: bool,
}

/// 즐겨찾기 주소와 현재 시스템 조회 결과. 일치 항목이 없으면 NONE으로 표시한다.
#[derive(Clone, Debug)]
pub struct FavoriteRowView {
    pub addr_display: String,
    pub port: u16,
    pub matched: Option<FavoriteMatch>,
}

/// The system-wide scan row a favorite currently matches, projected down to
/// what the favorites row needs (full `PortRowView` carries scope/source
/// details the summary row doesn't show).
#[derive(Clone, Debug)]
pub struct FavoriteMatch {
    pub process_name: Option<String>,
    pub pid: Option<u32>,
    /// `Some` only when the matched socket belongs to a Tasty process tree.
    pub workspace_name: Option<String>,
    pub state: PortState,
}

/// 비동기 조회 상태. 내부 Close 버튼은 Idle로 초기화하지만 바깥 클릭으로 닫으면 유지한다.
pub enum PortScanState {
    Idle,
    Loading {
        ticket: Arc<()>,
        request: Option<ScanSnapshot>,
        scope: ScanScope,
    },
    Ready {
        rows: Vec<PortRowView>,
        scope: ScanScope,
    },
    Failed(String),
}

/// Send-safe snapshot consumed by the background scan worker. Built on the
/// main thread from the current workspace tree; the worker reads it without
/// touching `CoreState`.
pub struct ScanSnapshot {
    pub surfaces: Vec<(u32, u32, SurfaceDisplayPath)>,
    pub show_all_system: bool,
}

/// 정렬 열. Proto와 State는 정렬하지 않는다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SortKey {
    Port,
    Address,
    Process,
    Workspace,
    Tab,
}

/// 정렬 방향.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SortDir {
    Asc,
    Desc,
}

/// One of the seven table columns. Drives column visibility (chooser) and width
/// computation. The order here is the table's left-to-right order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColumnId {
    Port,
    Proto,
    Address,
    Process,
    Workspace,
    Tab,
    State,
}

impl ColumnId {
    /// Left-to-right table order. Also the bit order of [`ColumnVisibility`].
    pub const ALL: [ColumnId; 7] = [
        ColumnId::Port,
        ColumnId::Proto,
        ColumnId::Address,
        ColumnId::Process,
        ColumnId::Workspace,
        ColumnId::Tab,
        ColumnId::State,
    ];

    /// Bit index in [`ColumnVisibility`].
    fn bit(self) -> u8 {
        match self {
            ColumnId::Port => 0,
            ColumnId::Proto => 1,
            ColumnId::Address => 2,
            ColumnId::Process => 3,
            ColumnId::Workspace => 4,
            ColumnId::Tab => 5,
            ColumnId::State => 6,
        }
    }

    /// The shared ports table column with the same meaning.
    fn shared(self) -> PortsColumn {
        match self {
            ColumnId::Port => PortsColumn::Port,
            ColumnId::Proto => PortsColumn::Proto,
            ColumnId::Address => PortsColumn::Address,
            ColumnId::Process => PortsColumn::Process,
            ColumnId::Workspace => PortsColumn::Workspace,
            ColumnId::Tab => PortsColumn::Tab,
            ColumnId::State => PortsColumn::State,
        }
    }

    /// Port is the identity / default sort column and is always shown (its
    /// chooser checkbox is locked). This also guarantees ≥1 visible column, so
    /// no extra "can't hide the last column" rule is needed.
    fn mandatory(self) -> bool {
        matches!(self, ColumnId::Port)
    }
}

/// Per-column show/hide state, persisted in [`FilterState`]. Bit `i` set =
/// column `ColumnId::ALL[i]` visible. `Default` shows every column.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ColumnVisibility(u8);

impl Default for ColumnVisibility {
    fn default() -> Self {
        Self(0b0111_1111)
    }
}

impl ColumnVisibility {
    /// Whether `col` is currently shown. Mandatory columns are always shown.
    pub fn is_visible(self, col: ColumnId) -> bool {
        col.mandatory() || (self.0 & (1 << col.bit())) != 0
    }

    /// Show/hide `col`. Mandatory columns ignore a hide request (stay shown).
    pub fn set(&mut self, col: ColumnId, visible: bool) {
        if col.mandatory() {
            self.0 |= 1 << col.bit();
            return;
        }
        if visible {
            self.0 |= 1 << col.bit();
        } else {
            self.0 &= !(1 << col.bit());
        }
    }
}

/// User-controlled view state, persisted in `egui::Memory` across frames
/// (Id: `"port_scanner.filter"`).
#[derive(Clone, Debug, PartialEq)]
pub struct FilterState {
    pub show_all_system: bool,
    pub query: String,
    pub sort_key: SortKey,
    pub sort_dir: SortDir,
    /// Port number of the currently selected row (design `selectedKey`), or
    /// `None` when nothing is selected. Drives the row highlight + footer
    /// "Copy address" enablement.
    pub selected_port: Option<u16>,
    /// Per-column show/hide state (column chooser). Default shows all columns.
    pub columns: ColumnVisibility,
    /// TCP states to **show** in the table (a shown set, not a hidden set).
    /// Default is `{Listen}` (LISTEN-only) — the backend scans every TCP state,
    /// but the viewer surfaces only listening sockets until the user widens it.
    /// A shown set keeps the default a single entry and stays robust as new
    /// `PortState` variants are added (a hidden set would have to enumerate all
    /// the others).
    pub visible_states: HashSet<PortState>,
}

impl Default for FilterState {
    fn default() -> Self {
        Self {
            show_all_system: false,
            query: String::new(),
            sort_key: SortKey::Port,
            sort_dir: SortDir::Asc,
            selected_port: None,
            columns: ColumnVisibility::default(),
            visible_states: HashSet::from([PortState::Listen]),
        }
    }
}

/// 조회 상태와 표시 행. total은 상태 필터 적용 후·검색 전 개수이며
/// listening은 상태 필터·검색과 무관한 조회 범위 전체의 LISTEN 개수다.
#[derive(Clone, Copy)]
pub enum PortScannerViewState<'a> {
    Loading,
    Ready {
        rows: &'a [PortRowView],
        /// 상태 필터를 적용하고 검색은 적용하기 전의 개수.
        total: usize,
        /// Count of LISTEN-state ports in the current scope, before the search
        /// filter. Feeds the header tag (`{listening} listening`).
        listening: usize,
    },
    Failed {
        message: &'a str,
    },
}

/// Filter inputs handed to the view (read-only projection of [`FilterState`]).
pub struct PortScannerFilter<'a> {
    pub show_all_system: bool,
    pub query: &'a str,
    pub sort_key: SortKey,
    pub sort_dir: SortDir,
    /// Currently selected row's port (design `selectedKey`). `None` = no
    /// selection.
    pub selected_port: Option<u16>,
    /// Per-column show/hide state (column chooser projection).
    pub columns: ColumnVisibility,
    /// TCP states currently shown (shown set; default `{Listen}`). Drives the
    /// state-filter button's active styling + the dropdown checkbox seeding.
    pub visible_states: &'a HashSet<PortState>,
    /// True when the scope has rows but the state filter (not the search query)
    /// hid them all. Lets the empty body distinguish "no LISTEN ports" from
    /// "no ports at all".
    pub hidden_by_state: bool,
}

/// Pure inputs to `draw_port_scanner_view`. Contains no `MainViewState` /
/// `CoreState` — every value is read-only. All user-facing strings are
/// pre-resolved by the wrapper so the view is i18n-agnostic.
pub struct PortScannerProps<'a> {
    pub theme: &'a Theme,
    pub view_state: PortScannerViewState<'a>,
    pub filter: PortScannerFilter<'a>,
    /// Pinned favorites, rendered by the always-visible section above the
    /// table regardless of the table's scope/search/state filter. LISTEN/NONE
    /// judgment here is system-wide (see `matched` on each row).
    pub favorites: &'a [FavoriteRowView],
    /// TCP states present in the current scope (before any filter), for the
    /// state-filter dropdown list + select-all / apply intersection. LISTEN
    /// first, then `label()` alphabetical.
    pub present_states: &'a [PortState],
    /// When true, the LISTEN state dot is drawn static (no pulse ring).
    pub reduced_motion: bool,
    pub label_heading: &'a str,
    pub label_search_placeholder: &'a str,
    pub label_filter_show_all_system: &'a str,
    pub label_loading: &'a str,
    pub label_failed: &'a str,
    pub label_close: &'a str,
    pub label_refresh: &'a str,
    pub label_copy_address: &'a str,
    pub label_external_dash: &'a str,
    pub label_no_ports_tasty_empty: &'a str,
    pub label_no_ports_system_empty: &'a str,
    pub label_no_ports_search_zero: &'a str,
    pub label_footer_loading: &'a str,
    pub label_header_tag_scanning: &'a str,
    /// Format string with `{n}` placeholder, e.g. `"{n} listening"`.
    pub label_header_tag_count: &'a str,
    /// `{shown}`·`{total}` 자리를 받는 카운터. 호출자가 total 에 맞는 템플릿(단수 규칙)을 넘긴다.
    pub label_footer_counter: &'a str,
    pub label_column_port: &'a str,
    pub label_column_proto: &'a str,
    pub label_column_address: &'a str,
    pub label_column_process: &'a str,
    pub label_column_workspace: &'a str,
    pub label_column_tab: &'a str,
    pub label_column_state: &'a str,
    /// Column-chooser trigger tooltip (header IconButton).
    pub label_columns_button: &'a str,
    /// Column-chooser popup title.
    pub label_columns_menu_title: &'a str,
    /// State-filter button base label (e.g. `State`).
    pub label_state_filter: &'a str,
    /// State-filter dropdown title (e.g. `Filter by state`).
    pub label_state_filter_title: &'a str,
    /// State-filter dropdown "select all present states" action.
    pub label_state_filter_select_all: &'a str,
    /// State-filter dropdown "deselect all" action.
    pub label_state_filter_deselect_all: &'a str,
    /// State-filter dropdown "reset to LISTEN-only" action.
    pub label_state_filter_reset: &'a str,
    /// State-filter dropdown "apply draft" action.
    pub label_state_filter_apply: &'a str,
    /// Empty-body message when the state filter hid every scope row.
    pub label_no_ports_state_filtered: &'a str,
    /// Favorites section caption when the list is empty (no count suffix).
    pub label_favorites_heading: &'a str,
    /// Favorites section caption format string with `{n}`, e.g. `"Favorites · {n}"`.
    pub label_favorites_count: &'a str,
    /// Favorites section caption's right-aligned "system-wide" hint.
    pub label_favorites_system_wide: &'a str,
    /// Favorites section empty-state hint line.
    pub label_favorites_empty: &'a str,
    /// Favorites row detail text when the target has no system-wide match.
    pub label_favorites_not_running: &'a str,
    /// Status label for a favorite with no system-wide match (`StatusKind::Idle`).
    pub label_state_none: &'a str,
    /// Star tooltip format string with `{key}` (the `addr:port` string), shown
    /// when the row is not yet favorited.
    pub label_favorite_add: &'a str,
    /// Star tooltip format string with `{key}`, shown when the row is already
    /// favorited (click removes it).
    pub label_favorite_remove: &'a str,
    /// PID Tag 툴팁 형식 문자열(`{pid}`). Tag 는 kit 처럼 숫자만 보이므로 뜻은 호버로 알린다.
    pub label_pid_tooltip: &'a str,
}

/// 화면에서 요청한 동작. 호출부가 설정·클립보드·조회에 반영한다.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PortScannerAction {
    None,
    Close,
    Refresh,
    /// Row clicked → toggle selection of the given port (design `onRowClick`).
    Select(u16),
    /// Footer "Copy address" clicked → copy this address string to clipboard.
    CopyAddress(String),
    SetShowAllSystem(bool),
    SetQuery(String),
    /// Emitted by header click → toggle sort.
    SetSort(SortKey),
    /// Column chooser toggled a column's visibility.
    SetColumnVisible(ColumnId, bool),
    /// State filter Apply → replace the shown TCP-state set.
    SetVisibleStates(HashSet<PortState>),
    /// Star icon clicked (main table row or favorites section) → toggle
    /// favorite status for this `(addr, port)`. `addr` is the row's
    /// display-form address string (round-trips through `str::parse`).
    ToggleFavorite(String, u16),
}

const FILTER_MEMORY_ID: &str = "port_scanner.filter";
/// egui temp-memory id for the state-filter dropdown's editing buffer
/// (apply-on-confirm draft). Seeded from `visible_states` when the dropdown
/// opens; committed via `SetVisibleStates` on Apply.
const STATE_FILTER_DRAFT_ID: &str = "port_scanner.state_filter_draft";
/// egui popup id for the state-filter dropdown. Used both to open/toggle the
/// `popup_above_or_below_widget` and for the Escape guard's open check.
const STATE_FILTER_POPUP_ID: &str = "port_scanner.state_filter_popup";
/// egui popup id for the column-chooser dropdown (`draw_column_chooser`).
const COLUMN_CHOOSER_POPUP_ID: &str = "port_scanner.columns_chooser";

fn read_filter_state(ctx: &egui::Context) -> FilterState {
    ctx.memory(|mem| {
        mem.data
            .get_temp::<FilterState>(egui::Id::new(FILTER_MEMORY_ID))
            .unwrap_or_default()
    })
}

fn write_filter_state(ctx: &egui::Context, filter: FilterState) {
    ctx.memory_mut(|mem| {
        mem.data
            .insert_temp(egui::Id::new(FILTER_MEMORY_ID), filter);
    });
}

pub fn draw_port_scanner_popup(
    ui: &mut egui::Ui,
    state: &mut MainViewState,
    engine: &EngineRead<'_>,
) -> PopupAction {
    let th = theme::theme();
    let ctx = ui.ctx().clone();

    let mut filter_state = read_filter_state(&ctx);
    let target_show_all_system = filter_state.show_all_system;

    let need_kick = match &state.port_scan {
        PortScanState::Idle => true,
        PortScanState::Ready { scope, .. } => *scope != scope_from_flag(target_show_all_system),
        _ => false,
    };
    if need_kick {
        kick_off_scan(
            &mut state.port_scan,
            &state.navigation,
            &engine.as_ref(),
            &ctx,
            target_show_all_system,
        );
    }

    // 즐겨찾기는 메인 범위와 별개로 시스템 전체를 조회한다. 항목이 없으면 조회하지 않는다.
    let has_favorites = !engine.port_favorites.is_empty();
    if has_favorites && matches!(state.port_favorites_scan, PortScanState::Idle) {
        kick_off_scan(
            &mut state.port_favorites_scan,
            &state.navigation,
            &engine.as_ref(),
            &ctx,
            true,
        );
    }

    // 푸터 total은 상태 필터 적용 후·검색 전 개수다. 선택은 포트 번호로 유지한다.
    let (filtered_rows, state_total): (Vec<PortRowView>, usize) = match &state.port_scan {
        PortScanState::Ready { rows, .. } => {
            let state_rows: Vec<&PortRowView> = rows
                .iter()
                .filter(|r| filter_state.visible_states.contains(&r.state))
                .collect();
            let state_total = state_rows.len();
            let mut v: Vec<PortRowView> = state_rows
                .into_iter()
                .filter(|r| matches_query(r, &filter_state.query))
                .cloned()
                .collect();
            sort_rows(&mut v, filter_state.sort_key, filter_state.sort_dir);
            for row in &mut v {
                if let Ok(addr) = row.addr_display.parse::<IpAddr>() {
                    row.favorited = engine
                        .port_favorites
                        .iter()
                        .any(|favorite| favorite.addr == addr && favorite.port == row.port);
                }
            }
            (v, state_total)
        }
        _ => (Vec::new(), 0),
    };

    let favorite_system_rows: Option<&[PortRowView]> = match &state.port_favorites_scan {
        PortScanState::Ready { rows, .. } => Some(rows.as_slice()),
        _ => None,
    };
    let favorite_rows = build_favorite_rows(engine.port_favorites, favorite_system_rows);

    let present_states: Vec<PortState> = match &state.port_scan {
        PortScanState::Ready { rows, .. } => present_states(rows),
        _ => Vec::new(),
    };

    // 검색어가 없고 상태 필터가 모든 행을 가렸으면 포트 없음과 구분해 안내한다.
    let hidden_by_state = match &state.port_scan {
        PortScanState::Ready { rows, .. } => {
            !rows.is_empty() && filtered_rows.is_empty() && filter_state.query.trim().is_empty()
        }
        _ => false,
    };

    let view_state = match &state.port_scan {
        PortScanState::Idle | PortScanState::Loading { .. } => PortScannerViewState::Loading,
        // Scope counts are search-independent: `total` = state-filtered count
        // (state pass, before search — per design), `listening` = scope-wide
        // LISTEN-only count for the header tag (independent of the state filter).
        PortScanState::Ready { rows, .. } => PortScannerViewState::Ready {
            rows: &filtered_rows,
            total: state_total,
            listening: rows.iter().filter(|r| r.state.is_listen()).count(),
        },
        PortScanState::Failed(msg) => PortScannerViewState::Failed { message: msg },
    };

    let props = PortScannerProps {
        theme: &th,
        view_state,
        present_states: &present_states,
        reduced_motion: engine.settings.accessibility.reduced_motion,
        favorites: &favorite_rows,
        filter: PortScannerFilter {
            show_all_system: filter_state.show_all_system,
            query: &filter_state.query,
            sort_key: filter_state.sort_key,
            sort_dir: filter_state.sort_dir,
            selected_port: filter_state.selected_port,
            columns: filter_state.columns,
            visible_states: &filter_state.visible_states,
            hidden_by_state,
        },
        label_heading: t("port_scanner.heading"),
        label_search_placeholder: t("port_scanner.search_placeholder"),
        label_filter_show_all_system: t("port_scanner.filter_show_all_system"),
        label_loading: t("port_scanner.loading"),
        label_failed: t("port_scanner.failed_label"),
        label_close: t("port_scanner.close"),
        label_refresh: t("port_scanner.refresh"),
        label_copy_address: t("port_scanner.copy_address"),
        label_external_dash: t("port_scanner.external_dash"),
        label_no_ports_tasty_empty: t("port_scanner.no_ports_tasty_empty"),
        label_no_ports_system_empty: t("port_scanner.no_ports_system_empty"),
        label_no_ports_search_zero: t("port_scanner.no_ports_search_zero"),
        label_footer_loading: t("port_scanner.footer_loading"),
        label_header_tag_scanning: t("port_scanner.header_tag_scanning"),
        label_header_tag_count: t("port_scanner.header_tag_count"),
        label_footer_counter: t_count_template("port_scanner.footer_counter", state_total as u64),
        label_column_port: t("port_scanner.column_port"),
        label_column_proto: t("port_scanner.column_proto"),
        label_column_address: t("port_scanner.column_address"),
        label_column_process: t("port_scanner.column_process"),
        label_column_workspace: t("port_scanner.column_workspace"),
        label_column_tab: t("port_scanner.column_tab"),
        label_column_state: t("port_scanner.column_state"),
        label_columns_button: t("port_scanner.columns_button"),
        label_columns_menu_title: t("port_scanner.columns_menu_title"),
        label_state_filter: t("port_scanner.state_filter"),
        label_state_filter_title: t("port_scanner.state_filter_title"),
        label_state_filter_select_all: t("port_scanner.state_filter_select_all"),
        label_state_filter_deselect_all: t("port_scanner.state_filter_deselect_all"),
        label_state_filter_reset: t("port_scanner.state_filter_reset"),
        label_state_filter_apply: t("port_scanner.state_filter_apply"),
        label_no_ports_state_filtered: t("port_scanner.no_ports_state_filtered"),
        label_favorites_heading: t("port_scanner.favorites_heading"),
        label_favorites_count: t("port_scanner.favorites_count"),
        label_favorites_system_wide: t("port_scanner.favorites_system_wide"),
        label_favorites_empty: t("port_scanner.favorites_empty"),
        label_favorites_not_running: t("port_scanner.favorites_not_running"),
        label_state_none: t("port_scanner.state_none_label"),
        label_favorite_add: t("port_scanner.favorite_add_tooltip"),
        label_pid_tooltip: t("port_scanner.pid_tooltip"),
        label_favorite_remove: t("port_scanner.favorite_remove_tooltip"),
    };

    let action = draw_port_scanner_view(ui, &props);

    match action {
        PortScannerAction::None => PopupAction::None,
        PortScannerAction::Close => {
            // 요청 identity를 버려 App이 이전 스캔 결과를 새 popup에 적용하지 않게 한다.
            state.port_scan = PortScanState::Idle;
            state.port_favorites_scan = PortScanState::Idle;
            PopupAction::Close
        }
        PortScannerAction::Refresh => {
            kick_off_scan(
                &mut state.port_scan,
                &state.navigation,
                &engine.as_ref(),
                &ctx,
                target_show_all_system,
            );
            if has_favorites {
                kick_off_scan(
                    &mut state.port_favorites_scan,
                    &state.navigation,
                    &engine.as_ref(),
                    &ctx,
                    true,
                );
            }
            PopupAction::None
        }
        PortScannerAction::Select(port) => {
            filter_state.selected_port = if filter_state.selected_port == Some(port) {
                None
            } else {
                Some(port)
            };
            write_filter_state(&ctx, filter_state);
            PopupAction::None
        }
        PortScannerAction::CopyAddress(addr) => {
            ui.ctx().copy_text(addr);
            PopupAction::None
        }
        PortScannerAction::SetShowAllSystem(v) => {
            filter_state.show_all_system = v;
            write_filter_state(&ctx, filter_state);
            PopupAction::None
        }
        PortScannerAction::SetQuery(q) => {
            filter_state.query = q;
            write_filter_state(&ctx, filter_state);
            PopupAction::None
        }
        PortScannerAction::SetColumnVisible(col, visible) => {
            // 열을 숨겨도 정렬과 선택은 유지한다.
            filter_state.columns.set(col, visible);
            write_filter_state(&ctx, filter_state);
            PopupAction::None
        }
        PortScannerAction::SetSort(key) => {
            if filter_state.sort_key == key {
                filter_state.sort_dir = match filter_state.sort_dir {
                    SortDir::Asc => SortDir::Desc,
                    SortDir::Desc => SortDir::Asc,
                };
            } else {
                filter_state.sort_key = key;
                filter_state.sort_dir = SortDir::Asc;
            }
            write_filter_state(&ctx, filter_state);
            PopupAction::None
        }
        PortScannerAction::SetVisibleStates(set) => {
            // 필터는 egui 임시 메모리에 보관해 같은 실행 중 다시 열 때 유지한다.
            filter_state.visible_states = set;
            write_filter_state(&ctx, filter_state);
            PopupAction::None
        }
        PortScannerAction::ToggleFavorite(addr_display, port) => {
            if let Ok(addr) = addr_display.parse::<IpAddr>() {
                state.dispatch_intent(
                    crate::intent::Intent::Engine(
                        crate::app::engine_action::EngineAction::TogglePortFavorite {
                            address: addr,
                            port,
                            label: format_host_port(&addr_display, port),
                        },
                    )
                    .from_user_context_menu(),
                );
            }
            PopupAction::None
        }
    }
}

/// Sort rows in place by the given key + direction. `None` values are always
/// placed at the tail, regardless of direction (per design — None ≈ "no info,
/// don't surface").
pub fn sort_rows(rows: &mut [PortRowView], key: SortKey, dir: SortDir) {
    rows.sort_by(|a, b| compare_rows(key, dir, a, b));
}

/// Two-row comparator used by [`sort_rows`]. None-or-empty values for the
/// active key bubble to the tail regardless of `dir`.
pub fn compare_rows(
    key: SortKey,
    dir: SortDir,
    a: &PortRowView,
    b: &PortRowView,
) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    let (a_missing, b_missing) = (value_missing(a, key), value_missing(b, key));
    match (a_missing, b_missing) {
        (true, true) => return Ordering::Equal,
        (true, false) => return Ordering::Greater,
        (false, true) => return Ordering::Less,
        (false, false) => {}
    }
    let ord = match key {
        SortKey::Port => a.port.cmp(&b.port),
        SortKey::Address => a.addr_display.cmp(&b.addr_display),
        SortKey::Process => a
            .process_name
            .as_deref()
            .unwrap_or("")
            .cmp(b.process_name.as_deref().unwrap_or("")),
        SortKey::Workspace => workspace_name(a).cmp(workspace_name(b)),
        SortKey::Tab => tab_name(a).unwrap_or("").cmp(tab_name(b).unwrap_or("")),
    };
    match dir {
        SortDir::Asc => ord,
        SortDir::Desc => ord.reverse(),
    }
}

/// "Missing" for sort purposes: None pid / no process_name / External source
/// (for workspace/tab) / Tasty source with no tab_name (for tab only).
fn value_missing(row: &PortRowView, key: SortKey) -> bool {
    match key {
        SortKey::Port | SortKey::Address => false,
        SortKey::Process => row.process_name.is_none(),
        SortKey::Workspace => matches!(row.source, SourceTag::External),
        SortKey::Tab => tab_name(row).is_none(),
    }
}

fn workspace_name(row: &PortRowView) -> &str {
    match &row.source {
        SourceTag::Tasty { workspace_name, .. } => workspace_name.as_str(),
        SourceTag::External => "",
    }
}

fn tab_name(row: &PortRowView) -> Option<&str> {
    match &row.source {
        SourceTag::Tasty {
            tab_name: Some(t), ..
        } => Some(t.as_str()),
        _ => None,
    }
}

/// 포트·주소·PID·프로세스·워크스페이스·탭을 대소문자 없이 검색한다. 열 표시 여부와는 무관하다.
pub fn matches_query(row: &PortRowView, query: &str) -> bool {
    let q = query.trim();
    if q.is_empty() {
        return true;
    }
    let q_lower = q.to_lowercase();
    let port_str = row.port.to_string();
    let pid_str = row.pid.map(|p| p.to_string()).unwrap_or_default();
    let process = row.process_name.as_deref().unwrap_or("");
    let (ws, tab) = match &row.source {
        SourceTag::Tasty {
            workspace_name,
            tab_name,
        } => (workspace_name.as_str(), tab_name.as_deref().unwrap_or("")),
        SourceTag::External => ("", ""),
    };
    let haystacks: [&str; 6] = [
        port_str.as_str(),
        row.addr_display.as_str(),
        pid_str.as_str(),
        process,
        ws,
        tab,
    ];
    haystacks
        .iter()
        .any(|h| h.to_lowercase().contains(&q_lower))
}

/// TCP states actually present in the scope `rows` (before state/search
/// filtering). Display order: LISTEN first, then `label()` alphabetical.
/// Derived from the full scope so a toggled-off state can still be re-enabled
/// (a state filtered out of the *visible* rows would otherwise vanish from the
/// dropdown). Deduplicated.
pub fn present_states(rows: &[PortRowView]) -> Vec<PortState> {
    let mut seen: Vec<PortState> = Vec::new();
    for r in rows {
        if !seen.contains(&r.state) {
            seen.push(r.state);
        }
    }
    seen.sort_by_key(|s| (!s.is_listen(), s.label()));
    seen
}

fn scope_from_flag(show_all_system: bool) -> ScanScope {
    if show_all_system {
        ScanScope::System
    } else {
        ScanScope::Tasty
    }
}

/// Snapshot every Tasty surface that has a live shell PID, paired with its
/// workspace/tab display path. The background worker can run without any
/// CoreState reference.
fn build_snapshot(
    engine: &EngineRead<'_>,
    presentation: &dyn crate::model::StructurePresentation,
    show_all_system: bool,
) -> ScanSnapshot {
    let mut surfaces: Vec<(u32, u32, SurfaceDisplayPath)> = Vec::new();
    for ws in &engine.workspaces() {
        for pane_id in ws.pane_layout().all_pane_ids() {
            if let Some(pane) = ws.pane_layout().find_pane(pane_id) {
                for tab in &pane.tabs {
                    for sid in tab.all_surface_ids() {
                        let Some(shell_pid) = engine.terminals.process_id(sid) else {
                            continue;
                        };
                        let Some(path) = engine.surface_display_path(sid, presentation) else {
                            continue;
                        };
                        surfaces.push((sid, shell_pid, path));
                    }
                }
            }
        }
    }
    ScanSnapshot {
        surfaces,
        show_all_system,
    }
}

/// 원 표시 요청의 snapshot을 App에 넘긴다. worker와 결과 채널은 App이 소유한다.
/// 메인 조회와 즐겨찾기 조회는 각각 독립된 상태를 사용한다.
pub fn kick_off_scan(
    slot: &mut PortScanState,
    presentation: &dyn crate::model::StructurePresentation,
    engine: &EngineRead<'_>,
    ctx: &egui::Context,
    show_all_system: bool,
) {
    let snapshot = build_snapshot(engine, presentation, show_all_system);
    let scope = scope_from_flag(show_all_system);
    *slot = PortScanState::Loading {
        ticket: Arc::new(()),
        request: Some(snapshot),
        scope,
    };
    ctx.request_repaint();
}

/// Project `favorites` into display rows, matching each `(addr, port)`
/// against `system_rows` (the favorites-scan's `Ready` rows, or `None` while
/// it hasn't completed yet — favorites render with `matched: None` until
/// then, same as a genuine "not observed" result). A favorite with more than
/// one matching socket (e.g. a LISTEN plus an inbound ESTABLISHED sharing the
/// port) prefers the LISTEN row.
fn build_favorite_rows(
    favorites: &[crate::core::port_favorites::PortFavorite],
    system_rows: Option<&[PortRowView]>,
) -> Vec<FavoriteRowView> {
    favorites
        .iter()
        .map(|fav| {
            let addr_display = format_addr(fav.addr);
            let matched = system_rows.and_then(|rows| {
                rows.iter()
                    .filter(|r| r.addr_display == addr_display && r.port == fav.port)
                    .max_by_key(|r| r.state.is_listen())
            });
            FavoriteRowView {
                addr_display,
                port: fav.port,
                matched: matched.map(|r| FavoriteMatch {
                    process_name: r.process_name.clone(),
                    pid: r.pid,
                    workspace_name: match &r.source {
                        SourceTag::Tasty { workspace_name, .. } => Some(workspace_name.clone()),
                        SourceTag::External => None,
                    },
                    state: r.state,
                }),
            }
        })
        .collect()
}

/// Drain a pending result from the channel for both scan slots (main +
impl PortScanState {
    pub(crate) fn take_request(&mut self) -> Option<(Weak<()>, ScanSnapshot)> {
        match self {
            Self::Loading {
                ticket, request, ..
            } => request
                .take()
                .map(|request| (Arc::downgrade(ticket), request)),
            _ => None,
        }
    }

    pub(crate) fn accept(
        &mut self,
        request: &Weak<()>,
        result: Result<Vec<PortRowView>, String>,
    ) -> bool {
        let Self::Loading { ticket, scope, .. } = self else {
            return false;
        };
        if !Arc::downgrade(ticket).ptr_eq(request) {
            return false;
        }
        *self = match result {
            Ok(rows) => Self::Ready {
                rows,
                scope: *scope,
            },
            Err(error) => Self::Failed(error),
        };
        true
    }
}

/// Pure view: draws the popup body from `props` and reports intent.
///
/// No `MainViewState` / `CoreState` / global `theme::theme()` access. Safe to call
/// from a gallery with mock props.
pub fn draw_port_scanner_view(
    ui: &mut egui::Ui,
    props: &PortScannerProps<'_>,
) -> PortScannerAction {
    if ui.ctx().input(|i| i.key_pressed(egui::Key::Escape)) {
        // 드롭다운이 열려 있으면 Escape로 드롭다운만 닫는다.
        let popup_open = ui
            .ctx()
            .memory(|m| m.is_popup_open(egui::Id::new(STATE_FILTER_POPUP_ID)));
        if popup_open {
            ui.ctx().memory_mut(|m| m.close_popup());
            return PortScannerAction::None;
        }
        return PortScannerAction::Close;
    }

    let mut action = PortScannerAction::None;

    // 공통 내부 여백 없이 구역마다 여백을 주고 구역 사이 간격은 없앤다.
    let full = ui.max_rect();
    let sep = egui::Stroke::new(
        props.theme.border_width.value(),
        props.theme.border_strong(),
    );
    let saved_spacing = ui.spacing().item_spacing;
    ui.spacing_mut().item_spacing.y = 0.0;

    let h_ir = egui::Frame::NONE
        .inner_margin(egui::Margin {
            left: PANEL_PAD_X,
            right: PANEL_PAD_X,
            top: props.theme.spacing_md.value() as i8,
            bottom: props.theme.spacing_md.value() as i8,
        })
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing = saved_spacing;
            draw_header_row(ui, props)
        });
    if let Some(a) = h_ir.inner {
        action = a;
    }
    ui.painter()
        .hline(full.x_range(), h_ir.response.rect.bottom(), sep);
    // 실제 헤더 전체를 이동 영역으로 보고한다.
    super::report_header_drag_rect(
        ui.ctx(),
        PORT_SCANNER_POPUP_ID,
        egui::Rect::from_x_y_ranges(full.x_range(), full.top()..=h_ir.response.rect.bottom()),
    );

    let f_ir = egui::Frame::NONE
        .inner_margin(egui::Margin {
            left: PANEL_PAD_X,
            right: PANEL_PAD_X,
            top: props.theme.spacing_sm.value() as i8,
            bottom: props.theme.spacing_sm.value() as i8,
        })
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing = saved_spacing;
            draw_filter_row(ui, props)
        });
    if let Some(a) = f_ir.inner {
        action = a;
    }
    ui.painter()
        .hline(full.x_range(), f_ir.response.rect.bottom(), sep);

    // 즐겨찾기 높이는 위에서, 푸터는 아래에서 먼저 확보하고 나머지를 본문에 준다.
    if let Some(a) = draw_favorites_section(ui, props) {
        action = a;
    }

    // 푸터 공간을 먼저 확보하도록 CentralPanel보다 먼저 만든다.
    let mut footer_action: Option<PortScannerAction> = None;
    let footer = egui::TopBottomPanel::bottom("port_scanner.footer")
        .resizable(false)
        .show_separator_line(false)
        .frame(egui::Frame::NONE.inner_margin(egui::Margin {
            left: PANEL_PAD_X,
            right: PANEL_PAD_X,
            top: FOOTER_PAD_Y,
            bottom: FOOTER_PAD_Y,
        }))
        .show_inside(ui, |ui| {
            ui.spacing_mut().item_spacing = saved_spacing;
            draw_footer(ui, props)
        });
    if let Some(a) = footer.inner {
        footer_action = Some(a);
    }
    ui.painter()
        .hline(full.x_range(), footer.response.rect.top(), sep);

    egui::CentralPanel::default()
        .frame(egui::Frame::NONE)
        .show_inside(ui, |ui| match &props.view_state {
            PortScannerViewState::Loading => draw_loading_body(ui, props),
            PortScannerViewState::Failed { message } => draw_failed_body(ui, props, message),
            PortScannerViewState::Ready { rows, .. } => {
                if let Some(a) = draw_ready_body(ui, props, rows) {
                    action = a;
                }
            }
        });

    if let Some(a) = footer_action {
        action = a;
    }

    action
}

/// `host:port` string, bracketing bare IPv6 literals (e.g. `[::]:8080`,
/// `127.0.0.1:3000`). Shared by the footer "Copy address" button, the star
/// tooltip's `{key}`, and the favorites section's address cell.
fn format_host_port(addr_display: &str, port: u16) -> String {
    let host = if addr_display.contains(':') {
        format!("[{addr_display}]")
    } else {
        addr_display.to_string()
    };
    format!("{host}:{port}")
}

/// Address string copied by the footer "Copy address" button.
fn row_copy_address(row: &PortRowView) -> String {
    format_host_port(&row.addr_display, row.port)
}

/// footer: 좌측 카운터 + 우측 `Copy address`(선택 없으면 disabled) + `Close`.
fn draw_footer(ui: &mut egui::Ui, props: &PortScannerProps<'_>) -> Option<PortScannerAction> {
    let th = props.theme;
    let mut out: Option<PortScannerAction> = None;
    let counter = match &props.view_state {
        PortScannerViewState::Loading => Some(props.label_footer_loading.to_string()),
        PortScannerViewState::Ready { rows, total, .. } => Some(
            props
                .label_footer_counter
                .replace("{shown}", &rows.len().to_string())
                .replace("{total}", &total.to_string()),
        ),
        PortScannerViewState::Failed { .. } => None,
    };
    let selected_addr = match &props.view_state {
        PortScannerViewState::Ready { rows, .. } => props
            .filter
            .selected_port
            .and_then(|p| rows.iter().find(|r| r.port == p))
            .map(row_copy_address),
        _ => None,
    };
    ui.horizontal(|ui| {
        if let Some(s) = &counter {
            ui.label(
                egui::RichText::new(s)
                    .color(th.text_disabled())
                    .size(th.font_size_caption.value()),
            );
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if Button::new(props.label_close)
                .variant(ButtonVariant::Secondary)
                .show(ui, th)
                .clicked()
            {
                out = Some(PortScannerAction::Close);
            }
            if Button::new(props.label_copy_address)
                .variant(ButtonVariant::Ghost)
                .enabled(selected_addr.is_some())
                .show(ui, th)
                .clicked()
                && let Some(addr) = &selected_addr
            {
                out = Some(PortScannerAction::CopyAddress(addr.clone()));
            }
        });
    });
    out
}

/// 헤더 accent Tag pill 텍스트: Loading → `scanning…`, Ready → `{n} listening`
/// (LISTEN-only scope count, search-independent), Failed → 없음.
fn header_tag_text(props: &PortScannerProps<'_>) -> Option<String> {
    match &props.view_state {
        PortScannerViewState::Loading => Some(props.label_header_tag_scanning.to_string()),
        PortScannerViewState::Ready { listening, .. } => Some(
            props
                .label_header_tag_count
                .replace("{n}", &listening.to_string()),
        ),
        PortScannerViewState::Failed { .. } => None,
    }
}

/// accent Tag pill 렌더 (pid 뱃지 패턴 재사용, accent 변형).
fn draw_header_count_tag(ui: &mut egui::Ui, th: &Theme, text: &str) {
    egui::Frame::default()
        .fill(th.accent_primary().into())
        .corner_radius(TAG_PILL_CORNER_RADIUS)
        // structural: 검색줄 control-internal nudge (size-4/size-1), spacing 리듬 아님.
        .inner_margin(margin_sym(STRUCT_GAP_4, STRUCT_GAP_1))
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new(text)
                    .color(th.text_on_accent())
                    .size(th.font_size_caption.value()),
            );
        });
}

/// 1줄 헤더: 좌측 포트 아이콘 + heading + accent count Tag, 우측 search +
/// Refresh + close.
fn draw_header_row(ui: &mut egui::Ui, props: &PortScannerProps<'_>) -> Option<PortScannerAction> {
    let th = props.theme;
    let mut out: Option<PortScannerAction> = None;
    // 헤더 글자 선택이 창 드래그를 막지 않도록 헤더 안에서만 텍스트 선택을 끈다.
    ui.style_mut().interaction.selectable_labels = false;
    ui.horizontal(|ui| {
        ui.add(icons::PORT.image(th.icon_glyph_size_md.value(), th.text_muted().into()));
        ui.label(
            egui::RichText::new(props.label_heading)
                .color(th.text_primary())
                .size(th.font_size_heading.value())
                .strong(),
        );
        if let Some(tag) = header_tag_text(props) {
            draw_header_count_tag(ui, th, &tag);
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if IconButton::new()
                .variant(IconButtonVariant::Ghost)
                .show(ui, th, &|ui, rect, c| {
                    icons::CLOSE
                        .image(th.icon_glyph_size_md.value(), c)
                        .paint_at(ui, rect)
                })
                .on_hover_text(props.label_close)
                .clicked()
            {
                out = Some(PortScannerAction::Close);
            }
            if IconButton::new()
                .variant(IconButtonVariant::Ghost)
                .show(ui, th, &|ui, rect, c| {
                    icons::REFRESH
                        .image(th.icon_glyph_size_md.value(), c)
                        .paint_at(ui, rect)
                })
                .on_hover_text(props.label_refresh)
                .clicked()
            {
                out = Some(PortScannerAction::Refresh);
            }
            if let Some(a) = draw_column_chooser(ui, props) {
                out = Some(a);
            }
            let mut buf = props.filter.query.to_string();
            let resp = Input::new()
                .placeholder(props.label_search_placeholder)
                .width(200.0)
                .icon(&|ui, rect, c| {
                    icons::SEARCH
                        .image(th.icon_glyph_size_row_action.value(), c)
                        .paint_at(ui, rect)
                })
                .show(ui, th, &mut buf);
            if resp.changed() && buf != props.filter.query {
                out = Some(PortScannerAction::SetQuery(buf));
            }
        });
    });
    out
}

/// 컬럼별 표시 라벨 (props 의 i18n 컬럼 라벨로 투영).
fn column_label<'a>(col: ColumnId, props: &PortScannerProps<'a>) -> &'a str {
    match col {
        ColumnId::Port => props.label_column_port,
        ColumnId::Proto => props.label_column_proto,
        ColumnId::Address => props.label_column_address,
        ColumnId::Process => props.label_column_process,
        ColumnId::Workspace => props.label_column_workspace,
        ColumnId::Tab => props.label_column_tab,
        ColumnId::State => props.label_column_state,
    }
}

/// 헤더 우측 COLUMNS IconButton + 컬럼 표시/숨김 chooser 팝업.
///
/// 팝업 본문은 컬럼별 [`checkbox`] 목록(전체폭). Port 는 식별/기본 정렬 컬럼이라
/// 항상 표시(checkbox disabled). 토글 시 `SetColumnVisible` 액션을 반환한다. 팝업은
/// `CloseOnClickOutside` 라 한 번 열어 여러 컬럼을 연속 토글할 수 있다.
fn draw_column_chooser(
    ui: &mut egui::Ui,
    props: &PortScannerProps<'_>,
) -> Option<PortScannerAction> {
    let th = props.theme;
    let mut out: Option<PortScannerAction> = None;

    let resp = IconButton::new()
        .variant(IconButtonVariant::Ghost)
        .show(ui, th, &|ui, rect, c| {
            icons::COLUMNS
                .image(th.icon_glyph_size_md.value(), c)
                .paint_at(ui, rect)
        })
        .on_hover_text(props.label_columns_button);

    let popup_id = ui.make_persistent_id(COLUMN_CHOOSER_POPUP_ID);
    if resp.clicked() {
        ui.memory_mut(|m| m.toggle_popup(popup_id));
    }

    tasty_egui_theme::with_popover_frame(ui, th, |ui| {
        egui::popup_below_widget(
            ui,
            popup_id,
            &resp,
            egui::PopupCloseBehavior::CloseOnClickOutside,
            |ui| {
                ui.set_min_width(th.port_columns_menu_min_width().value());
                ui.label(
                    egui::RichText::new(props.label_columns_menu_title)
                        .color(th.text_muted())
                        .size(th.font_size_caption.value())
                        .strong(),
                );
                ui.add_space(th.spacing_xs.value());
                for col in ColumnId::ALL {
                    let mut checked = props.filter.columns.is_visible(col);
                    let enabled = !col.mandatory();
                    if checkbox(
                        ui,
                        th,
                        &mut checked,
                        column_label(col, props),
                        enabled,
                        tasty_ui_widgets::Ground::Raised,
                    )
                    .changed()
                    {
                        out = Some(PortScannerAction::SetColumnVisible(col, checked));
                    }
                }
            },
        )
    });
    // 팝업 밖으로 나온 드롭다운도 안쪽 클릭으로 인식하도록 영역을 보고한다.
    super::report_dropdown_overlay(ui, PORT_SCANNER_POPUP_ID, COLUMN_CHOOSER_POPUP_ID, popup_id);

    out
}

fn read_state_draft(ctx: &egui::Context) -> HashSet<PortState> {
    ctx.memory(|m| {
        m.data
            .get_temp::<HashSet<PortState>>(egui::Id::new(STATE_FILTER_DRAFT_ID))
            .unwrap_or_default()
    })
}

fn write_state_draft(ctx: &egui::Context, draft: HashSet<PortState>) {
    ctx.memory_mut(|m| {
        m.data
            .insert_temp(egui::Id::new(STATE_FILTER_DRAFT_ID), draft);
    });
}

/// 일부 상태만 표시 중이면 강조하는 상태 필터 버튼.
fn state_filter_button(
    ui: &mut egui::Ui,
    th: &Theme,
    label: &str,
    filtered: bool,
) -> egui::Response {
    let text_col: egui::Color32 = if filtered {
        th.text_on_accent().into()
    } else {
        th.text_primary().into()
    };
    let fill: egui::Color32 = if filtered {
        th.accent_primary().into()
    } else {
        th.surface_raised().into()
    };
    let stroke = if filtered {
        egui::Stroke::NONE
    } else {
        egui::Stroke::new(th.border_width.value(), th.border_strong())
    };
    ui.add(
        egui::Button::image_and_text(
            icons::FUNNEL.image(th.icon_glyph_size_sm.value(), text_col),
            egui::RichText::new(label)
                .color(text_col)
                .size(th.font_size_body.value()),
        )
        .fill(fill)
        .stroke(stroke),
    )
}

/// 드롭다운 내부 구분선.
fn state_filter_hsep(ui: &mut egui::Ui, th: &Theme) {
    vspace(ui, STRUCT_GAP_2);
    let r = ui.max_rect();
    ui.painter().hline(
        r.x_range(),
        ui.cursor().top(),
        egui::Stroke::new(th.border_width.value(), th.border_strong()),
    );
    vspace(ui, STRUCT_GAP_2);
}

/// 상태 필터. 편집은 임시 값에 반영하고 적용 버튼으로 확정한다.
/// 체크한 상태를 표시하며 초기화는 LISTEN만 선택한다.
fn draw_state_filter(ui: &mut egui::Ui, props: &PortScannerProps<'_>) -> Option<PortScannerAction> {
    let th = props.theme;
    let present = props.present_states;
    let popup_id = egui::Id::new(STATE_FILTER_POPUP_ID);

    let total = present.len();
    let selected = present
        .iter()
        .filter(|s| props.filter.visible_states.contains(*s))
        .count();
    let filtered = selected < total;
    let label = if filtered {
        format!("{} · {}/{}", props.label_state_filter, selected, total)
    } else {
        props.label_state_filter.to_string()
    };

    let btn = state_filter_button(ui, th, &label, filtered);
    if btn.clicked() {
        if !ui.memory(|m| m.is_popup_open(popup_id)) {
            write_state_draft(ui.ctx(), props.filter.visible_states.clone());
        }
        ui.memory_mut(|m| m.toggle_popup(popup_id));
    }

    let mut applied: Option<PortScannerAction> = None;
    tasty_egui_theme::with_popover_frame(ui, th, |ui| {
        egui::popup::popup_above_or_below_widget(
            ui,
            popup_id,
            &btn,
            egui::AboveOrBelow::Below,
            egui::PopupCloseBehavior::CloseOnClickOutside,
            |ui| {
                ui.set_min_width(th.port_state_menu_min_width().value());
                ui.label(
                    egui::RichText::new(props.label_state_filter_title)
                        .color(th.text_muted())
                        .size(th.font_size_caption.value())
                        .family(tasty_ui_widgets::label_font_family()),
                );
                ui.add_space(th.spacing_xs.value());

                let mut draft = read_state_draft(ui.ctx());
                let mut draft_changed = false;
                egui::ScrollArea::vertical()
                    .max_height(th.port_state_menu_max_height().value())
                    .drag_to_scroll(false)
                    .show(ui, |ui| {
                        for st in present {
                            let mut checked = draft.contains(st);
                            if checkbox(
                                ui,
                                th,
                                &mut checked,
                                st.label(),
                                true,
                                tasty_ui_widgets::Ground::Raised,
                            )
                            .changed()
                            {
                                if checked {
                                    draft.insert(*st);
                                } else {
                                    draft.remove(st);
                                }
                                draft_changed = true;
                            }
                        }
                    });
                if draft_changed {
                    write_state_draft(ui.ctx(), draft);
                }

                state_filter_hsep(ui, th);
                ui.horizontal(|ui| {
                    if Button::new(props.label_state_filter_select_all)
                        .variant(ButtonVariant::Ghost)
                        .show(ui, th)
                        .clicked()
                    {
                        write_state_draft(ui.ctx(), present.iter().copied().collect());
                    }
                    if Button::new(props.label_state_filter_deselect_all)
                        .variant(ButtonVariant::Ghost)
                        .show(ui, th)
                        .clicked()
                    {
                        write_state_draft(ui.ctx(), HashSet::new());
                    }
                });
                ui.horizontal(|ui| {
                    if Button::new(props.label_state_filter_reset)
                        .variant(ButtonVariant::Ghost)
                        .show(ui, th)
                        .clicked()
                    {
                        write_state_draft(ui.ctx(), HashSet::from([PortState::Listen]));
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if Button::new(props.label_state_filter_apply)
                            .variant(ButtonVariant::Primary)
                            .show(ui, th)
                            .clicked()
                        {
                            // Apply = draft ∩ present (사라진 상태가 묻어 들어가지 않게 보정).
                            let set: HashSet<PortState> = read_state_draft(ui.ctx())
                                .into_iter()
                                .filter(|s| present.contains(s))
                                .collect();
                            applied = Some(PortScannerAction::SetVisibleStates(set));
                        }
                    });
                });
            },
        )
    });
    super::report_dropdown_overlay(ui, PORT_SCANNER_POPUP_ID, STATE_FILTER_POPUP_ID, popup_id);
    if applied.is_some() {
        ui.memory_mut(|m| m.close_popup());
    }
    applied
}

/// 2줄 헤더(필터 줄): 좌측 "전체 보기" 체크박스 + 우측 상태 필터 버튼.
fn draw_filter_row(ui: &mut egui::Ui, props: &PortScannerProps<'_>) -> Option<PortScannerAction> {
    let mut out: Option<PortScannerAction> = None;
    ui.horizontal(|ui| {
        let mut checked = props.filter.show_all_system;
        if checkbox(
            ui,
            props.theme,
            &mut checked,
            props.label_filter_show_all_system,
            true,
            tasty_ui_widgets::Ground::Panel,
        )
        .changed()
        {
            out = Some(PortScannerAction::SetShowAllSystem(checked));
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if let Some(a) = draw_state_filter(ui, props) {
                out = Some(a);
            }
        });
    });
    out
}

/// 즐겨찾기 리스트 스크롤 cap(design "5행 × 22px = 110 ≤ 112 cap") — 5행이 꽉 채워도
/// 스크롤 시작 전 여유 2px 를 남겨 스크롤 가능함을 암시한다.
const FAVORITES_LIST_MAX_H: LogicalPx = LogicalPx(112.0);

/// `PortStar` (design `PortStar`) — 22×22(`item_height_tree`) 별 토글. `on` 이면 채운
/// `STAR_FILL` + accent-warning(Explorer 즐겨찾기와 동일 골드), 아니면 outline `STAR`
/// 와 text-muted. hover 배경 = overlay-hover. 클릭 시 즉시 토글(확인 절차 없음) — 호출
/// 측이 `clicked()` 를 보고 액션을 emit 한다.
fn draw_port_star(ui: &mut egui::Ui, th: &Theme, on: bool) -> egui::Response {
    let side = th.item_height_tree.value();
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(side, side), egui::Sense::click());
    if resp.hovered() {
        ui.painter().rect_filled(
            rect,
            th.corner_radius_sm.value(),
            th.overlay_hover().to_egui_premultiplied(),
        );
    }
    let glyph = th.icon_glyph_size_sm.value();
    let icon_rect = egui::Rect::from_center_size(rect.center(), egui::vec2(glyph, glyph));
    let selected = in_selected_table_row(ui);
    if on {
        let fg = if selected {
            th.port_star_on_active()
        } else {
            th.port_star_on()
        };
        icons::STAR_FILL
            .image(glyph, fg.into())
            .paint_at(ui, icon_rect);
    } else {
        let fg = if selected {
            th.port_star_off_active()
        } else {
            th.port_star_off()
        };
        icons::STAR.image(glyph, fg.into()).paint_at(ui, icon_rect);
    }
    resp
}

/// 별 열(`port_star_col_width`) 안 가운데에 별을 둔다. 디자인 표의 tight 열과 즐겨찾기 행의
/// 별 칸이 모두 가운데 정렬이라 두 영역의 별이 같은 세로선에 선다.
fn draw_port_star_centered(ui: &mut egui::Ui, th: &Theme, on: bool) -> egui::Response {
    let side = th.item_height_tree.value();
    ui.add_space(((ui.available_width() - side) * 0.5).max(0.0));
    draw_port_star(ui, th, on)
}

/// `FavoritesSection` (design `FavoritesSection`) — 상단 즐겨찾기 섹션. 항상 노출되는
/// 캡션(22px: "Favorites"(+개수) · 우측 "system-wide") + 빈 상태(37% 투명 별 + 안내
/// 22px) 또는 스크롤 리스트(최대 112px, 행 22px). 배경 bg-sidebar + 하단 separator.
fn draw_favorites_section(
    ui: &mut egui::Ui,
    props: &PortScannerProps<'_>,
) -> Option<PortScannerAction> {
    let th = props.theme;
    let mut out: Option<PortScannerAction> = None;
    let row_h = th.item_height_tree.value();
    let full = ui.max_rect();

    let ir = egui::Frame::NONE
        .fill(th.bg_sidebar().into())
        // 디자인 즐겨찾기 행은 왼쪽 여백 없이 별 칸(28)부터 시작해 표의 별 열과 맞는다.
        // 캡션과 빈 안내 줄만 popup 좌우 여백을 둔다.
        .inner_margin(egui::Margin {
            left: 0,
            right: PANEL_PAD_X,
            top: 0,
            bottom: 0,
        })
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;

            ui.allocate_ui_with_layout(
                egui::vec2(ui.available_width(), row_h),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    ui.add_space(f32::from(PANEL_PAD_X));
                    let heading = if props.favorites.is_empty() {
                        props.label_favorites_heading.to_string()
                    } else {
                        props
                            .label_favorites_count
                            .replace("{n}", &props.favorites.len().to_string())
                    };
                    ui.label(
                        egui::RichText::new(heading)
                            .color(th.text_muted())
                            .size(th.font_size_caption.value())
                            .strong(),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            egui::RichText::new(props.label_favorites_system_wide)
                                .color(th.text_muted())
                                .size(th.font_size_caption.value()),
                        );
                    });
                },
            );

            if props.favorites.is_empty() {
                ui.allocate_ui_with_layout(
                    egui::vec2(ui.available_width(), row_h),
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| {
                        ui.add_space(f32::from(PANEL_PAD_X));
                        ui.spacing_mut().item_spacing.x = th.spacing_xs.value();
                        let sz = th.icon_glyph_size_sm.value();
                        let (r, _) =
                            ui.allocate_exact_size(egui::vec2(sz, sz), egui::Sense::hover());
                        // 즐겨찾기 별 아이콘 톤. 대응 토큰 없음 — 같은 아이콘이 두 곳에서
                        // 서로 다른 값을 쓴다(수렴은 디자인 판단).
                        const FAV_STAR_ICON_OPACITY: f32 = 0.37;
                        icons::STAR
                            .image(
                                sz,
                                th.text_muted()
                                    .to_egui()
                                    .gamma_multiply(FAV_STAR_ICON_OPACITY),
                            )
                            .paint_at(ui, r);
                        ui.label(
                            egui::RichText::new(props.label_favorites_empty)
                                .color(th.text_muted())
                                .italics()
                                .size(th.font_size_caption.value()),
                        );
                    },
                );
            } else {
                egui::ScrollArea::vertical()
                    .id_salt("port_scanner.favorites_scroll")
                    .max_height(FAVORITES_LIST_MAX_H.value())
                    .auto_shrink([false, true])
                    .drag_to_scroll(false)
                    .show(ui, |ui| {
                        for fav in props.favorites {
                            if let Some(a) = draw_favorite_row(ui, props, fav, row_h) {
                                out = Some(a);
                            }
                        }
                    });
            }
        });

    ui.painter().hline(
        full.x_range(),
        ir.response.rect.bottom(),
        egui::Stroke::new(
            th.border_width.value(),
            th.separator.to_egui_premultiplied(),
        ),
    );
    out
}

/// 즐겨찾기 리스트 1행(요약형) — 별(항상 on, 클릭 시 제거) · `{addr}:{port}`(mono) ·
/// 매칭 있으면 `{process} · {pid}`(+workspace) 없으면 "not running" · 우측 상태 배지
/// (LISTEN → running+pulse, 그 외 매칭 → waiting, 매칭 없음(NONE) → idle+"NONE").
fn draw_favorite_row(
    ui: &mut egui::Ui,
    props: &PortScannerProps<'_>,
    fav: &FavoriteRowView,
    row_h: f32,
) -> Option<PortScannerAction> {
    let th = props.theme;
    let mut out: Option<PortScannerAction> = None;
    let key = format_host_port(&fav.addr_display, fav.port);

    ui.allocate_ui_with_layout(
        egui::vec2(ui.available_width(), row_h),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            // 디자인 행은 별 칸·주소·상세·상태를 간격 없이 잇는다. 상세 앞 간격은 위젯이 둔다.
            ui.spacing_mut().item_spacing.x = 0.0;
            ui.allocate_ui_with_layout(
                egui::vec2(th.port_star_col_width().value(), row_h),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    let resp = draw_port_star_centered(ui, th, true)
                        .on_hover_text(props.label_favorite_remove.replace("{key}", &key));
                    if resp.clicked() {
                        out = Some(PortScannerAction::ToggleFavorite(
                            fav.addr_display.clone(),
                            fav.port,
                        ));
                    }
                },
            );
            ui.label(
                egui::RichText::new(&key)
                    .monospace()
                    .color(th.text_primary())
                    .size(th.font_size_caption.value()),
            );
            let (kind, state, pulse) = match &fav.matched {
                Some(m) if m.state.is_listen() => (StatusKind::Running, m.state.label(), true),
                Some(m) => (StatusKind::Waiting, m.state.label(), false),
                None => (StatusKind::Idle, props.label_state_none, false),
            };
            let detail = match &fav.matched {
                Some(m) => {
                    let proc = m.process_name.as_deref().unwrap_or("—");
                    let mut s = match m.pid {
                        Some(pid) => format!("{proc} · {pid}"),
                        None => proc.to_string(),
                    };
                    if let Some(ws) = &m.workspace_name {
                        s.push_str(" · ");
                        s.push_str(ws);
                    }
                    s
                }
                None => props.label_favorites_not_running.to_string(),
            };
            ports_favorite_detail_and_state(
                ui,
                th,
                &detail,
                kind,
                state,
                pulse,
                props.reduced_motion,
            );
        },
    );
    out
}

/// 콘텐츠 중앙 horizontal: Spinner + "Collecting…" 텍스트.
fn draw_loading_body(ui: &mut egui::Ui, props: &PortScannerProps<'_>) {
    let th = props.theme;
    ui.vertical_centered(|ui| {
        // 로딩·실패·빈 목록 메시지의 시작 높이를 맞춘다.
        vspace(ui, th.spacing_xl * 2.0);
        ui.horizontal(|ui| {
            ui.add(
                egui::Spinner::new()
                    .size(th.icon_glyph_size_md.value())
                    .color(th.text_muted()),
            );
            ui.label(
                egui::RichText::new(props.label_loading)
                    .color(th.text_muted())
                    .size(th.font_size_body.value()),
            );
        });
    });
}

/// Failed: 에러 메시지만. Refresh 는 헤더 우측 버튼(상시 노출)에서 처리한다.
fn draw_failed_body(ui: &mut egui::Ui, props: &PortScannerProps<'_>, message: &str) {
    let th = props.theme;
    ui.vertical_centered(|ui| {
        vspace(ui, th.spacing_xl * 2.0);
        ui.label(
            egui::RichText::new(props.label_failed)
                .color(th.accent_danger_raised())
                .size(th.font_size_body.value())
                .strong(),
        );
        ui.label(
            egui::RichText::new(message)
                .color(th.text_muted())
                .size(th.font_size_caption.value()),
        );
    });
}

/// Ready: 빈 결과 분기 4종 OR TableBuilder 7컬럼.
fn draw_ready_body(
    ui: &mut egui::Ui,
    props: &PortScannerProps<'_>,
    rows: &[PortRowView],
) -> Option<PortScannerAction> {
    let th = props.theme;
    if rows.is_empty() {
        // hidden_by_state 는 검색이 빈 상태에서만 wrapper 가 세우므로 search-zero 보다
        // 뒤에서 본다(검색 결과 0 이 우선). scope 에 행은 있으나 상태 필터가 전부 가렸을 때.
        let empty_label = if !props.filter.query.trim().is_empty() {
            props.label_no_ports_search_zero
        } else if props.filter.hidden_by_state {
            props.label_no_ports_state_filtered
        } else if props.filter.show_all_system {
            props.label_no_ports_system_empty
        } else {
            props.label_no_ports_tasty_empty
        };
        ui.vertical_centered(|ui| {
            vspace(ui, th.spacing_xl * 2.0);
            ui.label(
                egui::RichText::new(empty_label)
                    .color(th.text_muted())
                    .italics()
                    .size(th.font_size_body.value()),
            );
        });
        return None;
    }
    draw_table(ui, props, rows)
}

/// 디자인 Table td/th `padding: 0 12` — 좌측 정렬 셀. 콘텐츠를 좌측 12 들여쓴다.
fn cell_l(ui: &mut egui::Ui, content: impl FnOnce(&mut egui::Ui)) {
    let th = crate::theme::theme();
    hspace(ui, th.spacing_md);
    content(ui);
}

/// 7컬럼 공용 [`Table`] 위젯. 컬럼 폭은 디자이너 확정값.
fn draw_table(
    ui: &mut egui::Ui,
    props: &PortScannerProps<'_>,
    rows: &[PortRowView],
) -> Option<PortScannerAction> {
    let th = props.theme;
    // 헤더와 본문 행은 공용 Table 기본값(`table-cell-height`)을 쓴다.
    let cell_h = th.table_cell_height();

    // 본문 높이에서 고정 헤더와 간격만 뺀다. 푸터는 바깥 패널이 이미 확보했다.
    // 본문은 최소 한 행 높이를 남긴다.
    let gap = LogicalPx(ui.spacing().item_spacing.y);
    let max_scroll = (LogicalPx(ui.available_height()) - cell_h - gap).max(cell_h);

    // 열 폭은 공용 포트 표 정의를 따른다. 고정 열(Address 포함)은 하한 폭을 유지하고 Process가 남는
    // 폭을 모두 받는다. 하한의 합이 가용 폭을 넘으면 열을 줄이지 않고 본문만 가로 스크롤한다.
    let visible: Vec<ColumnId> = ColumnId::ALL
        .into_iter()
        .filter(|c| props.filter.columns.is_visible(*c))
        .collect();
    let widths = column_widths(&visible, th);

    let mut columns: Vec<TableColumn<SortKey>> = Vec::with_capacity(visible.len() + 1);
    columns.push(TableColumn {
        title: "",
        width: ports_star_column_width(th),
        align: TableAlign::Left,
        sort_id: None,
    });
    columns.extend(visible.iter().zip(widths).map(|(id, width)| TableColumn {
        title: column_label(*id, props),
        width,
        align: id.shared().align(),
        sort_id: sort_key(*id),
    }));

    let sort_dir = match props.filter.sort_dir {
        SortDir::Asc => TableSortDir::Asc,
        SortDir::Desc => TableSortDir::Desc,
    };
    let selected_port = props.filter.selected_port;

    // 별 클릭과 행 클릭이 겹치므로 별을 누른 프레임에는 즐겨찾기만 바꾼다.
    let mut fav_click: Option<(String, u16)> = None;

    let output = ports_table(columns, th)
        .id_salt("port_scanner.table")
        .active_sort(props.filter.sort_key, sort_dir)
        .max_scroll_height(max_scroll)
        .show(
            ui,
            th,
            rows,
            |row: &PortRowView| selected_port == Some(row.port),
            // 첫 열은 별이며 나머지는 숨긴 열을 제외한 ColumnId 순서로 찾는다.
            |ui, th, row, col_index| {
                if col_index == 0 {
                    let key = format_host_port(&row.addr_display, row.port);
                    let tooltip = if row.favorited {
                        props.label_favorite_remove.replace("{key}", &key)
                    } else {
                        props.label_favorite_add.replace("{key}", &key)
                    };
                    let resp =
                        draw_port_star_centered(ui, th, row.favorited).on_hover_text(tooltip);
                    if resp.clicked() {
                        fav_click = Some((row.addr_display.clone(), row.port));
                    }
                    return;
                }
                // kit 의 mono 열(Port·Proto·Address)은 font-mono 이고, strong 열(Process)은 UI 글꼴이다.
                // 둘 다 text-primary 다.
                // Workspace·Tab 은 색을 두지 않아 표의 행 글자색을 따른다.
                let value_fg = table_cell_ink(ui, th, th.text_primary());
                match visible[col_index - 1] {
                    ColumnId::Port => {
                        hspace(ui, th.spacing_md);
                        ui.label(
                            egui::RichText::new(row.port.to_string())
                                .color(value_fg)
                                .size(th.font_size_body.value())
                                .monospace(),
                        );
                    }
                    ColumnId::Proto => cell_l(ui, |ui| {
                        // Proto derived from the address family: IPv6 displays
                        // (always containing a colon) → `tcp6`, IPv4 → `tcp`.
                        let proto = if row.addr_display.contains(':') {
                            "tcp6"
                        } else {
                            "tcp"
                        };
                        ui.label(
                            egui::RichText::new(proto)
                                .color(value_fg)
                                .size(th.font_size_body.value())
                                .monospace(),
                        );
                    }),
                    ColumnId::Address => cell_l(ui, |ui| {
                        ui.label(
                            egui::RichText::new(&row.addr_display)
                                .color(value_fg)
                                .size(th.font_size_body.value())
                                .monospace(),
                        );
                    }),
                    ColumnId::Process => cell_l(ui, |ui| {
                        draw_process_cell(ui, th, row, props.label_pid_tooltip)
                    }),
                    ColumnId::Workspace => cell_l(ui, |ui| {
                        draw_workspace_cell(ui, th, row, props.label_external_dash)
                    }),
                    ColumnId::Tab => cell_l(ui, |ui| {
                        draw_tab_cell(ui, th, row, props.label_external_dash)
                    }),
                    ColumnId::State => cell_l(ui, |ui| {
                        draw_state_cell(ui, th, props.reduced_motion, row.state)
                    }),
                }
            },
        );

    if let Some((addr, port)) = fav_click {
        return Some(PortScannerAction::ToggleFavorite(addr, port));
    }
    if let Some(i) = output.clicked_row {
        return Some(PortScannerAction::Select(rows[i].port));
    }
    if let Some(k) = output.clicked_sort {
        return Some(PortScannerAction::SetSort(k));
    }
    None
}

/// 정렬할 수 있는 열의 정렬 키. Proto·State는 정렬하지 않는다.
fn sort_key(col: ColumnId) -> Option<SortKey> {
    match col {
        ColumnId::Port => Some(SortKey::Port),
        ColumnId::Address => Some(SortKey::Address),
        ColumnId::Process => Some(SortKey::Process),
        ColumnId::Workspace => Some(SortKey::Workspace),
        ColumnId::Tab => Some(SortKey::Tab),
        ColumnId::Proto | ColumnId::State => None,
    }
}

/// 보이는 열의 Table 폭. Process가 숨겨져 있으면 마지막 열이 남는 폭을 받는다.
fn column_widths(visible: &[ColumnId], th: &Theme) -> Vec<TableColumnWidth> {
    let mut widths: Vec<TableColumnWidth> = visible.iter().map(|c| c.shared().width(th)).collect();
    if !visible.iter().any(|c| c.shared().flex())
        && let (Some(last), Some(col)) = (widths.last_mut(), visible.last())
    {
        *last = TableColumnWidth::Flex {
            min_width: col.shared().floor(th),
        };
    }
    widths
}

/// Process 셀: process_name + PID 배지. 좁으면 이름만 말줄임하고 배지는 유지한다.
fn draw_process_cell(ui: &mut egui::Ui, th: &Theme, row: &PortRowView, pid_tooltip: &str) {
    let name = row.process_name.as_deref().unwrap_or("—");
    let pid = row.pid.map(|p| p.to_string());
    if let (Some(resp), Some(pid)) = (ports_process_cell(ui, th, name, pid.as_deref()), &pid) {
        resp.on_hover_text(pid_tooltip.replace("{pid}", pid));
    }
}

/// Workspace 셀: Tasty → workspace_name, External → dash.
fn draw_workspace_cell(ui: &mut egui::Ui, th: &Theme, row: &PortRowView, dash: &str) {
    match &row.source {
        // 글자색은 표가 정한다(쉬는 행 table-row-fg, hover 행 text-primary, 선택 행 table-row-fg-selected).
        SourceTag::Tasty { workspace_name, .. } => {
            ui.label(egui::RichText::new(workspace_name).size(th.font_size_body.value()));
        }
        SourceTag::External => {
            ui.colored_label(table_cell_ink(ui, th, th.text_muted()), dash);
        }
    }
}

/// Tab 셀: Tasty → tab_name (Some), 없거나 External → dash.
fn draw_tab_cell(ui: &mut egui::Ui, th: &Theme, row: &PortRowView, dash: &str) {
    let tab_name = match &row.source {
        SourceTag::Tasty {
            tab_name: Some(t), ..
        } => Some(t.as_str()),
        _ => None,
    };
    match tab_name {
        // 글자색은 표가 정한다(쉬는 행 table-row-fg, hover 행 text-primary, 선택 행 table-row-fg-selected).
        Some(name) => {
            ui.label(egui::RichText::new(name).size(th.font_size_body.value()));
        }
        None => {
            ui.colored_label(table_cell_ink(ui, th, th.text_muted()), dash);
        }
    }
}

/// State 셀: 공용 StatusDot 위젯 (디자인 `StatusDot status pulse label`).
///
/// `LISTEN` → running + pulse, 그 외 → waiting(정적). `reduced_motion` 이면
/// pulse 생략. 위젯이 디자인 pulse(scale 0.6→1.8, opacity 0.5→0, 1.6s)와
/// 라벨을 그린다.
fn draw_state_cell(ui: &mut egui::Ui, th: &Theme, reduced_motion: bool, state: PortState) {
    let is_listen = state.is_listen();
    let kind = if is_listen {
        StatusKind::Running
    } else {
        StatusKind::Waiting
    };
    status_dot(
        ui,
        th,
        kind,
        state.label(),
        is_listen,
        reduced_motion,
        tasty_ui_widgets::Ground::Panel,
    );
}

/// Bracketless display form. IPv6 is shown bare (e.g. wildcard `::`) per the
/// design table; brackets are added later in [`row_copy_address`] when building
/// the copyable `host:port` string.
pub(crate) fn format_addr(addr: IpAddr) -> String {
    match addr {
        IpAddr::V4(v4) if v4.is_unspecified() => "0.0.0.0".to_string(),
        IpAddr::V6(v6) if v6.is_unspecified() => "::".to_string(),
        IpAddr::V4(v4) => v4.to_string(),
        IpAddr::V6(v6) => v6.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_theme() -> Theme {
        tasty_themes::mocha_fallback()
    }

    fn dummy_row(port: u16) -> PortRowView {
        PortRowView {
            port,
            addr_display: "127.0.0.1".into(),
            pid: Some(42),
            process_name: None,
            source: SourceTag::External,
            state: PortState::Listen,
            favorited: false,
        }
    }

    /// LISTEN-only shown set referenced by the test props (matches the runtime
    /// default). A static lets `default_props` hand out a `&'a` without owning.
    static TEST_VISIBLE_STATES: std::sync::LazyLock<HashSet<PortState>> =
        std::sync::LazyLock::new(|| HashSet::from([PortState::Listen]));

    fn default_props<'a>(
        theme: &'a Theme,
        view_state: PortScannerViewState<'a>,
        query: &'a str,
        show_all_system: bool,
    ) -> PortScannerProps<'a> {
        PortScannerProps {
            theme,
            view_state,
            present_states: &[],
            reduced_motion: false,
            favorites: &[],
            filter: PortScannerFilter {
                show_all_system,
                query,
                sort_key: SortKey::Port,
                sort_dir: SortDir::Asc,
                selected_port: None,
                columns: ColumnVisibility::default(),
                visible_states: &TEST_VISIBLE_STATES,
                hidden_by_state: false,
            },
            label_heading: "Listening ports",
            label_search_placeholder: "Search…",
            label_filter_show_all_system: "Show all system ports",
            label_loading: "Scanning…",
            label_failed: "Scan failed",
            label_close: "Close",
            label_refresh: "Refresh",
            label_copy_address: "Copy address",
            label_external_dash: "—",
            label_no_ports_tasty_empty: "No Tasty ports.",
            label_no_ports_system_empty: "No system ports.",
            label_no_ports_search_zero: "No matches.",
            label_footer_loading: "Scanning…",
            label_header_tag_scanning: "scanning…",
            label_header_tag_count: "{n} listening",
            label_footer_counter: "{shown} of {total} ports",
            label_column_port: "Port",
            label_column_proto: "Proto",
            label_column_address: "Address",
            label_column_process: "Process",
            label_column_workspace: "Workspace",
            label_column_tab: "Tab",
            label_column_state: "State",
            label_columns_button: "Columns",
            label_columns_menu_title: "Show columns",
            label_state_filter: "State",
            label_state_filter_title: "Filter by state",
            label_state_filter_select_all: "Select all",
            label_state_filter_deselect_all: "Deselect all",
            label_state_filter_reset: "Reset (LISTEN only)",
            label_state_filter_apply: "Apply",
            label_no_ports_state_filtered: "No ports match the state filter.",
            label_favorites_heading: "Favorites",
            label_favorites_count: "Favorites · {n}",
            label_favorites_system_wide: "system-wide",
            label_favorites_empty: "No favorites yet — click a star in the list below to pin a port.",
            label_favorites_not_running: "not running",
            label_state_none: "NONE",
            label_favorite_add: "Add {key} to favorites",
            label_pid_tooltip: "PID {pid}",
            label_favorite_remove: "Remove {key} from favorites",
        }
    }

    fn run_view(raw: egui::RawInput, rows: &[PortRowView]) -> PortScannerAction {
        let ctx = egui::Context::default();
        let mut out = PortScannerAction::None;
        let theme = test_theme();
        drop(ctx.run(raw, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let view_state = PortScannerViewState::Ready {
                    rows,
                    total: rows.len(),
                    listening: rows.iter().filter(|r| r.state.is_listen()).count(),
                };
                let props = default_props(&theme, view_state, "", false);
                out = draw_port_scanner_view(ui, &props);
            });
        }));
        out
    }

    #[test]
    fn view_returns_none_on_empty_input() {
        let action = run_view(egui::RawInput::default(), &[]);
        assert_eq!(action, PortScannerAction::None);
    }

    #[test]
    fn view_returns_close_on_escape() {
        let mut raw = egui::RawInput::default();
        raw.events.push(egui::Event::Key {
            key: egui::Key::Escape,
            physical_key: Some(egui::Key::Escape),
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        });
        let action = run_view(raw, &[]);
        assert_eq!(action, PortScannerAction::Close);
    }

    #[test]
    fn only_the_current_scan_can_replace_the_displayed_result() {
        let ticket = Arc::new(());
        let token = Arc::downgrade(&ticket);
        let mut state = PortScanState::Loading {
            ticket,
            request: None,
            scope: ScanScope::Tasty,
        };
        let stale = Arc::new(());
        assert!(!state.accept(&Arc::downgrade(&stale), Ok(vec![dummy_row(1)])));
        assert!(state.accept(&token, Ok(vec![dummy_row(3000)])));
        assert!(matches!(&state, PortScanState::Ready { rows, .. } if rows.len() == 1));
        assert!(!state.accept(&token, Err("late failure".into())));
    }

    #[test]
    fn scope_change_triggers_reload_decision() {
        // Trigger ②: when state is Ready with a different scope, the popup
        // should choose to kick a new scan. Mirrors `need_kick` logic in
        // `draw_port_scanner_popup`.
        let state = PortScanState::Ready {
            rows: Vec::new(),
            scope: ScanScope::System,
        };
        let target = scope_from_flag(false);
        let need_kick = match &state {
            PortScanState::Idle => true,
            PortScanState::Ready { scope, .. } => *scope != target,
            _ => false,
        };
        assert!(need_kick, "scope change should force reload");
    }

    #[test]
    fn view_renders_with_entries_without_panic() {
        let rows = vec![dummy_row(3000), dummy_row(8080)];
        let action = run_view(egui::RawInput::default(), &rows);
        assert_eq!(action, PortScannerAction::None);
    }

    fn tasty_row(port: u16, ws: &str, tab: Option<&str>) -> PortRowView {
        PortRowView {
            port,
            addr_display: "0.0.0.0".into(),
            pid: Some(7),
            process_name: Some("node".into()),
            source: SourceTag::Tasty {
                workspace_name: ws.to_string(),
                tab_name: tab.map(str::to_string),
            },
            state: PortState::Listen,
            favorited: false,
        }
    }

    #[test]
    fn query_filter_matches_any_column_case_insensitive() {
        let row = tasty_row(3000, "frontend", Some("dev-server"));
        assert!(matches_query(&row, "3000"));
        assert!(matches_query(&row, "0.0.0.0"));
        assert!(matches_query(&row, "NODE"));
        assert!(matches_query(&row, "frontend"));
        assert!(matches_query(&row, "dev-server"));
        assert!(matches_query(&row, "7"));
        assert!(!matches_query(&row, "xyzzy"));
    }

    #[test]
    fn query_filter_empty_matches_everything() {
        let row = tasty_row(3000, "ws", Some("tab"));
        assert!(matches_query(&row, ""));
        assert!(matches_query(&row, "   "));
    }

    #[test]
    fn query_filter_external_row_skips_workspace_match() {
        let row = dummy_row(80);
        assert!(!matches_query(&row, "workspace"));
        assert!(matches_query(&row, "80"));
        assert!(matches_query(&row, "127.0.0.1"));
    }

    fn run_view_state(
        view_state: PortScannerViewState<'_>,
        query: &str,
        show_all_system: bool,
    ) -> PortScannerAction {
        let ctx = egui::Context::default();
        let mut out = PortScannerAction::None;
        let theme = test_theme();
        drop(ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let props = default_props(&theme, view_state, query, show_all_system);
                out = draw_port_scanner_view(ui, &props);
            });
        }));
        out
    }

    #[test]
    fn empty_ready_renders_tasty_empty_message() {
        let rows: Vec<PortRowView> = Vec::new();
        let action = run_view_state(
            PortScannerViewState::Ready {
                rows: &rows,
                total: 0,
                listening: 0,
            },
            "",
            false,
        );
        assert_eq!(action, PortScannerAction::None);
    }

    #[test]
    fn empty_ready_renders_system_empty_message() {
        let rows: Vec<PortRowView> = Vec::new();
        let action = run_view_state(
            PortScannerViewState::Ready {
                rows: &rows,
                total: 0,
                listening: 0,
            },
            "",
            true,
        );
        assert_eq!(action, PortScannerAction::None);
    }

    #[test]
    fn empty_ready_with_query_renders_search_zero_message() {
        let rows: Vec<PortRowView> = Vec::new();
        let action = run_view_state(
            PortScannerViewState::Ready {
                rows: &rows,
                total: 0,
                listening: 0,
            },
            "nginx",
            false,
        );
        assert_eq!(action, PortScannerAction::None);
    }

    #[test]
    fn loading_state_renders_without_panic() {
        let action = run_view_state(PortScannerViewState::Loading, "", false);
        assert_eq!(action, PortScannerAction::None);
    }

    fn row_with(port: u16, pid: Option<u32>, process: Option<&str>) -> PortRowView {
        PortRowView {
            port,
            addr_display: "127.0.0.1".into(),
            pid,
            process_name: process.map(str::to_string),
            source: SourceTag::External,
            state: PortState::Listen,
            favorited: false,
        }
    }

    #[test]
    fn sort_toggle_same_column_flips_direction() {
        // Mirrors the wrapper's SetSort logic — same key → flip dir.
        let mut fs = FilterState {
            sort_key: SortKey::Port,
            sort_dir: SortDir::Asc,
            ..FilterState::default()
        };
        let key = SortKey::Port;
        if fs.sort_key == key {
            fs.sort_dir = match fs.sort_dir {
                SortDir::Asc => SortDir::Desc,
                SortDir::Desc => SortDir::Asc,
            };
        } else {
            fs.sort_key = key;
            fs.sort_dir = SortDir::Asc;
        }
        assert_eq!(fs.sort_key, SortKey::Port);
        assert_eq!(fs.sort_dir, SortDir::Desc);

        let key = SortKey::Address;
        if fs.sort_key == key {
            fs.sort_dir = match fs.sort_dir {
                SortDir::Asc => SortDir::Desc,
                SortDir::Desc => SortDir::Asc,
            };
        } else {
            fs.sort_key = key;
            fs.sort_dir = SortDir::Asc;
        }
        assert_eq!(fs.sort_key, SortKey::Address);
        assert_eq!(fs.sort_dir, SortDir::Asc);
    }

    #[test]
    fn sort_puts_none_last_for_both_directions() {
        let some = row_with(3000, None, Some("nginx"));
        let none = row_with(3000, None, None);
        assert_eq!(
            compare_rows(SortKey::Process, SortDir::Asc, &some, &none),
            std::cmp::Ordering::Less,
        );
        assert_eq!(
            compare_rows(SortKey::Process, SortDir::Desc, &some, &none),
            std::cmp::Ordering::Less,
        );

        assert_eq!(
            compare_rows(SortKey::Process, SortDir::Asc, &none, &some),
            std::cmp::Ordering::Greater,
        );
        assert_eq!(
            compare_rows(SortKey::Process, SortDir::Desc, &none, &some),
            std::cmp::Ordering::Greater,
        );
    }

    #[test]
    fn sort_port_asc_then_desc() {
        let mut rows = vec![row_with(8080, None, None), row_with(80, None, None)];
        sort_rows(&mut rows, SortKey::Port, SortDir::Asc);
        assert_eq!(rows[0].port, 80);
        assert_eq!(rows[1].port, 8080);
        sort_rows(&mut rows, SortKey::Port, SortDir::Desc);
        assert_eq!(rows[0].port, 8080);
        assert_eq!(rows[1].port, 80);
    }

    #[test]
    fn sort_process_keeps_none_at_tail_with_desc() {
        let mut rows = vec![
            row_with(1, Some(1), None),
            row_with(2, Some(2), Some("alpha")),
            row_with(3, Some(3), Some("zulu")),
        ];
        sort_rows(&mut rows, SortKey::Process, SortDir::Desc);
        assert_eq!(rows[0].process_name.as_deref(), Some("zulu"));
        assert_eq!(rows[1].process_name.as_deref(), Some("alpha"));
        assert_eq!(rows[2].process_name, None);
    }

    #[test]
    fn sort_workspace_treats_external_as_missing() {
        let tasty = tasty_row(3000, "frontend", Some("tab1"));
        let external = dummy_row(3000);
        assert_eq!(
            compare_rows(SortKey::Workspace, SortDir::Asc, &tasty, &external),
            std::cmp::Ordering::Less,
        );
        assert_eq!(
            compare_rows(SortKey::Workspace, SortDir::Desc, &tasty, &external),
            std::cmp::Ordering::Less,
        );
    }

    #[test]
    fn failed_state_renders_without_panic() {
        let action = run_view_state(PortScannerViewState::Failed { message: "boom" }, "", false);
        assert_eq!(action, PortScannerAction::None);
    }

    #[test]
    fn copy_address_formats_ipv4_host_port() {
        let mut row = dummy_row(3000);
        row.addr_display = "127.0.0.1".into();
        assert_eq!(row_copy_address(&row), "127.0.0.1:3000");
    }

    #[test]
    fn copy_address_brackets_ipv6_literal() {
        let mut row = dummy_row(8080);
        row.addr_display = "::".into();
        assert_eq!(row_copy_address(&row), "[::]:8080");
    }

    #[test]
    fn header_tag_reflects_listen_count_not_total() {
        let rows = vec![dummy_row(3000)];
        let theme = test_theme();
        // Ready: tag uses the search-independent LISTEN count, not rows.len().
        let ready = default_props(
            &theme,
            PortScannerViewState::Ready {
                rows: &rows,
                total: 5,
                listening: 3,
            },
            "",
            false,
        );
        assert_eq!(header_tag_text(&ready).as_deref(), Some("3 listening"));
        let loading = default_props(&theme, PortScannerViewState::Loading, "", false);
        assert_eq!(header_tag_text(&loading).as_deref(), Some("scanning…"));
        let failed = default_props(
            &theme,
            PortScannerViewState::Failed { message: "boom" },
            "",
            false,
        );
        assert_eq!(header_tag_text(&failed), None);
    }

    #[test]
    fn column_visibility_default_shows_all() {
        let vis = ColumnVisibility::default();
        for col in ColumnId::ALL {
            assert!(vis.is_visible(col), "{col:?} should be visible by default");
        }
    }

    #[test]
    fn column_visibility_hide_then_show() {
        let mut vis = ColumnVisibility::default();
        vis.set(ColumnId::Workspace, false);
        assert!(!vis.is_visible(ColumnId::Workspace));
        assert!(vis.is_visible(ColumnId::Tab));
        assert!(vis.is_visible(ColumnId::Port));
        vis.set(ColumnId::Workspace, true);
        assert!(vis.is_visible(ColumnId::Workspace));
    }

    #[test]
    fn column_visibility_port_is_mandatory() {
        let mut vis = ColumnVisibility::default();
        vis.set(ColumnId::Port, false);
        assert!(vis.is_visible(ColumnId::Port));
        assert!(FilterState::default().columns.is_visible(ColumnId::Port));
    }

    #[test]
    fn column_widths_make_only_process_flex_and_fix_address_at_its_token() {
        let th = test_theme();
        let visible: Vec<ColumnId> = ColumnId::ALL.to_vec();
        let widths = column_widths(&visible, &th);
        for (id, w) in visible.iter().zip(&widths) {
            match (id, *w) {
                (ColumnId::Process, TableColumnWidth::Flex { min_width }) => {
                    assert_eq!(min_width, th.port_process_col_min_width());
                }
                (ColumnId::Process, _) => panic!("Process should take the spare width"),
                (ColumnId::Address, TableColumnWidth::Exact(v)) => {
                    assert_eq!(
                        v,
                        th.port_addr_col_min_width(),
                        "Address stays at its floor"
                    );
                }
                (_, TableColumnWidth::Exact(v)) => {
                    assert_eq!(v, id.shared().floor(&th), "{id:?} keeps its floor")
                }
                _ => panic!("fixed column {id:?} should have an exact width"),
            }
        }
    }

    #[test]
    fn column_widths_without_flex_columns_let_the_last_column_fill() {
        // Process가 숨겨져도 표가 가용 폭을 채우도록 마지막 열이 남는 폭을 받는다. Address는 늘지 않는다.
        let th = test_theme();
        let visible = vec![ColumnId::Port, ColumnId::Address, ColumnId::State];
        let widths = column_widths(&visible, &th);
        assert!(matches!(widths[0], TableColumnWidth::Exact(_)));
        assert!(
            matches!(widths[1], TableColumnWidth::Exact(v) if v == th.port_addr_col_min_width()),
            "Address stays at its floor"
        );
        match widths[2] {
            TableColumnWidth::Flex { min_width } => {
                assert_eq!(min_width, ColumnId::State.shared().floor(&th))
            }
            _ => panic!("the last column should take the spare width"),
        }
    }

    #[test]
    fn set_column_visible_action_round_trips_filter_state() {
        let mut fs = FilterState::default();
        fs.columns.set(ColumnId::Tab, false);
        assert!(!fs.columns.is_visible(ColumnId::Tab));
        fs.columns.set(ColumnId::Tab, true);
        assert!(fs.columns.is_visible(ColumnId::Tab));
    }

    #[test]
    fn selection_toggle_clears_on_same_port() {
        // Mirrors the wrapper's Select handling — re-selecting the same port
        // clears it; a different port replaces the selection.
        let mut fs = FilterState::default();
        assert_eq!(fs.selected_port, None);

        let toggle = |fs: &mut FilterState, port: u16| {
            fs.selected_port = if fs.selected_port == Some(port) {
                None
            } else {
                Some(port)
            };
        };

        toggle(&mut fs, 3000);
        assert_eq!(fs.selected_port, Some(3000));
        toggle(&mut fs, 3000);
        assert_eq!(fs.selected_port, None);
        toggle(&mut fs, 8080);
        assert_eq!(fs.selected_port, Some(8080));
    }

    fn row_with_state(port: u16, state: PortState) -> PortRowView {
        PortRowView {
            port,
            addr_display: "127.0.0.1".into(),
            pid: Some(1),
            process_name: None,
            source: SourceTag::External,
            state,
            favorited: false,
        }
    }

    #[test]
    fn state_filter_default_is_listen_only() {
        let fs = FilterState::default();
        assert_eq!(fs.visible_states, HashSet::from([PortState::Listen]));
    }

    #[test]
    fn state_filter_predicate_passes_only_shown_states() {
        // The wrapper filters rows with `visible_states.contains(&row.state)`.
        let listen = row_with_state(3000, PortState::Listen);
        let established = row_with_state(3001, PortState::Established);

        let default_set = HashSet::from([PortState::Listen]);
        assert!(default_set.contains(&listen.state));
        assert!(!default_set.contains(&established.state));

        let widened = HashSet::from([PortState::Listen, PortState::Established]);
        assert!(widened.contains(&listen.state));
        assert!(widened.contains(&established.state));
    }

    #[test]
    fn present_states_listen_first_then_alphabetical_deduped() {
        // Mixed + duplicated states across rows. Expected order: LISTEN first,
        // then the rest by label() alphabetical (CLOSE_WAIT < ESTABLISHED).
        let rows = vec![
            row_with_state(1, PortState::Established),
            row_with_state(2, PortState::CloseWait),
            row_with_state(3, PortState::Listen),
            row_with_state(4, PortState::Established), // dup
            row_with_state(5, PortState::Listen),      // dup
        ];
        let present = present_states(&rows);
        assert_eq!(
            present,
            vec![
                PortState::Listen,
                PortState::CloseWait,
                PortState::Established,
            ],
        );
    }

    #[test]
    fn present_states_empty_for_no_rows() {
        assert!(present_states(&[]).is_empty());
    }

    #[test]
    fn set_visible_states_action_round_trips_filter_state() {
        let mut fs = FilterState::default();
        assert_eq!(fs.visible_states, HashSet::from([PortState::Listen]));
        let set = HashSet::from([PortState::Listen, PortState::Established]);
        fs.visible_states = set.clone();
        assert_eq!(fs.visible_states, set);
    }

    #[test]
    fn format_host_port_brackets_ipv6_and_leaves_ipv4_bare() {
        assert_eq!(format_host_port("127.0.0.1", 3000), "127.0.0.1:3000");
        assert_eq!(format_host_port("::", 8080), "[::]:8080");
    }

    fn favorite(addr: &str, port: u16) -> crate::core::port_favorites::PortFavorite {
        crate::core::port_favorites::PortFavorite {
            label: format!("{addr}:{port}"),
            addr: addr.parse().unwrap(),
            port,
        }
    }

    #[test]
    fn build_favorite_rows_none_when_scan_not_ready() {
        let mut favs = PortFavorites::default();
        favs.items.push(favorite("127.0.0.1", 3000));
        let rows = build_favorite_rows(&favs.items, None);
        assert_eq!(rows.len(), 1);
        assert!(rows[0].matched.is_none());
    }

    #[test]
    fn build_favorite_rows_matches_by_addr_and_port() {
        // tasty_row() fixes addr_display to "0.0.0.0" — match on that address.
        let mut favs = PortFavorites::default();
        favs.items.push(favorite("0.0.0.0", 3000));
        favs.items.push(favorite("0.0.0.0", 9999)); // no matching scan row
        let system_rows = vec![tasty_row(3000, "frontend", Some("dev"))];
        let rows = build_favorite_rows(&favs.items, Some(&system_rows));
        assert_eq!(rows.len(), 2);
        let matched = rows[0].matched.as_ref().expect("3000 should match");
        assert_eq!(matched.process_name.as_deref(), Some("node"));
        assert_eq!(matched.workspace_name.as_deref(), Some("frontend"));
        assert!(matched.state.is_listen());
        assert!(rows[1].matched.is_none(), "9999 has no scan row → NONE");
    }

    #[test]
    fn build_favorite_rows_prefers_listen_when_multiple_matches() {
        let mut favs = PortFavorites::default();
        favs.items.push(favorite("127.0.0.1", 3000));
        let system_rows = vec![
            row_with_state(3000, PortState::Established),
            row_with_state(3000, PortState::Listen),
        ];
        let rows = build_favorite_rows(&favs.items, Some(&system_rows));
        assert!(rows[0].matched.as_ref().unwrap().state.is_listen());
    }

    #[test]
    fn view_renders_with_favorites_without_panic() {
        let theme = test_theme();
        let rows: Vec<PortRowView> = Vec::new();
        let view_state = PortScannerViewState::Ready {
            rows: &rows,
            total: 0,
            listening: 0,
        };
        let mut props = default_props(&theme, view_state, "", false);
        let favorites = vec![
            FavoriteRowView {
                addr_display: "127.0.0.1".into(),
                port: 3000,
                matched: Some(FavoriteMatch {
                    process_name: Some("node".into()),
                    pid: Some(42),
                    workspace_name: None,
                    state: PortState::Listen,
                }),
            },
            FavoriteRowView {
                addr_display: "0.0.0.0".into(),
                port: 9999,
                matched: None,
            },
        ];
        props.favorites = &favorites;
        let ctx = egui::Context::default();
        let mut out = PortScannerAction::None;
        drop(ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                out = draw_port_scanner_view(ui, &props);
            });
        }));
        assert_eq!(out, PortScannerAction::None);
    }
}
