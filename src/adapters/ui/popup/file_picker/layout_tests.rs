//! 실제 그린 텍스트와 clip 영역을 비교해 버튼·라벨이 보이는지 확인한다.
use super::*;

const CONFIRM: &str = "CONFIRM-LABEL";
const CANCEL: &str = "CANCEL-LABEL";

/// 메인 피커 PopupDef 대로(셸 타이틀바 유무 포함) 만든 같은 크기의 popup.
fn picker_state(size: egui::Vec2) -> crate::adapters::ui::popup::PopupState {
    let def = crate::adapters::ui::popup::defs::find(FILE_PICKER_POPUP_ID)
        .expect("file picker PopupDef 가 등록돼 있어야 한다");
    crate::adapters::ui::popup::PopupState::new(FILE_PICKER_POPUP_ID, "t", size)
        .with_headless(def.headless)
}

fn deep_crumbs(depth: usize, seg: &str) -> Vec<CrumbView> {
    let mut out = vec![CrumbView { label: "/".into() }];
    out.extend((1..depth).map(|i| CrumbView {
        label: format!("{seg}-{i:02}"),
    }));
    out
}

fn entries() -> Vec<FilePickerEntryView> {
    (0..40)
        .map(|i| FilePickerEntryView {
            name: format!("file-{i}.toml"),
            is_dir: i % 5 == 0,
            size_display: "1 KB".into(),
            modified_display: "2026-09-14".into(),
        })
        .collect()
}

/// 한 프레임을 popup 매니저와 같은 조건(Area · 콘텐츠 크기 고정 · 콘텐츠로 clip)으로 그리고,
/// 칠해진 텍스트마다 (문자열, 화면 사각형, clip 사각형) 을 모은다.
fn painted(
    size: egui::Vec2,
    crumbs: &[CrumbView],
    mode: FilePickerMode<'_>,
) -> (egui::Rect, Vec<(String, egui::Rect, egui::Rect)>) {
    painted_entries(size, crumbs, mode, &entries())
}

fn painted_entries(
    size: egui::Vec2,
    crumbs: &[CrumbView],
    mode: FilePickerMode<'_>,
    entries: &[FilePickerEntryView],
) -> (egui::Rect, Vec<(String, egui::Rect, egui::Rect)>) {
    painted_selection(size, crumbs, mode, entries, "file-1.toml")
}

fn painted_selection(
    size: egui::Vec2,
    crumbs: &[CrumbView],
    mode: FilePickerMode<'_>,
    entries: &[FilePickerEntryView],
    selection: &str,
) -> (egui::Rect, Vec<(String, egui::Rect, egui::Rect)>) {
    let (content, shapes) = painted_shapes(size, crumbs, mode, entries, selection);
    let mut out = Vec::new();
    for clipped in shapes.iter() {
        if let egui::epaint::Shape::Text(t) = &clipped.shape {
            let rect = egui::Rect::from_min_size(t.pos, t.galley.size());
            out.push((t.galley.text().to_string(), rect, clipped.clip_rect));
        }
    }
    (content, out)
}

/// 두 번째 프레임에 칠해진 모양 전체.
fn painted_shapes(
    size: egui::Vec2,
    crumbs: &[CrumbView],
    mode: FilePickerMode<'_>,
    entries: &[FilePickerEntryView],
    selection: &str,
) -> (egui::Rect, Vec<egui::epaint::ClippedShape>) {
    painted_shapes_in(&picker_state(size), crumbs, mode, entries, selection)
}

/// `popup` 이 내주는 콘텐츠 사각형에 뷰를 그린다.
fn painted_shapes_in(
    popup: &crate::adapters::ui::popup::PopupState,
    crumbs: &[CrumbView],
    mode: FilePickerMode<'_>,
    entries: &[FilePickerEntryView],
    selection: &str,
) -> (egui::Rect, Vec<egui::epaint::ClippedShape>) {
    painted_frames(popup, crumbs, mode, entries, selection, 2, &|_, _| {
        Vec::new()
    })
}

/// `frames` 프레임을 그리고 마지막 프레임의 모양을 돌려준다. `events` 는 프레임 번호와
/// 콘텐츠 사각형을 받아 그 프레임의 입력을 만든다.
fn painted_frames(
    popup: &crate::adapters::ui::popup::PopupState,
    crumbs: &[CrumbView],
    mode: FilePickerMode<'_>,
    entries: &[FilePickerEntryView],
    selection: &str,
    frames: usize,
    events: &dyn Fn(usize, egui::Rect) -> Vec<egui::Event>,
) -> (egui::Rect, Vec<egui::epaint::ClippedShape>) {
    painted_frames_filtered(popup, crumbs, mode, entries, selection, frames, events, &[])
}

