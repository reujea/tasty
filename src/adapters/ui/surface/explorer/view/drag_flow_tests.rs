//! 실제 egui 입력으로 끌기를 놓기까지 돌린다. 놓기는 한 번만 요청되고, Esc 는 그 누름의 끌기를 끝내며,
//! 링크로 다른 이름을 가진 같은 폴더도 이름이 같은 폴더처럼 판정한다.

use super::*;
use crate::adapters::ui::surface::explorer::ExplorerAction;

/// 시험이 행 잉크로 칠하는 선의 색. 틴트와 링 사이의 순서를 재는 표지다.
const ROW_INK: egui::Color32 = egui::Color32::MAGENTA;

/// 칸에 그릴 항목: 경로, 폴더 여부, 자리.
type Items = Vec<(PathBuf, bool, egui::Rect)>;

struct Pane {
    view: ExplorerView,
    body: egui::Rect,
    items: Items,
}

struct Harness {
    ctx: egui::Context,
    time: f64,
    panes: Vec<Pane>,
    drops: Vec<(usize, OpKind, Vec<PathBuf>, PathBuf)>,
    mods: egui::Modifiers,
    /// 마지막 프레임이 칠한 모양.
    shapes: Vec<egui::Shape>,
}

fn cell(x: f32, y: f32) -> egui::Rect {
    egui::Rect::from_min_size(egui::pos2(x, y), egui::vec2(120.0, 24.0))
}

fn moved(pos: egui::Pos2) -> egui::Event {
    egui::Event::PointerMoved(pos)
}

fn button(pos: egui::Pos2, pressed: bool) -> egui::Event {
    egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: Default::default(),
    }
}

fn escape() -> egui::Event {
    egui::Event::Key {
        key: egui::Key::Escape,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Default::default(),
    }
}

impl Harness {
    fn new(panes: Vec<(egui::Rect, Items)>) -> Self {
        Self {
            ctx: egui::Context::default(),
            time: 0.0,
            panes: panes
                .into_iter()
                .map(|(body, items)| Pane {
                    view: ExplorerView::new(),
                    body,
                    items,
                })
                .collect(),
            drops: Vec::new(),
            mods: egui::Modifiers::default(),
            shapes: Vec::new(),
        }
    }

    /// 한 프레임. `discard` 면 egui 가 같은 프레임을 한 pass 더 돈다.
    fn step(&mut self, events: Vec<egui::Event>, discard: bool) {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(800.0, 600.0),
            )),
            time: Some(self.time),
            modifiers: self.mods,
            events,
            ..Default::default()
        };
        self.time += 1.0 / 60.0;
        let theme = Theme::with_colors_and_zoom(tasty_themes::mocha_fallback_colors(), false, 1.0);
        let panes = &mut self.panes;
        let drops = &mut self.drops;
        let mut discard = discard;
        let out = self.ctx.run(input, |ctx| {
            egui::CentralPanel::default()
                .frame(egui::Frame::NONE)
                .show(ctx, |ui| {
                    for (index, pane) in panes.iter_mut().enumerate() {
                        let mut child =
                            ui.new_child(egui::UiBuilder::new().id_salt(index).max_rect(pane.body));
                        child.set_clip_rect(pane.body);
                        pane.view.ops.drag.reserve_files(&child);
                        child.painter().line_segment(
                            [pane.body.left_center(), pane.body.right_center()],
                            egui::Stroke::new(theme.border_width.value(), ROW_INK),
                        );
                        for (path, is_dir, rect) in &pane.items {
                            push(
                                &child,
                                Spot {
                                    path: path.clone(),
                                    is_dir: *is_dir,
                                    rect: *rect,
                                    full: egui::Rect::NOTHING,
                                    kind: Kind::Entry { wide: false },
                                },
                            );
                        }
                        let mut action = None;
                        frame(&child, &theme, &mut pane.view, pane.body, &mut action);
                        if let Some(ExplorerAction::Ops(OpsAction::Drop { kind, paths, dest })) =
                            action
                        {
                            drops.push((index, kind, paths, dest));
                        }
                    }
                });
            if std::mem::take(&mut discard) {
                ctx.request_discard("drag flow test");
            }
        });
        self.shapes = out.shapes.into_iter().map(|c| c.shape).collect();
    }

    fn dragging(&self) -> bool {
        egui::DragAndDrop::has_payload_of_type::<Payload>(&self.ctx)
    }

    /// `from` 에서 눌러 `to` 까지 끈다. 놓지는 않는다.
    fn drag(&mut self, from: egui::Pos2, to: egui::Pos2) {
        self.step(vec![moved(from), button(from, true)], false);
        self.step(vec![moved(from + egui::vec2(12.0, 0.0))], false);
        self.step(vec![moved(to)], false);
        self.step(vec![moved(to)], false);
        assert!(self.dragging(), "the drag must have started");
    }

    fn release(&mut self, at: egui::Pos2, discard: bool) {
        self.step(vec![moved(at), button(at, false)], discard);
    }
}

