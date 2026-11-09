//! 갤러리의 번역 조회. 본체와 같은 `lang/en.toml` 문자열을 읽어 사본을 두지 않는다.
//! 갤러리는 설정을 읽지 않는다. 처음 조회할 때 `TASTY_LOCALE`의 언어로 한 번 초기화하고, 값이 없으면
//! 영어다. 라벨 글꼴과 같은 값을 읽어 표본의 글꼴과 번역 문구가 같은 언어로 나온다.

fn ensure_init() {
    static INIT: std::sync::Once = std::sync::Once::new();
    INIT.call_once(|| {
        // 단위 시험은 영어 문구를 기대하고, Tasty 안에서 실행하면 사용자 언어를 물려받는다.
        let language = if cfg!(test) {
            String::new()
        } else {
            std::env::var("TASTY_LOCALE").unwrap_or_default()
        };
        let report = tasty_i18n::init(if language.is_empty() { "en" } else { &language });
        if report.fell_back() {
            tracing::warn!("gallery i18n init fell back: {report:?}");
        }
    });
}

/// `key`의 현재 언어 문자열.
pub fn t(key: &'static str) -> &'static str {
    ensure_init();
    tasty_i18n::t(key)
}

/// `key`의 현재 언어 문자열에서 첫 `{}` 를 `arg` 로 바꾼다.
pub fn t_fmt(key: &'static str, arg: &str) -> String {
    ensure_init();
    tasty_i18n::t_fmt(key, arg)
}

/// 수 `n` 에 맞는 현재 언어 문자열(`<key>` 또는 단수 변형 `<key>_one`)을 `args` 로 채운다.
pub fn t_count(key: &'static str, n: u64, args: &[&str]) -> String {
    ensure_init();
    tasty_i18n::t_count(key, n, args)
}

/// 수 `n` 에 맞는 현재 언어 템플릿(`<key>` 또는 `<key>_one`)을 채우지 않고 돌려준다.
pub fn t_count_template(key: &'static str, n: u64) -> &'static str {
    ensure_init();
    tasty_i18n::t_count_template(key, n)
}

/// `key`의 현재 언어 문자열에서 앞의 두 `{}` 를 차례로 바꾼다.
pub fn t_fmt2(key: &'static str, arg1: &str, arg2: &str) -> String {
    ensure_init();
    tasty_i18n::t_fmt2(key, arg1, arg2)
}

/// `key`의 현재 언어 문자열에서 `{}` 를 `args` 로 차례로 바꾼다.
pub fn t_args(key: &'static str, args: &[&str]) -> String {
    ensure_init();
    tasty_i18n::t_args(key, args)
}

/// 내장 언어의 번역 파일. 언어를 등록하면 여기에 한 줄을 더한다.
/// 빠지면 `catalogs_cover_every_builtin_language`가 실패한다.
const CATALOGS: &[(&str, &str)] = &[
    ("en", include_str!("../../../lang/en.toml")),
    ("ko", include_str!("../../../lang/ko.toml")),
    ("ja", include_str!("../../../lang/ja.toml")),
    ("zh-Hans", include_str!("../../../lang/zh-Hans.toml")),
];

/// 영어가 아닌 내장 언어. 표본을 언어별로 나란히 그릴 때 쓴다.
pub fn translated_languages() -> impl Iterator<Item = &'static str> {
    tasty_i18n::BUILTIN_CODES
        .into_iter()
        .filter(|code| *code != "en")
}

/// `key`의 `code` 문자열. 한 화면에 여러 언어의 예제를 나란히 보일 때 쓴다.
/// 본체와 같은 `lang/{code}.toml`을 읽으며 키가 없으면 영어로 돌아간다.
pub fn t_lang(code: &str, key: &'static str) -> &'static str {
    use std::collections::HashMap;
    use std::sync::OnceLock;
    static TABLES: OnceLock<HashMap<&'static str, HashMap<String, String>>> = OnceLock::new();
    let tables = TABLES.get_or_init(|| {
        CATALOGS
            .iter()
            .map(|(code, text)| {
                let mut map = HashMap::new();
                match toml::from_str::<toml::Value>(text) {
                    Ok(value) => tasty_i18n::flatten_catalog_toml("", &value, &mut map),
                    Err(e) => tracing::warn!("gallery lang/{code}.toml did not parse: {e}"),
                }
                (*code, map)
            })
            .collect()
    });
    match tables
        .get(code)
        .and_then(|table| table.get(key))
        .or_else(|| tables.get("en").and_then(|table| table.get(key)))
    {
        Some(value) => value.as_str(),
        None => t(key),
    }
}

#[cfg(test)]
pub(crate) fn catalog_value(code: &str) -> toml::Value {
    let (_, text) = CATALOGS
        .iter()
        .find(|(listed, _)| *listed == code)
        .unwrap_or_else(|| panic!("CATALOGS에 {code}가 없다"));
    toml::from_str(text).expect("lang file parses")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 표에 없는 언어는 영어로 조용히 돌아가므로 표와 내장 언어 목록을 대조한다.
    #[test]
    fn catalogs_cover_every_builtin_language() {
        let listed: Vec<&str> = CATALOGS.iter().map(|(code, _)| *code).collect();
        assert_eq!(
            listed,
            tasty_i18n::BUILTIN_CODES,
            "CATALOGS에 내장 언어의 번역 파일을 빠짐없이 적어야 한다"
        );
    }
}
