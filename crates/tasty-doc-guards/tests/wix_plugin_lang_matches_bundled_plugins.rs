//! wix/main.wxs가 나열한 플러그인 번역 파일이 번들 플러그인의 실제 lang 파일과 같은지 양방향으로 대조한다.
//! deb·rpm과 빌드 스크립트는 lang/*를 자동으로 담지만 WiX는 파일을 하나씩 적는다. 빠지면 Windows 설치본만 영어로 나온다.
//! bundle = false 플러그인은 번들 대상이 아니므로 WiX에 있으면 실패한다.
//! Component와 ComponentRef의 짝, 번역 파일과 그것을 감싼 Directory Id의 짝도 확인한다.
//! Directory의 Name이 플러그인 id와 같은지는 보지 않는다. MSI를 만들어 설치 결과를 보지는 않는다.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use tasty_doc_guards::floored_walk::{CountedOn, Descend, Floor, normalized_rel, walk_with_floor};

const MANIFEST_FLOOR: Floor = Floor {
    min: 7,
    measured: 9,
    measured_on: "2026-11-09",
    counted_on: CountedOn::Tree("0801838a8 — crates 바로 아래 플러그인의 tasty-plugin.toml 9개."),
    why_this_gap: "측정한 9개 중 2개는 bundle = false다. 번들 플러그인 7개가 모두 있어야 하므로 하한을 7로 둔다. 더 줄면 플러그인 이전인지 순회 오류인지 확인한다.",
};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("crates/tasty-doc-guards 위로 두 단계가 레포 루트다")
        .to_path_buf()
}

/// 표 머리 앞의 최상위 키만 본다. 다른 표의 같은 이름 키는 매니페스트의 bundle이 아니다.
fn is_excluded_from_bundle(manifest: &str) -> bool {
    for line in manifest.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            return false;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if key.trim() == "bundle" {
            let value = value.split('#').next().unwrap_or_default().trim();
            return value == "false";
        }
    }
    false
}

/// 번들 플러그인의 lang 파일을 저장소 상대 경로로 모은다.
fn bundled_lang_files(root: &Path) -> BTreeSet<String> {
    let crates = root.join("crates");
    let manifests = walk_with_floor(
        &crates,
        root,
        &MANIFEST_FLOOR,
        Descend::SkipBuildCachesAndDotDirs,
        &|entry| {
            entry
                .path
                .file_name()
                .is_some_and(|name| name == "tasty-plugin.toml")
                && entry.path.parent().and_then(Path::parent) == Some(crates.as_path())
        },
    )
    .unwrap_or_else(|why| panic!("{why}"));

    let mut files = BTreeSet::new();
    for entry in manifests {
        let text = std::fs::read_to_string(&entry.path)
            .unwrap_or_else(|e| panic!("{}: {e}", entry.path.display()));
        if is_excluded_from_bundle(&text) {
            continue;
        }
        let lang = entry.path.parent().expect("manifest parent").join("lang");
        let listing =
            std::fs::read_dir(&lang).unwrap_or_else(|e| panic!("{}: {e}", lang.display()));
        for file in listing {
            let path = file
                .unwrap_or_else(|e| panic!("{}: {e}", lang.display()))
                .path();
            if path.extension().is_some_and(|ext| ext == "toml") {
                files.insert(normalized_rel(&path, root));
            }
        }
    }
    files
}

/// XML 주석 안의 항목은 설치 내용이 아니다.
fn without_comments(xml: &str) -> String {
    let mut out = String::new();
    let mut rest = xml;
    while let Some(start) = rest.find("<!--") {
        out.push_str(&rest[..start]);
        let end = rest[start..]
            .find("-->")
            .unwrap_or_else(|| panic!("닫히지 않은 XML 주석: {}", &rest[start..]));
        rest = &rest[start + end + 3..];
    }
    out.push_str(rest);
    out
}

fn attr<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    for quote in ['\'', '"'] {
        let open = format!(" {name}={quote}");
        if let Some(start) = tag.find(&open) {
            let value = &tag[start + open.len()..];
            return value.find(quote).map(|end| &value[..end]);
        }
    }
    None
}

