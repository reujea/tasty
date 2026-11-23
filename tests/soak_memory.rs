//! 장시간 반복 작업 중 프로세스 트리 RSS, 핸들·fd, 자식 프로세스, GPU 자원 수를 JSONL로 기록한다.
//! 기본 실행에서는 제외하며 명시적으로 실행한다:
//!
//! ```bash
//! SOAK_SCENARIO=s9 SOAK_DURATION_SECS=86400 \
//!   cargo test --release --test soak_memory -- --ignored --nocapture
//! ```
//!
//! 이 하네스는 누수 여부를 판정하지 않는다. scripts/soak/analyze.py가 워밍업 이후 기울기와 기준선 복귀를 분석한다.
//! 절차는 docs/dev-guide/memory-leak-soak.md를 따른다.
//!
//! - SOAK_SCENARIO: s1|s2|s4|s6|s7|s8|s9, 기본 s9 혼합 작업.
//! - SOAK_DURATION_SECS: 실행 시간, 기본 600초.
//! - SOAK_CYCLES: 반복 횟수 상한, 기본은 시간으로만 제한.
//! - SOAK_CHECKPOINT_EVERY: 기록 간격, 기본 10회.
//! - SOAK_OUT_DIR: 출력 디렉터리, 기본은 OS 임시 디렉터리 아래 tasty-soak. 실제 경로는 시작 때 출력한다.

mod common;

use std::collections::BTreeMap;
use std::io::Write;
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use common::TastyInstance;
use serde_json::{Value, json};

fn env_u64(key: &str, default: u64) -> u64 {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn now_epoch() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

fn sample_process_tree(root_pid: u32) -> Value {
    use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System};
    let mut sys = System::new();
    // Linux 스레드 항목은 프로세스 RSS를 다시 보고하므로 tasks를 제외해 중복 합산하지 않는다.
    sys.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::nothing().with_memory(),
    );
    let procs = sys.processes();

    let mut children: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
    for (pid, p) in procs {
        // refresh 설정과 별도로 스레드 항목을 제외한다.
        if p.thread_kind().is_some() {
            continue;
        }
        if let Some(parent) = p.parent() {
            children
                .entry(parent.as_u32())
                .or_default()
                .push(pid.as_u32());
        }
    }
    let mut tree: Vec<u32> = vec![root_pid];
    let mut queue: Vec<u32> = vec![root_pid];
    while let Some(pid) = queue.pop() {
        if let Some(kids) = children.get(&pid) {
            for k in kids {
                tree.push(*k);
                queue.push(*k);
            }
        }
    }

    let mut rss_total: u64 = 0;
    let mut root_rss: u64 = 0;
    let mut by_name: BTreeMap<String, usize> = BTreeMap::new();
    let mut rss_by_name: BTreeMap<String, u64> = BTreeMap::new();
    for pid in &tree {
        if let Some(p) = procs.get(&sysinfo::Pid::from_u32(*pid)) {
            rss_total += p.memory();
            if *pid == root_pid {
                root_rss = p.memory();
            } else {
                let name = p.name().to_string_lossy().into_owned();
                *by_name.entry(name.clone()).or_insert(0) += 1;
                // 어느 자식 종류에서 메모리가 늘었는지 비교하도록 이름별로도 합산한다.
                *rss_by_name.entry(name).or_insert(0) += p.memory();
            }
        }
    }
    json!({
        "rss_tree_bytes": rss_total,
        "rss_root_bytes": root_rss,
        "proc_count": tree.len(),
        "children_by_name": by_name,
        "children_rss_by_name": rss_by_name,
    })
}

/// 체크포인트에서 루트 프로세스의 핸들 또는 fd 수를 수집한다.
fn sample_handle_count(pid: u32) -> Option<u64> {
    #[cfg(target_os = "windows")]
    {
        let out = std::process::Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                &format!("(Get-Process -Id {pid}).HandleCount"),
            ])
            .output()
            .ok()?;
        String::from_utf8_lossy(&out.stdout).trim().parse().ok()
    }
    #[cfg(target_os = "linux")]
    {
        std::fs::read_dir(format!("/proc/{pid}/fd"))
            .ok()
            .map(|d| d.count() as u64)
    }
    #[cfg(target_os = "macos")]
    {
        let out = std::process::Command::new("lsof")
            .args(["-p", &pid.to_string()])
            .output()
            .ok()?;
        Some(String::from_utf8_lossy(&out.stdout).lines().count() as u64)
    }
}

