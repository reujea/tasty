//! Lua 스크립트 관리와 Windows용 tastyrc 편집.
//! 단축키는 조회만 하며 편집은 Keybindings의 Scripts 화면에서 한다.

use std::collections::HashMap;
use tasty_type_geometry::length::LogicalPx;

use tasty_ui_widgets::{
    Button, ButtonVariant, CenterState, ControlSize, IconButton, IconButtonVariant, Input, kbd,
    script_trigger_add_control, script_trigger_menu,
};

use crate::adapters::ui::icons;
use crate::i18n::t;
use crate::settings::{AUTO_TRIGGER_EVENTS, AutoTrigger, KeybindingSettings, Settings, hash_file};
#[cfg(windows)] // tastyrc 섹션(Windows 전용)에서만 사용.
use tasty_ui_widgets::vspace;

/// changed 배지 고정 높이 (디자인 `height: 16`).
const BADGE_HEIGHT: LogicalPx = LogicalPx(16.0);
// 빈 상태 글리프 크기는 `tasty-ui-widgets::tokens` 가 단일 출처다 — 갤러리
// specimen(`components/script_manager.rs`)이 같은 상수를 읽는다.
use tasty_ui_widgets::tokens::STRUCT_GAP_2;
/// Add card 라벨 컬럼 폭 (디자인 `width: 100`).
const ADD_LABEL_W: LogicalPx = LogicalPx(100.0);

/// Misc › Scripts 관리 창의 UI-only 상호작용 상태. 스크립트 데이터 자체는
/// `Settings.scripts`(draft)에 있다.
#[derive(Default)]
pub struct ScriptsUiState {
    /// Add card 표시 중.
    adding: bool,
    /// Add card 파일 경로 입력 draft.
    draft_path: String,
    /// Add card 표시 이름 입력 draft.
    draft_name: String,
    /// 인라인 rename 중인 스크립트 id.
    rename_id: Option<String>,
    /// rename 입력 draft.
    rename_draft: String,
    /// 인라인 remove 확인 중인 스크립트 id.
    confirm_id: Option<String>,
    /// id → changed(디스크 해시 ≠ 저장 해시). `None` = 미계산(다음 draw 에서 계산).
    /// add/remove 시 무효화(재계산). 오픈마다 리셋.
    changed: Option<HashMap<String, bool>>,
    /// Add card 의 Browse… 가 눌렸다. 파일 선택 popup 은 설정 창 popup 매니저가 소유하므로
    /// 이 탭은 요청만 남기고, 결과는 [`ScriptsUiState::apply_browsed_file`] 로 돌아온다.
    browse_requested: bool,
}

impl ScriptsUiState {
    /// 설정 창 파일 선택에서 이 탭의 결과를 가려내는 키.
    pub(crate) const BROWSE_CONSUMER: &'static str = "scripts_add_file";

    /// Browse… 요청을 1 회 가져간다.
    pub(crate) fn take_browse_request(&mut self) -> bool {
        std::mem::take(&mut self.browse_requested)
    }

    /// 고른 파일을 Add card 에 채운다. 표시 이름이 비어 있으면 파일 stem 으로 채운다.
    pub(crate) fn apply_browsed_file(&mut self, path: &std::path::Path) {
        if self.draft_name.trim().is_empty()
            && let Some(stem) = path.file_stem()
        {
            self.draft_name = stem.to_string_lossy().into_owned();
        }
        self.draft_path = path.to_string_lossy().into_owned();
    }
}

/// RTL 액션 클러스터에서 kbd 키캡이 역순으로 그려지는 것을 상쇄하려 combo 파트를
/// 미리 뒤집는다(`"Ctrl+Shift+J"` → `"J+Shift+Ctrl"` → RTL 렌더 후 화면상 정순).
fn rtl_combo(combo: &str) -> String {
    combo.split('+').rev().collect::<Vec<_>>().join("+")
}

