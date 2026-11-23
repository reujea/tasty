//! 사용자 ssh config에서 가져오기 후보 Host alias를 읽는다. 시스템 전역 설정은 직접 읽지 않는다.
//! 프로세스를 실행하지 않으므로 ssh -G의 Match exec 명령도 실행하지 않는다.
//! HostName·User·Port는 해당 블록의 표시용 정보이며 최종 접속 설정을 재현하지 않는다.
//! 파일 부재는 debug, 다른 읽기 실패는 warn으로 기록하고 읽히는 설정만 열거한다.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use tasty_utils::path::os_home_dir;

use crate::profile::{RemoteProfile, RemoteProfiles};

/// Include 재귀 깊이 상한.
const MAX_INCLUDE_DEPTH: usize = 16;

/// 발견한 Host alias와 같은 블록의 표시용 정보. Host */Match 상속을 계산하지 않으며
/// 이 정보를 연결 프로필에 복사하지 않는다. 실제 설정 해석은 접속 시 ssh가 맡는다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SshConfigHost {
    /// 접속 가능한 리터럴 alias (와일드카드·부정 패턴은 애초에 수집되지 않는다).
    pub alias: String,
    /// 이 alias 가 적힌 파일. `Include` 로 갈라진 설정에서 출처를 되짚을 수 있어야 한다.
    pub source: PathBuf,
    /// 같은 블록의 `HostName` (표시 전용).
    pub hostname: Option<String>,
    /// 같은 블록의 `User` (표시 전용).
    pub user: Option<String>,
    /// 같은 블록의 `Port` (표시 전용).
    pub port: Option<u16>,
}

/// 사용자 홈의 .ssh/config 경로. 시스템 설정이나 명령별 override를 열거하는 API는 아니다.
pub fn user_config_path() -> Option<PathBuf> {
    os_home_dir().map(|home| home.join(".ssh").join("config"))
}

/// 최상위 설정 파일의 존재·열기 가능 여부. Include 파일은 별도로 검사하지 않는다.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ConfigAvailability {
    /// 그 경로에 무언가 있는가(정규 파일이 아니어도 true).
    pub exists: bool,
    /// 정규 파일이고 UTF-8 텍스트로 끝까지 읽히는지. 설정 문법은 검사하지 않는다.
    pub readable: bool,
}

/// 주어진 경로의 [`ConfigAvailability`]. `None`(홈을 못 찾음)이면 둘 다 false.
pub fn config_availability(path: Option<&Path>) -> ConfigAvailability {
    let Some(p) = path else {
        return ConfigAvailability::default();
    };
    ConfigAvailability {
        exists: p.exists(),
        // 디렉터리도 open에 성공할 수 있어 정규 파일 여부를 함께 확인한다.
        // UTF-8이 아니면 열거가 0건이 되므로 "호스트 없음"이 아니라 읽기 실패로 본다.
        readable: p.is_file() && std::fs::read_to_string(p).is_ok(),
    }
}

/// 기본 user config 에서 alias 를 열거한다. 홈을 못 찾거나 파일이 없으면 빈 목록.
pub fn enumerate_hosts() -> Vec<SshConfigHost> {
    let Some(path) = user_config_path() else {
        tracing::warn!("ssh_config: cannot resolve home directory — skipping enumeration");
        return Vec::new();
    };
    enumerate_hosts_at(&path)
}

/// 주어진 config 파일에서 alias 를 열거한다(테스트 주입 지점).
///
/// 상대 Include는 최상위 config의 디렉터리를 기준으로 해석한다.
pub fn enumerate_hosts_at(path: &Path) -> Vec<SshConfigHost> {
    let base_dir = path.parent().map(Path::to_path_buf).unwrap_or_default();
    let mut state = Scan {
        base_dir,
        visited: HashSet::new(),
        seen: HashSet::new(),
        hosts: Vec::new(),
    };
    state.read_file(path, 0);
    state.hosts
}

