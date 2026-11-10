//! 포커스된 서피스의 정보와 팔레트·테마 버튼을 그리는 상태바.
//! 원격 위치·브랜치·서피스 ID·셸·그리드는 표시 전용이며 값이 없으면 해당 항목과 간격도 생략한다.
//! 폭이 부족하면 그리드, 셸, 서피스 ID, 팔레트 키캡, 브랜치 이름, 원격 호스트 글자 순서로 숨긴다.
//! 원격 글리프·브랜치 아이콘·테마 아이콘은 마지막에도 남으며 더 좁으면 잘릴 수 있다.
//! 배치할 Ui와 번역 문구는 호출자가 제공한다.

use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;

use crate::chip::{kbd, kbd_width};

// 디자인의 바깥 여백과 항목 사이 간격. 각 셀의 패딩이 아니므로 중복해서 더하지 않는다.
const BAR_PAD_X: LogicalPx = LogicalPx(10.0);
const ITEM_GAP: LogicalPx = LogicalPx(10.0);

/// 브랜치 아이콘과 이름을 합친 최대 폭. 대응하는 컴포넌트 토큰이 없어 별도로 둔다.
const BRANCH_MAX_W: LogicalPx = LogicalPx(160.0);

/// 좌측 클러스터의 항목 수 — [`items_at`] 배열의 앞쪽 몇 칸이 좌측인지.
const LEFT_COUNT: usize = 5;

/// 마지막 축소 단계. 이보다 더 접을 것이 없다(테마 글리프는 안 빠진다).
const MAX_DROP_LEVEL: u8 = 6;

/// 포커스 surface 가 mirror workspace 에 있을 때 좌측 맨 앞에 두는 원격 위치 항목.
#[derive(Clone, Debug, Default)]
pub struct StatusBarRemote {
    /// 호스트 글자(attach 프로필 이름). 비면 `empty_label` 을 font-label 로 그린다.
    pub host: String,
    /// 연결이 끊겨 다시 연결하는 중이다. 원격 색을 빼고 `reconnecting_label` 을 붙인다.
    pub reconnecting: bool,
    /// 호스트 글자가 없을 때 그 자리에 쓰는 낱말.
    pub empty_label: String,
    /// 재연결 중 상태어.
    pub reconnecting_label: String,
    /// hover tooltip 첫 줄. 둘째 줄은 생략 없는 호스트 글자이며 비면 첫 줄만 보인다.
    pub tooltip: String,
}

/// view 입력 — 한 프레임 분의 StatusBar 표시 데이터.
#[derive(Clone, Debug, Default)]
pub struct StatusBarData {
    /// mirror surface 의 원격 위치. None 이면 항목을 생략한다(로컬 surface).
    pub remote: Option<StatusBarRemote>,
    /// 브랜치명 또는 detached HEAD 표시를 호출자가 완성해 전달한다. None이면 항목을 생략한다.
    pub branch: Option<String>,
    /// focus surface id(숫자). "Copy Terminal ID" 가 복사하는 값과 동일.
    pub surface_id: Option<u32>,
    /// 그 surface 를 담은 pane id. 있으면 `s<sid>·p<pane>`, 없으면 `s<sid>` 로 찍는다.
    pub pane_id: Option<u32>,
    /// 셸/포그라운드 프로세스명(terminal 한정).
    pub shell: Option<String>,
    /// 그리드 크기 (cols, rows) (terminal 한정).
    pub grid: Option<(usize, usize)>,
    /// 현재 테마가 light 인지 — `sun`(light) / `theme`(dark) 글리프를 가른다.
    pub theme_is_light: bool,
    /// 키캡으로 그릴 팔레트 단축키. 비어 있으면 항목을 생략한다.
    pub palette_keys: String,
    /// 팔레트 키캡 hover tooltip.
    pub palette_tooltip: String,
    /// 테마 글리프 hover tooltip.
    pub theme_tooltip: String,
}

/// view 가 보고하는 사용자 클릭 액션.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StatusBarAction {
    /// 팔레트 칩 클릭 → 커맨드 팔레트 토글.
    OpenPalette,
    /// 테마 토글 클릭 → latte ↔ mocha 전환.
    ToggleTheme,
}

