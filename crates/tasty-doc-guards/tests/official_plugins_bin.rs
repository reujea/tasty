//! `official-plugins` bin을 합성 릴리스로 실행해 종료 상태와 출력을 확인한다.
//! 합성 아카이브는 시스템 `tar`로 만들고, 잠금 파일은 라이브러리의 `generate`로 만든다.
//! linux-x64 산출물만 실제 파일이고 나머지 플랫폼의 해시는 형식만 맞춘 값이다.
//! 실제 릴리스 파일을 받는 확인은 받기 스크립트 실행으로 따로 한다.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use tasty_doc_guards::official_plugins::{
    PLATFORMS, TSV_HEADER, artifact_file, executable_name, generate, render,
};
use tasty_doc_guards::sha256::sha256_hex;
use tasty_doc_guards::temp_scratch::Scratch;

const BIN: &str = env!("CARGO_BIN_EXE_official-plugins");

const PLUGINS: [(&str, &str, bool); 2] = [
    ("com.tasty.alpha", "0.2.0", true),
    ("com.tasty.beta", "0.3.0", false),
];

fn run(args: &[&str]) -> Output {
    Command::new(BIN).args(args).output().expect("run bin")
}

fn text(out: &Output) -> String {
    format!(
        "status={:?}\nstdout={}\nstderr={}",
        out.status.code(),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

fn meta() -> String {
    let mut m = String::from(
        "schema = 1\n\n[release]\nrepository = \"owner/plugins\"\ntag = \"v9.9.9\"\n\
         commit = \"ba234e6e4bc3a7789517ce38cc22a7341f8719e9\"\n\
         tasty_rev = \"e277fce41d9a2db8bf3c14fda8f56dd6415bfcbc\"\n\
         host_api_version = \"1\"\nprotocol_version = \"0.2.16\"\nmanifest_version = 1\n\
         epaint_version = \"0.31.1\"\n\
         epaint_patch_tree = \"37251c43b1d13d12fb6d4c81f3589ace67a0ac12\"\n",
    );
    for (id, version, bundle) in PLUGINS {
        m.push_str(&format!(
            "\n[[plugin]]\nid = \"{id}\"\nversion = \"{version}\"\nbundle = {bundle}\n"
        ));
    }
    m
}

/// `<work>/stage/<id>/`를 만들고 `<work>/dl/<file>`로 묶는다. `manifest_version`으로 매니페스트의
/// version을 바꿀 수 있다.
fn make_archive(work: &Path, id: &str, version: &str, manifest_version: &str) -> PathBuf {
    let stage = work.join("stage");
    let root = stage.join(id);
    std::fs::create_dir_all(root.join("lang")).expect("mkdir");
    std::fs::write(
        root.join("tasty-plugin.toml"),
        format!("manifest_version = 1\nid = \"{id}\"\nversion = \"{manifest_version}\"\n"),
    )
    .expect("write manifest");
    std::fs::write(root.join("lang/en.toml"), "x = \"y\"\n").expect("write lang");
    let exe = root.join(executable_name(id, "linux-x64").expect("official id"));
    std::fs::write(&exe, "#!/bin/sh\n").expect("write exe");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    }
    let dl = work.join("dl");
    std::fs::create_dir_all(&dl).expect("mkdir dl");
    let file = artifact_file(id, version, "linux-x64");
    // -f 에 상대 경로만 넘긴다. Windows의 GNU tar는 `C:` 를 원격 호스트로 읽는다.
    let status = Command::new("tar")
        .current_dir(work)
        .args(["-czf", &format!("dl/{file}"), "-C", "stage", id])
        .status()
        .expect("run tar");
    assert!(status.success(), "tar -czf {file}");
    std::fs::remove_dir_all(&root).expect("clean stage");
    dl.join(file)
}

/// 합성 릴리스 하나를 만들고 잠금 파일 경로와 받은 폴더를 돌려준다.
fn fixture(work: &Path) -> (PathBuf, PathBuf) {
    let mut real = std::collections::BTreeMap::new();
    for (id, version, _) in PLUGINS {
        let path = make_archive(work, id, version, version);
        let bytes = std::fs::read(&path).expect("read archive");
        real.insert(id, sha256_hex(&bytes));
    }
    let tsvs: Vec<(String, String)> = PLATFORMS
        .iter()
        .map(|p| {
            let mut t = format!("{TSV_HEADER}\n");
            for (id, version, _) in PLUGINS {
                let sha = if *p == "linux-x64" {
                    real[id].clone()
                } else {
                    "0".repeat(64)
                };
                t.push_str(&format!(
                    "{id}\t{version}\t{p}\t{}\t{sha}\n",
                    artifact_file(id, version, p)
                ));
            }
            (format!("plugins-{p}.tsv"), t)
        })
        .collect();
    let lock = generate(&meta(), &tsvs).expect("generate");
    let lock_path = work.join("official-plugins.lock.toml");
    std::fs::write(&lock_path, render(&lock)).expect("write lock");
    (lock_path, work.join("dl"))
}

fn s(p: &Path) -> &str {
    p.to_str().expect("utf-8 path")
}

#[test]
fn generate_writes_a_lock_that_check_accepts_and_rejects_a_mismatch() {
    let scratch = Scratch::new("official-plugins-generate");
    let work = scratch.path();
    let meta_path = work.join("release-meta.toml");
    std::fs::write(&meta_path, meta()).expect("write meta");
    let mut tsv_args = Vec::new();
    for p in PLATFORMS {
        let mut t = format!("{TSV_HEADER}\n");
        for (id, version, _) in PLUGINS {
            t.push_str(&format!(
                "{id}\t{version}\t{p}\t{}\t{}\n",
                artifact_file(id, version, p),
                "a".repeat(64)
            ));
        }
        let path = work.join(format!("plugins-{p}.tsv"));
        std::fs::write(&path, t).expect("write tsv");
        tsv_args.push(path);
    }
    let lock = work.join("lock.toml");
    let mut args = vec!["generate", "--meta", s(&meta_path), "--out", s(&lock)];
    for p in &tsv_args {
        args.extend(["--tsv", s(p)]);
    }
    let out = run(&args);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    let out = run(&["check", "--lock", s(&lock)]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));

    // TSV 하나의 version이 메타데이터와 다르면 쓰지 않고 실패한다.
    std::fs::remove_file(&lock).expect("rm lock");
    let first = &tsv_args[0];
    let t = std::fs::read_to_string(first).expect("read tsv");
    std::fs::write(first, t.replace("0.3.0", "0.3.1")).expect("write tsv");
    let out = run(&args);
    assert_eq!(out.status.code(), Some(1), "{}", text(&out));
    assert!(!lock.exists(), "실패한 generate 가 잠금 파일을 썼다");
}

