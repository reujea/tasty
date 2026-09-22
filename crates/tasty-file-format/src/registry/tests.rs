//! `FileFormatRegistry` 단위 테스트 — manifest install/uninstall, extension priority,
//! user TOML round-trip, identify_by_*, plugin disable/enable 등.

use super::*;
use std::path::PathBuf;

fn target(p: &str) -> FileTarget {
    FileTarget::new(PathBuf::from(p))
}

/// host default + com.tasty.markdown plugin detector 동시 install.
/// markdown surface 가 별도 plugin 으로 분리됐기에, 기존 테스트가 가정하던
/// markdown detector 동작을 plugin install 로 흉내낸다.
fn install_host_with_markdown(reg: &FileFormatRegistry) {
    reg.install_host_defaults(crate::HOST_DEFAULTS_TOML);
    let decls = vec![DetectorDecl {
        id: "markdown".into(),
        display_name_i18n_key: Some("file_handler.format.markdown".into()),
        icon: None,
        disabled: None,
        rule: vec![DetectorRuleDecl::Extension {
            values: vec!["md".into(), "markdown".into()],
        }],
    }];
    reg.install_plugin_detectors("com.tasty.markdown", &decls);
}

#[test]
fn host_default_loads_and_identifies_markdown() {
    let reg = FileFormatRegistry::new();
    install_host_with_markdown(&reg);
    let id = reg.identify(&target("a/b.md"), DetectDepth::Cheap);
    assert_eq!(id, Some(DetectorId("markdown".into())));
    let id = reg.identify(&target("a/b.MARKDOWN"), DetectDepth::Cheap);
    assert_eq!(id, Some(DetectorId("markdown".into())));
    let id = reg.identify(&target("a/b.html"), DetectDepth::Cheap);
    assert_eq!(id, Some(DetectorId("html".into())));
    let id = reg.identify(&target("a/b.unknownext"), DetectDepth::Cheap);
    assert_eq!(id, None);
}

#[test]
fn plugin_extends_existing_detector() {
    let reg = FileFormatRegistry::new();
    install_host_with_markdown(&reg);
    // plugin 이 mdx 확장자 추가
    let decls = vec![DetectorDecl {
        id: "markdown".into(),
        display_name_i18n_key: None,
        icon: None,
        disabled: None,
        rule: vec![DetectorRuleDecl::Extension {
            values: vec!["mdx".into()],
        }],
    }];
    reg.install_plugin_detectors("com.example.mdx", &decls);
    let id = reg.identify(&target("a/b.mdx"), DetectDepth::Cheap);
    assert_eq!(id, Some(DetectorId("markdown".into())));
    // 기존 md 매칭 유지
    let id = reg.identify(&target("a/b.md"), DetectDepth::Cheap);
    assert_eq!(id, Some(DetectorId("markdown".into())));
}

#[test]
fn plugin_lua_rule_dropped_with_warn() {
    let reg = FileFormatRegistry::new();
    // plugin 이 Lua 와 Extension 을 섞어서 제공. Lua 만 drop 되고 Extension 은 유지.
    let decls = vec![DetectorDecl {
        id: "weird-fmt".into(),
        display_name_i18n_key: None,
        icon: None,
        disabled: None,
        rule: vec![
            DetectorRuleDecl::Lua {
                script: "return true".into(),
            },
            DetectorRuleDecl::Extension {
                values: vec!["wf".into()],
            },
        ],
    }];
    reg.install_plugin_detectors("com.example.weird", &decls);
    // Lua drop 후에도 Extension rule 이 살아 있어 매칭 가능.
    let id = reg.identify(&target("x.wf"), DetectDepth::Deep);
    assert_eq!(id, Some(DetectorId("weird-fmt".into())));
}

#[test]
fn plugin_lua_only_detector_skipped() {
    let reg = FileFormatRegistry::new();
    // Lua 만 들어있는 detector — install 자체가 무의미해서 skip.
    let decls = vec![DetectorDecl {
        id: "lua-only".into(),
        display_name_i18n_key: None,
        icon: None,
        disabled: None,
        rule: vec![DetectorRuleDecl::Lua {
            script: "return true".into(),
        }],
    }];
    reg.install_plugin_detectors("com.example.lua-only", &decls);
    // detector 자체가 등록되지 않으므로 어떤 파일에도 안 잡힘.
    assert_eq!(reg.identify(&target("anything"), DetectDepth::Deep), None);
}

#[test]
fn uninstall_plugin_removes_only_its_rules() {
    let reg = FileFormatRegistry::new();
    install_host_with_markdown(&reg);
    let decls = vec![DetectorDecl {
        id: "markdown".into(),
        display_name_i18n_key: None,
        icon: None,
        disabled: None,
        rule: vec![DetectorRuleDecl::Extension {
            values: vec!["mdx".into()],
        }],
    }];
    reg.install_plugin_detectors("com.example.mdx", &decls);
    assert_eq!(
        reg.identify(&target("a/b.mdx"), DetectDepth::Cheap),
        Some(DetectorId("markdown".into()))
    );
    reg.uninstall_plugin("com.example.mdx");
    // 호스트의 md 는 유지
    assert_eq!(
        reg.identify(&target("a/b.md"), DetectDepth::Cheap),
        Some(DetectorId("markdown".into()))
    );
    // plugin 의 mdx 는 사라짐
    assert_eq!(reg.identify(&target("a/b.mdx"), DetectDepth::Cheap), None);
}

