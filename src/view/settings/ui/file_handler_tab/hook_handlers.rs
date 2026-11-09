//! 훅 핸들러 레지스트리 설정. 웹훅 리스너의 주소·포트·비밀키는 다루지 않는다.
//! 변경은 초안에 보관하고 Save하면 레지스트리와 사용자 TOML에 저장한다.
//!
//! 모든 출처의 활성 상태를 바꿀 수 있고 ShellCommand는 명령을 편집할 수 있다.
//! 삭제는 user 항목만 허용하며 host/plugin 항목에는 자물쇠를 표시한다.
//! 사용자 patch 가 걸린 host/plugin 항목은 출처 Tag 뒤에 edited Tag 를, 둘째 줄에 Revert 를 단다.
//! Revert 는 초안에만 들어가며(Undo · "reverts on save") Save 하면 그 patch 를 지운다.
//! IpcSequence는 요약 한 줄과 Edit 이다. Edit 은 그 행 둘째 줄을 한 줄에 호출 하나인 문자열 편집기로
//! 바꾸고, Apply 한 결과는 초안에 들어간다. 한 줄 형식으로 쓸 수 없는 시퀀스는 편집기 대신
//! `tasty hook-handler get` 명령을 복사하게 한다.
//! 관련 문서: docs/features/hooks/index.md.

use std::collections::{BTreeMap, BTreeSet};

use tasty_type_geometry::length::LogicalPx;
use tasty_ui_widgets::{
    Button, ButtonVariant, CodeAreaKeys, ControlSize, Ground, IconButton, IconButtonVariant, Input,
    SequenceEditorAction, SequenceEditorError, SequenceEditorView, TagVariant, sequence_editor,
    switch, tag, vspace,
};

use crate::adapters::ui::icons;
use crate::adapters::ui::input::shortcuts::consume_binding_egui;
use crate::hook_handler::config::UserHookHandlerActionDecl;
use crate::hook_handler::registry::UserHookHandlerUpsertDecl;
use crate::hook_handler::sequence_text::{
    SequenceTextError, SequenceTextErrorKind, format_sequence, parse_sequence,
};
use crate::hook_handler::types::IpcCall;
use crate::hook_handler::types::is_valid_hook_handler_short_name;
use crate::hook_handler::{
    HookHandler, HookHandlerAction, HookHandlerId, HookHandlerOwner, HookSource,
};
use crate::i18n::{t, t_fmt, t_fmt2};

/// jsx `HookRow` line 2 의 "Shell cmd:" 라벨 폭 (`width: 74, flex: none`).
const HOOK_CMD_LABEL_W: LogicalPx = LogicalPx(74.0);
/// jsx add-draft 카드 필드 라벨 폭 (`width: 100, flex: none`).
const HOOK_ADD_LABEL_W: LogicalPx = LogicalPx(100.0);
/// 신규 핸들러 priority step (jsx `commitAdd`: `maxPrio + 10`).
const HOOK_PRIORITY_STEP: i32 = 10;

/// 훅 핸들러 변경 초안. Save에서 명시적인 user override로 반영한다.
#[derive(Debug, Clone, Default)]
pub(crate) struct HookHandlerEditDraft {
    /// handler id → 사용자가 원하는 enabled 상태. 없으면 변경 없음.
    enabled: BTreeMap<HookHandlerId, bool>,
    /// ShellCommand 행의 인라인 명령 편집 (전체 명령 문자열).
    cmd_edits: BTreeMap<HookHandlerId, String>,
    /// IpcSequence 편집기에서 Apply 한 호출 목록. host/plugin 행도 user override 로 저장한다.
    seq_edits: BTreeMap<HookHandlerId, Vec<IpcCall>>,
    /// 열려 있는 IpcSequence 편집기. 한 번에 한 행이다.
    seq_editor: Option<SeqEditor>,
    /// user-origin entry 삭제 의도.
    remove: BTreeSet<HookHandlerId>,
    /// host/plugin 항목에 걸린 사용자 patch 를 지워 기본값으로 되돌릴 의도.
    revert: BTreeSet<HookHandlerId>,
    /// 새로 추가될 user 핸들러 (short-name, 셸 명령).
    add: Vec<PendingHookAdd>,
    /// "Add handler" 인라인 draft 폼 상태.
    form: AddHookForm,
}

/// 열린 IpcSequence 편집기 — 대상 행과 편집 중인 문자열.
#[derive(Debug, Clone)]
struct SeqEditor {
    id: HookHandlerId,
    text: String,
    /// 처음 그리는 프레임에 포커스를 요청한다. egui 는 그 프레임에 그리지 않은 위젯의 포커스를
    /// 풀므로 Edit 을 누른 프레임에는 요청할 수 없다.
    focus: bool,
}

/// 이번 프레임에 IpcSequence 행에서 일어난 일.
enum SeqRowEvent {
    Open(HookHandlerId, String),
    Apply(HookHandlerId, Vec<IpcCall>),
    Close,
}

/// draft 에 쌓인 신규 user 핸들러 한 건.
#[derive(Debug, Clone)]
struct PendingHookAdd {
    short: String,
    cmd: String,
    priority: i32,
}

/// "Add handler" 인라인 draft 폼 (jsx `adding`/`draftId`/`draftCmd`).
#[derive(Debug, Clone, Default)]
struct AddHookForm {
    open: bool,
    id_input: String,
    cmd_input: String,
    error: Option<String>,
}

