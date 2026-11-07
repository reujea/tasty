//! 문서·소스·워크플로를 읽어 정합성을 검사한다.
//! 본체를 빌드하지 않고 문서 변경도 검사할 수 있도록 추가 의존성을 두지 않는다(docs/dev-guide/guard-verification.md).
//! 별도 의존성 검사는 없으므로 Cargo.toml 변경을 검토할 때 이 조건을 확인한다.

// 이유: 테스트 본문의 의도적인 반환값 무시는 허용한다. cfg_attr(test)이므로 제품 코드의 lint는 유지한다.
#![cfg_attr(test, allow(clippy::let_underscore_must_use))]

/// ADR 인덱스의 행을 ADR 헤더에서 만든다 — 생성기와 가드가 같은 함수를 부른다.
pub mod adr_index;
/// ADR 파일 집합과 명시적인 빈 인덱스를 대조한다.
pub mod adr_inventory;
/// ADR 재번호의 판정(매핑 파일 · 인용 형태) — `adr-renumber` 가 부른다.
pub mod adr_renumber;

/// `#[cfg(...)]` 술어를 읽는다.
pub mod cfg_predicate;
/// 지문 계산 규칙 — `build.rs` 와 **원문 한 벌**을 공유한다(그쪽은 `include!`).
mod fingerprint_rule;
pub mod freshness;
pub mod manifest_text;

/// Cargo 매니페스트의 의존 선언을 읽는다.
pub mod cargo_manifest;

/// 아키텍처 문서의 계층 절과 크레이트의 워크스페이스 내부 의존을 읽는다.
pub mod crate_layers;

/// 소스를 텍스트로 읽는 가드들이 공유하는 마스킹·순회.
pub mod source_text;

/// 함수 본문과 `match` 팔을 구간으로 읽는다 — 팔 명부 가드들의 공용 판정기.
pub mod match_arms;

/// 소스가 부르는 크레이트 루트 경로(`crate::…` · `super::…` · 중괄호 import)를 편다.
pub mod crate_paths;

/// 모듈 선언을 따라 제품 코드에 포함되는 파일을 판정한다.
pub mod shipping_scope;

/// 락 poison 을 보고 없이 복구하는 자리를 집는다.
pub mod poison_recovery;

/// 공유 temp 아래 고정 이름 임시 경로를 집는다(docs/dev-guide/unit-test-isolation.md, 공유 임시 경로 격리).
pub mod temp_path;
pub mod temp_scratch;

/// env·cwd 를 직렬화 없이 만지는 테스트를 집는다(docs/dev-guide/unit-test-isolation.md, 프로세스 환경 격리).
pub mod env_isolation;

/// 워크플로의 `on:` 트리거를 구조로 읽는다 — 주석과 트리거 키를 가른다.
pub mod workflow_triggers;

/// 순회 결과에 하한을 적용한다.
pub mod floored_walk;

/// 저장소 외부 경로를 수정 대상으로 안내하지 않게 한다.
pub mod tracked_scope;

/// 사본 대조용 SHA-256.
pub mod sha256;

/// 시험의 자식 프로세스가 부모의 저장소 지정 Git 환경을 상속하지 않게 한다.
pub mod git_env;

/// 앱이 따르는 디자인 정본(tasty-page) 체크아웃을 찾고 기록한 커밋과 대조한다.
pub mod design_source;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// CARGO_MANIFEST_DIR에서 두 단계 올라가 저장소 루트를 찾는다.
/// Cargo.toml만으로는 이 크레이트 자체와 구분할 수 없어 외부 표지 파일도 확인한다.
/// 표지가 없으면 빈 디렉터리에서 검사를 계속하지 않고 panic한다.
pub fn repo_root() -> PathBuf {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .unwrap_or_else(|| panic!("CARGO_MANIFEST_DIR 에서 두 칸 올라갈 수 없다"))
        .to_path_buf();

    for marker in [
        "Cargo.toml",
        "CHANGELOG.md",
        "docs/adr/index.md",
        ".github/workflows",
    ] {
        assert!(
            root.join(marker).exists(),
            "저장소 루트로 선택한 {}에 표지 {marker}가 없다. 루트 경로를 확인한다.",
            root.display()
        );
    }
    root
}

/// `CACHEDIR.TAG` 의 서명 줄. 규격이 정한 값이라 도구가 달라도 같다 — cargo·ccache·
/// bazel 등이 모두 이 문자열로 캐시 디렉토리를 표시한다.
const CACHEDIR_SIGNATURE: &[u8] = b"Signature: 8a477f597d28d172789f06886806bc55";

/// CACHEDIR.TAG의 서명까지 확인해 빌드 캐시를 찾는다.
/// CARGO_TARGET_DIR로 이름을 바꾼 캐시도 제외하기 위해 이름 검사와 함께 사용한다.
/// 같은 이름의 일반 파일이 소스 디렉터리를 제외하지 않도록 서명을 확인한다.
/// Git·node_modules·vendor처럼 캐시 표식이 없는 대상은 소비자가 별도로 다룬다.
pub fn is_build_cache_dir(dir: &Path) -> bool {
    let Ok(head) = std::fs::read(dir.join("CACHEDIR.TAG")) else {
        return false;
    };
    head.starts_with(CACHEDIR_SIGNATURE)
}