fn cycle_tab_churn(inst: &TastyInstance, pane_id: u64) {
    let r = inst.call("tab.create", json!({ "pane_id": pane_id }));
    let surface_id = r["surface_id"]
        .as_u64()
        .expect("tab.create returned surface_id");
    let tabs = inst.call("tab.list", json!({ "pane_id": pane_id }));
    let tab_id = tabs["tabs"]
        .as_array()
        .and_then(|arr| {
            arr.iter()
                .find(|t| t["surface_id"].as_u64() == Some(surface_id))
        })
        .and_then(|t| t["id"].as_u64())
        .expect("created tab not found in tab.list");
    // 생성 직후 닫지 않도록 화면에 출력이 나타날 때까지 기다린다. 프롬프트 종류까지 확인하지는 않는다.
    let deadline = Instant::now() + Duration::from_secs(20);
    assert!(
        tasty_utils::test_wait::holds_by_every(deadline, Duration::from_millis(50), |_| !inst
            .screen_text_of(surface_id)
            .trim()
            .is_empty()),
        "S1: surface {surface_id} never became ready"
    );
    inst.call("tab.close", json!({ "tab_id": tab_id }));
    std::thread::sleep(Duration::from_millis(100));
}

fn cycle_split_churn(inst: &TastyInstance, surface0: u64) {
    let r = inst.call(
        "split",
        json!({ "level": "surface", "target_surface": surface0, "direction": "vertical" }),
    );
    let new_sid = r["new_surface_id"]
        .as_u64()
        .expect("split returned new_surface_id");
    let deadline = Instant::now() + Duration::from_secs(20);
    assert!(
        tasty_utils::test_wait::holds_by_every(deadline, Duration::from_millis(50), |_| !inst
            .screen_text_of(new_sid)
            .trim()
            .is_empty()),
        "S2: surface {new_sid} never became ready"
    );
    inst.call("surface.close", json!({ "surface_id": new_sid }));
    std::thread::sleep(Duration::from_millis(100));
}

#[derive(Clone, Copy, Debug)]
enum ViewKind {
    Explorer,
    Image,
    Markdown,
}

impl ViewKind {
    fn nth(n: u64) -> Self {
        match n % 3 {
            0 => Self::Explorer,
            1 => Self::Image,
            _ => Self::Markdown,
        }
    }
}

fn sum_over_windows(gpu: &Value, pick: impl Fn(&Value) -> u64) -> u64 {
    gpu["windows"]
        .as_array()
        .map(|ws| ws.iter().map(&pick).sum())
        .unwrap_or(0)
}

/// explorer view와 image mesh target의 창별 합이다. 해당 서피스의 렌더 완료를 직접 확인하는 값은 아니다. Markdown은 대응 카운터가 없어 None이다.
fn rendered_count(inst: &TastyInstance, kind: ViewKind) -> Option<u64> {
    let gpu = inst.call("system.gpu_stats", json!({}));
    match kind {
        ViewKind::Explorer => Some(sum_over_windows(&gpu, |w| {
            w["explorer_views"].as_u64().unwrap_or(0)
        })),
        ViewKind::Image => Some(sum_over_windows(&gpu, |w| {
            w["stats"]["egui_mesh_targets"].as_u64().unwrap_or(0)
        })),
        ViewKind::Markdown => None,
    }
}