#[test]
fn directory_prefilter() {
    let reg = FileFormatRegistry::new();
    install_host_with_markdown(&reg);
    // tempfile 같은 디렉토리 만들기보다 root path 사용 — 디렉토리 매칭 동작만 확인.
    let dir = std::env::temp_dir();
    let t = FileTarget::new(dir);
    assert_eq!(
        reg.identify(&t, DetectDepth::Cheap),
        Some(DetectorId("$directory".into()))
    );
    // 파일 (확장자 없는 가짜 path) → IsDirectory 매칭 제외, 다른 detector 도 안 맞아 None
    let t = target("/nonexistent/file.no-such-ext");
    assert_eq!(reg.identify(&t, DetectDepth::Cheap), None);
}

#[test]
fn identify_deep_matches_magic_when_cheap_misses() {
    let reg = FileFormatRegistry::new();
    // 호스트 default 는 사용 안 함 — 확장자가 없는 파일이 magic byte 로 매칭되는지 확인.
    // 사용자 정의 detector: extension 매칭 실패해도 magic 으로 매칭.
    let user_toml = r#"
            [[detector]]
            id = "png"
            [[detector.rule]]
            kind = "extension"
            values = ["png"]
            [[detector.rule]]
            kind = "magic"
            offset = 0
            bytes_hex = "89504E470D0A1A0A"
        "#;
    let dir = tempfile::tempdir().unwrap();
    let cfg = dir.path().join("file-handlers.toml");
    std::fs::write(&cfg, user_toml).unwrap();
    reg.install_user_config(&cfg);

    // 확장자가 .dat 인 PNG 파일.
    let png_sig = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
    let img_path = dir.path().join("masquerade.dat");
    std::fs::write(&img_path, png_sig).unwrap();
    let t = FileTarget::new(img_path);

    // Cheap → 확장자 안 맞음, magic 평가 안 함 → None
    assert_eq!(reg.identify(&t, DetectDepth::Cheap), None);
    // Deep → magic 매칭 → Some("png")
    assert_eq!(
        reg.identify(&t, DetectDepth::Deep),
        Some(DetectorId("png".into()))
    );
}

#[test]
fn reload_user_config_replaces_user_entries_keeps_host() {
    let reg = FileFormatRegistry::new();
    install_host_with_markdown(&reg);
    // 1차: 사용자가 pdf detector 추가.
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("file-handlers.toml");
    std::fs::write(
        &p,
        r#"
                [[detector]]
                id = "pdf"
                [[detector.rule]]
                kind = "extension"
                values = ["pdf"]
            "#,
    )
    .unwrap();
    reg.install_user_config(&p);
    assert_eq!(
        reg.identify(&target("a/b.pdf"), DetectDepth::Cheap),
        Some(DetectorId("pdf".into()))
    );

    // 2차: 사용자가 pdf 를 빼고 csv 추가 → reload.
    std::fs::write(
        &p,
        r#"
                [[detector]]
                id = "csv"
                [[detector.rule]]
                kind = "extension"
                values = ["csv"]
            "#,
    )
    .unwrap();
    reg.reload_user_config(&p);

    // pdf 는 host default 에 없으므로 (user 만) 사라져야 함.
    assert_eq!(reg.identify(&target("a/b.pdf"), DetectDepth::Cheap), None);
    // csv 는 새로 잡힘.
    assert_eq!(
        reg.identify(&target("a/b.csv"), DetectDepth::Cheap),
        Some(DetectorId("csv".into()))
    );
    // host default markdown 은 그대로.
    assert_eq!(
        reg.identify(&target("a/b.md"), DetectDepth::Cheap),
        Some(DetectorId("markdown".into()))
    );
}

#[test]
fn reload_user_config_missing_file_clears_user_entries() {
    let reg = FileFormatRegistry::new();
    install_host_with_markdown(&reg);
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("file-handlers.toml");
    std::fs::write(
        &p,
        r#"
                [[detector]]
                id = "pdf"
                [[detector.rule]]
                kind = "extension"
                values = ["pdf"]
            "#,
    )
    .unwrap();
    reg.install_user_config(&p);
    assert!(reg.detector(&DetectorId("pdf".into())).is_some());

    // 파일 삭제 후 reload → user origin 제거.
    std::fs::remove_file(&p).unwrap();
    reg.reload_user_config(&p);
    assert!(reg.detector(&DetectorId("pdf".into())).is_none());
    // host markdown 은 보존.
    assert!(reg.detector(&DetectorId("markdown".into())).is_some());
}

#[test]
fn reload_user_config_parse_error_keeps_previous_state() {
    let reg = FileFormatRegistry::new();
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("file-handlers.toml");
    std::fs::write(
        &p,
        r#"
                [[detector]]
                id = "pdf"
                [[detector.rule]]
                kind = "extension"
                values = ["pdf"]
            "#,
    )
    .unwrap();
    reg.install_user_config(&p);
    assert!(reg.detector(&DetectorId("pdf".into())).is_some());

    // 파일을 의도적으로 깨뜨림 → reload 는 거부, 기존 user 항목 보존.
    std::fs::write(&p, "[[detector\n id = broken").unwrap();
    reg.reload_user_config(&p);
    assert!(reg.detector(&DetectorId("pdf".into())).is_some());
}

