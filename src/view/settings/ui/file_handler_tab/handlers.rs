//! File Handlers 서브탭 — 등록 목록(두 줄 행)과 user 추가 카드.
//!
//! handler는 출처가 하나다. user가 만든 handler(`user/<short>`)만 Remove를 갖고 host·plugin
//! handler는 자물쇠다. 같은 short name으로 다시 추가하면 앞선 user handler를 바꾼다.

use crate::file::handler::{
    HandlerAction, HandlerId, HandlerOwner, UserHandlerActionDecl, UserHandlerUpsertDecl,
    run_order_key,
};
use crate::i18n::{t, t_fmt};
use crate::runtime::file_catalog::FormatCatalog as FileFormatRegistry;
use crate::runtime::file_catalog::HandlerCatalog as FileHandlerRegistry;
use tasty_type_geometry::length::LogicalPx;
use tasty_ui_widgets::{
    FhCardLabels, FhLine, FhOrigin, FhRowView, FhSummarySeg, FhTrailing, Ground, Input,
};

use super::{AddHandlerActionKind, AddHandlerForm, FileHandlerEditDraft, RowLabels, draw_fh_intro};

/// 저장 전 행의 상태.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HdlPending {
    Add,
    Replace,
    Remove,
}

/// 목록 한 행의 자료.
#[derive(Debug, Clone, PartialEq, Eq)]
struct HdlRow {
    id: HandlerId,
    owner: HandlerOwner,
    priority: i32,
    detector: String,
    action: String,
    want_on: bool,
    pending: Option<HdlPending>,
}

fn handler_owner_label(owner: &HandlerOwner) -> String {
    match owner {
        HandlerOwner::Host => "host".into(),
        HandlerOwner::Plugin(id) => format!("plugin:{}", id),
        HandlerOwner::User => "user".into(),
    }
}

fn handler_action_summary(action: &HandlerAction) -> String {
    match action {
        HandlerAction::OpenSurface { surface_kind, .. } => format!("surface:{}", surface_kind),
        HandlerAction::Ipc { method, .. } => format!("ipc:{}", method),
        HandlerAction::System => "system".into(),
    }
}

fn decl_action_summary(action: Option<&UserHandlerActionDecl>) -> String {
    match action {
        Some(UserHandlerActionDecl::OpenSurface { surface_kind, .. }) => {
            format!("surface:{}", surface_kind)
        }
        Some(UserHandlerActionDecl::Ipc { method }) => format!("ipc:{}", method),
        Some(UserHandlerActionDecl::System) => "system".into(),
        None => String::new(),
    }
}

/// 등록 handler와 초안을 합쳐 실행 순서(priority → owner user > plugin > host → id)로 정렬한
/// 행을 만든다. 대기 행도 저장 뒤 실행될 자리에 둔다.
fn handler_rows(file_handler: &FileHandlerRegistry, fh: &FileHandlerEditDraft) -> Vec<HdlRow> {
    let mut rows: Vec<HdlRow> = file_handler
        .list_handlers()
        .iter()
        .filter_map(|id| file_handler.handler(id))
        .map(|h| HdlRow {
            want_on: fh
                .handler_enabled
                .get(&h.id)
                .copied()
                .unwrap_or(!h.disabled),
            pending: fh
                .remove_handler
                .contains(&h.id)
                .then_some(HdlPending::Remove),
            owner: h.owner,
            priority: h.priority,
            detector: h.detector.as_str().to_string(),
            action: handler_action_summary(&h.action),
            id: h.id,
        })
        .collect();
    for decl in &fh.add_handler {
        let id = HandlerId(decl.id.clone());
        let next = HdlRow {
            want_on: fh
                .handler_enabled
                .get(&id)
                .copied()
                .unwrap_or(decl.disabled != Some(true)),
            owner: HandlerOwner::User,
            priority: decl.priority.unwrap_or(100),
            detector: decl.detector.clone().unwrap_or_default(),
            action: decl_action_summary(decl.action.as_ref()),
            pending: Some(HdlPending::Add),
            id,
        };
        match rows.iter_mut().find(|r| r.id == next.id) {
            Some(r) => {
                *r = HdlRow {
                    pending: Some(HdlPending::Replace),
                    ..next
                }
            }
            None => rows.push(next),
        }
    }
    rows.sort_by(|a, b| {
        run_order_key(a.priority, &a.owner, &a.id).cmp(&run_order_key(b.priority, &b.owner, &b.id))
    });
    rows
}