/// 새 탭은 자동 선택되지 않으므로 보이는 탭의 서피스를 분할해 연다.
/// Explorer·Image는 자원 수 증가를 확인한 뒤 닫는다. Markdown은 대응 카운터가 없어 고정 시간만 기다린다.
fn cycle_plugin_view_churn(inst: &TastyInstance, surface0: u64, kind: ViewKind) {
    let before = rendered_count(inst, kind);
    let mut params = json!({
        "level": "surface",
        "target_surface": surface0,
        "direction": "vertical",
    });
    let root = env!("CARGO_MANIFEST_DIR");
    match kind {
        ViewKind::Explorer => {
            params["type"] = json!("explorer");
            params["path"] = json!(root);
        }
        ViewKind::Image => {
            params["type"] = json!("image");
            params["file"] = json!(format!("{root}/assets/icons/icon_256.png"));
        }
        ViewKind::Markdown => {
            params["type"] = json!("markdown");
            params["file"] = json!(format!("{root}/README.md"));
        }
    }
    let r = inst.call("split", params);
    let new_sid = r["new_surface_id"]
        .as_u64()
        .expect("split returned new_surface_id");
    match before {
        Some(before) => {
            let deadline = Instant::now() + Duration::from_secs(30);
            let mut now = 0;
            assert!(
                tasty_utils::test_wait::holds_by_every(
                    deadline,
                    Duration::from_millis(100),
                    |_| {
                        now = rendered_count(inst, kind).unwrap_or(0);
                        now > before
                    }
                ),
                "S6: {kind:?} surface {new_sid} resource count did not increase (count {now}, before {before})"
            );
        }
        None => std::thread::sleep(Duration::from_millis(700)),
    }
    inst.call("surface.close", json!({ "surface_id": new_sid }));
    std::thread::sleep(Duration::from_millis(100));
}

/// not found 출력이 나온 재시도 횟수다. 입력 유실의 단서로 기록하지만 이 문자열만으로 첫 바이트 유실을 확정하지는 않는다.
static INPUT_INCIDENTS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn cycle_heavy_output(inst: &TastyInstance, surface_id: u64) {
    for attempt in 0..3 {
        inst.set_mark(surface_id);
        // mark 이후 출력이 보존 범위를 넘지 않도록 출력량을 격리 설정의 스크롤백 상한보다 작게 둔다.
        inst.send_text(surface_id, "seq 1 5000\n");
        let start = Instant::now();
        let mut past = false;
        loop {
            let out = inst.read_since_mark(surface_id);
            // 명령 자체의 에코에는 없는 출력값으로 진행을 확인한다.
            if out.contains("4999") {
                return;
            }
            if out.contains("not found") {
                INPUT_INCIDENTS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                eprintln!("S4: input-loss incident (attempt {attempt}) — first byte dropped");
                std::thread::sleep(Duration::from_millis(500));
                break; // 재시도
            }
            // 기한이 지난 것을 본 뒤에도 한 번 더 읽고 판정한다. 기한 전 거짓 읽기만으로 실패하지 않는다.
            assert!(!past, "S4: timeout waiting for output; got:\n{out}");
            past = start.elapsed() >= Duration::from_secs(60);
            if !past {
                std::thread::sleep(Duration::from_millis(50));
            }
        }
    }
    panic!("S4: command-not-found output persisted after 3 attempts");
}

fn cycle_ipc_churn(inst: &TastyInstance) {
    for _ in 0..25 {
        inst.call("surface.list", json!({}));
        inst.call("system.info", json!({}));
    }
}

fn cycle_idle() {
    std::thread::sleep(Duration::from_secs(30));
}

fn checkpoint(inst: &TastyInstance, scenario: &str, cycle: u64) -> Value {
    // 지연 정리가 반영될 시간을 둔다. 고정 대기이므로 정리 완료를 동기화하지는 않는다.
    std::thread::sleep(Duration::from_secs(2));
    let gpu = inst.call("system.gpu_stats", json!({}));
    let surfaces = inst
        .call("surface.list", json!({}))
        .as_array()
        .map(|a| a.len())
        .unwrap_or(0);
    let tree = sample_process_tree(inst.pid());
    json!({
        "ts": now_epoch(),
        "scenario": scenario,
        "cycle": cycle,
        "tree": tree,
        "handles": sample_handle_count(inst.pid()),
        "surfaces": surfaces,
        "gpu": gpu,
        "input_incidents": INPUT_INCIDENTS.load(std::sync::atomic::Ordering::Relaxed),
    })
}

