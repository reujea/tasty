//! 번들 플러그인 목록을 BUILTINS, 디스크 매니페스트, 패키징 문서와 카탈로그에서 대조한다.
//! BUILTINS 는 모든 플랫폼이 쓰는 표 하나다. 한 플랫폼의 컴파일만으로는 다른 cfg 분기의 이름을 확인하지
//! 못하므로, 분기가 다시 생기면 그 자체를 알린다.
//! 개수뿐 아니라 ID와 크레이트의 집합을 비교해 다른 플러그인으로 바뀐 경우도 찾는다.

use std::collections::BTreeSet;

use super::{repo_root, strip_comments};

const BUILTIN_SRC: &str = "crates/tasty-host-plugin/src/builtin.rs";
const PACKAGING_DOC: &str = "docs/dev-guide/plugin-packaging.md";
const PACKAGING_TABLE_HEAD: &str = "| crate | plugin ID |";
const CATALOG_DOC: &str = "docs/plugins/index.md";
const CATALOG_TABLE_HEAD: &str = "| 플러그인 (id) | 무엇 | 주요 기여 |";
const PLUGIN_CRATE_PREFIX: &str = "tasty-plugin-";
/// 검사 대상 문자열을 소스에 직접 남기지 않도록 조립한다.
const ID_PREFIX: &str = concat!("com.", "tasty.");

fn read(rel: &str) -> String {
    let path = repo_root().join(rel);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{rel} 을 읽지 못했다: {e}"))
}

/// `BuiltinSpec { id: "...", crate_dir: "...", .. }` 항목에서 (id, crate_dir) 를 뽑는다.
fn specs_in(block: &str) -> BTreeSet<(String, String)> {
    let mut out = BTreeSet::new();
    let mut id: Option<String> = None;
    for line in block.lines() {
        let t = line.trim();
        if let Some(v) = field(t, "id:") {
            id = Some(v);
        } else if let Some(v) = field(t, "crate_dir:")
            && let Some(i) = id.take()
        {
            // crate_dir 줄일 때만 take해야 앞에서 읽은 id를 잃지 않는다.
            out.insert((i, v));
        }
    }
    out
}