/// [`painted_frames`] 에 호출자 확장자 필터를 더한 것.
#[allow(clippy::too_many_arguments)] // reason: 시험 하네스가 뷰 props 의 축을 그대로 받는다
fn painted_frames_filtered(
    popup: &crate::adapters::ui::popup::PopupState,
    crumbs: &[CrumbView],
    mode: FilePickerMode<'_>,
    entries: &[FilePickerEntryView],
    selection: &str,
    frames: usize,
    events: &dyn Fn(usize, egui::Rect) -> Vec<egui::Event>,
    filters: &[String],
) -> (egui::Rect, Vec<egui::epaint::ClippedShape>) {
    let th = crate::theme::theme();
    let ctx = egui::Context::default();
    let content = popup.content_rect();
    let selected = vec![selection.to_string()];
    let props = FilePickerProps {
        theme: &th,
        remote_host: None,
        crumbs,
        state: FpViewState::Loaded,
        entries,
        selected: &selected,
        mode,
        owns_escape: false,
        title_label: "title",
        name_field_label: "File name",
        name_placeholder: "placeholder",
        cancel_label: CANCEL,
        confirm_label: CONFIRM,
        overwrite_warning: "{name} already exists in this folder. Saving replaces it.",
        hidden_folders_one: "1 hidden",
        hidden_folders_many: "{} hidden",
        folder_not_save_target: "not a save target: {name}",
        folder_open_enters: "{name} is a folder — {confirm} enters it.",
        empty_label: "",
        loading_label: "",
        loading_body_local: "",
        loading_body_remote: "",
        error_perm_title: "",
        error_perm_retry: "",
        error_conn_title: "",
        error_conn_reconnect: "",
        col_name: "Name",
        col_size: "Size",
        col_modified: "Modified",
        filters,
        filter_showing: "Showing {list}",
    };
    let mut out = Vec::new();
    // 첫 프레임은 폰트가 확정되지 않아 galley 가 비는 경우가 있어 두 번 이상 돈다.
    for frame in 0..frames {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(2000.0, 1500.0),
            )),
            // 부드러운 스크롤이 몇 프레임 안에 끝나도록 시간을 크게 넘긴다.
            time: Some(frame as f64),
            events: events(frame, content),
            ..Default::default()
        };
        let full = ctx.run(input, |ctx| {
            egui::Area::new(egui::Id::new("fp_layout_test"))
                // 등장 페이드가 색을 반투명하게 바꾸지 않도록 끈다.
                .fade_in(false)
                .fixed_pos(content.min)
                .constrain(false)
                .show(ctx, |ui| {
                    ui.set_min_size(content.size());
                    ui.set_max_size(content.size());
                    ui.set_clip_rect(content);
                    draw_file_picker_view(ui, &props);
                });
        });
        out = full.shapes;
    }
    (content, out)
}

/// `label` 이 칠해졌고, 그 텍스트가 콘텐츠 영역과 자신의 clip 안에 **온전히** 들어간다.
fn assert_fully_visible(
    content: egui::Rect,
    shapes: &[(String, egui::Rect, egui::Rect)],
    label: &str,
    case: &str,
) {
    let hits: Vec<_> = shapes.iter().filter(|(t, _, _)| t == label).collect();
    assert!(!hits.is_empty(), "{case}: {label} 이 칠해지지 않았다");
    for (_, rect, clip) in hits {
        let visible = content.intersect(*clip);
        assert!(
            visible.expand(0.5).contains_rect(*rect),
            "{case}: {label} 이 잘렸다 — text {rect:?}, 보이는 영역 {visible:?}"
        );
    }
}

/// 경로 깊이·성분 길이·모드·popup 폭을 흔들어도 footer 의 취소·확정 버튼은 온전히 보인다.
#[test]
fn footer_buttons_are_never_clipped_at_any_path_length() {
    let long_seg = "a-very-long-directory-name-that-keeps-going-and-going";
    let long_name = "x".repeat(300);
    let modes = [
        (
            "open",
            FilePickerMode::Open {
                selection_text: "file-1.toml",
            },
        ),
        (
            "save",
            FilePickerMode::Save {
                name: "keybindings.toml",
                overwrite: false,
                can_confirm: true,
            },
        ),
        (
            "save-overwrite",
            FilePickerMode::Save {
                name: &long_name,
                overwrite: true,
                can_confirm: true,
            },
        ),
    ];
    for size in [egui::vec2(640.0, 480.0), egui::vec2(400.0, 360.0)] {
        for depth in [1, 4, 8, 30] {
            for seg in ["d", long_seg] {
                for (mode_name, mode) in modes {
                    let crumbs = deep_crumbs(depth, seg);
                    let case = format!("{size:?} depth={depth} seg={} mode={mode_name}", seg.len());
                    let (content, shapes) = painted(size, &crumbs, mode);
                    assert_fully_visible(content, &shapes, CONFIRM, &case);
                    assert_fully_visible(content, &shapes, CANCEL, &case);
                }
            }
        }
    }
}

