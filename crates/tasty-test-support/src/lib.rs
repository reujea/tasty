//! 테스트 전용 공용 유틸리티 — 프로세스 전역 상태(env · `TASTY_HOME`)를 테스트 동안만
//! 갈아끼우는 RAII 가드, 플랫폼별로 형태가 다른 절대경로 조립, 실행 파일 쓰기(unix).
//!
//! 소비자의 dev-dependency로만 사용하며 제품 바이너리에는 포함하지 않는다.

#[cfg(unix)]
mod executable;
mod process_state;
mod rerun_alone;

use std::sync::{Mutex, MutexGuard};

#[cfg(unix)]
pub use executable::write_executable;
pub use process_state::proc_stat_reads_finished;
pub use rerun_alone::{is_rerun_alone, rerun_alone};

/// TASTY_HOME 변경 시험의 공통 락. TastyHomeGuard가 이전 값 보관·복원과 함께 관리한다.
/// 락만 얻고 복원을 빠뜨리는 호출을 막기 위해 크레이트 밖에는 노출하지 않는다.
static TASTY_HOME_ENV_LOCK: Mutex<()> = Mutex::new(());

/// [`TASTY_HOME_ENV_LOCK`] 복구 보고의 대상 이름과 1 회 보고 플래그.
const LOCK_WHAT: &str = "TASTY_HOME_ENV_LOCK";
static LOCK_POISON_REPORTED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// TASTY_HOME을 임시 디렉터리로 바꾸고 Drop에서 원값을 복원한다.
/// 원래 없던 값은 제거하며, 복원을 마친 뒤 직렬화 락을 놓는다.
pub struct TastyHomeGuard {
    // drop 순서 = 선언 순서다. env 복원 → 임시 디렉토리 삭제 → 락 해제 순으로 끝나야
    // 하므로 이 셋의 순서를 바꾸지 않는다(복원과 정리가 모두 락 보유 중에 끝난다).
    _env: EnvVarGuard,
    dir: tempfile::TempDir,
    _lock: MutexGuard<'static, ()>,
}

impl TastyHomeGuard {
    /// 락 획득 → 이전 값 보관 → 새 임시 디렉토리로 `TASTY_HOME` 설정.
    pub fn new() -> Self {
        // 앞선 시험의 패닉으로 생긴 poison은 복구하고 처음 한 번 보고한다.
        let lock = tasty_utils::poison::recover_mutex(
            TASTY_HOME_ENV_LOCK.lock(),
            LOCK_WHAT,
            &LOCK_POISON_REPORTED,
        );
        let dir = tempfile::tempdir().expect("tempdir");
        let env = EnvVarGuard::set("TASTY_HOME", dir.path());
        Self {
            _env: env,
            dir,
            _lock: lock,
        }
    }

    /// 이 가드가 `TASTY_HOME` 으로 지정한 임시 디렉토리.
    pub fn path(&self) -> &std::path::Path {
        self.dir.path()
    }
}

/// `new()` 와 같은 부작용을 일으킨다 — 전역 락을 잡고 `TASTY_HOME` 을 바꾼다.
/// 이 타입을 `#[derive(Default)]` 구조체의 필드로 두면 그 부작용이 조용히 일어난다.
impl Default for TastyHomeGuard {
    fn default() -> Self {
        Self::new()
    }
}

/// 이 스레드의 tasty_home을 가드 전용 임시 디렉터리로 지정한다.
/// 환경변수보다 먼저 적용되는 스레드 로컬 override이므로 전역 락 없이 병렬로 쓸 수 있다.
/// 생성 뒤에도 파일을 쓰는 CoreState는 수명 전체에 걸쳐 가드를 유지해야 한다.
/// 같은 스레드의 여러 인스턴스도 디렉터리를 공유하지 않는다.
///
/// 자식 스레드에는 상속되지 않는다. 그 스레드가 tasty_home을 호출하면 실제 환경 경로를
/// 읽을 수 있다. 격리 검증 절차는 docs/dev-guide/unit-test-isolation.md를 따른다.
pub struct IsolatedHome {
    // drop 순서 = 선언 순서. override 를 먼저 내리고 그 다음에 디렉토리를 지운다 — 반대면
    // 지워진 경로를 가리키는 override 가 잠깐 살아 있다.
    _pop: PopHomeOverride,
    _dir: tempfile::TempDir,
}

