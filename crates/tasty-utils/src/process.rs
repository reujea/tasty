//! 백그라운드 자식 프로세스의 공통 설정. hide_console은 Windows에서
//! CREATE_NO_WINDOW를 적용하며 다른 OS에서는 아무 일도 하지 않는다.
//! 사용자 터미널 셸은 portable-pty의 ConPTY 경로로 생성하므로 별개다.

use std::ffi::OsString;
use std::process::Command;

/// Windows 콘솔 서브시스템 자식 프로세스가 새 콘솔 창을 띄우지 않도록
/// `CREATE_NO_WINDOW` 플래그를 적용한다. 비-Windows 에서는 아무 동작도 하지 않는다.
///
/// `Command` 를 mutable 로 받아 그대로 돌려주므로 빌더 체인 중간에 끼울 수 있다.
pub fn hide_console(cmd: &mut Command) -> &mut Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // winbase.h CREATE_NO_WINDOW. windows-sys 의존 회피용 직접 상수.
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

/// Tasty 를 띄운 프로세스에서 상속되지만 Tasty 가 띄우는 자식(터미널 셸·훅 명령·작업 자식)에
/// 넘기지 않는 환경변수의 접두사 중 본체가 직접 정하는 것. `CMUX_` 는 자식에서 cmux CLI 가
/// 동작하지 않게 한다. 특정 제품의 세션 변수는 그 제품 플러그인이 매니페스트의
/// `contributes.strip_inherited_env` 로 선언하며 [`set_plugin_inherited_env_rules`] 로 들어온다.
pub const STRIPPED_ENV_PREFIXES: &[&str] = &["CMUX_"];

/// 플러그인 하나가 선언한 상속 환경변수 제거 규칙. 이름이 같은 키, 접두사로 시작하는 키,
/// 지정한 이름이면서 값이 접두사로 시작하는 키를 지운다. 사용자가 직접 넣는 설정과 이름공간을
/// 공유하는 변수는 값 규칙으로 좁힌다.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InheritedEnvRules {
    pub names: Vec<String>,
    pub prefixes: Vec<String>,
    /// `(이름, 값 접두사 목록)`.
    pub value_prefixes: Vec<(String, Vec<String>)>,
}

impl InheritedEnvRules {
    pub fn is_empty(&self) -> bool {
        self.names.is_empty() && self.prefixes.is_empty() && self.value_prefixes.is_empty()
    }

    /// `key=value` 가 이 규칙에 걸리는지 판정한다. 값이 UTF-8 이 아니면 값 규칙에는 걸리지 않는다.
    pub fn matches(&self, key: &str, value: &std::ffi::OsStr) -> bool {
        if self.names.iter().any(|n| n == key)
            || self.prefixes.iter().any(|p| key.starts_with(p.as_str()))
        {
            return true;
        }
        self.value_prefixes
            .iter()
            .filter(|(name, _)| name == key)
            .any(|(_, prefixes)| {
                value
                    .to_str()
                    .is_some_and(|v| prefixes.iter().any(|p| v.starts_with(p.as_str())))
            })
    }
}

/// 소유자(플러그인 ID)별 제거 규칙. 설치되고 활성인 플러그인의 선언만 들어 있다.
/// 프로세스 실행 여부와 무관하게 설치·활성 상태만 따르므로 플러그인이 준비되기 전에
/// 만든 첫 자식에도 같은 규칙이 적용된다.
static PLUGIN_RULES: std::sync::RwLock<std::collections::BTreeMap<String, InheritedEnvRules>> =
    std::sync::RwLock::new(std::collections::BTreeMap::new());
static PLUGIN_RULES_POISON_REPORTED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);
const PLUGIN_RULES_WHAT: &str = "the plugin inherited-env rule table";

