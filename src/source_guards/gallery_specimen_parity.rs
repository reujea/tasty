//! **본체에 들어온 popup·무대가 갤러리에도 들어왔는가.**
//!
//! gallery-first 는 이 프로젝트의 불가침 원칙이다(ADR-0510 · CLAUDE.md "갤러리 완전성 ·
//! gallery-first" — *새 modal/popup/공용 위젯은 디자인 수령 → 갤러리 specimen → 본체 반영
//! 순서로 만든다*). 그런데 그것을 어겼을 때 빨개지는 것이 **하나도 없었다.**
//!
//! 실측(바늘 셋, 좁은 것부터): host 등록처 `all_defs()` 의 소비자를 전수로 훑어도 갤러리에
//! 닿는 것이 0 이고, 판정 코퍼스(`tests/` · `crates/**/tests/` · `scripts/` · `.github/`)에서
//! 갤러리를 언급하는 자리는 셋뿐인데 **셋 다 면제 목록**이다 — "specimen 이 **있을 때**
//! 그것을 규율한다" 이지 "specimen 이 **있는가**" 를 묻지 않는다. 갤러리 자신의
//! `specimen_smoke.rs` 는 81 개 모듈 중 넷만 이름으로 부르고, 카탈로그 트리
//! (`catalog.rs::pages()`)는 손으로 쓴 목록이다.
//!
//! 그래서 새 popup 이 본체에만 들어가면 **컴파일은 따라오고**(`cargo check --workspace` ·
//! `clippy --workspace --all-targets` 가 갤러리 크레이트를 포함한다) **판정은 안 따라온다.**
//! 이 파일이 그 자리를 메운다.
//!
//! # 왜 명부인가 — 자동 조인이 없다
//!
//! `PopupDef.id` 는 런타임 식별자지 갤러리와 짝을 맞추려고 지은 이름이 아니다. 실제로
//! `"notifications"` 의 specimen 은 `notification_panel.rs` 이고 `"convert_surface"` 는
//! `convert.rs` 다 — 규칙으로 유도되지 않는다. 이름 규칙을 지어내면 그 규칙이 곧 새로운
//! 사본이 되므로, 잇는 것을 **자리마다 적는다.** 새 popup 이 등록처에 들어오면 이 명부에
//! 행이 없어 그 자리에서 실패한다. 그것이 이 가드의 전부다.
//!
//! # 이 가드가 답하지 못하는 것 (산문으로 두지 않고 여기 박는다)
//!
//! - **공용 위젯**(`crates/tasty-ui-widgets`)에는 등록처가 없다. popup 은 `all_defs()`,
//!   무대는 `all_metas()` 라는 정적 표가 있어서 왼쪽을 셀 수 있지만, 위젯은 `pub` 타입이
//!   흩어져 있어 "무엇이 다 있나" 를 물을 자리가 아직 없다. 그래서 **새 공용 위젯이
//!   갤러리에 안 들어가는 것은 여전히 아무것도 안 본다.** 덮인 것은 popup 과 무대뿐이다.
//! - **"카탈로그 페이지에 실제로 얹혔는가"** 는 여기서 절반만 본다. specimen 파일이
//!   실재하고 그 모듈 경로가 `catalog.rs` 에 나오는지까지가 텍스트로 물을 수 있는 끝이고,
//!   그 `Spec` 이 어느 `Page` 에 달렸는지는 트리를 세워야 알 수 있다(갤러리 크레이트에
//!   의존하지 않는 이 가드의 자리에서는 못 한다).
//!   ★ 그 절반이 **어디서 만족되는지**를 분명히 해 둔다: `use` 줄 하나만 있어도 참이 된다.
//!   즉 이 검사가 보는 것은 얹힘이 아니라 **존재**다. `pages()` 가 짧은 경로
//!   (`canvas::dense_stage`)로 specimen 을 부른다는 실측이 있어서, 경로 문자열 대조로는
//!   여기까지가 한계다.
//! - **popup 쪽은 gui 조합에서만 돈다.** `adapters::ui` 가 `#[cfg(feature = "gui")]` 라
//!   헤드리스에는 등록처 자체가 없다. 무대 쪽은 메타 표가 gui 무관이라(`fullscreen_stages`)
//!   두 조합에서 다 돈다. 자동 채널은 Windows 유닛 잡과 Linux gui 유닛 잡이다.
//!
//! # 찍는 표에는 **자동 채널이 없다**
//!
//! libtest 는 통과한 테스트의 출력을 삼키고, 이 레포의 어느 회차 스텝도
//! `--show-output`·`--nocapture` 를 안 쓴다. 그러니 아래 `println!` 은 **초록 회차 어디에도
//! 안 나온다** — 손으로 `cargo test -p tasty --lib <이 모듈> -- --nocapture` 로 볼 때만 보인다.
//! 그것을 알고 둔다: 자동으로 지키는 것은 **단정**이고, 표는 사람이 눈으로 확인할 때 쓰는
//! 도구다. 표가 채널을 가진 것처럼 쓰지 마라.

