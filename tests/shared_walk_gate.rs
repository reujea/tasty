//! 공용 순회 래칫(`scripts/check-shared-walk-ratchet.sh`)의 **좌변**을 합성 트리로
//! 고정한다.
//!
//! 왜 좌변인가. 이 게이트는 같은 트리를 **세 번** 지목했다 — 추적 타깃을 낼 때 ·
//! 미추적 타깃을 볼 때 · 마스킹 사본을 뜰 때. 앞의 둘은 pathspec 과 한 겹 필터를
//! 글자 그대로 두 번 갖고 있었고 셋째는 또 다른 표기(디렉토리)였다. 형제
//! `check-intent-discipline.sh` 가 정확히 그 형태로 갈려 있었고(그쪽은 넷), 한쪽만
//! 손대면 나머지가 안 따라가면서 **그 어긋남이 조용했다.**
//!
//! **판정 방법: 좌변 값을 늘려 놓고 소비처가 따라오는지 종료 코드로 묻는다.**
//! "게이트에 `SCAN_SPECS` 가 나온다" 를 세는 문자열 확인은 **철자를 보는 것**이지 세
//! 소비처가 같은 값을 낸다는 것을 보는 게 아니다 — 변수를 선언해 놓고 소비처 하나가
//! 옛 문자열을 그대로 쓰는 상태(= 고치기 전의 모양)를 그대로 통과시킨다.
//!
//! ★ **래칫은 형제 hard-fail 과 관측 축이 다르다.** `check-intent-discipline.sh` 에서는
//! 좌변이 반쯤 죽어도 위반 0 이면 조용히 초록이라, 늘린 좌변에서 위반이 나오는지를
//! 물으면 됐다. 여기서는 래칫이 **양방향**이라 좌변이 줄면 `count < CAP` 으로 이미
//! 빨개진다 — 늘려도 `count > CAP` 으로 빨개진다. 그래서 rc=1 만으로는 어느 소비처가
//! 안 따라왔는지가 안 갈린다. 그 대신 **상한을 프로브에 맞춰 놓고 초록을 기대한다**:
//! 셋이 전부 따라오면 값이 상한과 맞아 rc=0 이고, 하나라도 안 따라오면 값이 어긋나
//! rc=1(또는 판정 불가 2)이 된다.
//!
//! 프로브가 셋인 이유는 하나로는 하나씩만 죽기 때문이다. 자세한 갈래는 각 시험에.

#![cfg(unix)]

use std::fs;
use std::path::Path;
use std::process::Command;

fn gate_src() -> String {
    format!(
        "{}/scripts/check-shared-walk-ratchet.sh",
        env!("CARGO_MANIFEST_DIR")
    )
}

/// 좌변이 안 빈 합성 git 레포. 게이트가 판정 불가를 낼 이유가 없는 바닥 상태다.
///
/// `git init` 이 필수다 — 좌변이 `git ls-files` 라, 레포가 아니면 빈 좌변으로 떨어져
/// 이 파일의 모든 시험이 **재려던 것과 다른 갈래**(판정 불가)를 재게 된다.
fn synth_root() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("임시 디렉토리");
    let root = dir.path();
    let here = env!("CARGO_MANIFEST_DIR");
    fs::create_dir_all(root.join("scripts/lib")).expect("scripts/lib");
    fs::copy(
        gate_src(),
        root.join("scripts/check-shared-walk-ratchet.sh"),
    )
    .expect("게이트 복사");
    fs::copy(
        format!("{here}/scripts/lib/judge-bin.sh"),
        root.join("scripts/lib/judge-bin.sh"),
    )
    .expect("판정기 찾기 공용 복사");
    // 좌변이 보는 두 모양을 하나씩 실재시킨다. 순회는 없다 — 프로브가 넣는다.
    write_file(root, "tests/zz_quiet.rs", "fn f() {}\n");
    write_file(root, "crates/zz/tests/zz_quiet.rs", "fn g() {}\n");
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

