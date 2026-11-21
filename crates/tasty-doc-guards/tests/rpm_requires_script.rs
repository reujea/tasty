//! `scripts/lib/rpm-requires.sh`의 판정을 합성 입력으로 확인한다.
//! - rpm Requires는 payload ELF의 직접 의존(RPM 4.18.2 elfdeps 규칙)과 정확히 같아야 한다.
//!   전이 의존이 더해지거나, 무버전·버전 요구·`rtld(GNU_HASH)`가 빠지거나 바뀌면 실패한다.
//! - 기대 집합 파싱은 가짜 `readelf`의 고정 출력으로 확인한다. 이 시험은 CI(`ubuntu-latest`)의
//!   rpm 버전에 기대지 않는다. 실제 rpm 4.18.2 find-requires와의 일치는 릴리스 러너의
//!   `scripts/build-linux.sh` 실행이 판정한다.
//! - 도구 사전 검사는 PATH를 임시 폴더 하나로 제한해 실제 rpm 도구가 보이지 않게 한다.
//! - `build-linux.sh`가 Requires 방식을 인자로 고정하는지는 소스에서 본다. 지금 두 러너는
//!   인자가 빠져도 같은 생성기를 골라 실제 빌드로는 누락이 드러나지 않는다.

#![cfg(target_os = "linux")]

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use tasty_doc_guards::temp_scratch::Scratch;

fn script() -> PathBuf {
    tasty_doc_guards::repo_root().join("scripts/lib/rpm-requires.sh")
}

/// 함수가 쓰는 실물 명령. 가짜로 바꿀 명령은 넣지 않는다.
const REAL_TOOLS: &[&str] = &[
    "awk", "sed", "grep", "sort", "find", "stat", "head", "od", "tr", "mktemp", "readlink", "rm",
    "cat", "dirname",
];

fn find_real(name: &str) -> PathBuf {
    for dir in ["/usr/bin", "/bin"] {
        let p = Path::new(dir).join(name);
        if p.exists() {
            return p;
        }
    }
    panic!("시험에 필요한 명령 {name}이 /usr/bin·/bin에 없다");
}

fn bash() -> PathBuf {
    find_real("bash")
}

fn write_exec(path: &Path, body: &str, mode: u32) {
    std::fs::write(path, body).expect("파일 쓰기");
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).expect("권한");
}

/// 실물 기본 명령 링크와 가짜 명령을 담은 PATH 폴더를 만든다.
fn tool_dir(root: &Path, fakes: &[(&str, &str)]) -> PathBuf {
    let dir = root.join("bin");
    std::fs::create_dir_all(&dir).expect("bin");
    for name in REAL_TOOLS {
        std::os::unix::fs::symlink(find_real(name), dir.join(name)).expect("symlink");
    }
    for (name, body) in fakes {
        write_exec(&dir.join(name), body, 0o755);
    }
    dir
}

/// 가짜 readelf: 마지막 인자 파일 옆의 `<파일>.h|.d|.v`를 옵션에 따라 출력한다. `<파일>.fail`이 있으면 실패한다.
const FAKE_READELF: &str = "#!/bin/sh\n\
for f; do :; done\n\
[ -e \"$f.fail\" ] && exit 1\n\
case \"$1\" in -hlW) cat \"$f.h\" ;; -dW) cat \"$f.d\" ;; -VW) cat \"$f.v\" ;; *) exit 2 ;; esac\n";