/// 셸 타이틀바 없이 위젯 헤더 하나만 있다 — 제목은 한 번만 칠해지고, popup 윗변에서
/// 타이틀바 한 줄 높이 안에 들어간다. 셸 타이틀바가 있으면 위젯 헤더가 그만큼 내려간다.
#[test]
fn the_title_is_painted_once_in_the_top_strip_of_the_popup() {
    let size = egui::vec2(640.0, 480.0);
    let (_, shapes) = painted(
        size,
        &deep_crumbs(1, "d"),
        FilePickerMode::Open { selection_text: "" },
    );
    let hits: Vec<_> = shapes.iter().filter(|(t, _, _)| t == "title").collect();
    assert_eq!(hits.len(), 1, "제목은 한 번만 칠해진다: {hits:?}");
    let top = picker_state(size).pos.y;
    let strip = crate::adapters::ui::popup::title_bar_height().value();
    assert!(
        hits[0].1.bottom() <= top + strip,
        "제목이 popup 윗줄({top}..{}) 밖에 있다: {:?}",
        top + strip,
        hits[0].1
    );
}

/// 경로 막대 줄은 창 좌우 끝까지 bg-sidebar 로 칠해지고, 그 안에 breadcrumb 가 있다.
#[test]
fn the_path_bar_band_is_filled_edge_to_edge_with_bg_sidebar() {
    assert_path_band_is_full_width(&picker_state(egui::vec2(640.0, 480.0)));
}

/// 같은 뷰를 쓰는 설정 창 파일 선택기도 셸 공통 여백 없이 띠가 창 끝까지 닿는다.
#[test]
fn the_settings_chooser_path_bar_band_is_filled_edge_to_edge() {
    let popup = crate::adapters::ui::popup::PopupState::new(
        crate::view::settings::ui::file_chooser::FILE_CHOOSER_POPUP_ID,
        "t",
        egui::vec2(640.0, 480.0),
    )
    .with_headless(true);
    assert_path_band_is_full_width(&popup);
}

fn assert_path_band_is_full_width(popup_state: &crate::adapters::ui::popup::PopupState) {
    let (content, shapes) = painted_shapes_in(
        popup_state,
        &deep_crumbs(2, "crumb"),
        FilePickerMode::Open { selection_text: "" },
        &entries(),
        "file-1.toml",
    );
    let fill = crate::theme::theme().bg_sidebar().to_egui();
    let popup = popup_state.popup_rect();
    let bands: Vec<egui::Rect> = shapes
        .iter()
        .filter_map(|c| match &c.shape {
            egui::epaint::Shape::Rect(r) if r.fill == fill => Some(r.rect),
            _ => None,
        })
        .collect();
    let crumb = shapes
        .iter()
        .find_map(|c| match &c.shape {
            egui::epaint::Shape::Text(t) if t.galley.text() == "crumb-01" => {
                Some(egui::Rect::from_min_size(t.pos, t.galley.size()))
            }
            _ => None,
        })
        .expect("breadcrumb 이 칠해져야 한다");
    assert!(
        bands.iter().any(|b| b.left() <= popup.left()
            && b.right() >= popup.right()
            && b.contains_rect(crumb)),
        "경로 막대를 덮는 전폭 bg-sidebar 띠가 없다: bands {bands:?}, crumb {crumb:?}, popup {popup:?}, content {content:?}"
    );
}

/// 깊은 경로는 가운데를 접고, 위치를 알려주는 둘(현재 폴더와 부모)과 root 는 보인다.
#[test]
fn deep_breadcrumb_elides_the_middle_and_keeps_root_and_last_two() {
    let crumbs = deep_crumbs(30, "segment");
    let (content, shapes) = painted(
        egui::vec2(640.0, 480.0),
        &crumbs,
        FilePickerMode::Open { selection_text: "" },
    );
    for label in ["/", "…", "segment-28", "segment-29"] {
        assert_fully_visible(content, &shapes, label, "deep");
    }
    assert!(
        !shapes.iter().any(|(t, _, _)| t == "segment-10"),
        "숨긴 조상은 칠해지지 않는다"
    );
}