#[derive(Debug, Default, PartialEq, Eq)]
struct WixLang {
    /// `Source`의 저장소 상대 경로.
    sources: BTreeSet<String>,
    /// 번역 파일을 담은 `Component`의 Id.
    components: BTreeSet<String>,
    /// `ComponentRef`의 Id 전체. 번역 파일이 아닌 항목도 들어 있다.
    refs: BTreeSet<String>,
    /// 번역 파일의 `Source`와 그 파일을 감싼 Directory들의 Id. 바깥쪽부터 적는다.
    placements: Vec<(String, Vec<String>)>,
}

/// 플러그인 lang 아래의 File과 그 파일을 담은 Component, 그리고 모든 ComponentRef를 읽는다.
fn read_wix(xml: &str) -> WixLang {
    let mut out = WixLang::default();
    let mut component: Option<String> = None;
    let mut directories: Vec<String> = Vec::new();
    for tag in without_comments(xml).split('<').skip(1) {
        if tag.starts_with("/Directory") {
            directories.pop();
            continue;
        }
        if tag.starts_with("/Component") && !tag.starts_with("/ComponentRef") {
            component = None;
            continue;
        }
        let name = tag
            .split([' ', '/', '>', '\n', '\r', '\t'])
            .next()
            .unwrap_or_default();
        match name {
            "Directory" => {
                let open_tag = tag.split('>').next().unwrap_or_default();
                if !open_tag.trim_end().ends_with('/') {
                    let id = attr(tag, "Id").unwrap_or_else(|| panic!("Id 없는 Directory: {tag}"));
                    directories.push(id.to_string());
                }
            }
            "Component" => component = attr(tag, "Id").map(str::to_string),
            "ComponentRef" => {
                let id = attr(tag, "Id").unwrap_or_else(|| panic!("Id 없는 ComponentRef: {tag}"));
                out.refs.insert(id.to_string());
            }
            "File" => {
                let Some(source) = attr(tag, "Source") else {
                    continue;
                };
                let source = normalized_rel(Path::new(source), Path::new(""));
                let is_plugin_lang = source.starts_with("crates/tasty-plugin-")
                    && source.contains("/lang/")
                    && source.ends_with(".toml");
                if is_plugin_lang {
                    let id = component
                        .clone()
                        .unwrap_or_else(|| panic!("Component 밖의 File: {source}"));
                    out.components.insert(id);
                    out.placements.push((source.clone(), directories.clone()));
                    out.sources.insert(source);
                }
            }
            _ => {}
        }
    }
    out
}

/// `crates/tasty-plugin-git-viewer/lang/ko.toml` → `GitViewer`.
fn plugin_id_stem(source: &str) -> String {
    let plugin = source
        .strip_prefix("crates/tasty-plugin-")
        .and_then(|rest| rest.split('/').next())
        .unwrap_or_else(|| panic!("플러그인 번역 경로가 아니다: {source}"));
    plugin
        .split('-')
        .map(|word| {
            let mut chars = word.chars();
            let first = chars.next().map(|c| c.to_ascii_uppercase());
            first.into_iter().chain(chars).collect::<String>()
        })
        .collect()
}

/// 번역 파일이 자기 플러그인의 `Plugin<이름>Dir` › `Plugin<이름>LangDir` 바로 아래에 있지 않은 항목.
fn misplaced(wix: &WixLang) -> Vec<String> {
    wix.placements
        .iter()
        .filter_map(|(source, directories)| {
            let stem = plugin_id_stem(source);
            let want = [format!("Plugin{stem}Dir"), format!("Plugin{stem}LangDir")];
            (!directories.ends_with(&want))
                .then(|| format!("{source}: 기대 {want:?}, 실제 {directories:?}"))
        })
        .collect()
}

#[test]
fn every_lang_file_sits_in_the_directory_of_its_plugin() {
    let root = repo_root();
    let xml = std::fs::read_to_string(root.join("wix/main.wxs")).expect("wix/main.wxs");
    let wix = read_wix(&xml);
    assert!(!wix.placements.is_empty(), "번역 파일 항목을 읽지 못했다");
    let wrong = misplaced(&wix);
    assert!(
        wrong.is_empty(),
        "다른 플러그인의 폴더에 설치되는 번역 파일: {wrong:#?}"
    );
}

