//! egui-mesh 로 plugin 이 직접 그리는 두 팝업(대용량 확인 · 파일열기) — 인스턴스 상태,
//! 한 frame 그리기, 그 팝업이 부르는 host 호출. 경계는 렌더 채널이다: 본문 문서는 webview
//! 채널(`render.rs` · `main.rs`)이고, 이 모듈은 `[[contributes.popup]]` 의 mesh 채널만 담는다.

use serde_json::{Value, json};
use tasty_plugin_sdk::{HostHandle, PopupSetContextCtx, Translator};
use tasty_type_appearance::theme::Theme;

#[cfg(any(unix, windows))]
use tasty_plugin_sdk::EguiMeshPopup;

use crate::{MarkdownPlugin, baked_icons, navigate, theme_from_wire};

/// 대용량 확인 팝업 인스턴스의 대상 정보(open_popup context 로 받아 보관). [열기] 시
/// 이 surface 의 문서 read 를 재개한다.
#[derive(Clone)]
pub(crate) struct LargeFileConfirm {
    pub(crate) surface_id: u32,
    /// 표시용 파일명(basename). 경로 전체 대신 파일명만 팝업에 노출.
    pub(crate) file_name: String,
    /// 크기 칩 라벨 (예: "3.2 MB").
    pub(crate) size_label: String,
}

/// 파일열기 팝업 인스턴스 상태 — 경로 입력 버퍼 + (선택) convert 대상 surface.
/// TextEdit 이 `path_input` 을 mutate 하고 browse(native 다이얼로그) 결과가 채운다.
/// `convert_surface_id` 가 `Some` 이면 확정 시 그 surface 를 제자리 markdown 변환
/// (`markdown.navigate`), `None` 이면 새 탭으로 연다(`file_handler.dispatch`). host 가
/// open context 의 `surface_id` 유무로 이 값을 정한다.
#[derive(Default)]
pub(crate) struct FileOpenState {
    pub(crate) path_input: String,
    pub(crate) convert_surface_id: Option<u32>,
    /// **찾아보기…** 가 여는 host 파일 피커의 출발점 — 이 팝업을 띄운 surface 의 폴더.
    pub(crate) picker_start: PickerStart,
}

/// host 파일 피커를 어디서 출발시킬지. popup context 에서 한 번 읽어 둔다.
#[derive(Debug, Default, Clone, PartialEq)]
pub(crate) struct PickerStart {
    /// 시작 디렉토리. mirror 출발이면 원격 경로 문자열(`remote_cwd`), 아니면 로컬 경로
    /// (`observed_cwd`). 둘 다 `inherit_cwd` 설정과 무관한 키다 — 게이트가 걸린 `cwd` 는
    /// "새 surface 가 상속하는가" 의 값이라 피커의 출발점과 다른 물음이다.
    pub(crate) dir: Option<String>,
    /// 팝업을 띄운 로컬 surface — host 가 로컬/원격 판정을 이 surface 의 workspace 로 한다.
    pub(crate) origin_surface_id: Option<u32>,
}

impl PickerStart {
    /// popup context 에서 읽는다. 키가 없거나 `null` 이어도(구버전 host · cwd 미상) 깨지지
    /// 않고 `None` 이 된다 — 그때 host 가 출발 surface 에서 직접 판정하거나 홈에서 연다.
    pub(crate) fn from_context(context: &Value) -> Self {
        let is_mirror = context
            .get("mirror")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let key = if is_mirror {
            "remote_cwd"
        } else {
            "observed_cwd"
        };
        Self {
            dir: context.get(key).and_then(Value::as_str).map(str::to_string),
            origin_surface_id: context
                .get("origin_surface_id")
                .and_then(Value::as_u64)
                .map(|v| v as u32),
        }
    }
}

