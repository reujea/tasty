use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant, SystemTime};

#[derive(Debug, Clone)]
pub enum HookCondition {
    /// tick에서 경과 시간을 확인해 반복한다. 정확한 시각의 실행을 예약하는 OS 타이머는 아니다.
    Interval(Duration),
    Once(Duration),
    /// tick에서 파일 mtime 변경을 확인한다.
    File(PathBuf),
}

impl HookCondition {
    /// interval:SECS, once:SECS, file:PATH를 해석한다. file: 뒤의 추가 콜론은 경로에 남긴다.
    pub fn parse(s: &str) -> Option<Self> {
        if let Some(rest) = s.strip_prefix("interval:") {
            let secs: f64 = rest.parse().ok()?;
            Some(HookCondition::Interval(Duration::from_secs_f64(secs)))
        } else if let Some(rest) = s.strip_prefix("once:") {
            let secs: f64 = rest.parse().ok()?;
            Some(HookCondition::Once(Duration::from_secs_f64(secs)))
        } else if let Some(rest) = s.strip_prefix("file:") {
            if rest.is_empty() {
                None
            } else {
                Some(HookCondition::File(PathBuf::from(rest)))
            }
        } else {
            None
        }
    }

    pub fn to_display_string(&self) -> String {
        match self {
            HookCondition::Interval(d) => format!("interval:{}", d.as_secs_f64()),
            HookCondition::Once(d) => format!("once:{}", d.as_secs_f64()),
            HookCondition::File(p) => format!("file:{}", p.display()),
        }
    }
}

#[derive(Debug, Clone)]
pub struct GlobalHook {
    pub id: u32,
    pub condition: HookCondition,
    pub command: String,
    pub label: Option<String>,
}

pub struct GlobalHookManager {
    hooks: HashMap<u32, GlobalHook>,
    /// 여러 engine이 같은 카운터를 사용해 hook ID 충돌을 피한다.
    next_id: Arc<AtomicU32>,
    last_fired: HashMap<u32, Instant>,
    created_at: HashMap<u32, Instant>,
    fired_once: Vec<u32>,
    /// 마지막 mtime. None은 부재뿐 아니라 metadata·수정 시각 조회 실패도 포함한다.
    last_mtime: HashMap<u32, Option<SystemTime>>,
    /// interval·file 훅의 실행 토큰. 접수된 작업이 복제본을 쥐고 있다가 drop될 때 놓는다.
    /// 복제본이 남아 있으면 그 훅의 이전 실행이 아직 끝나지 않은 것이다([`Self::has_pending_run`]).
    run_tokens: HashMap<u32, Arc<()>>,
}

impl GlobalHookManager {
    /// 검사 전용 독립 카운터. 제품 생성 경로는 공유 발급기를 받는다.
    #[cfg(test)]
    pub fn new() -> Self {
        Self::with_counter(Arc::new(AtomicU32::new(0)))
    }

    /// 다른 engine과 공유할 ID 카운터를 받는다.
    pub fn with_counter(next_id: Arc<AtomicU32>) -> Self {
        Self {
            hooks: HashMap::new(),
            next_id,
            last_fired: HashMap::new(),
            created_at: HashMap::new(),
            fired_once: Vec::new(),
            last_mtime: HashMap::new(),
            run_tokens: HashMap::new(),
        }
    }

    pub fn add(&mut self, condition: HookCondition, command: String, label: Option<String>) -> u32 {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed) + 1;
        let now = Instant::now();

        match &condition {
            HookCondition::Interval(_) => {
                self.last_fired.insert(id, now);
                self.run_tokens.insert(id, Arc::new(()));
            }
            HookCondition::Once(_) => {
                self.created_at.insert(id, now);
            }
            HookCondition::File(path) => {
                self.last_mtime.insert(id, file_mtime(path));
                self.run_tokens.insert(id, Arc::new(()));
            }
        }

