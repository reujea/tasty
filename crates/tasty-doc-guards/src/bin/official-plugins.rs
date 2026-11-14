//! 공식 플러그인 잠금 파일(`official-plugins.lock.toml`)을 만들고 읽는다.
//! 셸·PowerShell 스크립트가 TOML을 직접 읽지 않도록 이 도구가 잠금 파일을 줄 단위 출력으로 바꾼다.
//! 네트워크는 쓰지 않는다. 내려받기는 호출하는 스크립트(`scripts/fetch-official-plugins.sh`)가 한다.
//!
//! ```text
//! official-plugins generate --meta <release-meta.toml> --tsv <plugins-<p>.tsv>... --out <lock>
//! official-plugins check [--lock <lock>]
//! official-plugins plan --platform <p> [--all] [--lock <lock>]
//! official-plugins verify --dir <받은 폴더> --platform <p> [--all] [--lock <lock>]
//! official-plugins unpack --dir <받은 폴더> --out <스테이징 폴더> --platform <p> [--all] [--lock <lock>]
//! ```
//!
//! `--lock`을 생략하면 현재 디렉터리의 `official-plugins.lock.toml`을 읽는다.
//! `--all`이 없으면 `bundle = true` 플러그인만 다룬다.
//! `plan`은 플러그인마다 `id\tversion\tfile\turl\tsha256` 한 줄을 출력한다.
//! `unpack`은 먼저 `verify`와 같은 대조를 하고, 시스템 `tar`로 풀어 `<out>/<id>/`에 둔다.
//! 이미 있는 `<out>/<id>/`는 새것으로 바꾼다. 아카이브의 최상위는 `<id>/` 하나여야 하고,
//! 그 아래 `tasty-plugin.toml`과 실행 파일이 있어야 하며, 매니페스트의 id·version이 잠금 파일과
//! 같아야 한다. 그 밖의 파일(언어 파일, 고지 파일 등)은 목록을 고정하지 않는다.
//! 종료코드: 0은 통과, 1은 검사 실패(형식·완비·해시·누락·구조), 2는 인자·읽기·쓰기 오류다.

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use tasty_doc_guards::official_plugins::{
    LOCK_FILE, Lock, PlanItem, check, generate, manifest_top_level_str, parse_lock, plan, render,
};
use tasty_doc_guards::sha256::sha256_hex;

const USAGE: &str = "usage: official-plugins <generate|check|plan|verify|unpack> [options]\n\
  generate --meta <release-meta.toml> --tsv <tsv>... --out <lock>\n\
  check [--lock <lock>]\n\
  plan --platform <p> [--all] [--lock <lock>]\n\
  verify --dir <dir> --platform <p> [--all] [--lock <lock>]\n\
  unpack --dir <dir> --out <out> --platform <p> [--all] [--lock <lock>]";

/// 인자·입출력 오류. 종료코드 2.
struct UsageError(String);

#[derive(Default)]
struct Args {
    lock: Option<PathBuf>,
    meta: Option<PathBuf>,
    tsvs: Vec<PathBuf>,
    out: Option<PathBuf>,
    dir: Option<PathBuf>,
    platform: Option<String>,
    all: bool,
}

fn parse_args(rest: &[String]) -> Result<Args, UsageError> {
    let mut args = Args::default();
    let mut it = rest.iter();
    while let Some(flag) = it.next() {
        let mut value = || {
            it.next()
                .cloned()
                .ok_or_else(|| UsageError(format!("{flag} 뒤에 값이 없다")))
        };
        match flag.as_str() {
            "--lock" => args.lock = Some(PathBuf::from(value()?)),
            "--meta" => args.meta = Some(PathBuf::from(value()?)),
            "--tsv" => args.tsvs.push(PathBuf::from(value()?)),
            "--out" => args.out = Some(PathBuf::from(value()?)),
            "--dir" => args.dir = Some(PathBuf::from(value()?)),
            "--platform" => args.platform = Some(value()?),
            "--all" => args.all = true,
            other => return Err(UsageError(format!("모르는 인자: {other}"))),
        }
    }
    Ok(args)
}

fn read(path: &Path) -> Result<String, UsageError> {
    std::fs::read_to_string(path)
        .map_err(|e| UsageError(format!("{}를 읽지 못했다: {e}", path.display())))
}

fn required<'a, T>(value: &'a Option<T>, flag: &str) -> Result<&'a T, UsageError> {
    value
        .as_ref()
        .ok_or_else(|| UsageError(format!("{flag}가 필요하다")))
}

