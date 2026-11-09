//! 목록을 화면에 걸친 항목만 그리는지 세 보기 모드에서 시험한다. 화면 밖 항목으로 스크롤해
//! 고르기, 타입어헤드 이동, 스크롤해도 유지되는 이름 입력 줄도 함께 본다.

use std::collections::HashSet;
use std::path::PathBuf;

use egui::{Event, Modifiers, PointerButton, Pos2, RawInput, Rect};
use tasty_model::{ExplorerPanel, ExplorerViewMode};

const N: usize = 20_000;
const DIR: &str = "/tasty-test-no-such-dir";
const CELL: Rect = Rect::from_min_max(Pos2::ZERO, Pos2::new(1100.0, 600.0));
const MODES: [ExplorerViewMode; 3] = [
    ExplorerViewMode::Detail,
    ExplorerViewMode::List,
    ExplorerViewMode::Grid,
];

fn name(i: usize) -> String {
    format!("item{i:05}.txt")
}

fn path(i: usize) -> PathBuf {
    PathBuf::from(format!("{DIR}/{}", name(i)))
}

struct Harness {
    ctx: egui::Context,
    panel: ExplorerPanel,
    view: super::ExplorerView,
    focused: bool,
    /// 마지막 프레임이 낸 동작.
    action: Option<super::ExplorerAction>,
    /// 마지막 프레임이 그린 글자 전부.
    texts: Vec<String>,
    cell: Rect,
}

impl Harness {
    fn new(mode: ExplorerViewMode) -> Self {
        Self::with_count(mode, N)
    }

    fn with_count(mode: ExplorerViewMode, count: usize) -> Self {
        crate::i18n::init("en");
        let mut panel = ExplorerPanel::new(1, PathBuf::from(DIR));
        panel.active_tab_mut().view_mode = mode;
        let mut view = super::ExplorerView::new();
        // 만든 항목 고르기는 그 폴더를 보고 있을 때만 동작한다. 읽기를 시작해 폴더를 정하고
        // 읽기 결과 대신 항목을 직접 넣는다.
        view.sync(&panel, None);
        view.state = super::LoadState::Ok;
        view.entries = (0..count)
            .map(|i| super::DirEntryInfo {
                path: path(i),
                name: name(i),
                is_dir: false,
                size: 0,
                modified: None,
                ext: "txt".into(),
                ..Default::default()
            })
            .collect();
        Self {
            ctx: egui::Context::default(),
            panel,
            view,
            focused: false,
            action: None,
            texts: Vec::new(),
            cell: CELL,
        }
    }

    /// 한 프레임을 그리고 이 프레임에 그린 항목 이름과 그 사각형을 돌려준다.
    /// 스크롤 애니메이션이 진행되도록 프레임마다 시각을 1/60 초씩 올린다.
    fn frame(&mut self, events: Vec<Event>) -> Vec<(String, Rect)> {
        let font = crate::settings::EffectiveFont {
            font_family: String::new(),
            font_size: 13.0,
            custom_font_path: String::new(),
            line_height: 1.0,
            font_scale_mode: String::new(),
        };
        let shortcut_chars = HashSet::new();
        let input = super::ExplorerInput {
            focused: self.focused,
            overlay_open: false,
            shortcut_chars: &shortcut_chars,
        };
        let raw = RawInput {
            time: Some(self.ctx.input(|i| i.time) + 1.0 / 60.0),
            screen_rect: Some(self.cell),
            focused: true,
            events,
            ..Default::default()
        };
        let mirror = self.view.mirror_ws_id;
        let (panel, view, action) = (&self.panel, &mut self.view, &mut self.action);
        let out = self.ctx.run(raw, |ctx| {
            egui::CentralPanel::default()
                .frame(egui::Frame::NONE)
                .show(ctx, |ui| {
                    *action = super::draw_explorer(
                        ui,
                        panel,
                        view,
                        &font,
                        "virtual",
                        &[],
                        &HashSet::new(),
                        &[],
                        mirror,
                        &input,
                    );
                });
        });
        self.texts = out
            .shapes
            .iter()
            .filter_map(|clipped| match &clipped.shape {
                egui::Shape::Text(text) => Some(text.galley.text().to_owned()),
                _ => None,
            })
            .collect();
        out.shapes
            .iter()
            .filter_map(|clipped| match &clipped.shape {
                egui::Shape::Text(text) if text.galley.text().starts_with("item") => {
                    Some((text.galley.text().to_owned(), text.visual_bounding_rect()))
                }
                _ => None,
            })
            .collect()
    }

