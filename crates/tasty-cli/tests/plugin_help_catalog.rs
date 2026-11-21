//! Bundled manifest text, translation keys, and shipped catalogs move together.

/// 번들 플러그인 매니페스트 수의 하한. 공식 플러그인 8개(픽스처)와 workspace의 mesh-demo다.
/// 구성원을 빼는 것은 명시적인 제품 변경이라 설명 없는 감소를 받지 않는다.
const BUNDLED_MANIFESTS_FLOOR: usize = 9;

#[test]
fn every_bundled_cli_description_and_argument_has_a_catalog_key() {
    let mut slots = 0;
    let manifests = tasty_test_support::bundled_plugin_manifest_dirs();
    assert!(
        manifests.len() >= BUNDLED_MANIFESTS_FLOOR,
        "번들 매니페스트가 {}개로 하한 {BUNDLED_MANIFESTS_FLOOR}보다 적다: {manifests:?}",
        manifests.len()
    );
    for dir in &manifests {
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
