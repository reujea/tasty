//! child→caller 완료 알림 로그 — Monitor tool 이 `tail -F` 로 감시하는 append-only 파일.
//!
//! 배경: child(claude/codex)가 끝났을 때 caller(conductor)에게 완료를 알리는 원래
//! 경로는 `terminal.tell` 로 PTY 에 텍스트를 강제 주입하는 것뿐이었다. 이 방식은
//! **caller 세션이 busy(다른 turn 생성 중)일 때 주입이 씹힌다** — conductor 가 무거운
//! 작업을 도는 동안 완료 알림을 놓치는 사고가 실제로 있었다. 게다가 caller 가 Claude
//! Code CLI 세션이면 주입된 텍스트가 **사람이 직접 타이핑한 발화와 구분되지 않는
//! 형태**로 대화 트랜스크립트에 섞여 들어간다.
//!
//! 처방(현재 유일 경로): 완료 이벤트를 **파일에 한 줄씩 append** 한다. conductor 는 dispatch
//! 직후 한 번 `Monitor({ command: "tail -n0 -F <path>", persistent: true })` 로 arm 하면,
//! 이후 child 완료마다 append 된 라인이 background-task notification 으로 **idle 세션도
//! 깨워** 다음 턴에 전달된다. 이 파일 채널이 안정적으로 검증된 뒤 완료-알림 경로에서
//! `terminal.tell` 주입은 **제거**했다(위 위장 발화 부작용 때문) — completion-log 가
//! 완료 알림의 유일한 채널이다.
//!
//! 경로 규약: `<parent_home>/notify/<caller_surface>.log`. host 는 자식(plugin
//! 서브프로세스 = writer, conductor 셸 = reader) 양쪽에 자기 데이터 루트를
//! **`TASTY_PARENT_HOME`** env 로 주입한다. [`notify_log_path`] 는 이 값을 최우선으로
//! 보고, 없을 때만 [`crate::path::tasty_home`] 으로 fallback 하므로 writer/reader 의
//! 경로가 항상 일치한다.
//!
//! `TASTY_HOME`(= `tasty_home()` 의 override 1순위)이 아니라 별도 이름을 쓰는 이유:
//! 정보성 부모-루트 값을 `TASTY_HOME` 으로 주입하면 release 터미널 안에서 실행된 debug
//! 빌드가 그 값을 자기 데이터 루트 override 로 오인해 `~/.tasty-debug` 격리가 깨지고
//! release 의 포트파일까지 덮어쓰는 사고가 났다. self-determination(`TASTY_HOME`)과
//! broadcast(`TASTY_PARENT_HOME`)를 환경변수 이름으로 분리한다.
//!
//! # 보존 범위 — 이 로그가 답할 수 있는 물음의 크기
//!
//! 이 파일은 되감을 수 있는 기록이 아니다. 무엇이 남는지를 세 축으로 못박는다.
//!
//! - **정체성** — 한 로그의 정체성은 경로 그 자체, 즉 **(데이터 루트, caller surface
//!   id)** 다. 그 외에 인스턴스를 가리키는 표식은 줄에도 파일에도 없다. 데이터 루트는
//!   `TASTY_PARENT_HOME`(없으면 [`crate::path::tasty_home`])이 정하므로, 그 값이 다르면
//!   **같은 surface 번호라도 다른 로그**이고 같으면 같은 로그다. writer 가 그 env 없이
//!   돌면 자기 홈으로 떨어져 reader 와 조용히 갈린다 — `resolve_home` 의 fallback 이
//!   그 자리다.
//! - **세대** — surface id 는 호스트 실행마다 1 부터 다시 발급된다. 그래서 이전 실행이
//!   남긴 파일과 이번 실행의 파일은 **이름이 겹친다.** 겹침을 막는 것은 파일 안의
//!   표식이 아니라 **호스트가 부팅 때 `notify/` 를 통째로 지우는 것** 하나다. 즉 이
//!   로그의 보존 범위는 **지금 호스트 세대 하나**이고, 재시작을 사이에 두고 과거 줄을
//!   되읽을 방법은 없다. 그 삭제가 데이터 루트 단위라는 점이 곧 전제다 — **한 데이터
//!   루트에 호스트 하나.** 둘이 같은 루트로 뜨면 나중 것이 먼저 것의 살아 있는 로그를
//!   지운다. 단 포트 파일을 데이터 루트 **밖**으로 옮긴 호스트는 지우지 않는다 — 그
//!   판정은 호스트 쪽 `TcpIpcServer::notify_dir_to_clear` 에 있다(ADR-0416). 메타 파일도
//!   같은 디렉토리라 함께 지워지고, 새 세대의 `retention_start` 는 0 부터 다시 센다.
//! - **크기** — 축은 **바이트 하나**다(`NOTIFY_LOG_CAP_BYTES`). 시간 상한도, 파일 수
//!   상한도 없다. 닫힌 surface 의 파일은 그 세대가 끝날 때까지 남는다(회수는 다음 부팅
//!   삭제뿐). 그리고 cap 은 "마지막 256 KiB 를 남긴다" 가 아니라 **"넘으면 전량
//!   버린다"** 다 — 남는 양은 0 과 cap 사이를 톱니로 오간다.
//!
//! 버리는 쪽은 **숨기지 않는다.** 비울 때마다 버린 바이트 수를 로그(`tracing`)로
//! 남긴다 — 이 파일 자신에는 안 쓴다. 읽는 쪽 계약이 "한 줄 = 완료 통지" 라 메타 줄을
//! 끼우면 그것이 완료로 읽히기 때문이다(ADR-0330 이 그 대안을 기각한 자리). 근거·측정·
//! 재검토 조건은 `docs/adr/0344-the-completion-log-keeps-one-host-generation-and-says-what-it-threw-away.md`.
//!
//! # 독자 복구 — 줄 밖 메타 `<caller_surface>.log.meta`
//!
//! 멈췄다 재개하는 reader 는 "그 사이 무엇을 잃었나" 를 물을 수 있어야 한다. 그 값을 파일
//! 안에 둘 수 없으므로(위 문단) **옆 파일**에 둔다. 메타 파일은 `key=value` 줄이고 지금
//! 키는 하나다.
//!
//! - **`retention_start`** — 이 로그가 세대 시작부터 **버린 바이트 누계**. 달리 말해 지금
//!   파일의 첫 바이트가 세대 전체 스트림에서 몇 번째 바이트인가다. 비울 때마다 버린 양만큼
//!   늘고 줄지 않는다. 메타 파일이 없거나 비었거나 키가 없으면 0 이다(아직 한 번도 안
//!   비웠다).
//!
//! reader 는 **논리 오프셋** `next_offset`(= `retention_start` + 파일 안 위치)을 들고 있다가
//! 재개할 때 견준다 — `next_offset < retention_start` 면 그 사이 비우기가 있었고
//! (`truncated`), `retention_start - next_offset` 바이트를 못 읽고 잃었다(`skipped`). 그때는
//! 파일 처음부터 읽는다. 어휘는 `events.fetch` · `surface.read_since_mark` 의 것을 빌렸고
//! 저장소는 공유하지 않는다. 계약 전문·잠금 규약·대안은
//! `docs/adr/0415-a-resuming-completion-log-reader-learns-what-it-lost-from-a-sidecar.md`.
//!
//! 이 수가 **정확**하려면 비우기가 append 와 겹치면 안 된다. 그래서 메타 파일에 advisory
//! 잠금을 건다 — append 는 공유, 비우기(크기 재기 · 비우기 · 누계 갱신)는 배타. 배타는
//! 기다리지 않는다: 유한 시간만 시도하고, 못 잡으면 잠금 없이 비우되 누계는 안 올린다
//! (공유 잠금을 쥔 reader 가 멈춰도 완료 통지가 서지 않게 한다). 잠금
//! 대상이 로그가 아니라 메타인 이유는 Windows 의 잠금이 강제형이라 로그에 걸면 다른
//! writer 의 append 가 막히기 때문이다. 줄 형식과 경로는 안 바뀌므로 `tail -n0 -F` 소비자는
//! 이 파일을 몰라도 된다.
//!
//! 라인 포맷은 완료 메시지 한 줄로 둔다 — 예: `surface 42 작업 완료 (호출 방식: spawn)`.
//! 호출 방식(spawn/tell)을 문장 맨 앞에 두지 않는 이유는 `crates/tasty-plugin-claude`/
//! `tasty-plugin-codex` 의 `notify_done_message`/`notify_caller_message` 주석 참조 —
//! "{command} 완료" 형태는 명령 자체가 끝났다는 뜻으로 오독되기 쉬웠다.