/// Handlers sub-tab — 머리, 추가 카드, 등록 목록.
pub(super) fn draw_handlers(
    ui: &mut egui::Ui,
    fh: &mut FileHandlerEditDraft,
    file_format: &FileFormatRegistry,
    file_handler: &FileHandlerRegistry,
) {
    let th = crate::theme::theme();
    ui.spacing_mut().item_spacing.y = th.spacing_md.value();
    if draw_fh_intro(
        ui,
        "settings.file_handler.handlers.description",
        "settings.file_handler.handlers.add",
        fh.add_handler_form.open,
    ) {
        fh.add_handler_form = AddHandlerForm {
            open: true,
            ..AddHandlerForm::default()
        };
    }
    let rows = handler_rows(file_handler, fh);
    if fh.add_handler_form.open {
        let detectors = detector_options(file_format, fh);
        draw_add_handler_card(ui, fh, &rows, &detectors);
    }
    tasty_ui_widgets::fh_list_head(
        ui,
        &th,
        &t_fmt(
            "settings.file_handler.handlers.registered",
            &rows.len().to_string(),
        ),
    );
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        if rows.is_empty() {
            tasty_ui_widgets::fh_empty(ui, &th, t("settings.file_handler.handlers.empty"));
        }
        let labels = RowLabels::get();
        for row in &rows {
            let remove = row.pending == Some(HdlPending::Remove);
            let owner = handler_owner_label(&row.owner);
            let locked = t_fmt("settings.file_handler.common.locked", &owner);
            let prio = t_fmt(
                "settings.file_handler.hook_handlers.prio",
                &row.priority.to_string(),
            );
            let view = FhRowView {
                id: row.id.as_str(),
                origins: vec![FhOrigin {
                    plugin: matches!(row.owner, HandlerOwner::Plugin(_)),
                    label: owner.clone(),
                    struck: false,
                }],
                prio: Some(&prio),
                pending: row.pending.map(|p| match p {
                    HdlPending::Add => t("settings.file_handler.common.pending_added"),
                    HdlPending::Replace => t("settings.file_handler.common.pending_replaced"),
                    HdlPending::Remove => t("settings.file_handler.common.pending_removed"),
                }),
                strike_id: remove,
                on: row.want_on && !remove,
                switch_on: row.want_on,
                switch_disabled: remove,
                trailing: row_trailing(row),
                locked_tooltip: &locked,
                summary: vec![FhSummarySeg {
                    text: format!("{}  →  {}", row.detector, row.action),
                    struck: false,
                }],
                undo_label: labels.undo,
                remove_label: labels.remove,
                sources_label: "",
            };
            let ev = ui
                .push_id(("fh_handler_row", row.id.as_str()), |ui| {
                    tasty_ui_widgets::fh_row(ui, &th, &view)
                })
                .inner;
            if let Some(on) = ev.toggled {
                fh.handler_enabled.insert(row.id.clone(), on);
            }
            if ev.remove {
                fh.remove_handler.insert(row.id.clone());
            }
            if ev.undo {
                undo_handler(fh, row);
            }
        }
    });
}

fn row_trailing(row: &HdlRow) -> FhTrailing {
    if row.owner != HandlerOwner::User {
        FhTrailing::Lock
    } else if row.pending.is_some() {
        FhTrailing::Undo
    } else {
        FhTrailing::Remove
    }
}

