//! 근거 없는 억제 래칫(`scripts/check-allow-reason.sh`)의 **좌변**을 합성 트리로
//! 고정한다. 자매 픽스처 `tests/shared_walk_gate.rs` 와 같은 판정 방식이고, 다른 것은
//! 소비처 수(다섯)와 그중 둘이 **마스킹 사본 둘**이라는 점이다.
//!
//! 왜 좌변인가. 이 게이트는 같은 트리를 **다섯 번** 지목했다 — rg 분기 · git 분기 ·
//! 미추적 검사 · 마스킹 det · 마스킹 txt. 표기까지 갈렸다(git 은 pathspec, rg 와
//! 마스커는 디렉토리). 하나를 손대면 나머지가 안 따라가고 그 어긋남은 조용했다.
//!
//! **판정 방법: 좌변 값을 늘려 놓고 소비처가 따라오는지 종료 코드로 묻는다.**
//! 문자열 확인("게이트에 `SCAN_SPECS` 가 나온다")은 철자를 보는 것이지 다섯이 같은
//! 값을 낸다는 것을 보는 게 아니다 — 변수를 선언해 놓고 소비처 하나가 옛 문자열을
//! 그대로 쓰는 상태(= 고치기 전의 모양)를 통과시킨다.
//!
//! 래칫이라 좌변이 줄면 미달로, 늘면 초과로 **양쪽 다 rc=1** 이다. 그래서 상한을
//! 프로브에 맞춰 놓고 **초록을 기대한다** — 다섯이 다 따라오면 값이 상한과 맞는다.
//!
//! ★ **마스킹 둘은 값이 아니라 갈림으로만 관측된다.** 게이트는 사본에 없는 파일을
//! 건너뛰지 않고 **그 파일만 원문에서 센다**(모수가 조용히 줄지 않게 하려는 것이다).
//! 그래서 마스킹 뿌리가 안 따라와도 그냥 값이 같아진다 — 프로브가 원문과 사본에서
//! 다르게 세어지는 형태여야만 그 소비처가 보인다. 그 형태를 게이트 본문이 이미 두
//! 자리로 적어 두었다: 문자열 안의 억제(det 이 덮는다)와 문자열 안의 근거 마커(txt 가
//! 덮는다). 프로브는 그 둘을 그대로 쓴다.

#![cfg(unix)]

use std::fs;
use std::path::Path;
use std::process::Command;

fn gate_src() -> String {
    format!(
        "{}/scripts/check-allow-reason.sh",
        env!("CARGO_MANIFEST_DIR")
    )
}

/// 좌변이 안 빈 합성 git 레포.
///
/// `git init` 이 필수다 — git 분기의 좌변이 `git ls-files` 라, 레포가 아니면 빈 좌변으로
/// 떨어져 이 파일의 모든 시험이 **재려던 것과 다른 갈래**(판정 불가)를 재게 된다.
fn synth_root() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("임시 디렉토리");
    let root = dir.path();
    let here = env!("CARGO_MANIFEST_DIR");
    fs::create_dir_all(root.join("scripts/lib")).expect("scripts/lib");
    fs::copy(gate_src(), root.join("scripts/check-allow-reason.sh")).expect("게이트 복사");
    fs::copy(
        format!("{here}/scripts/lib/judge-bin.sh"),
        root.join("scripts/lib/judge-bin.sh"),
    )
    .expect("판정기 찾기 공용 복사");
    // 좌변이 보는 두 뿌리를 하나씩 실재시킨다. 억제는 없다 — 프로브가 넣는다.
    write_file(root, "src/zz_quiet.rs", "fn f() {}\n");
    write_file(root, "crates/zz/src/zz_quiet.rs", "fn g() {}\n");
    git(root, &["init", "-q"]);
    git(root, &["add", "-A"]);
    dir
}