/// bash를 절대 경로로 시작하고 PATH를 `path` 하나로 제한해 rpm-requires.sh를 source한 뒤 `body`를 실행한다.
fn run(cwd: &Path, path: &Path, body: &str) -> (i32, String, String) {
    let out = Command::new(bash())
        .env_clear()
        .env("PATH", path)
        .env("LC_ALL", "C")
        .arg("-c")
        .arg(format!(". \"$1\"\n{body}"))
        .arg("rpm-requires-test")
        .arg(script())
        .current_dir(cwd)
        .output()
        .expect("bash 실행");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// 가짜 ELF 하나: magic만 맞는 파일과 readelf 고정 출력.
fn fake_elf(path: &Path, mode: u32, header: &str, dynamic: &str, versions: &str) {
    write_exec(path, "\x7fELF fixture", mode);
    let p = path.display();
    std::fs::write(format!("{p}.h"), header).expect("h");
    std::fs::write(format!("{p}.d"), dynamic).expect("d");
    std::fs::write(format!("{p}.v"), versions).expect("v");
}

const HEADER_PIE: &str = "ELF Header:\n  Class:                             ELF64\n  \
Type:                              DYN (Position-Independent Executable file)\n\
Program Headers:\n  Type           Offset   VirtAddr\n  INTERP         0x000318 0x0000000000000318\n";
const HEADER_SO: &str = "ELF Header:\n  Class:                             ELF64\n  \
Type:                              DYN (Shared object file)\n";
const DYNAMIC_MAIN: &str = "Dynamic section at offset 0x1 contains 6 entries:\n  Tag        Type                         Name/Value\n \
0x0000000000000001 (NEEDED)             Shared library: [libfoo.so.1]\n \
0x0000000000000001 (NEEDED)             Shared library: [ld-linux-aarch64.so.1]\n \
0x0000000000000001 (NEEDED)             Shared library: [ld6-test.so]\n \
0x0000000000000001 (NEEDED)             Shared library: [foo.so]\n \
0x0000000000000001 (NEEDED)             Shared library: [libnoso]\n \
0x000000006ffffef5 (GNU_HASH)           0x3a0\n";
const VERSIONS_MAIN: &str = "Version symbols section '.gnu.version' contains 3 entries:\n \
Addr: 0x0  Offset: 0x0  Link: 6 (.dynsym)\n  000:   0 (*local*)       2 (FOO_1)\n\n\
Version needs section '.gnu.version_r' contains 2 entries:\n \
Addr: 0x0  Offset: 0x0  Link: 7 (.dynstr)\n  \
000000: Version: 1  File: libfoo.so.1  Cnt: 2\n  \
0x0010:   Name: FOO_1  Flags: none  Version: 2\n  \
0x0020:   Name: FOO_2  Flags: none  Version: 3\n  \
0x0030: Version: 1  File: foo.so  Cnt: 1\n  \
0x0040:   Name: BAR_1  Flags: none  Version: 4\n";
const DYNAMIC_SO: &str = "Dynamic section at offset 0x1 contains 3 entries:\n \
0x0000000000000001 (NEEDED)             Shared library: [libbar.so.2]\n \
0x0000000000000004 (HASH)               0x1f0\n \
0x000000006ffffef5 (GNU_HASH)           0x3a0\n";
const VERSIONS_SO: &str = "Version definition section '.gnu.version_d' contains 1 entry:\n  \
000000: Rev: 1  Flags: BASE  Index: 1  Cnt: 1  Name: libself.so.1\n";

const EXPECTED_MAIN: &[&str] = &[
    "ld-linux-aarch64.so.1()(64bit)",
    "ld6-test.so()(64bit)",
    "libfoo.so.1()(64bit)",
    "libfoo.so.1(FOO_1)(64bit)",
    "libfoo.so.1(FOO_2)(64bit)",
    "rtld(GNU_HASH)",
];

fn lines(s: &str) -> Vec<&str> {
    s.lines().filter(|l| !l.is_empty()).collect()
}

#[test]
fn expected_requires_follow_the_elfdeps_rules() {
    let dir = Scratch::new("rpm-expected");
    let bin = tool_dir(dir.path(), &[("readelf", FAKE_READELF)]);
    let payload = dir.path().join("payload");
    std::fs::create_dir_all(payload.join("usr/lib")).expect("payload");
    fake_elf(
        &payload.join("main"),
        0o755,
        HEADER_PIE,
        DYNAMIC_MAIN,
        VERSIONS_MAIN,
    );
    let (rc, out, err) = run(dir.path(), &bin, "rpm_expected_requires_file payload/main");
    assert_eq!(rc, 0, "{err}");
    let mut got = lines(&out);
    got.sort();
    assert_eq!(
        got, EXPECTED_MAIN,
        "foo.so·libnoso는 걸러지고 ld6-test.so는 남는다"
    );

    // DT_HASH가 함께 있으면 rtld(GNU_HASH)를 내지 않고, verdef의 Name은 버전 요구가 아니다.
    fake_elf(
        &payload.join("usr/lib/libself.so.1"),
        0o755,
        HEADER_SO,
        DYNAMIC_SO,
        VERSIONS_SO,
    );
    let (rc, out, err) = run(
        dir.path(),
        &bin,
        "rpm_expected_requires_file payload/usr/lib/libself.so.1",
    );
    assert_eq!(rc, 0, "{err}");
    assert_eq!(lines(&out), ["libbar.so.2()(64bit)"]);

    // 디렉터리 전체는 두 파일의 합집합을 정렬해 낸다.
    std::fs::write(payload.join("data.txt"), "data\n").expect("data");
    let (rc, out, err) = run(dir.path(), &bin, "rpm_expected_requires payload");
    assert_eq!(rc, 0, "{err}");
    let mut want: Vec<&str> = EXPECTED_MAIN.to_vec();
    want.push("libbar.so.2()(64bit)");
    want.sort();
    assert_eq!(lines(&out), want);
}

#[test]
fn an_interpreted_elf_without_an_execute_bit_has_no_requires() {
    let dir = Scratch::new("rpm-noexec");
    let bin = tool_dir(dir.path(), &[("readelf", FAKE_READELF)]);
    fake_elf(
        &dir.path().join("main"),
        0o644,
        HEADER_PIE,
        DYNAMIC_MAIN,
        VERSIONS_MAIN,
    );
    let (rc, out, err) = run(dir.path(), &bin, "rpm_expected_requires_file main");
    assert_eq!(rc, 0, "{err}");
    assert_eq!(out, "");
    // payload 계약은 이런 ELF를 거부한다.
    std::fs::create_dir(dir.path().join("payload")).expect("payload");
    std::fs::rename(dir.path().join("main"), dir.path().join("payload/main")).expect("mv");
    for ext in ["h", "d", "v"] {
        std::fs::rename(
            dir.path().join(format!("main.{ext}")),
            dir.path().join(format!("payload/main.{ext}")),
        )
        .expect("mv");
    }
    let (rc, _, err) = run(dir.path(), &bin, "rpm_expected_requires payload");
    assert_eq!(rc, 1);
    assert!(
        err.contains("ELF without an execute bit in the payload: /main"),
        "{err}"
    );
}

#[test]
fn a_readelf_failure_names_the_file() {
    let dir = Scratch::new("rpm-readelf-fail");
    let bin = tool_dir(dir.path(), &[("readelf", FAKE_READELF)]);
    let payload = dir.path().join("payload");
    std::fs::create_dir(&payload).expect("payload");
    fake_elf(&payload.join("broken"), 0o755, HEADER_PIE, "", "");
    std::fs::write(payload.join("broken.fail"), "").expect("fail");
    let (rc, _, err) = run(dir.path(), &bin, "rpm_expected_requires payload");
    assert_eq!(rc, 1);
    assert!(
        err.contains("readelf -hl failed on payload/broken"),
        "{err}"
    );
}

#[test]
fn an_executable_script_or_an_elf_free_payload_is_rejected() {
    // 실행 비트만 있는 데이터 파일은 Requires를 만들지 않아 허용한다(공식 플러그인 고지 파일 사례).
    let dir = Scratch::new("rpm-contract");
    let bin = tool_dir(dir.path(), &[("readelf", FAKE_READELF)]);
    let payload = dir.path().join("payload");
    std::fs::create_dir(&payload).expect("payload");
    std::fs::write(payload.join("icon.png"), "png").expect("png");
    write_exec(&payload.join("notice.txt"), "text\n", 0o755);
    let (rc, _, err) = run(dir.path(), &bin, "rpm_expected_requires payload");
    assert_eq!(rc, 1);
    assert!(err.contains("no ELF files in the payload"), "{err}");

    fake_elf(
        &payload.join("main"),
        0o755,
        HEADER_PIE,
        DYNAMIC_MAIN,
        VERSIONS_MAIN,
    );
    write_exec(&payload.join("helper"), "#!/bin/sh\nexit 0\n", 0o755);
    let (rc, _, err) = run(dir.path(), &bin, "rpm_expected_requires payload");
    assert_eq!(rc, 1);
    assert!(
        err.contains("executable script in the payload: /helper"),
        "{err}"
    );
}

/// compare_rpm_requires 결과(rc, stdout). `actual`은 rpm -qpR 줄 목록이다.
fn compare(actual: &[&str]) -> (i32, String) {
    let dir = Scratch::new("rpm-compare");
    let bin = tool_dir(dir.path(), &[]);
    std::fs::write(dir.path().join("expected"), EXPECTED_MAIN.join("\n") + "\n").expect("e");
    std::fs::write(dir.path().join("manual"), "freetype\nfontconfig\n").expect("m");
    std::fs::write(dir.path().join("actual"), actual.join("\n") + "\n").expect("a");
    let (rc, out, err) = run(
        dir.path(),
        &bin,
        "compare_rpm_requires actual expected manual",
    );
    assert!(err.is_empty(), "{err}");
    (rc, out)
}

fn matching() -> Vec<&'static str> {
    let mut v = vec![
        "/bin/sh",
        "fontconfig",
        "freetype",
        "rpmlib(CompressedFileNames) <= 3.0.4-1",
        "rpmlib(PayloadIsZstd) <= 5.4.18-1",
    ];
    v.extend_from_slice(EXPECTED_MAIN);
    v
}

