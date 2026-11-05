//! 플러그인 관리 화면: 설치 목록, 확인 필요 항목, 새 플러그인 추가.
//! PluginsSnapshot을 표시하고 조작은 PluginsAction 큐로 보낸다.

use crate::adapters::ui::icons;
use crate::i18n::t;
use crate::theme;
use tasty_type_geometry::length::LogicalPx;

/// 헤더 아이콘의 기존 크기. Theme 토큰과 별도로 지정하며 ui_scale을 적용하지 않는다.
/// 값 변경은 디자인 검토가 필요하다(docs/design/systems/theme.md).
const PLUG_HEADER_GLYPH: LogicalPx = LogicalPx(17.0);

/// semantic 역할을 지정하지 않은 탭 라벨의 primitive 폰트 크기(docs/design/systems/theme.md).
const SEGMENT_TAB_LABEL_PRIMITIVE_12: LogicalPx = LogicalPx(12.0);

/// 상세 패널에 표시할 plugin command 한 줄.
#[derive(Debug, Clone)]
pub struct PluginCommandEntry {
    /// 표시 라벨 i18n 키 (plugin lang_dir 의 `title_i18n_key`). `t()` 로 해석.
    pub title_key: String,
    /// 효과 단축키 (override 우선, 없으면 매니페스트 default). `None` 이면 미할당.
    pub keybinding: Option<String>,
}

/// 한 plugin의 화면 표시용 스냅샷.
#[derive(Debug, Clone)]
pub struct PluginEntry {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub authors: Vec<String>,
    pub homepage: String,
    pub enabled: bool,
    pub running: bool,
    /// spawn 반복 실패로 자동 비활성화된 error 상태인지. 디자인의 error dot
    /// (목록) + 경고 박스(상세) 표시 기준.
    pub health_error: bool,
    pub builtin: bool,
    pub surface_kinds: Vec<String>,
    pub manifest_permissions: Vec<String>,
    /// plugin 이 contribute 한 command 목록 (`[[contributes.commands]]`).
    pub commands: Vec<PluginCommandEntry>,
    pub log_path: String,
    /// 플러그인 설치 디렉터리.
    pub install_dir: String,
}

#[derive(Debug, Clone, Default)]
pub struct PluginsSnapshot {
    pub plugins: Vec<PluginEntry>,
    /// 등록 거부(서명/신뢰) 또는 실행 실패(health error) 로 "확인 필요" 한
    /// plugin 들. `Attention` 탭 목록 + 사이드바 경고 배지가 소비.
    pub attention: Vec<AttentionEntry>,
    /// Commands 절 키캡의 플랫폼과 수식키 표시 스타일(설정 › 일반).
    pub keycap_style: tasty_ui_widgets::PluginKeycapStyle,
}

/// "확인 필요" 사유 — `host-plugin` 의 `RejectionReason` + health error 를 합친 것.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttentionKind {
    /// 신뢰 목록에 없는 키로 서명 → 등록 거부.
    UnknownKey,
    /// 서명 누락/손상/검증 실패 → 등록 거부.
    SignatureInvalid,
    /// 매니페스트 권한 변경 → 재승인 필요.
    PermissionsChanged,
    /// enable 됐으나 런타임 반복 실패로 자동 비활성화.
    HealthError,
}

impl AttentionKind {
    /// danger(등록 거부) 면 true, warning(재검토) 면 false.
    pub fn is_danger(self) -> bool {
        matches!(self, Self::UnknownKey | Self::SignatureInvalid)
    }

    /// 매니페스트의 설명을 보여도 되는 사유인가. 서명이 깨진 번들은 매니페스트가
    /// 서명한 내용 그대로라는 보장이 없어 그 글을 보이지 않는다. 신뢰하지 않은 키의
    /// 서명은 내용이 그 키로 서명된 그대로이며, 추가 탭의 매니페스트 카드도 같은 경우에 보인다.
    pub fn shows_manifest_text(self) -> bool {
        !matches!(self, Self::SignatureInvalid)
    }
}