impl MarkdownPlugin {
    /// large-file 확인 팝업 한 frame 을 egui-mesh 로 그린다. [열기] 시 대상 surface 의
    /// 문서 read 를 재개하고 그 surface 의 webview 를 재생성한 뒤 팝업을 닫는다. [취소]
    /// 는 팝업만 닫는다(surface 는 대기 상태 유지). chrome(scrim/border/Esc/outside-click)
    /// 은 host 소유.
    #[cfg(any(unix, windows))]
    pub(crate) fn paint_confirm(&mut self, ctx: PopupSetContextCtx) {
        let iid = ctx.params.instance_id;
        let Some(theme) = ctx.params.theme.as_ref().map(theme_from_wire) else {
            tracing::debug!("markdown confirm popup {iid}: set_context without theme — skipping");
            return;
        };
        let Some(confirm) = self.confirm.get(&iid).cloned() else {
            // 우리 확인 팝업이 아닌 instance — 무시(있을 수 없음).
            return;
        };

        let mut chosen: Option<ConfirmChoice> = None;
        {
            // popups/popup_fonts_installed/tr 만 빌린다(docs·confirm 과 서로소) — 아래 처리부가
            // self 전체를 다시 빌릴 수 있도록 이 블록에서 borrow 를 끝낸다.
            let popups = &mut self.popups;
            let installed = &mut self.popup_fonts_installed;
            let tr = &self.tr;
            let popup = popups.entry(iid).or_insert_with(|| EguiMeshPopup::new(iid));
            if installed.insert(iid) {
                install_fonts(popup.context());
            }
            let result = popup.paint(&ctx.host, &ctx.params, |egui_ctx| {
                chosen = draw_confirm(egui_ctx, &theme, &confirm, tr);
            });
            if let Err(e) = result {
                tracing::warn!("markdown confirm popup {iid} paint failed: {e}");
            }
        }

        match chosen {
            Some(ConfirmChoice::Open) => {
                if let Some(doc) = self.docs.get_mut(&confirm.surface_id) {
                    doc.resume_load();
                }
                self.reload_webview(confirm.surface_id);
                close_popup(&ctx.host, iid);
            }
            Some(ConfirmChoice::Cancel) => close_popup(&ctx.host, iid),
            None => {}
        }
    }

    /// 파일열기 팝업 한 frame 을 egui-mesh 로 그린다. [browse] 는 host `fs.pick_file`
    /// (native 다이얼로그)로 경로를 채우고, [열기] 는 입력 경로를 host `file_handler.dispatch`
    /// 로 열고 팝업을 닫는다. [취소]/Esc 는 팝업만 닫는다. chrome(scrim/border)은 host 소유.
    #[cfg(any(unix, windows))]
    pub(crate) fn paint_file_open(&mut self, ctx: PopupSetContextCtx) {
        let iid = ctx.params.instance_id;
        let Some(theme) = ctx.params.theme.as_ref().map(theme_from_wire) else {
            tracing::debug!("markdown file-open popup {iid}: set_context without theme — skipping");
            return;
        };

        let mut action = FileOpenAction::None;
        {
            // popups/popup_fonts_installed/tr/file_open 서로소 필드만 빌린다 — 아래 처리부가
            // self 를 다시 빌릴 수 있도록 이 블록에서 borrow 를 끝낸다.
            let popups = &mut self.popups;
            let installed = &mut self.popup_fonts_installed;
            let tr = &self.tr;
            let Some(st) = self.file_open.get_mut(&iid) else {
                return;
            };
            let popup = popups.entry(iid).or_insert_with(|| EguiMeshPopup::new(iid));
            if installed.insert(iid) {
                install_fonts(popup.context());
            }
            let result = popup.paint(&ctx.host, &ctx.params, |egui_ctx| {
                action = draw_file_open(egui_ctx, &theme, st, tr);
            });
            if let Err(e) = result {
                tracing::warn!("markdown file-open popup {iid} paint failed: {e}");
            }
        }

        match action {
            FileOpenAction::Browse => {
                // host 소유 file_picker popup 을 트리거(ADR-0058) — attach
                // (원격) workspace 에서도 동작한다(native rfd 다이얼로그와 달리 원격
                // 개념이 있다). 즉시 request_id 만 돌아오고, 실제 선택 결과는 나중에
                // `on_event` 의 `"file_picker.result"` 로 비동기 도착한다.
                let start = self
                    .file_open
                    .get(&iid)
                    .map(|st| st.picker_start.clone())
                    .unwrap_or_default();
                if let Some(request_id) = trigger_file_picker(&ctx.host, iid, &start) {
                    self.pending_file_picker.insert(request_id, iid);
                }
            }
            FileOpenAction::Open => {
                let (path, convert_sid) = self
                    .file_open
                    .get(&iid)
                    .map(|s| (s.path_input.trim().to_string(), s.convert_surface_id))
                    .unwrap_or_default();
                if !path.is_empty() {
                    match convert_sid {
                        // convert 대상 surface → 제자리 markdown 변환.
                        Some(sid) => navigate(&ctx.host, sid, &path),
                        // 대상 없음 → 새 탭으로 연다.
                        None => open_markdown_file(&ctx.host, iid, &path),
                    }
                    close_popup(&ctx.host, iid);
                }
            }
            FileOpenAction::Cancel => close_popup(&ctx.host, iid),
            FileOpenAction::None => {}
        }
    }