fn git(root: &Path, args: &[&str]) {
    let out = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .expect("git 실행");
    assert!(
        out.status.success(),
        "git {args:?} 실패: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn write_file(root: &Path, rel: &str, body: &str) {
    let f = root.join(rel);
    fs::create_dir_all(f.parent().expect("부모")).expect("디렉토리");
    fs::write(&f, body).expect("파일 쓰기");
}

/// 줄 수를 보존한 채 문자열을(그리고 `--keep-comments` 가 아니면 주석도) 덮는 스텁.
///
/// 진짜 `mask-source` 를 안 쓰는 이유는 자매 픽스처와 같다 — cargo 산출물이라 여기서
/// 지으면 바깥 `cargo test` 와 빌드 디렉토리 잠금을 두고 서로를 기다린다.
///
/// **베끼기만 하면 안 된다.** 이 파일의 프로브 둘이 원문과 사본을 일부러 가른다.
fn install_stub_masker(root: &Path) -> std::path::PathBuf {
    let bin = root.join("stub-mask-source");
    fs::write(
        &bin,
        "#!/bin/sh\n\
         if [ \"$1\" = \"--check-fresh\" ]; then exit 0; fi\n\
         keep=0\n\
         if [ \"$1\" = \"--keep-comments\" ]; then keep=1; shift; fi\n\
         out=$1; shift; src=$1; shift\n\
         mkdir -p \"$out\" || exit 1\n\
         for d in \"$@\"; do\n\
         [ -d \"$src/$d\" ] || continue\n\
         cp -r \"$src/$d\" \"$out/$d\" || exit 1\n\
         done\n\
         find \"$out\" -name '*.rs' -type f | while IFS= read -r f; do\n\
         if [ \"$keep\" = 1 ]; then sed -i 's/\"[^\"]*\"/\"\"/g' \"$f\";\n\
         else sed -i 's/\"[^\"]*\"/\"\"/g; s|//.*||' \"$f\"; fi\n\
         done\n\
         exit 0\n",
    )
    .expect("스텁 판정기");
    let mut perm = fs::metadata(&bin).expect("스텁 권한 읽기").permissions();
    std::os::unix::fs::PermissionsExt::set_mode(&mut perm, 0o755);
    fs::set_permissions(&bin, perm).expect("스텁 실행권한");
    bin
}

/// 합성 루트에 **스텁 `rg`** 를 깔고 그 디렉토리를 반환한다.
///
/// 이 게이트의 좌변은 분기가 둘이고 어느 쪽을 타는지는 `command -v rg` 가 정한다.
/// 실측(2026-09-08, 이 호스트): bash 의 `PATH` 에 `rg` 바이너리가 없어 **항상 git
/// 분기**를 탄다. 그 말은 rg 분기의 좌변이 여기서 한 번도 안 돈다는 뜻이라, 그것을
/// 재려면 스텁을 깔아 분기를 강제해야 한다. 반대 방향(git 분기 강제)은 `PATH` 를
/// 좁히는 것으로 안 된다 — `git` 자신이 필요하다. 그래서 rg 만 얹는다.
fn install_stub_rg(root: &Path) -> std::path::PathBuf {
    let dir = root.join("stubbin");
    fs::create_dir_all(&dir).expect("스텁 디렉토리");
    let bin = dir.join("rg");
    fs::write(
        &bin,
        "#!/bin/sh\n\
         # `rg --files -g '*.rs' <dirs...>` 만 흉내낸다 — 게이트가 그 형태로만 부른다.\n\
         shift 3\n\
         for d in \"$@\"; do find \"$d\" -name '*.rs' -type f 2>/dev/null; done\n",
    )
    .expect("스텁 rg");
    let mut perm = fs::metadata(&bin).expect("스텁 권한 읽기").permissions();
    std::os::unix::fs::PermissionsExt::set_mode(&mut perm, 0o755);
    fs::set_permissions(&bin, perm).expect("스텁 실행권한");
    dir
}

fn run_inner(root: &Path, with_rg: bool) -> (i32, String) {
    let stub = install_stub_masker(root);
    let mut cmd = Command::new("bash");
    cmd.arg(root.join("scripts/check-allow-reason.sh"))
        .current_dir(root)
        .env("TASTY_MASK_SOURCE_BIN", &stub);
    if with_rg {
        let dir = install_stub_rg(root);
        let path = std::env::var("PATH").unwrap_or_default();
        cmd.env("PATH", format!("{}:{path}", dir.display()));
    }
    let out = cmd.output().expect("게이트 실행");
    (
        out.status.code().unwrap_or(-1),
        format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        ),
    )
}