use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// 완료 로그 파일의 크기 상한. 무한 성장을 막는 **유일한** 축이다 — 시간 상한도
/// 파일 수 상한도 없다(모듈 문서 "보존 범위").
///
/// 도달하면 남기는 것이 아니라 **전량 버린다**: "마지막 256 KiB 를 보존" 이 아니라
/// "256 KiB 에서 0 으로 되돌린다" 이므로, 실제 보존량은 0 과 이 값 사이를 오간다.
/// 완료 라인은 ~40 bytes 이므로 256 KiB ≈ 수천 개 이벤트분 — 한 세션에서 도달하기
/// 어렵지만, surface_id 가 세션 간 재사용되며 파일이 계속 누적되는 것을 방어한다.
const NOTIFY_LOG_CAP_BYTES: u64 = 256 * 1024;

/// caller_surface 별 완료 로그 파일 경로.
///
/// host 가 자식에 주입한 `TASTY_PARENT_HOME`(정보성 부모 루트)이 있으면 그 값을
/// 최우선으로 홈으로 쓰고, 없으면 [`crate::path::tasty_home`] 으로 fallback 한다.
/// writer(plugin)/reader(conductor) 양쪽 다 `TASTY_PARENT_HOME` 을 받으므로 이 함수가
/// 그걸 최우선으로 봐야 두 경로가 일치한다. 홈 해석 실패 시 `None`.
pub fn notify_log_path(caller_surface: u32) -> Option<PathBuf> {
    resolve_home(
        std::env::var("TASTY_PARENT_HOME").ok(),
        crate::path::tasty_home,
    )
    .map(|home| notify_log_path_in(&home, caller_surface))
}

/// 홈 루트 선택 로직(순수 — env 접근 없이 테스트 가능). `TASTY_PARENT_HOME` 값이
/// 비어있지 않으면 그것을, 아니면 `fallback`(보통 `tasty_home()`)을 쓴다. `fallback` 은
/// parent 가 없을 때만 호출하는 클로저라 불필요한 홈 해석을 피한다.
fn resolve_home(
    parent_env: Option<String>,
    fallback: impl FnOnce() -> Option<PathBuf>,
) -> Option<PathBuf> {
    if let Some(raw) = parent_env {
        let trimmed = raw.trim();
        if !trimmed.is_empty() {
            return Some(PathBuf::from(trimmed));
        }
    }
    fallback()
}

/// 순수 경로 조립(파일시스템 접근 없음) — 단위 테스트 대상.
fn notify_log_path_in(home: &Path, caller_surface: u32) -> PathBuf {
    home.join("notify").join(format!("{caller_surface}.log"))
}

/// 완료 로그 옆의 메타 파일 경로 — `<log>.meta`. 로그 경로에서 순수하게 만든다.
fn notify_meta_path(log: &Path) -> PathBuf {
    let mut raw = log.as_os_str().to_owned();
    raw.push(".meta");
    PathBuf::from(raw)
}

/// 메타 파일에서 버린 바이트 누계를 담는 키(모듈 문서 "독자 복구").
const RETENTION_START_KEY: &str = "retention_start";

/// 누계 값의 고정 폭. `u64` 의 최대 자릿수(20)다. 값을 **왼쪽 정렬 + 공백 채움**으로 늘
/// 같은 폭에 쓰므로 메타 파일은 한 번 쓰이면 길이가 안 변한다 — 제자리 덮어쓰기가
/// 파일을 줄이는 단계 없이 한 번의 `write` 로 끝나, 잠금을 안 거는 reader 도 빈 파일을
/// 볼 일이 없다. 0 채움이 아니라 공백 채움인 이유는 셸 산술(`$((…))`)이 앞자리 0 을
/// 8 진수로 읽기 때문이다.
const RETENTION_START_WIDTH: usize = 20;

/// 메타 파일 한 벌의 내용. 파싱은 관대하다 — 모르는 키는 건너뛰고, 값의 앞뒤 공백을
/// 벗기며, 못 읽으면 0(아직 안 비웠다)으로 본다.
fn parse_retention_start(meta: &str) -> u64 {
    meta.lines()
        .filter_map(|l| l.split_once('='))
        .find(|(k, _)| k.trim() == RETENTION_START_KEY)
        .and_then(|(_, v)| v.trim().parse().ok())
        .unwrap_or(0)
}

