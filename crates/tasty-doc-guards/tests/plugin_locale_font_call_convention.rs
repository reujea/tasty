//! 네 egui 플러그인의 소스에 폰트 환경변수 이름과 공용 검증 함수 호출이 있는지, 언어팩 글꼴을
//! 설치하는 곳이 호스트가 넘긴 글꼴 번호를 읽어 넘기는지 확인한다.
//! 동일한 폰트를 사용하도록 하는 규약이며 실제 화면의 글리프 렌더링까지 검사하지는 않는다.
//! 호스트의 환경변수 정의는 src/boot/locale.rs의 font_env_path에 있다.

use std::path::Path;

use tasty_doc_guards::floored_walk::{CountedOn, Descend, Floor, normalized_rel, walk_with_floor};
use tasty_doc_guards::repo_root;

/// 플러그인과 폰트 설치 파일. 파일을 옮기면 이 목록도 갱신한다.
const UI_PLUGINS: &[(&str, &str)] = &[
    ("tasty-plugin-clipboard-viewer", "src/main.rs"),
    ("tasty-plugin-git-viewer", "src/main.rs"),
    ("tasty-plugin-image", "src/main.rs"),
    ("tasty-plugin-markdown", "src/popup.rs"),
];

/// host 가 resolve 한 폰트 경로를 자식에 물려주는 env 이름. 넷이 이 철자를 읽어야 한다.
const LOCALE_FONT_ENV: &str = "TASTY_LOCALE_FONT";
/// 붙이기 전 ab_glyph 검증 + append 를 하는 공유 헬퍼. 넷이 이것을 불러야 한다(사본 금지).
const HELPER: &str = "install_locale_font_fallback";

#[test]
fn every_ui_plugin_reads_the_same_locale_font_env_and_calls_the_shared_helper() {
    let root = repo_root();
    let mut checked = 0usize;
    let mut missing_env = Vec::new();
    let mut missing_helper = Vec::new();

    for (plugin, file) in UI_PLUGINS {
        let path = root.join("crates").join(plugin).join(file);
        let src = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
        checked += 1;
        if !src.contains(LOCALE_FONT_ENV) {
            missing_env.push(*plugin);
        }
        if !src.contains(HELPER) {
            missing_helper.push(*plugin);
        }
    }

    assert_eq!(
        checked,
        UI_PLUGINS.len(),
        "scanned {checked} plugin(s), expected {} — the scan is broken, not clean",
        UI_PLUGINS.len()
    );
    assert!(
        missing_env.is_empty(),
        "UI plugins missing the locale font variable {LOCALE_FONT_ENV}: {missing_env:?}"
    );
    assert!(
        missing_helper.is_empty(),
        "UI plugins missing the shared font validation helper {HELPER}: {missing_helper:?}"
    );
}

/// 호스트에서 언어팩 글꼴을 설치하는 파일. 플러그인과 같은 번호를 읽어야 한다.
const HOST_LOCALE_FONT_INSTALLERS: &[&str] =
    &["src/adapters/ui/font_registry.rs", "src/gfx/gpu/fonts.rs"];

/// 공백을 뺀 소스에서 찾는다. 번호를 변수로 받거나 인자에 바로 쓰는 두 형태만 있다.
const FACE_IN_A_VARIABLE: &str = "letface=tasty_egui_theme::locale_font_face_from_env();";
const FACE_VARIABLE_PASSED: &str = "install_locale_font_fallback(&mutfonts,&path,face)";
const FACE_PASSED_DIRECTLY: &str =
    "install_locale_font_fallback(&mutfonts,&path,tasty_egui_theme::locale_font_face_from_env(),)";