/// [`draw_status_bar_view`] 의 반환값 — 클릭 액션 + 가장자리 리사이즈 우선권 판정.
#[derive(Clone, Debug, Default)]
pub struct StatusBarDrawResult {
    pub actions: Vec<StatusBarAction>,
    /// 마우스가 상태바의 실제 클릭 가능 요소(팔레트 칩·테마 토글) 위인지.
    /// 본체에서 `AppState.resize_edge_widget_hovered` 에 OR 로 합성된다.
    pub resize_priority_hovered: bool,
}

/// 전달된 Ui에서 영역을 할당해 상태바를 그리고 클릭 결과를 반환한다.
/// 화면 원점이 아닌 할당 영역의 좌표를 사용한다.
pub fn draw_status_bar_view(
    ui: &mut egui::Ui,
    th: &Theme,
    width: LogicalPx,
    data: &StatusBarData,
) -> StatusBarDrawResult {
    let mut actions = Vec::new();
    let mut resize_priority_hovered = false;
    // 서피스 ID와 그리드는 고정폭, 브랜치와 셸 이름은 UI 글꼴을 사용한다.
    let mono = egui::FontId::monospace(th.font_size_caption.value());
    let text = egui::FontId::proportional(th.font_size_caption.value());
    let muted: egui::Color32 = th.text_muted().into();
    let hover: egui::Color32 = th.text_secondary().into();
    let item_glyph: egui::Color32 = th.statusbar_glyph().into();
    // 밝은 테마는 sun, 어두운 테마는 theme 아이콘으로 구분한다.
    let theme_glyph_tint: egui::Color32 = th.statusbar_theme_glyph().into();
    let theme_glyph = if data.theme_is_light {
        tasty_icons::SUN
    } else {
        tasty_icons::THEME
    };
    let glyph_size = th.statusbar_glyph_size();
    // 시안 Workspace status bar 의 바 채움은 bg-sidebar 다.
    let bg: egui::Color32 = th.bg_sidebar().into();
    let bar_h = th.status_bar_height;

    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(width.value(), bar_h.value()),
        egui::Sense::hover(),
    );
    ui.painter().rect_filled(rect, 0.0, bg);
    ui.painter().hline(
        rect.x_range(),
        rect.top(),
        egui::Stroke::new(
            th.border_width.value(),
            th.separator.to_egui_premultiplied(),
        ),
    );

    let mut bar = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    bar.spacing_mut().item_spacing.x = 0.0;

    let widths = measure_items(&bar, th, &text, &mono, glyph_size, data);
    let level = drop_level(&widths, width);
    let shown = items_at(&widths, level);

    let left_w = cluster_width(&shown[..LEFT_COUNT]).unwrap_or_default();
    let right_w = cluster_width(&shown[LEFT_COUNT..]).unwrap_or_default();
    let spacer = (width - BAR_PAD_X * 2.0 - left_w - right_w).max(LogicalPx::default());

    bar.add_space(BAR_PAD_X.value());

    // ── 좌측 클러스터 (읽기 전용) ──
    let mut drawn = 0usize;
    if let Some(remote) = &data.remote
        && shown[0].is_some()
    {
        gap_before(&mut bar, &mut drawn);
        // 마지막 단계에서는 호스트 글자만 숨긴다. 글리프와 재연결 상태어는 남는다.
        let style = RemoteCellStyle::of(th, glyph_size, remote.reconnecting);
        remote_cell(&mut bar, bar_h, &style, remote, level < MAX_DROP_LEVEL);
    }
    if let Some(branch) = &data.branch
        && shown[1].is_some()
    {
        gap_before(&mut bar, &mut drawn);
        // 이름을 숨겨도 저장소 안에 있다는 아이콘은 남긴다.
        let name = (level < BRANCH_TEXT_DROP).then_some(branch.as_str());
        branch_cell(
            &mut bar,
            bar_h,
            &text,
            muted,
            &BranchCellStyle {
                glyph_size,
                gap: th.spacing_xs,
                tint: item_glyph,
            },
            name,
        );
    }
    if shown[2].is_some() {
        gap_before(&mut bar, &mut drawn);
        text_cell(&mut bar, bar_h, &mono, muted, &surface_label(data), None);
    }
    if let Some(shell) = &data.shell
        && shown[3].is_some()
    {
        gap_before(&mut bar, &mut drawn);
        text_cell(&mut bar, bar_h, &text, muted, shell, None);
    }
    if let Some(grid) = data.grid
        && shown[4].is_some()
    {
        gap_before(&mut bar, &mut drawn);
        text_cell(&mut bar, bar_h, &mono, muted, &grid_label(grid), None);
    }

    bar.add_space(spacer.value());

    // 좌우 그룹 사이 간격은 spacer가 담당한다.
    drawn = 0;
    if shown[5].is_some() {
        gap_before(&mut bar, &mut drawn);
        let resp =
            kbd_cell(&mut bar, th, bar_h, &data.palette_keys).on_hover_text(&data.palette_tooltip);
        resize_priority_hovered |= resp.hovered();
        if resp.clicked() {
            actions.push(StatusBarAction::OpenPalette);
        }
    }
    gap_before(&mut bar, &mut drawn);
    let resp = glyph_button_cell(
        &mut bar,
        bar_h,
        theme_glyph,
        glyph_size,
        theme_glyph_tint,
        hover,
    )
    .on_hover_text(&data.theme_tooltip);
    resize_priority_hovered |= resp.hovered();
    if resp.clicked() {
        actions.push(StatusBarAction::ToggleTheme);
    }

    StatusBarDrawResult {
        actions,
        resize_priority_hovered,
    }
}