/// 파싱 진행 상태. `Include` 재귀가 같은 목록·같은 중복 판정을 공유해야 하므로
/// 파일 단위가 아니라 스캔 단위로 들고 다닌다.
struct Scan {
    /// 상대 `Include` 경로의 기준 디렉토리(= 루트 config 의 디렉토리).
    base_dir: PathBuf,
    /// 이미 읽은 파일(순환 방지). 정규화 실패 시 원본 경로로 대신 기록한다.
    visited: HashSet<PathBuf>,
    /// 이미 수집한 alias(중복 제거 — 첫 등장 우선).
    seen: HashSet<String>,
    hosts: Vec<SshConfigHost>,
}

impl Scan {
    fn read_file(&mut self, path: &Path, depth: usize) {
        if depth > MAX_INCLUDE_DEPTH {
            tracing::warn!(
                path = %path.display(),
                MAX_INCLUDE_DEPTH,
                "ssh_config: Include nested too deep — stopping here"
            );
            return;
        }
        let key = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        if !self.visited.insert(key) {
            // 순환(a → b → a)이나 같은 파일 중복 Include. 이미 읽었으므로 조용히 끝낸다.
            return;
        }
        let Some(text) = read_text(path) else {
            return;
        };
        self.parse(&text, path, depth);
    }

    fn parse(&mut self, text: &str, path: &Path, depth: usize) {
        // 현재 Host 블록이 만든 항목들의 인덱스. hint 는 이 항목들에만 붙는다.
        let mut block: Vec<usize> = Vec::new();

        for line in text.lines() {
            let Some((keyword, args)) = split_line(line) else {
                continue;
            };
            match keyword.as_str() {
                "host" => {
                    // Match 이후의 Host도 새 블록으로 수집한다. 실제 접속 조건은 ssh가 해석한다.
                    block.clear();
                    for token in args {
                        if !is_literal_alias(&token) {
                            continue;
                        }
                        if !self.seen.insert(token.clone()) {
                            continue; // 첫 등장 우선 — 뒤 파일의 같은 alias 는 버린다.
                        }
                        block.push(self.hosts.len());
                        self.hosts.push(SshConfigHost {
                            alias: token,
                            source: path.to_path_buf(),
                            hostname: None,
                            user: None,
                            port: None,
                        });
                    }
                }
                "match" => {
                    // Match 블록의 설정은 어느 Host 소속도 아니다 — 컨텍스트만 끊는다.
                    // (다음 `Host` 가 새 컨텍스트를 열 때까지 hint 는 갈 곳이 없다.)
                    block.clear();
                }
                "hostname" | "user" | "port" => {
                    let Some(value) = args.into_iter().next() else {
                        continue;
                    };
                    self.apply_hint(&block, &keyword, &value, path);
                }
                "include" => {
                    for arg in args {
                        for file in self.resolve_include(&arg) {
                            self.read_file(&file, depth + 1);
                        }
                    }
                }
                _ => {}
            }
        }
    }

    /// 현재 블록이 만든 항목들에 표시용 hint 를 채운다. ssh 는 "먼저 나온 값이 이긴다"
    /// 이므로 이미 채워진 자리는 덮지 않는다.
    fn apply_hint(&mut self, block: &[usize], keyword: &str, value: &str, path: &Path) {
        for &i in block {
            let host = &mut self.hosts[i];
            match keyword {
                "hostname" => {
                    if host.hostname.is_none() {
                        host.hostname = Some(value.to_string());
                    }
                }
                "user" => {
                    if host.user.is_none() {
                        host.user = Some(value.to_string());
                    }
                }
                _ => {
                    if host.port.is_none() {
                        match value.parse::<u16>() {
                            Ok(p) => host.port = Some(p),
                            Err(e) => tracing::warn!(
                                path = %path.display(),
                                value,
                                error = %e,
                                "ssh_config: unparsable Port — hint left empty"
                            ),
                        }
                    }
                }
            }
        }
    }

    /// `Include` 인자 하나를 실제 파일 목록으로 푼다 — `~` 확장 + 상대경로 해석 + glob.
    fn resolve_include(&self, arg: &str) -> Vec<PathBuf> {
        let expanded = expand_tilde(arg);
        let full = if expanded.is_absolute() {
            expanded
        } else {
            self.base_dir.join(expanded)
        };
        expand_glob(&full)
    }
}