/// "확인 필요" 목록의 한 항목.
#[derive(Debug, Clone)]
pub struct AttentionEntry {
    pub id: String,
    pub name: String,
    pub version: String,
    pub authors: Vec<String>,
    /// 매니페스트 설명. [`AttentionKind::shows_manifest_text`] 가 false 인 사유면 비운다.
    pub description: String,
    /// 매니페스트 homepage. 설명처럼 [`AttentionKind::shows_manifest_text`] 가 false 면 비운다.
    pub homepage: String,
    pub builtin: bool,
    pub kind: AttentionKind,
    /// 서명 키 지문 (UnknownKey/PermissionsChanged). 표시용.
    pub fingerprint: Option<String>,
    /// PermissionsChanged 일 때 새로 요구된 / 더 이상 안 쓰는 권한.
    pub permissions_added: Vec<String>,
    pub permissions_removed: Vec<String>,
    /// HealthError 일 때 표시할 사유 (로그 경로 등). 없으면 일반 문구.
    pub health_detail: Option<String>,
    /// SignatureInvalid 일 때 서명 검증이 실패한 원인. 표시용.
    pub cause: Option<String>,
    /// 서명 사유일 때 미인증 원인 이름(`unknown-key` · `missing-pubkey` · `unsigned` ·
    /// `signature-error`). 신뢰 상자의 원인 문장을 고른다.
    pub unverified: Option<&'static str>,
    /// 탐색 때 읽은 매니페스트의 SHA-256. "그래도 등록"이 이 값을 실어 본 내용만 등록하게 한다.
    pub manifest_sha256: String,
}

/// `PluginsView`가 메인 루프에 발행하는 동작.
#[derive(Debug, Clone)]
pub enum PluginsAction {
    SetEnabled {
        id: String,
        enabled: bool,
    },
    Uninstall {
        id: String,
    },
    /// 이 모달을 닫고 설정의 Plugins 탭을 연다.
    OpenSettings,
    /// "확인 필요" 탭의 `Re-approve` — 권한 변경으로 거부된 plugin 을 현재
    /// 매니페스트 권한으로 재신뢰한다. 호스트가 known-plugins.toml 의 권한 스냅샷을
    /// 갱신 + 재grant + discover 재호출.
    Reapprove {
        id: String,
    },
    /// 헤더 X 버튼 — 모달을 닫는다.
    Close,
    /// 설치 디렉터리를 OS 파일 매니저로 연다.
    OpenInstallDir {
        path: String,
    },
    /// 매니페스트의 homepage 를 기본 브라우저로 연다. 실패는 이 창의 토스트로 알린다.
    OpenHomepage {
        url: String,
    },
    /// 외부 디렉터리를 데이터 루트의 plugins/<id>로 복사한다. 키로 확인된 플러그인만 설치한다.
    Install {
        src_path: String,
        /// 화면이 보인 매니페스트의 SHA-256. 설치 시점의 내용이 다르면 설치하지 않는다.
        manifest_sha256: String,
    },
    /// 미인증 경고를 보고 "그래도 추가"를 골랐다. 선택을 기록하고 설치한다.
    InstallUnverified {
        src_path: String,
        manifest_sha256: String,
    },
    /// 확인 필요 탭의 "그래도 등록". 플러그인 폴더에 있는 미인증 플러그인을 등록한다.
    RegisterUnverified {
        id: String,
        /// 확인 필요 탭이 보인 매니페스트의 SHA-256.
        manifest_sha256: String,
    },
    /// 공개 키 파일의 키를 신뢰하고 설치한다. 호스트가 그 키로 서명이 검증되는지 다시 확인하고
    /// 현재 매니페스트의 권한과 함께 신뢰 DB에 기록한다.
    TrustAndInstall {
        src_path: String,
        /// base64 (32-byte ed25519) — 화면이 보인 공개 키.
        pubkey_b64: String,
        manifest_sha256: String,
    },
}