/// surface id 표기 — pane 을 알면 `s3·p1`, 모르면 `s3`.
fn surface_label(data: &StatusBarData) -> String {
    match (data.surface_id, data.pane_id) {
        (Some(sid), Some(pane)) => format!("s{sid}·p{pane}"),
        (Some(sid), None) => format!("s{sid}"),
        _ => String::new(),
    }
}

/// grid 표기 — 디자인의 `120×32`.
fn grid_label((cols, rows): (usize, usize)) -> String {
    format!("{cols}×{rows}")
}

/// 그리기 전에 축소 단계를 선택하기 위한 항목별 폭. None인 항목은 간격도 차지하지 않는다.
/// branch는 이름을 포함한 폭, branch_glyph는 이름을 숨겼을 때의 폭이다.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct ItemWidths {
    /// 원격 항목 전체 폭과 호스트 글자를 숨긴 폭.
    remote: Option<LogicalPx>,
    remote_floor: Option<LogicalPx>,
    branch: Option<LogicalPx>,
    branch_glyph: Option<LogicalPx>,
    surface: Option<LogicalPx>,
    shell: Option<LogicalPx>,
    grid: Option<LogicalPx>,
    palette: Option<LogicalPx>,
    theme: LogicalPx,
}

/// 브랜치 이름을 숨기는 축소 단계. 그다음 단계([`MAX_DROP_LEVEL`])가 원격 호스트 글자를 숨긴다.
const BRANCH_TEXT_DROP: u8 = 5;

/// 축소 단계 `level` 에서 각 항목이 실제로 차지하는 폭. 순서는 디자인의 배치 순서와
/// 같다 — 원격 위치 · 브랜치 · surface id · shell · grid · 팔레트 키캡 · 테마 글리프.
fn items_at(w: &ItemWidths, level: u8) -> [Option<LogicalPx>; 7] {
    [
        if level >= MAX_DROP_LEVEL {
            w.remote_floor
        } else {
            w.remote
        },
        if level >= BRANCH_TEXT_DROP {
            w.branch_glyph
        } else {
            w.branch
        },
        (level < 3).then_some(w.surface).flatten(),
        (level < 2).then_some(w.shell).flatten(),
        (level < 1).then_some(w.grid).flatten(),
        (level < 4).then_some(w.palette).flatten(),
        Some(w.theme),
    ]
}

/// 폭의 합 — `LogicalPx` 는 `Sum` 을 구현하지 않아 접어서 더한다.
fn sum(widths: impl Iterator<Item = LogicalPx>) -> LogicalPx {
    widths.fold(LogicalPx::default(), |acc, w| acc + w)
}