fn run(root: &Path) -> (i32, String) {
    run_inner(root, false)
}

/// 게이트 사본의 좌변을 `extra/*.rs` 만큼 늘리고 상한을 프로브에 맞춘다.
///
/// 치환 실패는 그 자리에서 죽는다 — 좌변이 다시 여럿으로 흩어졌다는 뜻이라 그것도
/// 잡아야 할 회귀다.
fn widen_and_cap(root: &Path, cap: usize) {
    set_cap(root, cap);
    widen_only(root);
}

/// 상한만 프로브에 맞춘다. 좌변은 안 건드린다 — 늘리기 **전후**를 견주는 시험이
/// 앞쪽에서도 초록이어야 하기 때문이다.
fn set_cap(root: &Path, cap: usize) {
    let p = root.join("scripts/check-allow-reason.sh");
    let text = fs::read_to_string(&p).expect("게이트 사본을 읽을 수 없다");
    let capped = text.replace("CAP=173", &format!("CAP={cap}"));
    assert_ne!(
        capped, text,
        "게이트에 `CAP=173` 한 줄이 없다 — 상한 표기가 바뀌었다. 이 시험은 상한을 \
         프로브에 맞춰 놓고 초록을 기대하므로 여기서 멈춘다."
    );
    fs::write(&p, capped).expect("게이트 사본 쓰기");
}

/// 좌변만 `extra/*.rs` 만큼 늘린다.
fn widen_only(root: &Path) {
    let p = root.join("scripts/check-allow-reason.sh");
    let text = fs::read_to_string(&p).expect("게이트 사본을 읽을 수 없다");
    let widened = text.replace(
        "SCAN_SPECS=('crates/*.rs' 'src/*.rs')",
        "SCAN_SPECS=('crates/*.rs' 'src/*.rs' 'extra/*.rs')",
    );
    assert_ne!(
        widened, text,
        "게이트에 `SCAN_SPECS=(…)` 한 줄이 없다 — 좌변이 다시 여럿으로 흩어졌거나 \
         이름이 바뀌었다. 이 시험은 그 한 값을 늘려 소비처를 재므로 여기서 멈춘다."
    );
    fs::write(&p, widened).expect("게이트 사본 쓰기");
}

/// 근거 없는 억제 하나. 좌변이 이 파일을 보면 값이 1 이 된다.
const BARE: &str = "#[allow(dead_code)]\nfn probe() {}\n";

/// 좌변을 늘리면 **git 분기**가 따라온다.
///
/// 죽이는 변이: git pathspec 을 옛 문자열로 되돌리는 것. 그러면 `extra/` 를 안 봐 값이
/// 상한보다 작고, 래칫의 미달 분기로 빨개진다.
#[test]
fn widening_the_left_side_moves_the_git_branch() {
    let d = synth_root();
    write_file(d.path(), "extra/zz_probe.rs", BARE);
    git(d.path(), &["add", "-A"]);
    widen_and_cap(d.path(), 1);
    let (code, text) = run(d.path());
    assert_eq!(
        code, 0,
        "좌변을 늘렸는데 git 분기가 안 따라왔다 — 새 영토의 억제가 안 세어진다:\n{text}"
    );
    assert!(
        text.contains("좌변=git-ls-files"),
        "이 시험이 재려던 분기가 아니다:\n{text}"
    );
}

/// 좌변을 늘리면 **rg 분기**도 따라온다.
///
/// 이 분기는 이 호스트에서 한 번도 안 돈다(bash 의 `PATH` 에 `rg` 가 없다). 그래서
/// 스텁으로 강제하지 않으면 그 좌변은 **미측정**이고, 미측정인 채로 갈려 있으면 rg 가
/// 있는 러너에서만 조용히 다른 것을 센다.
#[test]
fn widening_the_left_side_moves_the_rg_branch() {
    let d = synth_root();
    write_file(d.path(), "extra/zz_probe.rs", BARE);
    git(d.path(), &["add", "-A"]);
    widen_and_cap(d.path(), 1);
    let (code, text) = run_inner(d.path(), true);
    assert!(
        text.contains("좌변=rg"),
        "스텁 rg 를 깔았는데 rg 분기를 안 탔다 — 이 시험이 재려던 것이 아니다:\n{text}"
    );
    assert_eq!(
        code, 0,
        "좌변을 늘렸는데 rg 분기가 안 따라왔다 — 새 영토의 억제가 안 세어진다:\n{text}"
    );
}