/// 짧은 경로는 접지 않는다 — 생략은 넘칠 때만의 렌더 규칙이다.
#[test]
fn short_breadcrumb_is_not_elided() {
    let crumbs = deep_crumbs(4, "d");
    let (content, shapes) = painted(
        egui::vec2(640.0, 480.0),
        &crumbs,
        FilePickerMode::Open { selection_text: "" },
    );
    for c in &crumbs {
        assert_fully_visible(content, &shapes, &c.label, "short");
    }
    assert!(!shapes.iter().any(|(t, _, _)| t == "…"));
}

/// 보이는 경로와 생략 메뉴의 경로를 합치면 전체 조상이 빠짐없이 남아야 한다.
#[test]
fn every_ancestor_is_either_painted_or_behind_the_ellipsis() {
    let crumbs = deep_crumbs(30, "segment");
    let (_, shapes) = painted(
        egui::vec2(POPUP_WIDTH.value(), POPUP_HEIGHT.value()),
        &crumbs,
        FilePickerMode::Open { selection_text: "" },
    );
    let painted_labels: Vec<&str> = shapes.iter().map(|(t, _, _)| t.as_str()).collect();
    assert!(painted_labels.contains(&"…"), "접혔는데 `…` 가 없다");
    let hidden: Vec<&str> = crumbs
        .iter()
        .map(|c| c.label.as_str())
        .filter(|l| !painted_labels.contains(l))
        .collect();
    assert!(
        !hidden.is_empty(),
        "테스트 경로는 주어진 폭을 넘어 일부 조상 폴더가 접혀야 한다"
    );
    // 숨은 것은 전부 **가운데** 조상이다: root 와 현재 폴더는 접히지 않는 폭이다.
    assert!(
        !hidden.contains(&crumbs[0].label.as_str()),
        "이 폭에서는 루트 경로가 표시되어야 한다"
    );
    assert!(
        !hidden.contains(&crumbs[crumbs.len() - 1].label.as_str()),
        "현재 폴더가 접혔다"
    );
}

/// 긴 파일명과 짧은 파일명 모두 크기·날짜 열을 덮지 않고 온전히 보인다.
fn assert_filenames_stay_inside_the_name_column(size: egui::Vec2) {
    for name in ["x".repeat(120) + ".toml", "notes.md".to_string()] {
        let rows = vec![FilePickerEntryView {
            name: name.clone(),
            is_dir: false,
            size_display: "0 B".into(),
            modified_display: "2026-09-14".into(),
        }];
        let (content, shapes) = painted_entries(
            size,
            &deep_crumbs(1, "d"),
            FilePickerMode::Open { selection_text: "" },
            &rows,
        );
        let case = format!("{size:?} name_len={}", name.len());
        for label in [name.as_str(), "0 B", "2026-09-14"] {
            assert_fully_visible(content, &shapes, label, &case);
        }
        let name_rect = shapes.iter().find(|(text, _, _)| text == &name).unwrap().1;
        for label in ["0 B", "2026-09-14"] {
            let meta_rect = shapes.iter().find(|(text, _, _)| text == label).unwrap().1;
            assert!(
                name_rect.right() < meta_rect.left(),
                "{case}: {name_rect:?} overlaps {meta_rect:?}"
            );
        }
    }
}

#[test]
fn filenames_stay_inside_the_name_column_at_default_width() {
    assert_filenames_stay_inside_the_name_column(egui::vec2(640.0, 480.0));
}

#[test]
fn filenames_stay_inside_the_name_column_at_narrow_width() {
    assert_filenames_stay_inside_the_name_column(egui::vec2(400.0, 360.0));
}

/// 폴더 안내와 확정 버튼 이름이 일치하고 두 모드 모두 버튼이 잘리지 않는지 확인한다.
#[test]
fn a_selected_folder_puts_its_line_in_the_footer_without_pushing_the_buttons_out() {
    let entries = entries();
    let folder = entries
        .iter()
        .find(|e| e.is_dir)
        .expect("픽스처에 폴더가 있다")
        .name
        .clone();
    let size = egui::vec2(POPUP_WIDTH.value(), POPUP_HEIGHT.value());
    let crumbs = deep_crumbs(3, "seg");

    let (content, shapes) = painted_selection(
        size,
        &crumbs,
        FilePickerMode::Save {
            name: "keys.toml",
            overwrite: false,
            can_confirm: true,
        },
        &entries,
        &folder,
    );
    let line = format!("not a save target: {folder}");
    assert_fully_visible(content, &shapes, &line, "save + folder");
    assert_fully_visible(content, &shapes, CONFIRM, "save + folder");
    assert_fully_visible(content, &shapes, CANCEL, "save + folder");

    let (content, shapes) = painted_selection(
        size,
        &crumbs,
        FilePickerMode::Open { selection_text: "" },
        &entries,
        &folder,
    );
    let line = format!("{folder} is a folder — {CONFIRM} enters it.");
    assert_fully_visible(content, &shapes, &line, "open + folder");
    assert_fully_visible(content, &shapes, CONFIRM, "open + folder");

    // 파일 선택 시에는 폴더 안내가 없어야 한다.
    let file = entries
        .iter()
        .find(|e| !e.is_dir)
        .expect("픽스처에 파일이 있다")
        .name
        .clone();
    let (_, shapes) = painted_selection(
        size,
        &crumbs,
        FilePickerMode::Open {
            selection_text: &file,
        },
        &entries,
        &file,
    );
    assert!(
        !shapes.iter().any(|(t, _, _)| t.contains("is a folder")),
        "파일을 선택했는데 폴더 안내가 표시됐다"
    );
}