/// node_modules는 도구가 정한 의존성 디렉터리명이라 이름으로 제외한다.
/// 외부 패키지 파일이 검사 대상 수를 늘려 순회 누락을 가리지 않게 한다.
pub fn is_dependency_tree_dir(dir: &Path) -> bool {
    dir.file_name().is_some_and(|n| n == "node_modules")
}

/// 텍스트 검사에서 제외할 바이너리·산출물 확장자인지 확인한다.
/// 가드의 목적에 따른 추가 제외 형식은 각 소비자가 관리한다.
pub fn is_binary_artifact_ext(ext: &str) -> bool {
    BINARY_ARTIFACT_EXTS.contains(&ext.to_ascii_lowercase().as_str())
}

/// [`is_binary_artifact_ext`] 의 denylist. 텍스트로 열 수 없는 형식만 — 여기 없는
/// 확장자는 스캔 대상이다(denylist 전수).
pub const BINARY_ARTIFACT_EXTS: &[&str] = &[
    "png", "jpg", "jpeg", "gif", "bmp", "ico", "icns", "pdf", "ttf", "otf", "woff", "woff2", "zip",
    "gz", "xz", "tar", "wasm", "bin", "so", "dylib", "dll", "exe", "sig",
];

/// 예외 목록이 가리키는 경로 중 존재하지 않거나 파일 종류가 다른 항목을 반환한다.
/// 후행 /는 디렉터리, 나머지는 파일로 확인한다. 예외가 필요한지까지 판단하지는 않는다.
pub fn missing_referents<'a>(
    root: &Path,
    cited: impl IntoIterator<Item = &'a str>,
) -> Vec<&'a str> {
    cited
        .into_iter()
        .filter(|rel| {
            let full = root.join(rel.trim_end_matches('/'));
            if rel.ends_with('/') {
                !full.is_dir()
            } else {
                !full.is_file()
            }
        })
        .collect()
}

// METHOD_TABLE의 권한을 텍스트로 읽는다. 실제 런타임 표와의 일치는 본체의
// tests/method_table_readings_agree.rs가 확인해 여기의 추가 의존성을 피한다.

/// 줄 주석을 지운다. 문자열 리터럴 안의 `//` 는 건드리지 않는다.
pub fn strip_line_comments(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    for line in src.lines() {
        let mut in_str = false;
        let mut cut = line.len();
        let b = line.as_bytes();
        let mut i = 0;
        while i < b.len() {
            match b[i] {
                b'\\' if in_str => i += 1,
                b'"' => in_str = !in_str,
                b'/' if !in_str && i + 1 < b.len() && b[i + 1] == b'/' => {
                    cut = i;
                    break;
                }
                _ => {}
            }
            i += 1;
        }
        out.push_str(&line[..cut]);
        out.push(' ');
    }
    out
}

/// 지원하는 생성자와 plugin 호출 허용 여부. 실제 소스에서 추출한 생성자 집합과 대조한다.
pub const KNOWN_CTORS: &[(&str, bool)] = &[
    ("plugin", true),
    // plugin_only도 권한 목록의 의미는 plugin과 같다.
    ("plugin_only", true),
    ("local_only", false),
];