/// 좌변을 늘리면 **억제를 찾는 사본**(det)도 따라온다.
///
/// 억제 형태를 **문자열 안**에 둔다. det 사본을 뜨면 덮여서 0 이고, 안 뜨면 게이트가
/// 그 파일만 원문에서 세어 1 이 된다. 상한 0 이면 그 갈림이 그대로 rc 가 된다.
///
/// 이 형태는 픽스처가 아니라 실물이다 — 게이트 본문이 184 → 183 을 "문자열 안의
/// 억제(생성 코드를 조립하는 자리)" 로 적어 둔다.
#[test]
fn widening_the_left_side_moves_the_detection_copy() {
    let d = synth_root();
    write_file(
        d.path(),
        "extra/zz_probe.rs",
        "fn probe() -> &'static str {\n    \"#[allow(dead_code)]\"\n}\n",
    );
    git(d.path(), &["add", "-A"]);
    widen_and_cap(d.path(), 0);
    let (code, text) = run(d.path());
    assert_eq!(
        code, 0,
        "좌변을 늘렸는데 억제 탐지 사본이 안 따라왔다 — 새 영토를 원문으로 세어 \
         문자열 안의 억제 형태가 실물로 잡힌다:\n{text}"
    );
}

/// 좌변을 늘리면 **근거를 찾는 사본**(txt)도 따라온다.
///
/// 근거 마커를 **문자열 안**에 둔다(조건부 억제의 feature 이름). txt 사본은 주석은
/// 남기고 문자열만 덮으므로 마커가 사라져 근거 없음 = 1 이고, 사본을 안 뜨면 원문에
/// 마커가 살아 있어 근거 있음 = 0 이다. 상한 1 이면 그 갈림이 그대로 rc 가 된다.
///
/// det 쪽은 이 프로브에서 갈리지 않는다 — 문자열을 덮어도 `cfg_attr(` 과 `allow(` 는
/// 남아 억제로 탐지된다. 그래서 이 시험은 txt 만 잰다.
#[test]
fn widening_the_left_side_moves_the_reason_copy() {
    let d = synth_root();
    write_file(
        d.path(),
        "extra/zz_probe.rs",
        "#[cfg_attr(feature = \"이유: 문자열 안의 가짜 근거\", allow(dead_code))]\nfn probe() {}\n",
    );
    git(d.path(), &["add", "-A"]);
    widen_and_cap(d.path(), 1);
    let (code, text) = run(d.path());
    assert_eq!(
        code, 0,
        "좌변을 늘렸는데 근거 탐색 사본이 안 따라왔다 — 새 영토를 원문으로 읽어 \
         문자열 안의 마커가 근거로 인정된다:\n{text}"
    );
}

/// 좌변을 늘리면 **미추적 검사**도 따라온다.
///
/// 죽이는 변이: 미추적 pathspec 을 옛 문자열로 되돌리는 것. 그러면 `extra/` 의 미추적
/// 파일이 안 보여 판정 불가(2)가 아니라 값을 낸다 — 그 값은 그 파일의 억제가 빠진
/// 수이고, 상한보다 작으면 "줄었다" 로 읽혀 상한이 내려간다.
#[test]
fn widening_the_left_side_moves_the_untracked_check() {
    let d = synth_root();
    // 일부러 `git add` 하지 않는다 — 이것이 재려는 창이다.
    write_file(d.path(), "extra/zz_probe.rs", BARE);
    widen_and_cap(d.path(), 0);
    let (code, text) = run(d.path());
    assert_eq!(
        code, 2,
        "좌변을 늘렸는데 미추적 검사가 안 따라왔다 — 인덱스에 없는 파일이 있는데 값을 \
         낸다:\n{text}"
    );
    assert!(
        text.contains("extra/zz_probe.rs"),
        "무엇이 인덱스에 없는지를 안 찍는다:\n{text}"
    );
}