/// `Drop` 에서 override 를 내리기만 하는 자리표시자. [`IsolatedHome`] 의 필드 순서로
/// "override 내림 → 디렉토리 삭제" 를 강제하려고 별도 타입으로 둔다.
struct PopHomeOverride;

impl Drop for PopHomeOverride {
    fn drop(&mut self) {
        tasty_utils::path::pop_home_override();
    }
}

impl IsolatedHome {
    /// 새 임시 디렉토리를 만들어 이 스레드의 `tasty_home()` 으로 세운다.
    pub fn new() -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        tasty_utils::path::push_home_override(dir.path().to_path_buf());
        Self {
            _pop: PopHomeOverride,
            _dir: dir,
        }
    }
}

/// `new()` 와 같은 부작용을 일으킨다 — 이 스레드의 home override 를 push 한다.
/// 이 타입을 `#[derive(Default)]` 구조체의 필드로 두면 그 부작용이 조용히 일어난다.
impl Default for IsolatedHome {
    fn default() -> Self {
        Self::new()
    }
}

/// 플랫폼에 맞는 시험용 절대경로. tmp/exp는 Unix의 /tmp/exp, Windows의 C:\tmp\exp가 된다.
pub fn abs_path(rel: &str) -> std::path::PathBuf {
    #[cfg(windows)]
    {
        std::path::PathBuf::from(format!(r"C:\{}", rel.replace('/', r"\")))
    }
    #[cfg(not(windows))]
    {
        std::path::PathBuf::from(format!("/{rel}"))
    }
}

/// 환경변수를 바꾸고 Drop에서 원값을 복원한다. 직접 직렬화 락을 얻지는 않으므로
/// 호출자가 동시 환경변수 접근을 조율해야 한다. TASTY_HOME은 TastyHomeGuard를 사용한다.
pub struct EnvVarGuard {
    key: &'static str,
    prev: Option<std::ffi::OsString>,
}

impl EnvVarGuard {
    /// 현재 값을 보관하고 `value` 로 덮어쓴다.
    pub fn set(key: &'static str, value: impl AsRef<std::ffi::OsStr>) -> Self {
        let guard = Self {
            key,
            prev: std::env::var_os(key),
        };
        // SAFETY: 단위 테스트 한정. 같은 키를 만지는 테스트끼리의 직렬화는 호출부가
        // 보장한다(이 가드를 쓰는 곳은 전용 락 또는 단일 스레드 실행 조건 아래 있다).
        unsafe { std::env::set_var(key, value) };
        guard
    }
}

impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        match &self.prev {
            // SAFETY: `set` 과 동일 조건 — 전용 락으로 직렬화된 단위 테스트 한정.
            Some(v) => unsafe { std::env::set_var(self.key, v) },
            // SAFETY: 상동.
            None => unsafe { std::env::remove_var(self.key) },
        }
    }
}

/// 상속 환경변수 제거(`tasty_utils::process::is_stripped_inherited_env`) 시험이 공유하는 키 목록.
/// 터미널 셸·surface 훅·전역 훅·hook_handler·작업 자식 경로의 시험이 같은 목록을 쓴다.
/// 제품 세션 변수의 목록은 번들 Claude·Codex 매니페스트가 정본이며, 시험은
/// [`install_bundled_plugin_rules`] 로 그 선언을 설치·활성 상태처럼 공용 표에 올린다.
/// 출하 크레이트 밖에 두어 목록을 고쳐도 번들 플러그인의 배포 내용이 바뀌지 않는다.
pub mod strip_env_keys {
    use std::collections::BTreeMap;
    use std::ffi::OsString;

    use tasty_utils::process::{InheritedEnvRules, set_plugin_inherited_env_rules};

    /// 번들 플러그인이 설치·활성일 때 지워야 하는 키. 값은 [`inherited`] 가 정한다.
    /// `CMUX_SOCKET_PATH` 는 본체가 지운다.
    pub const STRIPPED: &[&str] = &[
        "CLAUDECODE",
        "CLAUDE_PID",
        "CLAUDE_EFFORT",
        "CLAUDE_PLUGIN_ROOT",
        "CLAUDE_PLUGIN_DATA",
        "CLAUDE_PROJECT_DIR",
        "CLAUDE_ENV_FILE",
        "CLAUDE_CODE_SESSION_ID",
        "CLAUDE_CODE_CHILD_SESSION",
        "CLAUDE_CODE_SESSION_ATTENDED",
        "CLAUDE_CODE_EXECPATH",
        "CLAUDE_CODE_ENTRYPOINT",
        "CLAUDE_CODE_MESSAGING_SOCKET",
        "CLAUDE_CODE_MESSAGING_TOKEN",
        "CLAUDE_CODE_INVOKED_SKILLS",
        "CLAUDE_CODE_BRIDGE_SESSION_ID",
        "CODEX_THREAD_ID",
        "CLAUDE_PLUGIN_OPTION_API_KEY",
        "CMUX_SOCKET_PATH",
        "AI_AGENT",
    ];

    /// 지우지 않아야 하는 키. 사용자가 넣는 Claude Code 설정과 Claude 전용이 아닌 이름을 포함한다.
    pub const KEPT: &[&str] = &[
        "TASTY_SURFACE_ID",
        "TASTY_PARENT_HOME",
        "TASTY_LOCALE",
        "TERM",
        "CLAUDE_CONFIG_DIR",
        "CLAUDE_CODE_GIT_BASH_PATH",
        "CLAUDE_CODE_OAUTH_TOKEN",
        "CLAUDE_CODE_FORCE_SESSION_PERSISTENCE",
        "CLAUDE_CODE_USE_BEDROCK",
        "TRACEPARENT",
        "ANTHROPIC_API_KEY",
        "MY_CLAUDECODE",
        "CODEX_HOME",
    ];

    /// `AI_AGENT` 에 Claude Code 2.1.291 이 Bash 자식에 넣는 형태의 값을 준다.
    pub const CLAUDE_AI_AGENT: &str = "claude-code_2-1-291_agent";

    /// 시험에 쓸 값. `AI_AGENT` 만 Claude 가 넣은 값이고 나머지는 판정에 영향 없는 값이다.
    pub fn value_for(key: &str) -> &'static str {
        if key == "AI_AGENT" {
            CLAUDE_AI_AGENT
        } else {
            "1"
        }
    }

    /// [`STRIPPED`] 와 [`KEPT`] 를 상속 환경처럼 `(키, 값)` 으로 늘어놓는다.
    pub fn inherited() -> Vec<(OsString, OsString)> {
        STRIPPED
            .iter()
            .chain(KEPT)
            .map(|k| (OsString::from(k), OsString::from(value_for(k))))
            .collect()
    }

    /// 번들 Claude·Codex 매니페스트의 제거 선언. 키는 매니페스트 id 다.
    pub fn bundled_plugin_rules() -> BTreeMap<String, InheritedEnvRules> {
        let crates = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        ["tasty-plugin-claude", "tasty-plugin-codex"]
            .into_iter()
            .map(|dir| {
                let m = tasty_plugin_manifest::Manifest::load(&crates.join(dir))
                    .unwrap_or_else(|e| panic!("{dir} 매니페스트를 읽지 못했다: {e}"));
                (m.id, m.contributes.strip_inherited_env.to_rules())
            })
            .collect()
    }

    /// 번들 두 플러그인이 설치·활성인 상태처럼 공용 표에 선언을 올린다. 같은 시험 바이너리의
    /// 다른 시험에도 보이므로, 번들 키가 남는지 보는 시험은 이 표 대신 [`bundled_plugin_rules`] 의
    /// 규칙으로 직접 판정한다.
    pub fn install_bundled_plugin_rules() {
        for (owner, rules) in bundled_plugin_rules() {
            set_plugin_inherited_env_rules(&owner, rules);
        }
    }

    #[cfg(test)]
    mod tests {
        use std::ffi::{OsStr, OsString};

        use super::{
            KEPT, STRIPPED, bundled_plugin_rules, inherited, install_bundled_plugin_rules,
        };
        use tasty_utils::process::{env_keys_to_strip, is_stripped_inherited_env};

        /// 번들 선언이 설치되면 공용 판정이 제품 세션 키와 CMUX_* 는 고르고 사용자 설정·TASTY_*·
        /// 일반 키는 남긴다.
        #[test]
        fn claude_session_and_cmux_keys_are_stripped_but_others_are_kept() {
            install_bundled_plugin_rules();
            let picked = env_keys_to_strip(inherited());
            let expected: Vec<OsString> = STRIPPED.iter().map(OsString::from).collect();
            assert_eq!(picked, expected);
        }

        /// `AI_AGENT` 는 Claude Code 가 넣은 값만 지우고 사용자·다른 도구의 값은 남긴다.
        #[test]
        fn ai_agent_is_stripped_only_for_claude_code_values() {
            install_bundled_plugin_rules();
            for v in [
                "claude-code_2-1-291_agent",
                "claude-code_2-1-291_harness",
                "claude-code/1.0",
            ] {
                assert!(
                    is_stripped_inherited_env("AI_AGENT", OsStr::new(v)),
                    "{v} 는 지워야 한다"
                );
            }
            for v in ["my-agent", "codex", "", "x-claude-code_"] {
                assert!(
                    !is_stripped_inherited_env("AI_AGENT", OsStr::new(v)),
                    "{v} 는 남겨야 한다"
                );
            }
        }

        /// 제품 키는 그 제품 플러그인의 선언으로만 지워진다. 한 플러그인만 활성이면 다른
        /// 플러그인의 키는 남는다(비활성·미설치 플러그인의 선언은 적용되지 않는다).
        #[test]
        fn each_product_key_belongs_to_its_own_plugin_only() {
            let rules = bundled_plugin_rules();
            let claude = &rules["com.tasty.claude"];
            let codex = &rules["com.tasty.codex"];
            for key in STRIPPED.iter().filter(|k| **k != "CMUX_SOCKET_PATH") {
                let v = OsStr::new(super::value_for(key));
                let by_codex = codex.matches(key, v);
                let by_claude = claude.matches(key, v);
                assert!(
                    by_claude ^ by_codex,
                    "{key}: claude={by_claude} codex={by_codex}"
                );
                assert_eq!(by_codex, *key == "CODEX_THREAD_ID", "{key}");
            }
            for key in KEPT {
                let v = OsStr::new(super::value_for(key));
                assert!(!claude.matches(key, v) && !codex.matches(key, v), "{key}");
            }
        }
    }
}