#[test]
fn a_drop_is_requested_once_even_when_the_frame_runs_again() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let file = tmp.path().join("a").join("x.txt");
    let dest = tmp.path().join("b");
    std::fs::create_dir_all(file.parent().expect("parent")).expect("mkdir a");
    std::fs::create_dir_all(&dest).expect("mkdir b");
    std::fs::write(&file, b"x").expect("write");
    let left = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(300.0, 600.0));
    let right = egui::Rect::from_min_size(egui::pos2(400.0, 0.0), egui::vec2(300.0, 600.0));
    let mut h = Harness::new(vec![
        (left, vec![(file.clone(), false, cell(10.0, 10.0))]),
        (right, vec![(dest.clone(), true, cell(410.0, 10.0))]),
    ]);
    let (a, b) = (egui::pos2(20.0, 20.0), egui::pos2(420.0, 20.0));
    h.drag(a, b);
    assert!(h.drops.is_empty(), "hovering must not drop");
    h.release(b, true);
    for _ in 0..3 {
        h.step(vec![moved(b)], false);
    }
    assert_eq!(h.drops, vec![(1, OpKind::Move, vec![file], dest)]);
    assert!(!h.dragging());
}

/// 대상은 행을 그린 뒤에 정해지지만 틴트는 행 잉크 아래, 링은 위에 놓인다.
#[test]
fn the_drop_tint_lands_under_the_row_ink_and_the_ring_over_it() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let file = tmp.path().join("a").join("x.txt");
    let dest = tmp.path().join("b");
    std::fs::create_dir_all(file.parent().expect("parent")).expect("mkdir a");
    std::fs::create_dir_all(&dest).expect("mkdir b");
    std::fs::write(&file, b"x").expect("write");
    let left = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(300.0, 600.0));
    let right = egui::Rect::from_min_size(egui::pos2(400.0, 0.0), egui::vec2(300.0, 600.0));
    let mut h = Harness::new(vec![
        (left, vec![(file, false, cell(10.0, 10.0))]),
        (right, vec![(dest, true, cell(410.0, 10.0))]),
    ]);
    h.drag(egui::pos2(20.0, 20.0), egui::pos2(420.0, 20.0));
    let theme = Theme::with_colors_and_zoom(tasty_themes::mocha_fallback_colors(), false, 1.0);
    let (tint, border) = (
        theme.explorer_drop_target_bg().to_egui(),
        theme.explorer_drop_target_border().to_egui(),
    );
    let at = |want: &dyn Fn(&egui::Shape) -> bool| h.shapes.iter().position(want);
    let tint_at = at(&|s| matches!(s, egui::Shape::Rect(r) if r.fill == tint)).expect("tint");
    let ring_at =
        at(&|s| matches!(s, egui::Shape::Rect(r) if r.stroke.color == border)).expect("ring");
    // 오른쪽 칸의 행 잉크. 왼쪽 칸의 것은 틴트보다 먼저 그려진다.
    let ink_at = h
        .shapes
        .iter()
        .rposition(
            |s| matches!(s, egui::Shape::LineSegment { stroke, .. } if stroke.color == ROW_INK),
        )
        .expect("row ink");
    assert!(tint_at < ink_at, "tint {tint_at}, ink {ink_at}");
    assert!(ink_at < ring_at, "ink {ink_at}, ring {ring_at}");
}

#[test]
fn escape_ends_the_drag_for_the_rest_of_the_press() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let file = tmp.path().join("x.txt");
    let dest = tmp.path().join("b");
    std::fs::create_dir_all(&dest).expect("mkdir b");
    std::fs::write(&file, b"x").expect("write");
    let body = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(400.0, 600.0));
    let mut h = Harness::new(vec![(
        body,
        vec![
            (file.clone(), false, cell(10.0, 10.0)),
            (dest.clone(), true, cell(10.0, 200.0)),
        ],
    )]);
    let (a, b) = (egui::pos2(20.0, 20.0), egui::pos2(20.0, 210.0));
    h.drag(a, b);
    h.step(vec![escape()], false);
    assert!(!h.dragging(), "Esc clears the payload");
    // 같은 누름을 계속 끌어도 다시 시작하지 않고, 놓아도 아무것도 하지 않는다.
    h.step(vec![moved(b + egui::vec2(4.0, 0.0))], false);
    h.step(vec![moved(b)], false);
    assert!(!h.dragging(), "the same press must not start again");
    h.release(b, false);
    assert!(h.drops.is_empty(), "{:?}", h.drops);
    // 다음 누름은 다시 끌 수 있다.
    h.drag(a, b);
    h.release(b, false);
    assert_eq!(h.drops, vec![(0, OpKind::Move, vec![file], dest)]);
}

