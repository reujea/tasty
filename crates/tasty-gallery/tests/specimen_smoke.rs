//! GPU 없이 egui 프레임을 실행해 패닉과 ID 충돌을 검사한다.
//! 픽셀이나 모든 레이아웃 오류를 판정하는 검사는 아니다.

// 일반 렌더 검사는 패닉 여부를 보므로 FullOutput을 버린다. ID 충돌 검사는 출력을 직접 읽는다.
#![allow(clippy::let_underscore_must_use)]

use tasty_gallery::catalog::chrome_loading;
use tasty_gallery::catalog::components::{dag, prim_drilldown, prim_listctrl};
use tasty_ui_widgets::{Button, ButtonVariant, ControlSize, DrillDown, DrillDownView};

fn run_frames(mut body: impl FnMut(&mut egui::Ui)) {
    let ctx = egui::Context::default();
    // 상태(thread_local)가 프레임을 넘어 유지되는 경로까지 몇 프레임 돌린다.
    for _ in 0..3 {
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| body(ui));
        });
    }
}

/// egui는 ID 충돌을 패닉 대신 텍스트 마커로 표시하므로 FullOutput의 텍스트를 검사한다.
/// 영역 밖 그리기처럼 마커가 없는 레이아웃 오류는 검출하지 못한다.
fn assert_no_id_clash(label: &str, mut body: impl FnMut(&mut egui::Ui)) {
    let ctx = egui::Context::default();
    // release에서도 같은 마커가 나오도록 ID 충돌 표시를 켠다.
    ctx.options_mut(|o| o.warn_on_id_clash = true);
    let mut found: Vec<String> = Vec::new();
    for _ in 0..3 {
        let output = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| body(ui));
        });
        for cs in &output.shapes {
            collect_id_clash_text(&cs.shape, &mut found);
        }
    }
    assert!(
        found.is_empty(),
        "{label}: egui id 충돌 마커가 렌더됐다 — 같은 자리에서 auto-id 가 재사용된다\
         (push_id 로 갈라야 한다). 마커: {found:?}"
    );
}

fn collect_id_clash_text(shape: &egui::epaint::Shape, out: &mut Vec<String>) {
    match shape {
        egui::epaint::Shape::Text(t) => {
            let s = t.galley.text();
            // "First use of … ID …" / "Double use of … ID …" (egui context.rs).
            if s.contains("use of") && s.contains("ID") {
                out.push(s.to_string());
            }
        }
        egui::epaint::Shape::Vec(v) => {
            for s in v {
                collect_id_clash_text(s, out);
            }
        }
        _ => {}
    }
}

#[test]
fn modifier_hint_specimen_은_id_충돌_없이_렌더된다() {
    use tasty_gallery::catalog::components::modifier_hint;
    let theme = tasty_themes::mocha_fallback();
    assert_no_id_clash("modifier_hint::draw", |ui| modifier_hint::draw(ui, &theme));
}

#[test]
fn tutorial_specimen_4종은_id_충돌_없이_렌더된다() {
    use tasty_gallery::catalog::widgets::tutorial;
    let theme = tasty_themes::mocha_fallback();
    assert_no_id_clash("tutorial::draw_marker", |ui| {
        tutorial::draw_marker(ui, &theme)
    });
    assert_no_id_clash("tutorial::draw_callout", |ui| {
        tutorial::draw_callout(ui, &theme)
    });
    assert_no_id_clash("tutorial::draw_topics", |ui| {
        tutorial::draw_topics(ui, &theme)
    });
    assert_no_id_clash("tutorial::draw_composite", |ui| {
        tutorial::draw_composite(ui, &theme)
    });
}

#[test]
fn listctrl_specimen_은_헤드리스로_렌더된다() {
    let theme = tasty_themes::mocha_fallback();
    run_frames(|ui| prim_listctrl::draw(ui, &theme));
}

#[test]
fn text_caret_specimen_은_헤드리스로_렌더된다() {
    use tasty_gallery::catalog::components::prim_text_caret;
    let theme = tasty_themes::mocha_fallback();
    assert_no_id_clash("text-caret", |ui| prim_text_caret::draw(ui, &theme));
}

#[test]
fn drilldown_specimen_은_헤드리스로_렌더된다() {
    let theme = tasty_themes::mocha_fallback();
    run_frames(|ui| prim_drilldown::draw(ui, &theme));
}

