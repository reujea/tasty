//! 파일·디렉터리 순회와 직접 엔트리 열거에 하한을 적용한다.
//! 호출자는 하한과 실측 근거를 제공한다. 빈 결과를 검사 통과로 오해하지 않게 한다.

mod direct_entries;

pub use direct_entries::entries_with_floor;

use std::path::{Path, PathBuf};

/// 측정한 트리의 출처. 측정하지 않은 값과 커밋으로 다시 찾을 수 없는 측정을 구분한다.
/// 오래된 좌표의 커밋이 사라졌다면 해시만 바꾸지 말고 현재 트리에서 다시 측정한다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CountedOn {
    /// 통합 브랜치(develop)에서 도달 가능한 측정 커밋. 해시 뒤에 근거 설명을 덧붙일 수 있다.
    Tree(&'static str),
    /// 작업 브랜치에서 측정한 값. 해당 커밋이 develop의 조상이 아니므로 통합 후 다시 측정한다.
    LaneTip(&'static str),
    /// 시험이 직접 만든 임시 트리를 측정했다. 저장소 커밋은 해당하지 않는다.
    SyntheticTree,
    /// 측정 출처를 확인할 수 없으며 그 이유를 기록한다. 임의의 날짜나 값을 넣지 않는다.
    Unmeasured(&'static str),
}

impl CountedOn {
    /// 측정 출처를 확인하지 못한 기존 선언의 공통 사유.
    pub const NEVER_COUNTED: CountedOn = CountedOn::Unmeasured(
        "측정한 커밋과 방법을 확인할 기록이 선언이나 커밋 설명에 남아 있지 않다.",
    );

    fn validate(&self) -> Result<(), String> {
        let hash_ok = |h: &str| {
            let head: String = h.chars().take_while(|c| c.is_ascii_hexdigit()).collect();
            head.len() >= 7
        };
        match self {
            CountedOn::Tree(h) | CountedOn::LaneTip(h) => {
                if hash_ok(h) {
                    Ok(())
                } else {
                    Err(format!(
                        "`counted_on`에 커밋 해시가 없다: {h:?}. 측정한 트리를 구분할 수 있도록 7자 이상의 해시로 시작해야 한다."
                    ))
                }
            }
            CountedOn::SyntheticTree => Ok(()),
            CountedOn::Unmeasured(why) => {
                if why.split_whitespace().count() >= 5 {
                    Ok(())
                } else {
                    Err(format!(
                        "`counted_on`의 미측정 사유가 너무 짧다({} 낱말). 측정 출처를 확인할 수 없는 이유를 구체적으로 적는다.",
                        why.split_whitespace().count()
                    ))
                }
            }
        }
    }
}

/// 순회 하한과 그 근거. 실측값·측정일·트리·허용 여유를 정한 이유를 함께 기록한다.
/// 파일 분할이나 크레이트 이동처럼 예상되는 변화에 맞춰 여유를 정한다.
pub struct Floor {
    /// 이 개수보다 적게 수집하면 실패한다. 하한으로 0은 허용하지 않는다.
    pub min: usize,
    /// 마지막으로 실제로 센 값.
    pub measured: usize,
    /// 그 값을 잰 날 (`YYYY-MM-DD`).
    pub measured_on: &'static str,
    /// 측정한 트리.
    pub counted_on: CountedOn,
    /// 실제 수보다 낮은 하한을 택한 이유와 허용한 감소 범위.
    pub why_this_gap: &'static str,
}

/// 여러 가드가 공유하는 검사 대상을 한 번 잰 값.
/// 같은 대상의 수·날짜·측정법은 공유하고, 소비자마다 다른 하한과 여유는 Floor에 둔다.
/// `measured`는 `counted_on` 트리에서 잰 값이며 현재 트리와 같을 필요는 없다. 실제 개수는
/// 소비자의 순회가 실행할 때마다 세고 하한과 비교한다. 파일을 더하거나 옮겨도 갱신하지 않는다.
pub struct Population {
    /// 마지막으로 실제로 센 값.
    pub measured: usize,
    /// 그 값을 잰 날 (`YYYY-MM-DD`).
    pub measured_on: &'static str,
    /// 측정한 트리. 같은 날 측정했어도 트리가 다르면 수가 다를 수 있다.
    pub counted_on: CountedOn,
    /// 포함·제외 범위와 수를 세는 방법.
    pub how: &'static str,
}

/// 여러 가드가 함께 사용하는 저장소 대상을 잰 값. 하한을 정할 때의 근거이며 현재 개수가 아니다.
/// 합성 트리의 하한은 그 픽스처에서 따로 정의하며 여기에 합치지 않는다.
pub mod populations {
    use super::Population;

    /// `src/` 아래 `.rs` 전부.
    pub const SRC_RS: Population = Population {
        measured: 951,
        measured_on: "2026-10-07",
        counted_on: super::CountedOn::Tree(
            "e66eabbe8 — git ls-tree -r --name-only로 센 src 아래 Rust 파일 951개.",
        ),
        how: "src/ 아래에서 .rs로 끝나는 파일을 깊이 제한 없이 센다. 캐시는 제외한다. git ls-tree의 같은 경로 집합으로 대조하며 측정 당시 미추적 파일은 없었다.",
    };

    /// 루트 패키지의 통합 테스트 타깃 — `tests/` 바로 아래 한 겹.
    pub const ROOT_TEST_TARGETS: Population = Population {
        measured: 49,
        measured_on: "2026-10-09",
        counted_on: super::CountedOn::Tree(
            "5a253f345 — 루트 tests/ 바로 아래 통합 테스트 타깃 49개를 확인했다. \
             48 -> 49 이고 더해진 하나는 `tests/plugin_bump_fixup.rs` 다. \
             이전 회차: 400820ee3 — 48개.",
        ),
        how: "`tests/` 바로 아래 `.rs` — **한 겹만**이다. 더 깊은 것은 타깃이 아니라 그 \
              타깃의 모듈이라 `cargo test` 가 따로 안 돌린다. 재는 법: 경로가 `tests/` 로 \
              시작하고 `/` 가 정확히 하나이며 `.rs` 로 끝나는 줄.",
    };

    /// 크레이트들의 통합 테스트 타깃 — `crates/<크레이트>/tests/` 바로 아래 한 겹.
    pub const CRATE_TEST_TARGETS: Population = Population {
        measured: 147,
        measured_on: "2026-10-10",
        counted_on: super::CountedOn::Tree(
            "a6099a66e 작업 트리 — crates/<crate>/tests/<file>.rs 146개에 tasty-doc-guards의 \
             site_vendor_manifest_matches 통합 테스트를 더해 147개를 확인했다.",
        ),
        how: "crates/<크레이트>/tests/<파일>.rs 형태의 네 구성요소 경로만 센다. 세 번째 요소가 tests이며 파일명이 .rs로 끝나야 한다.",
    };

    /// Git이 추적하는 `docs/` 아래 Markdown 파일 수.
    pub const DOCS_MD: Population = Population {
        measured: 246,
        measured_on: "2026-10-06",
        counted_on: super::CountedOn::Tree(
            "e1edd7408 — 이 트리에서 Git이 추적하는 docs/ 아래 .md 파일 246개를 다시 확인했다.",
        ),
        how: "git ls-files에서 docs/로 시작하고 .md로 끝나는 경로를 센다. 깊이는 제한하지 않는다.",
    };
}

/// 순회 중 제외할 디렉터리를 정한다. Cargo 캐시는 이름 대신 CACHEDIR.TAG 표식도 확인한다.
pub enum Descend {
    /// 제외 없이 내려간다. 호출자가 순회 루트를 제한해야 한다.
    Everything,
    /// 빌드 캐시와 패키지 의존 트리를 제외한다. 각각 캐시 표식과 도구의 디렉터리명을 사용한다.
    SkipBuildCaches,
    /// 캐시·의존 트리와 점으로 시작하는 디렉터리를 제외한다.
    /// 로컬 작업 문서가 검사에 섞이지 않도록 저장소 전체 순회에 사용한다.
    /// .github 아래도 봐야 한다면 그 디렉터리를 별도 루트로 넘긴다. 루트 자체는 제외하지 않는다.
    SkipBuildCachesAndDotDirs,
}

/// 파일을 읽을 절대 경로와 비교에 사용할 저장소 상대 경로.
/// 상대 경로 구분자는 Windows에서도 /로 정규화한다.
#[derive(Debug)]
pub struct Walked {
    /// 파일을 다시 열 때 쓴다.
    pub path: PathBuf,
    /// 비교에 쓴다. 구분자는 언제나 `/`.
    pub rel: String,
}

impl Floor {
    /// 순회 전에 하한 선언의 형식과 값 관계를 확인한다.
    fn validate(&self) -> Result<(), String> {
        if self.min == 0 {
            return Err(
                "하한이 0 이다. 실제 검사 대상을 세고 허용할 감소 범위에 맞는 양수 하한을 정한다."
                    .to_string(),
            );
        }
        if self.min > self.measured {
            return Err(format!(
                "하한 {}이 마지막 실측 {}보다 크다. 다시 측정해 measured와 measured_on을 갱신하거나, 하한이 잘못됐다면 이유와 함께 수정한다.",
                self.min, self.measured
            ));
        }
        if self.measured_on.len() != 10 || self.measured_on.matches('-').count() != 2 {
            return Err(format!(
                "`measured_on`이 YYYY-MM-DD 형식이 아니다: {:?}. 실제 측정일을 기록한다.",
                self.measured_on
            ));
        }
        self.counted_on.validate()?;
        if self.why_this_gap.split_whitespace().count() < 10 {
            return Err(format!(
                "`why_this_gap`이 너무 짧다({} 낱말). 검사 대상이 언제 늘거나 줄어드는지와 그 변화에 비추어 현재 하한을 정한 이유를 적는다.",
                self.why_this_gap.split_whitespace().count()
            ));
        }
        Ok(())
    }
}

/// 디렉토리를 만났을 때 무엇을 할지.
pub enum Pick {
    /// 안 모으고 그 아래로 계속 내려간다.
    Skip,
    /// 모으고 그 아래로도 계속 내려간다.
    Take,
    /// 수집한 뒤 하위 디렉터리는 순회하지 않는다.
    TakeAndStop,
}

/// root 아래에서 pick이 고른 디렉터리를 수집한다.
/// 하한은 수집 수가 아닌 방문 수에 적용한다. 조건에 맞는 디렉터리가 없는 것은 정상일 수 있다.
#[must_use = "순회 하한 검사 결과를 처리해야 한다."]
pub fn walk_dirs_with_floor(
    root: &Path,
    rel_base: &Path,
    floor: &Floor,
    pick: &dyn Fn(&Walked) -> Pick,
) -> Result<Vec<Walked>, String> {
    floor.validate().map_err(|why| {
        format!(
            "순회 하한 선언이 앞뒤가 안 맞는다 ({}): {why}",
            root.display()
        )
    })?;

    let mut out = Vec::new();
    let mut walked = 0usize;
    collect_dirs(root, rel_base, pick, &mut out, &mut walked);
    out.sort_by(|a, b| a.rel.cmp(&b.rel));

    if walked < floor.min {
        return Err(format!(
            "{} 순회가 디렉토리를 {} 개만 훑었다(하한 {}, 수집 {}).\n마지막 측정: {} / {}개. 하한 근거: {}\n하한을 내려서 통과시키지 마라. 먼저 루트·심볼릭 링크·read_dir 오류를 확인한다. 실제 대상이 줄었다면 measured·measured_on·why_this_gap을 함께 갱신한다.",
            root.display(),
            walked,
            floor.min,
            out.len(),
            floor.measured_on,
            floor.measured,
            floor.why_this_gap,
        ));
    }
    Ok(out)
}

fn collect_dirs(
    dir: &Path,
    rel_base: &Path,
    pick: &dyn Fn(&Walked) -> Pick,
    out: &mut Vec<Walked>,
    walked: &mut usize,
) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        // 저장소 밖으로 순회하지 않도록 심볼릭 링크를 제외한다.
        if kind.is_symlink() || !kind.is_dir() {
            continue;
        }
        let path = entry.path();
        *walked += 1;
        let found = Walked {
            rel: normalized_rel(&path, rel_base),
            path,
        };
        match pick(&found) {
            Pick::Skip => collect_dirs(&found.path, rel_base, pick, out, walked),
            Pick::Take => {
                let next = found.path.clone();
                out.push(found);
                collect_dirs(&next, rel_base, pick, out, walked);
            }
            Pick::TakeAndStop => out.push(found),
        }
    }
}

