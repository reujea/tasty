//! MSI의 번들 플러그인 파일은 `wix/main.wxs`에 적지 않고 `scripts/build-windows.ps1`이 받은 플러그인
//! 폴더에서 만든 fragment(`target\bundled-plugins.wxs`)로 들어간다. 이 시험은 그 연결을 문서 대조로 확인한다.
//! - `main.wxs`에 플러그인 파일을 가리키는 `File`이 없다. 남아 있으면 fragment와 겹치거나 받은 폴더와 다른
//!   파일이 설치된다.
//! - `main.wxs`가 `Bin` 바로 아래에 빈 `Directory Id='PluginsDir' Name='plugins'`를 두고, `Binaries` 기능이
//!   `ComponentGroupRef Id='BundledPlugins'`를 담는다.
//! - `build-windows.ps1`이 그 Id로 `DirectoryRef`·`ComponentGroup`을 만들고 `cargo wix --include`로 넘긴다.
//!
//! fragment 내용(받은 폴더의 파일이 모두 들어가는지)은 MSI를 만든 뒤 build-windows.ps1의
//! `Test-BundledPluginTree`가 설치 트리와 받은 폴더를 대조해 확인한다. 이 시험은 MSI를 만들지 않는다.

use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("crates/tasty-doc-guards 위로 두 단계가 레포 루트다")
        .to_path_buf()
}