/// 합성 루트에 **스텁 판정기**를 깐다 — 진짜 `mask-source` 를 안 쓴다.
///
/// 이유는 자매 픽스처와 같다: 진짜 판정기는 cargo 산출물이라 여기서 지으면 바깥
/// `cargo test` 와 빌드 디렉토리 잠금을 두고 서로를 기다린다.
///
/// ★ 이 스텁은 **베끼기만 하면 안 된다.** 형제 픽스처의 스텁은 그래도 충실했지만
/// (그쪽 합성 소스에는 주석도 문자열도 없다), 여기 프로브 하나는 순회 수단의 이름을
/// **문자열 안에** 넣어 원문과 사본을 일부러 갈라 놓는다. 덮지 않으면 그 시험이
/// 재려던 갈림이 사라진다. 그래서 줄 수를 보존한 채 문자열과 주석을 지운다.
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
         # 줄 수는 보존한 채 문자열을 덮는다. 주석은 --keep-comments 일 때만 남긴다.\n\
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

fn run(root: &Path) -> (i32, String) {
    let stub = install_stub_masker(root);
    let out = Command::new("bash")
        .arg(root.join("scripts/check-shared-walk-ratchet.sh"))
        .current_dir(root)
        .env("TASTY_MASK_SOURCE_BIN", &stub)
        .output()
        .expect("게이트 실행");
    (
        out.status.code().unwrap_or(-1),
        format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        ),
    )
}

/// 게이트 사본의 좌변을 `extra/*.rs` 만큼 늘리고, 상한을 프로브에 맞춘다.
///
/// 늘어난 쪽이 안 보이면 시험이 무의미하므로 치환 실패는 그 자리에서 죽는다 — 좌변이
/// 다시 여럿으로 흩어졌다는 뜻이라 그것도 잡아야 할 회귀다. 상한 치환도 같다.
fn widen_and_cap(root: &Path, cap: usize) {
    let p = root.join("scripts/check-shared-walk-ratchet.sh");
    let text = fs::read_to_string(&p).expect("게이트 사본을 읽을 수 없다");
    let widened = text.replace(
        "SCAN_SPECS=('tests/*.rs' 'crates/*/tests/*.rs')",
        "SCAN_SPECS=('tests/*.rs' 'crates/*/tests/*.rs' 'extra/*.rs')",
    );
    assert_ne!(
        widened, text,
        "게이트에 `SCAN_SPECS=(…)` 한 줄이 없다 — 좌변이 다시 여럿으로 흩어졌거나 \
         이름이 바뀌었다. 이 시험은 그 한 값을 늘려 소비처를 재므로 여기서 멈춘다."
    );
    fs::write(&p, widened).expect("게이트 사본 쓰기");
    set_cap(root, cap);
}

/// 상한만 프로브에 맞춘다. 좌변은 안 건드린다.
///
/// **상한 값 자체를 재는 것이 아니다.** 픽스처가 게이트의 상수로 트리를 지으면 그
/// 상수에 대해서는 항등식이 된다 — 여기서 재는 것은 판정 **기제**(넘으면 1 · 모자라면
/// 1 · 같으면 0)이고, 실제 상한이 옳은지는 게이트가 세는 실물이 답한다.
fn set_cap(root: &Path, cap: usize) {
    let p = root.join("scripts/check-shared-walk-ratchet.sh");
    let text = fs::read_to_string(&p).expect("게이트 사본을 읽을 수 없다");
    let capped = text.replace("CAP=54", &format!("CAP={cap}"));
    assert_ne!(
        capped, text,
        "게이트에 `CAP=54` 한 줄이 없다 — 상한 표기가 바뀌었다. 이 시험은 상한을 \
         프로브에 맞춰 놓고 초록을 기대하므로 여기서 멈춘다."
    );
    fs::write(&p, capped).expect("게이트 사본 쓰기");
}