/// 현재 활성 탭.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PluginsTab {
    #[default]
    List,
    Attention,
    Add,
}

/// `Add` 탭에서 사용자가 경로를 검증한 결과 — 추가/취소 확인 단계로 진입.
#[derive(Debug, Clone)]
pub struct AddPreview {
    pub src_path: String,
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub authors: Vec<String>,
    pub homepage: String,
    pub surface_kinds: Vec<String>,
    pub permissions: Vec<String>,
    /// 같은 id의 플러그인이 이미 설치됐다. 추가 버튼을 끄고 액션 바에 이유를 적는다.
    pub already_installed: bool,
    /// 매니페스트 sig 검증으로 결정된 trust 상태. 신뢰 상자의 종류를 정한다.
    pub trust_state: AddTrustState,
    /// Verify 때 읽은 매니페스트의 SHA-256. 추가 동작이 이 값을 실어 본 내용만 설치하게 한다.
    pub manifest_sha256: String,
}

/// 매니페스트 신뢰 판정 — 신뢰 상자와 추가 버튼을 정한다. 사용자 선택 기록은 보지 않으므로
/// 미인증 플러그인은 매번 경고를 보인다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AddTrustState {
    /// 임베드 키 또는 known-plugins.toml 통과. 바로 install 가능.
    Trusted,
    /// 신뢰한 키지만 권한이 늘었다. 그 키를 새 권한과 함께 다시 신뢰하며 추가한다.
    PermissionsChanged {
        fingerprint: String,
        /// 신뢰 DB에 있는 키(base64) — TrustAndInstall action 으로 전달.
        pubkey_b64: String,
    },
    /// 미인증 — 신뢰 목록에 없는 키. 공개 키 파일의 키로 서명이 검증되며, 추가하면 그 키를 신뢰한다.
    UnknownKey {
        /// 공개 키의 SHA-256 지문.
        fingerprint: String,
        /// base64 (32-byte) — TrustAndInstall action 으로 전달.
        pubkey_b64: String,
    },
    /// 미인증 — 신뢰 목록에 없는 키이고 공개 키 파일이 없거나 읽을 수 없다.
    MissingPubkey { fingerprint: String },
    /// 미인증 — 서명 파일이 없다.
    Unsigned,
    /// 미인증 — 서명을 검증하지 못했다(공개 키 불일치 포함). 원인은 검증 시점에 로그로 남긴다.
    SignatureError,
}

impl AddTrustState {
    /// 경고를 보고 명시적으로 골라야 추가하는 미인증 판정인지.
    pub fn is_unverified(&self) -> bool {
        !matches!(
            self,
            AddTrustState::Trusted | AddTrustState::PermissionsChanged { .. }
        )
    }
}

/// 모달 자체 상태 (탭, 선택, 검색 입력 등).
#[derive(Debug, Default)]
pub struct PluginsUiState {
    pub(crate) folder_chooser: crate::view::file_chooser::WindowFileChooser,
    pub active_tab: PluginsTab,
    pub selected_id: Option<String>,
    /// `Attention` 탭의 선택 항목 — `selected_id`(Installed) 와 독립.
    pub attention_selected_id: Option<String>,
    /// 제거 확인 중인 plugin id. 선택이 바뀌거나 Esc·Cancel 이면 지운다.
    pub confirm_uninstall_id: Option<String>,
    /// 제거 확인이 막 열렸다. 다음 프레임에 Cancel 로 포커스를 옮기고 지운다.
    pub confirm_focus_pending: bool,
    /// `Installed` 탭 목록 검색/필터 입력 버퍼 (name/authors/description 부분일치).
    pub filter: String,
    /// `Add` 탭의 경로 입력 버퍼.
    pub add_path_input: String,
    /// 검증 후 preview 정보. 있으면 추가/취소 화면을 보여준다.
    pub add_preview: Option<AddPreview>,
    /// 매니페스트를 읽지 못했거나 검사에 실패한 이유. 있으면 안내 상자 자리에 오류 상자를 둔다.
    pub add_error: Option<AddError>,
    /// 추가 요청을 보냈고 App의 결과를 기다린다. 그동안 두 버튼을 끈다.
    pub add_in_flight: bool,
    /// 미인증 판정을 막 보였다. 다음 프레임에 Cancel 로 포커스를 옮기고 지운다.
    pub add_focus_pending: bool,
}