fn format_retention_start(value: u64) -> String {
    format!("{RETENTION_START_KEY}={value:<RETENTION_START_WIDTH$}\n")
}

/// caller_surface 의 완료 로그에 한 줄 append 한다(개행 자동 부가). `notify/` 디렉토리는
/// 없으면 생성한다. best-effort — 호출자는 실패 시 `tracing::warn!` 로 흘려보내고 기존
/// `terminal.tell` 알림 경로에는 영향을 주지 않는다.
pub fn append_notify_line(caller_surface: u32, line: &str) -> io::Result<()> {
    let path = notify_log_path(caller_surface)
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "tasty_home() unavailable"))?;
    append_line_to(&path, line, NOTIFY_LOG_CAP_BYTES)
}

/// 실제 append/truncate 로직 — 경로·cap 을 명시로 받아 env 조작 없이 테스트 가능.
///
/// # 이 파일에는 writer 가 여럿이다
///
/// 한 caller 밑에 claude child 와 codex child 가 함께 뜨면 **서로 다른 두 프로세스**가
/// 같은 `<caller_surface>.log` 에 쓴다. 그래서 이 함수의 불변식은 "한 줄은 통째로
/// 남거나 통째로 없다" 이고, 그것을 두 가지로 지킨다.
///
/// - **핸들은 항상 append 다.** 쓰기 핸들에 `truncate` 를 섞지 않는다. 섞으면 그
///   핸들은 offset 0 부터 쓰므로 다른 writer 가 방금 append 한 줄의 앞부분을 덮어
///   **양쪽 모두 아닌 잔해**를 남긴다.
/// - **한 줄은 한 번의 `write` 로 보낸다.** `writeln!` 은 포맷 조각마다 write 를
///   나눠 보낼 수 있고, 그 사이에 다른 프로세스의 append 가 끼면 두 줄이 섞인다.
///   `O_APPEND` 가 보장하는 것은 **한 번의 write** 가 끝에 통째로 붙는 것뿐이다.
///
/// 비우는 일은 쓰기와 **분리한다.** 판정과 실행 사이에 다른 writer 가 이미 비웠을 수
/// 있어 같은 핸들로 한 번 더 재고 비운다. 재기와 비우기는 메타 파일의 **배타** 잠금
/// 아래에서, append 는 **공유** 잠금 아래에서 하므로 둘이 겹치지 않는다 — 비우기가 잰
/// 크기가 곧 버린 양이고, 그 수가 `retention_start` 에 더해진다. 배타 잠금을 유한 시간
/// 안에 못 잡거나 잠금을 모르는 writer(메타 이전 버전)가 비우면 그 비우기는 누계에 안
/// 잡힌다 — 재개 reader 에게 "모른다" 로 보인다. 그때도 사라지는 것은 줄 단위이고 줄
/// 중간이 잘리지는 않는다.
fn append_line_to(path: &Path, line: &str, cap: u64) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let meta_path = notify_meta_path(path);
    let meta = open_meta(&meta_path);
    // 기존 파일이 cap 이상이면 새로 시작. `tail -F` 는 파일 축소를 감지해 재오픈하므로
    // arm 된 Monitor 가 그 뒤 append 된 라인을 계속 받는다.
    if std::fs::metadata(path)
        .map(|m| m.len() >= cap)
        .unwrap_or(false)
    {
        truncate_and_account(path, cap, meta.as_ref(), &meta_path);
    }
    // append 는 **공유** 잠금 아래에서 한다. 비우기는 배타 잠금을 잡으므로, 비우기가 크기를
    // 잰 뒤와 비우기 사이에 이 줄이 끼어 **누계에 안 잡힌 채 사라지는** 일이 없다.
    let _shared = meta.as_ref().and_then(|m| MetaLock::shared(m, &meta_path));
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    let mut record = String::with_capacity(line.len() + 1);
    record.push_str(line);
    record.push('\n');
    file.write_all(record.as_bytes())
}

/// 메타 파일을 연다(없으면 빈 파일로 만든다). 못 열면 잠금·누계 없이 진행한다 — 완료
/// 통지를 메타 때문에 잃지도, **기다리지도** 않는 것이 우선이다. 같은 이유로 비우기는
/// 배타 잠금을 유한 시간만 시도한다([`MetaLock::exclusive`]). 그 대가는 이번 비우기가
/// 누계에 안 잡히는 것이고, 재개하는 reader 는 그것을 "계약 밖 비우기" 로 알아챈다(ADR-0415).
fn open_meta(meta_path: &Path) -> Option<std::fs::File> {
    match std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(meta_path)
    {
        Ok(f) => Some(f),
        Err(e) => {
            tracing::warn!(
                "notify meta {} open failed: {e} — appending without the lock, a truncation now \
                 would not be counted",
                meta_path.display()
            );
            None
        }
    }
}

/// 비우기가 배타 잠금을 기다리는 상한. 이 값은 **파생되지 않는다** — 근거로 고른 값이다
/// (ADR-0415 "잠금 대기 상한").
///
/// - 아래로 누르는 쪽: append 가 공유 잠금을 쥐는 구간은 한 줄 write 한 번이라 µs 단위다
///   (Gate 4 리뷰 실측: cap 아래 append 전체가 11.6 µs). writer 끼리의 경합으로는 이 상한에 닿지 않아야 한다 —
///   닿으면 정상 운용에서도 누계가 "모른다" 로 떨어진다. 그 구간보다 네 자릿수 이상 크다.
/// - 위로 누르는 쪽: 이 대기는 완료 통지 한 줄의 지연에 그대로 더해진다. 통지는 사람이
///   기다리는 신호라 그 지연이 눈에 띄지 않을 만큼 짧아야 한다.
const EXCLUSIVE_LOCK_BUDGET: std::time::Duration = std::time::Duration::from_millis(200);

/// 배타 잠금 재시도 간격. append 의 공유 구간(µs)보다 훨씬 길게 잡아 빈 틈을 여러 번
/// 본다. 상한 안에서 시도 횟수는 40 번이다.
const EXCLUSIVE_LOCK_RETRY: std::time::Duration = std::time::Duration::from_millis(5);

/// 메타 파일 advisory 잠금. drop 에서 푼다(핸들을 닫아도 풀리지만 핸들 수명이 더 길다).
///
/// 공유 잠금은 기다린다 — 배타를 쥐는 것은 비우는 writer 뿐이고 그 구간은 자기 일(재기 ·
/// 비우기 · 누계 한 줄)로 끝난다. 배타 쪽은 기다리지 않는다([`MetaLock::exclusive`]).
struct MetaLock<'a>(&'a std::fs::File);