/// 현재 폴더는 구분에 필요한 끝부분을 남기고 앞에 생략 표시를 붙인다.
#[test]
fn the_current_folder_elides_at_the_front_and_never_clips_without_an_ellipsis() {
    let long = "a".repeat(53);
    let crumbs = vec![
        CrumbView { label: "/".into() },
        CrumbView {
            label: long.clone(),
        },
        CrumbView {
            label: format!("{long}-tail"),
        },
    ];
    let (content, shapes) = painted(
        egui::vec2(400.0, 360.0),
        &crumbs,
        FilePickerMode::Open { selection_text: "" },
    );
    let current = shapes
        .iter()
        .map(|(t, _, _)| t.as_str())
        .find(|t| t.ends_with("-tail"))
        .unwrap_or_else(|| {
            panic!(
                "현재 폴더의 꼬리가 사라졌다 — 칠해진 것: {:?}",
                shapes.iter().map(|(t, _, _)| t).collect::<Vec<_>>()
            )
        });
    assert_ne!(
        current,
        format!("{long}-tail"),
        "테스트 경로는 이 폭에서 말줄임되어야 한다"
    );
    assert!(
        current.starts_with('…'),
        "현재 폴더가 앞 말줄임 표지 없이 잘렸다: {current:?}"
    );
    assert_fully_visible(content, &shapes, CONFIRM, "narrow");
    assert_fully_visible(content, &shapes, CANCEL, "narrow");
}

/// 구역마다 `fp-*` 인셋을 쓴다. 헤더는 8 + sm IconButton 24 + 8 = 40, 경로 막대는
/// 4 + 24 + 4 = 32이고, 헤더 글리프·첫 crumb·행 아이콘·푸터 라벨이 창 안쪽 + 12에서 시작한다.
#[test]
fn every_section_starts_on_the_fp_inset_start_column() {
    let th = crate::theme::theme();
    let size = egui::vec2(640.0, 480.0);
    let (content, shapes) = painted_shapes(
        size,
        &deep_crumbs(2, "crumb"),
        FilePickerMode::Open { selection_text: "" },
        &entries(),
        "file-1.toml",
    );
    let text_rect = |label: &str| {
        shapes
            .iter()
            .find_map(|c| match &c.shape {
                egui::epaint::Shape::Text(t) if t.galley.text() == label => {
                    Some(egui::Rect::from_min_size(t.pos, t.galley.size()))
                }
                _ => None,
            })
            .unwrap_or_else(|| panic!("{label} 이 칠해지지 않았다"))
    };
    let fill = th.bg_sidebar().to_egui();
    let band = shapes
        .iter()
        .find_map(|c| match &c.shape {
            egui::epaint::Shape::Rect(r) if r.fill == fill => Some(r.rect),
            _ => None,
        })
        .expect("경로 막대 띠가 칠해져야 한다");
    assert_eq!(band.top() - content.top(), 40.0, "헤더 높이");
    assert_eq!(band.height(), 32.0, "경로 막대 높이");

    let start = content.left() + th.fp_inset_start().value();
    let icon_col = th.icon_glyph_size_md.value() + th.fp_section_gap().value();
    let name_col = th.icon_glyph_size_md.value() + th.spacing_sm.value();
    let near = |got: f32, want: f32, what: &str| {
        assert!((got - want).abs() <= 1.0, "{what}: x {got}, 기대 {want}");
    };
    // 글리프는 텍스트 앞 한 칸이므로 제목·행 이름 위치에서 역산한다.
    near(text_rect("title").left() - icon_col, start, "헤더 글리프");
    near(text_rect("/").left(), start, "첫 crumb");
    near(
        text_rect("file-2.toml").left() - name_col,
        start,
        "행 아이콘",
    );
    near(text_rect("File name").left(), start, "푸터 라벨");
    // 행 높이는 4 + 이름 줄(13 × 1.4) + 4다. egui 가 좌표를 1/16 단위로 맞춘다.
    let pitch = text_rect("file-3.toml").center().y - text_rect("file-2.toml").center().y;
    assert!(
        (pitch - row_height(&th).value()).abs() < 0.05 && (pitch - 26.2).abs() < 0.05,
        "행 간격 {pitch}"
    );
    near(
        content.bottom() - text_rect(CONFIRM).center().y,
        th.fp_footer_pad_y().value() + th.button_height().value() * 0.5,
        "푸터 아래 여백",
    );
}