impl PluginsUiState {
    /// Add 탭의 추가가 끝났다. 성공하면 입력 화면으로 되돌리고, 실패하면 판정을 남긴다.
    pub fn finish_add(&mut self, installed: bool) {
        self.add_in_flight = false;
        if installed {
            self.add_preview = None;
            self.add_error = None;
            self.add_path_input.clear();
        }
    }

    /// 제거 확인을 닫는다. 아직 옮기지 않은 포커스 요청도 버린다.
    pub fn cancel_uninstall_confirm(&mut self) {
        self.confirm_uninstall_id = None;
        self.confirm_focus_pending = false;
    }
}

/// `Add` 탭 오류 상자의 종류. 제목만 다르고 둘째 줄은 원문 메시지를 번역하지 않고 보인다.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AddError {
    /// 파일이 없거나 TOML로 읽을 수 없다.
    Read(String),
    /// 읽었지만 선언 검사(바이너리 경로·감지기 등)에 실패했다.
    Invalid(String),
}

impl AddError {
    /// 상자 제목의 번역 키.
    pub fn title_key(&self) -> &'static str {
        match self {
            AddError::Read(_) => "plugins.add_read_error",
            AddError::Invalid(_) => "plugins.add_invalid",
        }
    }

    /// 상자 둘째 줄에 보일 원문.
    pub fn reason(&self) -> &str {
        match self {
            AddError::Read(reason) | AddError::Invalid(reason) => reason,
        }
    }
}

