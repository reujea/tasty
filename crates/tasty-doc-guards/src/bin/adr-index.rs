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
//! 0 = 파일이 생성 결과와 같다(또는 `--write` 로 맞췄다). 1 = 다르다(`--check`), 또는 생성 구역
//! **밖**에 표 줄이 있다(두 모드 모두 — 마커 밖은 생성기가 안 건드리므로 `--write` 로 안 없어진다).
//! 2 = 만들 수 없다 — ADR 디렉토리·인덱스를 못 읽음, 마커 짝이 깨짐, ADR 0 개.
//! 0 개를 0 으로 돌려주면 빈 모수에서 초록이 난다.

use std::path::PathBuf;

use tasty_doc_guards::adr_index::{
    INDEX, collect, group_slugs, region_row_count, render_index, stray_table_lines,
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
    eprintln!(
        "[adr-index] ADR {} · 그룹 {} · 생성 구역 행 {} · 구역 밖 표 줄 {}",
        adrs.len(),
        group_slugs(&current).len(),
        region_row_count(&rendered),
        stray.len()
    );
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
        if !stray.is_empty() {
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