/// agent provider 자격 판정이 읽는 namespace 소유 표의 시험용 고정값.
/// 한 시험 바이너리의 시험들이 공용 표를 공유하므로 모두 같은 표를 발행한다.
pub mod namespace_owners {
    use std::collections::{BTreeMap, BTreeSet};
    use tasty_utils::plugin_namespaces::{NamespaceOwner, publish};

    /// 자격이 있는 provider. 번들 Claude·Codex 와 임의 이름의 커스텀 플러그인.
    pub const READY: &[&str] = &["claude", "codex", "foo"];
    /// 소유 플러그인이 비활성인 provider.
    pub const DISABLED: &str = "off";
    /// 소유 플러그인에 `agent.turn_report` 허가가 없는 provider.
    pub const NO_PERMISSION: &str = "noperm";

    fn owner(plugin_id: &str, enabled: bool, permissions: &[&str]) -> NamespaceOwner {
        NamespaceOwner {
            plugin_id: plugin_id.to_string(),
            enabled,
            permissions: permissions
                .iter()
                .map(|p| p.to_string())
                .collect::<BTreeSet<_>>(),
        }
    }

    pub fn publish_fixture() {
        let mut table = BTreeMap::new();
        for provider in READY {
            table.insert(
                provider.to_string(),
                owner(
                    &format!("com.example.{provider}"),
                    true,
                    &["agent.turn_report"],
                ),
            );
        }
        table.insert(
            DISABLED.into(),
            owner("com.example.off", false, &["agent.turn_report"]),
        );
        table.insert(
            NO_PERMISSION.into(),
            owner("com.example.noperm", true, &["agent"]),
        );
        publish(table);
    }
}
