//! 지정 파일에 없는 글자를 설정 family, 묶음 D2Coding, 플랫폼 대체 순서로 찾게 하는 대체 목록.
//!
//! cosmic-text 는 Attrs 에 family 하나만 받고, 그 family 에 없는 글자는 `Fallback` 이 주는 이름
//! 순서로 찾는다. 플랫폼 목록 앞에 두 이름을 더하고, 그 이름의 면은 지정 파일을 쓸 때만
//! 등록한다. 지정 파일이 없으면 두 이름의 면이 없어 플랫폼 순서(로케일·한자 자형 선택 포함)
//! 그대로다.

use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
use std::sync::{Mutex, OnceLock};

use cosmic_text::{Fallback, PlatformFallback};
use unicode_script::Script;

/// 지정 파일을 쓸 때 설정 family 의 면에 붙이는 이름.
pub const CUSTOM_FALLBACK_FAMILY: &str = "Tasty Custom Font Fallback";
/// 지정 파일을 쓸 때 묶음 D2Coding 의 면에 붙이는 이름.
pub const CUSTOM_FALLBACK_BUNDLED: &str = "Tasty Custom Font Bundled Fallback";

const PREFIX: [&str; 2] = [CUSTOM_FALLBACK_FAMILY, CUSTOM_FALLBACK_BUNDLED];

fn prefixed(platform: &[&'static str]) -> Vec<&'static str> {
    PREFIX
        .iter()
        .copied()
        .chain(platform.iter().copied())
        .collect()
}

pub(crate) struct TerminalFallback {
    platform: PlatformFallback,
    common: Vec<&'static str>,
}

/// 문자 체계·로케일별 목록. `PlatformFallback` 의 답은 인자로만 정해지므로 프로세스에서 한 번
/// 만들어 모든 인스턴스(창·설정 창·판정)가 같이 쓴다. trait 이 `&'static str` 조각을 돌려받으므로
/// 만든 목록은 놓아 두며, 그 수는 문자 체계 수와 로케일 수의 곱으로 묶인다.
type ScriptLists = Mutex<HashMap<(Script, String), &'static [&'static str]>>;

fn script_lists() -> &'static ScriptLists {
    static LISTS: OnceLock<ScriptLists> = OnceLock::new();
    LISTS.get_or_init(|| Mutex::new(HashMap::new()))
}

impl TerminalFallback {
    pub(crate) fn new() -> Self {
        let platform = PlatformFallback;
        let common = prefixed(platform.common_fallback());
        Self { platform, common }
    }
}

impl Fallback for TerminalFallback {
    fn common_fallback(&self) -> &[&'static str] {
        &self.common
    }

    fn forbidden_fallback(&self) -> &[&'static str] {
        self.platform.forbidden_fallback()
    }

    fn script_fallback(&self, script: Script, locale: &str) -> &[&'static str] {
        static REPORTED: AtomicBool = AtomicBool::new(false);
        // 목록은 넣기만 하므로 poison 뒤에도 남은 항목은 온전하다.
        let mut scripts = tasty_utils::poison::recover_mutex(
            script_lists().lock(),
            "terminal font fallback lists",
            &REPORTED,
        );
        scripts
            .entry((script, locale.to_owned()))
            .or_insert_with(|| {
                Box::leak(
                    prefixed(self.platform.script_fallback(script, locale)).into_boxed_slice(),
                )
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 창·설정 창·판정마다 인스턴스가 새로 생겨도 문자 체계·로케일 하나에 목록은 하나다.
    #[test]
    fn instances_share_one_list_per_script_and_locale() {
        let first = TerminalFallback::new();
        let second = TerminalFallback::new();
        let a = first.script_fallback(Script::Hangul, "ko-KR");
        let b = second.script_fallback(Script::Hangul, "ko-KR");
        assert!(std::ptr::eq(a, b));
        assert_eq!(a[..PREFIX.len()], PREFIX);
    }
}
