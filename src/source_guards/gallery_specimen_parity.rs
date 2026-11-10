//! 본체에 등록된 popup·전체화면 무대에 대응하는 갤러리 specimen이 있는지 확인한다.
//! 이름만으로 대응 관계를 정할 수 없어 명부를 유지한다. 디자인 반영 순서는 docs/design/systems/theme.md를 따른다.
//!
//! 파일 존재와 catalog.rs의 모듈 경로 문자열을 확인한다. use 줄만 있어도 통과할 수 있으므로
//! 실제 페이지에 Spec이 배치됐는지까지 보장하지는 않는다. 등록처가 없는 공용 위젯도 검사 범위 밖이다.
//! popup 등록처는 gui feature에서만 사용할 수 있고 무대 검사는 헤드리스에서도 실행된다.
//! 출력 명부는 --nocapture로 볼 수 있으며 통과한 시험의 기본 출력에는 숨겨진다.

const CATALOG_ROOT: &str = "crates/tasty-gallery/src/catalog";

const CATALOG_TREE: &str = "crates/tasty-gallery/src/catalog.rs";

/// 전송 진행·실패나 적용 범위처럼 여러 popup ID가 한 specimen을 공유할 수 있다.
#[cfg(feature = "gui")]
const POPUP_SPECIMENS: &[(&str, &str)] = &[
    ("notifications", "components/notification_panel.rs"),
    ("convert_surface", "components/convert.rs"),
    ("script_changed_confirm", "components/script_confirm.rs"),
    ("rename", "components/rename_popup.rs"),
    ("explorer_properties", "components/explorer_file_info.rs"),
    ("explorer_conflict", "components/explorer_jobs.rs"),
    ("search_bar", "components/search_bar.rs"),
    ("tools_menu", "components/tools_menu.rs"),
    ("info_modal", "components/info_modal.rs"),
    ("approval", "components/approval.rs"),
    ("file_handler_picker", "components/file_handler_picker.rs"),
    ("file_picker", "components/file_picker.rs"),
    ("port_scanner", "components/port_scanner.rs"),
    ("command_palette", "components/command_palette.rs"),
    ("dag_list", "components/dag/window.rs"),
    ("remote_tool", "components/remote.rs"),
    ("remote_attach", "components/remote_attach.rs"),
    ("transfer_progress", "components/transfer.rs"),
    ("transfer_error", "components/transfer.rs"),
    ("download_run", "components/download_run.rs"),
    ("download_run_large", "components/download_run.rs"),
    ("download_run_progress", "components/download_run.rs"),
    ("apply_workspace_preset", "components/apply_preset.rs"),
    ("apply_tab_preset", "components/apply_preset.rs"),
    ("apply_pane_preset", "components/apply_preset.rs"),
    ("mouse_capture_banner_menu", "widgets/banner.rs"),
    ("confirm_delete_category", "components/category_dialogs.rs"),
    ("tutorial_topics", "widgets/tutorial.rs"),
    ("rail_category", "components/category_dialogs.rs"),
    (
        "confirm_force_detach_workspace",
        "components/occupancy_borders.rs",
    ),
    ("confirm_close_remote", "components/remote_row_menu.rs"),
    ("remote_row", "layouts_remote_row.rs"),
    ("remote_row_over_screen", "layouts_remote_row.rs"),
];

/// specimen이 없는 popup과 미완성 사유. 검사를 통과하기 위한 일반 예외 목록이 아니다.
#[cfg(feature = "gui")]
const POPUP_WITHOUT_SPECIMEN: &[(&str, &str)] = &[];

/// blank 무대는 내용 없는 셸이므로 notifications와 같은 전체화면 specimen을 쓴다.
const STAGE_SPECIMENS: &[(&str, &str)] = &[
    ("blank", "components/fullscreen_stage.rs"),
    ("notifications", "components/fullscreen_stage.rs"),
];

/// specimen이 없는 무대와 미완성 사유.
const STAGE_WITHOUT_SPECIMEN: &[(&str, &str)] = &[];

