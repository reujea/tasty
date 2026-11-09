//! 수를 받는 문자열의 단수 변형.
//!
//! 수를 받는 키 `<key>` 옆에 `<key>_one` 을 두면 수가 1일 때 그 값을 쓴다. 영어처럼 1과 나머지가
//! 갈리는 언어의 카탈로그만 변형을 두고, ko·ja·zh-Hans 처럼 한 문자열로 충분한 언어는 두지 않는다.
//!
//! 변형은 자기 키와 같은 층에서 온 것만 쓴다. 카탈로그는 영어 바탕 위에 언어 파일과 사용자
//! 파일을 덮어 만들기 때문에, `<key>` 를 덮은 층이 `<key>_one` 을 주지 않으면 아래 층의 변형을
//! 지운다. 그러지 않으면 ko 문장 대신 영어 단수 문장이 나온다.

use std::collections::HashMap;

/// 단수 변형 키의 접미사.
pub const ONE_SUFFIX: &str = "_one";

/// `key` 의 단수 변형 키.
pub fn one_key(key: &str) -> String {
    format!("{key}{ONE_SUFFIX}")
}

/// `overlay` 를 `strings` 위에 덮는다. `overlay` 가 키를 바꾸면서 그 단수 변형을 주지 않으면
/// 아래 층의 변형을 지운다.
pub fn extend_layer<V>(strings: &mut HashMap<String, V>, overlay: HashMap<String, V>) {
    for key in overlay.keys() {
        let one = one_key(key);
        if !overlay.contains_key(&one) {
            strings.remove(&one);
        }
    }
    strings.extend(overlay);
}

/// 수 `n` 에 맞는 템플릿. `n` 이 1이고 카탈로그에 단수 변형이 있으면 그 값, 아니면 `key` 의 값이다.
pub fn pick<'a>(n: u64, key: &'a str, lookup: impl Fn(&str) -> Option<&'a str>) -> &'a str {
    if n == 1
        && let Some(one) = lookup(&one_key(key))
    {
        return one;
    }
    lookup(key).unwrap_or(key)
}

impl crate::Translations {
    fn lookup(&self, key: &str) -> Option<&'static str> {
        if let Some(s) = self.base.get(key) {
            return Some(s);
        }
        let ns = tasty_utils::poison::recover_read(
            self.namespaces.read(),
            crate::NAMESPACES_WHAT,
            &crate::NAMESPACES_POISONED,
        );
        ns.values().find_map(|map| map.get(key).copied())
    }

    /// 수 `n` 에 맞는 템플릿(`<key>` 또는 `<key>_one`). 키가 없으면 키 자체다.
    pub fn get_count_template<'a>(&self, key: &'a str, n: u64) -> &'a str {
        pick(n, key, |k| self.lookup(k))
    }

    /// 수 `n` 에 맞는 문자열(`<key>` 또는 `<key>_one`)을 골라 `args` 를
    /// [`fill_args`](crate::fill_args) 로 채운다.
    pub fn get_count(&self, key: &str, n: u64, args: &[&str]) -> String {
        crate::fill_args(pick(n, key, |k| self.lookup(k)), args)
    }
}

/// 수 `n` 에 맞는 템플릿을 채우지 않고 돌려준다. 수를 나중에 넣는 위젯에 템플릿을 넘길 때 쓴다.
pub fn t_count_template(key: &str, n: u64) -> &str {
    crate::store(key).get_count_template(key, n)
}

/// 수 `n` 에 맞는 문자열을 골라 `args` 로 채운다. 수 자체도 보여야 하면 `args` 에 넣는다.
/// [`init`](crate::init) 전에는 [`t`](crate::t) 처럼 내장 영어 표를 읽는다.
pub fn t_count(key: &str, n: u64, args: &[&str]) -> String {
    crate::store(key).get_count(key, n, args)
}

#[cfg(test)]
#[path = "plural_tests.rs"]
mod tests;