    /// 스크롤 애니메이션이 끝날 만큼 프레임을 돌린다.
    fn settle(&mut self) -> Vec<(String, Rect)> {
        let mut drawn = Vec::new();
        for _ in 0..60 {
            drawn = self.frame(Vec::new());
        }
        drawn
    }
}

fn on_screen(drawn: &[(String, Rect)], i: usize) -> Option<Rect> {
    let name = name(i);
    drawn
        .iter()
        .find(|(n, r)| *n == name && CELL.contains(r.center()))
        .map(|(_, r)| *r)
}

fn click(at: Pos2) -> Vec<Event> {
    let button = |pressed| Event::PointerButton {
        pos: at,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::default(),
    };
    vec![Event::PointerMoved(at), button(true), button(false)]
}

#[test]
fn only_the_items_on_screen_are_drawn() {
    for mode in MODES {
        let mut h = Harness::new(mode);
        h.frame(Vec::new());
        let drawn = h.frame(Vec::new());
        assert!(
            on_screen(&drawn, 0).is_some(),
            "{mode:?}: the first item drawn"
        );
        assert!(
            drawn.len() < 200,
            "{mode:?}: {} of {N} items drawn",
            drawn.len()
        );
    }
}

#[test]
fn a_far_item_is_scrolled_into_view_and_a_click_selects_it() {
    for mode in MODES {
        let mut h = Harness::new(mode);
        h.frame(Vec::new());
        h.view.reveal_created(&path(15_000));
        let drawn = h.settle();
        let rect = on_screen(&drawn, 15_000)
            .unwrap_or_else(|| panic!("{mode:?}: item 15000 not on screen"));
        assert!(drawn.len() < 200, "{mode:?}: {} items drawn", drawn.len());

        h.frame(click(rect.center()));
        assert_eq!(
            h.view.selected.owned().collect::<Vec<_>>(),
            vec![path(15_000)],
            "{mode:?}"
        );
    }
}

#[test]
fn type_ahead_reaches_an_item_far_down_the_list() {
    for mode in MODES {
        let mut h = Harness::new(mode);
        h.focused = true;
        h.frame(Vec::new());
        h.frame("item18765".chars().map(|c| Event::Text(c.into())).collect());
        assert!(h.view.selected.contains(&path(18_765)), "{mode:?}");
        let drawn = h.settle();
        assert!(
            on_screen(&drawn, 18_765).is_some(),
            "{mode:?}: not on screen"
        );
    }
}

#[test]
fn the_name_input_keeps_focus_while_the_list_scrolls_away() {
    for mode in MODES {
        let mut h = Harness::new(mode);
        h.frame(Vec::new());
        h.view.start_create(PathBuf::from(DIR), true);
        h.settle();
        let editor = h.ctx.memory(|m| m.focused());
        assert!(editor.is_some(), "{mode:?}: the name input takes focus");

        h.view.reveal_created(&path(19_000));
        let drawn = h.settle();
        assert!(
            on_screen(&drawn, 19_000).is_some(),
            "{mode:?}: scrolled away"
        );
        assert!(h.view.create.is_some(), "{mode:?}: the input stays open");
        assert_eq!(
            h.ctx.memory(|m| m.focused()),
            editor,
            "{mode:?}: focus kept"
        );
    }
}

#[test]
fn the_list_keeps_one_row_per_item_down_to_the_last() {
    for mode in [ExplorerViewMode::Detail, ExplorerViewMode::List] {
        let mut h = Harness::new(mode);
        h.frame(Vec::new());
        h.view.reveal_created(&path(N - 1));
        let drawn = h.settle();
        let last = on_screen(&drawn, N - 1).expect("last item on screen");
        let before = on_screen(&drawn, N - 2).expect("the item before it on screen");
        let pitch = last.center().y - before.center().y;
        assert!(pitch > 0.0 && pitch < 40.0, "{mode:?}: pitch {pitch}");
    }
}