/// 리스트 순회 중에는 registry 를 변형할 수 없으므로 한 프레임의 변형을 지연 수집한다.
enum Pending {
    /// id 의 표시 이름을 변경.
    Rename(String, String),
    /// id 를 제거(+ 연결된 단축키 해제).
    Remove(String),
    /// id 에 자동실행 트리거 추가.
    AddTrigger(String, AutoTrigger),
    /// id 의 자동실행 트리거 제거.
    RemoveTrigger(String, AutoTrigger),
}

/// tastyrc 섹션: Tasty 모드에서 적용되는 bashrc 사용자 영역 편집.
#[cfg(windows)]
pub fn draw_tastyrc_subtab(ui: &mut egui::Ui, bashrc_user_draft: &mut Option<String>) {
    let th = crate::theme::theme();

    ui.heading(t("settings.misc.bashrc.heading"));
    vspace(ui, th.spacing_xs);
    ui.label(
        egui::RichText::new(t("settings.misc.bashrc.description"))
            .small()
            .color(th.text_muted()),
    );
    vspace(ui, th.spacing_sm);

    // draft는 mod.rs 진입부에서 lazy 로드되므로 이 시점엔 항상 Some.
    let draft = bashrc_user_draft.get_or_insert_with(crate::settings::general::load_user_bashrc);

    ui.horizontal(|ui| {
        if ui.button(t("settings.misc.bashrc.reset_button")).clicked() {
            *draft = crate::settings::general::INITIAL_USER_BASHRC.to_string();
        }
    });
    vspace(ui, th.spacing_xs);

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .drag_to_scroll(false)
        .show(ui, |ui| {
            ui.add(
                egui::TextEdit::multiline(draft)
                    .font(egui::TextStyle::Monospace)
                    .desired_rows(20)
                    .desired_width(f32::INFINITY)
                    .code_editor(),
            );
        });
}

// ── Scripts 관리 창 ───────────────────────────────────────────────────────

/// Misc › Scripts — 등록 Lua 스크립트 목록 관리(추가/이름변경/제거) + 바운드
/// 단축키 표시 + changed(해시 불일치) 시각화. 반환 `true` = bind 버튼 클릭
/// (호출측이 Keybindings › Scripts 로 진입).
pub fn draw_scripts_subtab(
    ui: &mut egui::Ui,
    settings: &mut Settings,
    st: &mut ScriptsUiState,
) -> bool {
    let th = crate::theme::theme();
    let mut goto_keybindings = false;

    // changed 캐시 — 미계산이면 각 스크립트의 디스크 해시를 저장 해시와 비교.
    if st.changed.is_none() {
        let mut map = HashMap::new();
        for e in settings.scripts.iter() {
            // 읽기 실패(파일 없음 등)는 changed 로 표시하지 않는다(별도 에러 UI 없음).
            let changed = hash_file(&e.path).map(|h| h != e.sha256).unwrap_or(false);
            map.insert(e.id.clone(), changed);
        }
        st.changed = Some(map);
    }

    // ── 헤더 (좌: 제목 + 설명, 우: Add script) ──
    ui.horizontal_top(|ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
            if Button::new(t("settings.scripts.add"))
                .variant(ButtonVariant::Secondary)
                .size(ControlSize::Sm)
                .leading_icon(&|ui, rect, c| {
                    icons::PLUS.image(rect.width(), c).paint_at(ui, rect);
                })
                .show(ui, &th)
                .clicked()
            {
                st.adding = true;
                st.draft_path.clear();
                st.draft_name.clear();
                st.rename_id = None;
                st.confirm_id = None;
            }
            ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                ui.spacing_mut().item_spacing.y = STRUCT_GAP_2.value();
                ui.label(
                    egui::RichText::new(t("settings.misc.scripts"))
                        .size(th.font_size_max.value())
                        .strong()
                        .color(th.text_primary()),
                );
                ui.set_max_width(th.measure_md.value());
                ui.label(
                    egui::RichText::new(t("settings.scripts.description"))
                        .size(th.font_size_caption.value())
                        .color(th.text_muted()),
                );
            });
        });
    });
    ui.add_space(th.spacing_md.value());

    // ── Add card ──
    if st.adding {
        draw_add_card(ui, &th, settings, st);
        ui.add_space(th.spacing_md.value());
    }

    // ── 목록 / 빈 상태 ──
    if settings.scripts.is_empty() && !st.adding {
        draw_empty(ui, &th);
        return goto_keybindings;
    }

    let mut pending: Option<Pending> = None;
    let ids: Vec<String> = settings.scripts.iter().map(|e| e.id.clone()).collect();
    for id in ids {
        draw_script_row(
            ui,
            &th,
            settings,
            st,
            &id,
            &mut goto_keybindings,
            &mut pending,
        );
    }

    // 지연 변형 적용 + changed 캐시 무효화.
    match pending {
        Some(Pending::Rename(id, name)) => {
            settings.scripts.rename(&id, name);
            st.rename_id = None;
        }
        Some(Pending::Remove(id)) => {
            settings.scripts.remove(&id);
            // 디자인 note: 제거 시 연결된 단축키도 해제. 자동실행 트리거는
            // ScriptEntry 소유라 엔트리 제거로 함께 사라진다.
            settings.keybindings.remove_script_binding(&id);
            st.confirm_id = None;
            st.changed = None;
        }
        Some(Pending::AddTrigger(id, trig)) => {
            settings.scripts.add_trigger(&id, trig);
        }
        Some(Pending::RemoveTrigger(id, trig)) => {
            settings.scripts.remove_trigger(&id, &trig);
        }
        None => {}
    }

    goto_keybindings
}