fn undo_handler(fh: &mut FileHandlerEditDraft, row: &HdlRow) {
    match row.pending {
        Some(HdlPending::Add) => {
            fh.add_handler.retain(|d| d.id != row.id.as_str());
            fh.handler_enabled.remove(&row.id);
        }
        Some(HdlPending::Replace) => fh.add_handler.retain(|d| d.id != row.id.as_str()),
        Some(HdlPending::Remove) => {
            fh.remove_handler.remove(&row.id);
        }
        None => {}
    }
}

/// Detector Select의 항목 — 등록 detector와 같은 초안에서 추가한 detector.
fn detector_options(file_format: &FileFormatRegistry, fh: &FileHandlerEditDraft) -> Vec<String> {
    let mut ids: Vec<String> = file_format
        .list_detectors()
        .iter()
        .map(|id| id.as_str().to_string())
        .collect();
    for decl in &fh.add_detector {
        if !ids.contains(&decl.id) {
            ids.push(decl.id.clone());
        }
    }
    ids
}

/// 카드가 열린 동안 고른 detector가 목록에서 사라지면 값을 비운다. 트리거는 placeholder로
/// 돌아가고 Add는 다시 고를 때까지 눌리지 않는다.
fn forget_vanished_detector(form: &mut AddHandlerForm, options: &[String]) {
    if form.detector.as_ref().is_some_and(|d| !options.contains(d)) {
        form.detector = None;
    }
}

/// `com.example.foo.open`처럼 점으로 이은 이름인지. 조각은 영숫자 · `_` · `-`다.
fn is_dotted_method(m: &str) -> bool {
    let parts: Vec<&str> = m.split('.').collect();
    parts.len() >= 2
        && parts.iter().all(|p| {
            !p.is_empty()
                && p.chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        })
}

/// 카드의 검증 결과.
#[derive(Debug, Clone, PartialEq, Eq)]
struct HandlerCheck {
    full_id: String,
    exists: bool,
    err_short: Option<&'static str>,
    err_priority: Option<&'static str>,
    err_method: Option<&'static str>,
    can_add: bool,
}

fn check_handler_form(form: &AddHandlerForm, rows: &[HdlRow]) -> HandlerCheck {
    let short = form.short_name_input.trim();
    let full_id = format!("user/{short}");
    let exists = !short.is_empty() && rows.iter().any(|r| r.id.as_str() == full_id);
    let err_short = (!short.is_empty()
        && !crate::file::handler::is_valid_handler_short_name(short))
    .then(|| t("settings.file_handler.handlers.err_short_chars"));
    let prio = form.priority_input.trim();
    let err_priority = (!prio.is_empty() && prio.parse::<i32>().is_err())
        .then(|| t("settings.file_handler.handlers.err_priority_int"));
    let method = form.action_ipc_method.trim();
    let err_method = (form.action_kind == AddHandlerActionKind::Ipc
        && !method.is_empty()
        && !is_dotted_method(method))
    .then(|| t("settings.file_handler.handlers.err_method"));
    let kind_ready = match form.action_kind {
        AddHandlerActionKind::OpenSurface => !form.action_surface_kind.trim().is_empty(),
        AddHandlerActionKind::Ipc => !method.is_empty(),
        AddHandlerActionKind::System => true,
    };
    HandlerCheck {
        full_id,
        exists,
        err_short,
        err_priority,
        err_method,
        can_add: !short.is_empty() && form.detector.is_some() && kind_ready,
    }
}