/// 파일을 읽는다. 부재는 debug, 다른 읽기 실패는 warn으로 남기고 None을 반환한다.
fn read_text(path: &Path) -> Option<String> {
    match std::fs::read_to_string(path) {
        Ok(t) => Some(t),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            tracing::debug!(path = %path.display(), "ssh_config: no such file — skipped");
            None
        }
        Err(e) => {
            tracing::warn!(path = %path.display(), error = %e, "ssh_config: unreadable — skipped");
            None
        }
    }
}

/// 접속에 쓸 수 있는 리터럴 alias 인지. 와일드카드(`*` `?`)나 부정(`!`)이 섞인 토큰은
/// 이름이 아니라 패턴이라 열거 대상이 아니다.
fn is_literal_alias(token: &str) -> bool {
    !token.is_empty() && !token.contains(['*', '?', '!'])
}

fn expand_tilde(arg: &str) -> PathBuf {
    let rest = arg
        .strip_prefix("~/")
        .or_else(|| arg.strip_prefix("~\\"))
        .or_else(|| if arg == "~" { Some("") } else { None });
    match (rest, os_home_dir()) {
        (Some(rest), Some(home)) => home.join(rest),
        // ~user 형태는 확장하지 않는다.
        _ => PathBuf::from(arg),
    }
}

/// glob 전개 — 컴포넌트 단위로 디렉토리를 훑는다. `*` 와 `?` 만 지원한다(OpenSSH 가
/// 쓰는 glob(3) 의 부분집합). 와일드카드가 없으면 경로 자체를 그대로 돌려주므로
/// 존재 판정은 호출 측(읽기 실패 → warn)이 맡는다.
fn expand_glob(pattern: &Path) -> Vec<PathBuf> {
    let has_wildcard = pattern
        .components()
        .any(|c| c.as_os_str().to_string_lossy().contains(['*', '?']));
    if !has_wildcard {
        return vec![pattern.to_path_buf()];
    }

    let mut current: Vec<PathBuf> = vec![PathBuf::new()];
    for comp in pattern.components() {
        let part = comp.as_os_str().to_string_lossy().to_string();
        if !part.contains(['*', '?']) {
            for p in &mut current {
                p.push(&part);
            }
            continue;
        }
        let mut next = Vec::new();
        for dir in &current {
            // 빈 경로는 현재 디렉토리를 뜻하지만, Include 경로는 항상 base_dir 이나
            // 절대경로에서 시작하므로 여기 오면 읽을 게 없다.
            let read_dir = if dir.as_os_str().is_empty() {
                continue;
            } else {
                std::fs::read_dir(dir)
            };
            let entries = match read_dir {
                Ok(e) => e,
                Err(e) => {
                    tracing::warn!(dir = %dir.display(), error = %e, "ssh_config: Include glob directory unreadable");
                    continue;
                }
            };
            let mut matched: Vec<PathBuf> = entries
                .filter_map(|entry| match entry {
                    Ok(entry) => Some(entry),
                    Err(e) => {
                        tracing::warn!(dir = %dir.display(), error = %e, "ssh_config: Include glob entry unreadable");
                        None
                    }
                })
                .filter(|entry| glob_match(&part, &entry.file_name().to_string_lossy()))
                .map(|entry| entry.path())
                .collect();
            // read_dir 순서는 OS 마다 다르다 — 같은 설정이 매번 같은 순서를 내도록 고정한다.
            matched.sort();
            next.extend(matched);
        }
        current = next;
    }
    current
}

/// 별표와 물음표만 지원하는 glob. 문자 클래스는 지원하지 않는다.
fn glob_match(pattern: &str, name: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let n: Vec<char> = name.chars().collect();
    // 표준 반복형 백트래킹 — `*` 의 마지막 위치를 기억했다가 실패 시 한 칸 물린다.
    let (mut pi, mut ni) = (0usize, 0usize);
    let (mut star, mut backtrack) = (None, 0usize);
    while ni < n.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == n[ni]) {
            pi += 1;
            ni += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star = Some(pi);
            backtrack = ni;
            pi += 1;
        } else if let Some(s) = star {
            pi = s + 1;
            backtrack += 1;
            ni = backtrack;
        } else {
            return false;
        }
    }
    p[pi..].iter().all(|&c| c == '*')
}