impl<'a> MetaLock<'a> {
    fn shared(file: &'a std::fs::File, meta_path: &Path) -> Option<Self> {
        match file.lock_shared() {
            Ok(()) => Some(Self(file)),
            Err(e) => {
                tracing::warn!(
                    "notify meta {} shared lock failed: {e}",
                    meta_path.display()
                );
                None
            }
        }
    }

    /// 배타 잠금은 **기다리지 않는다** — `try_lock` 을 [`EXCLUSIVE_LOCK_BUDGET`] 동안
    /// [`EXCLUSIVE_LOCK_RETRY`] 간격으로 다시 시도하고, 못 잡으면 `None` 이다.
    ///
    /// 공유 잠금을 쥐는 쪽에는 reader 도 있다. 그 reader 의 출력 소비자가 멈추면 잠금이
    /// 계속 잡혀 있고, 블로킹 `lock()` 이면 cap 에 닿은 writer — 곧 plugin 의 완료 통지
    /// 경로 — 가 그 reader 가 풀 때까지 선다. 통지 지연의 상한을 reader 가 정하게 두지
    /// 않는다. 못 잡으면 호출자가 잠금 없이 비우고 누계를 올리지 않는다(재개 reader 에게는
    /// 계약의 "모른다" 갈래다 — ADR-0415).
    fn exclusive(file: &'a std::fs::File, meta_path: &Path) -> Option<Self> {
        let deadline = std::time::Instant::now() + EXCLUSIVE_LOCK_BUDGET;
        loop {
            match file.try_lock() {
                Ok(()) => return Some(Self(file)),
                Err(std::fs::TryLockError::WouldBlock) => {
                    if std::time::Instant::now() >= deadline {
                        tracing::warn!(
                            "notify meta {} is still locked by another holder after {:?} — \
                             truncating without the lock",
                            meta_path.display(),
                            EXCLUSIVE_LOCK_BUDGET
                        );
                        return None;
                    }
                    std::thread::sleep(EXCLUSIVE_LOCK_RETRY);
                }
                Err(std::fs::TryLockError::Error(e)) => {
                    tracing::warn!(
                        "notify meta {} exclusive lock failed: {e}",
                        meta_path.display()
                    );
                    return None;
                }
            }
        }
    }
}

impl Drop for MetaLock<'_> {
    fn drop(&mut self) {
        if let Err(e) = self.0.unlock() {
            tracing::warn!("notify meta unlock failed: {e}");
        }
    }
}

/// 배타 잠금 아래에서 비우고, 버린 양을 누계에 더하고, 로그에 남긴다.
///
/// 잠금을 못 잡아도 비우기는 한다 — 크기 축(ADR-0344)이 먼저다. 그때는 누계를 **올리지
/// 않는다**: 잠금 없이 잰 크기는 버린 양과 다를 수 있고, 틀린 수보다 "모른다" 가 낫다.
/// 재개 reader 는 그 비우기를 계약의 "모른다" 갈래로 본다. 그 사실이 warn 로 남는다.
fn truncate_and_account(path: &Path, cap: u64, meta: Option<&std::fs::File>, meta_path: &Path) {
    let exclusive = meta.and_then(|m| MetaLock::exclusive(m, meta_path));
    let Some(discarded) = truncate_over_cap(path, cap) else {
        return;
    };
    let retention_start = match (&exclusive, meta) {
        (Some(_), Some(m)) => advance_retention_start(m, discarded, meta_path),
        _ => None,
    };
    // 버린 양을 **로그에** 남긴다 — 이 파일 자신에는 안 쓴다. 읽는 쪽 계약은
    // "한 줄 = 완료 통지" 이므로 여기에 메타 줄을 끼우면 그 줄이 완료로 읽힌다
    // (ADR-0330 이 그 대안을 기각한 이유). 재개하는 reader 에게는 옆 메타 파일의
    // 누계가 같은 사실을 값으로 준다.
    match retention_start {
        Some(start) => tracing::warn!(
            "notify log {} hit the {cap}-byte cap — discarded {discarded} bytes of completion lines, \
             including anything a lagging reader had not read yet (retention_start is now {start})",
            path.display()
        ),
        None => tracing::warn!(
            "notify log {} hit the {cap}-byte cap — discarded {discarded} bytes of completion lines, \
             including anything a lagging reader had not read yet (retention_start NOT advanced — \
             a resuming reader will see an unaccounted truncation)",
            path.display()
        ),
    }
}

/// 누계를 읽어 `discarded` 를 더하고 제자리에 다시 쓴다. 새 누계를 돌려준다. 호출자가 배타
/// 잠금을 쥐고 있어야 한다.
fn advance_retention_start(meta: &std::fs::File, discarded: u64, meta_path: &Path) -> Option<u64> {
    use std::io::{Read, Seek, SeekFrom};
    let mut handle = meta;
    let mut current = String::new();
    let result = handle
        .seek(SeekFrom::Start(0))
        .and_then(|_| handle.read_to_string(&mut current))
        .and_then(|_| {
            let next = parse_retention_start(&current).saturating_add(discarded);
            // 내용이 이 모듈이 쓴 한 줄의 폭이 아니면(빈 파일 포함) 먼저 비운다. 폭이 같으면
            // 줄이는 단계 없이 덮어쓴다(`RETENTION_START_WIDTH` 의 이유).
            if current.len() != format_retention_start(0).len() {
                handle.set_len(0)?;
            }
            handle.seek(SeekFrom::Start(0))?;
            handle.write_all(format_retention_start(next).as_bytes())?;
            Ok(next)
        });
    match result {
        Ok(next) => Some(next),
        Err(e) => {
            tracing::warn!("notify meta {} update failed: {e}", meta_path.display());
            None
        }
    }
}