        self.hooks.insert(
            id,
            GlobalHook {
                id,
                condition,
                command,
                label,
            },
        );
        id
    }

    pub fn remove(&mut self, id: u32) -> bool {
        self.last_fired.remove(&id);
        self.created_at.remove(&id);
        self.last_mtime.remove(&id);
        self.run_tokens.remove(&id);
        self.hooks.remove(&id).is_some()
    }

    /// 이 훅이 제출한 작업이 실행기에 남아 있는지(아직 drop되지 않았는지). 슬롯·대기열에 있는 동안과
    /// 그 작업의 마지막 보고가 끝날 때까지 참이다. once 훅과 없는 id는 거짓이다.
    pub(super) fn has_pending_run(&self, id: u32) -> bool {
        self.run_tokens
            .get(&id)
            .is_some_and(|token| Arc::strong_count(token) > 1)
    }

    /// 제출할 작업에 복제해 넣을 실행 토큰. once 훅과 없는 id는 None이다.
    fn run_token(&self, id: u32) -> Option<&Arc<()>> {
        self.run_tokens.get(&id)
    }

    pub fn list(&self) -> Vec<&GlobalHook> {
        self.hooks.values().collect()
    }

    #[allow(dead_code)]
    pub fn get(&self, id: u32) -> Option<&GlobalHook> {
        self.hooks.get(&id)
    }

    /// 지금 조건을 만족한 요청 목록을 만든다. 실행 전에 시간·once·mtime 기록을 갱신한다.
    /// interval·file 훅은 이전 실행이 남아 있으면([`Self::has_pending_run`]) 목록에서 뺀다. interval은 그 회차의
    /// 시각을 기록하고, file은 mtime 기준을 그대로 둬 이전 실행이 끝난 뒤 다시 감지한다.
    /// 실제 명령 실행 실패를 자동 재시도하지는 않는다.
    pub fn tick(&mut self) -> Vec<(u32, String)> {
        let now = Instant::now();
        let mut to_fire: Vec<(u32, String)> = Vec::new();
        let mut file_mtime_updates: Vec<(u32, Option<SystemTime>)> = Vec::new();
        let mut interval_due: Vec<u32> = Vec::new();

        for (id, hook) in &self.hooks {
            match &hook.condition {
                HookCondition::Interval(period) => {
                    let last = self.last_fired.get(id).copied().unwrap_or(now);
                    if now.duration_since(last) >= *period {
                        interval_due.push(*id);
                        if self.has_pending_run(*id) {
                            log_skipped(*id, &hook.command);
                        } else {
                            to_fire.push((*id, hook.command.clone()));
                        }
                    }
                }
                HookCondition::Once(delay) => {
                    let created = self.created_at.get(id).copied().unwrap_or(now);
                    if now.duration_since(created) >= *delay {
                        to_fire.push((*id, hook.command.clone()));
                        self.fired_once.push(*id);
                    }
                }
                HookCondition::File(path) => {
                    // 크기 제한 없는 파일을 매번 전부 읽지 않도록 mtime만 본다.
                    // 같은 mtime으로 내용이 바뀌거나 시각이 복원되면 변경을 놓칠 수 있다.
                    let current = file_mtime(path);
                    // 조회 실패는 기존 mtime을 유지한다. 같은 mtime으로 다시 생긴 파일도 구분하지 못한다.
                    if let Some(mtime) = current {
                        let last = self.last_mtime.get(id).copied().unwrap_or(None);
                        if last != Some(mtime) {
                            if self.has_pending_run(*id) {
                                log_skipped(*id, &hook.command);
                            } else {
                                to_fire.push((*id, hook.command.clone()));
                                file_mtime_updates.push((*id, Some(mtime)));
                            }
                        }
                    }
                }
            }
        }

        for id in interval_due {
            self.last_fired.insert(id, now);
        }

        let to_remove: Vec<u32> = self.fired_once.drain(..).collect();
        for id in to_remove {
            self.remove(id);
        }

        for (id, mtime) in file_mtime_updates {
            self.last_mtime.insert(id, mtime);
        }

        to_fire
    }
}

/// 건너뜀은 결정한 동작이라 알림 없이 로그만 남긴다.
fn log_skipped(id: u32, command: &str) {
    tracing::debug!("global hook {id} skipped: its previous run has not finished; cmd: {command}");
}

/// 조건을 만족한 전역 훅을 `pool`에 제출한다. 제품은 공유 실행기를 넘긴다.
pub(super) fn run_due(mgr: &mut GlobalHookManager, pool: &Arc<super::shell_pool::ShellPool>) {
    for (id, command) in mgr.tick() {
        submit_command_in(pool, &command, mgr.run_token(id));
    }
}

/// 전역 훅 명령을 실행기에 넣는다. 실행기는 셸이 끝날 때까지 기다리며 종료 코드는 판정하지 않는다.
/// 실행기가 받지 않았거나 셸을 띄우지 못하면 로그와 오류 알림을 남긴다. stdio는 호스트의 것을 물려받는다.
/// 토큰은 접수될 때만(`make` 안에서) 복제한다.
pub(super) fn submit_command_in(
    pool: &Arc<super::shell_pool::ShellPool>,
    command: &str,
    token: Option<&Arc<()>>,
) {
    use super::notice::{self, AutoOrigin};
    if let Err(reason) = pool.submit(|| global_job(command, token.cloned())) {
        tracing::warn!("global hook command not run: {reason}; cmd: {command}");
        notice::shell_not_queued(AutoOrigin::GlobalHook, command, &reason);
    }
}

