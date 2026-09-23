//! `adr-renumber` bin 을 합성 레포(git 저장소)에 돌려, 옮기고 고친 결과를 파일로 확인한다.
//!
//! 판정 규칙의 단위 시험은 `src/adr_renumber.rs` 에 있다. 여기는 그 규칙이 **파일 · git ·
//! 인덱스 재생성** 과 이어졌을 때를 본다 — 교환이 연쇄로 되돌아가지 않는가, 역매핑이 원본을
//! 바이트 단위로 되살리는가, 삭제되는 ADR 을 부르는 자리가 남으면 아무것도 안 쓰는가.
//!
//! 이 파일은 가짜 ADR 을 담은 픽스처라 bin 의 `FIXTURES` 에 올라 있다.

use std::collections::BTreeMap;
use std::path::Path;
use std::process::Command;

use tasty_doc_guards::temp_scratch::Scratch;

const INDEX: &str = "# ADR\n\n## g\n\n머리말: 0001 → 0002 → 0003.\n\n\
<!-- adr-rows:begin g -->\n<!-- adr-rows:end g -->\n";

fn adr(num: &str, title: &str, body: &str) -> String {
    format!(
        "# ADR-{num}: {title}\n\n- **Status**: Accepted\n- **Date**: 2026-09-23\n\
         - **Tags**: t\n- **Group**: g\n\n{body}\n"
    )
}