/// 한 스크립트 행 — 글리프 / 이름·경로·changed / 우측 단축키+액션. 인라인 rename·
/// remove-confirm 상태를 포함한다.
fn draw_script_row(
    ui: &mut egui::Ui,
    th: &tasty_type_appearance::theme::Theme,
    settings: &mut Settings,
    st: &mut ScriptsUiState,
    id: &str,
    goto_keybindings: &mut bool,
    pending: &mut Option<Pending>,
) {
    // 행 데이터 스냅샷(레이아웃 중 registry 재대여 회피).
    let Some(entry) = settings.scripts.get(id) else {
        return;
    };
    let name = entry.name.clone();
    let path_display = abbreviate_home(&entry.path.to_string_lossy());
    let triggers = entry.triggers.clone();
    let changed = st
        .changed
        .as_ref()
        .and_then(|m| m.get(id).copied())
        .unwrap_or(false);
    let combo = settings
        .keybindings
        .script_binding_combo(id)
        .unwrap_or("")
        .to_string();
    let renaming = st.rename_id.as_deref() == Some(id);
    let confirming = st.confirm_id.as_deref() == Some(id);

    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = th.spacing_md.value();
        // 좌: script 글리프 16 · text-muted · margin-top 2.
        ui.vertical(|ui| {
            ui.add_space(STRUCT_GAP_2.value());
            let (rect, _) = ui.allocate_exact_size(
                egui::vec2(th.icon_glyph_size_md.value(), th.icon_glyph_size_md.value()),
                egui::Sense::hover(),
            );
            icons::SCRIPT
                .image(th.icon_glyph_size_md.value(), th.text_muted().to_egui())
                .paint_at(ui, rect);
        });

        if renaming {
            // 중앙 = Input + Save/Cancel (우측 액션·경로 숨김).
            // 세로 중앙 정렬이 남은 높이까지 확장하지 않도록 상단 정렬한다.
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                ui.spacing_mut().item_spacing.x = th.spacing_sm.value();
                let cancel = Button::new(t("button.cancel"))
                    .variant(ButtonVariant::Ghost)
                    .size(ControlSize::Sm)
                    .show(ui, th)
                    .clicked();
                let save = Button::new(t("button.save"))
                    .variant(ButtonVariant::Primary)
                    .size(ControlSize::Sm)
                    .show(ui, th)
                    .clicked();
                let resp = Input::new().show(ui, th, &mut st.rename_draft);
                let enter = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                let esc = ui.input(|i| i.key_pressed(egui::Key::Escape));
                if save || enter {
                    let new = st.rename_draft.trim().to_string();
                    *pending = Some(Pending::Rename(
                        id.to_string(),
                        if new.is_empty() { name.clone() } else { new },
                    ));
                } else if cancel || esc {
                    st.rename_id = None;
                }
            });
        } else {
            // 액션은 오른쪽부터 배치하되 행 높이가 늘지 않도록 상단 정렬한다.
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                ui.spacing_mut().item_spacing.x = th.spacing_sm.value();
                if confirming {
                    // Remove? Cancel Remove (danger 톤).
                    if Button::new(t("settings.scripts.remove"))
                        .variant(ButtonVariant::Danger)
                        .size(ControlSize::Sm)
                        .show(ui, th)
                        .clicked()
                    {
                        *pending = Some(Pending::Remove(id.to_string()));
                    }
                    if Button::new(t("button.cancel"))
                        .variant(ButtonVariant::Ghost)
                        .size(ControlSize::Sm)
                        .show(ui, th)
                        .clicked()
                    {
                        st.confirm_id = None;
                    }
                    ui.label(
                        egui::RichText::new(t("settings.scripts.remove_confirm"))
                            .size(th.font_size_term_sm.value())
                            .color(th.text_secondary()),
                    );
                } else {
                    // trash → edit → keyboard → shortcut (rightmost 부터).
                    if IconButton::new()
                        .variant(IconButtonVariant::Ghost)
                        .size(ControlSize::Sm)
                        .show(ui, th, &|ui, rect, c| {
                            icons::TRASH.image(rect.width(), c).paint_at(ui, rect);
                        })
                        .clicked()
                    {
                        st.confirm_id = Some(id.to_string());
                        st.rename_id = None;
                    }
                    if IconButton::new()
                        .variant(IconButtonVariant::Ghost)
                        .size(ControlSize::Sm)
                        .show(ui, th, &|ui, rect, c| {
                            icons::EDIT.image(rect.width(), c).paint_at(ui, rect);
                        })
                        .clicked()
                    {
                        st.rename_id = Some(id.to_string());
                        st.rename_draft = name.clone();
                        st.confirm_id = None;
                    }
                    if IconButton::new()
                        .variant(IconButtonVariant::Ghost)
                        .size(ControlSize::Sm)
                        .show(ui, th, &|ui, rect, c| {
                            icons::KEYBOARD.image(rect.width(), c).paint_at(ui, rect);
                        })
                        .clicked()
                    {
                        *goto_keybindings = true;
                    }
                    if combo.is_empty() {
                        ui.label(
                            egui::RichText::new(t("settings.scripts.unbound"))
                                .size(th.font_size_term_sm.value())
                                .italics()
                                .color(th.text_muted()),
                        );
                    } else {
                        // 이 클러스터는 RTL 이라 kbd 키캡이 역순으로 그려진다 → 파트를
                        // 미리 뒤집어 화면상 정순(Ctrl+Shift+J)이 되게 한다.
                        kbd(
                            ui,
                            th,
                            &rtl_combo(&KeybindingSettings::format_display(
                                &combo,
                                &settings.general,
                            )),
                            tasty_ui_widgets::Ground::Panel,
                        );
                    }
                }

                // 남은 좌측 폭 = 중앙 컬럼(name/path/help).
                ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                    ui.spacing_mut().item_spacing.y = STRUCT_GAP_2.value();
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = th.spacing_sm.value();
                        ui.label(
                            egui::RichText::new(&name)
                                .size(th.font_size_body.value())
                                .strong()
                                .color(th.text_primary()),
                        );
                        if changed {
                            draw_changed_badge(ui, th);
                        }
                    });
                    draw_script_path(ui, th, &path_display);
                    if changed {
                        ui.label(
                            egui::RichText::new(t("settings.scripts.changed_help"))
                                .size(th.font_size_caption.value())
                                .color(th.accent_warning_raised()),
                        );
                    }
                    draw_trigger_row(ui, th, id, &triggers, pending);
                });
            });
        }
    });

    // 행 하단 1px separator.
    let w = ui.available_width();
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(w, th.border_width.value()), egui::Sense::hover());
    ui.painter().hline(
        rect.x_range(),
        rect.center().y,
        egui::Stroke::new(
            th.border_width.value(),
            th.separator.to_egui_premultiplied(),
        ),
    );
}