#[test]
fn matching_requires_pass() {
    assert_eq!(compare(&matching()), (0, String::new()));
}

#[test]
fn a_transitive_dependency_is_rejected() {
    let mut a = matching();
    a.push("libicuuc.so.74()(64bit)");
    assert_eq!(
        compare(&a),
        (1, "unexpected: libicuuc.so.74()(64bit)\n".into())
    );
}

#[test]
fn a_missing_or_changed_entry_is_rejected() {
    for gone in [
        "libfoo.so.1()(64bit)",
        "libfoo.so.1(FOO_2)(64bit)",
        "rtld(GNU_HASH)",
        "freetype",
        "/bin/sh",
    ] {
        let a: Vec<&str> = matching().into_iter().filter(|l| *l != gone).collect();
        assert_eq!(compare(&a), (1, format!("missing: {gone}\n")), "{gone}");
    }
    let a: Vec<&str> = matching()
        .into_iter()
        .map(|l| {
            if l == "libfoo.so.1(FOO_2)(64bit)" {
                "libfoo.so.1(FOO_9)(64bit)"
            } else {
                l
            }
        })
        .collect();
    assert_eq!(
        compare(&a),
        (
            1,
            "missing: libfoo.so.1(FOO_2)(64bit)\nunexpected: libfoo.so.1(FOO_9)(64bit)\n".into()
        )
    );
}