impl HookHandlerEditDraft {
    pub(crate) fn into_edits(self) -> Vec<crate::app::settings_edit::RegistryEdit> {
        use crate::app::settings_edit::RegistryEdit as E;
        // 되돌리기를 먼저 적용한다. 그 뒤에 남은 편집은 기본값 위에 새 patch 로 얹힌다.
        self.revert
            .into_iter()
            .map(E::RemoveHook)
            .chain(
                self.enabled
                    .into_iter()
                    .map(|(id, value)| E::HookEnabled(id, value)),
            )
            .chain(self.remove.into_iter().map(E::RemoveHook))
            .chain(self.cmd_edits.into_iter().map(|(id, cmd)| {
                E::UpsertHook(UserHookHandlerUpsertDecl {
                    id: id.as_str().to_owned(),
                    source: Some(HookSource::Hook),
                    priority: None,
                    display_name_i18n_key: None,
                    disabled: None,
                    action: Some(UserHookHandlerActionDecl::ShellCommand {
                        command: cmd,
                        args: Vec::new(),
                    }),
                })
            }))
            .chain(self.seq_edits.into_iter().map(|(id, calls)| {
                // 시퀀스만 바꾼다. source 를 비워 두어야 webhook 에 묶인 핸들러의 출처 게이트가 유지된다.
                E::UpsertHook(UserHookHandlerUpsertDecl {
                    id: id.as_str().to_owned(),
                    source: None,
                    priority: None,
                    display_name_i18n_key: None,
                    disabled: None,
                    action: Some(UserHookHandlerActionDecl::IpcSequence { calls }),
                })
            }))
            .chain(self.add.into_iter().map(|add| {
                E::UpsertHook(UserHookHandlerUpsertDecl {
                    id: format!("user/{}", add.short),
                    source: Some(HookSource::Hook),
                    priority: Some(add.priority),
                    display_name_i18n_key: None,
                    disabled: Some(false),
                    action: Some(UserHookHandlerActionDecl::ShellCommand {
                        command: add.cmd,
                        args: Vec::new(),
                    }),
                })
            }))
            .collect()
    }
}

/// Hook Handlers sub-tab 콘텐츠 (jsx `HookHandlers` 전사).
pub(super) fn draw_hook_handlers(
    ui: &mut egui::Ui,
    hh: &mut HookHandlerEditDraft,
    kb: &crate::settings::KeybindingSettings,
) {
    let th = crate::theme::theme();
    vspace(ui, th.spacing_xs);

    // ── intro row: 설명 paragraph(flex 1, measure-md) + "Add handler" 버튼 ──
    ui.horizontal_top(|ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
            if Button::new(t("settings.file_handler.hook_handlers.add_button"))
                .variant(ButtonVariant::Secondary)
                .size(ControlSize::Sm)
                .leading_icon(&|ui, rect, c| {
                    icons::PLUS.image(rect.width(), c).paint_at(ui, rect);
                })
                .show(ui, &th)
                .clicked()
            {
                hh.form = AddHookForm {
                    open: true,
                    ..AddHookForm::default()
                };
            }
            ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                ui.set_max_width(th.measure_md.value());
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(t("settings.file_handler.hook_handlers.description"))
                            .size(th.font_size_caption.value())
                            .color(th.text_muted()),
                    )
                    .wrap(),
                );
            });
        });
    });
    vspace(ui, th.spacing_md);

    // ── "Add handler" 인라인 draft 카드 (jsx `adding && …`) ──
    if hh.form.open {
        draw_add_card(ui, &th, hh, &crate::runtime::file_catalog::hook_handlers());
        vspace(ui, th.spacing_md);
    }

    // ── 대문자 섹션 헤드 (jsx `headStyle`) ──
    label_caps_head(
        ui,
        &th,
        t("settings.file_handler.hook_handlers.section_head"),
    );
    vspace(ui, th.spacing_xs);

    // ── 등록 핸들러 rows (registry 정렬순: priority↑ → owner → id) + draft 추가분 ──
    let rows = crate::runtime::file_catalog::hook_handlers();
    let defaults = crate::runtime::file_catalog::hook_handler_defaults();
    if rows.is_empty() && hh.add.is_empty() {
        ui.label(
            egui::RichText::new(t("settings.file_handler.hook_handlers.empty"))
                .size(th.font_size_caption.value())
                .color(th.text_muted()),
        );
        return;
    }
    let mut toggle: Option<(HookHandlerId, bool)> = None;
    let mut cmd_edit: Option<(HookHandlerId, String)> = None;
    let mut remove_toggle: Option<HookHandlerId> = None;
    let mut revert_toggle: Option<HookHandlerId> = None;
    let mut seq_event: Option<SeqRowEvent> = None;
    let mut editor = hh.seq_editor.take();
    for h in &rows {
        let mut out = RowOutput {
            toggle: &mut toggle,
            cmd_edit: &mut cmd_edit,
            remove_toggle: &mut remove_toggle,
            revert_toggle: &mut revert_toggle,
            seq_event: &mut seq_event,
        };
        let item = RowItem {
            h,
            default: defaults.get(&h.id),
        };
        draw_hook_row(ui, &th, hh, &item, kb, &mut editor, &mut out);
    }
    hh.seq_editor = editor;
    match seq_event {
        Some(SeqRowEvent::Open(id, text)) => {
            hh.seq_editor = Some(SeqEditor {
                id,
                text,
                focus: true,
            });
        }
        Some(SeqRowEvent::Apply(id, calls)) => {
            hh.seq_edits.insert(id, calls);
            hh.seq_editor = None;
        }
        Some(SeqRowEvent::Close) => hh.seq_editor = None,
        None => {}
    }
    // draft 로 추가된 행 — jsx 는 commitAdd 가 목록에 바로 push 하므로 pending add
    // 도 동일 비주얼의 행으로 노출한다 (Save 전까지는 draft 에만 존재).
    let mut remove_add: Option<usize> = None;
    for (i, add) in hh.add.iter_mut().enumerate() {
        draw_pending_add_row(ui, &th, i, add, &mut remove_add);
    }
    if let Some((id, enabled)) = toggle {
        hh.enabled.insert(id, enabled);
    }
    if let Some((id, cmd)) = cmd_edit {
        hh.cmd_edits.insert(id, cmd);
    }
    if let Some(id) = remove_toggle
        && !hh.remove.remove(&id)
    {
        hh.remove.insert(id);
    }
    if let Some(i) = remove_add {
        hh.add.remove(i);
    }
    if let Some(id) = revert_toggle {
        hh.toggle_revert(id);
    }
}

impl HookHandlerEditDraft {
    /// Revert ↔ Undo. Revert 는 그 행의 초안 편집도 함께 버린다 — patch 전부를 한 번에 되돌리는 동작이다.
    fn toggle_revert(&mut self, id: HookHandlerId) {
        if !self.revert.remove(&id) {
            self.enabled.remove(&id);
            self.cmd_edits.remove(&id);
            self.seq_edits.remove(&id);
            if self.seq_editor.as_ref().is_some_and(|ed| ed.id == id) {
                self.seq_editor = None;
            }
            self.revert.insert(id);
        }
    }
}

