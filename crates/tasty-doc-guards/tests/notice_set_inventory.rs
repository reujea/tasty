//! 배포물에 넣는 고지 세트의 정적 일치를 확인한다.
//! - `THIRD_PARTY_LICENSES.md`가 `LICENSES/`의 모든 파일을 링크하고, 링크한 파일이 모두 있다.
//! - `wix/main.wxs`가 `LICENSES/`의 파일마다 `Source='LICENSES\<이름>'`인 File을 가진
//!   Component를 선언하고, 그 Component를 Feature의 `ComponentRef`로 참조한다.
//!   `build-windows.ps1`의 빌드 전 검사는 Source 문자열만 보므로 `ComponentRef` 누락은
//!   여기서 잡는다.
//!
//! 인벤토리가 실제로 번들된 모든 자산을 적었는지는 검사하지 않는다.

use std::collections::BTreeSet;
use std::path::Path;

const INVENTORY: &str = "THIRD_PARTY_LICENSES.md";
const TEXTS_DIR: &str = "LICENSES";
const WXS: &str = "wix/main.wxs";

/// `LICENSES/` 바로 아래 일반 파일 이름.
fn license_files(root: &Path) -> BTreeSet<String> {
    let dir = root.join(TEXTS_DIR);
    let entries =
        std::fs::read_dir(&dir).unwrap_or_else(|e| panic!("{} 을 읽지 못했다: {e}", dir.display()));
    let mut files = BTreeSet::new();
    for entry in entries {
        let entry = entry.expect("디렉터리 항목");
        // file_type은 링크를 따라가지 않으므로 링크를 파일로 세지 않는다.
        if entry.file_type().expect("파일 종류").is_file() {
            files.insert(entry.file_name().to_string_lossy().into_owned());
        }
    }
    files
}

/// `text`의 Markdown 링크 `](LICENSES/<이름>)`이 가리키는 이름.
fn linked_texts(text: &str) -> BTreeSet<String> {
    let needle = format!("]({TEXTS_DIR}/");
    let mut out = BTreeSet::new();
    let mut rest = text;
    while let Some(pos) = rest.find(&needle) {
        let after = &rest[pos + needle.len()..];
        if let Some(end) = after.find(')') {
            out.insert(after[..end].to_string());
        }
        rest = after;
    }
    out
}

/// `<Component Id='X' ...>` 안의 `<File ... Source='LICENSES\<이름>' ...>`을 (이름, Component Id)로 읽는다.
fn wix_notice_components(wxs: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut component = None::<String>;
    for line in wxs.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("<Component Id='") {
            component = rest.split('\'').next().map(str::to_owned);
            continue;
        }
        if line.starts_with("</Component>") {
            component = None;
            continue;
        }
        let needle = format!("Source='{TEXTS_DIR}\\");
        if line.starts_with("<File ")
            && let Some(pos) = line.find(&needle)
        {
            let name = line[pos + needle.len()..].split('\'').next().unwrap_or("");
            let id = component.clone().unwrap_or_default();
            out.push((name.to_owned(), id));
        }
    }
    out
}

/// `<ComponentRef Id='X'/>`의 Id 집합.
fn wix_component_refs(wxs: &str) -> BTreeSet<String> {
    let needle = "<ComponentRef Id='";
    let mut out = BTreeSet::new();
    let mut rest = wxs;
    while let Some(pos) = rest.find(needle) {
        let after = &rest[pos + needle.len()..];
        if let Some(end) = after.find('\'') {
            out.insert(after[..end].to_string());
        }
        rest = after;
    }
    out
}

#[test]
fn the_inventory_links_every_license_text_and_nothing_else() {
    let root = tasty_doc_guards::repo_root();
    let files = license_files(&root);
    assert!(
        !files.is_empty(),
        "{TEXTS_DIR}/ 에 라이선스 본문이 없다. 배포물에 제3자 고지가 빠진다"
    );
    let text = std::fs::read_to_string(root.join(INVENTORY))
        .unwrap_or_else(|e| panic!("{INVENTORY} 을 읽지 못했다: {e}"));
    let links = linked_texts(&text);
    let unlinked: Vec<_> = files.difference(&links).collect();
    let missing: Vec<_> = links.difference(&files).collect();
    assert!(
        unlinked.is_empty() && missing.is_empty(),
        "{INVENTORY} 와 {TEXTS_DIR}/ 가 다르다.\n링크가 없는 파일: {unlinked:?}\n\
         없는 파일을 가리키는 링크: {missing:?}"
    );
}

#[test]
fn the_msi_installs_every_license_text() {
    let root = tasty_doc_guards::repo_root();
    let files = license_files(&root);
    let wxs = std::fs::read_to_string(root.join(WXS))
        .unwrap_or_else(|e| panic!("{WXS} 을 읽지 못했다: {e}"));
    let components = wix_notice_components(&wxs);
    let declared: BTreeSet<String> = components.iter().map(|(n, _)| n.clone()).collect();
    let undeclared: Vec<_> = files.difference(&declared).collect();
    let stale: Vec<_> = declared.difference(&files).collect();
    assert!(
        undeclared.is_empty() && stale.is_empty(),
        "{WXS} 의 LICENSES File 과 {TEXTS_DIR}/ 가 다르다.\nFile 이 없는 본문: {undeclared:?}\n\
         없는 본문을 가리키는 File: {stale:?}"
    );
    let refs = wix_component_refs(&wxs);
    let unreferenced: Vec<_> = components
        .iter()
        .filter(|(_, id)| id.is_empty() || !refs.contains(id))
        .collect();
    assert!(
        unreferenced.is_empty(),
        "{WXS} 의 Feature 가 참조하지 않는 고지 Component 가 있다. MSI에 그 본문이 들어가지 않는다: \
         {unreferenced:?}"
    );
}

#[test]
fn wix_reader_pairs_files_with_their_component() {
    let wxs = "<Component Id='A' Guid='*'>\n  <File Id='AF' Source='LICENSES\\a.txt'/>\n</Component>\n\
               <Component Id='B' Guid='*'>\n  <File Id='BF' Source='LICENSES\\b.txt'/>\n</Component>\n\
               <File Id='C' Source='other.txt'/>\n<ComponentRef Id='A'/>";
    assert_eq!(
        wix_notice_components(wxs),
        vec![
            ("a.txt".to_string(), "A".to_string()),
            ("b.txt".to_string(), "B".to_string())
        ]
    );
    assert_eq!(wix_component_refs(wxs), BTreeSet::from(["A".to_string()]));
}

#[test]
fn links_are_read_from_markdown_link_targets_only() {
    let text = "| a | [`LICENSES/a.txt`](LICENSES/a.txt) |\nplain LICENSES/b.txt\n\
                [x](LICENSES/c.txt) and [y](LICENSES/c.txt)";
    assert_eq!(
        linked_texts(text),
        BTreeSet::from(["a.txt".to_string(), "c.txt".to_string()])
    );
}
