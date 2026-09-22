//! 옵저버: PTY 라인 → 파서 → sink fan-out.
//!
//! 메인 thread 에 `ObserverRouter` 가 살고, 각 옵저버의 sink 는 자기 worker
//! thread 한 개와 bounded channel (`std::sync::mpsc::sync_channel`) 로 연결돼
//! 있다. `dispatch_text` 가 호출되면 라인 단위로 쪼개 매칭 옵저버에
//! `try_send` — 가득 차면 drop + counter 증가.
//!
//! 첫 PR scope: memory + file sink 2 종, 휘발성 spec. socket/fifo 와 spec
//! persistence 는 후속 phase.

use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::mpsc::{SyncSender, TrySendError, sync_channel};
use std::thread::{self, JoinHandle};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::json;
use tasty_output::{DEFAULT_PARSER_IDS, ParsedItem, Parser, lookup};

/// 옵저버 고유 id (호스트 자동 할당, 1 부터 증가).
pub type ObserverId = u64;

/// 옵저버 등록 spec. `surface_id = None` 이면 모든 surface 를 본다.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObserverSpec {
    pub surface_id: Option<u32>,
    /// 활성 파서 id 리스트. 비어있으면 [`DEFAULT_PARSER_IDS`] 사용.
    pub parsers: Vec<String>,
    /// kind 필터 (예: `["path","url"]`). `None` 이면 통과.
    pub kinds: Option<Vec<String>>,
    pub sink: SinkSpec,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SinkSpec {
    /// `tasty-memory` 의 `scope=Host` 위 `tasty.observer.<id>.<unix-ms>.<seq>` key 로
    /// JSON 저장. `max_records=0` 이면 무한, 그 외는 ring buffer (오래된 키
    /// 부터 삭제).
    Memory { max_records: usize },
    /// `path` 가 `None` 이면 `~/.tasty/observers/<id>.jsonl` 에 자동 append.
    File { path: Option<PathBuf> },
}

/// `output.observe_info` / `output.observe_list` 응답.
#[derive(Debug, Clone, Serialize)]
pub struct ObserverInfo {
    pub id: ObserverId,
    #[serde(flatten)]
    pub spec_view: SpecView,
    /// dispatch 시도된 ParsedItem 수 (필터 적용 후).
    pub total_in: u64,
    /// sink channel 에 성공적으로 들어간 수.
    pub total_out: u64,
    /// backpressure 로 drop 된 수.
    pub dropped: u64,
    /// 마지막 try_send 성공 시각 (unix-ms).
    pub last_event_ms: Option<i64>,
}

/// `ObserverSpec` 의 응답용 평면화. `sink` 안의 path 가 default 였으면
/// 실제 resolved path 도 같이 노출.
#[derive(Debug, Clone, Serialize)]
pub struct SpecView {
    pub surface_id: Option<u32>,
    pub parsers: Vec<String>,
    pub kinds: Option<Vec<String>>,
    pub sink: SinkView,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SinkView {
    Memory { max_records: usize },
    File { path: PathBuf },
}

fn unix_ms_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// 메인 스레드 상태. 각 옵저버는 자기 worker thread 와 bounded sender 를 갖는다.
pub struct ObserverRouter {
    /// **engine 들이 이 Arc 를 공유한다** — router 마다 따로 세면 두 창이 같은 observer
    /// id 를 발급하고, `Kind::Observer` 라우팅이 먼저 찾힌 engine 을 고르므로 나중 것은
    /// 어떤 요청으로도 못 닿는다(`IdGenerator` doc 의 "글로벌 유니크").
    next_id: std::sync::Arc<std::sync::atomic::AtomicU64>,
    observers: HashMap<ObserverId, ObserverEntry>,
    /// surface 별 partial-line 버퍼. `'\n'` 이 들어올 때까지 누적.
    line_buffers: HashMap<u32, LineBuffer>,
    /// surface close 로 자동 해제된 sink 워커의 join 핸들 — [`ObserverRouter::retire`]
    /// 참조. 렌더 스레드에서 join 하지 않고 여기 모아뒀다가, 이미 끝난 것만
    /// 논블로킹으로 걷어내고(`reap_finished`) 남은 것은 앱 종료 시
    /// [`ObserverRouter::join_retired`] 가 한 번에 회수한다.
    retired: Vec<RetiredSink>,
}

/// 해제됐지만 아직 join 하지 않은 sink 워커.
struct RetiredSink {
    id: ObserverId,
    join: JoinHandle<()>,
}

struct ObserverEntry {
    spec: ObserverSpec,
    resolved_sink: SinkView,
    parser_handles: Vec<&'static dyn Parser>,
    kinds_filter: Option<Vec<String>>,
    tx: SyncSender<ParsedItem>,
    join: Option<JoinHandle<()>>,
    total_in: u64,
    total_out: u64,
    dropped: u64,
    last_event_ms: Option<i64>,
}

#[derive(Default)]
struct LineBuffer {
    /// 다음 emit 할 라인의 0-based index.
    next_idx: u32,
    /// 미완성 partial 라인 (마지막 `\n` 이후).
    partial: String,
}

/// 옵저버 등록 / dispatch 시 발생할 수 있는 에러.
#[derive(Debug)]
pub enum ObserverError {
    UnknownParser(String),
    InvalidPath(String),
    FileOpen(String),
    NotFound(ObserverId),
    /// sink 워커 스레드 spawn 실패(스레드 한계·EAGAIN 등). `observe.start` 마다
    /// 스레드를 하나 만들므로, 패닉으로 승격하면 호스트 전체가 죽는다 — 파일 열기
    /// 실패와 같은 비대칭을 없애고 에러로 반환한다.
    ThreadSpawn(String),
}

impl std::fmt::Display for ObserverError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ObserverError::UnknownParser(id) => write!(f, "unknown parser: {id}"),
            ObserverError::InvalidPath(p) => write!(f, "invalid sink path: {p}"),
            ObserverError::FileOpen(e) => write!(f, "failed to open sink file: {e}"),
            ObserverError::NotFound(id) => write!(f, "observer not found: {id}"),
            ObserverError::ThreadSpawn(e) => write!(f, "failed to spawn sink thread: {e}"),
        }
    }
}

