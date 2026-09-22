//! **새 bin 이 들어오면 조립은 저절로 따라오고 판정은 안 따라온다** — 그 비대칭을 막는다.
//!
//! 이 크레이트의 `src/bin/` 에 파일을 하나 놓으면 cargo 가 `[[bin]]` 선언 없이 자동으로
//! 발견해 빌드한다. 그래서 `doc-guards.yml` 의 패키지 테스트가 **컴파일 채널은 공짜로**
//! 준다. 그런데 그 바이너리가 무엇을 출력하는지는 아무것도 안 본다 — 판정 채널은 누가
//! 손으로 만들어야만 생긴다. 실측으로 그 형태가 났다(2026-09-06): 새 bin 하나를 들였고,
//! 컴파일은 저절로 됐고, 판정은 같은 회차에 직접 쓴 테스트뿐이었다. 안 썼으면 없었다.
//!
//! 그때 고쳐진 것은 **사례 하나**지 부류가 아니다. 다음 사람이 두 번째 bin 을 들이면
//! 같은 일이 다시 일어난다. 그래서 부류로 만든다.
//!
//! # 무엇으로 짝을 찾는가 — 이름이 아니다
//!
//! `mask-source.rs` ↔ `mask_source_bin.rs` 처럼 **이름 규약**으로 짝지을 수 있다. 쓰지
//! 않는다. 그것은 관례를 흉내 내는 것이지 성질을 묻는 것이 아니고, 두 방향으로 다 틀린다:
//! 테스트 파일 이름만 바꾸면 커버리지가 그대로인데 빨개지고, 이름만 맞고 아무것도 안
//! 돌리는 빈 파일은 초록이다.
//!
//! 대신 **cargo 자신이 주는 실행 핸들**을 묻는다. 통합 테스트가 빌드된 바이너리를 찾는
//! 지원되는 방법은 `env!("CARGO_BIN_EXE_<이름>")` 하나뿐이다. 그 상수가 이 크레이트의
//! 테스트 어딘가에 있다는 것은 **누군가 그것을 실제로 실행한다**는 뜻이고, 그것은 이름이
//! 아니라 성질이다.
//!
//! 이 선택이 틀리는 방향은 하나뿐이고 **시끄러운 쪽**이다: 누가 핸들 대신 경로를
//! 하드코딩해 돌리면 이 가드는 "채널이 없다" 고 **거짓 경보**를 낸다. 조용히 놓치지는
//! 않는다.
//!
//! # 이 가드가 단정하지 않는 것
//!
//! **그 실행이 옳게 판정하는지는 안 본다.** 볼 수 없다 — 출력이 맞는지는 이 가드가 아니라
//! 그 테스트가 답할 물음이다. 둘째 축은 그보다 훨씬 약한 것만 묻는다: 그 파일이 출력을
//! **읽기는 하는가**(`status` / `stdout`). "돌려놓고 눈을 돌린" 형태만 배제한다.
//!
//! 그리고 **이 판정의 배선 자체는 아무도 안 본다.** 아래 픽스처 둘은 판독기(`handles_in`)의
//! 극성을 고정하지만, 레포 판정의 걸러내는 줄을 "언제나 통과" 로 바꾸면 다섯이 모두 초록이다
//! — 실측했다(2026-09-06). 자기 자신을 재는 자리는 언제나 한 겹 남고, 그 한 겹은 여기서
//! 못 닫는다. 적어두는 이유는 닫혔다고 오해하지 않게 하려는 것이다.
//!
//! 곁으로 하나 — 이 파일처럼 **아직 커밋되지 않은 새 파일**에서는 변이 적용 단정으로
//! `git diff` 를 쓰면 안 된다. 추적 대상이 아니라 무엇을 고쳐도 빈 출력이라, "변이가
//! 적용됐다" 를 재는 자리가 조용히 무력해진다. 원문 사본과 대조하거나 치환 횟수를 세라.

// 이유: 이 타깃은 시험 범위다. `let _` 로 값을 버리는 자리를 여기서 명부에 올리면
//       그 명부가 프로덕션 자리를 가리키는 뜻을 잃는다 —
//       `crates/tasty-doc-guards/tests/let_underscore_documented.rs` 의 명부 순수성 판정이 그것을 막는다.
#![allow(clippy::let_underscore_must_use)]

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

