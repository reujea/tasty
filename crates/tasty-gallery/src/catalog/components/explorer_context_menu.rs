//! 공용 메뉴 항목을 조합한 탐색기 우클릭 메뉴 예제.
//! 빈 영역·파일·폴더·다중 선택·원격 칸에 따른 구성을 보여준다. OS 연계 행(시스템에서 열기·파일 관리자에서
//! 보기)은 로컬 칸에서만 자기 묶음으로 오고, 모든 메뉴는 구분선 뒤 Properties 로 끝난다.
//! 붙여넣을 목록이 없으면 Paste 행은 빠지지 않고 꺼진다. 대상이 사라진 링크의 들어 있는 폴더에서 보기도 꺼진다.

mod history;

use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;
use tasty_ui_widgets::{MenuItemVariant, menu_item, menu_separator};

use crate::catalog::icons::{
    COPY, EDIT, FILE_PLUS, FOLDER_OPEN, FOLDER_PLUS, INFO, MockGlyph, STAR, TRASH,
};
use crate::catalog::spec::{StageVariant, TokenChip, meta, note, stage};
use crate::i18n::t;

/// 메뉴 한 줄.
pub(super) enum Mi {
    /// (leading glyph, label, danger 여부)
    Item(Option<MockGlyph>, &'static str, bool),
    /// 작업 문구, 설정 키, 활성 여부와 비활성 이유.
    Row(String, &'static str, bool, Option<String>),
    /// 이유 없이 꺼진 행 (leading glyph, label).
    Off(Option<MockGlyph>, &'static str),
    Sep,
}

/// 붙여넣기 행. 붙여넣을 목록이 없으면 이유 없이 꺼진다.
fn paste_row(label: &str, enabled: bool) -> Mi {
    Mi::Row(label.into(), "paste", enabled, None)
}

fn empty_menu() -> Vec<Mi> {
    empty_menu_with(true)
}

fn empty_menu_nothing_to_paste() -> Vec<Mi> {
    empty_menu_with(false)
}

fn empty_menu_with(paste: bool) -> Vec<Mi> {
    vec![
        Mi::Item(Some(FOLDER_PLUS), t("explorer.command.new_folder"), false),
        Mi::Item(Some(FILE_PLUS), t("explorer.command.new_file"), false),
        Mi::Sep,
        Mi::Item(Some(COPY), "Copy path", false),
        Mi::Item(Some(STAR), "Add to favorites", false),
        Mi::Sep,
        paste_row("Paste", paste),
        Mi::Sep,
        properties(),
    ]
}

fn properties() -> Mi {
    Mi::Item(Some(INFO), t("explorer.context_menu.properties"), false)
}

fn file_menu() -> Vec<Mi> {
    vec![
        Mi::Item(Some(COPY), "Copy path", false),
        Mi::Item(Some(COPY), "Copy", false),
        Mi::Item(None, "Cut", false),
        Mi::Sep,
        Mi::Item(Some(TRASH), "Delete", true),
        Mi::Item(Some(EDIT), "Rename", false),
        Mi::Sep,
        Mi::Item(None, t("explorer.context_menu.open_in_system"), false),
        Mi::Item(None, t("explorer.context_menu.reveal_files"), false),
        Mi::Sep,
        properties(),
    ]
}

fn folder_menu() -> Vec<Mi> {
    folder_menu_with(true)
}

fn folder_menu_nothing_to_paste() -> Vec<Mi> {
    folder_menu_with(false)
}

fn folder_menu_with(paste: bool) -> Vec<Mi> {
    vec![
        Mi::Item(Some(COPY), "Copy path", false),
        Mi::Item(Some(STAR), "Add to favorites", false),
        Mi::Item(Some(COPY), "Copy", false),
        Mi::Item(None, "Cut", false),
        paste_row("Paste (into)", paste),
        Mi::Sep,
        Mi::Item(Some(FOLDER_PLUS), t("explorer.command.new_folder"), false),
        Mi::Item(Some(FILE_PLUS), t("explorer.command.new_file"), false),
        Mi::Item(Some(TRASH), "Delete", true),
        Mi::Item(Some(EDIT), "Rename", false),
        Mi::Sep,
        Mi::Item(None, t("explorer.context_menu.open_in_system"), false),
        Mi::Item(None, t("explorer.context_menu.reveal_files"), false),
        Mi::Sep,
        properties(),
    ]
}

fn multi_menu() -> Vec<Mi> {
    vec![
        Mi::Item(Some(COPY), "Copy path (newline-sep)", false),
        Mi::Item(Some(COPY), "Copy", false),
        Mi::Item(None, "Cut", false),
        Mi::Sep,
        Mi::Item(Some(TRASH), "Delete", true),
        Mi::Sep,
        Mi::Item(None, t("explorer.context_menu.open_in_system"), false),
        Mi::Sep,
        properties(),
    ]
}

/// 원격 칸의 파일 하나. OS 연계 행과 그 구분선이 함께 빠진다.
fn remote_menu() -> Vec<Mi> {
    vec![
        Mi::Item(Some(COPY), "Copy path", false),
        Mi::Item(Some(COPY), "Copy", false),
        Mi::Item(None, "Cut", false),
        Mi::Sep,
        Mi::Item(Some(TRASH), "Delete", true),
        Mi::Item(Some(EDIT), "Rename", false),
        Mi::Sep,
        properties(),
    ]
}

/// 하위 폴더 검색 결과 하나를 눌렀을 때. 들어 있는 폴더에서 보기가 Open 바로 뒤에 온다.
fn search_result_menu() -> Vec<Mi> {
    vec![
        Mi::Item(None, t("button.open"), false),
        Mi::Item(Some(FOLDER_OPEN), t("explorer.menu.show_in_folder"), false),
        Mi::Sep,
        Mi::Item(None, "Copy path", false),
    ]
}

/// 대상이 사라진 링크 하나를 눌렀을 때. 들어 있는 폴더에서 보기는 꺼진 채 맨 앞에 남는다.
fn broken_link_menu() -> Vec<Mi> {
    vec![
        Mi::Off(Some(FOLDER_OPEN), t("explorer.menu.show_in_folder")),
        Mi::Sep,
        Mi::Item(None, "Copy path", false),
    ]
}

/// (caption, 항목 빌더) — 한 컨텍스트 메뉴 variant.
type Variant = (&'static str, fn() -> Vec<Mi>);

/// 시안 메뉴 한 장의 바깥 폭(`width: 220`, border-box).
const MENU_OUTER_W: LogicalPx = LogicalPx(220.0);
/// 시안 무대의 메뉴 사이 간격(`gap: 18`). 대응 토큰이 없는 전시용 값이다.
const MENU_GAP: LogicalPx = LogicalPx(18.0);

pub fn draw(ui: &mut egui::Ui, theme: &Theme) {
    // 시안 메뉴 폭 220 은 테두리·안쪽 여백을 포함한 바깥 폭이다.
    let menu_w =
        MENU_OUTER_W.value() - theme.spacing_xs.value() * 2.0 - theme.border_width.value() * 2.0;
    let variants: [Variant; 4] = [
        ("empty area → cwd", empty_menu),
        ("file (single)", file_menu),
        ("folder (single)", folder_menu),
        ("multi-select", multi_menu),
    ];

    stage(ui, theme, StageVariant::Wrap, |ui| {
        ui.spacing_mut().item_spacing = egui::vec2(MENU_GAP.value(), theme.spacing_lg.value());
        for (caption, build) in variants {
            menu_column(ui, theme, menu_w, caption, &build());
        }
    });
    // 원격 칸 메뉴와 검색 결과 메뉴는 대상 넷과 따로 한 줄에 둔다. 같은 줄에 두면 좁은 창에서 줄이 넘친다.
    stage(ui, theme, StageVariant::Wrap, |ui| {
        ui.spacing_mut().item_spacing = egui::vec2(MENU_GAP.value(), theme.spacing_lg.value());
        menu_column(ui, theme, menu_w, "remote cell · file", &remote_menu());
        menu_column(ui, theme, menu_w, "search result", &search_result_menu());
        menu_column(ui, theme, menu_w, "link · target gone", &broken_link_menu());
    });
    // 붙여넣을 목록이 없는 빈 영역·폴더 메뉴. Paste 행이 꺼진 채 남는다.
    stage(ui, theme, StageVariant::Wrap, |ui| {
        ui.spacing_mut().item_spacing = egui::vec2(MENU_GAP.value(), theme.spacing_lg.value());
        menu_column(
            ui,
            theme,
            menu_w,
            "empty area · nothing to paste",
            &empty_menu_nothing_to_paste(),
        );
        menu_column(
            ui,
            theme,
            menu_w,
            "folder · nothing to paste",
            &folder_menu_nothing_to_paste(),
        );
    });
    history::draw(ui, theme);

    meta(
        ui,
        theme,
        &[
            ("container", "surface-raised · 1px border-strong"),
            ("item", "menu_item (28 control-height)"),
            ("separator", "menu_separator (1px)"),
            ("danger", "Delete = accent-danger label"),
            (
                "targets",
                "empty · file · folder · multi · remote cell · search result · link with its target gone · nothing to paste",
            ),
            (
                "open in system · show in file manager",
                "local only, own group after Delete · Open in system opens every chosen item · Show in Finder / File Explorer / file manager selects one item in its folder · remote hides both rows and their separator · row icons wait for native menu icon support",
            ),
            (
                "show in folder",
                "search results and links · opens the real parent · selects + current item · scrolls into view · no flash",
            ),
            (
                "paste",
                "always in its slot · nothing to paste (or a list from another computer): disabled row, no reason",
            ),
            (
                "show in folder · target gone",
                "a link whose target is gone keeps a disabled row first · no Open row",
            ),
            (
                "stale step",
                "disabled row · reason tooltip on Linux/macOS · reason label on Windows",
            ),
            (
                "undo / redo",
                "empty area · before Properties · empty history keeps disabled Undo / Redo",
            ),
            (
                "create",
                "empty area: New folder · New file first · folder: in the file-ops group · file / multi: none · remote that can't write: disabled · reason tooltip",
            ),
        ],
        &[
            TokenChip::new("menu-bg", "menu fill", egui::Color32::from(theme.menu_bg())),
            TokenChip::new(
                "border-strong",
                "menu edge",
                egui::Color32::from(theme.border_strong()),
            ),
            TokenChip::new(
                "accent-danger",
                "delete label",
                egui::Color32::from(theme.accent_danger()),
            ),
            TokenChip::without_color("shadow-popover", "float"),
            TokenChip::without_color("menu-border", "1px edge"),
        ],
    );

    note(
        ui,
        theme,
        "Target rule (body): a right-clicked item inside the selection acts on the whole \
         selection; outside it, the selection resets to that item; on the background, the \
         cwd. New folder and New file create in the cwd from the background and inside the folder \
         from a folder row; the target is fixed when the command starts. Add-to-favorites shows only on a single folder or the background; Open in \
         system only on a folder.",
    );
}

/// caption 과 메뉴 한 장을 세로로 묶는다.
fn menu_column(ui: &mut egui::Ui, theme: &Theme, menu_w: f32, caption: &str, items: &[Mi]) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = theme.spacing_sm.value();
        ui.label(
            egui::RichText::new(caption.to_uppercase())
                .size(theme.font_size_micro.value())
                .color(egui::Color32::from(theme.text_muted())),
        );
        render_menu(ui, theme, menu_w, items);
    });
}

pub(super) fn render_menu(ui: &mut egui::Ui, theme: &Theme, width: f32, items: &[Mi]) {
    egui::Frame::new()
        .fill(egui::Color32::from(theme.surface_raised()))
        .stroke(egui::Stroke::new(
            theme.border_width.value(),
            egui::Color32::from(theme.border_strong()),
        ))
        .corner_radius(theme.corner_radius.value())
        .inner_margin(egui::Margin::same(theme.spacing_xs.value() as i8))
        .show(ui, |ui| {
            ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                ui.set_width(width);
                ui.spacing_mut().item_spacing.y = 0.0;
                for it in items {
                    match it {
                        Mi::Sep => menu_separator(ui, theme),
                        Mi::Off(glyph, label) => {
                            let paint = glyph.map(|g| {
                                move |ui: &mut egui::Ui, rect: egui::Rect, c: egui::Color32| {
                                    g.image(rect.height(), c).paint_at(ui, rect)
                                }
                            });
                            menu_item(
                                ui,
                                theme,
                                paint
                                    .as_ref()
                                    .map(|p| p as tasty_ui_widgets::IconPainter<'_>),
                                label,
                                None,
                                MenuItemVariant::Normal,
                                false,
                                false,
                            );
                        }
                        Mi::Row(label, field, enabled, tooltip) => {
                            let shortcut = shortcut(field);
                            let row = menu_item(
                                ui,
                                theme,
                                None,
                                label,
                                shortcut.as_deref(),
                                MenuItemVariant::Normal,
                                false,
                                *enabled,
                            );
                            if let Some(why) = tooltip {
                                row.on_disabled_hover_text(why);
                            }
                        }
                        Mi::Item(glyph, label, danger) => {
                            let shortcut = item_shortcut(label);
                            let variant = if *danger {
                                MenuItemVariant::Danger
                            } else {
                                MenuItemVariant::Normal
                            };
                            match glyph {
                                Some(g) => {
                                    let g = *g;
                                    menu_item(
                                        ui,
                                        theme,
                                        Some(&|ui, rect, c| {
                                            g.image(rect.height(), c).paint_at(ui, rect)
                                        }),
                                        label,
                                        shortcut.as_deref(),
                                        variant,
                                        false,
                                        true,
                                    );
                                }
                                None => {
                                    menu_item(
                                        ui,
                                        theme,
                                        None,
                                        label,
                                        shortcut.as_deref(),
                                        variant,
                                        false,
                                        true,
                                    );
                                }
                            }
                        }
                    }
                }
            });
        });
}

