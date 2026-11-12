//! release.yml의 모든 `gh release upload` 대상이 릴리스 가이드의 "릴리스 자산" 표와 같은지 확인한다.
//! 이름과 업로드 잡을 쌍으로 양방향 대조한다. 각 업로드 줄의 마지막 인자를 읽으며 `dist/` 접두와
//! 버전 자리(`${VERSION}` → `{ver}`)만 정규화한다. glob·셸 변수처럼 이름을 펼칠 수 없는 대상은
//! 그대로 남겨 표에 없는 자산으로 실패시킨다.
//! 빌드 스크립트가 그 이름의 파일을 실제로 만드는지는 검사하지 않는다.
//! doc-guards.yml의 경로 필터 없는 develop·release·main push·PR에서 실행된다.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use tasty_doc_guards::temp_scratch::Scratch;

const GUIDE: &str = "docs/dev-guide/release.md";
const SECTION: &str = "## 릴리스 자산";

/// 2026-10-14 실측 19개에 여유를 둔 하한. 표·업로드 판독 누락을 찾는 보조 검사다.
const MIN_ASSETS: usize = 12;

fn repo_root() -> PathBuf {
    tasty_doc_guards::repo_root()
}

/// release.yml의 업로드 대상을 (이름, 잡) 쌍으로 읽는다.
fn uploaded(root: &Path) -> BTreeSet<(String, String)> {
    let text = std::fs::read_to_string(root.join(".github/workflows/release.yml"))
        .expect("release.yml 을 읽지 못했다");
    let mut job = String::new();
    let mut out = BTreeSet::new();
    for line in text.lines() {
        // jobs: 아래 두 칸 들여쓴 키가 잡 이름이다.
        if let Some(rest) = line.strip_prefix("  ")
            && !rest.starts_with(' ')
            && let Some(name) = rest.strip_suffix(':')
        {
            job = name.to_owned();
            continue;
        }
        let Some(pos) = line.find("gh release upload") else {
            continue;
        };
        // `|| true` 같은 뒤따르는 셸 명령은 업로드 인자가 아니다.
        let cmd = line[pos..]
            .split(['|', '&', ';'])
            .next()
            .unwrap_or_default();
        let Some(last) = cmd.split_whitespace().last() else {
            continue;
        };
        let target = last.trim_matches('"');
        let target = target.strip_prefix("dist/").unwrap_or(target);
        out.insert((target.replace("${VERSION}", "{ver}"), job.clone()));
    }
    out
}

/// 가이드 "릴리스 자산" 절의 표에서 (이름, 잡) 쌍을 읽는다. 다른 절의 표는 읽지 않는다.
fn documented(root: &Path) -> BTreeSet<(String, String)> {
    let text = std::fs::read_to_string(root.join(GUIDE)).expect("release.md 를 읽지 못했다");
    let start = text
        .find(&format!("{SECTION}\n"))
        .unwrap_or_else(|| panic!("{GUIDE} 에 `{SECTION}` 절이 없다"));
    let body = &text[start + SECTION.len()..];
    let body = &body[..body.find("\n## ").unwrap_or(body.len())];
    let mut out = BTreeSet::new();
    for line in body.lines() {
        let cells: Vec<&str> = line.split('|').map(str::trim).collect();
        // | `자산` | `잡` | 종류 | → ["", 자산, 잡, 종류, ""]
        if cells.len() < 4 || !cells[1].starts_with('`') {
            continue;
        }
        let name = cells[1].trim_matches('`');
        let job = cells[2].trim_matches('`');
        out.insert((name.to_owned(), job.to_owned()));
    }
    out
}

fn show(set: &BTreeSet<&(String, String)>) -> String {
    set.iter()
        .map(|(n, j)| format!("{n} ({j})"))
        .collect::<Vec<_>>()
        .join("\n  ")
}

#[test]
fn release_uploads_are_exactly_the_documented_assets() {
    let root = repo_root();
    let up = uploaded(&root);
    let doc = documented(&root);
    assert!(
        up.len() >= MIN_ASSETS && doc.len() >= MIN_ASSETS,
        "업로드 {}개, 표 {}개를 읽었다(하한 {MIN_ASSETS}). 실제 감소와 판독 실패를 구별한다.",
        up.len(),
        doc.len()
    );
    let extra: BTreeSet<_> = up.difference(&doc).collect();
    let missing: BTreeSet<_> = doc.difference(&up).collect();
    assert!(
        extra.is_empty() && missing.is_empty(),
        "release.yml 업로드와 {GUIDE} `{SECTION}` 표가 다르다.\n\
         표에 없는 업로드:\n  {}\n업로드하지 않는 표 항목:\n  {}\n\
         자산을 바꾸려면 표·release.yml·빌드 스크립트·사용자 가이드를 함께 고친다.",
        show(&extra),
        show(&missing)
    );
}

/// 합성 트리에서 판독이 잡 구분, 접두 정규화, 펼칠 수 없는 대상, 절 범위를 바르게 처리하는지 확인한다.
#[test]
fn both_readers_answer_on_a_substituted_tree() {
    let probe = Scratch::new("release-assets-reader");
    let dir = probe.path();

    let wf = dir.join(".github/workflows");
    std::fs::create_dir_all(&wf).expect("합성 워크플로 트리를 만들지 못했다");
    std::fs::write(
        wf.join("release.yml"),
        concat!(
            "jobs:\n",
            "  build-a:\n",
            "    steps:\n",
            "      - name: Upload\n",
            "        run: |\n",
            "          gh release upload --clobber \"v${VERSION}\" \"dist/alpha-${VERSION}.tar.gz\"\n",
            "          echo \"dist/never-uploaded.zip\"\n",
            "  publish:\n",
            "    steps:\n",
            "      - run: |\n",
            "          gh release upload --clobber \"v${VERSION}\" LICENSES/*\n",
            "          gh release upload --clobber \"v${VERSION}\" \"$f\" || true\n",
        ),
    )
    .expect("합성 release.yml 을 쓰지 못했다");

    let up = uploaded(dir);
    let pairs = |v: &[(&str, &str)]| -> BTreeSet<(String, String)> {
        v.iter()
            .map(|(n, j)| ((*n).to_owned(), (*j).to_owned()))
            .collect()
    };
    assert_eq!(
        up,
        pairs(&[
            ("alpha-{ver}.tar.gz", "build-a"),
            ("LICENSES/*", "publish"),
            ("$f", "publish"),
        ]),
        "업로드 판독이 합성 트리에서 다른 답을 냈다"
    );

    let docs = dir.join("docs/dev-guide");
    std::fs::create_dir_all(&docs).expect("합성 문서 트리를 만들지 못했다");
    std::fs::write(
        docs.join("release.md"),
        concat!(
            "## 앞 절\n\n| `before-decoy` | `build-a` | x |\n\n",
            "## 릴리스 자산\n\n| 자산 | 업로드 잡 | 종류 |\n|---|---|---|\n",
            "| `alpha-{ver}.tar.gz` | `build-a` | GUI |\n\n",
            "## 뒷 절\n\n| `after-decoy` | `build-a` | x |\n",
        ),
    )
    .expect("합성 release.md 를 쓰지 못했다");

    let doc = documented(dir);
    assert_eq!(
        doc,
        pairs(&[("alpha-{ver}.tar.gz", "build-a")]),
        "표 판독이 절 밖의 표나 헤더 행을 읽었다"
    );

    let extra: Vec<_> = up.difference(&doc).collect();
    assert_eq!(
        extra.len(),
        2,
        "glob·셸 변수 업로드를 표에 없는 자산으로 잡지 못했다: {extra:?}"
    );
}