#[test]
fn drilldown_detail_뷰는_backbar_와_본문을_렌더된다() {
    // specimen 초기 상태는 List 라 Detail 경로는 위젯 직접 호출로 커버한다.
    let theme = tasty_themes::mocha_fallback();
    run_frames(|ui| {
        let apply = |ui: &mut egui::Ui, th: &tasty_type_appearance::theme::Theme| {
            Button::new("Apply")
                .variant(ButtonVariant::Primary)
                .size(ControlSize::Sm)
                .show(ui, th);
        };
        let out = DrillDown::new("smoke_detail")
            .view(DrillDownView::Detail)
            .title("Default")
            .back_label("Back")
            .height(theme.measure_sm.value())
            .show(
                ui,
                &theme,
                |_, _| {},
                |ui, _| {
                    ui.label("preview body");
                },
                Some(&apply),
            );
        assert!(!out.back_clicked);
    });
}
// 중첩 ScrollArea와 절대좌표 그리기가 함께 실행될 때의 패닉을 확인한다.

#[test]
fn dag_canvas_specimen_은_헤드리스로_렌더된다() {
    let theme = tasty_themes::mocha_fallback();
    run_frames(|ui| dag::canvas::draw(ui, &theme));
}

#[test]
fn dag_node_specimen_3종은_헤드리스로_렌더된다() {
    let theme = tasty_themes::mocha_fallback();
    run_frames(|ui| dag::node::draw_states(ui, &theme));
    run_frames(|ui| dag::node::draw_kinds(ui, &theme));
    run_frames(|ui| dag::node::draw_lod(ui, &theme));
}

#[test]
fn dag_edges_specimen_은_헤드리스로_렌더된다() {
    let theme = tasty_themes::mocha_fallback();
    run_frames(|ui| dag::edges::draw(ui, &theme));
    run_frames(|ui| dag::routes::draw(ui, &theme));
}

#[test]
fn dag_chrome_와_runner_specimen_은_헤드리스로_렌더된다() {
    let theme = tasty_themes::mocha_fallback();
    run_frames(|ui| dag::chrome::draw(ui, &theme));
    run_frames(|ui| dag::runner::draw(ui, &theme));
}

#[test]
fn dag_detail_와_states_specimen_은_헤드리스로_렌더된다() {
    let theme = tasty_themes::mocha_fallback();
    run_frames(|ui| dag::detail::draw(ui, &theme));
    run_frames(|ui| dag::states::draw(ui, &theme));
}

#[test]
fn dag_surface_specimen_은_헤드리스로_렌더된다() {
    let theme = tasty_themes::mocha_fallback();
    run_frames(|ui| dag::surface::draw(ui, &theme));
}

#[test]
fn 공통_그래프_뷰_상태_specimen_은_헤드리스로_렌더된다() {
    let theme = tasty_themes::mocha_fallback();
    run_frames(|ui| dag::canvas::draw_states(ui, &theme));
}

#[test]
fn dag_rows_와_window_specimen_은_헤드리스로_렌더된다() {
    let theme = tasty_themes::mocha_fallback();
    run_frames(|ui| dag::rows::draw(ui, &theme));
    run_frames(|ui| dag::window::draw(ui, &theme));
    run_frames(|ui| dag::window::draw_detail(ui, &theme));
}

/// 부팅·종료 예제가 공유하는 그리기 경로를 여러 상태로 실행한다.
#[test]
fn loading_specimen_은_헤드리스로_렌더된다() {
    let theme = tasty_themes::mocha_fallback();
    run_frames(|ui| chrome_loading::draw_default(ui, &theme));
    run_frames(|ui| chrome_loading::draw_min(ui, &theme));
    run_frames(|ui| chrome_loading::draw_phases(ui, &theme));
    run_frames(|ui| chrome_loading::draw_no_text(ui, &theme));
    run_frames(|ui| chrome_loading::draw_latte(ui, &theme));
    run_frames(|ui| chrome_loading::draw_shutdown(ui, &theme));
}