/// 대문자 라벨 글꼴 섹션 제목. 자간은 시안의 `letter-spacing-caps` 다.
fn label_caps_head(ui: &mut egui::Ui, th: &tasty_type_appearance::theme::Theme, text: &str) {
    ui.label(
        egui::RichText::new(text.to_uppercase())
            .family(tasty_ui_widgets::label_font_family())
            .size(th.font_size_micro.value())
            .extra_letter_spacing(th.letter_spacing_caps(th.font_size_micro).value())
            .color(th.text_muted()),
    );
}

/// 한 행이 돌려주는 변경 의도. 초안은 모든 행을 그린 뒤 한 번에 고친다.
struct RowOutput<'a> {
    toggle: &'a mut Option<(HookHandlerId, bool)>,
    cmd_edit: &'a mut Option<(HookHandlerId, String)>,
    remove_toggle: &'a mut Option<HookHandlerId>,
    revert_toggle: &'a mut Option<HookHandlerId>,
    seq_event: &'a mut Option<SeqRowEvent>,
}

/// 그릴 행 — 병합된 핸들러와, 사용자 patch 가 걸린 host/plugin 행이면 그 기본값.
struct RowItem<'a> {
    h: &'a HookHandler,
    default: Option<&'a HookHandler>,
}

/// 편집기 글자 영역의 id. 편집기를 처음 그린 프레임에 이 id 로 포커스를 요청한다.
fn seq_editor_id(id: &HookHandlerId) -> egui::Id {
    egui::Id::new(("hook_seq_editor", id.as_str()))
}

/// 한 핸들러 행 (jsx `HookRow` 전사) — 2행 컬럼 + 하단 separator + disabled 시
/// row 전체 opacity.
fn draw_hook_row(
    ui: &mut egui::Ui,
    th: &tasty_type_appearance::theme::Theme,
    hh: &HookHandlerEditDraft,
    item: &RowItem<'_>,
    kb: &crate::settings::KeybindingSettings,
    editor: &mut Option<SeqEditor>,
    out: &mut RowOutput<'_>,
) {
    let RowOutput {
        toggle,
        cmd_edit,
        remove_toggle,
        revert_toggle,
        seq_event,
    } = out;
    let h = item.h;
    // 병합 결과의 owner 는 patch 를 단 User 다. 출처 Tag·자물쇠는 기본값의 owner 를 따른다.
    let origin = item.default.map_or(&h.owner, |d| &d.owner);
    let pending_revert = item.default.is_some() && hh.revert.contains(&h.id);
    // 되돌리기 대기 중이면 Save 뒤 모습(기본값)을 보인다.
    let shown = match item.default {
        Some(d) if pending_revert => d,
        _ => h,
    };
    // 되돌리기를 걸면 그 행의 Switch 편집이 지워지므로 대기 중에는 기본값이 보인다.
    let on = hh.enabled.get(&h.id).copied().unwrap_or(!shown.disabled);
    let pending_remove = hh.remove.contains(&h.id);
    let is_shell = matches!(shown.action, HookHandlerAction::ShellCommand { .. });
    let seq_calls: Option<&[IpcCall]> = match &shown.action {
        HookHandlerAction::IpcSequence { calls } => Some(
            hh.seq_edits
                .get(&h.id)
                .map_or(calls.as_slice(), Vec::as_slice),
        ),
        HookHandlerAction::ShellCommand { .. } => None,
    };
    let cmd_display = match seq_calls {
        Some(calls) => sequence_summary(calls),
        None => hh
            .cmd_edits
            .get(&h.id)
            .cloned()
            .unwrap_or_else(|| action_display(&shown.action)),
    };
    let (origin_label, origin_variant) = origin_tag(origin);
    // Revert/Undo 슬롯 — 둘째 줄 Edit 왼쪽(셸 행은 Input 오른쪽)이다.
    let mut revert_slot = |ui: &mut egui::Ui| {
        if item.default.is_some() && revert_button(ui, th, pending_revert, origin_label) {
            **revert_toggle = Some(h.id.clone());
        }
    };

    let resp = ui.scope(|ui| {
        // jsx padding: sm(상하) xs(좌우), 내부 행간 gap xs.
        egui::Frame::NONE
            .inner_margin(egui::Margin {
                left: th.spacing_xs.value() as i8,
                right: th.spacing_xs.value() as i8,
                top: th.spacing_sm.value() as i8,
                bottom: th.spacing_sm.value() as i8,
            })
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = th.spacing_xs.value();
                // ── line 1: id · origin Tag · prio · (우측) Switch + remove ──
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = th.spacing_sm.value();
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        // 오른쪽부터 배치한다. user 행은 삭제 버튼, host/plugin은 자물쇠를 쓴다.
                        if matches!(origin, HookHandlerOwner::User) {
                            if IconButton::new()
                                .variant(IconButtonVariant::Ghost)
                                .size(ControlSize::Sm)
                                .show(ui, th, &|ui, rect, c| {
                                    icons::TRASH.image(rect.width(), c).paint_at(ui, rect);
                                })
                                .clicked()
                            {
                                **remove_toggle = Some(h.id.clone());
                            }
                        } else {
                            lock_slot(ui, th);
                        }
                        if pending_remove {
                            ui.label(
                                egui::RichText::new(t(
                                    "settings.file_handler.common.pending_remove",
                                ))
                                .size(th.font_size_caption.value())
                                .color(th.text_muted()),
                            );
                        }
                        // 되돌리기 대기 중에는 Undo 나 Save 전까지 Switch 를 잠근다.
                        let mut checked = on;
                        let switch_resp = switch(ui, th, &mut checked, None, !pending_revert);
                        if pending_revert {
                            switch_resp.on_hover_text(t(
                                "settings.file_handler.hook_handlers.pending_locked_tip",
                            ));
                        } else if switch_resp.changed() {
                            **toggle = Some((h.id.clone(), checked));
                        }
                        // 좌측 나머지 (LTR 로 되돌림): id + Tag + prio.
                        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                            ui.spacing_mut().item_spacing.x = th.spacing_sm.value();
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(h.id.as_str())
                                        .monospace()
                                        .strong()
                                        .size(th.font_size_body.value())
                                        .color(if on {
                                            th.text_primary()
                                        } else {
                                            th.text_muted()
                                        }),
                                )
                                .truncate(),
                            );
                            tag(ui, th, origin_label, origin_variant, false);
                            if item.default.is_some() {
                                edited_tag(ui, th, pending_revert);
                            }
                            ui.label(
                                egui::RichText::new(t_fmt(
                                    "settings.file_handler.hook_handlers.prio",
                                    &h.priority.to_string(),
                                ))
                                .family(tasty_ui_widgets::label_font_family())
                                .size(th.font_size_micro.value())
                                .color(th.text_muted()),
                            );
                        });
                    });
                });
                // ── line 2: action — 셸 명령 인라인 Input / IpcSequence 요약 또는 편집기 ──
                if let Some(ed) = editor.as_mut().filter(|ed| ed.id == h.id) {
                    if let Some(event) = draw_seq_editor(ui, th, ed, kb) {
                        **seq_event = Some(event);
                    }
                    return;
                }
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = th.spacing_sm.value();
                    row_label(
                        ui,
                        th,
                        if is_shell {
                            t("settings.file_handler.hook_handlers.shell_cmd_label")
                        } else {
                            t("settings.file_handler.hook_handlers.action_label")
                        },
                    );
                    if is_shell {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            revert_slot(ui);
                            let mut buf = cmd_display.clone();
                            if Input::new()
                                .mono(true)
                                .enabled(on)
                                .show(ui, th, &mut buf)
                                .changed()
                            {
                                **cmd_edit = Some((h.id.clone(), buf));
                            }
                        });
                    } else if let Some(calls) = seq_calls {
                        seq_summary_line(
                            ui,
                            th,
                            SeqLine {
                                h,
                                calls,
                                summary: cmd_display,
                                locked: pending_revert,
                            },
                            seq_event,
                            &mut revert_slot,
                        );
                    }
                });
            });
    });
    row_separator(ui, th, resp.response.rect);
}