/// 잠금 파일을 읽고 검사한다. 형식·완비 문제가 있으면 출력하고 1로 끝낸다.
fn load_lock(args: &Args) -> Result<Result<Lock, ExitCode>, UsageError> {
    let path = args
        .lock
        .clone()
        .unwrap_or_else(|| PathBuf::from(LOCK_FILE));
    let text = read(&path)?;
    let lock = match parse_lock(&text) {
        Ok(lock) => lock,
        Err(errors) => {
            report(&path, &errors);
            return Ok(Err(ExitCode::from(1)));
        }
    };
    let problems = check(&lock);
    if !problems.is_empty() {
        report(&path, &problems);
        return Ok(Err(ExitCode::from(1)));
    }
    Ok(Ok(lock))
}

fn report(path: &Path, problems: &[String]) {
    eprintln!(
        "[official-plugins] {}: 문제 {}건",
        path.display(),
        problems.len()
    );
    for p in problems {
        eprintln!("  {p}");
    }
}

fn cmd_generate(args: &Args) -> Result<ExitCode, UsageError> {
    let meta = read(required(&args.meta, "--meta")?)?;
    let out = required(&args.out, "--out")?;
    if args.tsvs.is_empty() {
        return Err(UsageError("--tsv가 하나 이상 필요하다".into()));
    }
    let mut tsvs = Vec::new();
    for path in &args.tsvs {
        tsvs.push((path.display().to_string(), read(path)?));
    }
    match generate(&meta, &tsvs) {
        Ok(lock) => {
            std::fs::write(out, render(&lock))
                .map_err(|e| UsageError(format!("{}에 쓰지 못했다: {e}", out.display())))?;
            let artifacts: usize = lock.plugins.iter().map(|p| p.artifacts.len()).sum();
            eprintln!(
                "[official-plugins] {} {}: 플러그인 {} · 산출물 {} → {}",
                lock.release.repository,
                lock.release.tag,
                lock.plugins.len(),
                artifacts,
                out.display()
            );
            Ok(ExitCode::SUCCESS)
        }
        Err(errors) => {
            report(Path::new("generate"), &errors);
            Ok(ExitCode::from(1))
        }
    }
}

fn planned(args: &Args, lock: &Lock) -> Result<Vec<PlanItem>, UsageError> {
    let platform = required(&args.platform, "--platform")?;
    plan(lock, platform, args.all).map_err(UsageError)
}

/// 받은 폴더의 파일을 잠금 파일의 SHA-256과 대조한다. 문제 줄마다 파일 이름을 담는다.
fn verify_items(dir: &Path, items: &[PlanItem]) -> Vec<String> {
    let mut problems = Vec::new();
    for item in items {
        let path = dir.join(&item.file);
        match std::fs::read(&path) {
            Ok(bytes) => {
                let got = sha256_hex(&bytes);
                if got != item.sha256 {
                    problems.push(format!(
                        "sha256 mismatch: {} (expected {}, got {got})",
                        item.file, item.sha256
                    ));
                }
            }
            Err(e) => problems.push(format!("missing: {} ({e})", item.file)),
        }
    }
    problems
}

fn cmd_verify(args: &Args, lock: &Lock) -> Result<ExitCode, UsageError> {
    let dir = required(&args.dir, "--dir")?;
    let items = planned(args, lock)?;
    let problems = verify_items(dir, &items);
    if problems.is_empty() {
        eprintln!("[official-plugins] verified {} file(s)", items.len());
        Ok(ExitCode::SUCCESS)
    } else {
        report(dir, &problems);
        Ok(ExitCode::from(1))
    }
}