/// 전역 훅 명령의 실행기 작업. `pending` 토큰은 보고 클로저가 쥔다. build 클로저는 실행 직전에 소비되므로
/// 담지 않는다. 작업이 어느 경로로 끝나든(정상·시작 실패·패닉·고아 보고·전달 실패) drop될 때 토큰이 풀린다.
pub(super) fn global_job(command: &str, pending: Option<Arc<()>>) -> super::shell_pool::ShellJob {
    use super::notice::{self, AutoOrigin};
    let built = command.to_string();
    let reported = command.to_string();
    super::shell_pool::ShellJob::new(
        command,
        move || global_hook_command(&built, tasty_utils::process::process_env_keys_to_strip()),
        move |event| {
            // 이 클로저가 토큰을 쥐게 한다(클로저는 본문에서 쓰는 값만 잡는다).
            let _pending = &pending;
            if let crate::hook_handler::exec::ShellEvent::NotStarted(e) = event {
                notice::not_run(
                    AutoOrigin::GlobalHook,
                    crate::i18n::t_fmt2("hook_notice.spawn_failed", &reported, &e),
                );
            }
        },
    )
}

/// 심볼릭 링크를 따라 metadata의 수정 시각을 읽는다. 실패는 None이다.
/// 디렉터리 경로도 읽을 수 있지만 내부 파일 내용의 변경까지 재귀 추적하지는 않는다.
fn file_mtime(path: &std::path::Path) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

/// 전역 훅 명령을 sh/cmd로 감싼 Command를 만든다. `strip` 키는 상속 환경에서 지운다. 터미널 셸과 같은
/// 판정에 바깥 인스턴스의 호출자 신원을 더한 목록이다(`tasty_utils::process::hook_env_keys_to_strip`).
fn global_hook_command(command: &str, strip: Vec<std::ffi::OsString>) -> std::process::Command {
    #[cfg(windows)]
    let mut cmd = {
        let mut c = std::process::Command::new("cmd");
        c.args(["/C", command]);
        c
    };
    #[cfg(not(windows))]
    let mut cmd = {
        let mut c = std::process::Command::new("sh");
        c.args(["-c", command]);
        c
    };
    for key in strip {
        cmd.env_remove(key);
    }
    cmd
}

#[cfg(all(test, unix))]
#[path = "global_pending_tests.rs"]
mod pending_tests;

#[cfg(test)]
mod tests {
    use super::*;

    /// 전역 훅도 터미널 셸과 같은 판정으로 활성 플러그인이 선언한 세션 키와 CMUX_* 를 지우고,
    /// 바깥 인스턴스의 신원 변수도 지운다. 나머지는 남긴다.
    #[test]
    fn global_hook_strips_claude_session_and_cmux_env() {
        use tasty_test_support::strip_env_keys::{
            KEPT, STRIPPED, inherited, install_bundled_plugin_rules,
        };
        use tasty_utils::process::{OUTER_IDENTITY_ENV, hook_env_keys_to_strip};
        install_bundled_plugin_rules();
        let mut inherited = inherited();
        inherited.extend(
            OUTER_IDENTITY_ENV
                .iter()
                .map(|k| (k.into(), "outer".into())),
        );
        let strip = hook_env_keys_to_strip(inherited);
        let cmd = global_hook_command("true", strip);
        let removed: Vec<_> = cmd
            .get_envs()
            .filter(|(_, v)| v.is_none())
            .map(|(k, _)| k.to_owned())
            .collect();
        for key in STRIPPED.iter().chain(OUTER_IDENTITY_ENV) {
            assert!(
                removed.iter().any(|k| k == std::ffi::OsStr::new(key)),
                "{key} 는 지워져야 한다"
            );
        }
        for key in KEPT.iter().filter(|k| !OUTER_IDENTITY_ENV.contains(k)) {
            assert!(
                !removed.iter().any(|k| k == std::ffi::OsStr::new(key)),
                "{key} 는 남아야 한다"
            );
        }
    }

    #[test]
    fn parse_accepts_interval_and_once() {
        assert!(matches!(
            HookCondition::parse("interval:5"),
            Some(HookCondition::Interval(d)) if d == Duration::from_secs(5)
        ));
        assert!(matches!(
            HookCondition::parse("once:1.5"),
            Some(HookCondition::Once(d)) if d == Duration::from_secs_f64(1.5)
        ));
        assert!(HookCondition::parse("garbage:1").is_none());
    }

    #[test]
    fn tick_does_not_fire_interval_hook_before_period_elapses() {
        let mut mgr = GlobalHookManager::new();
        mgr.add(
            HookCondition::Interval(Duration::from_secs(60)),
            "echo x".to_string(),
            None,
        );
        assert!(mgr.tick().is_empty());
    }