/// row4 — 자동실행 트리거: 등록 chip(클릭=제거) + 점선 Add trigger… 컨트롤(남은 이벤트 메뉴).
/// 이벤트명은 기술 식별자라 번역하지 않는다(mono 표기, i18n 하드코딩 허용 예외).
fn draw_trigger_row(
    ui: &mut egui::Ui,
    th: &tasty_type_appearance::theme::Theme,
    id: &str,
    triggers: &[AutoTrigger],
    pending: &mut Option<Pending>,
) {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = th.spacing_xs.value();
        ui.label(
            egui::RichText::new(t("settings.scripts.autorun_label"))
                .size(th.font_size_caption.value())
                .color(th.text_muted()),
        );
        for trig in triggers {
            let AutoTrigger::Event { name } = trig;
            if trigger_chip(ui, th, name)
                .on_hover_text(t("settings.scripts.trigger_remove"))
                .clicked()
            {
                *pending = Some(Pending::RemoveTrigger(id.to_string(), trig.clone()));
            }
        }
        // 아직 이 스크립트에 안 걸린 이벤트만 추가 후보로 노출 (화이트리스트 검증 겸용).
        let available: Vec<&str> = AUTO_TRIGGER_EVENTS
            .iter()
            .copied()
            .filter(|ev| {
                !triggers
                    .iter()
                    .any(|AutoTrigger::Event { name }| name == ev)
            })
            .collect();
        // 모두 걸려 있으면 컨트롤을 숨기지 않고 disabled로 둔다.
        let enabled = !available.is_empty();
        let popup_id = ui.make_persistent_id(("script_trigger_add", id));
        let open = ui.memory(|m| m.is_popup_open(popup_id));
        let resp = script_trigger_add_control(
            ui,
            th,
            t("settings.scripts.trigger_add"),
            BADGE_HEIGHT.value(),
            enabled,
            open,
        )
        .on_hover_text(if enabled {
            t("settings.scripts.trigger_add_tooltip")
        } else {
            t("settings.scripts.trigger_all_bound")
        });
        if resp.clicked() {
            ui.memory_mut(|m| m.toggle_popup(popup_id));
        }
        tasty_egui_theme::with_popover_frame(ui, th, |ui| {
            egui::popup_below_widget(
                ui,
                popup_id,
                &resp,
                egui::PopupCloseBehavior::CloseOnClick,
                |ui| {
                    if let Some(ev) = script_trigger_menu(ui, th, &available) {
                        *pending = Some(Pending::AddTrigger(
                            id.to_string(),
                            AutoTrigger::Event {
                                name: ev.to_string(),
                            },
                        ));
                    }
                },
            );
        });
    });
}