/// 칠해진 텍스트 하나의 사각형과 글꼴 크기.
fn text_shape(shapes: &[egui::epaint::ClippedShape], label: &str) -> (egui::Rect, f32) {
    let hits: Vec<_> = shapes
        .iter()
        .filter_map(|c| match &c.shape {
            egui::epaint::Shape::Text(t) if t.galley.text() == label => Some((
                egui::Rect::from_min_size(t.pos, t.galley.size()),
                t.galley.job.sections[0].format.font_id.size,
            )),
            _ => None,
        })
        .collect();
    assert_eq!(hits.len(), 1, "{label} 은 한 번 칠해진다: {hits:?}");
    hits[0]
}

/// kit FilePickerFrame 의 목록 머리 — 경로 막대 바로 아래 mono micro 대문자 열 이름이
/// 행 값과 같은 열에 서고, 첫 행은 머리 높이(4 + 10 × 1.4 + 4) 아래에서 시작한다.
#[test]
fn the_list_head_names_the_columns_above_the_rows() {
    let th = crate::theme::theme();
    let (content, shapes) = painted_shapes(
        egui::vec2(640.0, 480.0),
        &deep_crumbs(2, "crumb"),
        FilePickerMode::Open { selection_text: "" },
        &entries(),
        "file-1.toml",
    );
    let (name, name_size) = text_shape(&shapes, "NAME");
    let (size_col, _) = text_shape(&shapes, "SIZE");
    let (modified, _) = text_shape(&shapes, "MODIFIED");
    assert_eq!(
        name_size,
        th.font_size_micro.value(),
        "열 이름은 micro 크기"
    );
    let (row0, _) = text_shape(&shapes, "file-0.toml");
    let near = |got: f32, want: f32, what: &str| {
        assert!((got - want).abs() <= 1.0, "{what}: {got}, 기대 {want}");
    };
    near(name.left(), row0.left(), "Name 열 왼쪽");
    let first_row_values: Vec<egui::Rect> = shapes
        .iter()
        .filter_map(|c| match &c.shape {
            egui::epaint::Shape::Text(t)
                if (t.galley.text() == "1 KB" || t.galley.text() == "2026-09-14")
                    && (t.pos.y + t.galley.size().y * 0.5 - row0.center().y).abs() < 1.0 =>
            {
                Some(egui::Rect::from_min_size(t.pos, t.galley.size()))
            }
            _ => None,
        })
        .collect();
    assert_eq!(first_row_values.len(), 2, "첫 행의 크기·수정일");
    near(
        size_col.right(),
        first_row_values[0].right(),
        "Size 열 오른쪽",
    );
    near(
        modified.right(),
        first_row_values[1].right(),
        "Modified 열 오른쪽",
    );
    // 헤더 40 + 경로 막대 32 아래가 머리, 그 아래 첫 행.
    let head_top = content.top() + 72.0;
    let head_h = list_head_height(&th).value();
    assert!((head_h - 22.0).abs() < 0.05, "머리 높이 {head_h}");
    near(name.center().y, head_top + head_h * 0.5, "머리 가운데");
    near(
        row0.center().y,
        head_top + head_h + row_height(&th).value() * 0.5,
        "첫 행 가운데",
    );
}

/// 선택 행은 surface-active 채움에 왼쪽 한 변만 selection-edge 폭 accent bar 를 둔다.
#[test]
fn the_selected_row_has_one_edge_selection_bar() {
    let th = crate::theme::theme();
    let (content, shapes) = painted_shapes(
        egui::vec2(640.0, 480.0),
        &deep_crumbs(2, "crumb"),
        FilePickerMode::Open { selection_text: "" },
        &entries(),
        "file-1.toml",
    );
    let (row1, _) = text_shape(&shapes, "file-1.toml");
    let accent = th.accent_primary_on_active().to_egui();
    let bars: Vec<egui::Rect> = shapes
        .iter()
        .filter_map(|c| match &c.shape {
            egui::epaint::Shape::Rect(r)
                if r.fill == accent && r.rect.y_range().contains(row1.center().y) =>
            {
                Some(r.rect)
            }
            _ => None,
        })
        .collect();
    assert_eq!(bars.len(), 1, "선택 bar 는 하나: {bars:?}");
    let bar = bars[0];
    assert_eq!(bar.width(), th.selection_edge_width.value(), "bar 폭");
    assert_eq!(bar.left(), content.left(), "bar 는 행 왼쪽 끝");
    assert!(
        (bar.height() - row_height(&th).value()).abs() < 0.05,
        "bar 높이 {}",
        bar.height()
    );
}