/// root 아래에서 keep이 고른 파일을 수집하며, 수집 수가 하한보다 적으면 오류를 반환한다.
///
/// ```ignore
/// let files = walk_with_floor(&root.join("src"), &root, &floor, Descend::Everything, &|w| {
///     w.rel.ends_with(".rs")
/// })
/// .unwrap_or_else(|why| panic!("{why}"));
/// ```
#[must_use = "순회 하한 검사 결과를 처리해야 한다."]
pub fn walk_with_floor(
    root: &Path,
    rel_base: &Path,
    floor: &Floor,
    descend: Descend,
    keep: &dyn Fn(&Walked) -> bool,
) -> Result<Vec<Walked>, String> {
    floor.validate().map_err(|why| {
        format!(
            "순회 하한 선언이 앞뒤가 안 맞는다 ({}): {why}",
            root.display()
        )
    })?;

    let mut out = Vec::new();
    collect(root, rel_base, &descend, keep, &mut out);
    out.sort_by(|a, b| a.rel.cmp(&b.rel));

    if out.len() < floor.min {
        return Err(format!(
            "{} 순회가 {} 개만 모았다(하한 {}).\n마지막 측정: {} / {}개. 하한 근거: {}\n하한을 내려서 통과시키지 마라. 먼저 루트·가지치기·read_dir 오류를 확인한다. 실제 대상이 줄었다면 measured·measured_on·why_this_gap을 함께 갱신한다.",
            root.display(),
            out.len(),
            floor.min,
            floor.measured_on,
            floor.measured,
            floor.why_this_gap,
        ));
    }
    Ok(out)
}