impl std::error::Error for ObserverError {}

const SINK_CHANNEL_CAP: usize = 256;

impl ObserverRouter {
    pub fn new() -> Self {
        Self::with_counter(std::sync::Arc::new(std::sync::atomic::AtomicU64::new(1)))
    }

    /// engine 들이 공유하는 카운터로 만든다 — production 경로는 이쪽이다.
    pub fn with_counter(next_id: std::sync::Arc<std::sync::atomic::AtomicU64>) -> Self {
        Self {
            next_id,
            observers: HashMap::new(),
            line_buffers: HashMap::new(),
            retired: Vec::new(),
        }
    }

    /// 옵저버 등록. id 반환.
    ///
    /// `memory` 는 Memory sink 용 — Core memory port 의 Arc clone.
    pub fn register(
        &mut self,
        spec: ObserverSpec,
        memory: std::sync::Arc<std::sync::Mutex<dyn tasty_memory::MemoryStorage>>,
    ) -> Result<ObserverId, ObserverError> {
        // 파서 lookup
        let parser_ids: Vec<String> = if spec.parsers.is_empty() {
            DEFAULT_PARSER_IDS.iter().map(|s| s.to_string()).collect()
        } else {
            spec.parsers.clone()
        };
        let mut parser_handles = Vec::with_capacity(parser_ids.len());
        for id in &parser_ids {
            match lookup(id) {
                Some(p) => parser_handles.push(p),
                None => return Err(ObserverError::UnknownParser(id.clone())),
            }
        }

        let id: ObserverId = self
            .next_id
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        // Sink resolved view (외부 응답용)
        let resolved_sink = match &spec.sink {
            SinkSpec::Memory { max_records } => SinkView::Memory {
                max_records: *max_records,
            },
            SinkSpec::File { path } => {
                let resolved = match path {
                    Some(p) => p.clone(),
                    None => default_file_path(id)?,
                };
                SinkView::File { path: resolved }
            }
        };

        // Sink worker spawn
        let (tx, rx) = sync_channel::<ParsedItem>(SINK_CHANNEL_CAP);
        let join = match &resolved_sink {
            SinkView::Memory { max_records } => {
                let cap = *max_records;
                let worker_id = id;
                let mem = memory.clone();
                Some(
                    thread::Builder::new()
                        .name(format!("tasty-observer-mem-{worker_id}"))
                        .spawn(move || run_memory_sink(worker_id, cap, rx, mem))
                        .map_err(|e| ObserverError::ThreadSpawn(e.to_string()))?,
                )
            }
            SinkView::File { path } => {
                let file = OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(path)
                    .map_err(|e| ObserverError::FileOpen(e.to_string()))?;
                let worker_id = id;
                Some(
                    thread::Builder::new()
                        .name(format!("tasty-observer-file-{worker_id}"))
                        .spawn(move || run_file_sink(worker_id, file, rx))
                        .map_err(|e| ObserverError::ThreadSpawn(e.to_string()))?,
                )
            }
        };

        let mut normalized = spec.clone();
        normalized.parsers = parser_ids;

        let kinds_filter = normalized.kinds.clone();

        self.observers.insert(
            id,
            ObserverEntry {
                spec: normalized,
                resolved_sink,
                parser_handles,
                kinds_filter,
                tx,
                join,
                total_in: 0,
                total_out: 0,
                dropped: 0,
                last_event_ms: None,
            },
        );
        Ok(id)
    }

    /// 명시적 해제(`output.observe_stop`). 호출이 돌아온 시점에 sink 가 닫혀
    /// 있기를 기대하는 API 라 **여기서는 join 을 유지한다** — 이 경로는 surface
    /// 수만큼 반복되지 않으므로 per-surface 블로킹 문제와 무관하다. surface close
    /// 로 인한 자동 해제는 [`ObserverRouter::drop_surface`] → [`Self::retire`] 를
    /// 탄다.
    pub fn unregister(&mut self, id: ObserverId) -> Result<(), ObserverError> {
        let entry = self
            .observers
            .remove(&id)
            .ok_or(ObserverError::NotFound(id))?;
        // tx drop → worker recv loop exits → join
        drop(entry.tx);
        if let Some(j) = entry.join
            && let Err(e) = j.join()
        {
            tracing::warn!("observer {id} sink thread panicked: {e:?}");
        }
        Ok(())
    }