/// 자동실행 트리거 chip — 이벤트명(mono micro) + CLOSE 글리프 12. 클릭 = 제거.
/// changed 배지와 동일 지오메트리(높이 16, radius-sm), 톤만 중립(border-default).
fn trigger_chip(
    ui: &mut egui::Ui,
    th: &tasty_type_appearance::theme::Theme,
    event: &str,
) -> egui::Response {
    let fg = th.text_secondary().to_egui();
    let micro = th.font_size_micro.value();
    let glyph = th.icon_glyph_size_xs.value(); // 12
    let gap = th.spacing_xs.value(); // 4 (라벨↔글리프)
    let pad_x = th.spacing_xs.value(); // 4 (좌우, 시안 `padding: 0 space-xs`)
    let galley = ui.painter().layout_no_wrap(
        event.to_owned(),
        egui::FontId::monospace(micro),
        egui::Color32::PLACEHOLDER,
    );
    let w = pad_x * 2.0 + galley.rect.width() + gap + glyph;
    let (rect, resp) =
        ui.allocate_exact_size(egui::vec2(w, BADGE_HEIGHT.value()), egui::Sense::click());
    let radius = th.corner_radius_sm.value();
    // 호버 오버레이 — 전경색 저알파 mix (위젯 공통 규칙과 동일 도출).
    if resp.hovered() {
        // overlay_hover는 합성된 색이므로 여기의 혼합 비율 대신 쓸 수 없다.
        const HOVER_OVERLAY_OPACITY: f32 = 0.12;
        ui.painter()
            .rect_filled(rect, radius, fg.gamma_multiply(HOVER_OVERLAY_OPACITY));
    }
    ui.painter().rect_stroke(
        rect,
        radius,
        egui::Stroke::new(th.border_width.value(), th.border_default().to_egui()),
        egui::StrokeKind::Inside,
    );
    let pos = egui::pos2(
        rect.left() + pad_x,
        rect.center().y - galley.rect.height() * 0.5,
    );
    ui.painter().galley(pos, galley, fg);
    let gy = egui::Rect::from_min_size(
        egui::pos2(rect.right() - pad_x - glyph, rect.center().y - glyph * 0.5),
        egui::vec2(glyph, glyph),
    );
    icons::CLOSE
        .image(glyph, th.text_muted().to_egui())
        .paint_at(ui, gy);
    resp
}

