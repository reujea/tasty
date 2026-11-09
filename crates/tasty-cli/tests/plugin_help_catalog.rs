//! Bundled manifest text, translation keys, and shipped catalogs move together.
use std::path::Path;
use tasty_doc_guards::floored_walk::{CountedOn, Descend, Floor, walk_with_floor};

#[test]
fn every_bundled_cli_description_and_argument_has_a_catalog_key() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let mut slots = 0;
    let floor = Floor {
        min: 9,
        measured: 9,
        measured_on: "2026-09-15",
        counted_on: CountedOn::Tree(
            "4d307aeb3 — 이 lane 의 tip 은 체리픽으로 main에 반영되며 사라졌다. 반영된 트리에서 다시 셌고 \
             값은 9 로 같다.",
        ),
        why_this_gap: "Each bundled plugin has one manifest directly under its crate. Removing a member is an explicit product change, so no unexplained decrease is accepted.",
    };
    let manifests = walk_with_floor(
        root,
        root,
        &floor,
        Descend::SkipBuildCachesAndDotDirs,
        &|entry| {
            entry
                .path
                .file_name()
                .is_some_and(|name| name == "tasty-plugin.toml")
                && entry.path.parent().and_then(Path::parent) == Some(root)
        },
    )
    .unwrap();
    for entry in manifests {
        let dir = entry.path.parent().unwrap();
        let manifest = tasty_plugin_manifest::Manifest::load(dir).unwrap();
        let catalogs: Vec<_> = tasty_i18n::BUILTIN_CODES
            .iter()
            .map(|locale| {
                tasty_i18n::plugin_catalog::load(
                    &dir.join(&manifest.lang_dir),
                    locale,
                    &manifest.id,
                    None,
                )
            })
            .collect();
        let check = |text: &Option<String>, key: &Option<String>| {
            let Some(text) = text.as_ref().filter(|t| !t.is_empty()) else {
                return 0;
            };
            let key = key
                .as_ref()
                .expect("bundled CLI text needs a translation key");
            assert_eq!(catalogs[0].get(key), Some(text), "{}: {key}", manifest.id);
            for catalog in &catalogs {
                assert!(
                    catalog.get(key).is_some_and(|s| !s.trim().is_empty()),
                    "{}: {key}",
                    manifest.id
                );
            }
            1
        };
        for cli in &manifest.contributes.cli {
            // 번들 plugin의 도움말 그룹은 선택 가능한 값이어야 한다. 오타는 plugin 그룹으로 떨어진다.
            if let Some(group) = cli.help_group.as_deref() {
                assert!(
                    tasty_cli::help_groups::PLUGIN_SELECTABLE.contains(&group),
                    "{}: help_group {group}",
                    manifest.id
                );
            }
            slots += check(&cli.description, &cli.description_i18n_key);
            for sub in &cli.subcommands {
                slots += check(&sub.description, &sub.description_i18n_key);
            }
            for group in cli.arg_groups.values() {
                for arg in group.positional.iter().chain(&group.flags) {
                    slots += check(&arg.help, &arg.help_i18n_key);
                }
            }
        }
    }
    assert!(slots > 0, "no bundled CLI text was examined");
}