/// ★ 출력에만 실리는 값 축 — **훑은 수는 rc 에 안 들어간다.**
///
/// 위 다섯은 전부 `count`(근거 없는 억제)로 rc 를 가른다. 그런데 좌변이 갈리는 흔한
/// 모양은 그게 아니다: 새 영토에 억제가 아예 없으면 `count` 는 그대로고 rc 도 그대로다.
/// 바뀌는 것은 `훑은 .rs N개` 한 수뿐인데 그 수는 판정에 안 들어간다 — 좌변이
/// **완전히** 비어야 판정 불가로 갈리고, 반쯤 줄어든 좌변은 여기로 온다.
///
/// 실측(2026-09-08): 그 수를 상수 `0` 으로 바꾸는 변이에서 위 다섯이 전부 통과했다.
/// 게이트 본문은 그 수를 "좌변이 반쯤 줄어도 이 수 없이는 화면에 아무 신호가 없다" 는
/// 이유로 찍는데, 정작 그 수가 좌변을 따라가는지는 아무도 안 봤다.
///
/// 절대값을 외우지 않는다 — 늘리기 전후의 **차**를 본다. 늘린 것이 파일 하나라 1 이다.
#[test]
fn widening_the_left_side_moves_the_scanned_count() {
    fn scanned(text: &str) -> usize {
        let tail = text
            .split("훑은 .rs ")
            .nth(1)
            .unwrap_or_else(|| panic!("초록 문구에서 훑은 수를 못 읽었다:\n{text}"));
        tail.split('개')
            .next()
            .and_then(|n| n.trim().parse().ok())
            .unwrap_or_else(|| panic!("훑은 수가 숫자가 아니다:\n{text}"))
    }

    let d = synth_root();
    // 억제가 없는 파일이다 — `count` 를 안 움직이는 것이 이 시험의 요점이다.
    write_file(d.path(), "extra/zz_quiet.rs", "fn probe() {}\n");
    git(d.path(), &["add", "-A"]);

    set_cap(d.path(), 0);
    let (code, before) = run(d.path());
    assert_eq!(code, 0, "늘리기 전인데 초록이 아니다:\n{before}");

    widen_only(d.path());
    let (code, after) = run(d.path());
    assert_eq!(code, 0, "좌변을 늘렸더니 초록이 아니다:\n{after}");

    assert_eq!(
        scanned(&after),
        scanned(&before) + 1,
        "좌변을 늘렸는데 훑은 수가 안 늘었다 — 새 영토에 억제가 없는 동안 이 수가 그 \
         좌변을 관측하는 유일한 자리다.\n늘리기 전:\n{before}\n늘린 뒤:\n{after}"
    );
}

// ── 판정 자체 — 래칫이 **양쪽으로 서는가** ──────────────────────────────
//
// 위 여섯은 전부 **좌변**을 잰다: 상한을 프로브에 맞춰 놓고 초록을 기대하는 형태라,
// 판정 갈래가 통째로 무너져도(두 `exit 1` 이 `exit 0` 이 되어도) 여섯 다 초록이다.
//
// 실측 2026-09-08: 이 게이트의 거절 다섯 자리에 완화 변이를 하나씩 넣으니 **둘만
// 죽었다**(미추적 검사·판정기 부재). 살아남은 셋이 좌변이 통째로 빈 갈래와 **래칫의
// 판정 둘**이다 — 이 게이트가 존재하는 이유인 그 판정에 아무 시험도 없었다. 형제
// `check-shared-walk-ratchet.sh` 도 같은 모양이었다(5 중 2).
//
// **상한 값을 재는 것이 아니다.** 픽스처가 게이트의 상수로 트리를 지으면 그 상수에
// 대해서는 항등식이 된다. 여기서 재는 것은 판정 **기제**(넘으면 1 · 모자라면 1 ·
// 같으면 0)이고, 실제 상한이 옳은지는 게이트가 세는 실물이 답한다.

/// 근거 없는 억제를 `n` 개 담은 프로브.
fn bare_probe(root: &Path, n: usize) {
    let body: String = (0..n)
        .map(|i| format!("#[allow(dead_code)]\nfn probe{i}() {{}}\n"))
        .collect();
    write_file(root, "src/zz_probe.rs", &body);
    git(root, &["add", "-A"]);
}

