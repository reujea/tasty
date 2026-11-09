//! 공유 렌더러 고정값 계산(시험 전용 — docs/surfaces/mdedit/index.md#렌더러-제자리-컴파일). render.rs·`crates/tasty-plugin-markdown/src/render/` 하위 모듈과 markdown 의 `assets/` 전부를
//! 상대 경로 순으로 이어 sha256 을 낸다. 경로까지 넣는 것은 자산 하나가 이름만 바뀌어도
//! 산출물(include 경로)이 달라지기 때문이다.

use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .map(|e| e.expect("dir entry").path())
        .collect();
    entries.sort();
    for p in entries {
        if p.is_dir() {
            collect(&p, out);
        } else {
            out.push(p);
        }
    }
}

pub fn current_pin() -> String {
    let md = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tasty-plugin-markdown");
    let mut files = vec![md.join("src/render.rs")];
    collect(&md.join("src/render"), &mut files);
    collect(&md.join("assets"), &mut files);
    let mut h = Sha256::new();
    for f in &files {
        let rel = f
            .strip_prefix(&md)
            .unwrap_or(f)
            .to_string_lossy()
            .replace('\\', "/");
        h.update(rel.as_bytes());
        h.update([0]);
        h.update(std::fs::read(f).unwrap_or_else(|e| panic!("{}: {e}", f.display())));
        h.update([0]);
    }
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}