    /// unix/windows 외에는 egui-mesh 채널이 비활성이라 확인 팝업 렌더는 no-op(참고:
    /// 이 두 cfg 는 사실상 모든 실제 지원 플랫폼을 덮는다 — 예외적 타겟용 안전망).
    #[cfg(not(any(unix, windows)))]
    pub(crate) fn paint_confirm(&mut self, _ctx: PopupSetContextCtx) {}

    /// unix/windows 외에는 egui-mesh 채널이 비활성이라 파일열기 팝업 렌더는 no-op.
    #[cfg(not(any(unix, windows)))]
    pub(crate) fn paint_file_open(&mut self, _ctx: PopupSetContextCtx) {}
}

/// 경로에서 표시용 파일명(basename)을 파생한다. 파생 실패 시 경로 그대로.
pub(crate) fn basename(path: &str) -> String {
    std::path::Path::new(path)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string())
}

/// bytes → "3.2 MB" 형태. 10MB 이상은 소수점 없이(host size_confirm 미러).
pub(crate) fn format_size(bytes: u64) -> String {
    let mb = bytes as f64 / (1024.0 * 1024.0);
    if mb >= 10.0 {
        format!("{mb:.0} MB")
    } else {
        format!("{mb:.1} MB")
    }
}

/// host 에 팝업 인스턴스 닫기를 요청한다(셸 생명주기는 host 소유).
#[cfg(any(unix, windows))]
fn close_popup(host: &HostHandle, instance_id: u64) {
    if let Err(e) = host.call("popup.close", json!({ "instance_id": instance_id })) {
        tracing::warn!("markdown confirm popup close failed: {e}");
    }
}

/// 확인 팝업에서 사용자가 고른 결정.
#[cfg(any(unix, windows))]
#[derive(Clone, Copy, PartialEq, Eq)]
enum ConfirmChoice {
    /// 대용량이어도 읽어 렌더한다.
    Open,
    /// 열지 않는다(surface 대기 유지).
    Cancel,
}

/// large-file 확인 팝업 콘텐츠. 파일명 + 크기 태그 + 안내문 + [취소]/[열기]. 색·폰트·
/// 간격은 host 가 보낸 `Theme` 토큰에서만 가져온다. 셸(scrim/border/Esc)은 host 소유.
#[cfg(any(unix, windows))]
fn draw_confirm(
    ctx: &egui::Context,
    theme: &Theme,
    confirm: &LargeFileConfirm,
    tr: &Translator,
) -> Option<ConfirmChoice> {
    use tasty_ui_widgets::{Button, ButtonVariant, TagVariant, margin_all, tag, vspace};

    let frame = egui::Frame::new()
        .fill(theme.bg_panel().to_egui())
        .inner_margin(margin_all(theme.spacing_md));
    let mut choice = None;
    egui::CentralPanel::default().frame(frame).show(ctx, |ui| {
        ui.spacing_mut().item_spacing.y = theme.spacing_sm.value();

        // 제목.
        ui.label(
            egui::RichText::new(tr.t("markdown.large_file.title"))
                .size(theme.font_size_body.value())
                .strong()
                .color(theme.text_primary().to_egui()),
        );

        // 파일명 (mono, muted).
        ui.add(
            egui::Label::new(
                egui::RichText::new(&confirm.file_name)
                    .size(theme.font_size_caption.value())
                    .family(egui::FontFamily::Monospace)
                    .color(theme.text_muted().to_egui()),
            )
            .truncate(),
        );

        // 경고 태그(크기) + 안내문.
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = theme.spacing_sm.value();
            tag(ui, theme, &confirm.size_label, TagVariant::Warning, false);
            ui.label(
                egui::RichText::new(tr.t("markdown.large_file.body"))
                    .size(theme.font_size_caption.value())
                    .color(theme.text_secondary().to_egui()),
            );
        });

        vspace(ui, theme.spacing_xs);

        // 푸터: 취소(ghost) / 열기(primary), 우측 정렬.
        ui.horizontal(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if Button::new(tr.t("markdown.large_file.open"))
                    .variant(ButtonVariant::Primary)
                    .show(ui, theme)
                    .clicked()
                {
                    choice = Some(ConfirmChoice::Open);
                }
                if Button::new(tr.t("markdown.large_file.cancel"))
                    .variant(ButtonVariant::Ghost)
                    .show(ui, theme)
                    .clicked()
                {
                    choice = Some(ConfirmChoice::Cancel);
                }
            });
        });
    });
    choice
}

