//! User-owned in-app tutorials. Runtime, history and painting have separate boundaries.
pub mod callout;
mod catalog;
pub mod marker;
mod progress;
mod runtime;
pub mod topic_popup;
pub use catalog::{MarkerTarget, all_topics};
pub use runtime::{PracticeContext, PracticeEvent, TutorialRuntime};

use crate::adapters::ui::LayoutContext;
use crate::i18n::t;
use crate::intent::{OpenPopupMode, UiIntent};
use crate::state::MainViewState;
use tasty_type_appearance::theme::Theme;

pub fn resolve_marker_rect(
    target: MarkerTarget,
    content: egui::Rect,
    pane: Option<egui::Rect>,
    surface: Option<egui::Rect>,
    tab_height: f32,
) -> Option<egui::Rect> {
    match target {
        MarkerTarget::ContentArea => Some(content),
        MarkerTarget::Pane => pane,
        MarkerTarget::Surface => surface,
        MarkerTarget::TabHeader => pane.map(|p| {
            egui::Rect::from_min_size(p.min, egui::vec2(p.width(), tab_height.min(p.height())))
        }),
        MarkerTarget::Summary => None,
    }
}

/// The catalog can be opened by menu, shortcut or the existing debug UI path.
pub fn open_catalog(state: &mut MainViewState) {
    if let Some(a) = state.tutorial.active {
        state.tutorial.interrupt();
        state.tutorial.save_progress(a.topic);
    }
    state.tutorial.load_progress();
}

pub fn interrupt_and_reopen(state: &mut MainViewState) {
    if let Some(a) = state.tutorial.active {
        state.tutorial.interrupt();
        state.tutorial.save_progress(a.topic);
    }
    state.dispatch_intent(
        UiIntent::OpenPopup {
            id: topic_popup::TUTORIAL_TOPICS_POPUP_ID,
            mode: OpenPopupMode::CenteredFocused(crate::model::popup_kind::PopupScope::Window),
        }
        .from_user_menu("tutorial.reopen"),
    );
}