const CRATE_DIR: &str = "crates/tasty-doc-guards";

/// bin 이 이보다 적으면 수집이 깨진 것이다 — 모수가 0 이면 "전부 덮였다" 는 언제나 참이다.
///
/// **판별식** — 이 수가 지금도 옳은지는 서로 다른 두 출처를 대조해서 잰다:
///
/// ```text
/// ls crates/tasty-doc-guards/src/bin/*.rs | wc -l    # 이 가드가 세는 것(파일)
/// cargo metadata --no-deps …  kind 가 bin 인 target 수  # cargo 가 실제로 만드는 것
/// ```
///
/// 실측 2026-09-23: 둘 다 **4** 다 — `adr-index` · `mask-source` · `strip-cfg-test` ·
/// `workflow-channels`. 이 하한도 4 라 **여유가 0** 이고, 그것이 맞다: 하한은 실측과
/// 붙어 있어야 예리하고, 벌어진 만큼이 곧 안 보는 구간이다.
///
/// ★ 이 하한이 왜 필요한지는 **세 번째 수**가 답한다 — `Cargo.toml` 의 `[[bin]]` 선언이
/// **0** 이다. 이 크레이트는 cargo 의 자동 발견에만 기대고 있어서, `src/bin/` 에서 파일이
/// 사라지면 bin 도 함께 조용히 사라진다. "넷이어야 한다" 고 적힌 곳이 레포 어디에도 없고,
/// 그 침묵을 메우는 것이 이 수다.
///
/// **이 수를 내려서 초록을 만들지 마라.** 내리면 아래 전수 명제("모든 bin 이 실행 채널을
/// 갖는다")가 사라진 bin 을 아예 안 세면서 통과한다 — 모수가 준 만큼 명제가 약해지는데
/// 색은 그대로다.
///
/// 정당한 수선: bin 을 **실제로 지웠으면** 이 수도 같은 커밋에서 함께 내려라. 그때 위 두
/// 출처를 다시 세서 값이 같은지 확인해라 — 파일만 지우고 빌드 산출물이 남아 있으면 두 수가
/// 갈리고, 그 상태에서 고른 값은 둘 중 어느 쪽도 아니다.
const MIN_BINS: usize = 4;

/// 출력을 읽는다고 볼 표지. `status` 는 종료코드, `stdout` 은 내용이다.
const READS_OUTPUT: &[&str] = &["stdout", "status", "code()"];

/// 그 소스가 실행 결과를 **읽기는 하는가.**
///
/// 판정을 함수로 꺼낸 이유는 부를 수 있게 하기 위해서다 — 목록이 순회 안에 인라인이면
/// 성분 하나를 지워도 레포 판정은 조용하다. 실측 2026-09-06(트리 9c1419aa2):
/// `"stdout"` 을 지워도 `cargo test -p tasty-doc-guards` 는 rc=0 이었다. 이유는
/// **형제가 받쳐 주기** 때문이다 — bin 을 돌리는 파일 넷 중 `stdout` 을 담은 둘이
/// `status`/`code()` 도 함께 담고 있어서, 하나를 지워도 다른 하나가 같은 파일을 덮었다.
/// 목록이 뚫린 것은 아니지만 **그 성분을 지키는 것이 없다.**
fn reads_output(text: &str) -> bool {
    READS_OUTPUT.iter().any(|m| text.contains(m))
}

fn repo_root() -> PathBuf {
    let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    while !dir.join(".git").exists() {
        if !dir.pop() {
            panic!("레포 루트를 못 찾았다");
        }
    }
    dir
}

/// `src/bin/` 의 바이너리 이름들 — 파일 stem 이 곧 cargo 가 쓰는 이름이다.
fn bin_names(crate_dir: &Path) -> BTreeSet<String> {
    let dir = crate_dir.join("src/bin");
    let Ok(entries) = std::fs::read_dir(&dir) else {
        panic!("`{}` 를 읽지 못했다", dir.display());
    };
    let mut names = BTreeSet::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_some_and(|e| e == "rs")
            && let Some(stem) = path.file_stem()
        {
            names.insert(stem.to_string_lossy().to_string());
        }
    }
    names
}