#[test]
fn an_unknown_entry_is_rejected() {
    let mut a = matching();
    a.push("/bin/bash");
    assert_eq!(compare(&a), (1, "unexpected: /bin/bash\n".into()));
}

#[test]
fn an_empty_requires_list_is_rejected() {
    let dir = Scratch::new("rpm-empty");
    let bin = tool_dir(dir.path(), &[]);
    std::fs::write(dir.path().join("reqs"), "").expect("reqs");
    let (rc, _, err) = run(
        dir.path(),
        &bin,
        "verify_rpm_requires_from reqs payload Cargo.toml /nonexistent",
    );
    assert_eq!(rc, 1);
    assert!(err.contains("the rpm has no requires at all"), "{err}");
}

#[test]
fn manual_requires_come_from_cargo_toml_and_accept_only_any_version() {
    let dir = Scratch::new("rpm-manual");
    let bin = tool_dir(dir.path(), &[]);
    let manifest = tasty_doc_guards::repo_root().join("Cargo.toml");
    let (rc, out, err) = run(
        dir.path(),
        &bin,
        &format!("rpm_manual_requires '{}'", manifest.display()),
    );
    assert_eq!(rc, 0, "{err}");
    assert_eq!(lines(&out), ["freetype", "fontconfig"]);

    let versioned =
        "[package.metadata.generate-rpm.requires]\nfreetype = \"*\"\nfontconfig = \">= 2.14\"\n";
    std::fs::write(dir.path().join("v.toml"), versioned).expect("v");
    let (rc, _, err) = run(dir.path(), &bin, "rpm_manual_requires v.toml");
    assert_eq!(rc, 1);
    assert!(err.contains("only name = \"*\""), "{err}");

    let empty = "[package.metadata.generate-rpm.requires]\n\n[other]\nx = \"*\"\n";
    std::fs::write(dir.path().join("e.toml"), empty).expect("e");
    let (rc, _, err) = run(dir.path(), &bin, "rpm_manual_requires e.toml");
    assert_eq!(rc, 1);
    assert!(
        err.contains("no [package.metadata.generate-rpm.requires] entries"),
        "{err}"
    );
}

const FAKE_TOOL: &str = "#!/bin/sh\nexit 0\n";

/// check_rpm_tools를 가짜 도구 PATH에서 돌린다. `drop`은 PATH에서 뺄 도구, `generator`는 find-requires 본문.
fn tools_check(what: &str, drop: Option<&str>, generator: Option<&str>) -> (i32, String) {
    let dir = Scratch::new(what);
    let fakes: Vec<(&str, &str)> = ["rpm", "rpm2cpio", "cpio", "readelf"]
        .into_iter()
        .filter(|t| Some(*t) != drop)
        .map(|t| (t, FAKE_TOOL))
        .collect();
    let bin = tool_dir(dir.path(), &fakes);
    let gen_path = dir.path().join("find-requires");
    if let Some(body) = generator {
        write_exec(&gen_path, body, 0o755);
    }
    let (rc, _, err) = run(
        dir.path(),
        &bin,
        &format!("check_rpm_tools '{}'", gen_path.display()),
    );
    (rc, err)
}