#[test]
fn grid_lines_far_down_start_where_the_full_grid_would_put_them() {
    let mut h = Harness::new(ExplorerViewMode::Grid);
    h.frame(Vec::new());
    let first = h.frame(Vec::new());
    // 첫 줄은 `..` 칸과 항목들이다. 항목 0 과 같은 높이에 놓인 항목 수 + 1 이 한 줄의 칸 수다.
    let top = on_screen(&first, 0).expect("item 0").center().y;
    let cols = first
        .iter()
        .filter(|(_, r)| (r.center().y - top).abs() < 1.0)
        .count()
        + 1;
    assert!(cols > 2, "cols {cols}");

    h.view.reveal_created(&path(15_000));
    let drawn = h.settle();
    let index = |n: &str| {
        n["item".len()..n.len() - ".txt".len()]
            .parse::<usize>()
            .unwrap()
    };
    let mut lines: std::collections::BTreeMap<i64, Vec<usize>> = Default::default();
    // 첫 줄은 늘 그려 화면 위 밖에 있다. 화면 안 줄만 본다.
    for (n, r) in drawn.iter().filter(|(_, r)| CELL.contains(r.center())) {
        lines
            .entry(r.center().y.round() as i64)
            .or_default()
            .push(index(n));
    }
    assert!(lines.len() > 2, "lines: {lines:?}");
    for items in lines.values() {
        let lead = items.iter().min().copied().unwrap_or_default();
        assert_eq!(
            (lead + 1) % cols,
            0,
            "a line starts at item {lead} with {cols} columns"
        );
        assert_eq!(
            items.len(),
            cols.min(N - lead),
            "line from {lead}: {items:?}"
        );
    }
}

/// 보이는 영역 위쪽 끝(`view_top`)에 이름 글자가 걸친 항목이 그려졌는지 본다. 화면 밖에 완전히
/// 잘린 글자는 egui 가 출력에서 빼므로 행이 아니라 이름 글자가 걸치는지로 본다.
fn assert_top_edge_drawn(
    mode: ExplorerViewMode,
    drawn: &[(String, Rect)],
    view_top: f32,
    pitch: f32,
) {
    let index = |n: &str| {
        n["item".len()..n.len() - ".txt".len()]
            .parse::<usize>()
            .unwrap()
    };
    let (anchor_name, label) = drawn
        .iter()
        .filter(|(_, r)| CELL.contains(r.center()))
        .max_by(|a, b| a.1.center().y.total_cmp(&b.1.center().y))
        .expect("an item on screen");
    let anchor = index(anchor_name);
    let half = label.height() / 2.0;
    let rows_above = ((label.center().y - view_top) / pitch).ceil() as usize;
    for k in anchor.saturating_sub(rows_above)..anchor {
        let center = label.center().y - (anchor - k) as f32 * pitch;
        if center + half > view_top + 1.0 {
            assert!(
                drawn.iter().any(|(n, _)| *n == name(k)),
                "{mode:?}: the name of item {k} crosses the top edge but was not drawn"
            );
        }
    }
}

fn wheel(at: Pos2, points: f32) -> Vec<Event> {
    vec![
        Event::PointerMoved(at),
        Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: egui::vec2(0.0, -points),
            modifiers: Modifiers::default(),
        },
    ]
}

#[test]
fn the_item_at_the_top_edge_is_drawn_after_scrolling() {
    for mode in [ExplorerViewMode::Detail, ExplorerViewMode::List] {
        let mut h = Harness::new(mode);
        h.frame(Vec::new());
        let still = h.frame(Vec::new());
        let c0 = on_screen(&still, 0).expect("item 0").center().y;
        let c1 = on_screen(&still, 1).expect("item 1").center().y;
        let pitch = c1 - c0;
        // 맨 위 `..` 행부터 목록이 보인다. 스크롤해도 보이는 영역의 위쪽 끝은 그대로다.
        let view_top = c0 - pitch * 1.5;

        h.view.reveal_created(&path(15_000));
        let mut drawn = h.settle();
        assert_top_edge_drawn(mode, &drawn, view_top, pitch);
        // 행 경계와 맞지 않는 여러 스크롤 위치에서도 본다.
        let at = on_screen(&drawn, 15_000).expect("item 15000").center();
        for _ in 0..8 {
            h.frame(wheel(at, 9.0));
            drawn = h.settle();
            assert_top_edge_drawn(mode, &drawn, view_top, pitch);
        }
    }
}