/// 테스트 파일 하나가 실행 핸들로 지목한 bin 이름들.
///
/// 순수 함수라 픽스처로 양극성을 고정할 수 있다. 이름을 통째로 적지 않고 **조각으로
/// 조립**해서 찾는다 — 이 파일 자신이 상수에 완성된 핸들을 담으면, 레포를 훑을 때 이
/// 파일이 그 bin 을 덮는 것으로 잘못 세게 된다. 그 오류는 조용한 방향이다.
fn handles_in(source: &str) -> BTreeSet<String> {
    let needle = concat!("CARGO_BIN_", "EXE_");
    let mut found = BTreeSet::new();
    let mut from = 0;
    while let Some(rel) = source[from..].find(needle) {
        let at = from + rel + needle.len();
        from = at;
        let name: String = source[at..]
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
            .collect();
        if !name.is_empty() {
            found.insert(name);
        }
    }
    found
}

/// bin 이름 → 그것을 실행하는 테스트 파일들.
fn executors(crate_dir: &Path) -> BTreeMap<String, Vec<String>> {
    let dir = crate_dir.join("tests");
    let Ok(entries) = std::fs::read_dir(&dir) else {
        panic!("`{}` 를 읽지 못했다", dir.display());
    };
    let mut map: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_none_or(|e| e != "rs") {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let file = path.file_name().unwrap_or_default().to_string_lossy();
        for name in handles_in(&text) {
            map.entry(name).or_default().push(file.to_string());
        }
    }
    map
}

#[test]
fn every_bin_is_run_by_some_test() {
    let crate_dir = repo_root().join(CRATE_DIR);
    let bins = bin_names(&crate_dir);
    assert!(
        bins.len() >= MIN_BINS,
        "`{CRATE_DIR}/src/bin` 에서 bin 을 {}개밖에 못 셌다(하한 {MIN_BINS}) — 수집이 \
         깨졌다. 모수가 줄면 '전부 덮였다' 는 아무것도 안 지킨다",
        bins.len()
    );

    let runs = executors(&crate_dir);
    let orphans: Vec<&String> = bins.iter().filter(|b| !runs.contains_key(*b)).collect();
    assert!(
        orphans.is_empty(),
        "아래 bin 은 컴파일만 되고 **아무도 안 돌린다**. cargo 가 `src/bin/*.rs` 를 자동으로 \
         발견해 빌드하므로 조립 채널은 저절로 생겼지만, 출력을 보는 자리는 손으로 만들어야 \
         생긴다:\n  {}\n\n  [무엇을 보는가] 이 크레이트 `tests/` 안에서 그 bin 의 실행 \
         핸들(cargo 가 주는 `CARGO_BIN_EXE_<이름>` 상수)이 쓰인 자리를 찾는다. 이름 규약으로 \
         짝짓지 않으므로 테스트 파일 이름은 무엇이든 좋다.\n  \
         [단정하지 않는 것] 그 실행이 **옳게 판정하는지는 안 본다.** 그러니 이 빨강의 처방은 \
         하나뿐이다 — 그 bin 을 실제로 돌려 출력을 확인하는 테스트를 더해라. 이미 그런 \
         테스트가 있는데 빨갛다면, 핸들 대신 경로를 하드코딩한 것이다(그때는 핸들로 바꿔라 \
         — 경로는 프로필마다 달라 러너에서 깨진다).",
        orphans
            .iter()
            .map(|b| format!("src/bin/{b}.rs"))
            .collect::<Vec<_>>()
            .join("\n  ")
    );
}