/// 한 클러스터의 폭 — 보이는 항목의 합 + 그 사이 gap. **없는 항목은 gap 도 안 든다.**
fn cluster_width(items: &[Option<LogicalPx>]) -> Option<LogicalPx> {
    let shown = items.iter().flatten().count();
    if shown == 0 {
        return None;
    }
    Some(sum(items.iter().flatten().copied()) + ITEM_GAP * (shown - 1) as f32)
}

/// `level` 에서 바 전체가 요구하는 최소 폭(바깥 여백 + 항목 + 그 사이 gap).
/// 좌우 클러스터 **둘 다** 있으면 그 사이 gap 도 한 번 든다 — 그 자리가 flex spacer
/// 이고, 최소가 gap 이다.
fn total_at(w: &ItemWidths, level: u8) -> LogicalPx {
    let items = items_at(w, level);
    let left = cluster_width(&items[..LEFT_COUNT]);
    let right = cluster_width(&items[LEFT_COUNT..]);
    let between = if left.is_some() && right.is_some() {
        Some(ITEM_GAP)
    } else {
        None
    };
    sum([left, right, between].into_iter().flatten()) + BAR_PAD_X * 2.0
}

/// 주어진 폭에 **들어가는 가장 작은** 축소 단계. 어느 단계도 안 들어가면 마지막 단계
/// 에서 멈춘다 — 더 접을 것이 없고, 그때는 테마 글리프가 잘릴지언정 빠지지는 않는다.
fn drop_level(w: &ItemWidths, available: LogicalPx) -> u8 {
    (0..=MAX_DROP_LEVEL)
        .find(|&l| total_at(w, l) <= available)
        .unwrap_or(MAX_DROP_LEVEL)
}

/// 항목 사이 gap — 첫 항목 앞에는 안 넣는다.
fn gap_before(ui: &mut egui::Ui, drawn: &mut usize) {
    if *drawn > 0 {
        ui.add_space(ITEM_GAP.value());
    }
    *drawn += 1;
}

/// 한 줄 텍스트의 galley. `max_w` 가 있으면 그 폭에서 말줄임(`…`)한다.
fn laid(
    ui: &egui::Ui,
    text: &str,
    font: &egui::FontId,
    max_w: Option<LogicalPx>,
) -> std::sync::Arc<egui::Galley> {
    let mut job = egui::text::LayoutJob::simple_singleline(
        text.to_owned(),
        font.clone(),
        egui::Color32::PLACEHOLDER,
    );
    if let Some(max_w) = max_w {
        job.wrap = egui::text::TextWrapping {
            max_width: max_w.value(),
            max_rows: 1,
            break_anywhere: true,
            overflow_character: Some('…'),
        };
    }
    ui.fonts(|f| f.layout_job(job))
}

/// 텍스트만 있는 셀(비클릭). 패딩은 바 바깥 여백이 들고 여기는 글자 폭만 차지한다.
fn text_cell(
    ui: &mut egui::Ui,
    h: LogicalPx,
    font: &egui::FontId,
    color: egui::Color32,
    text: &str,
    max_w: Option<LogicalPx>,
) {
    let galley = laid(ui, text, font, max_w);
    let w = galley.rect.width();
    let (r, _) = ui.allocate_exact_size(egui::vec2(w, h.value()), egui::Sense::hover());
    let pos = egui::pos2(r.left(), r.center().y - galley.rect.height() / 2.0);
    ui.painter().galley(pos, galley, color);
}

/// 브랜치 셀 — 글리프(+ 이름). 이름이 `None` 이면 글리프만(축소 마지막 단계).
fn branch_cell(
    ui: &mut egui::Ui,
    h: LogicalPx,
    font: &egui::FontId,
    color: egui::Color32,
    style: &BranchCellStyle,
    name: Option<&str>,
) {
    let gs = style.glyph_size.value();
    let name_galley = name.map(|n| {
        laid(
            ui,
            n,
            font,
            Some(branch_name_budget(style.glyph_size, style.gap)),
        )
    });
    let gap = if name_galley.is_some() {
        style.gap
    } else {
        LogicalPx::default()
    };
    let text_w = name_galley.as_ref().map_or(0.0, |g| g.rect.width());
    let w = gs + gap.value() + text_w;
    let (r, _) = ui.allocate_exact_size(egui::vec2(w, h.value()), egui::Sense::hover());
    let grect = egui::Rect::from_center_size(
        egui::pos2(r.left() + gs / 2.0, r.center().y),
        egui::Vec2::splat(gs),
    );
    tasty_icons::GIT_BRANCH
        .image(gs, style.tint)
        .paint_at(ui, grect);
    if let Some(galley) = name_galley {
        let pos = egui::pos2(
            r.left() + gs + gap.value(),
            r.center().y - galley.rect.height() / 2.0,
        );
        ui.painter().galley(pos, galley, color);
    }
}