#[test]
fn user_disabled_overrides_host() {
    let reg = FileFormatRegistry::new();
    install_host_with_markdown(&reg);
    // 사용자가 markdown detector 를 disable
    let user_toml = r#"
            [[detector]]
            id = "markdown"
            disabled = true
        "#;
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("file-handlers.toml");
    std::fs::write(&p, user_toml).unwrap();
    reg.install_user_config(&p);
    assert_eq!(reg.identify(&target("a/b.md"), DetectDepth::Cheap), None);
}

// ── export_user_config / save_user_config (MD4) ─────────────────────

#[test]
fn export_emits_user_only_origin() {
    let reg = FileFormatRegistry::new();
    install_host_with_markdown(&reg);
    // 사용자가 pdf 추가 + markdown disable.
    let user_toml = r#"
            [[detector]]
            id = "pdf"
            [[detector.rule]]
            kind = "extension"
            values = ["pdf"]

            [[detector]]
            id = "markdown"
            disabled = true
        "#;
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("file-handlers.toml");
    std::fs::write(&p, user_toml).unwrap();
    reg.install_user_config(&p);

    let exported = reg.export_user_config();
    // 호스트 detector 본문 (markdown 의 md 확장자 rule 등) 은 들어가면 안 됨.
    // 단 user 가 disable 한 markdown id 자체는 등장.
    assert!(exported.contains("pdf"), "exported = {exported}");
    assert!(exported.contains("markdown"));
    assert!(exported.contains("disabled = true"));
    // 호스트가 markdown 에 부여한 md 확장자는 user 가 만든 게 아니므로 미포함.
    // (확실히 하기 위해 user 가 등록한 pdf 의 'pdf' 확장자는 있어야).
    assert!(exported.contains("\"pdf\""));
}

#[test]
fn export_round_trip_preserves_user_state() {
    let reg = FileFormatRegistry::new();
    install_host_with_markdown(&reg);
    let user_toml = r#"
            [[detector]]
            id = "pdf"
            display_name_i18n_key = "file_format.pdf"
            icon = "file-pdf"
            [[detector.rule]]
            kind = "extension"
            values = ["pdf"]
            [[detector.rule]]
            kind = "magic"
            offset = 0
            bytes_hex = "255044462D"

            [[detector]]
            id = "markdown"
            disabled = true
        "#;
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("file-handlers.toml");
    std::fs::write(&p, user_toml).unwrap();
    reg.install_user_config(&p);

    let exported = reg.export_user_config();

    // 두 번째 registry 에 export 결과만 user origin 으로 로드.
    let reg2 = FileFormatRegistry::new();
    install_host_with_markdown(&reg2);
    let p2 = dir.path().join("export.toml");
    std::fs::write(&p2, &exported).unwrap();
    reg2.install_user_config(&p2);

    // identify 결과가 동일해야 함.
    // pdf 매칭 (extension)
    assert_eq!(
        reg.identify(&target("a/b.pdf"), DetectDepth::Cheap),
        reg2.identify(&target("a/b.pdf"), DetectDepth::Cheap),
    );
    // markdown 은 disabled — 둘 다 None
    assert_eq!(
        reg.identify(&target("a/b.md"), DetectDepth::Cheap),
        reg2.identify(&target("a/b.md"), DetectDepth::Cheap),
    );

    // 메타도 보존 — display_name / icon
    let pdf = reg2.detector(&DetectorId("pdf".into())).unwrap();
    assert_eq!(
        pdf.display_name_i18n_key.as_deref(),
        Some("file_format.pdf")
    );
    assert_eq!(pdf.icon.as_deref(), Some("file-pdf"));
}

#[test]
fn export_preserves_unknown_rule_payload() {
    let reg = FileFormatRegistry::new();
    // forward-compat: 미지의 kind 도 round-trip 보존.
    let user_toml = r#"
            [[detector]]
            id = "futureproof"
            [[detector.rule]]
            kind = "ai_classify"
            model = "v2"
            confidence = 0.8
        "#;
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("file-handlers.toml");
    std::fs::write(&p, user_toml).unwrap();
    reg.install_user_config(&p);

    let exported = reg.export_user_config();
    assert!(exported.contains("ai_classify"));
    assert!(exported.contains("model"));
    assert!(exported.contains("\"v2\""));
    assert!(exported.contains("confidence"));
}

#[test]
fn save_user_config_atomic_write() {
    let reg = FileFormatRegistry::new();
    let user_toml = r#"
            [[detector]]
            id = "pdf"
            [[detector.rule]]
            kind = "extension"
            values = ["pdf"]
        "#;
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("src.toml");
    std::fs::write(&src, user_toml).unwrap();
    reg.install_user_config(&src);

    let dst = dir.path().join("subdir").join("dst.toml");
    reg.save_user_config(&dst).unwrap();
    assert!(dst.exists());
    let written = std::fs::read_to_string(&dst).unwrap();
    assert!(written.contains("pdf"));
    assert!(written.contains("\"pdf\""));
}