/// 값이 상한과 같으면 통과(0)다. **대조군.**
///
/// 양성 대조(2026-09-08, 내 트리): 게이트의 상한 비교를 `-gt` → `-ge` 로 바꾸면(같아도
/// 위반) 이 시험이 rc=101 로 죽는다. **다만 배타적이지 않다** — 같은 변이가
/// `widening_the_left_side_moves_*` 다섯도 함께 죽인다(이 타깃 6 failed). 그 다섯은
/// 좌변을 넓히기 **전** 팔에서 상한을 프로브에 맞춰 놓고 rc=0 을 기대하므로, 같음 갈래를
/// 각자 한 번씩 더 밟는다.
///
/// 그래서 이 대조군이 더하는 것은 **검출이 아니라 이름**이다. 그 갈래가 깨졌을 때
/// 다섯 개의 "좌변이 안 움직인다" 대신 "값이 상한과 같은데 초록이 아니다" 가 함께 뜬다 —
/// 무엇이 깨졌는지를 가리키는 것은 뒤쪽뿐이다. 숨기지 않고 적어 둔다.
#[test]
fn a_count_at_the_cap_passes() {
    let d = synth_root();
    bare_probe(d.path(), 2);
    set_cap(d.path(), 2);
    let (code, text) = run(d.path());
    assert_eq!(code, 0, "값이 상한과 같은데 초록이 아니다:\n{text}");
}

/// 상한을 넘으면 위반(1)이고, 실패문이 **상한을 올리지 말라**고 말한다.
#[test]
fn a_count_over_the_cap_is_a_violation() {
    let d = synth_root();
    bare_probe(d.path(), 2);
    set_cap(d.path(), 1);
    let (code, text) = run(d.path());
    assert_eq!(code, 1, "상한을 넘었는데 위반이 아니다:\n{text}");
    assert!(
        text.contains("상한을 올려서 통과시키지 마라"),
        "상한을 올리지 말라는 처방이 없다 — 이 자리의 눈에 보이는 레버가 상한과 \
         면제 주석 둘이라, 처방을 안 적으면 둘 중 하나가 만져진다:\n{text}"
    );
}

/// 상한 아래로 내려가도 위반(1)이다.
#[test]
fn a_count_under_the_cap_is_also_a_violation() {
    let d = synth_root();
    bare_probe(d.path(), 2);
    set_cap(d.path(), 3);
    let (code, text) = run(d.path());
    assert_eq!(
        code, 1,
        "값이 상한 아래인데 초록이다 — 남는 여유가 곧 안 보는 구간이다:\n{text}"
    );
    assert!(
        text.contains("CAP 을"),
        "상한을 내리라는 처방이 없다:\n{text}"
    );
}

/// 근거 주석이 붙은 억제를 `n` 개 담은 프로브. `head` 가 억제 바로 위 주석 줄이다.
fn reasoned_probe(root: &Path, head: &str, n: usize) {
    let body: String = (0..n)
        .map(|i| format!("{head}\n#[allow(dead_code)]\nfn probe{i}() {{}}\n"))
        .collect();
    write_file(root, "src/zz_probe.rs", &body);
    git(root, &["add", "-A"]);
}

/// **마커만 있고 뒤가 비면 근거가 아니다.**
///
/// 이것이 이 게이트의 오래된 구멍이었다 — 판정이 `줄 ~ 마커정규식` 이라 `// 이유:` 한
/// 줄이면 그 억제는 영영 안 보였다. 옳은 수선의 **형태만** 흉내 내면 통과하는 자리다.
/// 프로브 둘을 상한 둘에 맞춰 두었으므로, 근거로 인정되면 값이 0 이 되어 **상한 미달**
/// 로도 죽는다 — 그래서 이 시험은 rc 만이 아니라 값을 문장에서 읽는다.
#[test]
fn a_bare_marker_with_nothing_after_it_is_not_a_reason() {
    let d = synth_root();
    reasoned_probe(d.path(), "// 이유:", 2);
    set_cap(d.path(), 2);
    let (code, text) = run(d.path());
    assert_eq!(
        code, 0,
        "마커만 있는 자리를 근거로 세고 있다 — 값이 상한 2 에 안 닿았다:\n{text}"
    );
    assert!(
        text.contains(": 2건"),
        "빈 마커 둘이 근거 없는 억제로 안 세어졌다:\n{text}"
    );
}