/// 파일열기 팝업에서 사용자가 취한 동작.
#[cfg(any(unix, windows))]
enum FileOpenAction {
    /// 아무것도 안 함.
    None,
    /// native 파일 다이얼로그를 연다(host fs.pick_file).
    Browse,
    /// 입력 경로로 파일을 연다.
    Open,
    /// 열지 않고 닫는다.
    Cancel,
}

/// host → 이 plugin unicast 이벤트 key(ADR-0058). host 측 대응값은
/// `src/app/dispatch/file_picker.rs::FILE_PICKER_RESULT_EVENT` — 공유 crate 가
/// 없어 리터럴을 양쪽에 중복 정의한다(git-viewer 의 `GIT_VIEWER_QUERY_RESULT_EVENT`
/// 와 동일 근거).
pub(crate) const FILE_PICKER_RESULT_EVENT: &str = "file_picker.result";

/// `"file_picker.result"` 이벤트 payload wire — ADR-0058 Decision 4 의 최소 필드
/// (`request_id`/`paths`/`cancelled`) 그대로.
#[derive(serde::Deserialize)]
pub(crate) struct FilePickerResultWire {
    pub(crate) request_id: u64,
    #[serde(default)]
    pub(crate) paths: Vec<String>,
    #[serde(default)]
    pub(crate) cancelled: bool,
}

/// browse — host 소유 `file_picker` popup 을 트리거한다(`file_picker.trigger`,
/// ADR-0058). plugin 프로세스는 native OS 다이얼로그도, host 의 in-app
/// popup 도 직접 못 열기 때문에 host 에 위임한다. markdown 확장자로 필터. 반환값은
/// **선택 경로가 아니라** 이 요청의 `request_id` — 실제 경로는 나중에 `on_event`
/// 의 `"file_picker.result"` 로 비동기 도착한다(ADR-0058 의 즉시 ack + 이벤트 push,
/// 옛 `fs.pick_file`/rfd 동기 모달과 달리 이 호출 자체는 popup 확정을 기다리지 않고
/// 곧장 반환된다).
///
/// `owner_popup_instance` 로 자기 popup instance 를 함께 신고한다 — host 가 두 팝업을
/// 부모-자식 스택으로 다루는 근거다(ADR-0084). `start` 는 피커의 출발 폴더와 출발 surface
/// 다 — 없으면 host 가 홈에서 연다.
#[cfg(any(unix, windows))]
fn trigger_file_picker(
    host: &HostHandle,
    owner_popup_instance: u64,
    start: &PickerStart,
) -> Option<u64> {
    match host.call(
        "file_picker.trigger",
        file_picker_trigger_params(owner_popup_instance, start),
    ) {
        Ok(v) => v.get("request_id").and_then(Value::as_u64),
        Err(e) => {
            tracing::warn!("markdown file-open browse (file_picker.trigger) failed: {e}");
            None
        }
    }
}

/// `file_picker.trigger` 파라미터. 호출(`HostHandle`)과 떼어 두어 단위 테스트가 wire 모양을 본다.
#[cfg(any(unix, windows))]
pub(crate) fn file_picker_trigger_params(owner_popup_instance: u64, start: &PickerStart) -> Value {
    json!({
        "filters": ["md", "markdown"],
        // 부모-자식 스택을 host 가 세울 수 있게 자기 popup instance 를 신고한다
        // (ADR-0084). 이게 없으면 이 팝업이 피커보다 먼저 닫혀 고아가 생기고,
        // 고른 파일이 조용히 버려진다.
        "owner_popup_instance": owner_popup_instance,
        "start_dir": start.dir,
        "origin_surface_id": start.origin_surface_id,
    })
}