#[test]
fn no_executor_runs_the_bin_and_looks_away() {
    let crate_dir = repo_root().join(CRATE_DIR);
    let runs = executors(&crate_dir);
    assert!(
        !runs.is_empty(),
        "실행 핸들을 하나도 못 찾았다 — 판독이 깨졌다. 이 상태에서는 앞 판정도 전부 \
         '고아' 로 나오거나(시끄러움) 아무것도 안 본 것이다"
    );

    let mut blind = Vec::new();
    for (bin, files) in &runs {
        for file in files {
            let path = crate_dir.join("tests").join(file);
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            if !reads_output(&text) {
                blind.push(format!("tests/{file} — `{bin}` 을 돌리고 출력을 안 읽는다"));
            }
        }
    }
    assert!(
        blind.is_empty(),
        "아래는 bin 을 실행하지만 그 **출력을 읽지 않는다**(`status` 도 `stdout` 도 안 \
         본다). 실행만 하는 것은 '죽지 않는다' 만 재는 것이라 컴파일 채널보다 조금 나은 \
         정도다:\n  {}\n\n  [단정하지 않는 것] 출력을 **옳게** 읽는지는 여기서 못 본다 — \
         읽기는 하는가만 묻는다.",
        blind.join("\n  ")
    );
}

// ─── 판정기 자신의 양극성 (합성 입력) ────────────────────────────────────────
//
// 레포 상태는 지금 3/3 이 덮여 있어 **한 방향밖에 안 보인다.** 한 방향만 재면 무정보다 —
// `handles_in` 이 늘 비어 있어도, 늘 가득 차 있어도 레포 판정은 초록일 수 있다. 그래서
// 양극을 픽스처로 고정한다. 핸들은 **조각으로 조립**한다(위 `handles_in` 의 주석 참조).

fn handle(name: &str) -> String {
    format!("env!(\"{}{name}\")", concat!("CARGO_BIN_", "EXE_"))
}

#[test]
fn the_reader_answers_both_yes_and_no() {
    let covered = format!("const BIN: &str = {};", handle("mask-source"));
    assert!(
        handles_in(&covered).contains("mask-source"),
        "실행 핸들이 있는 소스에서 그 이름을 못 찾았다 — 이 판독이 늘 '없다' 를 내면 \
         모든 bin 이 고아로 나온다"
    );

    let uncovered = "let path = \"target/debug/mask-source\"; // 경로 하드코딩";
    assert!(
        handles_in(uncovered).is_empty(),
        "핸들이 아닌 경로 문자열을 실행 채널로 셌다 — 이 판독이 늘 '있다' 를 내면 앞 \
         판정은 아무것도 안 지킨다"
    );
}

#[test]
fn a_second_bin_in_the_same_file_is_not_swallowed() {
    let both = format!("{}\n{}", handle("alpha"), handle("beta-two"));
    let found = handles_in(&both);
    assert!(
        found.contains("alpha") && found.contains("beta-two"),
        "한 파일이 bin 둘을 돌리는데 하나만 셌다: {found:?} — 뒤엣것이 고아로 나온다"
    );
}

#[test]
fn a_bare_prefix_is_not_a_bin_name() {
    // 이 파일 자신이 상수에 담고 있는 형태다. 빈 이름을 주우면 그 이름의 bin 이 늘
    // 덮인 것으로 세어져, `handles_in` 이 아무 이름에나 참을 내는 것과 같아진다.
    let bare = format!(
        "const NEEDLE: &str = \"{}\";",
        concat!("CARGO_BIN_", "EXE_")
    );
    assert!(
        handles_in(&bare).is_empty(),
        "이름 없는 접두사를 bin 이름으로 주웠다"
    );
}

/// [`READS_OUTPUT`] 의 성분마다, 그 성분 **하나만** 든 소스가 "출력을 읽는다" 로 읽히는가.
///
/// 조각을 상수에서 만들지 않고 손으로 적는다 — 목록을 순회해 조각을 지으면 오타 난
/// 항목(`stdoutt`)도 자기 자신과는 맞아 통과하고, 그러면 이 테스트가 목록의 사본이 된다.
///
/// ★ 조각마다 성분을 **하나만** 담는 것이 여기서는 특히 중요하다. 이 목록이 조용했던
/// 이유가 정확히 그것이기 때문이다 — 레포의 실제 파일들은 세 성분을 여럿 함께 담고 있어
/// 하나를 지워도 다른 하나가 받쳐 주었다. 그래서 `code()` 조각에는 `status` 를 안 쓰고
/// (`exit.code()` 로 적는다), `status` 조각에는 `code()` 를 안 쓴다.
#[test]
fn every_output_marker_is_actually_read_as_reading_output() {
    let cases: [(&str, &str); 3] = [
        (
            "stdout",
            r#"let text = String::from_utf8_lossy(&out.stdout);"#,
        ),
        (
            "status",
            r#"assert!(out.status.success(), "판정기가 죽었다");"#,
        ),
        (
            "code()",
            r#"assert_eq!(exit.code(), Some(0), "종료 코드");"#,
        ),
    ];
    for (marker, snippet) in cases {
        assert!(
            reads_output(snippet),
            "`{marker}` 하나로 출력을 읽는 소스를 '안 읽는다' 로 읽는다. 그 성분이 \
             목록에서 빠졌거나 철자가 틀렸다 — 그러면 그 방식으로만 결과를 보는 \
             테스트가 '돌리고 눈감는다' 로 고발된다(거짓 빨강)"
        );
    }
}