/// 설정 한 줄을 `(소문자 키워드, 인자들)` 로 자른다. 인자가 없거나 주석·빈 줄이면 `None`.
///
/// ssh_config(5) 문법: 키워드는 대소문자 무시, 키워드와 첫 인자는 공백 **또는** `=` 로
/// 구분(`Host=foo`), `#` 이후는 주석, 인자는 따옴표로 감쌀 수 있다.
fn split_line(line: &str) -> Option<(String, Vec<String>)> {
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0;
    while i < chars.len() && chars[i].is_whitespace() {
        i += 1;
    }
    if i >= chars.len() || chars[i] == '#' {
        return None;
    }
    let start = i;
    while i < chars.len() && !chars[i].is_whitespace() && chars[i] != '=' {
        i += 1;
    }
    let keyword: String = chars[start..i].iter().collect::<String>().to_lowercase();

    // 키워드와 첫 인자 사이의 `=` 는 한 번만 구분자로 인정한다. 인자 안의 `=`
    // (`Include conf=d/x`)까지 자르면 경로가 깨진다.
    while i < chars.len() && chars[i].is_whitespace() {
        i += 1;
    }
    if i < chars.len() && chars[i] == '=' {
        i += 1;
    }

    let mut args = Vec::new();
    let mut cur = String::new();
    let mut quote: Option<char> = None;
    while i < chars.len() {
        let c = chars[i];
        i += 1;
        match quote {
            Some(q) if c == q => quote = None,
            Some(_) => cur.push(c),
            None if c == '"' || c == '\'' => quote = Some(c),
            // 따옴표 밖의 `#` 부터는 주석이다.
            None if c == '#' => break,
            None if c.is_whitespace() => {
                if !cur.is_empty() {
                    args.push(std::mem::take(&mut cur));
                }
            }
            None => cur.push(c),
        }
    }
    if !cur.is_empty() {
        args.push(cur);
    }
    if args.is_empty() {
        return None;
    }
    Some((keyword, args))
}

/// 가져오기 거부 사유. 사용자 안내 문구는 CLI·GUI·IPC 호출자가 만든다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportError {
    /// 로컬 ssh config 에 그런 alias 가 없다(오타 방지).
    UnknownAlias(String),
    /// 같은 이름의 프로필이 있어 덮어쓰지 않는다.
    NameTaken(String),
}

/// alias만 host에 넣은 ssh 프로필을 만든다. 표시용 hint를 복사하지 않아 이후 ssh config 변경을 따른다.
/// shell은 auto로 두고 접속·셸 감지는 수행하지 않는다. 저장은 호출자가 한다.
pub fn prepare_import(
    profiles: &RemoteProfiles,
    hosts: &[SshConfigHost],
    alias: &str,
    name: &str,
    label: Option<String>,
) -> Result<RemoteProfile, ImportError> {
    if !hosts.iter().any(|h| h.alias == alias) {
        return Err(ImportError::UnknownAlias(alias.to_string()));
    }
    if profiles.get(name).is_some() {
        return Err(ImportError::NameTaken(name.to_string()));
    }
    let mut p = RemoteProfile::new(name, "ssh");
    p.set_field("host", alias.to_string());
    p.set_field("shell", "auto".to_string());
    p.label = label;
    Ok(p)
}

/// host 필드가 alias와 정확히 같은 ssh 프로필 이름. user@alias 같은 변형은 해석하지 않는다.
pub fn imported_as<'a>(profiles: &'a RemoteProfiles, alias: &str) -> Option<&'a str> {
    profiles
        .profiles
        .iter()
        .find(|p| p.as_ssh().and_then(|v| v.host()) == Some(alias))
        .map(|p| p.name.as_str())
}

#[cfg(test)]
// 이유: 테스트의 let _ =를 제품 코드의 오류 처리 명부에서 제외한다.
#[allow(clippy::let_underscore_must_use)]
mod tests {
    use super::*;

    /// Host·Match·Include를 함께 가진 열거 시험 입력.
    const MAIN: &str = r#"
Host devbox
    HostName 10.0.0.5
    User maya
    Port 2200

Host *
    ServerAliveInterval 30

Host bastion jump-*
    HostName jump.example.com

Match exec "test -n \"$WORK\""
    Host after-match-host

Include extra.conf
"#;

    const EXTRA: &str = r#"
Host work
    HostName work.internal
Host devbox
    Port 2201
"#;