/// 좌변을 늘리면 **추적 타깃을 내는 쪽**(`git ls-files` + 한 겹 필터)이 따라온다.
///
/// 죽이는 변이: pathspec 이나 한 겹 필터를 옛 문자열로 되돌리는 것. 그러면 `extra/` 의
/// 순회가 안 세어져 값이 상한보다 **작고**, 래칫의 미달 분기로 빨개진다.
///
/// 마스킹 뿌리만 되돌린 변이는 이 시험이 **안 죽인다** — 사본에 없는 파일은 게이트가
/// 그 파일만 원문에서 세므로(폴백) 값이 같다. 그쪽은 아래 문자열 프로브가 잡는다.
#[test]
fn widening_the_left_side_moves_the_tracked_scan() {
    let d = synth_root();
    write_file(
        d.path(),
        "extra/zz_probe.rs",
        "fn f() {\n    let _ = std::fs::read_dir(\".\");\n}\n",
    );
    git(d.path(), &["add", "-A"]);
    widen_and_cap(d.path(), 1);
    let (code, text) = run(d.path());
    assert_eq!(
        code, 0,
        "좌변을 늘렸는데 추적 타깃을 내는 쪽이 안 따라왔다 — 새 영토의 순회가 안 세어진다:\n{text}"
    );
    assert!(
        text.contains("타깃 3개"),
        "훑은 타깃 수가 새 영토를 안 담았다:\n{text}"
    );
}

/// 좌변을 늘리면 **마스킹 사본을 뜨는 쪽**도 따라온다.
///
/// 순회 수단의 이름을 **문자열 안**에 둔다. 사본을 뜨면 덮여서 0 이고, 안 뜨면 게이트가
/// 그 파일만 원문에서 세어 1 이 된다. 상한을 0 으로 두면 그 갈림이 그대로 rc 가 된다.
///
/// 이 형태는 픽스처가 아니라 실물이다 — 이 레포에는 순회 수단의 이름을 문자열로 담는
/// 가드 명부가 있고, 게이트 본문이 그 자리를 원문 58 · 사본 57 의 차 하나로 적어 둔다.
#[test]
fn widening_the_left_side_moves_the_masking() {
    let d = synth_root();
    write_file(
        d.path(),
        "extra/zz_probe.rs",
        "fn f() -> &'static str {\n    \"read_dir(\"\n}\n",
    );
    git(d.path(), &["add", "-A"]);
    widen_and_cap(d.path(), 0);
    let (code, text) = run(d.path());
    assert_eq!(
        code, 0,
        "좌변을 늘렸는데 마스킹이 안 따라왔다 — 새 영토를 원문으로 세어 문자열 안의 \
         언급이 순회로 잡힌다:\n{text}"
    );
}

/// 좌변을 늘리면 **미추적 타깃을 보는 쪽**도 따라온다.
///
/// 죽이는 변이: 미추적 pathspec 을 옛 문자열로 되돌리는 것. 그러면 `extra/` 의 미추적
/// 파일이 안 보여 판정 불가(2)가 아니라 그냥 값을 낸다 — 그 값은 그 파일의 순회가
/// 빠진 수이고, 상한과 같으면 **초록이 거짓이 된다.** 게이트 본문이 그 갈래를 실측으로
/// 적어 둔 자리다.
#[test]
fn widening_the_left_side_moves_the_untracked_check() {
    let d = synth_root();
    // 일부러 `git add` 하지 않는다 — 이것이 재려는 창이다.
    write_file(
        d.path(),
        "extra/zz_probe.rs",
        "fn f() {\n    let _ = std::fs::read_dir(\".\");\n}\n",
    );
    widen_and_cap(d.path(), 0);
    let (code, text) = run(d.path());
    assert_eq!(
        code, 2,
        "좌변을 늘렸는데 미추적 검사가 안 따라왔다 — 인덱스에 없는 타깃이 있는데 값을 \
         낸다. 그 값은 그 파일의 순회가 빠진 수이고, 상한과 같으면 초록이 거짓이 된다:\n{text}"
    );
    assert!(
        text.contains("extra/zz_probe.rs"),
        "무엇이 인덱스에 없는지를 안 찍는다:\n{text}"
    );
}

// ── 판정 자체 — 래칫이 **양쪽으로 서는가** ──────────────────────────────
//
// 위 셋은 전부 **좌변**을 잰다: 상한을 프로브에 맞춰 놓고 초록을 기대하는 형태라,
// 판정 갈래가 통째로 무너져도(예: 두 `exit 1` 이 `exit 0` 이 되어도) 셋 다 초록이다.
//
// 실측 2026-09-08: 이 게이트의 거절 다섯 자리에 완화 변이를 하나씩 넣으니 **둘만
// 죽었다**(미추적 검사·판정기 부재). 살아남은 셋이 좌변이 통째로 빈 갈래와 **래칫의
// 판정 둘**이다 — 즉 이 게이트가 존재하는 이유인 그 판정에 아무 시험도 없었다.
// 형제 `check-allow-reason.sh` 도 같은 모양이었다(5 중 2).