/// 갤러리 specimen 이 사는 디렉토리(레포 상대).
const CATALOG_ROOT: &str = "crates/tasty-gallery/src/catalog";

/// 카탈로그 트리 — specimen `draw` 를 페이지에 얹는 손으로 쓴 목록.
const CATALOG_TREE: &str = "crates/tasty-gallery/src/catalog.rs";

/// host popup 등록처(`adapters::ui::popup::defs::all_defs()`)의 id ↔ 갤러리 specimen.
///
/// 여러 id 가 한 specimen 을 가리키는 것은 정상이다 — 전송 진행/실패는 한 프레임의 두
/// 상태이고, 프리셋 적용 셋은 범위만 다르다. 갤러리가 그것을 한 카드에서 보여준다.
#[cfg(feature = "gui")]
const POPUP_SPECIMENS: &[(&str, &str)] = &[
    ("notifications", "components/notification_panel.rs"),
    ("convert_surface", "components/convert.rs"),
    ("script_changed_confirm", "components/script_confirm.rs"),
    ("rename", "components/rename_popup.rs"),
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
];

/// specimen 이 **없는** popup 과 그 사유.
///
/// 이것은 면제가 아니라 **원칙을 어긴 자리의 목록**이다. 비어 있는 것이 좋은 상태이고,
/// 지금 비어 있다 — 24 자리 전부 specimen 이 있다. 행이 생기면 그 행이 곧 빚이다.
#[cfg(feature = "gui")]
const POPUP_WITHOUT_SPECIMEN: &[(&str, &str)] = &[];

/// 전체화면 무대 메타 표(`fullscreen_stages::all_metas()`) ↔ 갤러리 specimen.
///
/// `blank` 은 콘텐츠가 없는 무대다(`draw_blank_stage`) — 즉 **무대 셸 그 자체**이고,
/// 갤러리의 `fullscreen_stage::draw` 가 보여주는 것이 정확히 그 셸이다. 그래서 두 무대가
/// 한 specimen 을 가리킨다.
const STAGE_SPECIMENS: &[(&str, &str)] = &[
    ("blank", "components/fullscreen_stage.rs"),
    ("notifications", "components/fullscreen_stage.rs"),
];

/// 무대 쪽 빚 목록. popup 쪽과 같은 뜻이다.
const STAGE_WITHOUT_SPECIMEN: &[(&str, &str)] = &[];