/// 경로 중간생략 — 디렉토리 tail 이 먼저 ellipsis 로 잘리고 파일명은 항상 완전 표시.
/// dir=`text-muted` / file=`text-secondary`, mono `font-size-caption`(11).
fn draw_script_path(ui: &mut egui::Ui, th: &tasty_type_appearance::theme::Theme, path: &str) {
    let sep = path.rfind(['/', '\\']);
    let (dir, file) = match sep {
        Some(i) => (&path[..=i], &path[i + 1..]),
        None => ("", path),
    };
    let size = th.font_size_caption.value();
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        // 파일명 폭을 먼저 확보하고, 디렉토리는 남는 폭에서 ellipsis 로 잘린다.
        let file_galley = ui.painter().layout_no_wrap(
            file.to_owned(),
            egui::FontId::monospace(size),
            egui::Color32::PLACEHOLDER,
        );
        let file_w = file_galley.rect.width();
        let dir_w = (ui.available_width() - file_w).max(0.0);
        if !dir.is_empty() {
            ui.allocate_ui_with_layout(
                egui::vec2(dir_w, file_galley.rect.height()),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(dir)
                                .size(size)
                                .monospace()
                                .color(th.text_muted()),
                        )
                        .truncate(),
                    );
                },
            );
        }
        ui.label(
            egui::RichText::new(file)
                .size(size)
                .monospace()
                .color(th.text_secondary()),
        );
    });
}

