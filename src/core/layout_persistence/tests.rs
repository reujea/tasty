//! 저장 형식 호환성, 슬롯 저장·백업과 scrollback 참조를 검사한다.

use serde_json::json;

use super::schema::SavedSurface;

fn parse(s: &str) -> SavedSurface {
    serde_json::from_str(s).unwrap_or_else(|e| panic!("parse failed for {s:?}: {e}"))
}

#[test]
fn v1_markdown_now_fails_to_parse() {
    let result: Result<SavedSurface, _> =
        serde_json::from_str(r#"{"Markdown":{"path":"/tmp/x.md"}}"#);
    assert!(result.is_err());
}

#[test]
fn v2_terminal_round_trips() {
    let s = SavedSurface::Terminal {
        cwd: Some("/tmp".into()),
        restore_command: None,
        scrollback_ref: None,
    };
    let json = serde_json::to_string(&s).unwrap();
    match parse(&json) {
        SavedSurface::Terminal {
            cwd,
            restore_command,
            scrollback_ref,
        } => {
            assert_eq!(cwd.as_deref(), Some("/tmp"));
            assert!(restore_command.is_none());
            assert!(scrollback_ref.is_none());
        }
        _ => panic!("expected Terminal"),
    }
}

#[test]
fn legacy_terminal_without_scrollback_ref_parses() {
    // scrollback_ref가 없는 이전 Terminal 형식도 읽어야 한다.
    let json = r#"{"Terminal":{"cwd":"/home","restore_command":null}}"#;
    match parse(json) {
        SavedSurface::Terminal {
            cwd,
            restore_command,
            scrollback_ref,
        } => {
            assert_eq!(cwd.as_deref(), Some("/home"));
            assert!(restore_command.is_none());
            assert!(scrollback_ref.is_none());
        }
        _ => panic!("expected Terminal"),
    }
}

#[test]
fn v2_terminal_with_scrollback_ref_round_trips() {
    let s = SavedSurface::Terminal {
        cwd: None,
        restore_command: None,
        scrollback_ref: Some("deadbeef".into()),
    };
    let json = serde_json::to_string(&s).unwrap();
    assert!(json.contains("scrollback_ref"));
    match parse(&json) {
        SavedSurface::Terminal { scrollback_ref, .. } => {
            assert_eq!(scrollback_ref.as_deref(), Some("deadbeef"));
        }
        _ => panic!("expected Terminal"),
    }
}

#[test]
fn v2_generic_round_trips() {
    let s = SavedSurface::Generic {
        kind: "markdown".into(),
        data: json!({"path": "/x.md"}),
    };
    let json = serde_json::to_string(&s).unwrap();
    match parse(&json) {
        SavedSurface::Generic { kind, data } => {
            assert_eq!(kind, "markdown");
            assert_eq!(data["path"], "/x.md");
        }
        _ => panic!("expected Generic"),
    }
}

#[test]
fn unknown_variant_is_rejected() {
    let result: Result<SavedSurface, _> = serde_json::from_str(r#"{"Bogus":{}}"#);
    assert!(result.is_err());
}

#[test]
fn saved_workspace_old_attach_mapping_is_read_and_not_written_back() {
    use super::schema::SavedWorkspace;
    // 원격 attach 매핑이 있던 옛 저장 형식. 값은 읽기만 하고 다시 쓰지 않는다.
    let legacy = r#"{
        "name": "ws",
        "subtitle": "",
        "description": "",
        "pane_layout": { "Leaf": { "tabs": [
            { "name": "Shell", "explicit_name": null,
              "surface": { "Leaf": { "Terminal": {} } } }
        ], "active_tab": 0 } },
        "focused_pane_index": 0,
        "attach_mapping": { "target": { "Profile": { "name": "gx10" } }, "remote_workspace": 1 }
    }"#;
    let ws: SavedWorkspace = serde_json::from_str(legacy).unwrap();
    let json = serde_json::to_string(&ws).unwrap();
    assert!(!json.contains("attach_mapping"), "{json}");
}

#[test]
fn saved_workspace_without_category_field_defaults_to_normal() {
    use super::schema::SavedWorkspace;
    use crate::model::NORMAL_CATEGORY_ID;
    // category가 없는 이전 저장 형식.
    let legacy = r#"{
        "name": "ws",
        "subtitle": "",
        "description": "",
        "pane_layout": { "Leaf": { "tabs": [
            { "name": "Shell", "explicit_name": null,
              "surface": { "Leaf": { "Terminal": {} } } }
        ], "active_tab": 0 } },
        "focused_pane_index": 0
    }"#;
    let ws: SavedWorkspace = serde_json::from_str(legacy).unwrap();
    assert_eq!(ws.category, NORMAL_CATEGORY_ID);
}