#[test]
fn dag_레이아웃은_task_상태에_영향받지_않는다() {
    let theme = tasty_themes::mocha_fallback();
    let before = dag::layout(
        &dag::build_dag(),
        &theme,
        tasty_dag_layout::Orientation::TopDown,
    );
    let mut mutated = dag::build_dag();
    for n in &mut mutated.nodes {
        n.status = dag::Status::Succeeded;
        n.dur = Some("999s".into());
    }
    let after = dag::layout(&mutated, &theme, tasty_dag_layout::Orientation::TopDown);
    assert_eq!(
        before.nodes, after.nodes,
        "task 상태가 DAG 레이아웃 좌표를 바꿨다"
    );
}

#[test]
fn 신규_오버레이_specimen_은_헤드리스로_렌더된다() {
    use tasty_gallery::catalog::components::{
        drop_overlay, info_modal, notification_panel, quit_modal, script_confirm,
    };
    let theme = tasty_themes::mocha_fallback();
    run_frames(|ui| {
        notification_panel::draw(ui, &theme);
        info_modal::draw(ui, &theme);
        script_confirm::draw(ui, &theme);
        quit_modal::draw(ui, &theme);
        drop_overlay::draw(ui, &theme);
    });
}

#[test]
fn 신규_크롬_specimen_은_헤드리스로_렌더된다() {
    use tasty_gallery::catalog::components::{empty_surface, plugins_window, titlebar};
    let theme = tasty_themes::mocha_fallback();
    run_frames(|ui| {
        titlebar::draw(ui, &theme);
        empty_surface::draw(ui, &theme);
        plugins_window::draw(ui, &theme);
    });
}

#[test]
fn layout_shell_specimen_은_헤드리스로_렌더된다() {
    // 여러 프레임을 실행해 thread_local 상태 재사용 과정의 중복 대여도 확인한다.
    use tasty_gallery::catalog::components::prim_layout_shell;
    let theme = tasty_themes::mocha_fallback();
    run_frames(|ui| prim_layout_shell::draw(ui, &theme));
}

/// 갤러리 Spec 그리기 함수의 형태.
type SpecFn = fn(&mut egui::Ui, &tasty_type_appearance::theme::Theme);

#[test]
fn 시안에서_옮긴_결정_specimen_은_id_충돌_없이_렌더된다() {
    use tasty_gallery::catalog::components::{dag, file_picker, remote};
    use tasty_gallery::catalog::{
        components_settled, foundations_settled, layouts_settled, overlays_settled, plugins_settled,
    };
    let theme = tasty_themes::mocha_fallback();
    let specs: [(&str, SpecFn); 22] = [
        ("shadows", foundations_settled::draw_shadows),
        ("half-pixel", foundations_settled::draw_half_pixel),
        ("tint", foundations_settled::draw_tint),
        ("structural", foundations_settled::draw_structural),
        ("one-keycap", components_settled::draw_one_keycap),
        ("glyph-sizes", components_settled::draw_glyph_sizes),
        ("dot-family", components_settled::draw_dot_family),
        ("mixed-split", layouts_settled::draw_mixed_split),
        ("attention-scale", layouts_settled::draw_attention_scale),
        ("profile-form", remote::draw_profile_form),
        ("generic-passkey", remote::draw_generic_passkey_forms),
        ("path-fit", file_picker::draw_path_fit),
        ("plugin-identity", overlays_settled::draw_plugin_identity),
        ("diff-toolbar", overlays_settled::draw_diff_toolbar),
        ("drag-handles", overlays_settled::draw_drag_handles),
        ("spinner-hero", chrome_loading::draw_spinner_hero),
        ("dag-window", dag::window::draw),
        ("dag-window-detail", dag::window::draw_detail),
        ("doc-background", plugins_settled::draw_doc_background),
        (
            "quote-find",
            tasty_gallery::catalog::components::markdown_viewer::quote_find::spec().draw,
        ),
        ("address-states", plugins_settled::draw_address_states),
        (
            "md-large-file",
            tasty_gallery::catalog::components::md_large_file::draw,
        ),
    ];
    for (label, draw) in specs {
        assert_no_id_clash(label, |ui| draw(ui, &theme));
    }
}

#[test]
fn folder_picker_specimens_have_no_duplicate_ids() {
    let theme = tasty_themes::mocha_fallback();
    assert_no_id_clash("filepicker-folder-mode", |ui| {
        tasty_gallery::catalog::components::file_picker::draw_folder_mode(ui, &theme)
    });
}