const GOOD_GENERATOR: &str = "#!/bin/sh\ncat >/dev/null\necho 'libc.so.6()(64bit)'\n";

#[test]
fn the_tool_check_passes_with_every_tool_and_a_working_generator() {
    let (rc, err) = tools_check("rpm-tools-ok", None, Some(GOOD_GENERATOR));
    assert_eq!((rc, err.as_str()), (0, ""));
}

#[test]
fn the_tool_check_names_each_missing_tool() {
    let (rc, err) = tools_check("rpm-tools-nogen", None, None);
    assert_eq!(rc, 1);
    assert!(
        err.contains("rpm packaging tools missing:") && err.contains("find-requires"),
        "{err}"
    );
    for tool in ["rpm", "rpm2cpio", "cpio", "readelf"] {
        let (rc, err) = tools_check("rpm-tools-drop", Some(tool), Some(GOOD_GENERATOR));
        assert_eq!(rc, 1, "{tool}");
        assert!(
            err.contains(&format!("rpm packaging tools missing: {tool}")),
            "{tool}: {err}"
        );
    }
}

#[test]
fn the_tool_check_rejects_a_broken_generator() {
    let failing = "#!/bin/sh\ncat >/dev/null\necho 'libc.so.6()(64bit)'\nexit 3\n";
    let (rc, err) = tools_check("rpm-tools-fail", None, Some(failing));
    assert_eq!(rc, 1);
    assert!(err.contains("find-requires failed on"), "{err}");
    let empty = "#!/bin/sh\ncat >/dev/null\n";
    let (rc, err) = tools_check("rpm-tools-empty", None, Some(empty));
    assert_eq!(rc, 1);
    assert!(err.contains("printed no libc.so.6()(64bit)"), "{err}");
}

#[test]
fn a_real_elf_yields_libc_requires() {
    let dir = Scratch::new("rpm-real-elf");
    let exe = std::env::current_exe().expect("current_exe");
    let path = std::env::var_os("PATH").expect("PATH");
    let out = Command::new(bash())
        .env("LC_ALL", "C")
        .env("PATH", path)
        .arg("-c")
        .arg(". \"$1\"\nrpm_expected_requires_file \"$2\"")
        .arg("rpm-requires-test")
        .arg(script())
        .arg(&exe)
        .current_dir(dir.path())
        .output()
        .expect("bash 실행");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "readelf(binutils)가 필요하다: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        stdout.lines().any(|l| l == "libc.so.6()(64bit)"),
        "{stdout}"
    );
    assert!(
        stdout.lines().any(|l| l.starts_with("libc.so.6(GLIBC_")),
        "{stdout}"
    );
}

#[test]
fn build_linux_pins_the_requires_generator_before_building() {
    let src = std::fs::read_to_string(tasty_doc_guards::repo_root().join("scripts/build-linux.sh"))
        .expect("build-linux.sh");
    assert!(src.contains("RPM_FIND_REQUIRES=/usr/lib/rpm/find-requires\n"));
    assert!(
        src.contains("cargo generate-rpm --profile \"$PROFILE\" --auto-req find-requires\n"),
        "rpm Requires 방식은 저장소가 정한다"
    );
    let check = "    check_rpm_tools \"$RPM_FIND_REQUIRES\" || exit 1\n";
    let guarded = format!("if [[ \"$PROFILE\" != \"debug\" ]]; then\n{check}fi\n");
    assert!(
        src.contains(&guarded),
        "--debug는 rpm을 만들지 않아 검사를 건너뛴다"
    );
    let check_at = src.find(check).expect("check_rpm_tools 호출");
    let build_at = src.find("cargo build $CARGO_FLAGS").expect("cargo build");
    assert!(check_at < build_at, "도구 검사는 본체 빌드 전에 돈다");
    assert!(src.contains("verify_rpm_requires \"$RPM_FILE\""));
    assert!(
        !src.contains("command -v rpm &>/dev/null"),
        "rpm 검사는 도구 유무로 건너뛰지 않는다"
    );
}