fn read(rel: &str) -> String {
    let path = repo_root().join(rel);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
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

#[derive(Debug, Default)]
struct Wix {
    /// 플러그인 파일로 보이는 `File`의 `Source`.
    plugin_sources: Vec<String>,
    /// `PluginsDir` Directory를 감싼 Directory Id들(바깥쪽부터)과 그 Directory가 비었는지.
    plugins_dir: Vec<(Vec<String>, bool)>,
    /// `ComponentGroupRef`의 Id와 그것을 감싼 `Feature` Id.
    group_refs: Vec<(String, Option<String>)>,
}

/// `Source`가 플러그인 크레이트나 받은 플러그인 폴더, 또는 플러그인 실행 파일을 가리키는지 본다.
fn is_plugin_source(source: &str) -> bool {
    let s = source.replace('\\', "/").to_ascii_lowercase();
    s.contains("tasty-plugin") || s.contains("bundled-plugins") || s.contains("/plugins/")
}

fn read_wix(xml: &str) -> Wix {
    let mut out = Wix::default();
    let mut directories: Vec<String> = Vec::new();
    let mut features: Vec<String> = Vec::new();
    for tag in without_comments(xml).split('<').skip(1) {
        if tag.starts_with("/Directory") && !tag.starts_with("/DirectoryRef") {
            directories.pop();
            continue;
        }
        if tag.starts_with("/Feature") {
            features.pop();
            continue;
        }
        let name = tag
            .split([' ', '/', '>', '\n', '\r', '\t'])
            .next()
            .unwrap_or_default();
        let open_tag = tag.split('>').next().unwrap_or_default();
        let self_closing = open_tag.trim_end().ends_with('/');
        match name {
            "Directory" => {
                let id = attr(open_tag, "Id").unwrap_or_else(|| panic!("Id 없는 Directory: {tag}"));
                if id == "PluginsDir" {
                    out.plugins_dir.push((directories.clone(), self_closing));
                }
                if !self_closing {
                    directories.push(id.to_string());
                }
            }
            "Feature" => {
                if !self_closing {
                    let id =
                        attr(open_tag, "Id").unwrap_or_else(|| panic!("Id 없는 Feature: {tag}"));
                    features.push(id.to_string());
                }
            }
            "ComponentGroupRef" => {
                let id = attr(open_tag, "Id")
                    .unwrap_or_else(|| panic!("Id 없는 ComponentGroupRef: {tag}"));
                out.group_refs
                    .push((id.to_string(), features.last().cloned()));
            }
            "File" => {
                if let Some(source) = attr(open_tag, "Source")
                    && is_plugin_source(source)
                {
                    out.plugin_sources.push(source.to_string());
                }
            }
            _ => {}
        }
    }
    out
}

#[test]
fn main_wxs_lists_no_plugin_file() {
    let wix = read_wix(&read("wix/main.wxs"));
    assert!(
        wix.plugin_sources.is_empty(),
        "wix/main.wxs가 플러그인 파일을 직접 적고 있다. 번들 플러그인은 build-windows.ps1이 받은 폴더에서 만든 \
         fragment로만 들어간다: {:#?}",
        wix.plugin_sources
    );
}

#[test]
fn main_wxs_leaves_an_empty_plugins_dir_under_bin() {
    let wix = read_wix(&read("wix/main.wxs"));
    assert_eq!(
        wix.plugins_dir.len(),
        1,
        "PluginsDir Directory가 하나여야 한다: {:#?}",
        wix.plugins_dir
    );
    let (parents, empty) = &wix.plugins_dir[0];
    assert_eq!(
        parents.last().map(String::as_str),
        Some("Bin"),
        "PluginsDir는 Bin 바로 아래여야 한다(bundle_root가 실행 파일 옆 plugins를 찾는다): {parents:?}"
    );
    assert!(
        *empty,
        "PluginsDir는 비어 있어야 한다. 내용은 생성 fragment의 DirectoryRef가 채운다"
    );
}

#[test]
fn the_binaries_feature_takes_the_bundled_plugins_group() {
    let wix = read_wix(&read("wix/main.wxs"));
    let hits: Vec<_> = wix
        .group_refs
        .iter()
        .filter(|(id, _)| id == "BundledPlugins")
        .collect();
    assert_eq!(
        hits.len(),
        1,
        "ComponentGroupRef BundledPlugins가 하나여야 한다: {:#?}",
        wix.group_refs
    );
    assert_eq!(
        hits[0].1.as_deref(),
        Some("Binaries"),
        "BundledPlugins는 Binaries 기능에 바로 들어가야 한다(PATH 하위 기능은 끌 수 있다)"
    );
}

#[test]
fn the_windows_script_generates_and_includes_the_group() {
    let script = read("scripts/build-windows.ps1");
    for needle in [
        "<DirectoryRef Id='PluginsDir'>",
        "<ComponentGroup Id='BundledPlugins'>",
        "\"--include\", $BundledPluginsWxs",
        "New-BundledPluginsWxs",
    ] {
        assert!(
            script.contains(needle),
            "scripts/build-windows.ps1에 `{needle}`가 없다. main.wxs의 PluginsDir·BundledPlugins와 짝을 이뤄야 한다"
        );
    }
}

#[test]
fn the_reader_finds_plugin_files_dirs_and_group_refs() {
    let xml = r#"
        <Directory Id='Bin' Name='bin'>
          <Component Id='A'><File Id='a' Source='crates\tasty-plugin-a\lang\en.toml'/></Component>
          <Component Id='B'><File Id='b' Source='$(var.CargoTargetBinDir)\tasty-plugin-b.exe'/></Component>
          <Component Id='C'><File Id='c' Source='LICENSE'/></Component>
          <!-- <Component Id='D'><File Id='d' Source='target\bundled-plugins\x\y'/></Component> -->
          <Directory Id='PluginsDir' Name='plugins'/>
        </Directory>
        <Feature Id='Binaries'>
          <ComponentGroupRef Id='BundledPlugins'/>
          <Feature Id='Environment'><ComponentGroupRef Id='Other'/></Feature>
        </Feature>
    "#;
    let wix = read_wix(xml);
    assert_eq!(
        wix.plugin_sources,
        vec![
            r"crates\tasty-plugin-a\lang\en.toml".to_string(),
            r"$(var.CargoTargetBinDir)\tasty-plugin-b.exe".to_string(),
        ]
    );
    assert_eq!(wix.plugins_dir, vec![(vec!["Bin".to_string()], true)]);
    assert_eq!(
        wix.group_refs,
        vec![
            ("BundledPlugins".to_string(), Some("Binaries".to_string())),
            ("Other".to_string(), Some("Environment".to_string())),
        ]
    );
}

#[test]
fn a_filled_plugins_dir_is_not_empty() {
    let xml = r#"
        <Directory Id='Bin' Name='bin'>
          <Directory Id='PluginsDir' Name='plugins'>
            <Directory Id='X' Name='com.tasty.x'/>
          </Directory>
        </Directory>
    "#;
    let wix = read_wix(xml);
    assert_eq!(wix.plugins_dir, vec![(vec!["Bin".to_string()], false)]);
}