/// 압축을 푼 `<tmp>/<id>/`의 구조와 매니페스트 id·version을 확인한다.
fn check_unpacked(tmp: &Path, item: &PlanItem) -> Vec<String> {
    let mut problems = Vec::new();
    match std::fs::read_dir(tmp) {
        Ok(entries) => {
            let names: Vec<String> = entries
                .flatten()
                .map(|e| e.file_name().to_string_lossy().to_string())
                .collect();
            if names != [item.id.clone()] {
                problems.push(format!(
                    "{}: 최상위 항목이 {}/ 하나가 아니다: {names:?}",
                    item.file, item.id
                ));
                return problems;
            }
        }
        Err(e) => {
            problems.push(format!("{}: 푼 폴더를 읽지 못했다: {e}", item.file));
            return problems;
        }
    }
    let root = tmp.join(&item.id);
    let exe = root.join(&item.executable);
    if !exe.is_file() {
        problems.push(format!(
            "{}: 실행 파일 {} 이 없다",
            item.file, item.executable
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = std::fs::metadata(&exe)
            && meta.permissions().mode() & 0o111 == 0
        {
            problems.push(format!(
                "{}: {} 에 실행 권한이 없다",
                item.file, item.executable
            ));
        }
    }
    match std::fs::read_to_string(root.join("tasty-plugin.toml")) {
        Ok(text) => {
            for (key, want) in [("id", &item.id), ("version", &item.version)] {
                let got = manifest_top_level_str(&text, key);
                if got.as_deref() != Some(want.as_str()) {
                    problems.push(format!(
                        "{}: tasty-plugin.toml {key} = {got:?}, 잠금 파일은 {want}",
                        item.file
                    ));
                }
            }
        }
        Err(e) => problems.push(format!("{}: tasty-plugin.toml 이 없다 ({e})", item.file)),
    }
    problems
}

fn extract(archive_dir: &Path, item: &PlanItem, tmp: &Path) -> Result<(), String> {
    // -f 에는 상대 이름을 준다. GNU tar 는 `C:` 처럼 콜론이 든 -f 인자를 원격 호스트로 읽는다.
    let out = Command::new("tar")
        .current_dir(archive_dir)
        .arg("-xzf")
        .arg(&item.file)
        .arg("-C")
        .arg(tmp)
        .output()
        .map_err(|e| format!("{}: tar 를 실행하지 못했다: {e}", item.file))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(format!(
            "{}: tar 실패 ({}): {}",
            item.file,
            out.status,
            String::from_utf8_lossy(&out.stderr).trim()
        ))
    }
}

fn io(what: &str, path: &Path, e: std::io::Error) -> UsageError {
    UsageError(format!("{what} {}: {e}", path.display()))
}

fn cmd_unpack(args: &Args, lock: &Lock) -> Result<ExitCode, UsageError> {
    let dir = required(&args.dir, "--dir")?;
    let out = required(&args.out, "--out")?;
    let items = planned(args, lock)?;
    let problems = verify_items(dir, &items);
    if !problems.is_empty() {
        report(dir, &problems);
        return Ok(ExitCode::from(1));
    }
    std::fs::create_dir_all(out).map_err(|e| io("폴더를 만들지 못했다", out, e))?;
    let tmp = out.join(".official-plugins-unpack");
    let mut failures = Vec::new();
    for item in &items {
        if tmp.exists() {
            std::fs::remove_dir_all(&tmp).map_err(|e| io("지우지 못했다", &tmp, e))?;
        }
        std::fs::create_dir_all(&tmp).map_err(|e| io("폴더를 만들지 못했다", &tmp, e))?;
        if let Err(e) = extract(dir, item, &tmp) {
            failures.push(e);
            continue;
        }
        let problems = check_unpacked(&tmp, item);
        if !problems.is_empty() {
            failures.extend(problems);
            continue;
        }
        let dest = out.join(&item.id);
        if dest.exists() {
            std::fs::remove_dir_all(&dest).map_err(|e| io("지우지 못했다", &dest, e))?;
        }
        std::fs::rename(tmp.join(&item.id), &dest).map_err(|e| io("옮기지 못했다", &dest, e))?;
        println!("staged\t{}\t{}", item.id, item.version);
    }
    if tmp.exists() {
        std::fs::remove_dir_all(&tmp).map_err(|e| io("지우지 못했다", &tmp, e))?;
    }
    if failures.is_empty() {
        Ok(ExitCode::SUCCESS)
    } else {
        report(out, &failures);
        Ok(ExitCode::from(1))
    }
}

fn run(argv: &[String]) -> Result<ExitCode, UsageError> {
    let Some((cmd, rest)) = argv.split_first() else {
        return Err(UsageError("하위 명령이 없다".into()));
    };
    let args = parse_args(rest)?;
    if cmd == "generate" {
        return cmd_generate(&args);
    }
    let lock = match load_lock(&args)? {
        Ok(lock) => lock,
        Err(code) => return Ok(code),
    };
    match cmd.as_str() {
        "check" => {
            let artifacts: usize = lock.plugins.iter().map(|p| p.artifacts.len()).sum();
            eprintln!(
                "[official-plugins] {} {}: 플러그인 {} · 산출물 {} · 문제 0",
                lock.release.repository,
                lock.release.tag,
                lock.plugins.len(),
                artifacts
            );
            Ok(ExitCode::SUCCESS)
        }
        "plan" => {
            for item in planned(&args, &lock)? {
                println!(
                    "{}\t{}\t{}\t{}\t{}",
                    item.id, item.version, item.file, item.url, item.sha256
                );
            }
            Ok(ExitCode::SUCCESS)
        }
        "verify" => cmd_verify(&args, &lock),
        "unpack" => cmd_unpack(&args, &lock),
        other => Err(UsageError(format!("모르는 하위 명령: {other}"))),
    }
}

fn main() -> ExitCode {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    match run(&argv) {
        Ok(code) => code,
        Err(UsageError(msg)) => {
            eprintln!("[official-plugins] {msg}\n{USAGE}");
            ExitCode::from(2)
        }
    }
}