/// draft 로 추가된(아직 Save 전) user 핸들러 행 — 실제 행과 동일 비주얼.
fn draw_pending_add_row(
    ui: &mut egui::Ui,
    th: &tasty_type_appearance::theme::Theme,
    index: usize,
    add: &mut PendingHookAdd,
    remove_add: &mut Option<usize>,
) {
    let resp = ui.scope(|ui| {
        egui::Frame::NONE
            .inner_margin(egui::Margin {
                left: th.spacing_xs.value() as i8,
                right: th.spacing_xs.value() as i8,
                top: th.spacing_sm.value() as i8,
                bottom: th.spacing_sm.value() as i8,
            })
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = th.spacing_xs.value();
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = th.spacing_sm.value();
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if IconButton::new()
                            .variant(IconButtonVariant::Ghost)
                            .size(ControlSize::Sm)
                            .show(ui, th, &|ui, rect, c| {
                                icons::TRASH.image(rect.width(), c).paint_at(ui, rect);
                            })
                            .clicked()
                        {
                            *remove_add = Some(index);
                        }
                        ui.label(
                            egui::RichText::new(t(
                                "settings.file_handler.hook_handlers.pending_add",
                            ))
                            .size(th.font_size_caption.value())
                            .color(th.text_muted()),
                        );
                        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                            ui.spacing_mut().item_spacing.x = th.spacing_sm.value();
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(format!("user/{}", add.short))
                                        .monospace()
                                        .strong()
                                        .size(th.font_size_body.value())
                                        .color(th.text_primary()),
                                )
                                .truncate(),
                            );
                            tag(ui, th, "user", TagVariant::Default, false);
                            ui.label(
                                egui::RichText::new(t_fmt(
                                    "settings.file_handler.hook_handlers.prio",
                                    &add.priority.to_string(),
                                ))
                                .family(tasty_ui_widgets::label_font_family())
                                .size(th.font_size_micro.value())
                                .color(th.text_muted()),
                            );
                        });
                    });
                });
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = th.spacing_sm.value();
                    row_label(
                        ui,
                        th,
                        t("settings.file_handler.hook_handlers.shell_cmd_label"),
                    );
                    Input::new().mono(true).show(ui, th, &mut add.cmd);
                });
            });
    });
    row_separator(ui, th, resp.response.rect);
}

/// jsx HookRow line 2 라벨 — `width:74, fontSize caption, text-muted`.
fn row_label(ui: &mut egui::Ui, th: &tasty_type_appearance::theme::Theme, text: &str) {
    ui.allocate_ui_with_layout(
        egui::vec2(HOOK_CMD_LABEL_W.value(), th.input_height().value()),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.label(
                egui::RichText::new(text)
                    .size(th.font_size_caption.value())
                    .color(th.text_muted()),
            );
        },
    );
}

/// 행 하단 1px separator (jsx `borderBottom: 1px solid separator`).
fn row_separator(ui: &mut egui::Ui, th: &tasty_type_appearance::theme::Theme, rect: egui::Rect) {
    ui.painter().hline(
        rect.x_range(),
        rect.bottom(),
        egui::Stroke::new(
            th.border_width.value(),
            th.separator.to_egui_premultiplied(),
        ),
    );
}

/// 자물쇠 슬롯 — 휴지통 IconButton 과 **같은 크기의 자리**를 차지한다. 행마다 우측
/// 끝이 어긋나지 않게 하려는 것이고, 누를 수 있는 것이 아니라 읽는 표시다.
fn lock_slot(ui: &mut egui::Ui, th: &tasty_type_appearance::theme::Theme) {
    let side = ControlSize::Sm.height(th);
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(side, side), egui::Sense::hover());
    let glyph = ControlSize::Sm.icon_glyph(th);
    let icon_rect = egui::Rect::from_center_size(rect.center(), egui::vec2(glyph, glyph));
    icons::LOCK
        .image(icon_rect.width(), th.glyph_dim().into())
        .paint_at(ui, icon_rect);
    resp.on_hover_text(t("settings.file_handler.hook_handlers.not_removable"));
}