#[test]
fn check_rejects_a_lock_without_one_platform() {
    let scratch = Scratch::new("official-plugins-check");
    let (lock, _) = fixture(scratch.path());
    let t = std::fs::read_to_string(&lock).expect("read lock");
    let start = t
        .find("  [[plugin.artifact]]\n  platform = \"macos-arm64\"")
        .expect("entry");
    let end = start + t[start..].find("sha256").expect("sha line");
    let end = end + t[end..].find('\n').expect("eol") + 1;
    std::fs::write(&lock, format!("{}{}", &t[..start], &t[end..])).expect("write lock");
    let out = run(&["check", "--lock", s(&lock)]);
    assert_eq!(out.status.code(), Some(1), "{}", text(&out));
    assert!(
        text(&out).contains("macos-arm64 산출물이 없다"),
        "{}",
        text(&out)
    );
}

#[test]
fn plan_lists_bundled_plugins_and_all_on_request() {
    let scratch = Scratch::new("official-plugins-plan");
    let (lock, _) = fixture(scratch.path());
    let out = run(&["plan", "--lock", s(&lock), "--platform", "linux-x64"]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    assert_eq!(stdout.lines().count(), 1, "{stdout}");
    assert!(stdout.starts_with("com.tasty.alpha\t0.2.0\tcom.tasty.alpha-0.2.0-linux-x64.tar.gz\thttps://github.com/owner/plugins/releases/download/v9.9.9/"), "{stdout}");
    let out = run(&[
        "plan",
        "--lock",
        s(&lock),
        "--platform",
        "linux-x64",
        "--all",
    ]);
    assert_eq!(String::from_utf8_lossy(&out.stdout).lines().count(), 2);
    let out = run(&["plan", "--lock", s(&lock), "--platform", "plan9-x64"]);
    assert_eq!(out.status.code(), Some(2), "{}", text(&out));
}

#[test]
fn verify_names_the_file_whose_bytes_changed() {
    let scratch = Scratch::new("official-plugins-verify");
    let (lock, dl) = fixture(scratch.path());
    let args = [
        "verify",
        "--lock",
        s(&lock),
        "--dir",
        s(&dl),
        "--platform",
        "linux-x64",
        "--all",
    ];
    let out = run(&args);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));

    let file = dl.join(artifact_file("com.tasty.beta", "0.3.0", "linux-x64"));
    let mut bytes = std::fs::read(&file).expect("read");
    let last = bytes.len() - 1;
    bytes[last] ^= 0x01;
    std::fs::write(&file, bytes).expect("write");
    let out = run(&args);
    assert_eq!(out.status.code(), Some(1), "{}", text(&out));
    assert!(
        text(&out).contains("sha256 mismatch: com.tasty.beta-0.3.0-linux-x64.tar.gz"),
        "{}",
        text(&out)
    );

    std::fs::remove_file(&file).expect("rm");
    let out = run(&args);
    assert_eq!(out.status.code(), Some(1), "{}", text(&out));
    assert!(
        text(&out).contains("missing: com.tasty.beta-0.3.0-linux-x64.tar.gz"),
        "{}",
        text(&out)
    );
}