    /// 고유 임시 디렉터리. 반환한 TempDir을 유지해야 시험 중 경로가 삭제되지 않는다.
    fn tmpdir(name: &str) -> tempfile::TempDir {
        tempfile::Builder::new()
            .prefix(&format!("tasty-ssh-config-{name}-"))
            .tempdir()
            .expect("tempdir")
    }

    fn write(dir: &Path, name: &str, body: &str) -> PathBuf {
        let p = dir.join(name);
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent).expect("parent dir");
        }
        std::fs::write(&p, body).expect("fixture written");
        p
    }

    #[test]
    fn collects_literal_aliases_only() {
        let dir = tmpdir("literal");
        let main = write(dir.path(), "config", MAIN);
        write(dir.path(), "extra.conf", EXTRA);
        let names: Vec<String> = enumerate_hosts_at(&main)
            .into_iter()
            .map(|h| h.alias)
            .collect();
        assert_eq!(names, vec!["devbox", "bastion", "after-match-host", "work"]);
    }

    /// Match에서 표시용 정보의 수집을 끊고 다음 Host에서 다시 시작한다.
    #[test]
    fn match_block_ends_host_context_but_not_collection() {
        let dir = tmpdir("match");
        let main = write(
            dir.path(),
            "config",
            "Host a\n  HostName ha\nMatch host b\n  User inside\nHost after-match\n  User me\n",
        );
        let hosts = enumerate_hosts_at(&main);
        assert_eq!(hosts.len(), 2);
        assert_eq!(hosts[0].alias, "a");
        assert_eq!(hosts[0].user, None);
        assert_eq!(hosts[1].alias, "after-match");
        assert_eq!(hosts[1].user.as_deref(), Some("me"));
    }

    #[test]
    fn include_resolves_relative_to_config_dir() {
        let dir = tmpdir("include-rel");
        let main = write(dir.path(), "config", "Include extra.conf\n");
        write(
            dir.path(),
            "extra.conf",
            "Host work\n  HostName work.internal\n",
        );
        let hosts = enumerate_hosts_at(&main);
        assert_eq!(hosts.len(), 1);
        assert_eq!(hosts[0].alias, "work");
        assert_eq!(hosts[0].source, dir.path().join("extra.conf"));
    }

    #[test]
    fn include_glob_expands_and_is_ordered() {
        let dir = tmpdir("include-glob");
        let main = write(dir.path(), "config", "Include conf.d/*.conf\n");
        write(dir.path(), "conf.d/b.conf", "Host bee\n");
        write(dir.path(), "conf.d/a.conf", "Host ay\n");
        write(dir.path(), "conf.d/skip.txt", "Host nope\n");
        let names: Vec<String> = enumerate_hosts_at(&main)
            .into_iter()
            .map(|h| h.alias)
            .collect();
        assert_eq!(names, vec!["ay", "bee"]);
    }

    #[test]
    fn include_cycle_does_not_hang() {
        let dir = tmpdir("cycle");
        let main = write(dir.path(), "config", "Host root\nInclude a.conf\n");
        write(dir.path(), "a.conf", "Host a\nInclude b.conf\n");
        write(dir.path(), "b.conf", "Host b\nInclude a.conf\n");
        let names: Vec<String> = enumerate_hosts_at(&main)
            .into_iter()
            .map(|h| h.alias)
            .collect();
        assert_eq!(names, vec!["root", "a", "b"]);
    }

    #[test]
    fn missing_config_returns_empty_not_error() {
        let dir = tmpdir("missing");
        let hosts = enumerate_hosts_at(&dir.path().join("nope").join("config"));
        assert!(hosts.is_empty());
    }

    #[test]
    fn duplicate_alias_kept_once_in_first_seen_order() {
        let dir = tmpdir("dup");
        let main = write(
            dir.path(),
            "config",
            "Host devbox\n  Port 2200\nInclude extra.conf\n",
        );
        write(dir.path(), "extra.conf", "Host devbox\n  Port 2201\n");
        let hosts = enumerate_hosts_at(&main);
        assert_eq!(hosts.len(), 1);
        assert_eq!(hosts[0].port, Some(2200));
        assert_eq!(hosts[0].source, main);
    }

    #[test]
    fn keyword_case_and_equals_separator() {
        let dir = tmpdir("syntax");
        let main = write(
            dir.path(),
            "config",
            "host=devbox\n  hostname = 10.0.0.5\nHOST bastion\n  PORT\t2222\n",
        );
        let hosts = enumerate_hosts_at(&main);
        assert_eq!(hosts.len(), 2);
        assert_eq!(hosts[0].alias, "devbox");
        assert_eq!(hosts[0].hostname.as_deref(), Some("10.0.0.5"));
        assert_eq!(hosts[1].alias, "bastion");
        assert_eq!(hosts[1].port, Some(2222));
    }

    #[test]
    fn comments_and_quotes_are_handled() {
        let dir = tmpdir("comment");
        let main = write(
            dir.path(),
            "config",
            "# 전체 주석\nHost devbox # 줄 끝 주석\n  User \"space name\"\n",
        );
        let hosts = enumerate_hosts_at(&main);
        assert_eq!(hosts.len(), 1);
        assert_eq!(hosts[0].alias, "devbox");
        assert_eq!(hosts[0].user.as_deref(), Some("space name"));
    }

    #[test]
    fn hint_captures_only_directives_in_own_block() {
        let dir = tmpdir("hint");
        let main = write(dir.path(), "config", MAIN);
        write(dir.path(), "extra.conf", EXTRA);
        let hosts = enumerate_hosts_at(&main);
        let devbox = &hosts[0];
        assert_eq!(devbox.hostname.as_deref(), Some("10.0.0.5"));
        assert_eq!(devbox.user.as_deref(), Some("maya"));
        assert_eq!(devbox.port, Some(2200));
        let bastion = &hosts[1];
        assert_eq!(bastion.hostname.as_deref(), Some("jump.example.com"));
        assert_eq!(bastion.user, None);
        assert_eq!(bastion.port, None);
    }

    #[test]
    fn unparsable_port_leaves_hint_empty() {
        let dir = tmpdir("port");
        let main = write(dir.path(), "config", "Host devbox\n  Port not-a-number\n");
        let hosts = enumerate_hosts_at(&main);
        assert_eq!(hosts.len(), 1);
        assert_eq!(hosts[0].port, None);
    }

    /// 매번 고유 디렉터리에 같은 입력을 만든다. 열거가 끝날 때까지 TempDir을 유지한다.
    fn fixture_hosts(slot: &str) -> Vec<SshConfigHost> {
        let dir = tmpdir(slot);
        let main = write(dir.path(), "config", MAIN);
        write(dir.path(), "extra.conf", EXTRA);
        enumerate_hosts_at(&main)
    }

    #[test]
    fn import_stores_alias_in_host_field_only() {
        let hosts = fixture_hosts("import-fields");
        let p = prepare_import(&RemoteProfiles::default(), &hosts, "devbox", "my-gpu", None)
            .expect("import prepared");
        assert_eq!(p.kind, "ssh");
        let ssh = p.as_ssh().expect("ssh view");
        assert_eq!(ssh.host(), Some("devbox"));
        assert_eq!(ssh.shell(), "auto");
        assert!(!p.fields.contains_key("user"));
        assert!(!p.fields.contains_key("port"));
        assert!(!p.fields.contains_key("extra_options"));
        assert!(p.passkey_ref.is_none());
        assert_eq!(p.fields.len(), 2, "host + shell 만 저장한다");
    }

    #[test]
    fn import_keeps_optional_label() {
        let hosts = fixture_hosts("import-label");
        let p = prepare_import(
            &RemoteProfiles::default(),
            &hosts,
            "devbox",
            "my-gpu",
            Some("GPU box".into()),
        )
        .expect("import prepared");
        assert_eq!(p.label.as_deref(), Some("GPU box"));
    }

    #[test]
    fn import_rejects_unknown_alias() {
        let hosts = fixture_hosts("import-unknown");
        let err = prepare_import(&RemoteProfiles::default(), &hosts, "nope", "x", None)
            .expect_err("unknown alias rejected");
        assert_eq!(err, ImportError::UnknownAlias("nope".into()));
    }

    #[test]
    fn import_rejects_existing_profile_name() {
        let hosts = fixture_hosts("import-conflict");
        let mut profiles = RemoteProfiles::default();
        profiles.upsert(RemoteProfile::new("my-gpu", "ssh").with_field("host", "other"));
        let err = prepare_import(&profiles, &hosts, "devbox", "my-gpu", None)
            .expect_err("name conflict rejected");
        assert_eq!(err, ImportError::NameTaken("my-gpu".into()));
        assert_eq!(
            profiles
                .get("my-gpu")
                .and_then(|p| p.as_ssh())
                .unwrap()
                .host(),
            Some("other")
        );
    }

    #[test]
    fn imported_as_matches_host_field_literally() {
        let mut profiles = RemoteProfiles::default();
        profiles.upsert(RemoteProfile::new("gpu", "ssh").with_field("host", "devbox"));
        profiles.upsert(RemoteProfile::new("other", "ssh").with_field("host", "maya@devbox"));
        assert_eq!(imported_as(&profiles, "devbox"), Some("gpu"));
        assert_eq!(imported_as(&profiles, "bastion"), None);
    }

    // ── config_availability (존재 / 권한 / 부재) ──────────────────────

    #[test]
    fn config_availability_reports_existing_readable_file() {
        let dir = tmpdir("avail-ok");
        let p = write(dir.path(), "config", "Host a\n");
        let a = config_availability(Some(&p));
        assert!(a.exists);
        assert!(a.readable);
    }

    #[test]
    fn config_availability_reports_missing_file_and_no_home() {
        let dir = tmpdir("avail-missing");
        let a = config_availability(Some(&dir.path().join("nope")));
        assert!(!a.exists);
        assert!(!a.readable);
        assert_eq!(config_availability(None), ConfigAvailability::default());
    }

    /// 존재하지만 열 수 없는 파일을 부재와 구분한다.
    #[cfg(unix)]
    #[test]
    fn config_availability_separates_unreadable_from_missing() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tmpdir("avail-perm");
        let p = write(dir.path(), "config", "Host a\n");
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o000)).expect("chmod 000");
        // root 는 퍼미션을 무시하고 읽는다 — 그 환경에서는 이 케이스를 만들 수 없으니
        // 거짓 실패를 내지 않고 건너뛴다(판정 로직이 아니라 픽스처의 한계다).
        if std::fs::File::open(&p).is_ok() {
            return;
        }
        let a = config_availability(Some(&p));
        assert!(a.exists, "파일은 그대로 있다");
        assert!(!a.readable, "권한이 없으면 readable 이 false 여야 한다");
        // 임시 파일 정리 전에 원래 접근 가능한 권한으로 돌린다.
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o644))
            .expect("소유자라 퍼미션 복구는 실패할 이유가 없다");
    }

    /// open에 성공해도 디렉터리는 읽을 설정 파일로 판정하지 않는다.
    #[test]
    fn config_availability_treats_a_directory_as_unreadable() {
        let dir = tmpdir("avail-dir");
        let as_config = dir.path().join("config");
        std::fs::create_dir(&as_config).expect("디렉토리 생성");
        let a = config_availability(Some(&as_config));
        assert!(a.exists, "경로에 무언가 있긴 하다");
        assert!(
            !a.readable,
            "정규 파일이 아니면 읽을 수 있는 것으로 치지 않는다"
        );
    }

    /// 열리는 정규 파일이어도 UTF-8이 아니면 읽을 수 있는 설정으로 판정하지 않는다.
    #[test]
    fn config_availability_treats_invalid_utf8_as_unreadable() {
        let dir = tmpdir("avail-utf8");
        let p = dir.path().join("config");
        std::fs::write(&p, b"Host a\n\xff\xfe\n").expect("비 UTF-8 픽스처");
        let a = config_availability(Some(&p));
        assert!(a.exists, "파일은 그대로 있다");
        assert!(!a.readable, "디코드에 실패하면 readable 이 false 여야 한다");
    }

    #[test]
    fn glob_match_basics() {
        assert!(glob_match("*.conf", "a.conf"));
        assert!(!glob_match("*.conf", "a.txt"));
        assert!(glob_match("a?c", "abc"));
        assert!(!glob_match("a?c", "ac"));
        assert!(glob_match("*", "anything"));
        assert!(glob_match("a*b*c", "aXXbYYc"));
        assert!(!glob_match("a*b*c", "aXXbYY"));
    }
}