fn shortcut(field: &str) -> Option<String> {
    let settings = tasty_settings::Settings::default();
    let binding = settings.keybindings.get_bindings(field)?.first()?;
    let mut parts =
        tasty_settings::KeybindingSettings::format_display_parts(binding, &settings.general);
    if let Some(key) = parts.last_mut()
        && key.eq_ignore_ascii_case("delete")
    {
        *key = "Del".into();
    }
    Some(parts.join("+"))
}

fn item_shortcut(label: &str) -> Option<String> {
    let field = match label {
        "Copy path" | "Copy path (newline-sep)" => "copy_path",
        "Copy" => "copy",
        "Cut" => "cut",
        "Paste" | "Paste (into)" => "paste",
        "Rename" => "explorer_rename",
        "Delete" => "explorer_trash",
        label if label == t("explorer.command.new_folder") => "explorer_new_folder",
        label if label == t("explorer.command.new_file") => "explorer_new_file",
        label if label == t("explorer.context_menu.properties") => "explorer_properties",
        _ => return None,
    };
    shortcut(field)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 붙여넣기 행의 (활성, 이유). 메뉴마다 하나다.
    fn paste(items: &[Mi]) -> (bool, bool) {
        let rows: Vec<_> = items
            .iter()
            .filter_map(|i| match i {
                Mi::Row(_, "paste", enabled, why) => Some((*enabled, why.is_some())),
                _ => None,
            })
            .collect();
        assert_eq!(rows.len(), 1, "one paste row");
        rows[0]
    }

    #[test]
    fn nothing_to_paste_keeps_a_disabled_paste_row_without_a_reason() {
        assert_eq!(paste(&empty_menu()), (true, false));
        assert_eq!(paste(&folder_menu()), (true, false));
        assert_eq!(paste(&empty_menu_nothing_to_paste()), (false, false));
        assert_eq!(paste(&folder_menu_nothing_to_paste()), (false, false));
        // 꺼진 변형은 켠 변형과 행 수가 같다(행이 빠지지 않는다).
        assert_eq!(empty_menu().len(), empty_menu_nothing_to_paste().len());
        assert_eq!(folder_menu().len(), folder_menu_nothing_to_paste().len());
    }

    #[test]
    fn a_link_whose_target_is_gone_shows_a_disabled_show_in_folder_first() {
        let items = broken_link_menu();
        assert!(matches!(
            &items[0],
            Mi::Off(Some(_), label) if *label == t("explorer.menu.show_in_folder")
        ));
        assert!(matches!(items[1], Mi::Sep));
        assert!(
            !items
                .iter()
                .any(|i| matches!(i, Mi::Item(_, label, _) if *label == t("button.open"))),
            "no Open row for a link whose target is gone"
        );
    }
}
