use std::collections::BTreeSet;

use crate::file::format::{
    DetectorDecl, DetectorRuleDecl, DetectorRuleKind, FileFormatRegistry, RuleOrigin,
};
use crate::i18n::t;

use super::{AddDetectorForm, FileHandlerEditDraft, draw_intro_block};
use tasty_ui_widgets::vspace;

/// Detectors sub-tab — Enabled 토글, user-origin 삭제, user 추가 form.
pub(super) fn draw_detectors(
    ui: &mut egui::Ui,
    fh: &mut FileHandlerEditDraft,
    file_format: &FileFormatRegistry,
) {
    draw_intro_block(
        ui,
        "settings.file_handler.detectors.description",
        &[
            "settings.file_handler.detectors.bullet_what",
            "settings.file_handler.detectors.bullet_rules",
            "settings.file_handler.detectors.bullet_origin",
            "settings.file_handler.detectors.bullet_status",
            "settings.file_handler.detectors.bullet_add",
        ],
    );

    let th = crate::theme::theme();
    let ids = file_format.list_detectors();
    if ids.is_empty() {
        ui.label(t("settings.file_handler.detectors.empty"));
    } else {
        egui::Grid::new("file_handler_detectors_grid")
            .num_columns(5)
            .striped(true)
            .spacing(egui::vec2(th.spacing_md.value(), th.spacing_xs.value()))
            .show(ui, |ui| {
                ui.strong(t("settings.file_handler.detectors.col_status"));
                ui.strong(t("settings.file_handler.detectors.col_id"));
                ui.strong(t("settings.file_handler.detectors.col_origin"));
                ui.strong(t("settings.file_handler.detectors.col_rules"));
                ui.strong(""); // actions
                ui.end_row();

                for id in &ids {
                    let Some(det) = file_format.detector(id) else {
                        continue;
                    };
                    // 효과 상태: draft 우선, 없으면 registry 상태.
                    let want_enabled = fh
                        .detector_enabled
                        .get(id)
                        .copied()
                        .unwrap_or(!det.disabled);
                    let mut checked = want_enabled;
                    if tasty_ui_widgets::switch(ui, &th, &mut checked, None, true).changed() {
                        fh.detector_enabled.insert(id.clone(), checked);
                    }
                    ui.label(id.as_str());
                    ui.label(detector_origins_summary(&file_format.rule_origins(id)));
                    ui.label(rule_kinds_summary(&det.rules));
                    // user 출처 contribution 이 있으면 Remove 버튼. finalize 된 rule 의 origin 으로
                    // 판정하지 않는다 — 같은 rule 을 plugin 도 적으면 dedupe 가 user 를 지운다.
                    let has_user = file_format.has_user_contribution(id);
                    let pending_remove = fh.remove_detector.contains(id);
                    if has_user {
                        ui.horizontal(|ui| {
                            if pending_remove {
                                ui.weak(t("settings.file_handler.common.pending_remove"));
                                if ui.small_button(t("button.cancel")).clicked() {
                                    fh.remove_detector.remove(id);
                                }
                            } else if ui
                                .small_button(t("settings.file_handler.detectors.remove_user"))
                                .clicked()
                            {
                                fh.remove_detector.insert(id.clone());
                            }
                        });
                    } else {
                        ui.label("");
                    }
                    ui.end_row();
                }
            });
    }

    vspace(ui, th.spacing_md);
    ui.separator();
    vspace(ui, th.spacing_xs);

    // "+ Add user detector" inline form.
    draw_add_detector_form(ui, fh);
}