#[test]
fn saved_layout_categories_round_trip() {
    use super::schema::{
        SavedCategory, SavedLayout, SavedPane, SavedPaneNode, SavedSurfaceLayout, SavedTab,
        SavedWorkspace,
    };
    let leaf = SavedSurfaceLayout::Leaf(SavedSurface::Terminal {
        cwd: None,
        restore_command: None,
        scrollback_ref: None,
    });
    let layout = SavedLayout {
        version: super::LAYOUT_VERSION,
        active_workspace: 0,
        categories: vec![
            SavedCategory {
                id: 0,
                name: "normal".into(),
                collapsed: false,
            },
            SavedCategory {
                id: 1,
                name: "work".into(),
                collapsed: true,
            },
        ],
        workspaces: vec![SavedWorkspace {
            name: "ws".into(),
            subtitle: String::new(),
            description: String::new(),
            pane_layout: SavedPaneNode::Leaf(SavedPane {
                tabs: vec![SavedTab {
                    name: "Shell".into(),
                    explicit_name: None,
                    surface: leaf,
                }],
                active_tab: 0,
            }),
            focused_pane_index: 0,
            category: 1,
        }],
    };
    let json = serde_json::to_string(&layout).unwrap();
    let back: SavedLayout = serde_json::from_str(&json).unwrap();
    assert_eq!(back.categories.len(), 2);
    assert_eq!(back.categories[1].name, "work");
    assert!(back.categories[1].collapsed);
    assert_eq!(
        back.workspaces
            .first()
            .expect("workspace index is valid")
            .category,
        1
    );
}

#[test]
fn saved_layout_without_categories_field_is_empty() {
    use super::schema::SavedLayout;
    // categories가 없으면 역직렬화 결과는 빈 목록이다. 기본 분류 생성은 별도의 복원 처리다.
    let legacy = r#"{
        "version": 2,
        "active_workspace": 0,
        "workspaces": []
    }"#;
    let layout: SavedLayout = serde_json::from_str(legacy).unwrap();
    assert!(layout.categories.is_empty());
}

// 실제 홈 대신 디렉터리를 받는 저장 함수를 사용한다.

mod slots {
    use std::path::Path;

    use super::super::schema::{
        SavedLayout, SavedPane, SavedPaneNode, SavedSurface, SavedSurfaceLayout, SavedTab,
        SavedWorkspace,
    };
    use super::super::{
        LAYOUT_VERSION, LegacyMigration, SlotLoad, collect_slots, delete_slot_in,
        gc_scrollback_orphans_all_slots_in, list_slots_in, load_slot_in, migrate_legacy_in,
        migrate_legacy_with, preserve_unparsable_slot, slot_path_in,
    };

    fn expect_loaded(dir: &Path, slot: u32) -> SavedLayout {
        match load_slot_in(dir, slot) {
            SlotLoad::Loaded(layout) => layout,
            SlotLoad::Absent => panic!("슬롯 {slot} 이 없다"),
            SlotLoad::Unreadable => panic!("슬롯 {slot} 을 읽지 못했다"),
            SlotLoad::Unparsable => panic!("슬롯 {slot} 을 해석하지 못했다"),
        }
    }