/// cap 을 넘은 파일을 비운다 — **쓰기 핸들과 분리된 자리**다.
///
/// 연 핸들로 크기를 **다시 재는** 이유: 바깥의 `metadata()` 판정과 여기 사이에 다른
/// writer 가 이미 비웠을 수 있다. 그때 또 비우면 그 writer 가 새로 쓴 줄들을 지운다.
///
/// 반환값은 **실제로 버린 바이트 수**다. `None` 은 안 비웠다는 뜻이고 갈래가 셋이다 —
/// 못 열었거나, 다시 재 보니 이미 cap 아래였거나, `set_len` 이 실패했다. 셋 다 "버린
/// 것이 없다" 이므로 호출자가 손실을 보고해서는 안 된다. 이 값을 돌려주는 이유는
/// 손실의 **양**이 여기서만 알려지기 때문이다: 비우고 나면 파일이 0 이라 나중에
/// 아무도 얼마가 사라졌는지 되물을 수 없다.
fn truncate_over_cap(path: &Path, cap: u64) -> Option<u64> {
    let opened = std::fs::OpenOptions::new().write(true).open(path);
    // 못 열면 여기서 보고하지 않는다 — 곧바로 이어지는 append 가 같은 원인으로
    // 실패해 호출자에게 `io::Error` 로 올라간다. 두 번 시끄럽게 할 이유가 없다.
    let file = opened.ok()?;
    let len = file.metadata().map(|m| m.len()).unwrap_or(0);
    if len < cap {
        return None;
    }
    if let Err(e) = file.set_len(0) {
        // best-effort — 못 비워도 append 는 그대로 진행한다(파일이 cap 을 넘어 자랄
        // 뿐 알림은 계속 간다). 조용히 넘기면 그 성장을 아무도 설명하지 못한다.
        tracing::warn!("notify log truncate failed for {}: {e}", path.display());
        return None;
    }
    Some(len)
}

#[cfg(test)]
mod tests {
    use super::*;

    // notify_log_path() 의 홈 선택 로직은 pure `resolve_home` 로 추출돼 있어(이 crate 는
    // `#![forbid(unsafe_code)]` 라 테스트에서 env 를 직접 조작할 수 없다) env 조작 없이
    // 검증한다. resolve_home 이 곧 notify_log_path 가 쓰는 우선순위 규칙 전부다.

    // TASTY_PARENT_HOME 이 설정돼 있으면 그 값을 쓴다(fallback 은 호출조차 안 함).
    #[test]
    fn resolve_home_prefers_parent_env() {
        let got = resolve_home(Some("/tmp/fake-parent-home".to_string()), || {
            panic!("fallback must not run when parent env is set")
        });
        assert_eq!(got, Some(PathBuf::from("/tmp/fake-parent-home")));
    }

    // 이를 notify_log_path 경로 조립까지 연결하면 최종 경로가 parent home 밑에 놓인다.
    #[test]
    fn parent_env_drives_full_notify_path() {
        let home = resolve_home(Some("/tmp/fake-parent-home".to_string()), || None).unwrap();
        assert_eq!(
            notify_log_path_in(&home, 9),
            PathBuf::from("/tmp/fake-parent-home/notify/9.log")
        );
    }

    // TASTY_PARENT_HOME 이 없으면 fallback(= tasty_home())을 쓴다.
    #[test]
    fn resolve_home_falls_back_when_parent_absent() {
        let got = resolve_home(None, || Some(PathBuf::from("/tmp/fallback-root")));
        assert_eq!(got, Some(PathBuf::from("/tmp/fallback-root")));
    }

    // 빈/공백 값도 미설정으로 간주하고 fallback 한다.
    #[test]
    fn resolve_home_treats_empty_parent_as_absent() {
        let got = resolve_home(Some("   ".to_string()), || {
            Some(PathBuf::from("/tmp/fallback-root2"))
        });
        assert_eq!(got, Some(PathBuf::from("/tmp/fallback-root2")));
    }

    #[test]
    fn path_in_builds_notify_subdir_and_log_suffix() {
        let home = Path::new("/home/u/.tasty");
        assert_eq!(
            notify_log_path_in(home, 42),
            PathBuf::from("/home/u/.tasty/notify/42.log")
        );
    }