fn draw_add_detector_form(ui: &mut egui::Ui, fh: &mut FileHandlerEditDraft) {
    let th = crate::theme::theme();
    let toggle_label = if fh.add_detector_form.open {
        t("settings.file_handler.detectors.add_close")
    } else {
        t("settings.file_handler.detectors.add_open")
    };
    if ui.button(toggle_label).clicked() {
        fh.add_detector_form.open = !fh.add_detector_form.open;
        fh.add_detector_form.error = None;
    }
    if !fh.add_detector_form.open {
        // 이미 추가된 draft 항목 표시.
        for decl in &fh.add_detector {
            ui.weak(format!(
                "{} {}",
                t("settings.file_handler.detectors.pending_add"),
                decl.id
            ));
        }
        return;
    }
    egui::Frame::new()
        .inner_margin(egui::vec2(th.spacing_sm.value(), th.spacing_xs.value()))
        .stroke(ui.visuals().widgets.noninteractive.bg_stroke)
        .show(ui, |ui| {
            egui::Grid::new("file_handler_detectors_add_grid")
                .num_columns(2)
                .spacing(egui::vec2(th.spacing_sm.value(), th.spacing_xs.value()))
                .show(ui, |ui| {
                    ui.label(t("settings.file_handler.detectors.field_id"));
                    ui.add(
                        egui::TextEdit::singleline(&mut fh.add_detector_form.id_input)
                            .hint_text("my-detector")
                            .desired_width(th.field_width_lg.value()),
                    );
                    ui.end_row();

                    ui.label(t("settings.file_handler.detectors.field_extensions"));
                    ui.add(
                        egui::TextEdit::singleline(&mut fh.add_detector_form.extensions_input)
                            .hint_text("md, mdx")
                            .desired_width(th.field_width_lg.value()),
                    );
                    ui.end_row();

                    ui.label(t("settings.file_handler.detectors.field_path_glob"));
                    ui.add(
                        egui::TextEdit::singleline(&mut fh.add_detector_form.path_glob_input)
                            .hint_text("Dockerfile")
                            .desired_width(th.field_width_lg.value()),
                    );
                    ui.end_row();
                });
            if let Some(err) = &fh.add_detector_form.error {
                ui.colored_label(crate::theme::theme().red, err);
            }
            ui.horizontal(|ui| {
                if ui.button(t("button.add")).clicked() {
                    match build_add_detector_decl(&fh.add_detector_form) {
                        Ok(decl) => {
                            fh.add_detector.push(decl);
                            fh.add_detector_form = AddDetectorForm::default();
                        }
                        Err(e) => fh.add_detector_form.error = Some(e),
                    }
                }
                if ui.button(t("button.cancel")).clicked() {
                    fh.add_detector_form = AddDetectorForm::default();
                }
            });
        });
    for decl in &fh.add_detector {
        ui.weak(format!(
            "{} {}",
            t("settings.file_handler.detectors.pending_add"),
            decl.id
        ));
    }
}

fn build_add_detector_decl(form: &AddDetectorForm) -> Result<DetectorDecl, String> {
    let id = form.id_input.trim().to_string();
    if id.is_empty() {
        return Err(t("settings.file_handler.detectors.err_id_empty").to_string());
    }
    if !crate::file::format::is_valid_detector_id(&id) {
        return Err(t("settings.file_handler.detectors.err_id_invalid").to_string());
    }
    let mut rules = Vec::new();
    let exts: Vec<String> = form
        .extensions_input
        .split(|c: char| c == ',' || c.is_whitespace())
        .filter_map(|s| {
            let v = s.trim().trim_start_matches('.').to_ascii_lowercase();
            (!v.is_empty()).then_some(v)
        })
        .collect();
    if !exts.is_empty() {
        rules.push(DetectorRuleDecl::Extension { values: exts });
    }
    let glob = form.path_glob_input.trim();
    if !glob.is_empty() {
        rules.push(DetectorRuleDecl::PathGlob {
            pattern: glob.to_string(),
        });
    }
    if rules.is_empty() {
        return Err(t("settings.file_handler.detectors.err_no_rules").to_string());
    }
    Ok(DetectorDecl {
        id,
        display_name_i18n_key: None,
        icon: None,
        disabled: None,
        rule: rules,
    })
}

fn detector_origins_summary(rule_origins: &[RuleOrigin]) -> String {
    let mut origins: BTreeSet<String> = BTreeSet::new();
    for origin in rule_origins {
        let label = match origin {
            RuleOrigin::HostDefault => "host".to_string(),
            RuleOrigin::Plugin(id) => format!("plugin:{}", id),
            RuleOrigin::User => "user".to_string(),
        };
        origins.insert(label);
    }
    if origins.is_empty() {
        "—".into()
    } else {
        origins.into_iter().collect::<Vec<_>>().join(", ")
    }
}

fn rule_kinds_summary(rules: &[crate::file::format::DetectorRule]) -> String {
    let mut kinds: Vec<&str> = rules
        .iter()
        .map(|r| match &r.kind {
            DetectorRuleKind::Extension { .. } => "ext",
            DetectorRuleKind::PathGlob { .. } => "glob",
            DetectorRuleKind::Mime { .. } => "mime",
            DetectorRuleKind::Magic { .. } => "magic",
            DetectorRuleKind::IsDirectory => "dir",
            DetectorRuleKind::Lua { .. } => "lua",
            DetectorRuleKind::StructureCheck { .. } => "structure",
            DetectorRuleKind::Unknown { .. } => "?",
        })
        .collect();
    kinds.sort();
    kinds.dedup();
    if kinds.is_empty() {
        "—".into()
    } else {
        kinds.join(", ")
    }
}