/// 프로브 하나에 `read_dir(` 를 `n` 번 담는다.
fn probe_with(root: &Path, n: usize) {
    let body: String = (0..n)
        .map(|i| format!("fn f{i}() {{\n    let _ = std::fs::read_dir(\".\");\n}}\n"))
        .collect();
    write_file(root, "tests/zz_probe.rs", &body);
    git(root, &["add", "-A"]);
}

/// 값이 상한과 같으면 통과(0)다. **대조군** — 이것이 없으면 아래 둘은 "항상 실패하는
/// 게이트" 로도 통과한다.
///
/// 양성 대조(2026-09-08, 내 트리): 게이트의 상한 비교를 `-gt` → `-ge` 로 바꾸면(같아도
/// 위반) 이 시험이 rc=101 로 죽는다. **배타적이지는 않다** — 같은 변이가
/// `widening_the_left_side_moves_the_masking` · `..._moves_the_tracked_scan` 도 함께
/// 죽인다(이 타깃 3 failed). 그 둘도 넓히기 전 팔에서 상한을 프로브에 맞춰 rc=0 을
/// 기대하기 때문이다.
///
/// 그래서 이 대조군이 더하는 것은 **검출이 아니라 이름**이다. 위 doc 이 말하는 "항상
/// 실패하는 게이트" 를 실제로 거르는 것은 이 갈래이고, 그 갈래가 깨졌을 때 어느 문장이
/// 뜨는지가 다음 사람을 어디로 보내는지 정한다.
#[test]
fn a_count_at_the_cap_passes() {
    let d = synth_root();
    probe_with(d.path(), 2);
    set_cap(d.path(), 2);
    let (code, text) = run(d.path());
    assert_eq!(code, 0, "값이 상한과 같은데 초록이 아니다:\n{text}");
}

/// 상한을 넘으면 위반(1)이고, 실패문이 **상한을 올리지 말라**고 말한다.
///
/// 그 문구가 판정의 일부인 이유: 이 자리에서 눈에 보이는 레버가 상한이라, 처방을 안
/// 적으면 다음 사람이 그것을 올린다. 그러면 래칫이 영구히 그만큼 헐거워진다.
#[test]
fn a_count_over_the_cap_is_a_violation() {
    let d = synth_root();
    probe_with(d.path(), 2);
    set_cap(d.path(), 1);
    let (code, text) = run(d.path());
    assert_eq!(code, 1, "상한을 넘었는데 위반이 아니다:\n{text}");
    assert!(
        text.contains("상한을 올려서 통과시키지 마라"),
        "상한을 올리지 말라는 처방이 없다 — 이 자리의 유일한 레버가 상한이다:\n{text}"
    );
}

/// 상한 아래로 내려가도 위반(1)이다. **한 방향만 서는 것은 래칫이 아니다.**
#[test]
fn a_count_under_the_cap_is_also_a_violation() {
    let d = synth_root();
    probe_with(d.path(), 2);
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

/// 좌변이 통째로 비면 **판정 불가(2)** 다. 위반 0 이 아니다.
///
/// 이 갈래가 초록이 되면 "직접 순회가 없다" 와 "아무것도 안 봤다" 가 같은 줄로 나간다.
#[test]
fn an_empty_left_side_is_undecidable() {
    let d = synth_root();
    for rel in ["tests/zz_quiet.rs", "crates/zz/tests/zz_quiet.rs"] {
        fs::remove_file(d.path().join(rel)).expect("프로브 제거");
    }
    git(d.path(), &["add", "-A"]);
    let (code, text) = run(d.path());
    assert_eq!(code, 2, "좌변이 비었는데 값을 냈다:\n{text}");
    assert!(
        text.contains("아무것도 안 봤다"),
        "빈 좌변을 '순회가 없다' 로 읽고 있다:\n{text}"
    );
}