/// METHOD_TABLE을 메서드별 필요 권한으로 읽는다. None은 plugin 호출 불가다.
/// 여러 줄 항목과 후행 쉼표를 허용하고 모르는 생성자는 실패로 처리한다.
pub fn method_table(src: &str) -> BTreeMap<String, Option<Vec<String>>> {
    let start = src
        .find("pub const METHOD_TABLE")
        .expect("METHOD_TABLE 을 못 찾았다");
    let end = src[start..]
        .find("\npub const DEBUG_METHODS")
        .expect("METHOD_TABLE 의 끝을 못 찾았다");
    let flat = strip_line_comments(&src[start..start + end]);
    let b = flat.as_bytes();
    let mut out = BTreeMap::new();
    let mut unknown: Vec<String> = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if b[i] != b'"' {
            i += 1;
            continue;
        }
        let Some(close) = flat[i + 1..].find('"') else {
            break;
        };
        let name = &flat[i + 1..i + 1 + close];
        let after = &flat[i + 1 + close + 1..];
        let trimmed = after.trim_start();
        if !name.is_empty()
            && name
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '.' || c == '_')
            && trimmed.starts_with(',')
        {
            let tail = trimmed[1..].trim_start();
            let ctor: String = tail
                .chars()
                .take_while(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == '_')
                .collect();
            if tail[ctor.len()..].starts_with('(') {
                let Some((_, plugin_callable)) =
                    KNOWN_CTORS.iter().find(|(n, _)| *n == ctor.as_str())
                else {
                    unknown.push(format!("{name} → {ctor}(…)"));
                    i += 1 + close + 1;
                    continue;
                };
                if *plugin_callable {
                    let rest = &tail[ctor.len() + 1..];
                    let open = rest.find('[').expect("plugin 계열은 &[..] 형태다");
                    let close2 = rest[open..].find(']').expect("&[..] 가 안 닫혔다");
                    let inner = &rest[open + 1..open + close2];
                    let vs: Vec<String> = inner
                        .split(',')
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect();
                    out.insert(name.to_string(), Some(vs));
                } else {
                    out.insert(name.to_string(), None);
                }
            }
        }
        i += 1 + close + 1;
    }
    // 빈 결과를 정상적인 권한 목록으로 반환하지 않는다.
    assert!(
        !out.is_empty(),
        "METHOD_TABLE에서 항목이 한 건도 나오지 않았다. 표가 비었거나 지원하지 않는 문법인지 확인한다."
    );
    assert!(
        unknown.is_empty(),
        "METHOD_TABLE에 모르는 생성자가 있다. KNOWN_CTORS에 해석을 추가한다:\n  {}",
        unknown.join("\n  ")
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 추가 의존성 없이 시험용 캐시 디렉터리를 만든다.
    fn temp_dir_named(suffix: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("tasty-cachedir-{}-{suffix}", std::process::id()));
        // 이전 실행의 임시 경로를 정리한다. 없으면 무시한다.
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("임시 디렉토리 생성");
        dir
    }

    #[test]
    fn missing_referents_reports_only_what_is_absent() {
        let root = repo_root();
        let found = missing_referents(
            &root,
            [
                "Cargo.toml",
                "src/__no_such_file__.rs",
                "crates/",
                "crates/__no_such_dir__/",
            ],
        );
        assert_eq!(
            found,
            vec!["src/__no_such_file__.rs", "crates/__no_such_dir__/"]
        );
    }

    #[test]
    fn the_trailing_slash_decides_which_kind_is_required() {
        let root = repo_root();
        assert_eq!(
            missing_referents(&root, ["Cargo.toml/"]),
            vec!["Cargo.toml/"]
        );
        assert_eq!(missing_referents(&root, ["crates"]), vec!["crates"]);
        assert!(missing_referents(&root, ["Cargo.toml", "crates/"]).is_empty());
    }

    #[test]
    fn binary_exts_are_denied_and_text_is_scanned() {
        assert!(is_binary_artifact_ext("png"));
        assert!(is_binary_artifact_ext("PNG"), "대소문자 무관이어야 한다");
        assert!(is_binary_artifact_ext("woff2"));
        assert!(!is_binary_artifact_ext("rs"), "소스는 스캔 대상이다");
        assert!(!is_binary_artifact_ext("md"));
        assert!(
            !is_binary_artifact_ext(""),
            "확장자 없음은 텍스트로 본다(denylist 전수)"
        );
        // 소비자가 위에 얹는 형식은 정본에 없다 — 그건 각 가드의 모수다.
        assert!(!is_binary_artifact_ext("lock"));
        assert!(!is_binary_artifact_ext("svg"));
    }

    #[test]
    fn a_directory_with_the_signature_is_a_build_cache() {
        let dir = temp_dir_named("hit");
        std::fs::write(
            dir.join("CACHEDIR.TAG"),
            "Signature: 8a477f597d28d172789f06886806bc55\n# cargo\n",
        )
        .expect("표식 쓰기");
        assert!(is_build_cache_dir(&dir));
        // 임시 파일 정리 오류가 원래 시험 결과를 가리지 않게 한다.
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_file_with_the_same_name_but_wrong_content_is_not_enough() {
        let dir = temp_dir_named("miss");
        std::fs::write(dir.join("CACHEDIR.TAG"), "메모\n").expect("가짜 표식 쓰기");
        assert!(!is_build_cache_dir(&dir));

        std::fs::remove_file(dir.join("CACHEDIR.TAG")).expect("표식 제거");
        assert!(!is_build_cache_dir(&dir), "표식이 없으면 캐시가 아니다");
        // 임시 파일 정리 오류가 원래 시험 결과를 가리지 않게 한다.
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 다른 CARGO_TARGET_DIR를 쓸 수 있으므로 target 표식이 있을 때만 확인한다.
    #[test]
    fn the_repo_root_is_not_a_cache_but_a_build_dir_is() {
        let root = repo_root();
        assert!(!is_build_cache_dir(&root));
        assert!(!is_build_cache_dir(&root.join("crates")));

        let target = root.join("target");
        if target.join("CACHEDIR.TAG").exists() {
            assert!(is_build_cache_dir(&target));
        }
    }

    #[test]
    fn the_root_is_the_repo_not_this_crate() {
        let root = repo_root();
        assert!(root.join("crates/tasty-doc-guards/Cargo.toml").exists());
        assert!(!root.ends_with("tasty-doc-guards"));
    }

    #[test]
    fn this_crate_dir_would_not_pass_as_the_root() {
        let here = Path::new(env!("CARGO_MANIFEST_DIR"));
        assert!(here.join("Cargo.toml").exists(), "대조: Cargo.toml 은 있다");
        assert!(!here.join("CHANGELOG.md").exists());
        assert!(!here.join("docs/adr/index.md").exists());
    }
}