/// 소유자의 제거 규칙을 바꾼다. 빈 규칙은 그 소유자를 지운다. 이후 만드는 자식부터 적용된다.
pub fn set_plugin_inherited_env_rules(owner: &str, rules: InheritedEnvRules) {
    let mut table = crate::poison::recover_write(
        PLUGIN_RULES.write(),
        PLUGIN_RULES_WHAT,
        &PLUGIN_RULES_POISON_REPORTED,
    );
    if rules.is_empty() {
        table.remove(owner);
    } else {
        table.insert(owner.to_string(), rules);
    }
}

/// 현재 적용 중인 소유자별 규칙의 사본.
pub fn plugin_inherited_env_rules() -> std::collections::BTreeMap<String, InheritedEnvRules> {
    crate::poison::recover_read(
        PLUGIN_RULES.read(),
        PLUGIN_RULES_WHAT,
        &PLUGIN_RULES_POISON_REPORTED,
    )
    .clone()
}

/// `key=value` 가 자식에게 넘기지 않는 상속 환경변수인지 판정한다. 본체의 [`STRIPPED_ENV_PREFIXES`]
/// 와 활성 플러그인의 규칙을 함께 본다. Tasty 가 넣는 TASTY_* 는 대상이 아니다.
pub fn is_stripped_inherited_env(key: &str, value: &std::ffi::OsStr) -> bool {
    if STRIPPED_ENV_PREFIXES.iter().any(|p| key.starts_with(p)) {
        return true;
    }
    // TASTY_* 의 제거·재주입은 본체가 정한다. 플러그인 규칙이 덮지 못한다.
    if key.starts_with("TASTY_") {
        return false;
    }
    crate::poison::recover_read(
        PLUGIN_RULES.read(),
        PLUGIN_RULES_WHAT,
        &PLUGIN_RULES_POISON_REPORTED,
    )
    .values()
    .any(|rules| rules.matches(key, value))
}

/// `inherited` 의 `(키, 값)` 중 자식 환경에서 지울 키를 고른다. UTF-8 이 아닌 키는 목록과 같을 수
/// 없어 건너뛴다.
pub fn env_keys_to_strip(
    inherited: impl IntoIterator<Item = (OsString, OsString)>,
) -> Vec<OsString> {
    inherited
        .into_iter()
        .filter(|(k, v)| k.to_str().is_some_and(|k| is_stripped_inherited_env(k, v)))
        .map(|(k, _)| k)
        .collect()
}

/// 바깥 Tasty 인스턴스가 이 프로세스에 남긴 신원 변수. 이 Tasty 를 다른 Tasty 의 터미널에서
/// 띄웠으면 그 인스턴스의 세션 토큰·에이전트 ID·surface·완료 알림 경로를 가리킨다. 자식이 부른
/// `tasty` 가 이 토큰을 실으면 이 인스턴스는 모르는 토큰이라 요청을 거절하고, surface 를 생략한
/// 명령은 엉뚱한 surface 를 가리킨다. Tasty 가 띄우는 자식(터미널 셸·훅·작업 자식)은 모두 이
/// 값을 물려주지 않는다. 터미널 셸은 지운 뒤 자기 `TASTY_SURFACE_ID`·`TASTY_PARENT_HOME` 을
/// 넣는다. 세션 토큰·에이전트 ID 는 이 인스턴스가 발급한 값이 없으므로 덮어쓰지 않는다. 자식
/// 에이전트의 값은 플러그인이 실행 명령 앞에 직접 붙인다.
pub const OUTER_IDENTITY_ENV: &[&str] = &[
    "TASTY_SESSION_TOKEN",
    "TASTY_SURFACE_ID",
    "TASTY_PARENT_HOME",
    "TASTY_AGENT_ID",
];

/// `key=value` 가 Tasty 가 띄우는 자식(훅·작업 자식)에게 넘기지 않는 상속 환경변수인지 판정한다.
/// [`is_stripped_inherited_env`] 에 [`OUTER_IDENTITY_ENV`] 를 더한다.
pub fn is_child_stripped_env(key: &str, value: &std::ffi::OsStr) -> bool {
    OUTER_IDENTITY_ENV.contains(&key) || is_stripped_inherited_env(key, value)
}

