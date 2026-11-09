use std::collections::HashMap;

use super::*;
use crate::Translations;

fn map(pairs: &[(&str, &str)]) -> HashMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
        .collect()
}

#[test]
fn a_layer_that_replaces_a_key_without_its_variant_drops_the_lower_variant() {
    let mut strings = map(&[
        ("a", "{} items"),
        ("a_one", "{} item"),
        ("b", "{} files"),
        ("b_one", "{} file"),
        ("c", "{} rows"),
        ("c_one", "{} row"),
    ]);
    extend_layer(
        &mut strings,
        map(&[("a", "{}개"), ("b", "{} Dateien"), ("b_one", "{} Datei")]),
    );
    assert_eq!(strings.get("a_one"), None);
    assert_eq!(strings.get("b_one").map(String::as_str), Some("{} Datei"));
    assert_eq!(strings.get("c_one").map(String::as_str), Some("{} row"));
}

#[test]
fn the_variant_is_picked_only_for_one() {
    let strings = map(&[("a", "{} items"), ("a_one", "{} item")]);
    let lookup = |k: &str| strings.get(k).map(String::as_str);
    assert_eq!(pick(1, "a", lookup), "{} item");
    assert_eq!(pick(0, "a", lookup), "{} items");
    assert_eq!(pick(2, "a", lookup), "{} items");
    assert_eq!(pick(1, "missing", lookup), "missing");
    let plain = map(&[("p", "{} 개")]);
    assert_eq!(pick(1, "p", |k| plain.get(k).map(String::as_str)), "{} 개");
}

/// 내장 ko·ja 는 en 의 단수 변형을 물려받지 않는다. 모든 en 변형을 본다.
#[test]
fn builtin_korean_and_japanese_do_not_inherit_english_variants() {
    let en = Translations::load_from("en", None).0;
    let variants: Vec<&String> = en
        .base
        .keys()
        .filter(|k| {
            k.strip_suffix(ONE_SUFFIX)
                .is_some_and(|base| en.base.contains_key(base))
        })
        .collect();
    assert!(!variants.is_empty(), "en 에 단수 변형이 하나도 없다");
    for code in crate::BUILTIN_CODES.iter().filter(|code| **code != "en") {
        let code = *code;
        let tr = Translations::load_from(code, None).0;
        // 그 언어 파일이 직접 둔 변형은 같은 층이라 쓴다.
        let mut own = HashMap::new();
        Translations::parse_toml_into(&mut own, crate::builtin_toml(code).unwrap_or_default());
        for one in variants.iter().filter(|k| !own.contains_key(k.as_str())) {
            let base = one.strip_suffix(ONE_SUFFIX).unwrap_or(one);
            assert_eq!(
                tr.get_count(base, 1, &["1"]),
                crate::fill_args(tr.get(base), &["1"]),
                "{code}: `{base}` 의 1 이 en 단수 변형을 썼다"
            );
        }
    }
}

#[test]
fn english_count_uses_the_variant_for_one() {
    let mut strings = Translations::english_base();
    strings.insert("x.count".into(), "{} items".into());
    strings.insert("x.count_one".into(), "{} item".into());
    let tr = Translations::from_strings(strings, "en", None);
    assert_eq!(tr.get_count("x.count", 1, &["1"]), "1 item");
    assert_eq!(tr.get_count("x.count", 3, &["3"]), "3 items");
}

#[test]
fn a_plugin_catalog_drops_the_english_variant_under_another_language() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("en.toml"),
        "[p]\nn = \"{} items\"\nn_one = \"{} item\"\n",
    )
    .unwrap();
    std::fs::write(dir.path().join("ko.toml"), "[p]\nn = \"{}개\"\n").unwrap();
    let en = crate::plugin_catalog::load(dir.path(), "en", "demo", None);
    let ko = crate::plugin_catalog::load(dir.path(), "ko", "demo", None);
    assert_eq!(en.get("p.n_one").map(String::as_str), Some("{} item"));
    assert_eq!(ko.get("p.n_one"), None);
}

#[test]
fn a_user_override_of_the_key_alone_drops_the_builtin_variant() {
    let dir = tempfile::tempdir().unwrap();
    let en = Translations::load_from("en", None).0;
    let (base, _) = en
        .base
        .iter()
        .find(|(k, _)| {
            k.strip_suffix(ONE_SUFFIX)
                .is_some_and(|b| en.base.contains_key(b))
        })
        .expect("en 에 단수 변형이 하나도 없다");
    let key = base.strip_suffix(ONE_SUFFIX).unwrap_or(base);
    let (table, leaf) = key.rsplit_once('.').expect("키에 표 이름이 없다");
    std::fs::write(
        dir.path().join("en.toml"),
        format!("[{table}]\n{leaf} = \"custom {{}}\"\n"),
    )
    .unwrap();
    let tr = Translations::load_from("en", Some(dir.path())).0;
    assert_eq!(tr.get_count(key, 1, &["1"]), "custom 1");
}