/// 브랜치 아이콘의 간격과 색상.
struct BranchCellStyle {
    glyph_size: LogicalPx,
    /// 글리프와 이름 사이 gap(`space-xs`).
    gap: LogicalPx,
    /// 글리프 tint(`statusbar-glyph`) — hover 와 무관하게 고정이다(비클릭 항목).
    tint: egui::Color32,
}

/// 이름에 남는 폭 — 항목 상한에서 글리프와 그 gap 을 뺀 나머지.
fn branch_name_budget(glyph_size: LogicalPx, gap: LogicalPx) -> LogicalPx {
    (BRANCH_MAX_W - glyph_size - gap).max(LogicalPx::default())
}

/// 원격 항목의 글꼴·색·간격. 연결됨은 원격 색, 재연결 중은 바의 기본 잉크다.
struct RemoteCellStyle {
    glyph_size: LogicalPx,
    gap: LogicalPx,
    host_max_w: LogicalPx,
    glyph: egui::Color32,
    host: egui::Color32,
    word: egui::Color32,
    mono: egui::FontId,
    label: egui::FontId,
}

impl RemoteCellStyle {
    fn of(th: &Theme, glyph_size: LogicalPx, reconnecting: bool) -> Self {
        let (glyph, host) = if reconnecting {
            (th.statusbar_glyph().into(), th.text_muted().into())
        } else {
            let remote: egui::Color32 = th.statusbar_remote_fg().into();
            (remote, remote)
        };
        Self {
            glyph_size,
            gap: th.statusbar_remote_gap(),
            host_max_w: th.statusbar_remote_max_width(),
            glyph,
            host,
            word: th.text_muted().into(),
            mono: egui::FontId::monospace(th.font_size_caption.value()),
            label: crate::label_font(th.font_size_caption.value()),
        }
    }
}

/// 호스트 자리의 galley — 호스트 글자는 mono, 비었을 때의 낱말은 font-label. 상한에서 끝을 줄인다.
fn remote_host_galley(
    ui: &egui::Ui,
    style: &RemoteCellStyle,
    remote: &StatusBarRemote,
) -> std::sync::Arc<egui::Galley> {
    let (text, font) = if remote.host.is_empty() {
        (remote.empty_label.as_str(), &style.label)
    } else {
        (remote.host.as_str(), &style.mono)
    };
    laid(ui, text, font, Some(style.host_max_w))
}

/// 원격 항목의 조각 — 글리프, (호스트), (재연결 상태어).
fn remote_parts(
    ui: &egui::Ui,
    style: &RemoteCellStyle,
    remote: &StatusBarRemote,
    with_host: bool,
) -> (
    Option<std::sync::Arc<egui::Galley>>,
    Option<std::sync::Arc<egui::Galley>>,
) {
    let host = with_host.then(|| remote_host_galley(ui, style, remote));
    let word = remote
        .reconnecting
        .then(|| laid(ui, &remote.reconnecting_label, &style.label, None));
    (host, word)
}

/// 조각 폭의 합. 조각 사이에는 `statusbar-remote-gap` 이 든다.
fn remote_parts_width(
    style: &RemoteCellStyle,
    host: Option<&std::sync::Arc<egui::Galley>>,
    word: Option<&std::sync::Arc<egui::Galley>>,
) -> LogicalPx {
    [host, word]
        .into_iter()
        .flatten()
        .fold(style.glyph_size, |acc, g| {
            acc + style.gap + LogicalPx(g.rect.width())
        })
}