/// 훅 실행(surface 훅·전역 훅·hook_handler)이 상속 환경에서 지울 키([`is_child_stripped_env`]).
/// 터미널 셸은 셸 설정 환경변수를 지키려고 [`OUTER_IDENTITY_ENV`] 를 따로 먼저 지운다.
pub fn hook_env_keys_to_strip(
    inherited: impl IntoIterator<Item = (OsString, OsString)>,
) -> Vec<OsString> {
    inherited
        .into_iter()
        .filter(|(k, v)| k.to_str().is_some_and(|k| is_child_stripped_env(k, v)))
        .map(|(k, _)| k)
        .collect()
}

/// 현재 프로세스 환경에서 훅 실행에 넘기지 않을 키([`hook_env_keys_to_strip`]).
pub fn process_env_keys_to_strip() -> Vec<OsString> {
    hook_env_keys_to_strip(std::env::vars_os())
}

/// 현재 실행 파일의 디렉터리를 base PATH 앞에 붙인다. 패키징된 앱의 제한된 PATH에서도
/// 자식이 tasty를 찾도록 한다. 기존 항목은 유지하며 사용자 로그인 셸의 PATH를 읽지는 않는다.
/// 실행 파일·부모 경로를 얻거나 PATH를 조립하지 못하면 None이다.
pub fn path_prepending_self_dir(base: Option<OsString>) -> Option<OsString> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;
    let mut entries = vec![dir.to_path_buf()];
    if let Some(base) = base.as_deref() {
        entries.extend(std::env::split_paths(base));
    }
    std::env::join_paths(entries).ok()
}

/// 터미널 셸 자식이 받을 PATH. 셸 판정의 PATH 탐색과 spawn이 함께 써서 판정한 파일과 실행한 파일이
/// 갈리지 않게 한다. 호스트 PATH 앞에 실행 파일 폴더를 붙인 값이 셸 환경변수(`extra_env`)의 PATH보다
/// 우선한다. 그 값을 만들지 못하면 `extra_env`의 PATH, 그다음 호스트 PATH다. 셋 다 없으면 `None`.
pub fn shell_child_path(extra_env: &[(&str, &str)]) -> Option<OsString> {
    let host = std::env::var_os("PATH");
    path_prepending_self_dir(host.clone())
        .or_else(|| {
            extra_env
                .iter()
                .rev()
                .find(|(key, _)| is_path_key(key))
                .map(|(_, value)| OsString::from(value))
        })
        .or(host)
}