#[test]
#[ignore = "장시간 soak — SOAK_* env 로 명시 실행 (docs/dev-guide/memory-leak-soak.md)"]
fn soak() {
    let scenario = std::env::var("SOAK_SCENARIO").unwrap_or_else(|_| "s9".into());
    let duration = Duration::from_secs(env_u64("SOAK_DURATION_SECS", 600));
    let max_cycles = env_u64("SOAK_CYCLES", u64::MAX);
    let checkpoint_every = env_u64("SOAK_CHECKPOINT_EVERY", 10).max(1);
    let out_dir = std::env::var_os("SOAK_OUT_DIR")
        .map(PathBuf::from)
        // 이유: 수동 실행의 공용 기본 출력 폴더다. 파일명은 시나리오와 초 단위 시각이므로 같은 시나리오를 동시에 실행하면 충돌할 수 있다.
        .unwrap_or_else(|| std::env::temp_dir().join("tasty-soak"));

    std::fs::create_dir_all(&out_dir).expect("failed to create SOAK_OUT_DIR");
    let out_path = out_dir
        .join(format!("soak-{}-{}.jsonl", scenario, now_epoch() as u64))
        .display()
        .to_string();
    let mut out = std::fs::File::create(&out_path).expect("failed to create output file");

    let inst = TastyInstance::spawn();
    let pane_id = inst.first_pane_id();
    let surface0 = inst.first_surface_id();

    let meta = json!({
        "meta": {
            "ts": now_epoch(),
            "scenario": scenario,
            "duration_secs": duration.as_secs(),
            "checkpoint_every": checkpoint_every,
            "os": std::env::consts::OS,
            "arch": std::env::consts::ARCH,
            "profile": if cfg!(debug_assertions) { "debug" } else { "release" },
            "pid": inst.pid(),
        }
    });
    writeln!(out, "{meta}").unwrap();
    println!("soak: scenario={scenario} duration={duration:?} out={out_path}");

    let start = Instant::now();
    let mut cycle: u64 = 0;
    while start.elapsed() < duration && cycle < max_cycles {
        match scenario.as_str() {
            "s1" => cycle_tab_churn(&inst, pane_id),
            "s2" => cycle_split_churn(&inst, surface0),
            "s4" => cycle_heavy_output(&inst, surface0),
            "s6" => cycle_plugin_view_churn(&inst, surface0, ViewKind::nth(cycle)),
            "s7" => cycle_ipc_churn(&inst),
            "s8" => cycle_idle(),
            "s9" => match cycle % 8 {
                0 | 1 => cycle_tab_churn(&inst, pane_id),
                2 => cycle_split_churn(&inst, surface0),
                3 => cycle_heavy_output(&inst, surface0),
                4 => cycle_plugin_view_churn(&inst, surface0, ViewKind::nth(cycle / 8)),
                5 | 6 => cycle_ipc_churn(&inst),
                _ => std::thread::sleep(Duration::from_secs(5)),
            },
            other => panic!("unknown SOAK_SCENARIO '{other}' (s1|s2|s4|s6|s7|s8|s9)"),
        }
        cycle += 1;
        if cycle.is_multiple_of(checkpoint_every) {
            let cp = checkpoint(&inst, &scenario, cycle);
            writeln!(out, "{cp}").unwrap();
            out.flush().unwrap();
            println!(
                "cycle {cycle}: rss_tree={}MB procs={} handles={:?}",
                cp["tree"]["rss_tree_bytes"].as_u64().unwrap_or(0) / 1_048_576,
                cp["tree"]["proc_count"],
                cp["handles"],
            );
        }
    }

    let cp = checkpoint(&inst, &scenario, cycle);
    writeln!(out, "{cp}").unwrap();
    out.flush().unwrap();
    println!("soak done: {cycle} cycles, output at {out_path}");
}