#[test]
fn export_empty_when_no_user_contributions() {
    let reg = FileFormatRegistry::new();
    install_host_with_markdown(&reg);
    assert_eq!(reg.export_user_config(), "");
}

// ── DetectorInfo trait (Phase E ME1) ───────────────────────────────

#[test]
fn advertised_extensions_returns_only_extension_rule_values() {
    let reg = FileFormatRegistry::new();
    // 같은 detector 가 extension + magic 둘 다 가짐. trait 은 extension 만 반환.
    let user_toml = r#"
            [[detector]]
            id = "png"
            [[detector.rule]]
            kind = "extension"
            values = ["png", "PNG"]
            [[detector.rule]]
            kind = "magic"
            offset = 0
            bytes_hex = "89504E47"
        "#;
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("u.toml");
    std::fs::write(&p, user_toml).unwrap();
    reg.install_user_config(&p);

    let exts = reg.advertised_extensions(&DetectorId("png".into()));
    // values 는 소문자 정규화됨 → 둘 다 "png" → dedup 결과 1개.
    assert_eq!(exts, vec!["png".to_string()]);

    // 없는 detector 는 빈 벡터.
    assert!(
        reg.advertised_extensions(&DetectorId("nope".into()))
            .is_empty()
    );
}

#[test]
fn detectors_for_extension_orders_by_install_order() {
    let reg = FileFormatRegistry::new();
    // 1번째 install: "zzz" id (알파벳 후순) 이 먼저 들어옴 → install_order=0.
    let user_toml_a = r#"
            [[detector]]
            id = "zzz"
            [[detector.rule]]
            kind = "extension"
            values = ["md"]
        "#;
    let dir = tempfile::tempdir().unwrap();
    let p1 = dir.path().join("a.toml");
    std::fs::write(&p1, user_toml_a).unwrap();
    reg.install_user_config(&p1);

    // 2번째 install (다른 origin — plugin): "aaa" id 가 같은 .md 광고. install_order=1.
    let decls = vec![DetectorDecl {
        id: "aaa".into(),
        display_name_i18n_key: None,
        icon: None,
        disabled: None,
        rule: vec![DetectorRuleDecl::Extension {
            values: vec!["md".into()],
        }],
    }];
    reg.install_plugin_detectors("com.example.aaa", &decls);

    let hits = reg.detectors_for_extension("md");
    // install_order 가 작은 zzz 가 먼저, 그 다음 aaa. (알파벳 정렬이 아님)
    assert_eq!(
        hits,
        vec![DetectorId("zzz".into()), DetectorId("aaa".into())]
    );
}

#[test]
fn detectors_for_extension_skips_disabled() {
    let reg = FileFormatRegistry::new();
    let user_toml = r#"
            [[detector]]
            id = "x"
            [[detector.rule]]
            kind = "extension"
            values = ["md"]

            [[detector]]
            id = "y"
            disabled = true
            [[detector.rule]]
            kind = "extension"
            values = ["md"]
        "#;
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("u.toml");
    std::fs::write(&p, user_toml).unwrap();
    reg.install_user_config(&p);

    let hits = reg.detectors_for_extension("md");
    assert_eq!(hits, vec![DetectorId("x".into())]);
}

#[test]
fn detectors_for_extension_accepts_leading_dot_and_uppercase() {
    let reg = FileFormatRegistry::new();
    let user_toml = r#"
            [[detector]]
            id = "x"
            [[detector.rule]]
            kind = "extension"
            values = ["md"]
        "#;
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("u.toml");
    std::fs::write(&p, user_toml).unwrap();
    reg.install_user_config(&p);

    // 점 prefix / 대문자 입력 모두 정규화 매칭.
    assert_eq!(
        reg.detectors_for_extension(".md"),
        vec![DetectorId("x".into())]
    );
    assert_eq!(
        reg.detectors_for_extension("MD"),
        vec![DetectorId("x".into())]
    );
    // 빈 문자열 / 점만 → 빈 결과.
    assert!(reg.detectors_for_extension("").is_empty());
    assert!(reg.detectors_for_extension(".").is_empty());
}

#[test]
fn all_advertised_extensions_dedupes_and_sorts() {
    let reg = FileFormatRegistry::new();
    let user_toml = r#"
            [[detector]]
            id = "a"
            [[detector.rule]]
            kind = "extension"
            values = ["md", "markdown"]

            [[detector]]
            id = "b"
            [[detector.rule]]
            kind = "extension"
            values = ["mdx", "md"]
        "#;
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("u.toml");
    std::fs::write(&p, user_toml).unwrap();
    reg.install_user_config(&p);

    let exts = reg.all_advertised_extensions();
    // 알파벳 정렬, dedup.
    assert_eq!(
        exts,
        vec!["markdown".to_string(), "md".to_string(), "mdx".to_string()],
    );
}