/// modal 메인 그리기. snapshot은 읽기 전용, action은 큐에 추가.
pub fn draw_plugins_panel(
    ctx: &egui::Context,
    snapshot: &PluginsSnapshot,
    ui_state: &mut PluginsUiState,
    actions: &mut Vec<PluginsAction>,
) {
    let th = theme::theme();

    // 제거 확인이 열린 동안에는 탭과 관계없이 창 어디서든 Esc 가 확인을 닫는다. 다른 탭에 가려진
    // 확인도 닫혀, Installed 로 돌아와도 다시 보이지 않는다. 확인이 없으면 Esc 를 소비하지 않는다.
    if !ui_state.folder_chooser.is_open()
        && ui_state.confirm_uninstall_id.is_some()
        && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
    {
        ui_state.cancel_uninstall_confirm();
    }

    egui::TopBottomPanel::top("plugins_header")
        .exact_height(th.plugins_header_height().value())
        .show(ctx, |ui| {
            ui.horizontal_centered(|ui| {
                hspace(ui, th.spacing_md);
                // 디자인 헤더: plug 아이콘 + 타이틀.
                // 헤더 장식 accent — `plugins-header-glyph` → `accent-decorative`(peach).
                ui.add(icons::PLUG.image(
                    PLUG_HEADER_GLYPH.value(),
                    egui::Color32::from(th.plugins_header_glyph()),
                ));
                hspace(ui, th.spacing_xs);
                ui.label(
                    egui::RichText::new(t("plugins.title"))
                        .size(th.font_size_max.value())
                        .strong()
                        .color(egui::Color32::from(th.text_primary())),
                );
                hspace(ui, th.spacing_sm);
                draw_header_divider(ui, &th);
                hspace(ui, th.spacing_sm);

                // 세그먼트 탭 (Installed N / Attention N / Add plugin).
                let installed_count = snapshot.plugins.len();
                if segment_tab(
                    ui,
                    &th,
                    t("plugins.tab_list"),
                    Some(installed_count),
                    false,
                    ui_state.active_tab == PluginsTab::List,
                ) {
                    ui_state.active_tab = PluginsTab::List;
                }
                hspace(ui, STRUCT_GAP_2);
                // Attention 탭 — 확인 필요 plugin 개수를 danger 배지로 노출.
                let attention_count = snapshot.attention.len();
                if segment_tab(
                    ui,
                    &th,
                    t("plugins.tab_attention"),
                    Some(attention_count),
                    true,
                    ui_state.active_tab == PluginsTab::Attention,
                ) {
                    ui_state.active_tab = PluginsTab::Attention;
                }
                hspace(ui, STRUCT_GAP_2);
                if segment_tab(
                    ui,
                    &th,
                    t("plugins.tab_add"),
                    None,
                    false,
                    ui_state.active_tab == PluginsTab::Add,
                ) {
                    ui_state.active_tab = PluginsTab::Add;
                }

                // 우측 클러스터 (오른쪽→왼쪽): X 닫기 → 검색 입력.
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    hspace(ui, th.spacing_sm);
                    if close_icon_button(ui, &th) {
                        actions.push(PluginsAction::Close);
                    }
                    // 검색/필터는 목록이 있는 Installed 탭에서만 동작 (Add 탭은
                    // 로컬 경로 설치 폼이라 필터 대상 목록이 없다).
                    if ui_state.active_tab == PluginsTab::List {
                        hspace(ui, th.spacing_sm);
                        let edit = egui::TextEdit::singleline(&mut ui_state.filter)
                            .hint_text(tasty_egui_theme::hint_text(
                                &th,
                                t("plugins.filter_placeholder"),
                            ))
                            .desired_width(th.field_width_lg.value());
                        ui.add(edit);
                    }
                });
            });
        });

    match ui_state.active_tab {
        PluginsTab::List => draw_list_tab(ctx, snapshot, ui_state, actions),
        PluginsTab::Attention => draw_attention_tab(ctx, snapshot, ui_state, actions),
        PluginsTab::Add => draw_add_tab(ctx, snapshot, ui_state, actions),
    }
    ui_state.folder_chooser.draw(ctx);
    add::apply_folder_result(ui_state, snapshot);
}

/// 헤더의 타이틀 ↔ 세그먼트 사이 세로 구분선 (1px × 20px).
fn draw_header_divider(ui: &mut egui::Ui, th: &theme::Theme) {
    let w = th.border_width.value();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(w, 20.0), egui::Sense::hover());
    ui.painter()
        .rect_filled(rect, 0.0, th.separator.to_egui_premultiplied());
}

/// 헤더 우측 X 닫기 버튼 (IconButton). 클릭 시 true.
fn close_icon_button(ui: &mut egui::Ui, th: &theme::Theme) -> bool {
    let size = 28.0;
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::click());
    if resp.is_pointer_button_down_on() {
        ui.painter().rect_filled(
            rect,
            th.corner_radius.value(),
            th.overlay_active().to_egui_premultiplied(),
        );
    } else if resp.hovered() {
        ui.painter().rect_filled(
            rect,
            th.corner_radius.value(),
            th.overlay_hover().to_egui_premultiplied(),
        );
    }
    let color = if resp.hovered() {
        egui::Color32::from(th.text_primary())
    } else {
        egui::Color32::from(th.text_muted())
    };
    let glyph = 16.0;
    let icon_rect = egui::Rect::from_center_size(rect.center(), egui::vec2(glyph, glyph));
    icons::CLOSE.image(glyph, color).paint_at(ui, icon_rect);
    resp.clicked()
}