/// 출처 Tag 뒤의 표시 — 평소 "edited"(툴팁: 기본값 갱신이 더는 적용되지 않음),
/// 되돌리기 대기 중이면 기본 Tag "reverts on save".
fn edited_tag(ui: &mut egui::Ui, th: &tasty_type_appearance::theme::Theme, pending: bool) {
    if pending {
        tag(
            ui,
            th,
            t("settings.file_handler.hook_handlers.reverts_on_save"),
            TagVariant::Default,
            false,
        );
    } else {
        tag(
            ui,
            th,
            t("settings.file_handler.hook_handlers.edited"),
            TagVariant::Default,
            false,
        )
        .on_hover_text(t("settings.file_handler.hook_handlers.edited_tip"));
    }
}

/// Revert(툴팁: 어느 출처의 기본값으로 가는지) 또는 대기 중이면 같은 자리의 Undo. 눌렸는가를 돌려준다.
fn revert_button(
    ui: &mut egui::Ui,
    th: &tasty_type_appearance::theme::Theme,
    pending: bool,
    origin: &str,
) -> bool {
    let label = if pending {
        t("settings.file_handler.hook_handlers.undo")
    } else {
        t("settings.file_handler.hook_handlers.revert")
    };
    let resp = Button::new(label)
        .variant(ButtonVariant::Ghost)
        .size(ControlSize::Sm)
        .show(ui, th);
    let resp = if pending {
        resp
    } else {
        resp.on_hover_text(t_fmt(
            "settings.file_handler.hook_handlers.revert_tip",
            origin,
        ))
    };
    resp.clicked()
}

/// 출처 표시: host, plugin ID, 사용자(you).
fn origin_tag(owner: &HookHandlerOwner) -> (&str, TagVariant) {
    match owner {
        HookHandlerOwner::Host => ("host", TagVariant::Default),
        HookHandlerOwner::Plugin(id) => (id.as_str(), TagVariant::Agent),
        HookHandlerOwner::User => ("you", TagVariant::Default),
    }
}

/// IpcSequence 요약 줄이 그리는 행 내용.
struct SeqLine<'a> {
    h: &'a HookHandler,
    calls: &'a [IpcCall],
    summary: String,
    /// 되돌리기 대기 중이면 Undo 나 Save 전까지 Edit 을 잠근다.
    locked: bool,
}

/// IpcSequence 요약 줄 — mono 한 줄 요약(줄어드는 항목) · 오른쪽 끝 Edit. 한 줄 형식으로 쓸 수 없는
/// 시퀀스는 Edit 대신 "Edit with CLI"(툴팁 = 명령) + 명령 복사 IconButton 이다.
fn seq_summary_line(
    ui: &mut egui::Ui,
    th: &tasty_type_appearance::theme::Theme,
    line: SeqLine<'_>,
    seq_event: &mut Option<SeqRowEvent>,
    revert_slot: &mut dyn FnMut(&mut egui::Ui),
) {
    let SeqLine {
        h,
        calls,
        summary,
        locked,
    } = line;
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        match format_sequence(calls) {
            Ok(text) => {
                if Button::new(t("settings.file_handler.hook_handlers.seq_edit"))
                    .variant(ButtonVariant::Ghost)
                    .size(ControlSize::Sm)
                    .enabled(!locked)
                    .show(ui, th)
                    .clicked()
                {
                    *seq_event = Some(SeqRowEvent::Open(h.id.clone(), text));
                }
            }
            Err(_) => {
                let command = format!("tasty hook-handler get --id {}", h.id.as_str());
                let copy = IconButton::new()
                    .variant(IconButtonVariant::Ghost)
                    .size(ControlSize::Sm)
                    .show(ui, th, &|ui, rect, c| {
                        icons::COPY.image(rect.width(), c).paint_at(ui, rect);
                    });
                if copy.clicked() {
                    ui.ctx().copy_text(command.clone());
                }
                copy.on_hover_text(t("settings.file_handler.hook_handlers.seq_copy_cli"));
                ui.label(
                    egui::RichText::new(t("settings.file_handler.hook_handlers.seq_edit_with_cli"))
                        .size(th.font_size_caption.value())
                        .color(th.text_muted()),
                )
                .on_hover_text(command);
            }
        }
        revert_slot(ui);
        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
            ui.add(
                egui::Label::new(
                    egui::RichText::new(summary)
                        .monospace()
                        .size(th.font_size_term_sm.value())
                        .color(th.text_secondary()),
                )
                .truncate(),
            );
        });
    });
}

/// 열린 편집기. 입력할 때마다 해석하고 첫 오류를 보인다. Apply 는 해석 결과를, Cancel 은 닫기를 돌려준다.
fn draw_seq_editor(
    ui: &mut egui::Ui,
    th: &tasty_type_appearance::theme::Theme,
    ed: &mut SeqEditor,
    kb: &crate::settings::KeybindingSettings,
) -> Option<SeqRowEvent> {
    let alert = |ui: &mut egui::Ui, rect: egui::Rect, c: egui::Color32| {
        icons::ALERT_CIRCLE
            .image(rect.width(), c)
            .paint_at(ui, rect);
    };
    let super_held = crate::adapters::ui::input::shortcuts::super_held(ui.ctx());
    let check = |text: &str| {
        parse_sequence(text)
            .map(|calls| calls.len())
            .map_err(editor_error)
    };
    let view = SequenceEditorView {
        placeholder: "system.info",
        help: t("settings.file_handler.hook_handlers.seq_help"),
        empty_note: t("settings.file_handler.hook_handlers.seq_empty"),
        cancel: t("button.cancel"),
        apply: t("settings.file_handler.hook_handlers.seq_apply"),
        alert_icon: &alert,
        check: &check,
        keys: Some(CodeAreaKeys {
            submit: &|i| consume_binding_egui(&kb.code_area_apply, i, super_held),
            cancel: &|i| consume_binding_egui(&kb.code_area_cancel, i, super_held),
        }),
    };
    let action = sequence_editor(
        ui,
        th,
        ("hook_seq_editor", ed.id.as_str()),
        &view,
        &mut ed.text,
    );
    if ed.focus {
        ui.memory_mut(|m| m.request_focus(seq_editor_id(&ed.id)));
        ui.ctx().request_repaint();
        ed.focus = false;
    }
    match action {
        SequenceEditorAction::Apply => parse_sequence(&ed.text)
            .ok()
            .map(|calls| SeqRowEvent::Apply(ed.id.clone(), calls)),
        SequenceEditorAction::Cancel => Some(SeqRowEvent::Close),
        SequenceEditorAction::None => None,
    }
}