    /// surface close 경로의 해제 — sender 만 떨어뜨리고 join 은 뒤로 미룬다.
    ///
    /// **데이터 유실이 없는 이유**: `try_send` 로 채널에 들어간 항목은 sender 가
    /// 전부 drop 된 뒤에도 `Receiver::recv` 가 버퍼를 끝까지 비운 다음에야
    /// `Err` 를 돌려준다(std mpsc 계약). 즉 워커는 여기서 join 하지 않아도 남은
    /// 항목을 모두 sink 에 쓰고 스스로 끝난다. 파일 sink 는 `File` 에 직접
    /// `writeln!` 하므로(`BufWriter` 없음) 유저스페이스에 붙들린 버퍼도 없다.
    /// 유일한 유실 경로는 "워커가 다 쓰기 전에 프로세스가 죽는 것" 이라,
    /// 앱 종료 시 [`Self::join_retired`] 로 반드시 회수한다.
    fn retire(&mut self, id: ObserverId, entry: ObserverEntry) {
        // tx drop → worker recv loop 가 남은 버퍼를 비우고 종료한다.
        drop(entry.tx);
        if let Some(join) = entry.join {
            self.retired.push(RetiredSink { id, join });
        }
    }

    /// 이미 끝난 retired 워커만 논블로킹으로 걷어낸다. 아직 도는 워커는 그대로
    /// 남겨두므로 이 호출은 절대 블로킹하지 않는다.
    fn reap_finished(&mut self) {
        let mut still_running = Vec::with_capacity(self.retired.len());
        for r in self.retired.drain(..) {
            if r.join.is_finished() {
                if let Err(e) = r.join.join() {
                    tracing::warn!("observer {} sink thread panicked: {e:?}", r.id);
                }
            } else {
                still_running.push(r);
            }
        }
        self.retired = still_running;
    }

    /// 남은 retired 워커를 전부 join 한다 — **앱 종료 경로 전용**. surface close 는
    /// join 을 미루므로, 프로세스가 끝나기 전에 여기서 한 번 회수해야 마지막
    /// 항목까지 sink 에 남는다. surface 마다 직렬로 기다리던 것과 달리 워커들이
    /// 그동안 병렬로 이미 배수를 끝냈으므로 여기서의 실제 대기는 거의 0 이다.
    pub fn join_retired(&mut self) {
        for r in self.retired.drain(..) {
            if let Err(e) = r.join.join() {
                tracing::warn!("observer {} sink thread panicked: {e:?}", r.id);
            }
        }
    }

    /// 아직 join 되지 않은 retired 워커 수 — 테스트/진단용.
    #[cfg(test)]
    pub(crate) fn retired_len(&self) -> usize {
        self.retired.len()
    }

    pub fn list(&self) -> Vec<ObserverInfo> {
        self.observers
            .iter()
            .map(|(id, e)| entry_to_info(*id, e))
            .collect()
    }

    pub fn info(&self, id: ObserverId) -> Option<ObserverInfo> {
        self.observers.get(&id).map(|e| entry_to_info(id, e))
    }

    /// 이 surface 의 출력을 보고 싶은 옵저버가 있는가 — terminal emit 게이트 판정.
    /// `dispatch_line` 의 매칭 규칙과 동일해야 한다.
    pub fn wants(&self, surface_id: u32) -> bool {
        self.observers.values().any(|e| match e.spec.surface_id {
            None => true,
            Some(sid) => sid == surface_id,
        })
    }

    /// PTY 가 emit 한 텍스트를 라인 단위로 쪼개 매칭 옵저버에 dispatch하고,
    /// 이번 호출로 완성된 라인들을 반환한다 — hook `OutputMatch` 가 이
    /// 라인 버퍼를 공유해서 쓴다, `HookManager::has_output_match_hook` 가 켠
    /// surface 는 옵저버가 하나도 없어도 라인 분리는 계속된다.
    pub fn dispatch_text(&mut self, surface_id: u32, text: &str) -> Vec<String> {
        let buf = self.line_buffers.entry(surface_id).or_default();
        buf.partial.push_str(text);

        // `'\n'` 으로 라인 분리. 마지막 `\n` 이후는 partial 로 남김.
        let mut completed_lines: Vec<(u32, String)> = Vec::new();
        while let Some(nl) = buf.partial.find('\n') {
            let rest = buf.partial.split_off(nl + 1);
            let mut line = std::mem::replace(&mut buf.partial, rest);
            line.pop(); // remove '\n'
            if line.ends_with('\r') {
                line.pop();
            }
            completed_lines.push((buf.next_idx, line));
            buf.next_idx = buf.next_idx.wrapping_add(1);
        }

        if completed_lines.is_empty() {
            return Vec::new();
        }

        let mut lines = Vec::with_capacity(completed_lines.len());
        for (idx, line) in completed_lines {
            if !self.observers.is_empty() {
                self.dispatch_line(surface_id, idx, &line);
            }
            lines.push(line);
        }
        lines
    }