/// 헤더 제목은 kit 대로 14(font-size-max)다.
#[test]
fn the_header_title_uses_the_modal_title_size() {
    let th = crate::theme::theme();
    let (_, shapes) = painted_shapes(
        egui::vec2(640.0, 480.0),
        &deep_crumbs(1, "d"),
        FilePickerMode::Open { selection_text: "" },
        &entries(),
        "file-1.toml",
    );
    let (_, size) = text_shape(&shapes, "title");
    assert_eq!(size, th.font_size_max.value());
}

/// 목록 머리 아래에 separator 선이 하나 있다(kit `borderBottom: 1px solid separator`).
#[test]
fn the_list_head_has_one_separator_along_its_bottom() {
    let th = crate::theme::theme();
    let (content, shapes) = painted_shapes(
        egui::vec2(640.0, 480.0),
        &deep_crumbs(2, "crumb"),
        FilePickerMode::Open { selection_text: "" },
        &entries(),
        "file-1.toml",
    );
    // 헤더 40 + 경로 막대 32 + 머리 높이.
    let head_bottom = content.top() + 72.0 + list_head_height(&th).value();
    let lines: Vec<[egui::Pos2; 2]> = shapes
        .iter()
        .filter_map(|c| match &c.shape {
            egui::epaint::Shape::LineSegment { points, stroke }
                if stroke.color == th.separator.to_egui_premultiplied()
                    && (points[0].y - head_bottom).abs() < 1.0
                    && (points[1].y - head_bottom).abs() < 1.0 =>
            {
                Some(*points)
            }
            _ => None,
        })
        .collect();
    assert_eq!(lines.len(), 1, "머리 아래 separator 는 하나: {lines:?}");
    assert_eq!(lines[0][0].x.min(lines[0][1].x), content.left(), "왼쪽 끝");
    assert_eq!(
        lines[0][0].x.max(lines[0][1].x),
        content.right(),
        "오른쪽 끝"
    );
}

/// 목록이 본문 칸보다 길 때 끝까지 스크롤하면 마지막 행이 푸터에 가리지 않고 온전히 보인다.
/// 본문 칸이 푸터까지 번지면 끝까지 스크롤한 마지막 행이 푸터에 가린다. 이 경우를 잡는다.
#[test]
fn the_last_row_is_fully_visible_after_scrolling_to_the_end() {
    let entries = entries();
    let last = entries.last().expect("행이 있다").name.clone();
    let (content, shapes) = painted_frames(
        &picker_state(egui::vec2(640.0, 480.0)),
        &deep_crumbs(2, "crumb"),
        FilePickerMode::Open { selection_text: "" },
        &entries,
        "file-1.toml",
        12,
        &|frame, content| {
            let pos = egui::pos2(content.center().x, content.top() + 150.0);
            let mut ev = vec![egui::Event::PointerMoved(pos)];
            if (1..6).contains(&frame) {
                ev.push(egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: egui::vec2(0.0, -5000.0),
                    modifiers: egui::Modifiers::NONE,
                });
            }
            ev
        },
    );
    let texts: Vec<(String, egui::Rect, egui::Rect)> = shapes
        .iter()
        .filter_map(|c| match &c.shape {
            egui::epaint::Shape::Text(t) => Some((
                t.galley.text().to_string(),
                egui::Rect::from_min_size(t.pos, t.galley.size()),
                c.clip_rect,
            )),
            _ => None,
        })
        .collect();
    let (_, first, first_clip) = texts
        .iter()
        .find(|(t, _, _)| t == "file-0.toml")
        .expect("첫 행");
    assert!(
        first.bottom() <= first_clip.top(),
        "끝까지 스크롤되지 않았다 — 첫 행 {first:?} 이 보이는 영역 {first_clip:?} 안에 있다"
    );
    assert_fully_visible(content, &texts, &last, "끝까지 스크롤");
    // 보이는 영역이 푸터로 번져도 위 검사는 통과하므로 푸터 첫 줄보다 위에 있는지도 본다.
    let (_, last_rect, _) = texts
        .iter()
        .find(|(t, _, _)| *t == last)
        .expect("마지막 행");
    let (_, footer_label, _) = texts
        .iter()
        .find(|(t, _, _)| t == "File name")
        .expect("푸터 라벨");
    assert!(
        last_rect.bottom() <= footer_label.top(),
        "마지막 행 {last_rect:?} 이 푸터 라벨 {footer_label:?} 과 겹친다"
    );
}