/// 해석 오류를 편집기 한 줄로 옮긴다. 문장은 번역하고 serde 원문 이유는 그대로 둔다.
fn editor_error(e: SequenceTextError) -> SequenceEditorError {
    let line = e.line.to_string();
    let (sentence, reason) = match e.kind {
        SequenceTextErrorKind::MissingMethod => (
            t_fmt(
                "settings.file_handler.hook_handlers.seq_err_missing_method",
                &line,
            ),
            None,
        ),
        SequenceTextErrorKind::InvalidMethod => (
            t_fmt(
                "settings.file_handler.hook_handlers.seq_err_invalid_method",
                &line,
            ),
            None,
        ),
        SequenceTextErrorKind::InvalidParams { column, message } => (
            t_fmt2(
                "settings.file_handler.hook_handlers.seq_err_invalid_params",
                &line,
                &column.to_string(),
            ),
            Some(message),
        ),
    };
    SequenceEditorError {
        line: e.line,
        sentence,
        reason,
    }
}

/// IpcSequence 한 줄 요약 — 메서드를 → 로 잇는다.
fn sequence_summary(calls: &[IpcCall]) -> String {
    let methods: Vec<&str> = calls.iter().map(|c| c.method.as_str()).collect();
    format!("ipc: {}", methods.join(" → "))
}

/// action 표시 문자열 — ShellCommand 는 전체 명령, IpcSequence 는 method 요약.
fn action_display(action: &HookHandlerAction) -> String {
    match action {
        HookHandlerAction::ShellCommand { command, args } => {
            if args.is_empty() {
                command.clone()
            } else {
                format!("{command} {}", args.join(" "))
            }
        }
        HookHandlerAction::IpcSequence { calls } => sequence_summary(calls),
    }
}

/// "Add handler" 인라인 draft 카드 (jsx `adding && …` 블록 전사) —
/// surface-raised + 1px border + radius 카드, caps 헤드 + 2 필드 행 + 우측 버튼.
fn draw_add_card(
    ui: &mut egui::Ui,
    th: &tasty_type_appearance::theme::Theme,
    hh: &mut HookHandlerEditDraft,
    handlers: &[HookHandler],
) {
    egui::Frame::new()
        .fill(th.surface_raised().to_egui())
        .stroke(egui::Stroke::new(
            th.border_width.value(),
            th.border_default().to_egui(),
        ))
        .corner_radius(th.corner_radius.value())
        .inner_margin(egui::Margin::same(th.spacing_md.value() as i8))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = th.spacing_sm.value();
            label_caps_head(ui, th, t("settings.file_handler.hook_handlers.new_head"));
            add_field_row(
                ui,
                th,
                t("settings.file_handler.hook_handlers.field_event_id"),
                t("settings.file_handler.hook_handlers.placeholder_event_id"),
                &mut hh.form.id_input,
            );
            add_field_row(
                ui,
                th,
                t("settings.file_handler.hook_handlers.field_shell_command"),
                t("settings.file_handler.hook_handlers.placeholder_shell_command"),
                &mut hh.form.cmd_input,
            );
            if let Some(err) = &hh.form.error {
                ui.label(
                    egui::RichText::new(err)
                        .size(th.font_size_caption.value())
                        .color(th.accent_danger_raised()),
                );
            }
            // 세로 중앙 정렬은 Frame의 남은 높이를 차지하므로 상단 정렬을 사용한다.
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                ui.spacing_mut().item_spacing.x = th.spacing_sm.value();
                // RTL: Add handler(primary, 우측 끝) ← Cancel(ghost).
                let can_add = !hh.form.id_input.trim().is_empty();
                if Button::new(t("settings.file_handler.hook_handlers.add_button"))
                    .variant(ButtonVariant::Primary)
                    .size(ControlSize::Sm)
                    .enabled(can_add)
                    .show(ui, th)
                    .clicked()
                {
                    commit_add(hh, handlers);
                }
                // 카드는 surface-raised다. ghost 글자도 raised 바탕의 색을 쓴다.
                if Button::new(t("button.cancel"))
                    .variant(ButtonVariant::Ghost)
                    .size(ControlSize::Sm)
                    .ground(Ground::Raised)
                    .show(ui, th)
                    .clicked()
                {
                    hh.form = AddHookForm::default();
                }
            });
        });
}

/// add 카드의 라벨(100px) + mono Input 행 (jsx `minHeight settings-row-min-height`).
/// 카드가 surface-raised라 라벨은 text-secondary-raised, Input은 raised 바탕이다.
fn add_field_row(
    ui: &mut egui::Ui,
    th: &tasty_type_appearance::theme::Theme,
    label: &str,
    placeholder: &str,
    buf: &mut String,
) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = th.spacing_lg.value();
        ui.allocate_ui_with_layout(
            egui::vec2(
                HOOK_ADD_LABEL_W.value(),
                th.settings_row_min_height().value(),
            ),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.label(
                    egui::RichText::new(label)
                        .size(th.font_size_body.value())
                        .color(th.text_secondary_raised()),
                );
            },
        );
        Input::new()
            .ground(Ground::Raised)
            .mono(true)
            .placeholder(placeholder)
            .show(ui, th, buf);
    });
}