/// 비교 전에 등록처의 하한과 알려진 항목을 확인해 빈 수집의 통과를 막는다.
fn check(
    what: &str,
    registry: &[&str],
    roster: &[(&str, &str)],
    debts: &[(&str, &str)],
    debt_budget: usize,
    floor: usize,
    control: &str,
) {
    // 미완성 항목의 증가뿐 아니라 해결 뒤 오래된 기록이 남는 경우도 찾도록 정확한 수를 맞춘다.
    assert_eq!(
        debts.len(),
        debt_budget,
        "{what}에 specimen이 없는 항목이 {}개다(기록 {debt_budget}). 실제 미완성 항목과 기록을 대조한다.",
        debts.len()
    );
    assert!(
        registry.len() >= floor,
        "{what} 등록처가 {}개로 하한 {floor} 미만이다. 등록처를 올바르게 읽었는지 확인한다.",
        registry.len()
    );
    assert!(
        registry.contains(&control),
        "{what} 등록처에서 대조 항목 `{control}`을 찾지 못했다"
    );

    let root = super::repo_root();
    let tree = std::fs::read_to_string(root.join(CATALOG_TREE)).unwrap_or_default();
    assert!(
        tree.len() > 1000,
        "카탈로그 `{CATALOG_TREE}`를 읽지 못해 모듈 경로를 확인할 수 없다"
    );

    let mut listed = Vec::new();
    for id in registry {
        if let Some((_, why)) = debts.iter().find(|(k, _)| k == id) {
            listed.push(format!("  {id:<26} specimen 없음 — {why}"));
            continue;
        }
        let Some((_, rel)) = roster.iter().find(|(k, _)| k == id) else {
            panic!(
                "{what} `{id}`에 대응하는 갤러리 specimen이 없다. docs/design/systems/theme.md에 따라 본체 반영 전에 specimen을 만들고 `{}`의 명부에 등록한다. 미완성 목록으로 옮겨도 구현이 끝난 것이 아니며 해당 수를 함께 기록해야 한다.",
                file!()
            );
        };
        let path = root.join(CATALOG_ROOT).join(rel);
        assert!(
            path.is_file(),
            "{what} `{id}` 의 specimen 으로 적힌 `{rel}` 이 없다 — 파일이 옮겨졌으면 명부를 따라 옮겨라"
        );
        let module = rel.trim_end_matches(".rs").replace('/', "::");
        assert!(
            tree.contains(&module),
            "{what} `{id}`의 specimen `{rel}`에 대응하는 모듈 경로 `{module}`이 `{CATALOG_TREE}`에 없다. 카탈로그 등록을 확인한다."
        );
        listed.push(format!("  {id:<26} → {rel}"));
    }

    for (id, _) in roster.iter().chain(debts.iter()) {
        assert!(
            registry.contains(id),
            "{what} 명부에 등록처에 없는 행 `{id}` 이 있다 — popup 이 지워졌으면 명부에서도 지워라"
        );
    }

    println!(
        "[{what}] 등록처 {} · 명부 {} · 빚 {}\n{}",
        registry.len(),
        roster.len(),
        debts.len(),
        listed.join("\n")
    );
}

#[cfg(feature = "gui")]
#[test]
fn every_host_popup_has_a_gallery_specimen() {
    let registry: Vec<&str> = crate::adapters::ui::popup::defs::all_defs()
        .iter()
        .map(|d| d.id)
        .collect();
    check(
        "popup",
        &registry,
        POPUP_SPECIMENS,
        POPUP_WITHOUT_SPECIMEN,
        0,
        20,
        "notifications",
    );
}

/// all_metas()가 시험용으로 추가한 __test 접두사 무대에는 specimen을 요구하지 않는다.
#[test]
fn every_fullscreen_stage_has_a_gallery_specimen() {
    let registry: Vec<&str> = crate::fullscreen_stages::all_metas()
        .iter()
        .map(|m| m.id)
        .filter(|id| !id.starts_with("__test"))
        .collect();
    check(
        "무대",
        &registry,
        STAGE_SPECIMENS,
        STAGE_WITHOUT_SPECIMEN,
        0,
        1,
        "notifications",
    );
}