    fn layout_with(ws_name: &str, scrollback_ref: Option<&str>) -> SavedLayout {
        SavedLayout {
            version: LAYOUT_VERSION,
            active_workspace: 0,
            categories: Vec::new(),
            workspaces: vec![SavedWorkspace {
                name: ws_name.into(),
                subtitle: String::new(),
                description: String::new(),
                pane_layout: SavedPaneNode::Leaf(SavedPane {
                    tabs: vec![SavedTab {
                        name: "Shell".into(),
                        explicit_name: None,
                        surface: SavedSurfaceLayout::Leaf(SavedSurface::Terminal {
                            cwd: None,
                            restore_command: None,
                            scrollback_ref: scrollback_ref.map(str::to_string),
                        }),
                    }],
                    active_tab: 0,
                }),
                focused_pane_index: 0,
                category: 0,
            }],
        }
    }

    fn write_slot(dir: &Path, slot: u32, layout: &SavedLayout) {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(
            slot_path_in(dir, slot),
            serde_json::to_vec_pretty(layout).unwrap(),
        )
        .unwrap();
    }

    fn touch_bin(dir: &Path, id: &str) {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(dir.join(format!("{id}.bin")), b"x").unwrap();
    }

    fn bin_exists(dir: &Path, id: &str) -> bool {
        dir.join(format!("{id}.bin")).exists()
    }