/// 음성 대조 — 이 술어가 늘 참을 내면 위 판정이 통째로 공허해진다.
#[test]
fn running_a_bin_without_looking_is_not_reading_output() {
    assert!(
        !reads_output(r#"Command::new(BIN).arg("--help").output().unwrap();"#),
        "결과를 하나도 안 보는 소스를 '읽는다' 로 셌다 — 이 술어가 늘 참이면 \
         `no_executor_runs_the_bin_and_looks_away` 는 아무것도 안 지킨다"
    );
}

/// 유일한 임시 뿌리. `line!()` 까지 넣어 같은 파일의 여러 자리가 겹치지 않게 한다.
fn probe_root(tag: &str, line: u32) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "tasty-binfloor-{tag}-{}-{line}",
        std::process::id()
    ));
    // 앞선 실행의 잔여를 치운다 — 없는 것이 정상이라 실패가 정보가 아니다.
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn with_bins(tag: &str, line: u32, names: &[&str]) -> PathBuf {
    let root = probe_root(tag, line);
    let bin = root.join("src/bin");
    std::fs::create_dir_all(&bin).expect("임시 디렉토리를 못 만들었다");
    for n in names {
        std::fs::write(bin.join(format!("{n}.rs")), "fn main() {}\n").expect("쓰기 실패");
    }
    root
}

/// [`MIN_BINS`] 의 **양성 대조** — 수집이 죽으면 이 수가 실제로 하한 밑으로 떨어지나.
///
/// 하한은 "수집이 깨지면 모수가 0 이 되고 그러면 전수 명제가 공허해진다" 를 막으려고 있다.
/// 그 전제 — **수집이 깨지면 수가 준다** — 를 지금까지 아무도 안 봤다. 하한 옆에 붙은
/// 판별식은 지금 값이 옳은지를 말하지, 이 계기가 반응하는지를 말하지 않는다.
///
/// 세 칸을 함께 두는 이유: 0 만 보이면 "이 함수는 언제나 0 을 낸다" 와 구별이 안 된다.
/// 마지막 칸이 그 비영 대조다.
#[test]
fn the_bin_floor_sees_a_collapsed_collection() {
    let empty = with_bins("empty", line!(), &[]);
    assert_eq!(
        bin_names(&empty).len(),
        0,
        "빈 `src/bin` 에서 0 이 아니면 이 수집기는 입력을 안 보는 것이고, 그러면 하한이 \
         지키는 것이 없다"
    );

    let one = with_bins("one", line!(), &["only"]);
    assert!(
        bin_names(&one).len() < MIN_BINS,
        "부분적으로 죽어도 하한 밑으로 떨어져야 한다 — 그래야 하한이 그것을 말한다"
    );

    // 비영 대조: 이 수집기가 언제나 작은 수를 내는 것은 아니다.
    let many = with_bins("many", line!(), &["a", "b", "c", "d"]);
    assert!(
        bin_names(&many).len() >= MIN_BINS,
        "하한을 넘는 입력에서도 넘지 못하면 위 두 칸은 수집기가 늘 0 이라는 뜻이라 \
         아무것도 안 지킨다"
    );

    for d in [empty, one, many] {
        // 뒷정리 실패는 무시한다 — 임시 디렉토리라 남아도 다음 실행이 먼저 지우고,
        // 여기서 죽으면 위 단정의 결과가 정리 오류에 가린다.
        let _ = std::fs::remove_dir_all(d);
    }
}