    fn dispatch_line(&mut self, surface_id: u32, line_idx: u32, line: &str) {
        let matching_ids = self.matching_observer_ids(surface_id);
        if matching_ids.is_empty() {
            return;
        }

        for oid in matching_ids {
            let Some(entry) = self.observers.get_mut(&oid) else {
                continue;
            };
            let items = parse_and_filter_items(entry, line, line_idx);
            if items.is_empty() {
                continue;
            }
            for item in items {
                send_item_to_sink(oid, entry, item);
            }
        }
    }

    /// `surface_id` 를 구독하는(wildcard 포함) 옵저버 id 목록 (borrow 분리).
    fn matching_observer_ids(&self, surface_id: u32) -> Vec<ObserverId> {
        self.observers
            .iter()
            .filter(|(_, e)| match e.spec.surface_id {
                None => true,
                Some(sid) => sid == surface_id,
            })
            .map(|(id, _)| *id)
            .collect()
    }

    /// Surface 가 닫혔을 때 호출. 그 surface 에 매인 옵저버 (wildcard 가
    /// 아닌) 는 자동 종료, line buffer 도 정리.
    ///
    /// 워크스페이스 close 는 이 함수를 leaf surface 수만큼 렌더 스레드에서 직렬
    /// 반복하므로 **여기서 워커를 join 하지 않는다** — sender 만 떨어뜨리고
    /// ([`Self::retire`]) 회수는 뒤로 미룬다. 유실이 없는 근거와 종료 시 회수는
    /// `retire` / [`Self::join_retired`] 문서 참조.
    pub fn drop_surface(&mut self, surface_id: u32) {
        self.line_buffers.remove(&surface_id);
        let tied: Vec<ObserverId> = self
            .observers
            .iter()
            .filter(|(_, e)| e.spec.surface_id == Some(surface_id))
            .map(|(id, _)| *id)
            .collect();
        for id in tied {
            let Some(entry) = self.observers.remove(&id) else {
                continue;
            };
            self.retire(id, entry);
        }
        // 이전 close 에서 미뤄둔 워커 중 이미 끝난 것을 여기서 걷는다(논블로킹).
        self.reap_finished();
    }
}