#[test]
fn every_locale_font_install_passes_the_face_number_from_the_host() {
    let root = repo_root();
    let plugin_files = UI_PLUGINS
        .iter()
        .map(|(plugin, file)| format!("crates/{plugin}/{file}"));
    let host_files = HOST_LOCALE_FONT_INSTALLERS.iter().map(ToString::to_string);
    let mut checked = 0usize;
    let mut dropped = Vec::new();
    for file in plugin_files.chain(host_files) {
        let src = std::fs::read_to_string(root.join(&file))
            .unwrap_or_else(|e| panic!("cannot read {file}: {e}"));
        let compact: String = src.chars().filter(|c| !c.is_whitespace()).collect();
        checked += 1;
        let through_variable =
            compact.contains(FACE_IN_A_VARIABLE) && compact.contains(FACE_VARIABLE_PASSED);
        if !through_variable && !compact.contains(FACE_PASSED_DIRECTLY) {
            dropped.push(file);
        }
    }
    assert_eq!(
        checked,
        UI_PLUGINS.len() + HOST_LOCALE_FONT_INSTALLERS.len(),
        "the scan is broken, not clean"
    );
    assert!(
        dropped.is_empty(),
        "these files do not pass locale_font_face_from_env to the locale font: {dropped:?}"
    );
}

/// 현재 후보의 파일 이름 전부. 이 파일 밖에 다시 적으면 프로세스마다 다른 글꼴을 고른다.
/// 후보에 새 이름을 넣으면 여기에도 넣는다. 넣지 않은 이름의 복사는 찾지 못한다.
const SYSTEM_CJK_FONT_FILES: &[&str] = &[
    "NotoSansCJK",
    "malgun.ttf",
    "msyh.ttc",
    "AppleSDGothicNeo",
    "ヒラギノ",
    "Arial Unicode",
];
const SYSTEM_CJK_LIST_OWNER: &str = "crates/tasty-egui-theme/src/lib.rs";

/// 빈 순회가 통과로 보이지 않게 한다. 대규모 누락만 잡고 파일 한두 개의 이동은 잡지 않는다.
const RUST_SOURCES_FLOOR: Floor = Floor {
    min: 2500,
    measured: 3047,
    measured_on: "2026-11-09",
    counted_on: CountedOn::Tree("3d02bf359 — git ls-files의 src·crates 아래 .rs 파일 수"),
    why_this_gap: "크레이트 몇 개가 옮겨 가도 실패하지 않도록 약 18%를 남긴다. 후보 이름의 \
                   복사를 찾는 검사라 일부 누락보다 빈 순회를 막는 것이 목적이다.",
};

#[test]
fn system_cjk_font_candidates_are_listed_in_one_file() {
    let root = repo_root();
    // Windows의 file!()은 역슬래시를 쓰고 순회 결과의 rel은 슬래시로 정규화돼 있다.
    let this_file = normalized_rel(Path::new(file!()), Path::new(""));
    let walked = walk_with_floor(
        &root,
        &root,
        &RUST_SOURCES_FLOOR,
        Descend::SkipBuildCachesAndDotDirs,
        &|found| {
            found.rel.ends_with(".rs")
                // 이 파일은 찾을 이름을 입력으로 적어 둔다.
                && found.rel != this_file
                && (found.rel.starts_with("src/") || found.rel.starts_with("crates/"))
        },
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let mut owner_lists_them = false;
    let mut copies = Vec::new();
    for file in &walked {
        let src = std::fs::read_to_string(&file.path)
            .unwrap_or_else(|e| panic!("cannot read {}: {e}", file.rel));
        if !SYSTEM_CJK_FONT_FILES.iter().any(|name| src.contains(name)) {
            continue;
        }
        if file.rel == SYSTEM_CJK_LIST_OWNER {
            owner_lists_them = SYSTEM_CJK_FONT_FILES.iter().all(|name| src.contains(name));
        } else {
            copies.push(file.rel.clone());
        }
    }
    assert!(
        owner_lists_them,
        "{SYSTEM_CJK_LIST_OWNER} should list every name in {SYSTEM_CJK_FONT_FILES:?}"
    );
    assert!(
        copies.is_empty(),
        "system CJK font candidates are repeated outside {SYSTEM_CJK_LIST_OWNER}: {copies:?}"
    );
}