fn is_path_key(key: &str) -> bool {
    if cfg!(windows) {
        key.eq_ignore_ascii_case("PATH")
    } else {
        key == "PATH"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 훅 실행은 플러그인 규칙의 키에 더해 바깥 인스턴스의 신원 변수를 지운다.
    #[test]
    fn hook_keys_add_the_outer_caller_identity() {
        set_plugin_inherited_env_rules(
            "test.hook-keys",
            InheritedEnvRules {
                names: vec!["XTEST_HOOK_KEYS_SESSION".into()],
                ..Default::default()
            },
        );
        let inherited = [
            ("XTEST_HOOK_KEYS_SESSION", "1"),
            ("TASTY_SESSION_TOKEN", "outer"),
            ("TASTY_AGENT_ID", "outer"),
            ("TASTY_SURFACE_ID", "3"),
            ("TASTY_HOME", "/h"),
        ]
        .map(|(k, v)| (OsString::from(k), OsString::from(v)));
        let keys = hook_env_keys_to_strip(inherited.clone());
        let shell_keys = env_keys_to_strip(inherited);
        set_plugin_inherited_env_rules("test.hook-keys", InheritedEnvRules::default());
        assert_eq!(
            keys,
            [
                "XTEST_HOOK_KEYS_SESSION",
                "TASTY_SESSION_TOKEN",
                "TASTY_AGENT_ID",
                "TASTY_SURFACE_ID"
            ]
            .map(OsString::from)
        );
        assert_eq!(shell_keys, [OsString::from("XTEST_HOOK_KEYS_SESSION")]);
    }

    /// 이름·접두사·값 접두사 규칙이 각각 동작하고, 값 규칙은 다른 값과 다른 이름을 남긴다.
    #[test]
    fn plugin_rules_match_names_prefixes_and_value_prefixes() {
        let rules = InheritedEnvRules {
            names: vec!["XTEST_RULE_NAME".into()],
            prefixes: vec!["XTEST_RULE_PFX_".into()],
            value_prefixes: vec![("XTEST_RULE_AGENT".into(), vec!["vendor_".into()])],
        };
        let m = |k: &str, v: &str| rules.matches(k, std::ffi::OsStr::new(v));
        assert!(m("XTEST_RULE_NAME", "x"));
        assert!(m("XTEST_RULE_PFX_ANY", "x"));
        assert!(m("XTEST_RULE_AGENT", "vendor_1"));
        assert!(!m("XTEST_RULE_AGENT", "user-set"));
        assert!(!m("XTEST_RULE_NAME_LONGER", "x"));
        assert!(!m("XTEST_OTHER", "vendor_1"));
    }

    /// 규칙을 지우면 그 소유자의 키가 다시 남고, 플러그인 규칙은 TASTY_* 를 지우지 못한다.
    #[test]
    fn removing_an_owner_restores_its_keys_and_tasty_keys_stay_core_owned() {
        let key = "XTEST_OWNER_REMOVAL";
        let v = std::ffi::OsStr::new("1");
        assert!(!is_stripped_inherited_env(key, v));
        set_plugin_inherited_env_rules(
            "test.owner-removal",
            InheritedEnvRules {
                names: vec![key.into(), "TASTY_LOCALE".into()],
                prefixes: vec!["TASTY_".into()],
                ..Default::default()
            },
        );
        let while_set = (
            is_stripped_inherited_env(key, v),
            is_stripped_inherited_env("TASTY_LOCALE", v),
            plugin_inherited_env_rules().contains_key("test.owner-removal"),
        );
        set_plugin_inherited_env_rules("test.owner-removal", InheritedEnvRules::default());
        assert_eq!(while_set, (true, false, true));
        assert!(!is_stripped_inherited_env(key, v));
        assert!(!plugin_inherited_env_rules().contains_key("test.owner-removal"));
        assert!(is_stripped_inherited_env("CMUX_SOCKET_PATH", v));
    }

    /// self-binary 디렉토리가 최소 PATH 맨 앞에 붙는지 검증한다 — 패키징된
    /// macOS `.app` 이 받는 `/usr/bin:/bin:...` 같은 최소 PATH 를 흉내낸다.
    #[test]
    fn prepends_self_binary_dir_to_minimal_path() {
        let exe = std::env::current_exe().expect("current_exe");
        let self_dir = exe.parent().expect("parent");
        // join_paths 로 만들어 구분자(`:`/`;`)를 크로스플랫폼으로 맞춘다.
        let base = std::env::join_paths([
            std::path::PathBuf::from("/usr/bin"),
            std::path::PathBuf::from("/bin"),
        ])
        .expect("join base");

        let result = path_prepending_self_dir(Some(base)).expect("path built");
        let entries: Vec<_> = std::env::split_paths(&result).collect();

        assert_eq!(
            entries.first().map(|p| p.as_path()),
            Some(self_dir),
            "self binary dir must be the first PATH entry"
        );
        // 기존 항목도 보존되어야 한다(덮어쓰기가 아니라 prepend).
        assert!(
            entries
                .iter()
                .any(|p| p == std::path::Path::new("/usr/bin")),
            "existing PATH entries must be preserved"
        );
    }

    /// base 가 없어도(부모에 PATH env 자체가 없는 드문 케이스) self-dir 만으로
    /// 유효한 PATH 를 만든다.
    #[test]
    fn builds_path_from_self_dir_when_base_absent() {
        let result = path_prepending_self_dir(None).expect("path built");
        let entries: Vec<_> = std::env::split_paths(&result).collect();
        assert_eq!(entries.len(), 1, "only the self binary dir");
    }
}
