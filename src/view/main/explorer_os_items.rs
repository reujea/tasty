//! explorer 컨텍스트 메뉴의 OS 연계 행. "시스템에서 열기"는 고른 항목을 모두 OS 기본 앱(폴더는
//! 파일 관리자)으로 열고, "파일 관리자에서 보기"는 한 항목을 고른 채 담은 폴더를 연다.
//! 생성 행처럼 메뉴 구성(`redraw.rs`)과 따로 둔다. 두 행 모두 로컬 칸에서만 쓰고 원격 칸에서는 꺼진다.

use super::MainView;
use crate::platform::native_menu::{MenuIcon, MenuItem};
use crate::runtime::engine_read::EngineRead;

const OPEN_IN_SYSTEM: u32 = 20;
const REVEAL: u32 = 21;

/// "파일 관리자에서 보기" 행 이름의 번역 키. 각 OS 가 자기 파일 관리자를 부르는 이름을 쓴다.
fn reveal_label_key() -> &'static str {
    if cfg!(target_os = "macos") {
        "explorer.context_menu.reveal_finder"
    } else if cfg!(windows) {
        "explorer.context_menu.reveal_explorer"
    } else {
        "explorer.context_menu.reveal_files"
    }
}

/// OS 메뉴는 크기를 스스로 정하므로 앱 zoom 을 적용하지 않은 원본 크기를 넘긴다.
fn menu_icon(svg: &'static str) -> MenuIcon {
    MenuIcon::from_svg(svg, tasty_type_appearance::theme::SIZING.icon_glyph_size_md)
}

impl MainView {
    /// 휴지통 뒤에 붙는 OS 연계 행. 파일 관리자에서 보기는 한 항목일 때만 둔다.
    /// `local_only` 이유가 있으면(원격 칸) 두 행을 끄고 그 이유를 툴팁으로 단다.
    pub(super) fn push_explorer_os_items(
        items: &mut Vec<MenuItem>,
        multi: bool,
        local_only: Option<&str>,
    ) {
        let row = |id: u32, label: &str, icon: MenuIcon| {
            let item = MenuItem::new(id, label).with_icon(icon);
            match local_only {
                None => item,
                Some(why) => MenuItem {
                    enabled: false,
                    ..item
                }
                .with_tooltip(why),
            }
        };
        items.push(row(
            OPEN_IN_SYSTEM,
            crate::i18n::t("explorer.context_menu.open_in_system"),
            menu_icon(tasty_icons::EXTERNAL.svg),
        ));
        if !multi {
            items.push(row(
                REVEAL,
                crate::i18n::t(reveal_label_key()),
                menu_icon(tasty_icons::FOLDER_OPEN.svg),
            ));
        }
    }

    /// 파일 관리자에서 보기 (아이템 21).
    pub(super) fn explorer_menu_reveal(
        &mut self,
        engine: &EngineRead<'_>,
        surface_id: u32,
        paths: &[std::path::PathBuf],
    ) {
        if engine.is_mirror_surface(surface_id) {
            self.toast_remote_write_unsupported();
            return;
        }
        if let Some(target) = paths.first().cloned() {
            self.state.request_explorer_file(
                engine,
                surface_id,
                crate::app::explorer_files::Operation::Reveal(target),
                crate::intent::IntentOrigin::User {
                    source: crate::intent::UserSource::ContextMenu,
                },
            );
        }
    }
}