    #[test]
    fn append_creates_dir_and_writes_line_with_newline() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("notify").join("7.log");
        append_line_to(&path, "surface 7 작업 완료 (호출 방식: spawn)", 1024).unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        assert_eq!(content, "surface 7 작업 완료 (호출 방식: spawn)\n");
    }

    #[test]
    fn append_accumulates_multiple_lines() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("a.log");
        append_line_to(&path, "first", 1024).unwrap();
        append_line_to(&path, "second", 1024).unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        assert_eq!(content, "first\nsecond\n");
    }

    // 이 파일에는 writer 가 여럿이라, cap 경계에서 겹쳐도 **한 줄이 통째로 남거나
    // 통째로 없어야** 한다. 그 불변식을 여기서 잰다.
    //
    // 한계: 여기 재는 것은 같은 프로세스의 두 스레드다. 실제 상황은 claude child 와
    // codex child 가 **서로 다른 프로세스**로 붙는 것이고, 프로세스 둘로 재면 같은
    // 결함이 훨씬 크게 드러난다. 두 프로세스를 띄우는 하네스는 이 크레이트 안에
    // 둘 자리가 없어(새 타깃이 필요하다) 스레드 판을 둔다 — 그래도 고치기 전
    // 코드에서는 이 판도 깨진다.
    //
    // 두 writer 의 줄 길이를 다르게 두는 것이 핵심이다. 길이가 같으면 덮어쓰기가
    // 같은 폭을 채워 흔적이 안 남는다.
    #[test]
    fn concurrent_writers_never_leave_a_partial_line() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("race.log");
        // cap 은 이 시험이 **truncate 갈래를 지나지 않을 만큼** 높게 둔다. 낮추면 뒤의
        // truncate 가 앞에서 생긴 잔해를 지워 시험이 간헐적으로 통과한다 — 고치기 전
        // 코드에 대고 재 보면 cap 512 에서는 3/3 통과, 64 KiB 에서는 1/3 통과였고
        // 여기 값에서만 3/3 실패였다. 그래서 이 시험이 재는 것은 **한 줄이 한 번의
        // write 로 나가는가** 하나이고, truncate 갈래의 경합은 이것으로 안 재진다.
        const CAP: u64 = 1 << 30;
        const ROUNDS: usize = 2_000;
        // writer 를 둘만 두면 겹치는 창이 좁다. 실제 상황(claude child + codex child)
        // 보다 많이 띄워 창을 넓힌다.
        const WRITERS_PER_SHAPE: usize = 4;
        let long = "X".repeat(60);
        std::thread::scope(|scope| {
            for _ in 0..WRITERS_PER_SHAPE {
                scope.spawn(|| {
                    for i in 0..ROUNDS {
                        append_line_to(&path, &format!("A-{i:06}"), CAP)
                            .expect("append_line_to failed");
                    }
                });
                scope.spawn(|| {
                    for i in 0..ROUNDS {
                        append_line_to(&path, &format!("B-{i:06}-{long}"), CAP)
                            .expect("append_line_to failed");
                    }
                });
            }
        });
        let text = std::fs::read_to_string(&path).unwrap();
        let malformed: Vec<&str> = text
            .lines()
            .filter(|l| {
                let a = l.len() == 8 && l.starts_with("A-");
                let b = l.len() == 69 && l.starts_with("B-");
                !(a || b)
            })
            .collect();
        assert!(
            malformed.is_empty(),
            "둘 중 누구의 줄도 아닌 잔해가 {} 줄 남았다 — 덮어썼거나 한 줄이 여러 번의 \
             write 로 쪼개져 끼어들었다는 뜻이다. 처음 셋: {:?}",
            malformed.len(),
            &malformed[..malformed.len().min(3)]
        );
    }

    // 비우기는 **얼마를 버렸는지**를 값으로 돌려준다. 비우고 나면 파일이 0 이라
    // 그 양을 나중에 되물을 방법이 없고, 그 수가 없으면 호출자가 손실을 로그에
    // 적을 수 없다 — "유실을 숨기지 않는다" 가 이 반환값 위에 서 있다.
    #[test]
    fn truncate_reports_how_many_bytes_it_threw_away() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("over.log");
        std::fs::write(&path, vec![b'x'; 300]).unwrap();
        assert_eq!(truncate_over_cap(&path, 256), Some(300));
        assert_eq!(std::fs::metadata(&path).unwrap().len(), 0);
    }

    // 안 버렸으면 아무 수도 돌려주지 않는다. 이 갈래는 실제로 일어난다 — 바깥의
    // `metadata()` 판정과 여기 사이에 다른 writer 가 이미 비웠을 수 있다. 그때
    // `Some(0)` 을 돌려주면 호출자가 "0 바이트를 버렸다" 를 로그에 적어, 아무 일도
    // 없던 자리에 손실 기록이 남는다.
    #[test]
    fn truncate_reports_nothing_when_another_writer_already_emptied_it() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("raced.log");
        std::fs::write(&path, b"short\n").unwrap();
        assert_eq!(truncate_over_cap(&path, 256), None);
        assert_eq!(std::fs::metadata(&path).unwrap().len(), 6);
    }

    // 없는 파일은 열리지 않으므로 버린 것도 없다. 이어지는 append 가 같은 원인으로
    // 실패해 호출자에게 `io::Error` 로 올라가므로 여기서 또 보고하지 않는다.
    #[test]
    fn truncate_reports_nothing_when_the_file_cannot_be_opened() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("absent.log");
        assert_eq!(truncate_over_cap(&path, 1), None);
    }

    // cap 도달은 **파일 전체**를 버린다 — 넘긴 만큼만 잘라 내는 것이 아니다. 뒤처진
    // reader 의 미독분도 그 안에 있고, 그것이 이 로그의 보존 범위가 "지금 세대의
    // 마지막 비우기 이후" 인 이유다.
    #[test]
    fn hitting_the_cap_discards_the_whole_file_not_just_the_excess() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("whole.log");
        for i in 0..20 {
            append_line_to(&path, &format!("unread-{i:02}"), 64).unwrap();
        }
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(
            content.lines().count() < 20,
            "cap 을 넘겼는데도 20 줄이 다 남았다 — 비우기가 안 돌았다"
        );
        assert!(
            !content.contains("unread-00"),
            "첫 줄이 남아 있다 — 비우기가 초과분만 잘라냈다는 뜻이고, 그러면 보존 \
             범위를 '마지막 비우기 이후' 로 말할 수 없다: {content:?}"
        );
    }

    // ── 독자 복구 계약(ADR-0415) ───────────────────────────────────────────────
    //
    // 아래 `resume` 은 ADR 이 적은 reader 절차의 **참조 구현**이다. 제품 코드에는 reader
    // 가 없고(소비자는 `tail -F` 와 셸이다) 계약만 있으므로, 그 계약이 writer 와 맞물리는지를
    // 여기서 잰다.

    /// 한 번의 재개 결과. 어휘는 `events.fetch` 의 것이다.
    #[derive(Debug)]
    struct Resume {
        lines: Vec<String>,
        next_offset: u64,
        truncated: bool,
        /// `None` 은 "모른다" — 누계에 안 잡힌 비우기(계약 밖)가 있었다.
        skipped: Option<u64>,
    }

    fn resume(log: &Path, next_offset: u64) -> Resume {
        let meta_path = notify_meta_path(log);
        // 공유 잠금 아래에서 누계와 로그를 함께 읽는다 — 비우기가 그 사이에 끼지 않는다.
        let meta = std::fs::OpenOptions::new().read(true).open(&meta_path).ok();
        let _guard = meta.as_ref().and_then(|m| MetaLock::shared(m, &meta_path));
        let base = std::fs::read_to_string(&meta_path)
            .map(|t| parse_retention_start(&t))
            .unwrap_or(0);
        let bytes = std::fs::read(log).unwrap_or_default();
        let len = bytes.len() as u64;
        let (physical, truncated, skipped) = if next_offset < base {
            (0, true, Some(base - next_offset))
        } else if next_offset - base > len {
            (0, true, None)
        } else {
            (next_offset - base, false, Some(0))
        };
        let tail = &bytes[physical as usize..];
        // 완결된 줄만 소비한다 — 마지막 개행 뒤의 조각은 다음 재개로 넘긴다.
        let consumed = tail.iter().rposition(|b| *b == b'\n').map_or(0, |i| i + 1);
        let lines = String::from_utf8_lossy(&tail[..consumed])
            .lines()
            .map(str::to_owned)
            .collect();
        Resume {
            lines,
            next_offset: base + physical + consumed as u64,
            truncated,
            skipped,
        }
    }

    // 한 번도 안 비웠으면 누계는 0 이고, 논리 오프셋은 파일 안 위치와 같다.
    #[test]
    fn before_any_truncation_the_offset_is_the_file_position() {
        let tmp = tempfile::tempdir().unwrap();
        let log = tmp.path().join("notify").join("3.log");
        append_line_to(&log, "one", 1024).unwrap();
        append_line_to(&log, "two", 1024).unwrap();
        let first = resume(&log, 0);
        assert_eq!(first.lines, ["one", "two"]);
        assert_eq!(first.next_offset, 8);
        assert!(!first.truncated);
        assert_eq!(
            parse_retention_start(&std::fs::read_to_string(notify_meta_path(&log)).unwrap()),
            0
        );
        let again = resume(&log, first.next_offset);
        assert!(again.lines.is_empty());
        assert_eq!(again.next_offset, 8);
    }

    // 비우기마다 버린 양이 누계에 **더해진다** — 덮어쓰는 것이 아니다. 두 번 비운 뒤의
    // 값이 두 비우기의 합이어야 reader 가 두 번 사이에 멈췄어도 맞게 센다.
    #[test]
    fn retention_start_accumulates_what_every_truncation_threw_away() {
        let tmp = tempfile::tempdir().unwrap();
        let log = tmp.path().join("acc.log");
        // cap 8: "aaaaaa\n"(7) 다음 "bbbbbb\n" 로 14 → 세 번째 append 전에 14 를 버린다.
        for l in ["aaaaaa", "bbbbbb", "cccccc", "dddddd", "eeeeee"] {
            append_line_to(&log, l, 8).unwrap();
        }
        // 버린 것: [aaaaaa bbbbbb](14) 그리고 [cccccc dddddd](14).
        let meta = std::fs::read_to_string(notify_meta_path(&log)).unwrap();
        assert_eq!(parse_retention_start(&meta), 28);
        assert_eq!(std::fs::read_to_string(&log).unwrap(), "eeeeee\n");
    }

    // 확인 절차 1 — 고유 표식으로 cap 전후와 reader 중지/재개를 대조한다. reader 가 멈춘
    // 사이 비우기가 났으면 `truncated` 이고, `skipped` 는 **못 읽은 줄의 바이트 합과 정확히
    // 같다.** 재개 뒤 받은 줄은 비우기 이후의 것뿐이다.
    #[test]
    fn a_resuming_reader_is_told_exactly_how_many_bytes_it_lost() {
        let tmp = tempfile::tempdir().unwrap();
        let log = tmp.path().join("resume.log");
        const CAP: u64 = 64;
        let mut written = Vec::new();
        let push = |i: usize| {
            let l = format!("mark-{i:04}");
            append_line_to(&log, &l, CAP).unwrap();
            l
        };
        for i in 0..3 {
            written.push(push(i));
        }
        let first = resume(&log, 0);
        assert_eq!(first.lines, written[..3]);
        // reader 가 멈춘 동안 cap 을 몇 번 넘긴다.
        for i in 3..30 {
            written.push(push(i));
        }
        let second = resume(&log, first.next_offset);
        assert!(
            second.truncated,
            "멈춘 사이 비우기가 있었는데 truncated 가 아니다"
        );
        let got: std::collections::BTreeSet<&str> =
            second.lines.iter().map(String::as_str).collect();
        let lost_bytes: u64 = written[3..]
            .iter()
            .filter(|l| !got.contains(l.as_str()))
            .map(|l| l.len() as u64 + 1)
            .sum();
        assert_eq!(second.skipped, Some(lost_bytes));
        // 받은 줄은 쓴 순서의 **꼬리**여야 한다 — 비우기 이후 것만 남는다.
        assert_eq!(second.lines, written[written.len() - second.lines.len()..]);
        assert!(!second.lines.is_empty());
    }

    // 누계에 안 잡힌 비우기(잠금을 모르는 writer · 누계 갱신 실패)는 reader 가 **모른다고**
    // 말해야 한다. 조용히 파일 중간부터 읽으면 줄 앞부분이 잘린 잔해를 완료로 읽는다.
    #[test]
    fn an_unaccounted_truncation_is_reported_as_unknown_loss() {
        let tmp = tempfile::tempdir().unwrap();
        let log = tmp.path().join("foreign.log");
        append_line_to(&log, "first-line", 1024).unwrap();
        let first = resume(&log, 0);
        // 메타 없이 비운 것처럼 만든다.
        std::fs::write(&log, b"x\n").unwrap();
        let second = resume(&log, first.next_offset);
        assert!(second.truncated);
        assert_eq!(second.skipped, None);
        assert_eq!(second.lines, ["x"]);
    }

    // 완결되지 않은 줄 조각은 소비하지 않는다 — 다음 재개가 통째로 받는다.
    #[test]
    fn a_partial_trailing_line_is_left_for_the_next_resume() {
        let tmp = tempfile::tempdir().unwrap();
        let log = tmp.path().join("partial.log");
        std::fs::write(&log, b"done\nhal").unwrap();
        let first = resume(&log, 0);
        assert_eq!(first.lines, ["done"]);
        assert_eq!(first.next_offset, 5);
    }

    // 메타 파일은 늘 같은 폭이다 — 제자리 덮어쓰기가 파일을 줄이지 않아야 잠금 없는
    // reader 가 빈 파일을 안 본다. 그리고 셸이 읽을 수 있어야 한다(앞자리 0 없음).
    #[test]
    fn the_meta_line_has_a_fixed_width_and_no_leading_zeros() {
        let small = format_retention_start(7);
        let big = format_retention_start(u64::MAX);
        assert_eq!(small.len(), big.len());
        // ADR-0415 이 이 폭(키 16 + 값 20 + 개행 1)을 바이트 수로 적는다 — 그 사본의 판정기.
        assert_eq!(small.len(), 37);
        assert!(small.starts_with("retention_start=7 "), "{small:?}");
        assert!(small.ends_with('\n'));
        assert_eq!(parse_retention_start(&small), 7);
        assert_eq!(parse_retention_start(&big), u64::MAX);
        assert_eq!(parse_retention_start(""), 0);
        assert_eq!(parse_retention_start("other=3\n"), 0);
    }

    // 공유 잠금을 오래 쥔 reader 가 있어도 cap 에 닿은 writer — 완료 통지 경로 — 는 서지
    // 않는다. 배타 잠금은 유한 시간만 시도하고, 못 잡으면 잠금 없이 비우되 누계는 안
    // 올린다. 그 비우기는 재개 reader 에게 "모른다" 로 보인다. 배타 잠금을 블로킹 `lock()`
    // 으로 되돌리면 이 시험의 append 가 잠금이 풀릴 때까지 돌아오지 않아 시간 초과로 죽는다.
    #[test]
    fn a_reader_holding_the_shared_lock_does_not_stall_a_truncating_writer() {
        let tmp = tempfile::tempdir().unwrap();
        let log = tmp.path().join("held.log");
        append_line_to(&log, "read-before-hold", 1 << 20).unwrap();
        let before = resume(&log, 0);
        std::fs::write(&log, vec![b'x'; 300]).unwrap();
        // reader 가 공유 잠금을 쥐고 멈춘 상태.
        let meta = std::fs::File::open(notify_meta_path(&log)).unwrap();
        meta.lock_shared().unwrap();

        let (tx, rx) = std::sync::mpsc::channel();
        let writer_log = log.clone();
        let started = std::time::Instant::now();
        // scope 가 아니라 떼어 낸 스레드다 — 되돌린 코드에서 이 시험이 죽을 때 막힌 writer
        // 를 join 하느라 시험 자체가 멈추지 않게 한다.
        std::thread::spawn(move || {
            let r = append_line_to(&writer_log, "after-cap", 256);
            // 수신 쪽이 이미 시간 초과로 떠났으면 보낼 곳이 없다 — 그 결과는 버린다.
            tx.send(r.is_ok()).ok();
        });
        let finished = rx.recv_timeout(EXCLUSIVE_LOCK_BUDGET * 10);
        let elapsed = started.elapsed();
        assert_eq!(
            finished,
            Ok(true),
            "공유 잠금을 쥔 reader 때문에 cap 을 넘긴 append 가 {elapsed:?} 뒤에도 안 끝났다"
        );
        assert!(
            elapsed >= EXCLUSIVE_LOCK_BUDGET,
            "재시도 없이 바로 포기했다: {elapsed:?}"
        );
        meta.unlock().unwrap();

        assert_eq!(std::fs::read_to_string(&log).unwrap(), "after-cap\n");
        let meta_text = std::fs::read_to_string(notify_meta_path(&log)).unwrap();
        assert_eq!(
            parse_retention_start(&meta_text),
            0,
            "잠금 없이 잰 수로 누계를 올렸다"
        );
        let after = resume(&log, before.next_offset);
        assert!(after.truncated);
        assert_eq!(after.skipped, None, "누계 밖 비우기는 '모른다' 여야 한다");
        assert_eq!(after.lines, ["after-cap"]);
    }

    // 세 값의 사본 자리: 200 ms 는 ADR-0415 · dev-guide(external-interaction 의 완료 알림 절) · CHANGELOG,
    // 5 ms 는 ADR-0415, 40 번은 위 `EXCLUSIVE_LOCK_RETRY` 의 doc 이다. 이 시험이 그 사본들의
    // 판정기다 — 값을 바꾸면 여기가 빨개지고, 그때 그 자리들을 같이 고친다.
    #[test]
    fn the_exclusive_lock_budget_matches_the_documented_values() {
        assert_eq!(EXCLUSIVE_LOCK_BUDGET, std::time::Duration::from_millis(200));
        assert_eq!(EXCLUSIVE_LOCK_RETRY, std::time::Duration::from_millis(5));
        assert_eq!(
            EXCLUSIVE_LOCK_BUDGET.as_millis() / EXCLUSIVE_LOCK_RETRY.as_millis(),
            40
        );
    }

    #[test]
    fn meta_path_sits_next_to_the_log() {
        assert_eq!(
            notify_meta_path(Path::new("/h/notify/42.log")),
            PathBuf::from("/h/notify/42.log.meta")
        );
    }

    // 확인 절차 1 의 동시 판 — 작은 cap 에서 writer 여럿과 주기적으로 재개하는 reader 하나.
    // 끝까지 따라잡은 뒤 **받은 바이트 + skipped 합 = 쓴 바이트 합** 이어야 한다. 비우기와
    // append 가 겹쳐 누계에 안 잡힌 줄이 생기면 좌변이 모자라 깨진다(잠금이 지키는 것).
    #[test]
    fn under_concurrent_writers_read_plus_skipped_equals_written() {
        let tmp = tempfile::tempdir().unwrap();
        let log = tmp.path().join("conc.log");
        const CAP: u64 = 512;
        const WRITERS: usize = 6;
        const ROUNDS: usize = 1_500;
        let done = std::sync::atomic::AtomicUsize::new(0);
        let (mut read_bytes, mut skipped_bytes, mut unknown) = (0u64, 0u64, 0usize);
        let mut seen = std::collections::HashSet::new();
        std::thread::scope(|scope| {
            for w in 0..WRITERS {
                let log = &log;
                let done = &done;
                scope.spawn(move || {
                    for i in 0..ROUNDS {
                        append_line_to(log, &format!("w{w}-{i:05}"), CAP).unwrap();
                    }
                    done.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                });
            }
            let mut offset = 0u64;
            loop {
                let finished = done.load(std::sync::atomic::Ordering::SeqCst) == WRITERS;
                let r = resume(&log, offset);
                for l in &r.lines {
                    read_bytes += l.len() as u64 + 1;
                    assert!(seen.insert(l.clone()), "같은 줄을 두 번 받았다: {l}");
                }
                match r.skipped {
                    Some(n) => skipped_bytes += n,
                    None => unknown += 1,
                }
                offset = r.next_offset;
                if finished {
                    break;
                }
                // 재개 사이에 잠금 밖에서 쉰다. 쉬지 않고 다시 잡으면 공유 잠금이 사실상 계속
                // 잡혀 있어(읽기가 느린 러너일수록) 비우는 writer 가 배타 잠금 상한을 넘기고,
                // 그 비우기는 계약대로 "모른다" 가 된다 — ADR-0415 "잃은 것" 이 reader 에게
                // 공유 구간을 짧게 하라고 요구하는 그 경우다. 이 시험이 재는 것은 writer 끼리의
                // 경합이지 reader 의 잠금 점유가 아니다.
                std::thread::sleep(EXCLUSIVE_LOCK_RETRY);
            }
        });
        let written: u64 = (0..WRITERS)
            .flat_map(|w| (0..ROUNDS).map(move |i| format!("w{w}-{i:05}").len() as u64 + 1))
            .sum();
        assert_eq!(unknown, 0, "누계에 안 잡힌 비우기를 {unknown} 번 봤다");
        assert_eq!(
            read_bytes + skipped_bytes,
            written,
            "받은 {read_bytes} + 잃은 {skipped_bytes} 가 쓴 {written} 과 다르다"
        );
    }

    #[test]
    fn append_truncates_when_over_cap() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("cap.log");
        // cap=8: "aaaaaa\n" = 7 bytes < 8 이므로 첫 줄은 남고 다음 append 전 크기는 7.
        append_line_to(&path, "aaaaaa", 8).unwrap();
        // 이제 파일 크기 7 < 8 → 두 번째는 append. 크기 7+"bbbbbb\n"(7)=14 ≥ 8.
        append_line_to(&path, "bbbbbb", 8).unwrap();
        // 세 번째 append 시점: 기존 14 ≥ cap 8 → truncate 후 이 줄만 남는다.
        append_line_to(&path, "cccccc", 8).unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        assert_eq!(content, "cccccc\n");
    }
}