/// **음성 대조 — 마커 뒤에 내용이 있으면 근거다.** 위 시험에서 한 구절만 더한다.
///
/// 이 짝이 없으면 위 시험은 "마커를 아예 안 읽는다" 여도 초록이다.
#[test]
fn a_marker_followed_by_text_is_a_reason() {
    let d = synth_root();
    reasoned_probe(d.path(), "// 이유: 생성 코드라 이름이 안 쓰인다.", 2);
    set_cap(d.path(), 0);
    let (code, text) = run(d.path());
    assert_eq!(code, 0, "근거가 붙었는데 위반으로 세고 있다:\n{text}");
    assert!(text.contains(": 0건"), "값이 0 이 아니다:\n{text}");
}

/// **근거가 다음 줄부터 이어져도 근거다.** 마커 줄에서만 뒤를 보면 이 형태가 오탐이 된다.
#[test]
fn a_marker_whose_text_starts_on_the_next_line_is_a_reason() {
    let d = synth_root();
    reasoned_probe(d.path(), "// 이유:\n//   생성 코드라 이름이 안 쓰인다.", 2);
    set_cap(d.path(), 0);
    let (code, text) = run(d.path());
    assert_eq!(
        code, 0,
        "마커 다음 줄에 있는 근거를 못 읽는다 — 이 형태를 오탐으로 만들면 그 자리에 \
         쓸데없는 한 줄이 강요된다:\n{text}"
    );
    assert!(text.contains(": 0건"), "값이 0 이 아니다:\n{text}");
}

/// **`SAFETY` 는 콜론을 요구한다.**
///
/// 콜론이 없으면 그것은 키가 아니라 **관례어**라, 그 단어를 언급한 줄과 못 갈린다 —
/// 이 게이트가 무엇을 세는지 설명하는 주석에도 그 단어가 있다. 실측(`12bc0f4b2`):
/// 레포의 콜론 없는 `SAFETY` 셋은 전부 그 블록에 `SAFETY:` 나 `이유:` 를 이미 갖고
/// 있어 이 강화로 **새로 걸리는 자리가 0** 이었다.
#[test]
fn a_safety_without_a_colon_is_not_a_reason() {
    let d = synth_root();
    reasoned_probe(
        d.path(),
        "// 이 블록의 SAFETY 조건은 다른 곳에 적혀 있다",
        2,
    );
    set_cap(d.path(), 2);
    let (code, text) = run(d.path());
    assert_eq!(code, 0, "콜론 없는 SAFETY 언급을 근거로 세고 있다:\n{text}");
    assert!(text.contains(": 2건"), "값이 2 가 아니다:\n{text}");
}

/// **음성 대조 — 콜론이 붙으면 근거다.** 위 시험에서 콜론 하나만 더한다.
#[test]
fn a_safety_with_a_colon_is_a_reason() {
    let d = synth_root();
    reasoned_probe(
        d.path(),
        "// SAFETY: 포인터는 같은 문장에서 mmap 된 것이라 살아 있다",
        2,
    );
    set_cap(d.path(), 0);
    let (code, text) = run(d.path());
    assert_eq!(code, 0, "SAFETY: 근거를 안 읽는다:\n{text}");
    assert!(text.contains(": 0건"), "값이 0 이 아니다:\n{text}");
}

/// 좌변이 통째로 비면 **판정 불가(2)** 다. 위반 0 이 아니다.
#[test]
fn an_empty_left_side_is_undecidable() {
    let d = synth_root();
    for rel in ["src/zz_quiet.rs", "crates/zz/src/zz_quiet.rs"] {
        fs::remove_file(d.path().join(rel)).expect("프로브 제거");
    }
    git(d.path(), &["add", "-A"]);
    let (code, text) = run(d.path());
    assert_eq!(code, 2, "좌변이 비었는데 값을 냈다:\n{text}");
    assert!(
        text.contains("아무것도 안 봤다"),
        "빈 좌변을 '근거 없는 억제가 없다' 로 읽고 있다:\n{text}"
    );
}