    #[test]
    fn list_slots_sorts_numerically_and_skips_junk() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        for name in ["01.json", "02.json", "10.json", "notes.txt", "01.json.tmp"] {
            std::fs::write(dir.join(name), "{}").unwrap();
        }
        assert_eq!(list_slots_in(dir).slots, vec![1, 2, 10]);
    }

    #[test]
    fn list_slots_folds_padded_and_unpadded_names_into_one() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        for name in ["1.json", "01.json"] {
            std::fs::write(dir.join(name), "{}").unwrap();
        }
        assert_eq!(list_slots_in(dir).slots, vec![1]);
    }

    #[test]
    fn an_unreadable_entry_marks_the_slot_list_incomplete() {
        let dir = Path::new("layouts");
        let entries = vec![
            Ok(dir.join("02.json")),
            Err(std::io::Error::other("injected entry failure")),
            Ok(dir.join("01.json")),
        ];
        let listed = collect_slots(dir, entries.into_iter());
        assert_eq!(listed.slots, vec![1, 2], "읽은 항목은 그대로 모은다");
        assert!(!listed.complete, "빠진 항목이 있을 수 있으면 불완전이다");

        let listed = collect_slots(dir, vec![Ok(dir.join("01.json"))].into_iter());
        assert!(listed.complete);
    }

    #[test]
    fn list_slots_is_empty_when_the_dir_does_not_exist() {
        let tmp = tempfile::tempdir().unwrap();
        let listed = list_slots_in(&tmp.path().join("layouts"));
        assert!(listed.slots.is_empty());
        assert!(listed.complete, "디렉터리가 없으면 슬롯이 없다고 확정한다");
    }

    #[test]
    fn migrate_legacy_moves_layout_json_to_slot_one() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path();
        let legacy = home.join("layout.json");
        std::fs::write(
            &legacy,
            serde_json::to_string_pretty(&layout_with("legacy-ws", None)).unwrap(),
        )
        .unwrap();

        migrate_legacy_in(home);

        let layouts = home.join("layouts");
        assert!(!legacy.exists(), "레거시 파일은 복사가 아니라 이동이다");
        assert!(slot_path_in(&layouts, 1).exists());
        let restored = expect_loaded(&layouts, 1);
        assert_eq!(
            restored
                .workspaces
                .first()
                .expect("workspace index is valid")
                .name,
            "legacy-ws"
        );

        migrate_legacy_in(home);
        assert!(matches!(load_slot_in(&layouts, 1), SlotLoad::Loaded(_)));
    }

    #[test]
    fn migrate_legacy_is_noop_when_slot_files_already_exist() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path();
        let legacy = home.join("layout.json");
        std::fs::write(
            &legacy,
            serde_json::to_string_pretty(&layout_with("legacy-ws", None)).unwrap(),
        )
        .unwrap();
        let layouts = home.join("layouts");
        write_slot(&layouts, 2, &layout_with("slot-ws", None));

        migrate_legacy_in(home);

        assert!(legacy.exists(), "이미 슬롯을 쓰는 인스턴스면 손대지 않는다");
        assert!(!slot_path_in(&layouts, 1).exists());
    }

    fn write_legacy(home: &Path, scrollback_ref: Option<&str>) -> std::path::PathBuf {
        let legacy = home.join("layout.json");
        std::fs::write(
            &legacy,
            serde_json::to_string_pretty(&layout_with("legacy-ws", scrollback_ref)).unwrap(),
        )
        .unwrap();
        legacy
    }

    /// 디렉터리만 만들고 rename은 실패하는 이관을 흉내 낸다.
    fn fail_after_creating_dir(_: &Path, dir: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(dir)?;
        Err(std::io::Error::other("injected rename failure"))
    }

    #[test]
    fn failed_migration_keeps_legacy_scrollback_on_this_and_the_next_boot() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path();
        let legacy = write_legacy(home, Some("aaa"));
        let layouts = home.join("layouts");
        let scrollback = home.join("scrollback");
        for id in ["aaa", "zzz"] {
            touch_bin(&scrollback, id);
        }

        // 첫 부팅: create_dir_all 뒤 rename만 실패한다.
        assert_eq!(
            migrate_legacy_with(home, fail_after_creating_dir),
            LegacyMigration::Failed
        );
        assert!(layouts.is_dir(), "실패 분기는 빈 layouts 디렉터리를 남긴다");
        assert!(legacy.exists());
        gc_scrollback_orphans_all_slots_in(&layouts, &legacy, &scrollback);
        assert!(
            bin_exists(&scrollback, "aaa"),
            "이관하지 못한 layout.json이 참조하는 scrollback은 남아야 한다"
        );
        assert!(!bin_exists(&scrollback, "zzz"), "참조 없는 파일은 지운다");

        // 다음 부팅도 같은 실패: 빈 layouts와 남은 layout.json.
        assert_eq!(
            migrate_legacy_with(home, fail_after_creating_dir),
            LegacyMigration::Failed,
            "빈 layouts 디렉터리면 이관을 다시 시도한다"
        );
        gc_scrollback_orphans_all_slots_in(&layouts, &legacy, &scrollback);
        assert!(bin_exists(&scrollback, "aaa"));

        // 그다음 부팅에서 rename이 성공하면 슬롯 1로 옮겨지고 참조는 슬롯에서 보호된다.
        migrate_legacy_in(home);
        assert!(!legacy.exists());
        assert!(matches!(load_slot_in(&layouts, 1), SlotLoad::Loaded(_)));
        gc_scrollback_orphans_all_slots_in(&layouts, &legacy, &scrollback);
        assert!(bin_exists(&scrollback, "aaa"));
    }

    #[test]
    fn legacy_left_beside_slots_still_protects_its_scrollback() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path();
        let legacy = write_legacy(home, Some("aaa"));
        let layouts = home.join("layouts");
        let scrollback = home.join("scrollback");
        write_slot(&layouts, 2, &layout_with("w2", Some("bbb")));
        for id in ["aaa", "bbb", "zzz"] {
            touch_bin(&scrollback, id);
        }

        migrate_legacy_in(home);
        assert!(legacy.exists());
        gc_scrollback_orphans_all_slots_in(&layouts, &legacy, &scrollback);

        assert!(bin_exists(&scrollback, "aaa"));
        assert!(bin_exists(&scrollback, "bbb"));
        assert!(!bin_exists(&scrollback, "zzz"));
    }

    #[test]
    fn gc_skips_when_the_legacy_layout_cannot_be_parsed() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path();
        let legacy = home.join("layout.json");
        std::fs::write(&legacy, "{ truncated").unwrap();
        let scrollback = home.join("scrollback");
        touch_bin(&scrollback, "aaa");

        gc_scrollback_orphans_all_slots_in(&home.join("layouts"), &legacy, &scrollback);

        assert!(bin_exists(&scrollback, "aaa"));
    }

    /// 권한 제거가 통하지 않는 환경(root 등)이면 시험 전제가 성립하지 않으므로 그 사실을 알린다.
    #[cfg(unix)]
    #[test]
    fn gc_skips_when_the_layouts_dir_cannot_be_listed() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = tempfile::tempdir().unwrap();
        let layouts = tmp.path().join("layouts");
        let scrollback = tmp.path().join("scrollback");
        write_slot(&layouts, 1, &layout_with("w1", Some("aaa")));
        for id in ["aaa", "zzz"] {
            touch_bin(&scrollback, id);
        }
        std::fs::set_permissions(&layouts, std::fs::Permissions::from_mode(0o000)).unwrap();
        let unreadable = std::fs::read_dir(&layouts).is_err();

        let listed = list_slots_in(&layouts);
        gc_scrollback_orphans_all_slots_in(&layouts, &tmp.path().join("layout.json"), &scrollback);
        std::fs::set_permissions(&layouts, std::fs::Permissions::from_mode(0o755)).unwrap();

        assert!(
            unreadable,
            "권한을 없애도 layouts를 읽을 수 있는 환경이라 이 시험의 전제가 성립하지 않는다"
        );
        assert!(!listed.complete);
        assert!(bin_exists(&scrollback, "aaa"));
        assert!(
            bin_exists(&scrollback, "zzz"),
            "목록을 확정하지 못하면 아무것도 지우지 않는다"
        );
    }

    #[test]
    fn delete_slot_removes_the_file_and_is_idempotent() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("layouts");
        write_slot(&dir, 2, &layout_with("a", None));
        delete_slot_in(&dir, 2);
        assert!(matches!(load_slot_in(&dir, 2), SlotLoad::Absent));
        delete_slot_in(&dir, 2); // 없는 파일은 no-op
    }

    #[test]
    fn gc_union_keeps_refs_from_every_slot() {
        let tmp = tempfile::tempdir().unwrap();
        let layouts = tmp.path().join("layouts");
        let scrollback = tmp.path().join("scrollback");
        write_slot(&layouts, 1, &layout_with("w1", Some("aaa")));
        write_slot(&layouts, 2, &layout_with("w2", Some("bbb")));
        for id in ["aaa", "bbb", "ccc"] {
            touch_bin(&scrollback, id);
        }

        gc_scrollback_orphans_all_slots_in(&layouts, &tmp.path().join("layout.json"), &scrollback);

        assert!(bin_exists(&scrollback, "aaa"));
        assert!(
            bin_exists(&scrollback, "bbb"),
            "다른 슬롯이 참조하는 scrollback도 보존해야 한다"
        );
        assert!(
            !bin_exists(&scrollback, "ccc"),
            "어느 슬롯도 참조하지 않는다"
        );
    }

    #[test]
    fn gc_skips_entirely_when_any_slot_fails_to_parse() {
        let tmp = tempfile::tempdir().unwrap();
        let layouts = tmp.path().join("layouts");
        let scrollback = tmp.path().join("scrollback");
        write_slot(&layouts, 1, &layout_with("w1", Some("aaa")));
        std::fs::write(slot_path_in(&layouts, 2), "{ truncated").unwrap();
        for id in ["aaa", "zzz"] {
            touch_bin(&scrollback, id);
        }

        gc_scrollback_orphans_all_slots_in(&layouts, &tmp.path().join("layout.json"), &scrollback);

        assert!(bin_exists(&scrollback, "aaa"));
        assert!(
            bin_exists(&scrollback, "zzz"),
            "손상 슬롯이 있으면 아무것도 지우지 않는다"
        );
    }

    #[test]
    fn gc_with_no_slots_treats_everything_as_orphan() {
        let tmp = tempfile::tempdir().unwrap();
        let scrollback = tmp.path().join("scrollback");
        touch_bin(&scrollback, "aaa");

        gc_scrollback_orphans_all_slots_in(
            &tmp.path().join("layouts"),
            &tmp.path().join("layout.json"),
            &scrollback,
        );

        assert!(!bin_exists(&scrollback, "aaa"));
    }

    #[test]
    fn slot_newer_than_supported_version_is_unreadable_and_blocks_gc() {
        let tmp = tempfile::tempdir().unwrap();
        let layouts = tmp.path().join("layouts");
        let scrollback = tmp.path().join("scrollback");
        let mut future = layout_with("w1", None);
        future.version = LAYOUT_VERSION + 1;
        write_slot(&layouts, 1, &future);
        touch_bin(&scrollback, "aaa");

        assert!(matches!(load_slot_in(&layouts, 1), SlotLoad::Unreadable));
        gc_scrollback_orphans_all_slots_in(&layouts, &tmp.path().join("layout.json"), &scrollback);
        assert!(bin_exists(&scrollback, "aaa"));
    }

    #[test]
    fn future_version_slot_file_is_left_in_place() {
        let tmp = tempfile::tempdir().unwrap();
        let layouts = tmp.path().join("layouts");
        let mut future = layout_with("w1", None);
        future.version = LAYOUT_VERSION + 1;
        write_slot(&layouts, 1, &future);
        let before = std::fs::read_to_string(slot_path_in(&layouts, 1)).unwrap();

        assert!(matches!(load_slot_in(&layouts, 1), SlotLoad::Unreadable));

        let after = std::fs::read_to_string(slot_path_in(&layouts, 1)).unwrap();
        assert_eq!(before, after, "미래 version 파일은 그대로 있어야 한다");
        assert!(
            !slot_path_in(&layouts, 1)
                .with_extension("json.bak")
                .exists(),
            "백업을 만들지 않는다"
        );
    }

    #[test]
    fn unparsable_slot_is_left_in_place_by_load() {
        let tmp = tempfile::tempdir().unwrap();
        let layouts = tmp.path().join("layouts");
        std::fs::create_dir_all(&layouts).unwrap();
        let path = slot_path_in(&layouts, 1);
        std::fs::write(&path, "{ this is not valid json").unwrap();

        for _ in 0..2 {
            assert!(matches!(load_slot_in(&layouts, 1), SlotLoad::Unparsable));
        }

        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "{ this is not valid json",
            "읽기는 원본을 옮기지 않는다"
        );
        assert!(!layouts.join("01.json.bak").exists());
    }

    #[test]
    fn preserving_an_unparsable_slot_moves_it_aside() {
        let tmp = tempfile::tempdir().unwrap();
        let layouts = tmp.path().join("layouts");
        std::fs::create_dir_all(&layouts).unwrap();
        let path = slot_path_in(&layouts, 1);
        std::fs::write(&path, "first corrupt").unwrap();

        assert!(preserve_unparsable_slot(&layouts, 1));
        assert!(!path.exists(), "원래 경로에는 파일이 남지 않아야 한다");
        assert_eq!(
            std::fs::read_to_string(layouts.join("01.json.bak")).unwrap(),
            "first corrupt"
        );

        std::fs::write(&path, "second corrupt").unwrap();
        assert!(preserve_unparsable_slot(&layouts, 1));
        assert_eq!(
            std::fs::read_to_string(layouts.join("01.json.bak")).unwrap(),
            "first corrupt"
        );
        assert_eq!(
            std::fs::read_to_string(layouts.join("01.json.bak.2")).unwrap(),
            "second corrupt"
        );

        assert!(preserve_unparsable_slot(&layouts, 1));
    }

    #[cfg(unix)]
    #[test]
    fn unreadable_slot_is_left_in_place_and_locked() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = tempfile::tempdir().unwrap();
        let layouts = tmp.path().join("layouts");
        write_slot(&layouts, 1, &layout_with("mine", None));
        let path = slot_path_in(&layouts, 1);
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o000)).unwrap();

        let verdict = load_slot_in(&layouts, 1);

        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert!(matches!(verdict, SlotLoad::Unreadable));
        assert_eq!(
            expect_loaded(&layouts, 1)
                .workspaces
                .first()
                .expect("workspace index is valid")
                .name,
            "mine",
            "권한을 되돌리면 원본이 그대로 있어야 한다"
        );
        assert!(
            !layouts.join("01.json.bak").exists(),
            "백업을 만들지 않는다"
        );
    }
}