/// 등록처 한 벌을 명부와 맞춘다.
///
/// `floor` 와 `control` 이 **추출이 죽었을 때 조용해지는 것**을 막는다 — 등록처가 비면
/// "모든 id 에 specimen 이 있다" 가 공허하게 참이 되고, 그 0 은 초록보다 조용하다.
fn check(
    what: &str,
    registry: &[&str],
    roster: &[(&str, &str)],
    debts: &[(&str, &str)],
    debt_budget: usize,
    floor: usize,
    control: &str,
) {
    // 빚은 **수로도** 못 박는다. 실패문이 "정 안 되면 빚 목록에 적어라" 라고 말하는 한,
    // 그 목록은 한 줄짜리 도망길이 된다 — 처방을 최소로 이행하면 보호가 사라지는 형태다.
    // 수를 함께 올려야 하면 그 도망이 diff 에 남고, 줄이는 쪽으로도 실패해 빚을 갚은 뒤
    // 장부를 안 닫는 것도 막는다.
    assert_eq!(
        debts.len(),
        debt_budget,
        "{what} 의 specimen 없는 자리가 {} 개다(기록 {debt_budget}). 늘었으면 원칙을 어긴 \
         자리가 는 것이고, 줄었으면 갚은 만큼 이 수를 내려라",
        debts.len()
    );
    assert!(
        registry.len() >= floor,
        "{what} 등록처가 {} 개다(하한 {floor}) — 표를 못 읽었으면 아래 판정은 전부 공허하다",
        registry.len()
    );
    assert!(
        registry.contains(&control),
        "{what} 등록처에 대조 항목 `{control}` 이 없다 — 표를 읽은 것이 맞는지부터 의심하라"
    );

    let root = super::repo_root();
    let tree = std::fs::read_to_string(root.join(CATALOG_TREE)).unwrap_or_default();
    assert!(
        tree.len() > 1000,
        "카탈로그 트리(`{CATALOG_TREE}`)를 못 읽었다 — 아래 '페이지에 얹혔나' 판정이 전부 거짓이 된다"
    );

    let mut listed = Vec::new();
    for id in registry {
        if let Some((_, why)) = debts.iter().find(|(k, _)| k == id) {
            listed.push(format!("  {id:<26} specimen 없음 — {why}"));
            continue;
        }
        let Some((_, rel)) = roster.iter().find(|(k, _)| k == id) else {
            panic!(
                "{what} `{id}` 에 대응하는 갤러리 specimen 이 명부에 없다.\n\
                 gallery-first(ADR-0510)는 본체보다 갤러리가 먼저다 — specimen 을 만들고 \
                 `{}` 의 명부에 그 자리를 적어라.\n\
                 빚 목록에 옮기는 것은 **이행이 아니다.** 그 목록은 면제가 아니라 \
                 원칙을 어긴 자리의 장부이고, 행을 더하려면 그 옆의 빚 수도 함께 올려야 \
                 한다 — 늘리는 쪽으로도 실패한다.",
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
            "{what} `{id}` 의 specimen `{rel}` 이 카탈로그 트리에 안 얹혀 있다 \
             (`{module}` 을 `{CATALOG_TREE}` 에서 못 찾았다) — 컴파일만 되고 아무 페이지에도 \
             안 나오는 specimen 은 없는 것과 같다"
        );
        listed.push(format!("  {id:<26} → {rel}"));
    }

    for (id, _) in roster.iter().chain(debts.iter()) {
        assert!(
            registry.contains(id),
            "{what} 명부에 등록처에 없는 행 `{id}` 이 있다 — popup 이 지워졌으면 명부에서도 지워라"
        );
    }

    // 양성 대조를 단정으로만 두면 **초록일 때 무엇을 집었는지** 안 보인다.
    // `-- --nocapture` 로 목록을 눈으로 확인할 수 있게 찍는다. 수가 아니라 목록이다.
    println!(
        "[{what}] 등록처 {} · 명부 {} · 빚 {}\n{}",
        registry.len(),
        roster.len(),
        debts.len(),
        listed.join("\n")
    );
}

/// popup 은 gui 조합에만 존재한다 — `adapters::ui` 가 그 feature 뒤다.
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

/// 무대 메타는 gui 무관이라(`fullscreen_stages`) 헤드리스에서도 돈다.
///
/// `all_metas()` 는 테스트 빌드에서 무대 교체 계약을 걷기 위한 가짜 둘을 덧붙인다.
/// 그것들은 출하물이 아니므로 접두사로 뺀다 — 안 빼면 이 가드가 존재하지 않는 무대의
/// specimen 을 요구한다.
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