#[test]
fn a_new_item_inside_a_folder_opens_right_after_that_folder() {
    for mode in MODES {
        let mut h = Harness::new(mode);
        let mut entries = h.view.entries.to_vec();
        entries[3].is_dir = true;
        h.view.set_entries(entries);
        h.frame(Vec::new());
        h.view.start_create(path(3), false);
        let drawn = h.settle();
        assert!(
            h.ctx.memory(|m| m.focused()).is_some(),
            "{mode:?}: the name input takes focus"
        );
        let at = |i| on_screen(&drawn, i).unwrap_or_else(|| panic!("{mode:?}: item {i}"));
        // 편집 줄(칸)은 폴더(3) 바로 다음에 끼어 3 과 4 사이만 벌어진다. 맨 앞(0 앞)에는 없다.
        let (before, inside) = match mode {
            ExplorerViewMode::Grid => (
                at(3).center().x - at(2).center().x,
                at(4).center().x - at(3).center().x,
            ),
            _ => (
                at(3).center().y - at(2).center().y,
                at(4).center().y - at(3).center().y,
            ),
        };
        let first = match mode {
            ExplorerViewMode::Grid => at(1).center().x - at(0).center().x,
            _ => at(1).center().y - at(0).center().y,
        };
        assert!(
            (first - before).abs() < 1.0,
            "{mode:?}: {first} vs {before}"
        );
        assert!(inside > before + 1.0, "{mode:?}: {inside} vs {before}");
    }
}

fn key(key: egui::Key) -> Vec<Event> {
    vec![Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::NONE,
    }]
}

#[test]
fn a_remote_name_stays_read_only_in_place_until_the_server_answers() {
    for mode in MODES {
        let mut h = Harness::new(mode);
        h.view.mirror_ws_id = Some(7);
        h.frame(Vec::new());
        h.view.start_create(PathBuf::from(DIR), true);
        h.settle();
        h.frame(key(egui::Key::Enter));
        assert!(
            matches!(&h.action, Some(super::ExplorerAction::Create { name, .. }) if name == "New folder"),
            "{mode:?}: {:?}",
            h.action
        );
        let waiting = |h: &Harness| h.view.create.as_ref().and_then(|c| c.waiting_path());
        assert_eq!(
            waiting(&h),
            Some(PathBuf::from(DIR).join("New folder")),
            "{mode:?}: the row stays and waits"
        );
        // 기다리는 동안 Esc·포커스 잃음·글자 입력은 줄을 닫거나 바꾸지 않는다.
        h.frame(key(egui::Key::Escape));
        h.frame(click(Pos2::new(1000.0, 500.0)));
        h.frame(vec![Event::Text("x".into())]);
        h.settle();
        assert!(waiting(&h).is_some(), "{mode:?}: still waiting");
        assert_eq!(h.view.create.as_ref().unwrap().edit.buf, "New folder");

        // 실패하면 글자를 둔 채 다시 편집한다.
        h.view.create.as_mut().unwrap().resume_editing();
        h.settle();
        h.frame(vec![Event::Text("x".into())]);
        assert_eq!(
            h.view
                .create
                .as_ref()
                .map(|c| (c.waiting, c.edit.buf.as_str())),
            Some((false, "New folderx")),
            "{mode:?}"
        );
    }
}

#[test]
fn a_local_name_closes_the_row_on_enter() {
    let mut h = Harness::new(ExplorerViewMode::Detail);
    h.frame(Vec::new());
    h.view.start_create(PathBuf::from(DIR), true);
    h.settle();
    h.frame(key(egui::Key::Enter));
    assert!(matches!(
        h.action,
        Some(super::ExplorerAction::Create { .. })
    ));
    assert!(h.view.create.is_none());
}

#[path = "virtual_tests/keyboard_select.rs"]
mod keyboard_select;

#[path = "virtual_tests/empty_create.rs"]
mod empty_create;

#[path = "virtual_tests/drop_rows.rs"]
mod drop_rows;

#[test]
fn internal_tabs_restore_scroll_in_all_three_views() {
    for mode in MODES {
        let mut h = Harness::new(mode);
        h.frame(Vec::new());
        h.view.reveal_created(&path(15_000));
        let before = h.settle();
        let before_rect = on_screen(&before, 15_000).expect("A scrolled");
        let entries = h.view.entries.to_vec();
        h.panel.add_tab();
        h.panel.active_tab_mut().view_mode = mode;
        h.view.sync(&h.panel, None);
        h.view.set_entries(entries);
        h.view.state = super::LoadState::Ok;
        let other = h.settle();
        assert!(on_screen(&other, 0).is_some(), "{mode:?}: B starts at top");
        assert!(h.view.selected.is_empty());
        h.panel.active = 0;
        h.view.sync(&h.panel, None);
        let after = h.settle();
        let after_rect = on_screen(&after, 15_000).expect("A restored scroll");
        assert!(
            (before_rect.top() - after_rect.top()).abs() < 1.0,
            "{mode:?}"
        );
        assert!(h.view.selected.contains(&path(15_000)));
        assert!(after.len() < 200);
    }
}

#[path = "virtual_tests/large.rs"]
mod large;