/// add 폼 확정 (jsx `commitAdd`) — short-name 검증 후 draft 에 push.
/// priority 는 현재 registry rows + pending adds 의 max + step (jsx `maxPrio + 10`).
fn commit_add(hh: &mut HookHandlerEditDraft, handlers: &[HookHandler]) {
    let short = hh.form.id_input.trim().to_string();
    if short.is_empty() {
        hh.form.error = Some(t("settings.file_handler.hook_handlers.err_id_empty").to_string());
        return;
    }
    if !is_valid_hook_handler_short_name(&short) {
        hh.form.error = Some(t("settings.file_handler.hook_handlers.err_id_invalid").to_string());
        return;
    }
    let max_prio = handlers
        .iter()
        .map(|h| h.priority)
        .chain(hh.add.iter().map(|a| a.priority))
        .max()
        .unwrap_or(0);
    hh.add.push(PendingHookAdd {
        short,
        cmd: hh.form.cmd_input.trim().to_string(),
        priority: max_prio + HOOK_PRIORITY_STEP,
    });
    hh.form = AddHookForm::default();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shell_add_draft(short: &str, cmd: &str, priority: i32) -> HookHandlerEditDraft {
        HookHandlerEditDraft {
            add: vec![PendingHookAdd {
                short: short.into(),
                cmd: cmd.into(),
                priority,
            }],
            ..Default::default()
        }
    }

    #[test]
    fn add_preserves_shell_command_and_priority_in_owner_request() {
        use crate::app::settings_edit::RegistryEdit;
        let edits = shell_add_draft("on-deploy", "~/ops/on-deploy.sh", 30).into_edits();
        assert_eq!(edits.len(), 1);
        let RegistryEdit::UpsertHook(h) = &edits[0] else {
            panic!("expected hook upsert")
        };
        assert_eq!(h.id, "user/on-deploy");
        assert_eq!(h.priority, Some(30));
        assert_eq!(h.disabled, Some(false));
        assert!(
            matches!(&h.action, Some(UserHookHandlerActionDecl::ShellCommand { command, args })
            if command == "~/ops/on-deploy.sh" && args.is_empty())
        );
    }

    #[test]
    fn toggle_preserves_requested_enabled_state() {
        use crate::app::settings_edit::RegistryEdit;
        let mut draft = HookHandlerEditDraft::default();
        draft
            .enabled
            .insert(HookHandlerId::new("user/noisy"), false);
        let edits = draft.into_edits();
        assert!(
            matches!(edits.as_slice(), [RegistryEdit::HookEnabled(id, false)] if id.as_str() == "user/noisy")
        );
    }

    /// Apply 한 시퀀스는 host 행이라도 같은 id 의 user override 로 저장되고, 출처·우선순위·활성은 건드리지 않는다.
    #[test]
    fn applied_sequence_saves_as_an_ipc_sequence_override() {
        use crate::app::settings_edit::RegistryEdit;
        let calls = parse_sequence("system.info\nnotification.send {\"title\":\"x\"}").unwrap();
        let mut draft = HookHandlerEditDraft::default();
        draft
            .seq_edits
            .insert(HookHandlerId::new("host/notify"), calls.clone());
        let edits = draft.into_edits();
        let [RegistryEdit::UpsertHook(h)] = edits.as_slice() else {
            panic!("expected one upsert")
        };
        assert_eq!(h.id, "host/notify");
        assert_eq!(h.source, None);
        assert_eq!(h.priority, None);
        assert_eq!(h.disabled, None);
        assert!(matches!(
            &h.action,
            Some(UserHookHandlerActionDecl::IpcSequence { calls: saved }) if *saved == calls
        ));
    }

    /// 편집기 오류 줄은 해석 오류의 줄 번호를 쓰고, params 오류만 serde 원문 이유를 붙인다.
    #[test]
    fn editor_error_keeps_the_line_and_only_params_carry_a_reason() {
        let e = editor_error(parse_sequence("# c\nsystem.info\nnotify {bad}").unwrap_err());
        assert_eq!(e.line, 3);
        assert!(e.reason.is_some());
        let e = editor_error(parse_sequence("system.info\n{\"a\":1}").unwrap_err());
        assert_eq!(e.line, 2);
        assert!(e.reason.is_none());
    }

    #[test]
    fn command_edit_does_not_overwrite_priority_or_enabled_state() {
        use crate::app::settings_edit::RegistryEdit;
        let mut draft = HookHandlerEditDraft::default();
        draft
            .cmd_edits
            .insert(HookHandlerId::new("user/greet"), "echo bye".into());
        let edits = draft.into_edits();
        let [RegistryEdit::UpsertHook(h)] = edits.as_slice() else {
            panic!("expected one upsert")
        };
        assert_eq!(h.id, "user/greet");
        assert_eq!(h.priority, None);
        assert_eq!(h.disabled, None);
        assert!(
            matches!(&h.action, Some(UserHookHandlerActionDecl::ShellCommand { command, .. }) if command == "echo bye")
        );
    }

    #[test]
    fn remove_preserves_original_handler_id() {
        use crate::app::settings_edit::RegistryEdit;
        let mut draft = HookHandlerEditDraft::default();
        draft.remove.insert(HookHandlerId::new("user/gone"));
        assert!(
            matches!(draft.into_edits().as_slice(), [RegistryEdit::RemoveHook(id)] if id.as_str() == "user/gone")
        );
    }

    #[test]
    fn revert_drops_row_edits_and_saves_before_new_edits() {
        use crate::app::settings_edit::RegistryEdit;
        let id = HookHandlerId::new("host/notify");
        let mut draft = HookHandlerEditDraft::default();
        draft.enabled.insert(id.clone(), false);
        draft.cmd_edits.insert(id.clone(), "echo old".into());
        draft.toggle_revert(id.clone());
        assert!(draft.enabled.is_empty() && draft.cmd_edits.is_empty());
        // Undo 는 대기만 푼다.
        draft.toggle_revert(id.clone());
        assert!(draft.revert.is_empty());
        draft.toggle_revert(id.clone());
        // 되돌리기 뒤에 다시 고친 값은 기본값 위의 새 patch 가 되도록 RemoveHook 뒤에 온다.
        draft.enabled.insert(id.clone(), false);
        let edits = draft.into_edits();
        assert!(
            matches!(edits.as_slice(), [RegistryEdit::RemoveHook(a), RegistryEdit::HookEnabled(b, false)] if a == &id && b == &id)
        );
    }

    #[test]
    fn commit_add_validates_and_steps_priority() {
        let mut hh = shell_add_draft("base", "echo", 25);
        let rows = vec![HookHandler {
            id: HookHandlerId::new("user/existing"),
            source: HookSource::Hook,
            priority: 40,
            owner: HookHandlerOwner::User,
            action: HookHandlerAction::ShellCommand {
                command: "echo existing".into(),
                args: vec![],
            },
            display_name_i18n_key: None,
            disabled: false,
        }];
        hh.form.id_input = "Bad.Name".into();
        commit_add(&mut hh, &rows);
        assert!(hh.form.error.is_some());
        assert_eq!(hh.add.len(), 1);
        hh.form.id_input = "pipeline-done".into();
        hh.form.cmd_input = "tasty notify done".into();
        hh.form.error = None;
        commit_add(&mut hh, &rows);
        assert_eq!(hh.add.len(), 2);
        assert_eq!(hh.add[1].priority, 50);
    }

    /// 행 하나를 그리고 Switch·Edit 을 누른 결과.
    struct RowClicks {
        switch_on: bool,
        switch_disabled: bool,
        toggled: Option<(HookHandlerId, bool)>,
        edit_opened: bool,
    }

    /// 한 프레임을 그려 Switch 트랙과 Edit 글자의 중심을 찾고, 그 자리를 누른 결과를 돌려준다.
    fn click_row_controls(pending: bool) -> RowClicks {
        let th = tasty_themes::mocha_fallback();
        let id = HookHandlerId::new("host/notify");
        let handler = |label: &str| HookHandler {
            id: id.clone(),
            source: HookSource::Hook,
            priority: 10,
            owner: HookHandlerOwner::Host,
            action: HookHandlerAction::IpcSequence {
                calls: parse_sequence(label).unwrap(),
            },
            display_name_i18n_key: None,
            disabled: false,
        };
        // 사용자 patch 가 Switch 를 끈 행이다. 기본값은 켜짐이다.
        let mut merged = handler("system.info");
        merged.owner = HookHandlerOwner::User;
        merged.disabled = true;
        let default = handler("system.info");
        let mut hh = HookHandlerEditDraft::default();
        if pending {
            hh.toggle_revert(id.clone());
        }
        let kb = crate::settings::KeybindingSettings::default();
        let ctx = egui::Context::default();
        // 테스트에서는 번역 키가 그대로 그려져 글자가 길다. 첫 줄 글자가 Switch 를 덮지 않도록 넓게 둔다.
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1600.0, 200.0));
        let edit_label = t("settings.file_handler.hook_handlers.seq_edit").to_string();
        let track = egui::vec2(
            th.switch_track_width().value(),
            th.switch_track_height().value(),
        );
        let frame = |events: Vec<egui::Event>| {
            let mut out = (None, None, None, None, None);
            let output = ctx.run(
                egui::RawInput {
                    screen_rect: Some(screen),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        let item = RowItem {
                            h: &merged,
                            default: Some(&default),
                        };
                        draw_hook_row(
                            ui,
                            &th,
                            &hh,
                            &item,
                            &kb,
                            &mut None,
                            &mut RowOutput {
                                toggle: &mut out.0,
                                cmd_edit: &mut out.1,
                                remove_toggle: &mut out.2,
                                revert_toggle: &mut out.3,
                                seq_event: &mut out.4,
                            },
                        );
                    });
                },
            );
            (output, out)
        };
        let (output, _) = frame(Vec::new());
        let mut switch_at = None;
        let mut edit_at = None;
        let mut switch_fill = None;
        let mut thumbs = Vec::new();
        let mut shapes: Vec<egui::Shape> = output.shapes.into_iter().map(|c| c.shape).collect();
        while let Some(shape) = shapes.pop() {
            match shape {
                egui::Shape::Vec(inner) => shapes.extend(inner),
                egui::Shape::Rect(r) if (r.rect.size() - track).length() < 0.5 => {
                    switch_at = Some(r.rect.center());
                    switch_fill = Some(r.fill);
                }
                egui::Shape::Circle(c) => thumbs.push(c.center),
                egui::Shape::Text(text) if text.galley.text() == edit_label => {
                    edit_at = Some(text.galley.rect.translate(text.pos.to_vec2()).center());
                }
                _ => {}
            }
        }
        let switch_at = switch_at.expect("switch track");
        // 썸이 트랙 중심보다 오른쪽이면 켜짐이다.
        let thumb = thumbs
            .into_iter()
            .find(|c| (c.y - switch_at.y).abs() < 0.5 && (c.x - switch_at.x).abs() <= track.x)
            .expect("switch thumb");
        let switch_on = thumb.x > switch_at.x;
        let switch_disabled = switch_fill == Some(th.state_disabled_fill().to_egui());
        // 이동·누름·뗌을 프레임마다 나눠 실제 입력처럼 클릭한다. 뗀 프레임의 출력을 돌려준다.
        let click = |at: egui::Pos2| {
            let button = |pressed| egui::Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            };
            frame(vec![egui::Event::PointerMoved(at)]);
            frame(vec![button(true)]);
            frame(vec![button(false)]).1
        };
        let toggled = click(switch_at);
        let edited = click(edit_at.expect("Edit label"));
        RowClicks {
            switch_on,
            switch_disabled,
            toggled: toggled.0,
            edit_opened: edited.4.is_some(),
        }
    }

    /// 되돌리기 대기 중인 행은 Switch 를 기본값으로 보이고, Undo 나 Save 전까지 Switch·Edit 을 잠근다.
    #[test]
    fn pending_revert_locks_the_switch_at_the_default_and_the_edit_button() {
        let row = click_row_controls(false);
        assert!(!row.switch_on, "patched row shows its own off state");
        assert!(!row.switch_disabled);
        assert!(
            row.toggled.as_ref().is_some_and(|(_, enabled)| *enabled),
            "{:?}",
            row.toggled
        );
        assert!(row.edit_opened);

        let row = click_row_controls(true);
        assert!(row.switch_on, "pending row shows the default on state");
        assert!(row.switch_disabled, "pending row draws a disabled Switch");
        assert_eq!(row.toggled, None);
        assert!(!row.edit_opened);
    }
}