/// 입력/선택한 markdown 파일을 host `file_handler.dispatch` 로 연다(origin 없이 → focused
/// pane 의 새 탭). markdown 감지·surface 생성은 host detector 가 담당한다.
#[cfg(any(unix, windows))]
fn open_markdown_file(host: &HostHandle, owner_popup_instance: u64, path: &str) {
    if let Err(e) = host.call(
        "file_handler.dispatch",
        file_open_dispatch_params(owner_popup_instance, path),
    ) {
        tracing::warn!("markdown file-open dispatch failed: {e}");
    }
}

/// 파일열기 팝업 [열기] 의 `file_handler.dispatch` 파라미터. 호출(`HostHandle`)과 떼어 두어
/// 단위 테스트가 wire 모양을 본다.
///
/// `owner_popup_instance` 는 이 호출이 사용자가 만진 이 팝업에서 왔다는 표지다 — host 는 그
/// 팝업이 이 plugin 소유이고 사용자의 입력을 받았을 때만 사용자 행동으로 친다. 빠지면 에이전트
/// 요청이 되어, 사용자가 연 새 탭이 선택되지 않고 적용 실패도 사용자에게 안 보인다(ADR-0526).
#[cfg(any(unix, windows))]
pub(crate) fn file_open_dispatch_params(owner_popup_instance: u64, path: &str) -> Value {
    json!({
        "path": path,
        "depth": "deep",
        "owner_popup_instance": owner_popup_instance,
    })
}

/// 파일열기 팝업 콘텐츠. 경로 입력 필드 + [browse] + [취소]/[열기]. 색·폰트·간격은 host 가
/// 보낸 `Theme` 토큰에서만 가져온다. 키보드/텍스트 입력은 host 가 popup raw_input 으로
/// forward 한다. 셸(scrim/border/Esc/outside-click)은 host 소유.
#[cfg(any(unix, windows))]
fn draw_file_open(
    ctx: &egui::Context,
    theme: &Theme,
    st: &mut FileOpenState,
    tr: &Translator,
) -> FileOpenAction {
    use tasty_ui_widgets::{Button, ButtonVariant, ControlSize, margin_all, vspace};

    let frame = egui::Frame::new()
        .fill(theme.bg_panel().to_egui())
        .inner_margin(margin_all(theme.spacing_md));
    let mut action = FileOpenAction::None;
    egui::CentralPanel::default().frame(frame).show(ctx, |ui| {
        ui.spacing_mut().item_spacing.y = theme.spacing_sm.value();

        // Esc → 닫기(host 셸도 처리하나 forward 된 입력에서 즉시 반응).
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            action = FileOpenAction::Cancel;
        }

        // 제목.
        ui.label(
            egui::RichText::new(tr.t("markdown.file_open.title"))
                .size(theme.font_size_body.value())
                .strong()
                .color(theme.text_primary().to_egui()),
        );

        // 경로 라벨.
        ui.label(
            egui::RichText::new(tr.t("markdown.file_open.path_label"))
                .size(theme.font_size_caption.value())
                .color(theme.text_secondary().to_egui()),
        );

        // 경로 입력 + browse. right_to_left 로 Browse 를 먼저(우측) 배치해 실제 렌더
        // 폭(아이콘+라벨+패딩)만큼 자연히 소비시키고, 입력 필드는 남은
        // `ui.available_width()` 를 그대로 쓴다 — misc.rs/remote_transfer.rs 의 Browse
        // 선례와 동형이라 고정 spacing 상수를 빼는 수동 계산(클리핑 원인)이 필요 없다.
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = theme.spacing_sm.value();
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let browse_clicked = Button::new(tr.t("markdown.file_open.browse"))
                    .variant(ButtonVariant::Secondary)
                    .size(ControlSize::Sm)
                    .leading_icon(&|ui, rect, c| {
                        tasty_plugin_sdk::baked_icon::draw(
                            ui.painter(),
                            baked_icons::FOLDER,
                            rect.center(),
                            rect.width(),
                            c,
                        );
                    })
                    .show(ui, theme)
                    .clicked();

                let field_w = ui.available_width();
                let resp = ui.add(
                    egui::TextEdit::singleline(&mut st.path_input)
                        .desired_width(field_w)
                        .hint_text(tr.t("markdown.file_open.path_label"))
                        .font(egui::FontId::new(
                            theme.font_size_body.value(),
                            egui::FontFamily::Monospace,
                        )),
                );
                if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    action = FileOpenAction::Open;
                }
                if browse_clicked {
                    action = FileOpenAction::Browse;
                }
            });
        });

        vspace(ui, theme.spacing_xs);

        // 푸터: 취소(ghost) / 열기(primary), 우측 정렬.
        ui.horizontal(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if Button::new(tr.t("markdown.file_open.open"))
                    .variant(ButtonVariant::Primary)
                    .show(ui, theme)
                    .clicked()
                {
                    action = FileOpenAction::Open;
                }
                if Button::new(tr.t("markdown.file_open.cancel"))
                    .variant(ButtonVariant::Ghost)
                    .show(ui, theme)
                    .clicked()
                {
                    action = FileOpenAction::Cancel;
                }
            });
        });
    });
    action
}