/// changed 배지 — warn 글리프 12 + "changed", font-label micro(10), accent-warning
/// color-mix(40% border / 12% bg).
fn draw_changed_badge(ui: &mut egui::Ui, th: &tasty_type_appearance::theme::Theme) {
    let warn = th.accent_warning().to_egui();
    let micro = th.font_size_micro.value();
    let glyph = th.icon_glyph_size_xs.value(); // 12
    let gap = th.spacing_xs.value(); // 4 (글리프↔라벨)
    let pad_x = th.spacing_sm.value(); // 8 (배지 좌우)
    let galley = ui.painter().layout_no_wrap(
        t("settings.scripts.changed_badge").to_string(),
        tasty_ui_widgets::label_font(micro),
        egui::Color32::PLACEHOLDER,
    );
    let w = pad_x * 2.0 + glyph + gap + galley.rect.width();
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(w, BADGE_HEIGHT.value()), egui::Sense::hover());
    let radius = th.corner_radius_sm.value();
    // 채움은 `tint-fill-alpha`. 테두리 계수는 디자인이 "채움만" 으로 한정한
    // 부분 사용이라(docs/design/systems/theme.md) 이 자리 고유 값으로 남는다.
    const WARN_BADGE_STROKE_OPACITY: f32 = 0.4;
    ui.painter()
        .rect_filled(rect, radius, warn.gamma_multiply(th.tint_fill_alpha()));
    ui.painter().rect_stroke(
        rect,
        radius,
        egui::Stroke::new(
            th.border_width.value(),
            warn.gamma_multiply(WARN_BADGE_STROKE_OPACITY),
        ),
        egui::StrokeKind::Inside,
    );
    let gy = egui::Rect::from_min_size(
        egui::pos2(rect.left() + pad_x, rect.center().y - glyph * 0.5),
        egui::vec2(glyph, glyph),
    );
    let ink = th.accent_warning_raised().to_egui();
    icons::ALERT_TRIANGLE.image(glyph, ink).paint_at(ui, gy);
    let pos = egui::pos2(
        rect.left() + pad_x + glyph + gap,
        rect.center().y - galley.rect.height() * 0.5,
    );
    ui.painter().galley(pos, galley, ink);
}

/// Add card — 인라인 카드(surface-raised + border-default + radius). File(경로 +
/// Browse…) / Display name / Cancel·Add script.
fn draw_add_card(
    ui: &mut egui::Ui,
    th: &tasty_type_appearance::theme::Theme,
    settings: &mut Settings,
    st: &mut ScriptsUiState,
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
            // "New script" caps 라벨.
            ui.label(
                egui::RichText::new(t("settings.scripts.new_card").to_uppercase())
                    .size(th.font_size_micro.value())
                    .family(tasty_ui_widgets::label_font_family())
                    .color(th.text_muted_raised()),
            );
            // File 행 — 라벨(100) + 경로 Input + Browse….
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = th.spacing_lg.value();
                add_label(ui, th, t("settings.scripts.file"));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if Button::new(t("settings.scripts.browse"))
                        .variant(ButtonVariant::Secondary)
                        .size(ControlSize::Sm)
                        .leading_icon(&|ui, rect, c| {
                            icons::FOLDER.image(rect.width(), c).paint_at(ui, rect);
                        })
                        .show(ui, th)
                        .clicked()
                    {
                        // 포털에 의존하지 않는 설정 창의 파일 선택을 사용한다(docs/features/file-handler/index.md).
                        st.browse_requested = true;
                    }
                    Input::new()
                        .mono(true)
                        .placeholder(t("settings.scripts.file_placeholder"))
                        .show(ui, th, &mut st.draft_path);
                });
            });
            // Display name 행 — 라벨(100) + Input.
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = th.spacing_lg.value();
                add_label(ui, th, t("settings.scripts.display_name"));
                Input::new()
                    .placeholder(t("settings.scripts.display_name_placeholder"))
                    .show(ui, th, &mut st.draft_name);
            });
            // 푸터 — Cancel / Add script (우측).
            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = th.spacing_sm.value();
                    let can_add = !st.draft_path.trim().is_empty();
                    if Button::new(t("settings.scripts.add"))
                        .variant(ButtonVariant::Primary)
                        .size(ControlSize::Sm)
                        .enabled(can_add)
                        .show(ui, th)
                        .clicked()
                        && can_add
                    {
                        commit_add(settings, st);
                    }
                    if Button::new(t("button.cancel"))
                        .variant(ButtonVariant::Ghost)
                        .size(ControlSize::Sm)
                        .show(ui, th)
                        .clicked()
                    {
                        st.adding = false;
                    }
                });
            });
        });
}