/// 디자인 pill 세그먼트 탭 한 개. active 면 surface-raised 배경 + inset border,
/// hover 면 overlay. `count` 가 있으면 라벨 우측에 카운트를 덧붙인다.
/// `danger` 면 카운트를 mono 텍스트 대신 accent-danger 필 배지로 그리고,
/// `count == 0` 일 땐 배지를 숨긴다 (확인 필요 0건이면 깔끔하게).
fn segment_tab(
    ui: &mut egui::Ui,
    th: &theme::Theme,
    label: &str,
    count: Option<usize>,
    danger: bool,
    selected: bool,
) -> bool {
    let label_color = if selected {
        egui::Color32::from(th.text_primary())
    } else {
        egui::Color32::from(th.text_muted())
    };
    let count_color = if selected {
        egui::Color32::from(th.text_secondary_raised())
    } else {
        egui::Color32::from(th.text_muted())
    };
    let label_galley = ui.painter().layout_no_wrap(
        label.to_string(),
        egui::FontId::proportional(SEGMENT_TAB_LABEL_PRIMITIVE_12.value()),
        label_color,
    );

    // danger 배지로 그릴지 / mono 카운트로 그릴지 결정.
    let badge = danger && count.is_some_and(|c| c > 0);
    let badge_galley = if badge {
        Some(ui.painter().layout_no_wrap(
            count.unwrap().to_string(),
            egui::FontId::proportional(th.badge_font_size().value()),
            egui::Color32::from(th.text_on_accent()),
        ))
    } else {
        None
    };
    let count_galley = if !danger {
        count.map(|c| {
            ui.painter().layout_no_wrap(
                c.to_string(),
                egui::FontId::monospace(th.font_size_micro.value()),
                count_color,
            )
        })
    } else {
        None
    };

    let pad_x = 12.0;
    let gap = 7.0;
    let height = 26.0;
    let badge_h = 15.0;
    let badge_pad = 4.0;
    let mut width = label_galley.size().x + pad_x * 2.0;
    if let Some(g) = &count_galley {
        width += gap + g.size().x;
    }
    if let Some(g) = &badge_galley {
        width += gap + (g.size().x + badge_pad * 2.0).max(badge_h);
    }

    let (rect, resp) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::click());
    let radius = th.corner_radius.value();
    if selected {
        ui.painter().rect(
            rect,
            radius,
            egui::Color32::from(th.surface_raised()),
            egui::Stroke::new(
                th.border_width.value(),
                egui::Color32::from(th.border_default()),
            ),
            egui::StrokeKind::Inside,
        );
    } else if resp.hovered() {
        ui.painter()
            .rect_filled(rect, radius, th.overlay_hover().to_egui_premultiplied());
    }

    let mut x = rect.left() + pad_x;
    let label_y = rect.center().y - label_galley.size().y / 2.0;
    let label_w = label_galley.size().x;
    ui.painter()
        .galley(egui::pos2(x, label_y), label_galley, label_color);
    if let Some(g) = count_galley {
        x += label_w + gap;
        let cy = rect.center().y - g.size().y / 2.0;
        ui.painter().galley(egui::pos2(x, cy), g, count_color);
    }
    if let Some(g) = badge_galley {
        x += label_w + gap;
        let badge_w = (g.size().x + badge_pad * 2.0).max(badge_h);
        let badge_rect = egui::Rect::from_min_size(
            egui::pos2(x, rect.center().y - badge_h / 2.0),
            egui::vec2(badge_w, badge_h),
        );
        ui.painter().rect_filled(
            badge_rect,
            badge_h / 2.0,
            egui::Color32::from(th.accent_danger()),
        );
        let gp = egui::pos2(
            badge_rect.center().x - g.size().x / 2.0,
            badge_rect.center().y - g.size().y / 2.0,
        );
        ui.painter()
            .galley(gp, g, egui::Color32::from(th.text_on_accent()));
    }
    resp.clicked()
}

mod add;
mod attention;
mod list;
#[cfg(test)]
mod tests;
#[cfg(test)]
pub(crate) mod text_probe;

use add::draw_add_tab;
use attention::draw_attention_tab;
use list::draw_list_tab;
use tasty_ui_widgets::hspace;
use tasty_ui_widgets::tokens::STRUCT_GAP_2;