/// plugin Context 에 CJK fallback 을 설치한다(확인 팝업 전용 — 본문은 native WebView 가
/// 자체 폰트 스택으로 그린다). egui 기본 폰트(Proportional/Monospace) 뒤에 시스템 CJK
/// 폰트를 fallback 으로 붙여 한글/일문/한자가 tofu 되지 않게 한다.
#[cfg(any(unix, windows))]
fn install_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    if let Some(bytes) = load_system_cjk_font_data() {
        fonts.font_data.insert(
            "system_cjk".to_owned(),
            std::sync::Arc::new(egui::FontData::from_owned(bytes)),
        );
        for fam in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
            fonts
                .families
                .entry(fam)
                .or_default()
                .push("system_cjk".to_owned());
        }
    }
    // 언어팩 `[font]` 폰트를 CJK 뒤, 체인 맨 뒤 폴백으로 붙인다. host 두 경로와 같은
    // 판정기(`tasty_egui_theme::install_locale_font_fallback`)를 쓴다 — 검증이 곧 "어떤
    // 폰트를 거부하는가" 라는 판정이라 사본을 두면 host 는 받고 plugin 은 거부하는 갈림이
    // 생긴다. 경로는 host 가 resolve 해 `TASTY_LOCALE_FONT` 로 물려준 것(SDK
    // `PluginEnv.locale_font` 와 같은 출처).
    if let Some(path) = std::env::var_os("TASTY_LOCALE_FONT").filter(|v| !v.is_empty()) {
        let path = std::path::PathBuf::from(path);
        if let Err(e) = tasty_egui_theme::install_locale_font_fallback(&mut fonts, &path) {
            tracing::warn!(
                "locale font at {} could not be installed: {e}",
                path.display()
            );
        }
    }
    ctx.set_fonts(fonts);
}

/// 시스템 CJK 폰트 바이트 로드 (host `font_registry::load_system_cjk_font_data` 미러).
#[cfg(any(unix, windows))]
fn load_system_cjk_font_data() -> Option<Vec<u8>> {
    #[cfg(target_os = "windows")]
    {
        // host font_registry 미러 — 맑은 고딕(한글 tofu 방지). 없으면 None.
        if let Ok(data) = std::fs::read("C:/Windows/Fonts/malgun.ttf") {
            return Some(data);
        }
    }
    #[cfg(target_os = "macos")]
    {
        for path in &[
            "/System/Library/Fonts/AppleSDGothicNeo.ttc",
            "/System/Library/Fonts/Supplemental/Arial Unicode.ttf",
        ] {
            if let Ok(data) = std::fs::read(path) {
                return Some(data);
            }
        }
    }
    #[cfg(target_os = "linux")]
    {
        for path in &[
            "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
            "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
            "/usr/share/fonts/google-noto-cjk/NotoSansCJK-Regular.ttc",
            "/usr/share/fonts/truetype/noto/NotoSansCJK-Regular.ttc",
        ] {
            if let Ok(data) = std::fs::read(path) {
                return Some(data);
            }
        }
    }
    None
}