fn remote_widths(
    ui: &egui::Ui,
    style: &RemoteCellStyle,
    remote: &StatusBarRemote,
    with_host: bool,
) -> LogicalPx {
    let (host, word) = remote_parts(ui, style, remote, with_host);
    remote_parts_width(style, host.as_ref(), word.as_ref())
}

/// 원격 항목 — 글리프 + 호스트 (+ 재연결 상태어). 읽기 전용이며 hover 하면 전체 호스트를 보인다.
fn remote_cell(
    ui: &mut egui::Ui,
    h: LogicalPx,
    style: &RemoteCellStyle,
    remote: &StatusBarRemote,
    with_host: bool,
) {
    let (host, word) = remote_parts(ui, style, remote, with_host);
    let w = remote_parts_width(style, host.as_ref(), word.as_ref());
    let (r, resp) = ui.allocate_exact_size(egui::vec2(w.value(), h.value()), egui::Sense::hover());
    let gs = style.glyph_size.value();
    let grect = egui::Rect::from_center_size(
        egui::pos2(r.left() + gs / 2.0, r.center().y),
        egui::Vec2::splat(gs),
    );
    tasty_icons::REMOTE
        .image(gs, style.glyph)
        .paint_at(ui, grect);
    let mut x = r.left() + gs;
    for (galley, color) in [(host, style.host), (word, style.word)] {
        let Some(galley) = galley else { continue };
        x += style.gap.value();
        let pos = egui::pos2(x, r.center().y - galley.rect.height() / 2.0);
        x += galley.rect.width();
        ui.painter().galley(pos, galley, color);
    }
    resp.on_hover_ui(|ui| {
        ui.label(&remote.tooltip);
        if !remote.host.is_empty() {
            ui.label(egui::RichText::new(&remote.host).font(style.mono.clone()));
        }
    });
}

/// 팔레트 키캡 셀 — [`crate::kbd`] 를 셀 안에 그리고 클릭을 받는다.
fn kbd_cell(ui: &mut egui::Ui, th: &Theme, h: LogicalPx, keys: &str) -> egui::Response {
    let w = kbd_width(ui.ctx(), th, keys);
    let (r, resp) = ui.allocate_exact_size(egui::vec2(w.value(), h.value()), egui::Sense::click());
    let mut inner = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(r)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    kbd(&mut inner, th, keys, crate::Ground::Panel);
    resp
}

/// 글리프만 있는 버튼 셀(클릭 + hover 밝아짐).
fn glyph_button_cell(
    ui: &mut egui::Ui,
    h: LogicalPx,
    glyph: tasty_icons::Icon,
    glyph_size: LogicalPx,
    tint: egui::Color32,
    hover: egui::Color32,
) -> egui::Response {
    let gs = glyph_size.value();
    let (r, resp) = ui.allocate_exact_size(egui::vec2(gs, h.value()), egui::Sense::click());
    let c = if resp.hovered() { hover } else { tint };
    let grect = egui::Rect::from_center_size(r.center(), egui::Vec2::splat(gs));
    glyph.image(gs, c).paint_at(ui, grect);
    resp
}

