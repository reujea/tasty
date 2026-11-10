//! 지정 파일에 없는 글자를 설정 family, 묶음 D2Coding, 플랫폼 대체 순서로 찾게 하는 대체 목록.
//!
//! cosmic-text 는 Attrs 에 family 하나만 받고, 그 family 에 없는 글자는 `Fallback` 이 주는 이름
//! 순서로 찾는다. 플랫폼 목록 앞에 두 이름을 더하고, 그 이름의 면은 지정 파일을 쓸 때만
//! 등록한다. 지정 파일이 없으면 두 이름의 면이 없어 플랫폼 순서(로케일·한자 자형 선택 포함)
//! 그대로다.

use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::AtomicBool;

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
    /// 문자 체계와 로케일마다 한 번 만든다. cosmic-text 도 문자 체계마다 한 번만 묻는다.
    scripts: Mutex<HashMap<(Script, String), &'static [&'static str]>>,
}

impl TerminalFallback {
    pub(crate) fn new() -> Self {
        let platform = PlatformFallback;
        let common = prefixed(platform.common_fallback());
        Self {
            platform,
            common,
            scripts: Mutex::new(HashMap::new()),
        }
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
            self.scripts.lock(),
            "terminal font fallback lists",
            &REPORTED,
        );
        scripts
            .entry((script, locale.to_owned()))
            // 조합 수는 문자 체계 수와 로케일 수의 곱으로 묶이므로 한 번 만든 목록을 놓아 둔다.
            .or_insert_with(|| {
                Box::leak(
                    prefixed(self.platform.script_fallback(script, locale)).into_boxed_slice(),
                )
            })
    }
}