fn draw_add_handler_card(
    ui: &mut egui::Ui,
    fh: &mut FileHandlerEditDraft,
    rows: &[HdlRow],
    detectors: &[String],
) {
    let th = crate::theme::theme();
    forget_vanished_detector(&mut fh.add_handler_form, detectors);
    let check = check_handler_form(&fh.add_handler_form, rows);
    let tried = fh.add_handler_form.tried;
    let shown = |e: Option<&'static str>| if tried { e } else { None };
    let add_label = if check.exists && check.err_short.is_none() {
        t("settings.file_handler.handlers.update")
    } else {
        t("settings.file_handler.handlers.add")
    };
    let saved_as = if fh.add_handler_form.short_name_input.trim().is_empty() {
        t_fmt("settings.file_handler.handlers.saved_as", "user/…")
    } else {
        t_fmt("settings.file_handler.handlers.saved_as", &check.full_id)
    };
    let info = (check.exists && check.err_short.is_none()).then(|| {
        t_fmt(
            "settings.file_handler.handlers.info_replace",
            &check.full_id,
        )
    });
    let labels = FhCardLabels {
        title: t("settings.file_handler.handlers.card_title"),
        cancel: t("button.cancel"),
        add: add_label,
    };
    let form = &mut fh.add_handler_form;
    let ev = tasty_ui_widgets::fh_card(ui, &th, labels, None, check.can_add, |ui| {
        tasty_ui_widgets::fh_field(
            ui,
            &th,
            t("settings.file_handler.handlers.field_short_name"),
            FhLine::pick(shown(check.err_short), info.as_deref(), Some(&saved_as)),
            |ui| {
                Input::new()
                    .ground(Ground::Raised)
                    .mono(true)
                    .placeholder("my-viewer")
                    .invalid(shown(check.err_short).is_some())
                    .show(ui, &th, &mut form.short_name_input);
            },
        );
        tasty_ui_widgets::fh_field(
            ui,
            &th,
            t("settings.file_handler.handlers.field_detector"),
            FhLine::None,
            |ui| {
                let names: Vec<&str> = detectors.iter().map(String::as_str).collect();
                let mut selected = form
                    .detector
                    .as_ref()
                    .and_then(|d| detectors.iter().position(|x| x == d));
                let width = th
                    .file_handler_select_width()
                    .min(LogicalPx(ui.available_width()))
                    .value();
                if tasty_ui_widgets::select_or_placeholder_on(
                    ui,
                    &th,
                    Ground::Raised,
                    "file_handler_add_detector",
                    &mut selected,
                    &names,
                    t("settings.file_handler.handlers.field_detector_select"),
                    width,
                    true,
                ) {
                    form.detector = selected.and_then(|i| detectors.get(i).cloned());
                }
            },
        );
        tasty_ui_widgets::fh_field(
            ui,
            &th,
            t("settings.file_handler.handlers.field_priority"),
            FhLine::pick(
                shown(check.err_priority),
                None,
                Some(t("settings.file_handler.handlers.prio_hint")),
            ),
            |ui| {
                Input::new()
                    .ground(Ground::Raised)
                    .mono(true)
                    .placeholder("100")
                    .invalid(shown(check.err_priority).is_some())
                    .width(th.file_handler_priority_width().value())
                    .show(ui, &th, &mut form.priority_input);
            },
        );
        tasty_ui_widgets::fh_field(
            ui,
            &th,
            t("settings.file_handler.handlers.field_action_kind"),
            if form.action_kind == AddHandlerActionKind::System {
                FhLine::Caption(t("settings.file_handler.handlers.system_hint"))
            } else {
                FhLine::None
            },
            |ui| {
                let names: Vec<&str> = AddHandlerActionKind::ALL
                    .iter()
                    .map(|k| t(k.label_key()))
                    .collect();
                let mut idx = AddHandlerActionKind::ALL
                    .iter()
                    .position(|k| *k == form.action_kind)
                    .unwrap_or(0);
                let width = th
                    .file_handler_kind_width()
                    .min(LogicalPx(ui.available_width()))
                    .value();
                if tasty_ui_widgets::select_on(
                    ui,
                    &th,
                    Ground::Raised,
                    "file_handler_add_action_kind",
                    &mut idx,
                    &names,
                    width,
                    true,
                ) && let Some(k) = AddHandlerActionKind::ALL.get(idx)
                {
                    form.action_kind = *k;
                }
            },
        );
        match form.action_kind {
            AddHandlerActionKind::OpenSurface => {
                tasty_ui_widgets::fh_field(
                    ui,
                    &th,
                    t("settings.file_handler.handlers.field_surface_kind"),
                    FhLine::None,
                    |ui| {
                        Input::new()
                            .ground(Ground::Raised)
                            .mono(true)
                            .placeholder("markdown_view")
                            .show(ui, &th, &mut form.action_surface_kind);
                    },
                );
                tasty_ui_widgets::fh_field(
                    ui,
                    &th,
                    t("settings.file_handler.handlers.field_param_key"),
                    FhLine::Caption(t("settings.file_handler.handlers.param_hint")),
                    |ui| {
                        Input::new()
                            .ground(Ground::Raised)
                            .mono(true)
                            .placeholder("file")
                            .show(ui, &th, &mut form.action_param_key);
                    },
                );
            }
            AddHandlerActionKind::Ipc => {
                tasty_ui_widgets::fh_field(
                    ui,
                    &th,
                    t("settings.file_handler.handlers.field_ipc_method"),
                    FhLine::pick(shown(check.err_method), None, None),
                    |ui| {
                        Input::new()
                            .ground(Ground::Raised)
                            .mono(true)
                            .placeholder("com.example.foo.open")
                            .invalid(shown(check.err_method).is_some())
                            .show(ui, &th, &mut form.action_ipc_method);
                    },
                );
            }
            AddHandlerActionKind::System => {}
        }
    });
    if ev.cancel {
        fh.add_handler_form = AddHandlerForm::default();
    } else if ev.add {
        commit_handler(fh, rows);
    }
}