fn git(root: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(root)
        .args([
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .output()
        .expect("git 을 못 돌렸다");
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).to_string()
}

/// 합성 레포 — ADR 셋, 그것들을 여러 형태로 부르는 문서 하나, 인덱스.
fn fixture(what: &str) -> Scratch {
    let s = Scratch::new(what);
    let root = s.path();
    let adr_dir = root.join("docs/adr");
    std::fs::create_dir_all(&adr_dir).expect("mkdir");
    let files = [
        (
            "0001-alpha.md",
            adr(
                "0001",
                "하나",
                "[ADR-0002](0002-beta.md) · ADR-0002/0003 · [0003](0003-gamma.md) · 맨 0002.",
            ),
        ),
        (
            "0002-beta.md",
            adr("0002", "둘", "`adr_0001_holds` 가 본다."),
        ),
        ("0003-gamma.md", adr("0003", "셋", "Tags 없이 끝.")),
    ];
    for (name, body) in &files {
        std::fs::write(adr_dir.join(name), body).expect("write");
    }
    std::fs::write(adr_dir.join("index.md"), INDEX).expect("write");
    std::fs::write(
        root.join("docs/other.md"),
        "[x](adr/0002-beta.md) · `docs/adr/0003-gamma.md` · [0002](adr/0002-beta.md) · \
         [h](adr/0001-alpha.md#adr-0001-하나) · 2026-09-23 · e\\u{0002}\n",
    )
    .expect("write");
    let out = Command::new(env!("CARGO_BIN_EXE_adr-index"))
        .arg("--write")
        .arg(root)
        .output()
        .expect("adr-index");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    git(root, &["init", "-q"]);
    git(root, &["add", "-A"]);
    git(root, &["commit", "-q", "-m", "base"]);
    s
}

fn run(root: &Path, map: &str, write: bool) -> (Option<i32>, String, String) {
    let map_path = root.join("../").join(format!(
        "{}.map",
        root.file_name().and_then(|n| n.to_str()).expect("name")
    ));
    std::fs::write(&map_path, map).expect("map");
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_adr-renumber"));
    cmd.arg(&map_path).arg("--root").arg(root);
    if write {
        cmd.arg("--write");
    }
    let out = cmd.output().expect("adr-renumber");
    let rm = std::fs::remove_file(&map_path);
    assert!(rm.is_ok(), "매핑 파일을 못 지웠다: {rm:?}");
    (
        out.status.code(),
        String::from_utf8_lossy(&out.stdout).to_string(),
        String::from_utf8_lossy(&out.stderr).to_string(),
    )
}

/// 작업 트리의 추적 파일 전부 — 경로 → 내용.
fn snapshot(root: &Path) -> BTreeMap<String, String> {
    git(root, &["ls-files"])
        .lines()
        .map(|p| {
            let body = std::fs::read_to_string(root.join(p)).unwrap_or_default();
            (p.to_string(), body)
        })
        .collect()
}

const SWAP: &str = "0001 0002\n0002 0001\n0003 0003\n";

#[test]
fn identity_mapping_changes_nothing() {
    let s = fixture("renumber-identity");
    let root = s.path();
    let (rc, report, err) = run(root, "0001 0001\n0002 0002\n0003 0003\n", true);
    assert_eq!(rc, Some(0), "{err}");
    assert!(err.contains("고칠 자리 0"), "{err}");
    // 모수가 0 이 아니었다는 것 — 형태 표가 본 자리를 센다.
    assert!(
        report.contains("| 파일명 NNNN-<slug> | 6 | 0 |"),
        "{report}"
    );
    assert_eq!(git(root, &["status", "--porcelain"]), "");
}

#[test]
fn swap_rewrites_every_form_at_once_and_the_inverse_restores_the_bytes() {
    let s = fixture("renumber-swap");
    let root = s.path();
    let before = snapshot(root);

    let (rc, _, err) = run(root, SWAP, true);
    assert_eq!(rc, Some(0), "{err}");
    let a = std::fs::read_to_string(root.join("docs/adr/0002-alpha.md")).expect("옮겨졌다");
    assert!(a.starts_with("# ADR-0002: 하나"), "{a}");
    assert!(
        a.contains("[ADR-0001](0001-beta.md) · ADR-0001/0003 · [0003](0003-gamma.md) · 맨 0002."),
        "교환이 한 번에 적용되고 맨 숫자는 그대로여야 한다:\n{a}"
    );
    let b = std::fs::read_to_string(root.join("docs/adr/0001-beta.md")).expect("옮겨졌다");
    assert!(b.contains("`adr_0002_holds`"), "{b}");
    let other = std::fs::read_to_string(root.join("docs/other.md")).expect("read");
    assert_eq!(
        other,
        "[x](adr/0001-beta.md) · `docs/adr/0003-gamma.md` · [0001](adr/0001-beta.md) · \
         [h](adr/0002-alpha.md#adr-0002-하나) · 2026-09-23 · e\\u{0002}\n"
    );
    let index = std::fs::read_to_string(root.join("docs/adr/index.md")).expect("read");
    assert!(index.contains("머리말: 0002 → 0001 → 0003."), "{index}");
    let check = Command::new(env!("CARGO_BIN_EXE_adr-index"))
        .arg("--check")
        .arg(root)
        .output()
        .expect("adr-index");
    assert!(
        check.status.success(),
        "재번호 뒤 인덱스가 생성 결과와 다르다: {}",
        String::from_utf8_lossy(&check.stderr)
    );

    git(root, &["add", "-A"]);
    git(root, &["commit", "-q", "-m", "swap"]);
    let (rc, _, err) = run(root, SWAP, true);
    assert_eq!(rc, Some(0), "{err}");
    assert_eq!(snapshot(root), before, "역매핑이 원본을 되살리지 못했다");
}

#[test]
fn a_cited_deletion_blocks_the_write() {
    let s = fixture("renumber-delete");
    let root = s.path();
    let map = "0001 0001\n0002 0002\n0003 DELETE\n";
    let (rc, report, err) = run(root, map, false);
    assert_eq!(rc, Some(1), "{err}");
    assert!(
        report.contains("docs/adr/0001-alpha.md:8: [파일명 NNNN-<slug> 0003→DELETE]"),
        "{report}"
    );
    assert!(
        report.contains("docs/other.md:1: [파일명 NNNN-<slug> 0003→DELETE]"),
        "{report}"
    );
    let (rc, _, err) = run(root, map, true);
    assert_eq!(rc, Some(1), "{err}");
    assert_eq!(
        git(root, &["status", "--porcelain"]),
        "",
        "막혔는데 무언가 썼다"
    );
}

#[test]
fn write_refuses_a_dirty_tree_and_an_incomplete_mapping() {
    let s = fixture("renumber-dirty");
    let root = s.path();
    let (rc, _, err) = run(root, "0001 0002\n0002 0001\n", false);
    assert_eq!(rc, Some(2), "빠진 ADR 을 받았다: {err}");
    assert!(err.contains("0003 이 매핑에 없다"), "{err}");
    std::fs::write(root.join("docs/stray.md"), "x").expect("write");
    let (rc, _, err) = run(root, SWAP, true);
    assert_eq!(rc, Some(2), "{err}");
    assert!(
        !root.join("docs/adr/0002-alpha.md").exists(),
        "더러운 트리에서 옮겼다"
    );
}