fn field(line: &str, key: &str) -> Option<String> {
    let rest = line.strip_prefix(key)?.trim_start();
    let rest = rest.strip_prefix('"')?;
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

/// 주석을 지운 뒤 각 cfg 분기의 BUILTINS 배열을 따로 읽는다.
fn builtin_arms() -> Vec<(String, BTreeSet<(String, String)>)> {
    let src = strip_comments(&read(BUILTIN_SRC));
    let mut arms = Vec::new();
    let rest = src.as_str();
    let mut cursor = 0usize;
    while let Some(i) = rest[cursor..].find("const BUILTINS") {
        let start = cursor + i;
        let label = src[..start]
            .rsplit('\n')
            .find(|l| l.trim_start().starts_with("#[cfg("))
            .unwrap_or("(cfg 없음)")
            .trim()
            .to_string();
        let end = rest[start..]
            .find("\n];")
            .map(|e| start + e)
            .unwrap_or(rest.len());
        arms.push((label, specs_in(&rest[start..end])));
        cursor = end;
    }
    arms
}

/// BUILTINS와 별개로 디스크의 플러그인 매니페스트에서 ID를 읽는다.
fn manifest_specs() -> BTreeSet<(String, String)> {
    let crates = repo_root().join("crates");
    let mut out = BTreeSet::new();
    let entries = std::fs::read_dir(&crates).expect("crates/ 를 읽지 못했다");
    for e in entries.flatten() {
        let dir = e.file_name().to_string_lossy().to_string();
        if !dir.starts_with(PLUGIN_CRATE_PREFIX) {
            continue;
        }
        let manifest = e.path().join("tasty-plugin.toml");
        let Ok(text) = std::fs::read_to_string(&manifest) else {
            continue; // 매니페스트가 없는 라이브러리 크레이트는 번들 대상이 아니다
        };
        for line in text.lines() {
            if let Some(v) = field(line.trim().replace(' ', "").as_str(), "id=") {
                out.insert((v, dir.clone()));
                break;
            }
        }
    }
    out
}

/// 지정한 머리글로 시작하는 표만 읽는다.
fn table_rows(doc: &str, head: &str) -> Vec<Vec<String>> {
    let text = read(doc);
    let mut rows = Vec::new();
    let mut inside = false;
    for line in text.lines() {
        let t = line.trim();
        if !inside {
            if t == head {
                inside = true;
            }
            continue;
        }
        if !t.starts_with('|') {
            break;
        }
        if t.chars().all(|c| "|-: ".contains(c)) {
            continue; // 구분선
        }
        rows.push(
            t.trim_matches('|')
                .split('|')
                .map(|c| c.trim().to_string())
                .collect(),
        );
    }
    assert!(
        !rows.is_empty(),
        "{doc} 에서 헤더 {head:?} 로 시작하는 표를 못 찾았다 — 표가 옮겨졌거나 헤더가 바뀌었다"
    );
    rows
}

fn backticked(cell: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = cell;
    while let Some(i) = rest.find('`') {
        let after = &rest[i + 1..];
        match after.find('`') {
            Some(j) => {
                out.push(after[..j].to_string());
                rest = &after[j + 1..];
            }
            None => break,
        }
    }
    out
}

fn packaging_specs() -> BTreeSet<(String, String)> {
    table_rows(PACKAGING_DOC, PACKAGING_TABLE_HEAD)
        .iter()
        .filter_map(|r| {
            let krate = backticked(r.first()?).into_iter().next()?;
            let id = backticked(r.get(1)?).into_iter().next()?;
            Some((id, krate))
        })
        .collect()
}

fn catalog_ids() -> BTreeSet<String> {
    table_rows(CATALOG_DOC, CATALOG_TABLE_HEAD)
        .iter()
        .filter_map(|r| {
            backticked(r.first()?)
                .into_iter()
                .find(|t| t.starts_with(ID_PREFIX))
        })
        .collect()
}

fn ids(specs: &BTreeSet<(String, String)>) -> BTreeSet<String> {
    specs.iter().map(|(i, _)| i.clone()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtins_is_one_table_for_every_platform() {
        let arms = builtin_arms();
        let labels: Vec<&String> = arms.iter().map(|(l, _)| l).collect();
        assert_eq!(
            labels,
            ["(cfg 없음)"],
            "{BUILTIN_SRC} 의 BUILTINS 는 cfg 없는 정의 하나여야 한다. 플랫폼마다 나누면 한 플랫폼의 컴파일로는 \
             다른 분기의 이름을 확인하지 못한다. 실행 파일 이름의 차이는 BuiltinSpec::bin_name 이 맡는다"
        );
        assert!(
            arms[0].1.len() >= 5,
            "BUILTINS 가 비었거나 너무 작다. 파싱 범위를 확인한다: {:?}",
            arms[0]
        );
    }

    /// 코드 목록을 별도로 수집한 매니페스트와 대조한다.
    #[test]
    fn the_manifests_on_disk_and_the_builtin_table_name_the_same_plugins() {
        let code = builtin_arms()
            .into_iter()
            .next()
            .expect("BUILTINS 정의 없음")
            .1;
        let disk = manifest_specs();
        assert!(
            disk.len() >= 5,
            "매니페스트를 {} 개밖에 못 찾았다 — 측정 실패다",
            disk.len()
        );
        assert_eq!(
            code,
            disk,
            "BUILTINS 와 디스크의 tasty-plugin.toml 이 어긋난다.\n  코드에만: {:?}\n  디스크에만: {:?}",
            code.difference(&disk).collect::<Vec<_>>(),
            disk.difference(&code).collect::<Vec<_>>(),
        );
    }

    #[test]
    fn both_docs_that_copy_the_builtin_table_still_match_it() {
        let code = builtin_arms()
            .into_iter()
            .next()
            .expect("BUILTINS 정의 없음")
            .1;
        let packaging = packaging_specs();
        assert_eq!(
            code,
            packaging,
            "{PACKAGING_DOC} 의 번들 목록 표가 BUILTINS 와 어긋난다.\n  코드에만: {:?}\n  문서에만: {:?}",
            code.difference(&packaging).collect::<Vec<_>>(),
            packaging.difference(&code).collect::<Vec<_>>(),
        );
        let catalog = catalog_ids();
        assert_eq!(
            ids(&code),
            catalog,
            "{CATALOG_DOC} 의 카탈로그가 BUILTINS 와 어긋난다.\n  코드에만: {:?}\n  문서에만: {:?}",
            ids(&code).difference(&catalog).collect::<Vec<_>>(),
            catalog.difference(&ids(&code)).collect::<Vec<_>>(),
        );
    }

    /// 같은 개수의 다른 플러그인으로 바꿔 집합 비교가 실패하는지 확인한다.
    #[test]
    fn swapping_one_entry_is_caught_although_the_count_is_unchanged() {
        let code = builtin_arms()
            .into_iter()
            .next()
            .expect("BUILTINS 정의 없음")
            .1;
        let mut mutated = code.clone();
        let victim = code.iter().next().expect("빈 집합").clone();
        mutated.remove(&victim);
        mutated.insert((format!("{ID_PREFIX}not-a-real-plugin"), victim.1.clone()));

        assert_eq!(
            mutated.len(),
            code.len(),
            "같은 개수를 유지하는 치환 입력이 아니다"
        );
        assert_ne!(
            mutated,
            manifest_specs(),
            "다른 플러그인으로 바꾼 목록을 같은 집합으로 판단했다"
        );
    }

    /// 같은 문서의 다른 표가 목록에 섞이지 않는지 확인한다.
    #[test]
    fn the_table_parser_reads_only_the_table_it_was_pointed_at() {
        let text = read(PACKAGING_DOC);
        let heads: Vec<&str> = text
            .lines()
            .map(str::trim)
            .filter(|l| l.starts_with('|') && !l.chars().all(|c| "|-: ".contains(c)))
            .collect();
        let other_heads = heads
            .iter()
            .filter(|l| **l != PACKAGING_TABLE_HEAD && l.matches('|').count() >= 3)
            .count();
        assert!(
            other_heads > 0,
            "{PACKAGING_DOC}에 비교할 다른 표가 없어 표 선택 범위를 확인할 수 없다"
        );
        let rows = table_rows(PACKAGING_DOC, PACKAGING_TABLE_HEAD);
        assert!(
            rows.iter().all(|r| r.len() == 2),
            "번들 목록 표는 2 열인데 다른 열 수의 행이 섞였다 — 표 경계를 넘어 읽었다: {rows:?}"
        );
        assert!(
            rows.iter().all(|r| backticked(&r[1])
                .first()
                .is_some_and(|t| t.starts_with(ID_PREFIX))),
            "표 밖의 행을 읽었다: {rows:?}"
        );
    }
}