/// Add를 누른다. 오류가 있으면 표시만 켜고, 없으면 초안에 넣고 카드를 닫는다.
fn commit_handler(fh: &mut FileHandlerEditDraft, rows: &[HdlRow]) {
    fh.add_handler_form.tried = true;
    let check = check_handler_form(&fh.add_handler_form, rows);
    if !check.can_add
        || check.err_short.is_some()
        || check.err_priority.is_some()
        || check.err_method.is_some()
    {
        return;
    }
    let Some(decl) = build_add_handler_decl(&fh.add_handler_form) else {
        return;
    };
    let id = HandlerId(decl.id.clone());
    fh.remove_handler.remove(&id);
    fh.add_handler.retain(|d| d.id != decl.id);
    fh.add_handler.push(decl);
    fh.add_handler_form = AddHandlerForm::default();
}

/// 검증을 통과한 카드 입력을 저장할 선언으로 바꾼다. priority는 비우면 100, param key는
/// 비우면 `file`이다. 저장 id는 `user/<short>`다.
fn build_add_handler_decl(form: &AddHandlerForm) -> Option<UserHandlerUpsertDecl> {
    let short = form.short_name_input.trim();
    let detector = form.detector.clone()?;
    let prio = form.priority_input.trim();
    let priority = if prio.is_empty() {
        100
    } else {
        prio.parse::<i32>().ok()?
    };
    let action = match form.action_kind {
        AddHandlerActionKind::OpenSurface => {
            let pk = form.action_param_key.trim();
            UserHandlerActionDecl::OpenSurface {
                surface_kind: form.action_surface_kind.trim().to_string(),
                param_key: if pk.is_empty() {
                    "file".to_string()
                } else {
                    pk.to_string()
                },
            }
        }
        AddHandlerActionKind::Ipc => UserHandlerActionDecl::Ipc {
            method: form.action_ipc_method.trim().to_string(),
        },
        AddHandlerActionKind::System => UserHandlerActionDecl::System,
    };
    Some(UserHandlerUpsertDecl {
        id: format!("user/{short}"),
        detector: Some(detector),
        priority: Some(priority),
        display_name_i18n_key: None,
        disabled: None,
        action: Some(action),
    })
}

#[cfg(test)]
mod tests;