#[test]
fn a_lang_file_under_another_plugin_directory_is_reported() {
    let xml = r#"
        <Directory Id='PluginsDir' Name='plugins'>
          <Directory Id='PluginGitViewerDir' Name='com.tasty.git-viewer'>
            <Directory Id='PluginGitViewerLangDir' Name='lang'>
              <Component Id='A'><File Source='crates\tasty-plugin-git-viewer\lang\en.toml'/></Component>
            </Directory>
          </Directory>
          <Directory Id='Empty' Name='empty'/>
          <Directory Id='PluginCodexDir' Name='com.tasty.codex'>
            <Directory Id='PluginCodexLangDir' Name='lang'>
              <Component Id='B'><File Source='crates\tasty-plugin-html\lang\ko.toml'/></Component>
            </Directory>
          </Directory>
        </Directory>
    "#;
    let wrong = misplaced(&read_wix(xml));
    assert_eq!(wrong.len(), 1, "{wrong:#?}");
    assert!(
        wrong[0].starts_with("crates/tasty-plugin-html/lang/ko.toml"),
        "{wrong:#?}"
    );
}

#[test]
fn wix_lists_exactly_the_lang_files_of_bundled_plugins() {
    let root = repo_root();
    let path = root.join("wix/main.wxs");
    let xml = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let wix = read_wix(&xml);
    let disk = bundled_lang_files(&root);
    assert!(
        !disk.is_empty(),
        "번들 플러그인의 lang 파일을 하나도 찾지 못했다"
    );

    let missing: Vec<&String> = disk.difference(&wix.sources).collect();
    let extra: Vec<&String> = wix.sources.difference(&disk).collect();
    assert!(
        missing.is_empty() && extra.is_empty(),
        "wix/main.wxs와 번들 플러그인의 lang 파일이 다르다.\n\
         WiX에 없어 Windows 설치본에서 빠지는 파일(Component와 ComponentRef를 추가한다): {missing:#?}\n\
         WiX에만 있는 파일(없는 파일이거나 bundle = false 플러그인이다): {extra:#?}"
    );

    let unreferenced: Vec<&String> = wix.components.difference(&wix.refs).collect();
    assert!(
        unreferenced.is_empty(),
        "ComponentRef가 없어 MSI에 들어가지 않는 번역 Component: {unreferenced:#?}"
    );
}

/// 번역 Component 이름 규칙에 맞는 ComponentRef는 실제 Component를 가리켜야 한다.
#[test]
fn every_lang_component_ref_points_at_a_lang_component() {
    let root = repo_root();
    let xml = std::fs::read_to_string(root.join("wix/main.wxs")).expect("wix/main.wxs");
    let wix = read_wix(&xml);
    let dangling: Vec<&String> = wix
        .refs
        .iter()
        .filter(|id| id.starts_with("Plugin") && id.contains("Lang"))
        .filter(|id| !wix.components.contains(*id))
        .collect();
    assert!(
        dangling.is_empty(),
        "대상이 없는 번역 ComponentRef: {dangling:#?}"
    );
}

#[test]
fn the_reader_takes_plugin_lang_files_only_and_skips_comments() {
    let xml = r#"
        <Component Id='A'><File Id='a' Source='crates\tasty-plugin-a\lang\en.toml' KeyPath='yes'/></Component>
        <Component Id='B'><File Id='b' Source="crates/tasty-plugin-b/lang/ko.toml"/></Component>
        <Component Id='C'><File Id='c' Source='crates\tasty-plugin-c\tasty-plugin.toml'/></Component>
        <Component Id='D'><File Id='d' Source='lang\en.toml'/></Component>
        <!-- <Component Id='E'><File Id='e' Source='crates\tasty-plugin-e\lang\en.toml'/></Component> -->
        <ComponentRef Id='A'/>
    "#;
    let wix = read_wix(xml);
    assert_eq!(
        wix.sources,
        BTreeSet::from([
            "crates/tasty-plugin-a/lang/en.toml".to_string(),
            "crates/tasty-plugin-b/lang/ko.toml".to_string(),
        ])
    );
    assert_eq!(
        wix.components,
        BTreeSet::from(["A".to_string(), "B".to_string()])
    );
    assert_eq!(wix.refs, BTreeSet::from(["A".to_string()]));
}

#[test]
fn only_a_top_level_false_excludes_a_plugin() {
    assert!(is_excluded_from_bundle(
        "id = 'x'\nbundle = false\n[entry]\n"
    ));
    assert!(is_excluded_from_bundle("bundle=false # demo\n"));
    assert!(!is_excluded_from_bundle("bundle = true\n"));
    assert!(!is_excluded_from_bundle(
        "id = 'x'\n[entry]\nbundle = false\n"
    ));
    assert!(!is_excluded_from_bundle("id = 'x'\n"));
}