impl Default for ObserverRouter {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for ObserverRouter {
    fn drop(&mut self) {
        let ids: Vec<ObserverId> = self.observers.keys().copied().collect();
        for id in ids {
            let _ = self.unregister(id); // best-effort on shutdown
        }
        // surface close 가 뒤로 미뤄둔 워커도 여기서 확정 회수한다. 종료 시퀀스의
        // S3b(`shutdown_join_observer_sinks`)가 이미 같은 일을 하지만, 그 호출이
        // 빠지거나 그 단계를 타지 않는 경로로 라우터가 드롭돼도 sink 파일이 잘리지
        // 않도록 하는 마지막 방어선이다 — 덕분에 S3b 는 정확성 요건이 아니라
        // "종료가 늦어지지 않게 미리 걷는" 최적화로 남는다.
        self.join_retired();
    }
}

/// 한 줄을 옵저버의 파서 체인에 통과시키고 kinds_filter 를 적용한다.
fn parse_and_filter_items(entry: &ObserverEntry, line: &str, line_idx: u32) -> Vec<ParsedItem> {
    let mut items: Vec<ParsedItem> = Vec::new();
    for p in &entry.parser_handles {
        p.parse_line(line, line_idx, &mut items);
    }
    if items.is_empty() {
        return items;
    }
    if let Some(filter) = &entry.kinds_filter {
        items.retain(|it| filter.iter().any(|k| k == it.kind));
    }
    items
}

/// 아이템 하나를 옵저버의 sink 채널로 try_send 하고 통계/drop 로깅을 갱신한다.
fn send_item_to_sink(oid: ObserverId, entry: &mut ObserverEntry, item: ParsedItem) {
    entry.total_in += 1;
    match entry.tx.try_send(item) {
        Ok(()) => {
            entry.total_out += 1;
            entry.last_event_ms = Some(unix_ms_now());
        }
        Err(TrySendError::Full(_)) => {
            entry.dropped += 1;
            if entry.dropped.is_multiple_of(1000) {
                tracing::warn!(
                    "observer {oid}: dropped {} items (sink backpressure)",
                    entry.dropped
                );
            }
        }
        Err(TrySendError::Disconnected(_)) => {
            tracing::warn!("observer {oid}: sink worker disconnected");
        }
    }
}

fn entry_to_info(id: ObserverId, entry: &ObserverEntry) -> ObserverInfo {
    ObserverInfo {
        id,
        spec_view: SpecView {
            surface_id: entry.spec.surface_id,
            parsers: entry.spec.parsers.clone(),
            kinds: entry.spec.kinds.clone(),
            sink: entry.resolved_sink.clone(),
        },
        total_in: entry.total_in,
        total_out: entry.total_out,
        dropped: entry.dropped,
        last_event_ms: entry.last_event_ms,
    }
}

fn default_file_path(id: ObserverId) -> Result<PathBuf, ObserverError> {
    let home = tasty_utils::path::tasty_home()
        .ok_or_else(|| ObserverError::InvalidPath("no $HOME / tasty home".to_string()))?;
    let dir = home.join("observers");
    std::fs::create_dir_all(&dir)
        .map_err(|e| ObserverError::InvalidPath(format!("create_dir {dir:?}: {e}")))?;
    Ok(dir.join(format!("{id}.jsonl")))
}

// ── workers ──────────────────────────────────────────────────────────────

fn run_memory_sink(
    observer_id: ObserverId,
    max_records: usize,
    rx: std::sync::mpsc::Receiver<ParsedItem>,
    memory: std::sync::Arc<std::sync::Mutex<dyn tasty_memory::MemoryStorage>>,
) {
    use tasty_memory::{HOST_OWNER, MemoryValue, PutOpts, Scope};
    let mut written_keys: std::collections::VecDeque<String> =
        std::collections::VecDeque::with_capacity(max_records.min(1024));
    // 키마다 붙는 이 sink 의 순번. 밀리초만으로는 같은 ms 에 온 항목이 한 키로 겹쳐
    // 덮어쓰이고, 그 키가 `written_keys` 에 여러 번 들어가 넘칠 때 오래된 칸의 삭제가
    // 같은 이름의 **살아 있는** 레코드를 지운다. 순번이 키를 유일하게 만들어 둘 다 없앤다.
    // 6 자리로 채워 같은 ms 안에서도 키 오름차순이 도착 순서가 된다.
    let mut seq: u64 = 0;
    while let Ok(item) = rx.recv() {
        let now = unix_ms_now();
        let key = format!("tasty.observer.{observer_id}.{now}.{seq:06}");
        seq += 1;
        let record = json!({
            "kind": item.kind,
            "line": item.line,
            "byte_start": item.byte_start,
            "byte_end": item.byte_end,
            "data": item.data,
            "at_ms": now,
        });
        // poison 보고 좌표는 store 의 port 가 준다 — 이 sink 는 port 만 알면 되고
        // 도메인(`crate::core`)을 알 이유가 없다. 좌표가 하나라 `core` 를 거치는 다른
        // 소비자와 첫-1 회 플래그를 나눈다.
        let mut guard = crate::poison::recover_mutex(
            memory.lock(),
            tasty_memory::STORE_LOCK_WHAT,
            &tasty_memory::STORE_LOCK_POISONED,
        );
        let put_result = guard.put(
            HOST_OWNER,
            &Scope::Global,
            &key,
            &MemoryValue::Json(record),
            &PutOpts::default(),
        );
        match put_result {
            Ok(_) => {
                if max_records > 0 {
                    written_keys.push_back(key);
                    while written_keys.len() > max_records {
                        let Some(old) = written_keys.pop_front() else {
                            break;
                        };
                        let _ = guard.delete(HOST_OWNER, &Scope::Global, &old, None); // best-effort evict — 실패해도 다음 put 의 누적 효과로 보정.
                    }
                }
            }
            Err(e) => {
                tracing::warn!("observer {observer_id} memory put failed: {e}");
            }
        }
    }
}

fn run_file_sink(
    observer_id: ObserverId,
    mut file: File,
    rx: std::sync::mpsc::Receiver<ParsedItem>,
) {
    while let Ok(item) = rx.recv() {
        let now = unix_ms_now();
        let record = json!({
            "kind": item.kind,
            "line": item.line,
            "byte_start": item.byte_start,
            "byte_end": item.byte_end,
            "data": item.data,
            "at_ms": now,
        });
        let line = match serde_json::to_string(&record) {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!("observer {observer_id} file sink serialize: {e}");
                continue;
            }
        };
        if let Err(e) = writeln!(file, "{line}") {
            tracing::warn!("observer {observer_id} file sink write: {e}");
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_split_basic() {
        let mut r = ObserverRouter::new();
        // No observers — nothing happens, but buffer machinery shouldn't crash.
        r.dispatch_text(1, "hello\nworld\n");
        r.dispatch_text(1, "partial");
        r.dispatch_text(1, " line\n");
        // ObserverRouter::dispatch_text is the public surface; with no observers
        // there are no externally observable effects. The test just ensures
        // the buffer-and-split path doesn't panic on partial chunks.
    }

    // dispatch_text 의 반환값(완성된 라인)은 OutputMatch 훅이 공유하는
    // 라인 버퍼다 — 옵저버가 하나도 없어도(has_output_match_hook 만으로 게이트가
    // 열린 surface) 정확히 동작해야 한다.
    #[test]
    fn dispatch_text_returns_completed_lines_split_across_chunks() {
        let mut r = ObserverRouter::new();
        // 패턴이 두 청크에 걸쳐 있으면(줄바꿈 전) 완성된 라인이 아직 없다.
        assert_eq!(r.dispatch_text(1, "partial ERR"), Vec::<String>::new());
        // 줄바꿈이 도착해 라인이 완성되면 그제서야 반환된다.
        assert_eq!(
            r.dispatch_text(1, "OR\n"),
            vec!["partial ERROR".to_string()]
        );
    }

    #[test]
    fn dispatch_text_returns_multiple_completed_lines_in_order() {
        let mut r = ObserverRouter::new();
        assert_eq!(
            r.dispatch_text(1, "one\ntwo\nthree"),
            vec!["one".to_string(), "two".to_string()]
        );
        assert_eq!(r.dispatch_text(1, "\n"), vec!["three".to_string()]);
    }

    #[test]
    fn dispatch_text_line_buffers_are_isolated_per_surface() {
        let mut r = ObserverRouter::new();
        assert_eq!(
            r.dispatch_text(1, "surface-one partial"),
            Vec::<String>::new()
        );
        // 다른 surface 의 partial 이 섞여 들어가지 않는다.
        assert_eq!(
            r.dispatch_text(2, "surface-two\n"),
            vec!["surface-two".to_string()]
        );
        assert_eq!(
            r.dispatch_text(1, " completed\n"),
            vec!["surface-one partial completed".to_string()]
        );
    }

    #[test]
    fn wants_matches_observer_registrations() {
        let mut r = ObserverRouter::new();
        assert!(!r.wants(1), "no observers — gate off everywhere");

        let memory: std::sync::Arc<std::sync::Mutex<dyn tasty_memory::MemoryStorage>> =
            std::sync::Arc::new(std::sync::Mutex::new(
                tasty_memory::MemoryStore::open_in_memory().unwrap(),
            ));
        let tied = r
            .register(
                ObserverSpec {
                    surface_id: Some(1),
                    parsers: vec![],
                    kinds: None,
                    sink: SinkSpec::Memory { max_records: 10 },
                },
                memory.clone(),
            )
            .unwrap();
        assert!(r.wants(1), "surface-tied observer enables its surface");
        assert!(!r.wants(2), "other surfaces stay off");

        let wildcard = r
            .register(
                ObserverSpec {
                    surface_id: None,
                    parsers: vec![],
                    kinds: None,
                    sink: SinkSpec::Memory { max_records: 10 },
                },
                memory,
            )
            .unwrap();
        assert!(r.wants(2), "wildcard observer enables every surface");

        r.unregister(wildcard).unwrap();
        assert!(!r.wants(2), "wildcard removed — surface 2 off again");
        r.unregister(tied).unwrap();
        assert!(!r.wants(1), "all observers removed — gate off");
    }

    // ── surface close 경로의 지연 join (per-surface 블로킹 제거) ──

    fn mem_store() -> std::sync::Arc<std::sync::Mutex<dyn tasty_memory::MemoryStorage>> {
        std::sync::Arc::new(std::sync::Mutex::new(
            tasty_memory::MemoryStore::open_in_memory().unwrap(),
        ))
    }

    fn register_file_observer(
        r: &mut ObserverRouter,
        surface_id: u32,
        path: &std::path::Path,
    ) -> ObserverId {
        r.register(
            ObserverSpec {
                surface_id: Some(surface_id),
                parsers: vec!["path".into()],
                kinds: None,
                sink: SinkSpec::File {
                    path: Some(path.to_path_buf()),
                },
            },
            mem_store(),
        )
        .unwrap()
    }

    #[test]
    fn drop_surface_retires_worker_without_joining() {
        let dir = tempfile::tempdir().unwrap();
        let mut r = ObserverRouter::new();
        register_file_observer(&mut r, 1, &dir.path().join("a.jsonl"));
        assert_eq!(r.retired_len(), 0);

        r.drop_surface(1);

        assert!(!r.wants(1), "옵저버는 즉시 등록 해제된다");
        // 워커가 아직 안 끝났으면 retired 에 남고, 이미 끝났으면 reap_finished 가
        // 걷어간다 — 어느 쪽이든 drop_surface 는 블로킹하지 않는다.
        assert!(r.retired_len() <= 1);
        r.join_retired();
        assert_eq!(r.retired_len(), 0, "join_retired 가 전부 회수한다");
    }

    #[test]
    fn retired_workers_flush_everything_accepted_into_the_channel() {
        // 지연 join 의 안전성 근거: `try_send` 로 채널에 들어간 항목은 sender 가
        // 떨어진 뒤에도 워커가 전부 sink 에 쓰고 끝난다(std mpsc 계약).
        let dir = tempfile::tempdir().unwrap();
        let sink = dir.path().join("flush.jsonl");
        let mut r = ObserverRouter::new();
        register_file_observer(&mut r, 7, &sink);

        // 채널 용량(256)보다 적게 보내 backpressure drop 이 끼지 않게 한다.
        let mut expected = 0usize;
        for i in 0..64 {
            r.dispatch_text(7, &format!("/tmp/retire-probe-{i}\n"));
            expected += 1;
        }

        r.drop_surface(7);
        r.join_retired();

        let written = std::fs::read_to_string(&sink).unwrap();
        let lines: Vec<&str> = written.lines().collect();
        assert_eq!(
            lines.len(),
            expected,
            "채널에 수락된 항목은 join 을 미뤄도 하나도 유실되지 않는다"
        );
        assert!(
            lines.last().unwrap().contains("/tmp/retire-probe-63"),
            "마지막 항목까지 기록된다: {:?}",
            lines.last()
        );
    }

    #[test]
    fn dropping_the_router_reaps_retired_workers_even_without_s3b() {
        // S3b(`join_retired`)를 부르지 않고 라우터를 드롭해도 sink 가 잘리지 않는다
        // — `ObserverRouter::drop` 이 마지막 방어선이라 S3b 는 최적화로 남는다.
        let dir = tempfile::tempdir().unwrap();
        let sink = dir.path().join("dropped.jsonl");
        let mut r = ObserverRouter::new();
        register_file_observer(&mut r, 3, &sink);

        let mut expected = 0usize;
        for i in 0..64 {
            r.dispatch_text(3, &format!("/tmp/drop-probe-{i}\n"));
            expected += 1;
        }

        r.drop_surface(3);
        drop(r); // join_retired 를 명시적으로 부르지 않는다.

        let written = std::fs::read_to_string(&sink).unwrap();
        let lines: Vec<&str> = written.lines().collect();
        assert_eq!(
            lines.len(),
            expected,
            "라우터 drop 만으로도 수락된 항목이 전부 기록된다"
        );
        assert!(
            lines.last().unwrap().contains("/tmp/drop-probe-63"),
            "마지막 항목까지 기록된다: {:?}",
            lines.last()
        );
    }

    #[test]
    fn many_surfaces_retire_and_join_once() {
        // 워크스페이스 close 재현 — surface 마다 join 하지 않고 모아뒀다가 한 번에
        // 회수한다. 각 sink 의 마지막 항목이 살아 있어야 한다.
        let dir = tempfile::tempdir().unwrap();
        let mut r = ObserverRouter::new();
        let sids: Vec<u32> = (1..=8).collect();
        for sid in &sids {
            register_file_observer(&mut r, *sid, &dir.path().join(format!("s{sid}.jsonl")));
        }
        for sid in &sids {
            for i in 0..16 {
                r.dispatch_text(*sid, &format!("/tmp/multi-{sid}-{i}\n"));
            }
        }
        for sid in &sids {
            r.drop_surface(*sid);
        }
        r.join_retired();

        for sid in &sids {
            let body = std::fs::read_to_string(dir.path().join(format!("s{sid}.jsonl"))).unwrap();
            let lines: Vec<&str> = body.lines().collect();
            assert_eq!(lines.len(), 16, "surface {sid}");
            assert!(
                lines
                    .last()
                    .unwrap()
                    .contains(&format!("/tmp/multi-{sid}-15")),
                "surface {sid} 마지막 항목 유실"
            );
        }
    }

    #[test]
    fn explicit_unregister_still_joins_synchronously() {
        // `output.observe_stop` 은 호출이 돌아온 시점에 sink 가 닫혀 있기를 기대하는
        // API 라 join 을 유지한다 — surface 수만큼 반복되는 경로가 아니다.
        let dir = tempfile::tempdir().unwrap();
        let sink = dir.path().join("explicit.jsonl");
        let mut r = ObserverRouter::new();
        let id = register_file_observer(&mut r, 3, &sink);
        r.dispatch_text(3, "/tmp/explicit-1\n");

        r.unregister(id).unwrap();

        assert_eq!(r.retired_len(), 0, "명시 해제는 retired 에 쌓이지 않는다");
        let body = std::fs::read_to_string(&sink).unwrap();
        assert!(body.contains("/tmp/explicit-1"), "복귀 시점에 이미 기록됨");
    }

    #[test]
    fn drop_surface_leaves_wildcard_observers_alone() {
        let dir = tempfile::tempdir().unwrap();
        let mut r = ObserverRouter::new();
        let wildcard = r
            .register(
                ObserverSpec {
                    surface_id: None,
                    parsers: vec!["path".into()],
                    kinds: None,
                    sink: SinkSpec::File {
                        path: Some(dir.path().join("wild.jsonl")),
                    },
                },
                mem_store(),
            )
            .unwrap();
        r.drop_surface(5);
        assert!(r.wants(9), "wildcard 는 surface close 로 해제되지 않는다");
        assert_eq!(r.retired_len(), 0);
        r.unregister(wildcard).unwrap();
    }

    #[test]
    fn register_unknown_parser_rejects() {
        let mut r = ObserverRouter::new();
        let memory: std::sync::Arc<std::sync::Mutex<dyn tasty_memory::MemoryStorage>> =
            std::sync::Arc::new(std::sync::Mutex::new(
                tasty_memory::MemoryStore::open_in_memory().unwrap(),
            ));
        let err = r
            .register(
                ObserverSpec {
                    surface_id: None,
                    parsers: vec!["bogus".to_string()],
                    kinds: None,
                    sink: SinkSpec::Memory { max_records: 10 },
                },
                memory,
            )
            .unwrap_err();
        assert!(matches!(err, ObserverError::UnknownParser(_)));
    }

    // ── memory sink 의 저장 계약 ──

    fn item(data: serde_json::Value) -> ParsedItem {
        ParsedItem {
            kind: "path",
            line: 0,
            byte_start: 0,
            byte_end: 1,
            data,
        }
    }

    /// 그 observer 가 남긴 레코드의 `data`, 키 오름차순(= 도착 순서).
    fn observer_records(
        memory: &std::sync::Arc<std::sync::Mutex<dyn tasty_memory::MemoryStorage>>,
        id: ObserverId,
    ) -> Vec<serde_json::Value> {
        let prefix = format!("tasty.observer.{id}.");
        memory
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .list(
                &tasty_memory::Scope::Global,
                &tasty_memory::ListOpts {
                    prefix: Some(prefix),
                    ..Default::default()
                },
            )
            .unwrap()
            .into_iter()
            .map(|e| match e.value {
                tasty_memory::MemoryValue::Json(v) => v["data"].clone(),
                other => panic!("observer 레코드는 JSON 이다: {other:?}"),
            })
            .collect()
    }

    /// sink 의 poison 보고 좌표는 `core` 를 거치는 소비자와 **같은 한 벌**이다.
    ///
    /// 좌표를 port 로 옮기면서 두 벌이 되면 같은 poison 이 두 번 보고되고 어느 쪽도
    /// "첫 1 회" 가 아니게 된다 — 이름을 잇는 재수출이 끊기면 여기가 죽는다.
    #[test]
    fn the_memory_sink_shares_one_poison_coordinate_with_the_core() {
        assert!(std::ptr::eq(
            &tasty_memory::STORE_LOCK_POISONED,
            &crate::core::MEMORY_POISONED
        ));
        assert_eq!(tasty_memory::STORE_LOCK_WHAT, crate::core::MEMORY_WHAT);
    }

    /// put 이 실패한 레코드는 버려지고 **sink 는 멈추지 않는다** — 다음 레코드는 쓰인다.
    ///
    /// 소비자에게 gap 신호는 가지 않는다(경고 로그뿐) — 그것이 이 sink 의 계약이고
    /// `docs/design/systems/storage.md` 의 observer sink 절이 적는다. sink 가 첫 실패에서
    /// 끝나면 뒤 레코드가 전부 사라지고, 그것은 조용한 손실이 한 건이 아니라 무한이다.
    #[test]
    fn a_failed_put_drops_that_record_and_the_sink_keeps_going() {
        let config = tasty_memory::MemoryConfig {
            entry_max_bytes: 512,
            ..Default::default()
        };
        let memory: std::sync::Arc<std::sync::Mutex<dyn tasty_memory::MemoryStorage>> =
            std::sync::Arc::new(std::sync::Mutex::new(
                tasty_memory::MemoryStore::open_in_memory_with_config(config).unwrap(),
            ));
        let (tx, rx) = sync_channel::<ParsedItem>(4);
        tx.send(item(json!({ "big": "x".repeat(4096) }))).unwrap();
        tx.send(item(json!({ "small": 1 }))).unwrap();
        drop(tx);

        run_memory_sink(41, 10, rx, memory.clone());

        let records = observer_records(&memory, 41);
        assert_eq!(
            records,
            vec![json!({ "small": 1 })],
            "실패한 레코드가 쓰였거나 그 뒤 레코드가 안 쓰였다"
        );
    }

    /// 상한을 넘기면 **가장 최근 N 건**이 남는다 — 같은 밀리초에 몰려 온 항목도 한 건씩 센다.
    ///
    /// 키가 밀리초만이던 때는 같은 ms 의 항목이 한 키로 겹쳐, 넘친 옛 칸의 삭제가 같은 이름의
    /// 살아 있는 레코드를 지웠다(`max_records=2` 에 여섯 건 → 남은 레코드 0). 여섯 건을 채널에
    /// 먼저 다 넣고 sink 를 돌려 대부분이 같은 ms 에 쓰이게 한다.
    #[test]
    fn the_sink_keeps_the_latest_records_even_within_one_millisecond() {
        let memory = mem_store();
        let (tx, rx) = sync_channel::<ParsedItem>(6);
        for n in 0..6 {
            tx.send(item(json!({ "n": n }))).unwrap();
        }
        drop(tx);
        run_memory_sink(43, 2, rx, memory.clone());
        assert_eq!(
            observer_records(&memory, 43),
            vec![json!({ "n": 4 }), json!({ "n": 5 })],
        );

        // 상한이 없으면 같은 ms 의 항목도 하나도 덮어쓰이지 않는다.
        let (tx, rx) = sync_channel::<ParsedItem>(6);
        for n in 0..6 {
            tx.send(item(json!({ "n": n }))).unwrap();
        }
        drop(tx);
        run_memory_sink(44, 0, rx, memory.clone());
        assert_eq!(
            observer_records(&memory, 44),
            (0..6).map(|n| json!({ "n": n })).collect::<Vec<_>>(),
        );
    }

    /// 락이 poison 돼도 sink 는 복구해서 쓴다 — 보고는 port 의 좌표로 간다.
    #[test]
    fn a_poisoned_store_lock_is_recovered_by_the_sink() {
        let memory = mem_store();
        let holder = memory.clone();
        let poisoned = std::thread::spawn(move || {
            let _guard = holder.lock().unwrap();
            panic!("poison the store lock on purpose");
        })
        .join();
        assert!(poisoned.is_err());
        assert!(memory.is_poisoned());

        let (tx, rx) = sync_channel::<ParsedItem>(1);
        tx.send(item(json!({ "n": 1 }))).unwrap();
        drop(tx);
        run_memory_sink(42, 10, rx, memory.clone());

        assert_eq!(observer_records(&memory, 42), vec![json!({ "n": 1 })]);
        assert!(
            tasty_memory::STORE_LOCK_POISONED.load(std::sync::atomic::Ordering::Relaxed),
            "복구가 port 의 좌표로 보고되지 않았다"
        );
    }
}