#[test]
fn is_enabled_reflects_disabled_field() {
    let reg = FileFormatRegistry::new();
    install_host_with_markdown(&reg);
    // host 의 markdown 은 enabled.
    assert!(reg.is_enabled(&DetectorId("markdown".into())));
    // 존재하지 않는 detector 는 false.
    assert!(!reg.is_enabled(&DetectorId("nope".into())));

    // user 가 disable 하면 false.
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("u.toml");
    std::fs::write(
        &p,
        r#"
                [[detector]]
                id = "markdown"
                disabled = true
            "#,
    )
    .unwrap();
    reg.install_user_config(&p);
    assert!(!reg.is_enabled(&DetectorId("markdown".into())));
}

// ── ExtensionPriority parse/export (Phase E ME2) ───────────────────

#[test]
fn extension_priority_user_config_parsed_and_queryable() {
    let reg = FileFormatRegistry::new();
    let user_toml = r#"
            [[detector]]
            id = "x"
            [[detector.rule]]
            kind = "extension"
            values = ["md"]

            [[detector]]
            id = "y"
            [[detector.rule]]
            kind = "extension"
            values = ["md"]

            [[extension_priority]]
            extension = "md"
            order = ["y", "x"]
        "#;
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("u.toml");
    std::fs::write(&p, user_toml).unwrap();
    reg.install_user_config(&p);

    let order = reg.extension_priority_order("md").expect("present");
    assert_eq!(order, vec![DetectorId("y".into()), DetectorId("x".into())]);
    // 점 prefix, 대문자 정규화도 동일 결과.
    assert_eq!(reg.extension_priority_order(".MD"), Some(order));
    // 미정의 확장자는 None.
    assert!(reg.extension_priority_order("zzz").is_none());
}

#[test]
fn extension_priority_user_overrides_host() {
    let reg = FileFormatRegistry::new();
    // host default 가 .md 에 ["host-md"] 우선순위 적용.
    let host_toml = r#"
            [[detector]]
            id = "host-md"
            [[detector.rule]]
            kind = "extension"
            values = ["md"]

            [[extension_priority]]
            extension = "md"
            order = ["host-md"]
        "#;
    reg.install_host_defaults(host_toml);
    assert_eq!(
        reg.extension_priority_order("md"),
        Some(vec![DetectorId("host-md".into())])
    );

    // user 가 같은 키 덮어쓰기 — last-writer-wins.
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("u.toml");
    std::fs::write(
        &p,
        r#"
                [[extension_priority]]
                extension = "md"
                order = ["user-md"]
            "#,
    )
    .unwrap();
    reg.install_user_config(&p);
    assert_eq!(
        reg.extension_priority_order("md"),
        Some(vec![DetectorId("user-md".into())])
    );
}

#[test]
fn extension_priority_empty_order_removes_entry() {
    let reg = FileFormatRegistry::new();
    // host 가 priority 설치.
    reg.install_host_defaults(
        r#"
                [[extension_priority]]
                extension = "md"
                order = ["host-md"]
            "#,
    );
    assert!(reg.extension_priority_order("md").is_some());

    // user 가 빈 order 로 명시 → 제거 의도로 entry 삭제.
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("u.toml");
    std::fs::write(
        &p,
        r#"
                [[extension_priority]]
                extension = "md"
                order = []
            "#,
    )
    .unwrap();
    reg.install_user_config(&p);
    assert!(reg.extension_priority_order("md").is_none());
}

#[test]
fn extension_priority_exported_only_user_origin() {
    let reg = FileFormatRegistry::new();
    reg.install_host_defaults(
        r#"
                [[extension_priority]]
                extension = "md"
                order = ["host-md"]
            "#,
    );
    // host-only — export 비어야 함.
    assert_eq!(reg.export_user_config(), "");

    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("u.toml");
    std::fs::write(
        &p,
        r#"
                [[extension_priority]]
                extension = "json"
                order = ["json-strict", "json"]
            "#,
    )
    .unwrap();
    reg.install_user_config(&p);

    let exported = reg.export_user_config();
    // user 가 적은 json 우선순위만 emit.
    assert!(exported.contains("extension_priority"), "got: {exported}");
    assert!(exported.contains("\"json\""));
    assert!(exported.contains("json-strict"));
    // host 의 md 우선순위는 emit 되지 않아야.
    assert!(!exported.contains("host-md"), "got: {exported}");
}

#[test]
fn extension_priority_round_trip_through_export() {
    let reg = FileFormatRegistry::new();
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("u.toml");
    std::fs::write(
        &p,
        r#"
                [[extension_priority]]
                extension = "md"
                order = ["mdx-strict", "markdown"]

                [[extension_priority]]
                extension = "json"
                order = ["jsonc"]
            "#,
    )
    .unwrap();
    reg.install_user_config(&p);
    let exported = reg.export_user_config();

    let reg2 = FileFormatRegistry::new();
    let p2 = dir.path().join("export.toml");
    std::fs::write(&p2, &exported).unwrap();
    reg2.install_user_config(&p2);

    assert_eq!(
        reg2.extension_priority_order("md"),
        Some(vec![
            DetectorId("mdx-strict".into()),
            DetectorId("markdown".into())
        ])
    );
    assert_eq!(
        reg2.extension_priority_order("json"),
        Some(vec![DetectorId("jsonc".into())])
    );
}