/// Add card 라벨 셀 (폭 100, 우측정렬 아님 — 좌측정렬 13/text-secondary).
fn add_label(ui: &mut egui::Ui, th: &tasty_type_appearance::theme::Theme, text: &str) {
    ui.allocate_ui_with_layout(
        egui::vec2(ADD_LABEL_W.value(), th.item_height_interactive.value()),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.label(
                egui::RichText::new(text)
                    .size(th.font_size_body.value())
                    .color(th.text_secondary()),
            );
        },
    );
}

/// Add 확정 — 경로의 `~` 를 확장하고 해시를 계산해 registry 에 등록.
fn commit_add(settings: &mut Settings, st: &mut ScriptsUiState) {
    let path = expand_home(st.draft_path.trim());
    let name = {
        let n = st.draft_name.trim();
        if n.is_empty() {
            path.file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| st.draft_path.trim().to_string())
        } else {
            n.to_string()
        }
    };
    // 해시 계산(읽기 실패 시 빈 해시 — 다음 열람에서 changed 로 안 뜨도록 unwrap_or).
    let sha = hash_file(&path).unwrap_or_default();
    settings.scripts.add(name, path, sha);
    st.adding = false;
    st.draft_path.clear();
    st.draft_name.clear();
    st.changed = None;
}

/// 빈 상태 — 목록 자리에 공용 CenterState 를 자연 높이로 그린다.
fn draw_empty(ui: &mut egui::Ui, th: &tasty_type_appearance::theme::Theme) {
    CenterState::empty(icons::SCRIPT, t("settings.scripts.empty_title"))
        .sub_line(Some(t("settings.scripts.empty_body")))
        .show(ui, th, None);
}

/// 절대 경로의 홈 디렉토리 접두를 `~` 로 축약(표시용).
fn abbreviate_home(path: &str) -> String {
    if let Some(base) = directories::BaseDirs::new() {
        let home = base.home_dir().to_string_lossy().into_owned();
        if !home.is_empty()
            && let Some(rest) = path.strip_prefix(&home)
        {
            return format!("~{rest}");
        }
    }
    path.to_string()
}

/// 입력 경로의 선행 `~` 를 홈 디렉토리로 확장(저장용).
fn expand_home(path: &str) -> std::path::PathBuf {
    if let Some(rest) = path.strip_prefix("~/").or_else(|| path.strip_prefix("~\\"))
        && let Some(base) = directories::BaseDirs::new()
    {
        return base.home_dir().join(rest);
    }
    std::path::PathBuf::from(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_theme() -> tasty_type_appearance::theme::Theme {
        tasty_themes::mocha_fallback()
    }

    fn seeded_settings(count: usize) -> Settings {
        let mut settings = Settings::default();
        for i in 0..count {
            settings.scripts.add(
                format!("script-{i}"),
                std::path::PathBuf::from(format!("/nonexistent/script-{i}.lua")),
                String::new(),
            );
        }
        settings
    }

    /// 스크립트 수와 관계없이 액션 영역이 패널 높이 전체로 늘어나지 않는지 확인한다.
    #[test]
    fn script_row_height_does_not_expand_to_panel_remainder() {
        let th = test_theme();
        for count in 1..=3usize {
            let mut settings = seeded_settings(count);
            let mut st = ScriptsUiState::default();
            let ids: Vec<String> = settings.scripts.iter().map(|e| e.id.clone()).collect();

            let ctx = egui::Context::default();
            let raw = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1147.0, 750.0),
                )),
                ..Default::default()
            };
            let mut heights = Vec::new();
            drop(ctx.run(raw, |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let mut goto_keybindings = false;
                    let mut pending = None;
                    for id in &ids {
                        let resp = ui.scope(|ui| {
                            draw_script_row(
                                ui,
                                &th,
                                &mut settings,
                                &mut st,
                                id,
                                &mut goto_keybindings,
                                &mut pending,
                            );
                        });
                        heights.push(resp.response.rect.height());
                    }
                });
            }));

            for (i, h) in heights.iter().enumerate() {
                assert!(
                    *h < 100.0,
                    "registered {count}: row {i} height {h} — RTL 클러스터가 패널 잔여 높이로 확장됨"
                );
            }
        }
    }
}