/// 한 프레임의 항목 폭을 전부 잰다 — 그리기와 **같은 galley·같은 토큰**으로 센다.
/// 값이 없는 항목은 `None` 이다(0 이 아니다 — [`ItemWidths`] 참조).
fn measure_items(
    ui: &egui::Ui,
    th: &Theme,
    text: &egui::FontId,
    mono: &egui::FontId,
    glyph_size: LogicalPx,
    data: &StatusBarData,
) -> ItemWidths {
    let measure = |s: &str, font: &egui::FontId, max_w: Option<LogicalPx>| {
        LogicalPx(laid(ui, s, font, max_w).rect.width())
    };
    let gap = th.spacing_xs;
    let remote = data.remote.as_ref().map(|remote| {
        let style = RemoteCellStyle::of(th, glyph_size, remote.reconnecting);
        (
            remote_widths(ui, &style, remote, true),
            remote_widths(ui, &style, remote, false),
        )
    });
    ItemWidths {
        remote: remote.map(|(full, _)| full),
        remote_floor: remote.map(|(_, floor)| floor),
        branch: data.branch.as_ref().map(|name| {
            glyph_size + gap + measure(name, text, Some(branch_name_budget(glyph_size, gap)))
        }),
        branch_glyph: data.branch.as_ref().map(|_| glyph_size),
        surface: data
            .surface_id
            .map(|_| measure(&surface_label(data), mono, None)),
        shell: data.shell.as_ref().map(|s| measure(s, text, None)),
        grid: data.grid.map(|g| measure(&grid_label(g), mono, None)),
        palette: (!data.palette_keys.is_empty())
            .then(|| kbd_width(ui.ctx(), th, &data.palette_keys)),
        theme: glyph_size,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 서로 다른 합성 폭으로 각 단계에서 남는 항목을 확인한다.
    fn full() -> ItemWidths {
        ItemWidths {
            remote: Some(LogicalPx(126.0)),
            remote_floor: Some(LogicalPx(12.0)),
            branch: Some(LogicalPx(100.0)),
            branch_glyph: Some(LogicalPx(13.0)),
            surface: Some(LogicalPx(41.0)),
            shell: Some(LogicalPx(31.0)),
            grid: Some(LogicalPx(45.0)),
            palette: Some(LogicalPx(61.0)),
            theme: LogicalPx(13.0),
        }
    }

    #[test]
    fn the_drop_order_is_grid_shell_surface_palette_branch_text_then_host_text() {
        let w = full();
        // 단계마다 **직전 단계에서 하나만** 사라진다.
        let seq: Vec<[Option<LogicalPx>; 7]> =
            (0..=MAX_DROP_LEVEL).map(|l| items_at(&w, l)).collect();
        assert_eq!(seq[0][4], w.grid, "level 0 은 아무것도 안 접는다");
        assert_eq!(seq[1][4], None, "1: grid");
        assert_eq!(seq[2][3], None, "2: shell");
        assert_eq!(seq[3][2], None, "3: surface id");
        assert_eq!(seq[4][5], None, "4: palette");
        assert_eq!(seq[5][1], w.branch_glyph, "5: 브랜치는 글리프만 남는다");
        assert_eq!(seq[5][0], w.remote, "5: 원격 호스트는 아직 남는다");
        assert_eq!(seq[6][0], w.remote_floor, "6: 원격은 글리프만 남는다");
    }

    #[test]
    fn the_theme_glyph_never_drops() {
        let w = full();
        for level in 0..=MAX_DROP_LEVEL {
            assert_eq!(
                items_at(&w, level)[6],
                Some(w.theme),
                "level {level} 에서 테마 글리프가 빠졌다"
            );
        }
    }

    #[test]
    fn a_value_less_item_takes_no_slot_and_no_gap() {
        // 브랜치가 없는 상태 — 폭은 브랜치 있는 상태보다 **항목 폭 + gap 만큼** 작다.
        let with = full();
        let without = ItemWidths {
            branch: None,
            branch_glyph: None,
            ..with
        };
        let delta = total_at(&with, 0) - total_at(&without, 0);
        assert_eq!(delta, with.branch.unwrap() + ITEM_GAP);
    }

    #[test]
    fn the_level_is_the_smallest_one_that_fits() {
        let w = full();
        // 딱 맞는 폭에서는 그 단계가 고른다.
        for level in 0..=MAX_DROP_LEVEL {
            let exact = total_at(&w, level);
            assert_eq!(drop_level(&w, exact), level, "level {level} 이 딱 맞는 폭");
        }
    }

    #[test]
    fn below_the_floor_it_stops_at_the_last_level() {
        let w = full();
        let floor = total_at(&w, MAX_DROP_LEVEL);
        assert_eq!(drop_level(&w, floor - LogicalPx(0.5)), MAX_DROP_LEVEL);
        assert_eq!(drop_level(&w, LogicalPx::default()), MAX_DROP_LEVEL);
    }

    #[test]
    fn the_surface_label_names_the_pane_when_it_knows_it() {
        let mut data = StatusBarData {
            surface_id: Some(3),
            pane_id: Some(1),
            ..Default::default()
        };
        assert_eq!(surface_label(&data), "s3·p1");
        data.pane_id = None;
        assert_eq!(surface_label(&data), "s3");
    }
}