#[test]
fn unpack_stages_each_plugin_and_rejects_a_manifest_that_disagrees() {
    let scratch = Scratch::new("official-plugins-unpack");
    let (lock, dl) = fixture(scratch.path());
    let out_dir = scratch.path().join("out");
    let args = [
        "unpack",
        "--lock",
        s(&lock),
        "--dir",
        s(&dl),
        "--out",
        s(&out_dir),
        "--platform",
        "linux-x64",
        "--all",
    ];
    let out = run(&args);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    for (id, _, _) in PLUGINS {
        let root = out_dir.join(id);
        assert!(root.join("tasty-plugin.toml").is_file(), "{id}");
        assert!(root.join("lang/en.toml").is_file(), "{id}");
        let exe = executable_name(id, "linux-x64").expect("official id");
        assert!(root.join(exe).is_file(), "{id}");
    }
    assert!(!out_dir.join(".official-plugins-unpack").exists());

    // 매니페스트 version이 잠금 파일과 다른 아카이브. 해시는 맞춰 구조 검사까지 가게 한다.
    let work = scratch.path().join("bad");
    let archive = make_archive(&work, "com.tasty.alpha", "0.2.0", "0.2.1");
    let bad_sha = sha256_hex(&std::fs::read(&archive).expect("read"));
    let t = std::fs::read_to_string(&lock).expect("read lock");
    let good = &t[t
        .find("com.tasty.alpha-0.2.0-linux-x64.tar.gz")
        .expect("file")..];
    let good_sha = good[good.find("sha256 = \"").expect("sha") + 10..][..64].to_string();
    std::fs::write(&lock, t.replace(&good_sha, &bad_sha)).expect("write lock");
    std::fs::copy(&archive, dl.join(archive.file_name().expect("name"))).expect("copy");
    let out = run(&args);
    assert_eq!(out.status.code(), Some(1), "{}", text(&out));
    assert!(
        text(&out).contains("tasty-plugin.toml version = Some(\"0.2.1\")"),
        "{}",
        text(&out)
    );
}

#[test]
fn unknown_subcommand_and_missing_lock_are_usage_errors() {
    let out = run(&["frobnicate"]);
    assert_eq!(out.status.code(), Some(2), "{}", text(&out));
    let out = run(&["check", "--lock", "no/such/official-plugins.lock.toml"]);
    assert_eq!(out.status.code(), Some(2), "{}", text(&out));
}
