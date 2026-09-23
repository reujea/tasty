//! `docs/adr/index.md` 의 생성 구역(`adr-rows:begin` ~ `adr-rows:end`)을 ADR 헤더에서
//! 다시 만든다. 규칙은 [`tasty_doc_guards::adr_index`] 하나이고, 가드
//! (`tests/adr_index_parity.rs`)가 같은 함수로 "파일이 생성 결과와 같은가" 를 묻는다.
//!
//! ```text
//! cargo run -p tasty-doc-guards --bin adr-index -- --write   # 다시 쓴다
//! cargo run -p tasty-doc-guards --bin adr-index               # 대조만(--check)
//! ```
//!
//! 뒤에 레포 루트 경로를 줄 수 있다(생략하면 현재 디렉토리).
//!
//! ## 종료코드
//!
//! 0 = 파일이 생성 결과와 같고(또는 `--write` 로 맞췄고) 그 결과가 완전하다. 1 = 다르다
//! (`--check`), 또는 생성 구역 **밖**에 표 줄이나 git 충돌 표지가 있다, 또는 생성 결과가
//! **불완전하다** — 생성 구역에 행이 없거나 둘 이상인 ADR 이 있다(`Group` 이 없거나 인덱스에
//! 없는 slug). 뒤의 둘은 두 모드 모두다 — 마커 밖은 생성기가 안 건드리고, 빠진 행은 헤더를
//! 고쳐야 생기므로 `--write` 로 안 없어진다. 빠진 행을 0 으로 돌려주면 "생성 결과와 같다" 만
//! 보고 ADR 이 인덱스에서 빠진 상태를 통과시킨다.
//! 2 = 만들 수 없다 — 모르는 인자(`--` 로 시작하는), ADR 디렉토리 · ADR 파일 · 인덱스를 못
//! 읽음, 마커 짝이 깨짐, ADR 0 개, 인덱스를 못 씀(`--write`).
//! 0 개를 0 으로 돌려주면 빈 모수에서 초록이 난다.

use std::path::PathBuf;

use tasty_doc_guards::adr_index::{
    INDEX, collect, conflict_marker_lines, group_slugs, incomplete_rows, region_row_count,
    render_index, stray_table_lines,
};

fn fail(msg: &str) -> ! {
    eprintln!("[adr-index] {msg}");
    std::process::exit(2)
}

fn main() {
    let mut write = false;
    let mut root = PathBuf::from(".");
    for a in std::env::args().skip(1) {
        match a.as_str() {
            "--write" => write = true,
            "--check" => write = false,
            other if other.starts_with("--") => fail(&format!("모르는 인자: {other}")),
            other => root = PathBuf::from(other),
        }
    }
    let adrs = collect(&root).unwrap_or_else(|e| fail(&e));
    if adrs.is_empty() {
        fail("ADR 을 하나도 못 찾았다 — 루트가 틀렸거나 수집이 죽었다");
    }
    let path = root.join(INDEX);
    let current = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| fail(&format!("{INDEX} 를 못 읽었다: {e}")))
        .replace("\r\n", "\n");
    let rendered = render_index(&current, &adrs)
        .unwrap_or_else(|errs| fail(&format!("마커 구조가 깨졌다:\n  {}", errs.join("\n  "))));
    let stray = stray_table_lines(&current);
    let conflicts = conflict_marker_lines(&current);
    let incomplete = incomplete_rows(&rendered, &adrs);
    // 행 수는 둘을 함께 찍는다 — 하나만 찍으면 그것이 파일의 상태인지 생성 결과인지 라벨로만
    // 갈리고, `--check` 가 빨간 때는 둘이 다르다(행을 지운 파일 · Group 을 지운 ADR).
    eprintln!(
        "[adr-index] ADR {} · 그룹 {} · 생성 구역 행 현재 {} / 생성 결과 {} · 구역 밖 표 줄 {} · \
         구역 밖 충돌 표지 {}",
        adrs.len(),
        group_slugs(&current).len(),
        region_row_count(&current),
        region_row_count(&rendered),
        stray.len(),
        conflicts.len()
    );
    if !conflicts.is_empty() {
        eprintln!(
            "[adr-index] {INDEX} 의 생성 구역 밖(머리말)에 git 충돌 표지가 남아 있다. 생성기는 \
             머리말을 안 건드린다 — 양쪽 산문을 사람이 합친 뒤 표지를 지운다:\n  {}",
            conflicts.join("\n  ")
        );
    }
    if !incomplete.is_empty() {
        eprintln!(
            "[adr-index] 생성 결과가 불완전하다 — ADR {} 개 중 {} 개가 생성 구역에 정확히 한 행을 \
             갖지 않는다. 행은 헤더의 `- **Group**: <slug>` 가 가리키는 구역에만 생긴다 — 그 ADR \
             헤더에 인덱스에 있는 slug 하나를 적고 다시 돌린다(고르는 기준은 template 의 \
             \"인덱스에 행을 추가할 때\"):\n  {}",
            adrs.len(),
            incomplete.len(),
            incomplete.join("\n  ")
        );
    }
    if !stray.is_empty() {
        eprintln!(
            "[adr-index] {INDEX} 의 생성 구역(`adr-rows:begin` ~ `adr-rows:end`) 밖에 표 줄이 있다. \
             생성기는 마커 밖을 안 건드리므로 이 줄은 `--write` 로 안 없어지고 생성 결과와의 대조에도 \
             안 걸린다. 행이면 지우고 그 ADR 헤더의 Group 을 확인한다 — 행은 생성 구역에만 산다:\n  {}",
            stray.join("\n  ")
        );
    }
    if rendered != current && write {
        std::fs::write(&path, &rendered)
            .unwrap_or_else(|e| fail(&format!("{INDEX} 를 못 썼다: {e}")));
        eprintln!("[adr-index] {INDEX} 를 다시 썼다");
    }
    if rendered == current || write {
        if !stray.is_empty() || !conflicts.is_empty() || !incomplete.is_empty() {
            std::process::exit(1)
        }
        return;
    }
    eprintln!(
        "[adr-index] {INDEX} 의 생성 구역이 ADR 헤더와 다르다. 생성 구역은 손으로 고치지 않는다 — \
         ADR 헤더(제목 · Status · Date · Tags · Group)를 고친 뒤 `cargo run -p tasty-doc-guards \
         --bin adr-index -- --write` 로 다시 만든다"
    );
    std::process::exit(1)
}