/// 링크는 같은 폴더의 다른 이름이다. 판정은 작업 엔진처럼 링크를 푼 경로로 한다.
#[cfg(unix)]
#[test]
fn a_symlink_alias_is_judged_as_the_folder_it_points_to() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path();
    let src = root.join("src");
    let other = root.join("other");
    std::fs::create_dir_all(src.join("sub")).expect("mkdir src");
    std::fs::create_dir_all(&other).expect("mkdir other");
    std::fs::write(src.join("x.txt"), b"x").expect("write");
    std::os::unix::fs::symlink(&src, root.join("alias")).expect("symlink");
    std::os::unix::fs::symlink(&src, root.join("link")).expect("symlink");
    let body = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(400.0, 600.0));
    let at = |row: f32| egui::pos2(20.0, 20.0 + row * 40.0);
    let spot =
        |path: PathBuf, is_dir: bool, row: f32| (path, is_dir, cell(10.0, 10.0 + row * 40.0));
    let cases = [
        // 폴더를 자기 자신의 링크 이름이나 그 아래로 넣지 않는다.
        (src.clone(), true, root.join("alias"), false),
        (src.clone(), true, root.join("alias").join("sub"), false),
        // 링크 이름으로 본 파일을 원래 폴더로 옮기는 것은 같은 폴더로의 이동이다.
        (root.join("alias").join("x.txt"), false, src.clone(), false),
        // 링크 항목 자체는 링크가 가리키는 폴더로 옮길 수 있다(엔진도 링크를 옮긴다).
        (root.join("link"), false, src.clone(), true),
        (src.clone(), true, other.clone(), true),
    ];
    for (from, from_dir, to, allowed) in cases {
        let mut h = Harness::new(vec![(
            body,
            vec![
                spot(from.clone(), from_dir, 0.0),
                spot(to.clone(), true, 2.0),
            ],
        )]);
        h.drag(at(0.0), at(2.0));
        h.release(at(2.0), false);
        let expected = if allowed {
            vec![(0, OpKind::Move, vec![from.clone()], to.clone())]
        } else {
            Vec::new()
        };
        assert_eq!(h.drops, expected, "{from:?} -> {to:?}");
    }
}

fn tree(root: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).expect("list") {
            let path = entry.expect("entry").path();
            if path.is_dir() {
                stack.push(path);
            } else {
                let bytes = std::fs::read(&path).expect("read");
                out.push((
                    path.strip_prefix(root).expect("inside").to_path_buf(),
                    bytes,
                ));
            }
        }
    }
    out.sort();
    out
}

/// 여러 항목을 끌어 놓은 요청을 작업 엔진까지 실행한다. 복사는 원본을 남기고 이동은 지우며, 놓인 내용은
/// 원본과 같다.
#[test]
fn a_multi_item_drag_runs_to_the_end_with_the_same_content() {
    use crate::app::explorer_files::job::{Choice, Shared, run_transfer};
    for flip in [false, true] {
        let tmp = tempfile::tempdir().expect("tempdir");
        let src = tmp.path().join("src");
        let dest = tmp.path().join("dest");
        std::fs::create_dir_all(src.join("sub")).expect("mkdir src");
        std::fs::create_dir_all(&dest).expect("mkdir dest");
        std::fs::write(src.join("a.txt"), b"alpha").expect("write");
        std::fs::write(src.join("b.bin"), [0u8, 1, 2, 255]).expect("write");
        std::fs::write(src.join("sub").join("c.txt"), b"gamma").expect("write");
        let before = tree(&src);
        let picked = [src.join("a.txt"), src.join("b.bin"), src.join("sub")];
        let body = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(400.0, 600.0));
        let mut items: Items = picked
            .iter()
            .enumerate()
            .map(|(i, path)| {
                (
                    path.clone(),
                    path.is_dir(),
                    cell(10.0, 10.0 + i as f32 * 40.0),
                )
            })
            .collect();
        items.push((dest.clone(), true, cell(10.0, 300.0)));
        let mut h = Harness::new(vec![(body, items)]);
        let view = &mut h.panes[0].view;
        view.ops.drag.flip = Combo::parse_modifiers("ctrl");
        for path in &picked {
            view.selected.insert(path.clone());
        }
        h.mods.ctrl = flip;
        let (from, to) = (egui::pos2(20.0, 60.0), egui::pos2(20.0, 310.0));
        h.drag(from, to);
        h.release(to, false);
        let kind = if flip { OpKind::Copy } else { OpKind::Move };
        assert_eq!(h.drops, vec![(0, kind, picked.to_vec(), dest.clone())]);
        let report = run_transfer(&Shared::fixed(Choice::KeepBoth), &picked, &dest, !flip);
        assert_eq!((report.done, report.failed.len()), (3, 0), "{report:?}");
        assert_eq!(tree(&dest), before, "the dropped content matches");
        let left: Vec<bool> = picked.iter().map(|p| p.exists()).collect();
        assert_eq!(left, vec![flip; 3], "copy keeps and move removes originals");
    }
}