/// 필터 칩의 (문자열, 화면 사각형) 과 이름 칸 오른쪽 끝. 칩 글자는 mono 로 칠해진다.
fn painted_filter_chip(filters: &[&str]) -> (egui::Rect, Vec<(String, egui::Rect)>) {
    let filters: Vec<String> = filters.iter().map(|f| f.to_string()).collect();
    let (content, shapes) = painted_frames_filtered(
        &picker_state(egui::vec2(640.0, 480.0)),
        &deep_crumbs(1, "d"),
        FilePickerMode::Open {
            selection_text: "file-1.toml",
        },
        &entries(),
        "file-1.toml",
        2,
        &|_, _| Vec::new(),
        &filters,
    );
    let texts = shapes
        .iter()
        .filter_map(|c| match &c.shape {
            egui::epaint::Shape::Text(t) if t.galley.text().starts_with("*.") => Some((
                t.galley.text().to_string(),
                egui::Rect::from_min_size(t.pos, t.galley.size()),
            )),
            _ => None,
        })
        .collect();
    (content, texts)
}

/// 필터가 없으면 칩을 그리지 않는다.
#[test]
fn no_filter_draws_no_chip() {
    let (_, chips) = painted_filter_chip(&[]);
    assert!(chips.is_empty(), "{chips:?}");
}

/// 필터가 있으면 호출자 순서·소문자로 칩을 그리고, 칩은 footer 이름 줄의 오른쪽 끝에 붙는다.
#[test]
fn a_filter_draws_one_chip_at_the_right_end_of_the_name_row() {
    let th = crate::theme::theme();
    let (content, chips) = painted_filter_chip(&["TOML", "json"]);
    assert_eq!(chips.len(), 1, "{chips:?}");
    assert_eq!(chips[0].0, "*.toml, *.json");
    let right_inset = th.fp_inset_end().value().max(th.fp_inset_start().value());
    assert!(
        chips[0].1.right() <= content.right() - th.spacing_sm.value(),
        "칩 글자가 콘텐츠 밖으로 나갔다: {:?} / {content:?}",
        chips[0].1
    );
    assert!(
        chips[0].1.right() >= content.right() - right_inset - th.spacing_sm.value() - 1.0,
        "칩이 이름 줄 오른쪽 끝에 붙지 않았다: {:?} / {content:?}",
        chips[0].1
    );
}

/// 긴 목록은 fp-filter-max-width 안에서 끝이 줄어든다.
#[test]
fn a_long_filter_list_is_capped_at_the_max_width() {
    let th = crate::theme::theme();
    let (_, chips) = painted_filter_chip(&["png", "jpg", "jpeg", "gif", "webp", "svg"]);
    assert_eq!(chips.len(), 1, "{chips:?}");
    let text_max = th.fp_filter_max_width().value() - 2.0 * th.spacing_sm.value();
    assert!(
        chips[0].1.width() <= text_max + 0.5,
        "칩 글자 폭 {} 이 상한 {text_max} 를 넘었다",
        chips[0].1.width()
    );
}

#[test]
fn folder_footer_keeps_all_three_rows_and_buttons_inside_each_received_size() {
    let path = format!("/home/{}/selected-folder", "long-parent-".repeat(12));
    for size in [
        egui::vec2(640.0, 480.0),
        egui::vec2(480.0, 360.0),
        egui::vec2(320.0, 420.0),
    ] {
        let (content, texts) = painted(
            size,
            &deep_crumbs(8, "long-parent"),
            FilePickerMode::Folder {
                path: &path,
                hint: "Double-click {name} to open it.",
                file_tooltip: "Only folders can be selected.",
            },
        );
        for label in [CONFIRM, CANCEL] {
            let (_, rect, clip) = texts
                .iter()
                .find(|(text, _, _)| text == label)
                .expect("footer button painted");
            assert!(
                content.contains_rect(*rect),
                "{label}: {rect:?} outside {content:?}"
            );
            assert!(clip.contains_rect(*rect), "{label} clipped");
        }
        assert!(
            texts
                .iter()
                .any(|(text, _, _)| text.starts_with('…') && text.ends_with("selected-folder")),
            "readout keeps tail"
        );
    }
}