#[test]
fn extension_priority_reload_clears_old_user_entries() {
    let reg = FileFormatRegistry::new();
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("u.toml");

    // 1차: md + json.
    std::fs::write(
        &p,
        r#"
                [[extension_priority]]
                extension = "md"
                order = ["mdx"]

                [[extension_priority]]
                extension = "json"
                order = ["json"]
            "#,
    )
    .unwrap();
    reg.install_user_config(&p);
    assert!(reg.extension_priority_order("md").is_some());
    assert!(reg.extension_priority_order("json").is_some());

    // 2차: md 만 (json 제거) → reload.
    std::fs::write(
        &p,
        r#"
                [[extension_priority]]
                extension = "md"
                order = ["mdx"]
            "#,
    )
    .unwrap();
    reg.reload_user_config(&p);
    assert!(reg.extension_priority_order("md").is_some());
    assert!(
        reg.extension_priority_order("json").is_none(),
        "user reload should drop previous json entry",
    );
}

#[test]
fn extension_priority_dedupes_duplicate_ids_in_order() {
    let reg = FileFormatRegistry::new();
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("u.toml");
    std::fs::write(
        &p,
        r#"
                [[extension_priority]]
                extension = "md"
                order = ["a", "b", "a", "b", "c"]
            "#,
    )
    .unwrap();
    reg.install_user_config(&p);

    let order = reg.extension_priority_order("md").unwrap();
    assert_eq!(
        order,
        vec![
            DetectorId("a".into()),
            DetectorId("b".into()),
            DetectorId("c".into())
        ]
    );
}

// ── identify cheap path cutover (Phase E ME3) ──────────────────────

#[test]
fn identify_uses_extension_priority_table() {
    let reg = FileFormatRegistry::new();
    // 두 detector 가 .md 광고. priority 표가 "b" 우선이라 b 가 이김.
    let user_toml = r#"
            [[detector]]
            id = "a"
            [[detector.rule]]
            kind = "extension"
            values = ["md"]

            [[detector]]
            id = "b"
            [[detector.rule]]
            kind = "extension"
            values = ["md"]

            [[extension_priority]]
            extension = "md"
            order = ["b", "a"]
        "#;
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("u.toml");
    std::fs::write(&p, user_toml).unwrap();
    reg.install_user_config(&p);

    let got = reg.identify(&target("hello.md"), DetectDepth::Cheap);
    assert_eq!(got, Some(DetectorId("b".into())));
}

#[test]
fn identify_falls_back_to_install_order_without_priority_table() {
    let reg = FileFormatRegistry::new();
    // a 가 먼저 install 됨 → install_order 0. b 가 두번째 → 1.
    let user_toml = r#"
            [[detector]]
            id = "z"
            [[detector.rule]]
            kind = "extension"
            values = ["md"]
        "#;
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("u.toml");
    std::fs::write(&p, user_toml).unwrap();
    reg.install_user_config(&p);

    // 두번째 plugin install — 알파벳상 더 앞이지만 install_order 가 더 큼.
    let decls = vec![DetectorDecl {
        id: "a".into(),
        display_name_i18n_key: None,
        icon: None,
        disabled: None,
        rule: vec![DetectorRuleDecl::Extension {
            values: vec!["md".into()],
        }],
    }];
    reg.install_plugin_detectors("com.example.a", &decls);

    // priority 표 없음 → install_order 우선 → "z" 가 이김 (알파벳 아닌 install 순).
    let got = reg.identify(&target("hello.md"), DetectDepth::Cheap);
    assert_eq!(got, Some(DetectorId("z".into())));
}

#[test]
fn identify_priority_entry_with_unknown_id_skips_to_next() {
    let reg = FileFormatRegistry::new();
    // priority 표가 미설치 detector "ghost" 를 1순위로 적었지만 그건 무시되고
    // advertised 후보 중 install_order 첫 번째인 "real" 이 이김.
    let user_toml = r#"
            [[detector]]
            id = "real"
            [[detector.rule]]
            kind = "extension"
            values = ["md"]

            [[extension_priority]]
            extension = "md"
            order = ["ghost", "real"]
        "#;
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("u.toml");
    std::fs::write(&p, user_toml).unwrap();
    reg.install_user_config(&p);

    let got = reg.identify(&target("a.md"), DetectDepth::Cheap);
    assert_eq!(got, Some(DetectorId("real".into())));
}

#[test]
fn identify_fast_path_skips_disabled_detectors() {
    let reg = FileFormatRegistry::new();
    let user_toml = r#"
            [[detector]]
            id = "off"
            disabled = true
            [[detector.rule]]
            kind = "extension"
            values = ["md"]

            [[detector]]
            id = "on"
            [[detector.rule]]
            kind = "extension"
            values = ["md"]

            [[extension_priority]]
            extension = "md"
            order = ["off", "on"]
        "#;
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("u.toml");
    std::fs::write(&p, user_toml).unwrap();
    reg.install_user_config(&p);

    // priority 1순위가 disabled → 2순위 on 이 이김.
    let got = reg.identify(&target("a.md"), DetectDepth::Cheap);
    assert_eq!(got, Some(DetectorId("on".into())));
}