    #[test]
    fn tick_fires_interval_hook_repeatedly_after_each_period() {
        let mut mgr = GlobalHookManager::new();
        let id = mgr.add(
            HookCondition::Interval(Duration::from_millis(10)),
            "echo x".to_string(),
            None,
        );
        std::thread::sleep(Duration::from_millis(15));
        assert_eq!(mgr.tick(), vec![(id, "echo x".to_string())]);
        assert_eq!(mgr.list().len(), 1);
        std::thread::sleep(Duration::from_millis(15));
        assert_eq!(mgr.tick(), vec![(id, "echo x".to_string())]);
    }

    #[test]
    fn tick_fires_once_hook_exactly_once_then_removes_it() {
        let mut mgr = GlobalHookManager::new();
        let id = mgr.add(
            HookCondition::Once(Duration::from_millis(10)),
            "echo once".to_string(),
            None,
        );
        std::thread::sleep(Duration::from_millis(15));
        assert_eq!(mgr.tick(), vec![(id, "echo once".to_string())]);
        assert!(mgr.list().is_empty(), "once 훅은 발화 후 제거되어야 한다");
        assert!(mgr.tick().is_empty());
    }

    #[test]
    fn parse_accepts_file_condition() {
        assert!(matches!(
            HookCondition::parse("file:/tmp/foo.txt"),
            Some(HookCondition::File(p)) if p == std::path::Path::new("/tmp/foo.txt")
        ));
        assert!(matches!(
            HookCondition::parse(r"file:C:\Users\foo\bar.txt"),
            Some(HookCondition::File(p)) if p == std::path::Path::new(r"C:\Users\foo\bar.txt")
        ));
        assert!(HookCondition::parse("file:").is_none());
    }

    #[test]
    fn tick_does_not_fire_file_hook_when_unchanged() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("watched.txt");
        std::fs::write(&path, "v1").unwrap();
        let mut mgr = GlobalHookManager::new();
        mgr.add(
            HookCondition::File(path.clone()),
            "echo x".to_string(),
            None,
        );
        assert!(mgr.tick().is_empty(), "등록 직후 발화 없음");
        assert!(mgr.tick().is_empty(), "변경 없으면 계속 발화 없음");
    }

    #[test]
    fn tick_fires_file_hook_repeatedly_on_each_mtime_change() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("watched.txt");
        std::fs::write(&path, "v1").unwrap();
        let mut mgr = GlobalHookManager::new();
        let id = mgr.add(
            HookCondition::File(path.clone()),
            "echo x".to_string(),
            None,
        );
        assert!(mgr.tick().is_empty());

        std::thread::sleep(Duration::from_millis(10));
        std::fs::write(&path, "v2").unwrap();
        assert_eq!(mgr.tick(), vec![(id, "echo x".to_string())]);
        assert!(mgr.tick().is_empty(), "같은 변경으로 두 번 발화하지 않음");

        std::thread::sleep(Duration::from_millis(10));
        std::fs::write(&path, "v3").unwrap();
        assert_eq!(mgr.tick(), vec![(id, "echo x".to_string())]);
        assert_eq!(mgr.list().len(), 1);
    }

    #[test]
    fn tick_fires_file_hook_when_file_appears_after_being_absent() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("not-yet.txt");
        let mut mgr = GlobalHookManager::new();
        let id = mgr.add(
            HookCondition::File(path.clone()),
            "echo x".to_string(),
            None,
        );
        assert!(mgr.tick().is_empty());

        std::fs::write(&path, "v1").unwrap();
        assert_eq!(
            mgr.tick(),
            vec![(id, "echo x".to_string())],
            "등록 후 파일이 새로 생기면 변경으로 간주해 발화"
        );
        assert!(mgr.tick().is_empty());
    }

    #[test]
    fn tick_treats_deleted_file_as_no_change_and_keeps_hook() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("watched.txt");
        std::fs::write(&path, "v1").unwrap();
        let mut mgr = GlobalHookManager::new();
        let id = mgr.add(
            HookCondition::File(path.clone()),
            "echo x".to_string(),
            None,
        );
        assert!(mgr.tick().is_empty());

        std::fs::remove_file(&path).unwrap();
        assert!(
            mgr.tick().is_empty(),
            "파일 삭제는 변경으로 취급하지 않는다"
        );
        assert_eq!(mgr.list().len(), 1, "훅이 자동 제거되지 않는다");

        std::thread::sleep(Duration::from_millis(10));
        std::fs::write(&path, "v2").unwrap();
        assert_eq!(
            mgr.tick(),
            vec![(id, "echo x".to_string())],
            "파일이 다시 생기면 재감지한다"
        );
    }
}