fn collect(
    dir: &Path,
    rel_base: &Path,
    descend: &Descend,
    keep: &dyn Fn(&Walked) -> bool,
    out: &mut Vec<Walked>,
) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        // 링크를 따라가면 저장소 밖의 로컬 파일까지 검사에 포함될 수 있다.
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_symlink() {
            continue;
        }
        let path = entry.path();
        if kind.is_dir() {
            if matches!(
                descend,
                Descend::SkipBuildCaches | Descend::SkipBuildCachesAndDotDirs
            ) && (crate::is_build_cache_dir(&path) || crate::is_dependency_tree_dir(&path))
            {
                continue;
            }
            if matches!(descend, Descend::SkipBuildCachesAndDotDirs)
                && path
                    .file_name()
                    .is_some_and(|n| n.to_string_lossy().starts_with('.'))
            {
                continue;
            }
            collect(&path, rel_base, descend, keep, out);
        } else {
            let found = Walked {
                rel: normalized_rel(&path, rel_base),
                path,
            };
            if keep(&found) {
                out.push(found);
            }
        }
    }
}

/// rel_base 기준 상대 경로를 만들고 구분자를 /로 정규화한다.
/// Windows의 구분자 변환은 Windows 검사에서, 공통 함수 사용 여부는 별도 소스 검사에서 확인한다.
pub fn normalized_rel(path: &Path, rel_base: &Path) -> String {
    path.strip_prefix(rel_base)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Tree(PathBuf);

    impl Tree {
        fn new(files: &[&str], cache_dirs: &[&str]) -> Self {
            // 시간 해상도에 의존하지 않고 병렬 픽스처마다 다른 경로를 만든다.
            static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
            let base = std::env::temp_dir().join(format!(
                "tasty-floored-walk-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            ));
            for rel in files {
                let path = base.join(rel);
                std::fs::create_dir_all(path.parent().expect("파일에 부모가 없다")).unwrap();
                std::fs::write(&path, b"x").unwrap();
            }
            for rel in cache_dirs {
                let dir = base.join(rel);
                std::fs::create_dir_all(&dir).unwrap();
                std::fs::write(
                    dir.join("CACHEDIR.TAG"),
                    b"Signature: 8a477f597d28d172789f06886806bc55",
                )
                .unwrap();
            }
            Self(base)
        }
    }

    impl Drop for Tree {
        fn drop(&mut self) {
            // 임시 파일 정리 실패가 원래 시험 결과를 가리지 않게 한다.
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn floor(min: usize, measured: usize) -> Floor {
        Floor {
            min,
            measured,
            measured_on: "2026-09-06",
            counted_on: CountedOn::SyntheticTree,
            why_this_gap: "시험용 고정값이다 — 이 모수는 테스트 안에서만 살고 아무것도 \
                           따라가지 않으므로 간격에 뜻이 없다",
        }
    }

    fn all(_: &Walked) -> bool {
        true
    }

    #[test]
    fn a_walk_that_meets_its_floor_returns_what_it_found() {
        let t = Tree::new(&["a.rs", "sub/b.rs", "sub/deep/c.rs"], &[]);
        let got = walk_with_floor(&t.0, &t.0, &floor(3, 3), Descend::Everything, &all)
            .expect("셋을 모았으면 통과해야 한다");
        assert_eq!(got.len(), 3, "재귀가 죽으면 얕은 자리만 모은다");
    }

    #[test]
    fn a_dead_walk_returns_why_instead_of_an_empty_list() {
        let t = Tree::new(&["a.rs"], &[]);
        let why = walk_with_floor(&t.0, &t.0, &floor(3, 10), Descend::Everything, &all)
            .expect_err("하한 미달인데 통과했다");
        assert!(
            why.contains("1 개만 모았다") && why.contains("하한 3"),
            "실패문이 실제로 몇 개를 모았는지 말하지 않는다: {why}"
        );
        assert!(
            why.contains("하한을 내려서 통과시키지 마라"),
            "금지가 빠졌다 — 이 문장이 없으면 다음 사람의 최소 수선은 하한을 내리는 것이다: {why}"
        );
    }

    #[test]
    fn the_keep_predicate_actually_filters() {
        let t = Tree::new(&["a.rs", "b.md", "c.md"], &[]);
        let got = walk_with_floor(&t.0, &t.0, &floor(1, 1), Descend::Everything, &|w| {
            w.rel.ends_with(".md")
        })
        .expect("둘을 모았으면 통과해야 한다");
        assert_eq!(
            got.len(),
            2,
            "`keep` 을 안 보고 전부 모으거나 아무것도 안 모은다"
        );
    }

    #[test]
    fn build_caches_are_skipped_only_when_asked() {
        let t = Tree::new(&["keep.rs", "cache/inside.rs"], &["cache"]);
        let skipped = walk_with_floor(&t.0, &t.0, &floor(1, 1), Descend::SkipBuildCaches, &|w| {
            w.rel.ends_with(".rs")
        })
        .expect("바깥 파일 하나는 남는다");
        assert_eq!(skipped.len(), 1, "빌드 캐시 안을 들여다봤다");

        let everything = walk_with_floor(&t.0, &t.0, &floor(1, 1), Descend::Everything, &|w| {
            w.rel.ends_with(".rs")
        })
        .expect("둘 다 모인다");
        assert_eq!(
            everything.len(),
            2,
            "`Everything`인데도 하위 디렉터리를 건너뛰었다."
        );
    }

    #[test]
    fn dependency_trees_are_skipped_only_when_asked() {
        let t = Tree::new(&["keep.rs", "node_modules/dep/inside.rs"], &[]);
        let skipped = walk_with_floor(&t.0, &t.0, &floor(1, 1), Descend::SkipBuildCaches, &|w| {
            w.rel.ends_with(".rs")
        })
        .expect("바깥 파일 하나는 남는다");
        assert_eq!(skipped.len(), 1, "`node_modules` 안을 들여다봤다");

        let everything = walk_with_floor(&t.0, &t.0, &floor(1, 1), Descend::Everything, &|w| {
            w.rel.ends_with(".rs")
        })
        .expect("둘 다 모인다");
        assert_eq!(
            everything.len(),
            2,
            "`Everything`인데도 하위 디렉터리를 건너뛰었다."
        );
    }

    /// 디렉터리 열거 순서에 우연히 기대지 않도록 여러 파일을 역순으로 만든다.
    #[test]
    fn the_walk_returns_a_sorted_list() {
        let names: Vec<String> = (0..20).rev().map(|i| format!("f{i:02}.rs")).collect();
        let refs: Vec<&str> = names.iter().map(String::as_str).collect();
        let t = Tree::new(&refs, &[]);
        let got = walk_with_floor(&t.0, &t.0, &floor(20, 20), Descend::Everything, &all)
            .expect("스무 개를 모았으면 통과해야 한다");
        assert!(
            got.windows(2).all(|p| p[0].rel <= p[1].rel),
            "순회 결과가 정렬돼 있지 않다 — 순서를 파일시스템에 맡기면 같은 결함이 완주마다 \
             다른 순서로 보고된다: {:?}",
            got.iter().map(|w| &w.rel).collect::<Vec<_>>()
        );
    }

    /// Windows의 symlink 생성은 추가 권한이 필요해 이 시험은 Unix에서만 실행한다.
    #[cfg(unix)]
    #[test]
    fn symlinks_are_not_followed_out_of_the_tree() {
        let outside = Tree::new(&["beyond.rs"], &[]);
        let inside = Tree::new(&["here.rs"], &[]);
        std::os::unix::fs::symlink(&outside.0, inside.0.join("link")).unwrap();

        let got = walk_with_floor(
            &inside.0,
            &inside.0,
            &floor(1, 1),
            Descend::Everything,
            &all,
        )
        .expect("트리 안의 파일 하나는 모인다");
        let rels: Vec<&str> = got.iter().map(|w| w.rel.as_str()).collect();
        assert_eq!(
            rels,
            vec!["here.rs"],
            "심볼릭 링크를 따라 저장소 밖의 파일을 수집했다."
        );

        let plain = Tree::new(&["here.rs", "sub/beyond.rs"], &[]);
        let got = walk_with_floor(&plain.0, &plain.0, &floor(1, 1), Descend::Everything, &all)
            .expect("둘 다 모인다");
        assert_eq!(got.len(), 2, "평범한 하위 디렉토리까지 건너뛴다");
    }

    #[test]
    fn the_directory_walk_floors_what_it_visited_not_what_it_kept() {
        let t = Tree::new(&["a/x.rs", "b/y.rs", "c/z.rs"], &[]);
        let got = walk_dirs_with_floor(&t.0, &t.0, &floor(3, 3), &|_| Pick::Skip)
            .expect("모은 것이 0 이어도 훑은 것이 하한을 넘으면 통과해야 한다");
        assert!(got.is_empty(), "아무것도 안 골랐는데 모았다");

        let thin = Tree::new(&["a/x.rs"], &[]);
        let why = walk_dirs_with_floor(&thin.0, &thin.0, &floor(3, 3), &|_| Pick::Skip)
            .expect_err("훑은 것이 하한에 못 미치는데 통과했다");
        assert!(
            why.contains("디렉토리를 1 개만 훑었다"),
            "실패문이 훑은 수를 안 말한다: {why}"
        );
    }

    #[test]
    fn take_and_stop_prunes_while_take_keeps_descending() {
        let t = Tree::new(&["hit/deep/x.rs", "plain/y.rs"], &[]);
        let stop = walk_dirs_with_floor(&t.0, &t.0, &floor(1, 4), &|w| {
            if w.rel.ends_with("hit") {
                Pick::TakeAndStop
            } else {
                Pick::Skip
            }
        })
        .expect("셋 이상 훑는다");
        assert_eq!(
            stop.iter().map(|w| w.rel.as_str()).collect::<Vec<_>>(),
            vec!["hit"],
            "`TakeAndStop` 인데 그 아래로 내려갔다"
        );

        let go = walk_dirs_with_floor(&t.0, &t.0, &floor(1, 4), &|w| {
            if w.rel.contains("hit") {
                Pick::Take
            } else {
                Pick::Skip
            }
        })
        .expect("셋 이상 훑는다");
        assert_eq!(
            go.iter().map(|w| w.rel.as_str()).collect::<Vec<_>>(),
            vec!["hit", "hit/deep"],
            "`Take`인데도 하위 디렉터리를 순회하지 않았다."
        );
    }

    #[test]
    fn a_floor_of_zero_is_refused_before_the_walk_runs() {
        let t = Tree::new(&[], &[]);
        let why = walk_with_floor(&t.0, &t.0, &floor(0, 10), Descend::Everything, &all)
            .expect_err("하한 0 을 받아들였다");
        assert!(
            why.contains("하한이 0 이다"),
            "0 을 다른 이유로 거부했다: {why}"
        );
    }

    #[test]
    fn a_floor_above_its_own_measurement_is_refused() {
        let t = Tree::new(&["a.rs"], &[]);
        let why = walk_with_floor(&t.0, &t.0, &floor(11, 10), Descend::Everything, &all)
            .expect_err("실측보다 높은 하한을 받아들였다");
        assert!(why.contains("보다 크다"), "다른 이유로 거부했다: {why}");
    }

    #[test]
    fn an_undated_measurement_is_refused() {
        let t = Tree::new(&["a.rs"], &[]);
        let bad = Floor {
            measured_on: "얼마 전",
            counted_on: CountedOn::SyntheticTree,
            ..floor(1, 10)
        };
        let why = walk_with_floor(&t.0, &t.0, &bad, Descend::Everything, &all)
            .expect_err("날짜 없는 실측을 받아들였다");
        assert!(why.contains("YYYY-MM-DD"), "다른 이유로 거부했다: {why}");
    }

    #[test]
    fn a_gap_without_a_reason_is_refused() {
        let t = Tree::new(&["a.rs"], &[]);
        let bad = Floor {
            why_this_gap: "적당히",
            ..floor(1, 10)
        };
        let why = walk_with_floor(&t.0, &t.0, &bad, Descend::Everything, &all)
            .expect_err("사유 없는 간격을 받아들였다");
        assert!(why.contains("너무 짧다"), "다른 이유로 거부했다: {why}");
    }
}