#[test]
fn identify_fast_path_does_not_apply_to_directory_target() {
    let reg = FileFormatRegistry::new();
    // 호스트 default 의 $directory 가 디렉토리에 매칭되어야 함 (fast path 가 디렉토리에는
    // 적용되지 않음을 확인).
    install_host_with_markdown(&reg);
    // 사용자가 .tmp 확장자를 가진 detector 등록.
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("u.toml");
    std::fs::write(
        &p,
        r#"
                [[detector]]
                id = "junk"
                [[detector.rule]]
                kind = "extension"
                values = ["tmp"]
            "#,
    )
    .unwrap();
    reg.install_user_config(&p);

    // 디렉토리 path 가 ".tmp" 로 끝나도 IsDirectory pre-filter 가 우선 — $directory 가 이김.
    let tmp_dir = dir.path().join("scratch.tmp");
    std::fs::create_dir_all(&tmp_dir).unwrap();
    let got = reg.identify(&FileTarget::new(tmp_dir), DetectDepth::Cheap);
    assert_eq!(got, Some(DetectorId("$directory".into())));
}

#[test]
fn identify_existing_tests_still_pass_after_cutover() {
    // 빠른 회귀 — host default 단순 매칭 (markdown) 이 깨지지 않음을 확인.
    // image detector 는 com.tasty.image plugin 이 제공 — host default 는 등록 안 함.
    let reg = FileFormatRegistry::new();
    install_host_with_markdown(&reg);
    assert_eq!(
        reg.identify(&target("a/b.md"), DetectDepth::Cheap),
        Some(DetectorId("markdown".into()))
    );
}

#[test]
fn install_order_persists_across_patch_from_other_origin() {
    let reg = FileFormatRegistry::new();
    // 1번째: host default 로 markdown install (install_order=0).
    install_host_with_markdown(&reg);
    let initial = reg
        .detector(&DetectorId("markdown".into()))
        .unwrap()
        .install_order;
    // 2번째: 사용자가 같은 id 에 mdx 추가 → patch. install_order 변하지 않아야 함.
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("u.toml");
    std::fs::write(
        &p,
        r#"
                [[detector]]
                id = "markdown"
                [[detector.rule]]
                kind = "extension"
                values = ["mdx"]
            "#,
    )
    .unwrap();
    reg.install_user_config(&p);
    let after = reg
        .detector(&DetectorId("markdown".into()))
        .unwrap()
        .install_order;
    assert_eq!(initial, after);
}

/// poison 된 레지스트리가 **조용히 아무것도 안 하는** 대신 계속 동작한다.
///
/// 이전에는 락 획득 24 곳이 전부 무음이라, poison 이후 detector 설치는 no-op 이 되고
/// `identify_*` 는 빈 결과를 돌려줬다. 증상은 "그 확장자를 아무것도 못 알아본다" 인데
/// 관측 지점이 0 이었다.
#[test]
fn a_poisoned_registry_still_installs_and_identifies() {
    let reg = std::sync::Arc::new(FileFormatRegistry::new());

    let held = std::sync::Arc::clone(&reg);
    let joined = std::thread::spawn(move || {
        let _guard = held.inner.write().expect("fresh rwlock");
        panic!("a thread dies while holding the registry");
    })
    .join();
    assert!(joined.is_err(), "그 스레드는 패닉했어야 한다");
    assert!(reg.inner.read().is_err(), "poison 됐어야 한다");

    install_host_with_markdown(&reg);
    assert!(
        reg.identify(&target("readme.md"), DetectDepth::Cheap)
            .is_some(),
        "poison 이후에도 설치가 반영되고 식별이 된다"
    );
}

/// `https://example.com/a.md` 의 확장자 `md` 로 markdown 을 고르던 fast path 가 URL 에서
/// 돌지 않는다. 같은 파일명의 로컬 경로는 그대로 markdown 이다(회귀 방지).
#[test]
fn identify_does_not_extension_match_a_url_target() {
    let reg = FileFormatRegistry::new();
    install_host_with_markdown(&reg);
    let url = target("https://example.com/a.md");
    assert_eq!(reg.identify(&url, DetectDepth::Cheap), None);
    assert_eq!(reg.identify(&url, DetectDepth::Deep), None);
    assert_eq!(
        reg.identify(&target("/tmp/a.md"), DetectDepth::Cheap),
        Some(DetectorId("markdown".into())),
    );
}

/// PathGlob 은 파일명(`file_name`)만 보므로 URL 의 마지막 세그먼트에 걸린다 — URL 에서는
/// glob 규칙도 매칭하지 않는다.
#[test]
fn identify_does_not_path_glob_match_a_url_target() {
    let reg = FileFormatRegistry::new();
    let decls = vec![DetectorDecl {
        id: "dockerfile".into(),
        display_name_i18n_key: None,
        icon: None,
        disabled: None,
        rule: vec![DetectorRuleDecl::PathGlob {
            pattern: "Dockerfile".into(),
        }],
    }];
    reg.install_plugin_detectors("com.example.docker", &decls);
    assert_eq!(
        reg.identify(&target("/repo/Dockerfile"), DetectDepth::Cheap),
        Some(DetectorId("dockerfile".into())),
    );
    assert_eq!(
        reg.identify(
            &target("https://example.com/repo/Dockerfile"),
            DetectDepth::Cheap
        ),
        None,
    );
}