pub fn draw_tutorial_overlay(
    ctx: &egui::Context,
    state: &mut MainViewState,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
    layout: &LayoutContext,
    content: egui::Rect,
    theme: &Theme,
) {
    state.tutorial.start_pending();
    let Some(a) = state.tutorial.active else {
        return;
    };
    // Focused popups own their input. The palette itself fulfills the discovery step.
    if state.popups.has_focused() || state.fullscreen_stage_active() {
        return;
    }
    let topic = &all_topics()[a.topic];
    let step = &topic.steps[a.step];
    let in_workspace = state.tutorial.practice.is_none_or(|c| {
        engine
            .workspace_at(state.active_workspace_index(engine))
            .is_some_and(|w| w.id == c.workspace)
    });
    let pane_id = state
        .tutorial
        .practice
        .map(|c| c.pane)
        .or_else(|| state.focused_pane(engine).map(|p| p.id));
    let pane = pane_id
        .and_then(|id| layout.pane_rects.iter().find(|(pid, _)| *pid == id))
        .map(|(_, r)| *r);
    let surface_id = if let Some(c) = state.tutorial.practice {
        engine
            .find_pane_by_id(c.pane)
            .and_then(|p| p.tabs.get(state.navigation.tab_index(p)))
            .and_then(|t| state.navigation.surface_id(t))
    } else {
        state.focused_surface_id(engine)
    };
    let surface = surface_id
        .and_then(|id| layout.surface_rects.iter().find(|(sid, _)| *sid == id))
        .map(|(_, r)| *r);
    let target = in_workspace
        .then(|| {
            resolve_marker_rect(
                step.target,
                content,
                pane,
                surface,
                theme.tab_bar_height.value(),
            )
        })
        .flatten();
    let missing = !in_workspace || (step.target != MarkerTarget::Summary && target.is_none());
    let screen = ctx.screen_rect();
    if let Some(rect) = target {
        marker::paint_spotlight(ctx, screen, rect, theme);
    }
    let mut body = if missing {
        t("tutorial.target_missing").to_owned()
    } else {
        t(step.body_key).to_owned()
    };
    if let Some(action) = step.action {
        let bindings = engine
            .settings
            .keybindings
            .get_bindings(action)
            .unwrap_or_default();
        let shortcut = bindings
            .first()
            .map(|b| {
                tasty_settings::KeybindingSettings::format_display(b, &engine.settings.general)
            })
            .unwrap_or_else(|| t("tutorial.binding_unset").to_owned());
        body.push_str(&format!(
            "\n\n{}: {}",
            t("tutorial.binding_label"),
            shortcut
        ));
    }
    let ready = state.tutorial.ready() && !missing;
    if step.requirement != catalog::Requirement::Read && ready {
        body.push_str(&format!("\n\n{}", t("tutorial.action_done")));
    }
    if state.tutorial.save_error {
        body.push_str(&format!("\n\n{}", t("tutorial.save_error")));
    }
    if state.tutorial.setup_error {
        body.push_str(&format!("\n\n{}", t("tutorial.setup_error")));
    }
    let prepare = step.requirement == catalog::Requirement::Prepare && !ready;
    let size = callout::measure_callout(ctx, theme, t(step.title_key), &body, screen.size());
    let placement = callout::place_callout(
        target.unwrap_or(egui::Rect::from_center_size(
            screen.center(),
            egui::Vec2::ZERO,
        )),
        size,
        screen,
        theme.spacing_md.value(),
        theme.spacing_sm.value(),
    );
    let click = callout::draw_callout(
        ctx,
        theme,
        placement,
        callout::CalloutProps {
            step: a.step + 1,
            total: topic.steps.len(),
            title: t(step.title_key),
            body: &body,
            first: a.step == 0,
            last: a.step + 1 == topic.steps.len(),
            size,
            ready: ready || (prepare && !state.tutorial.preparing),
            prepare,
            keyboard_focus: state.tutorial.keyboard_focus,
            anchored: target.is_some_and(|target| {
                !target.intersects(egui::Rect::from_min_size(placement.pos, size))
            }),
        },
    );
    state.tutorial.callout_rect = Some(click.rect);
    if click.clicked
        || !matches!(
            click.action,
            callout::CalloutClick::None | callout::CalloutClick::Practice
        )
    {
        state.tutorial.keyboard_focus = true;
    }
    match click.action {
        callout::CalloutClick::Next if prepare => {
            let ticket = state.tutorial.begin_preparation();
            state.dispatch_intent(
                crate::intent::Intent::PrepareTutorial { ticket }
                    .from_user_menu("tutorial.prepare"),
            );
        }
        callout::CalloutClick::Next => {
            let done = state.tutorial.next();
            state.tutorial.save_progress(a.topic);
            if done {
                interrupt_and_reopen(state);
            }
        }
        callout::CalloutClick::Back => {
            state.tutorial.back();
            state.tutorial.save_progress(a.topic);
        }
        callout::CalloutClick::Skip => interrupt_and_reopen(state),
        callout::CalloutClick::Practice => {
            state.tutorial.keyboard_focus = false;
        }
        callout::CalloutClick::None => {}
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn r(x: f32, y: f32, w: f32, h: f32) -> egui::Rect {
        egui::Rect::from_min_size(egui::pos2(x, y), egui::vec2(w, h))
    }

    #[test]
    fn content_area_target_always_resolves() {
        let content = r(100.0, 0.0, 900.0, 600.0);
        let got = resolve_marker_rect(MarkerTarget::ContentArea, content, None, None, 24.0);
        assert_eq!(got, Some(content));
    }

    #[test]
    fn tab_header_is_top_strip_of_pane() {
        let content = r(100.0, 0.0, 900.0, 600.0);
        let pane = r(100.0, 0.0, 900.0, 580.0);
        let got = resolve_marker_rect(MarkerTarget::TabHeader, content, Some(pane), None, 24.0)
            .expect("resolves");
        assert_eq!(got.min, pane.min);
        assert_eq!(got.width(), pane.width());
        assert_eq!(got.height(), 24.0);
    }

    #[test]
    fn pane_and_surface_pass_through() {
        let content = r(100.0, 0.0, 900.0, 600.0);
        let pane = r(100.0, 24.0, 900.0, 556.0);
        let surface = r(108.0, 32.0, 884.0, 540.0);
        assert_eq!(
            resolve_marker_rect(MarkerTarget::Pane, content, Some(pane), Some(surface), 24.0),
            Some(pane)
        );
        assert_eq!(
            resolve_marker_rect(
                MarkerTarget::Surface,
                content,
                Some(pane),
                Some(surface),
                24.0
            ),
            Some(surface)
        );
    }

    #[test]
    fn unresolved_pane_surface_is_none() {
        let content = r(100.0, 0.0, 900.0, 600.0);
        assert_eq!(
            resolve_marker_rect(MarkerTarget::Pane, content, None, None, 24.0),
            None
        );
        assert_eq!(
            resolve_marker_rect(MarkerTarget::Surface, content, None, None, 24.0),
            None
        );
    }
}