// ── 병합 순서: user patch 는 설치 순서와 무관하게 마지막에 이긴다 ──────────

fn plugin_markdown_decl(icon: &str, disabled: Option<bool>) -> DetectorDecl {
    DetectorDecl {
        id: "markdown".into(),
        display_name_i18n_key: Some("plugin.markdown".into()),
        icon: Some(icon.into()),
        disabled,
        rule: vec![DetectorRuleDecl::Extension {
            values: vec!["md".into()],
        }],
    }
}

fn user_config(body: &str) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("file-formats.toml");
    std::fs::write(&p, body).unwrap();
    (dir, p)
}

fn markdown(reg: &FileFormatRegistry) -> FileFormatDetector {
    reg.detector(&DetectorId("markdown".into()))
        .expect("markdown detector")
}

/// 부팅 순서 그대로 — host 기본값 → user 설정 → plugin. plugin 이 값을 주는 필드도 user 값이
/// 이긴다.
#[test]
fn a_user_patch_wins_over_a_plugin_installed_after_the_boot_load() {
    let reg = FileFormatRegistry::new();
    reg.install_host_defaults(crate::HOST_DEFAULTS_TOML);
    let (_dir, p) = user_config(
        r#"
            [[detector]]
            id = "markdown"
            icon = "user-icon"
            display_name_i18n_key = "user.markdown"
        "#,
    );
    reg.install_user_config(&p);
    reg.install_plugin_detectors("com.tasty.markdown", &[plugin_markdown_decl("p", None)]);

    let det = markdown(&reg);
    assert_eq!(det.icon.as_deref(), Some("user-icon"));
    assert_eq!(det.display_name_i18n_key.as_deref(), Some("user.markdown"));
}

/// plugin 을 껐다 켜면 그 contribution 이 user 뒤에 다시 붙는다 — reload 없이도 user 값이 남는다.
#[test]
fn a_user_patch_wins_over_a_plugin_that_contributes_again_without_a_reload() {
    let reg = FileFormatRegistry::new();
    let (_dir, p) = user_config(
        r#"
            [[detector]]
            id = "markdown"
            icon = "user-icon"
        "#,
    );
    reg.install_plugin_detectors("com.tasty.markdown", &[plugin_markdown_decl("p", None)]);
    reg.install_user_config(&p);
    assert_eq!(markdown(&reg).icon.as_deref(), Some("user-icon"));

    reg.uninstall_plugin("com.tasty.markdown");
    reg.install_plugin_detectors("com.tasty.markdown", &[plugin_markdown_decl("p", None)]);
    assert_eq!(markdown(&reg).icon.as_deref(), Some("user-icon"));
}

/// plugin 이 끈 detector 를 user 가 `disabled = false` 로 켠다 — 부팅 순서에서도, plugin 재기동
/// 뒤에도. Settings 의 켜기가 저장 파일에 남기는 모양이 이것이다.
#[test]
fn a_user_enable_beats_a_plugin_disable_across_boot_and_plugin_restart() {
    let reg = FileFormatRegistry::new();
    let (_dir, p) = user_config(
        r#"
            [[detector]]
            id = "markdown"
            disabled = false
        "#,
    );
    reg.install_user_config(&p);
    reg.install_plugin_detectors(
        "com.tasty.markdown",
        &[plugin_markdown_decl("p", Some(true))],
    );
    assert!(!markdown(&reg).disabled, "부팅 뒤 user 의 켜기가 졌다");

    reg.uninstall_plugin("com.tasty.markdown");
    reg.install_plugin_detectors(
        "com.tasty.markdown",
        &[plugin_markdown_decl("p", Some(true))],
    );
    assert!(
        !markdown(&reg).disabled,
        "plugin 재기동 뒤 user 의 켜기가 졌다"
    );

    // 그 상태를 저장하면 같은 모양이 나온다 — 다음 부팅의 입력이다.
    assert!(reg.export_user_config().contains("disabled = false"));
}

/// host · plugin 의 `disabled = false` 는 켜지 않는다 — 다른 출처가 끈 것은 그대로 꺼져 있다.
#[test]
fn a_plugin_disabled_false_does_not_enable_what_the_user_disabled() {
    let reg = FileFormatRegistry::new();
    let (_dir, p) = user_config(
        r#"
            [[detector]]
            id = "markdown"
            disabled = true
        "#,
    );
    reg.install_user_config(&p);
    reg.install_plugin_detectors(
        "com.tasty.markdown",
        &[plugin_markdown_decl("p", Some(false))],
    );
    assert!(markdown(&reg).disabled);
}

/// host 와 plugin 이 같은 id 를 contribute 하면 plugin 이 host 를 덮는다 — 설치 순서와 무관하게.
#[test]
fn a_plugin_overrides_a_host_default_whatever_the_install_order() {
    let host = r#"
        [[detector]]
        id = "markdown"
        icon = "host-icon"
        [[detector.rule]]
        kind = "extension"
        values = ["md"]
    "#;
    let reg = FileFormatRegistry::new();
    reg.install_plugin_detectors("com.tasty.markdown", &[plugin_markdown_decl("p", None)]);
    reg.install_host_defaults(host);
    assert_eq!(markdown(&reg).icon.as_deref(), Some("p"));
}
