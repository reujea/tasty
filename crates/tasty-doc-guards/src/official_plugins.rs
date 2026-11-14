//! 공식 플러그인 잠금 파일(`official-plugins.lock.toml`)의 형식·생성·검사.
//!
//! 잠금 파일은 공식 플러그인 레포 릴리스 하나를 고정한다. 릴리스 태그·커밋, 그 릴리스가 빌드에
//! 쓴 tasty 커밋과 계약 값, 플러그인마다 5플랫폼 산출물의 파일 이름·URL·SHA-256을 담는다.
//! 형식과 갱신 절차는 docs/dev-guide/plugin-packaging.md의 "공식 플러그인 잠금 파일" 절이다.
//!
//! 이 크레이트는 의존성을 두지 않으므로 TOML 전체를 읽지 않는다. 잠금 파일과 릴리스 메타데이터
//! (`release-meta.toml`)가 쓰는 부분만 읽는다: 주석, `[release]`, `[[plugin]]`,
//! `[[plugin.artifact]]`, 그리고 값이 문자열·정수·불리언인 `key = value` 줄. 그 밖의 형식
//! (배열, 인라인 표, 여러 줄 문자열, 모르는 표·키)은 읽지 않고 줄 번호와 함께 실패한다.
//! 잠금 파일은 `generate`가 만들고 손으로 고치지 않으므로 이 부분집합으로 충분하다.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// 저장소 루트 기준 잠금 파일 경로.
pub const LOCK_FILE: &str = "official-plugins.lock.toml";

/// 잠금 파일 형식 버전.
pub const SCHEMA: i64 = 1;

/// 릴리스 산출물의 플랫폼 표지. tasty `release.yml`의 다섯 빌드 잡과 1:1이다.
pub const PLATFORMS: [&str; 5] = [
    "macos-arm64",
    "windows-x64",
    "windows-arm64",
    "linux-x64",
    "linux-arm64",
];

/// 공식 플러그인 id 접두. 실행 파일 이름은 `tasty-plugin-<접두 뒤>`다.
pub const ID_PREFIX: &str = "com.tasty.";

/// 릴리스 TSV(`plugins-<platform>.tsv`)의 머리줄.
pub const TSV_HEADER: &str = "id\tversion\tplatform\tfile\tsha256";

/// 계약 값이 다를 때 덧붙이는 안내.
pub const REFRESH_HINT: &str = "공식 플러그인 레포의 새 릴리스로 잠금 파일을 갱신해야 한다 \
(docs/dev-guide/plugin-packaging.md 의 \"잠금 파일 갱신\" 절).";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    pub repository: String,
    pub tag: String,
    pub commit: String,
    pub tasty_rev: String,
    pub host_api_version: String,
    pub protocol_version: String,
    pub manifest_version: i64,
    pub epaint_version: String,
    pub epaint_patch_tree: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Artifact {
    pub platform: String,
    pub file: String,
    pub url: String,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plugin {
    pub id: String,
    pub version: String,
    pub bundle: bool,
    pub artifacts: Vec<Artifact>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lock {
    pub schema: i64,
    pub release: Release,
    pub plugins: Vec<Plugin>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Value {
    Str(String),
    Int(i64),
    Bool(bool),
}

type Pairs = BTreeMap<String, (usize, Value)>;

#[derive(Default)]
struct RawPlugin {
    line: usize,
    pairs: Pairs,
    artifacts: Vec<(usize, Pairs)>,
}

#[derive(Default)]
struct RawDoc {
    root: Pairs,
    release: Option<(usize, Pairs)>,
    plugins: Vec<RawPlugin>,
}

enum Section {
    Root,
    Release,
    Plugin,
    Artifact,
}

fn parse_string(rest: &str, line_no: usize) -> Result<(String, &str), String> {
    let mut out = String::new();
    let mut chars = rest.char_indices();
    while let Some((i, c)) = chars.next() {
        match c {
            '"' => return Ok((out, &rest[i + 1..])),
            '\\' => match chars.next() {
                Some((_, '"')) => out.push('"'),
                Some((_, '\\')) => out.push('\\'),
                other => {
                    return Err(format!("{line_no}행: 지원하지 않는 이스케이프 {other:?}"));
                }
            },
            _ => out.push(c),
        }
    }
    Err(format!("{line_no}행: 문자열이 닫히지 않았다"))
}

fn trailing_ok(rest: &str) -> bool {
    let t = rest.trim();
    t.is_empty() || t.starts_with('#')
}

fn parse_value(raw: &str, line_no: usize) -> Result<Value, String> {
    if let Some(rest) = raw.strip_prefix('"') {
        let (s, tail) = parse_string(rest, line_no)?;
        if !trailing_ok(tail) {
            return Err(format!("{line_no}행: 문자열 뒤에 읽을 수 없는 내용이 있다"));
        }
        return Ok(Value::Str(s));
    }
    let token = raw.split('#').next().unwrap_or("").trim();
    match token {
        "true" => return Ok(Value::Bool(true)),
        "false" => return Ok(Value::Bool(false)),
        _ => {}
    }
    let digits = token.strip_prefix('-').unwrap_or(token);
    if !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()) {
        return token
            .parse::<i64>()
            .map(Value::Int)
            .map_err(|e| format!("{line_no}행: 정수를 읽지 못했다: {e}"));
    }
    Err(format!(
        "{line_no}행: 문자열·정수·불리언이 아닌 값은 읽지 않는다: {raw}"
    ))
}

fn parse_doc(text: &str) -> Result<RawDoc, String> {
    let mut doc = RawDoc::default();
    let mut section = Section::Root;
    for (idx, line) in text.lines().enumerate() {
        let line_no = idx + 1;
        let t = line.trim();
        if t.is_empty() || t.starts_with('#') {
            continue;
        }
        if let Some(rest) = t.strip_prefix("[[") {
            let Some((name, _)) = rest.split_once("]]").filter(|(_, tail)| trailing_ok(tail))
            else {
                return Err(format!("{line_no}행: 배열 표 머리를 읽지 못했다: {t}"));
            };
            match name.trim() {
                "plugin" => {
                    doc.plugins.push(RawPlugin {
                        line: line_no,
                        ..RawPlugin::default()
                    });
                    section = Section::Plugin;
                }
                "plugin.artifact" => {
                    let Some(plugin) = doc.plugins.last_mut() else {
                        return Err(format!(
                            "{line_no}행: [[plugin]] 앞에 [[plugin.artifact]]가 있다"
                        ));
                    };
                    plugin.artifacts.push((line_no, Pairs::new()));
                    section = Section::Artifact;
                }
                other => return Err(format!("{line_no}행: 모르는 배열 표 [[{other}]]")),
            }
            continue;
        }
        if let Some(name) = t.strip_prefix('[') {
            let Some((name, tail)) = name.split_once(']') else {
                return Err(format!("{line_no}행: 표 머리를 읽지 못했다: {t}"));
            };
            if !trailing_ok(tail) {
                return Err(format!(
                    "{line_no}행: 표 머리 뒤에 읽을 수 없는 내용이 있다"
                ));
            }
            if name.trim() != "release" {
                return Err(format!("{line_no}행: 모르는 표 [{}]", name.trim()));
            }
            if doc.release.is_some() {
                return Err(format!("{line_no}행: [release]가 두 번 나왔다"));
            }
            doc.release = Some((line_no, Pairs::new()));
            section = Section::Release;
            continue;
        }
        let Some((key, raw)) = t.split_once('=') else {
            return Err(format!("{line_no}행: key = value 형식이 아니다: {t}"));
        };
        let key = key.trim();
        if key.is_empty()
            || !key
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
        {
            return Err(format!("{line_no}행: 읽지 않는 키 형식: {key}"));
        }
        let value = parse_value(raw.trim(), line_no)?;
        let target = match section {
            Section::Root => &mut doc.root,
            Section::Release => &mut doc.release.as_mut().expect("release section").1,
            Section::Plugin => &mut doc.plugins.last_mut().expect("plugin section").pairs,
            Section::Artifact => {
                &mut doc
                    .plugins
                    .last_mut()
                    .expect("plugin section")
                    .artifacts
                    .last_mut()
                    .expect("artifact section")
                    .1
            }
        };
        if target.insert(key.to_string(), (line_no, value)).is_some() {
            return Err(format!("{line_no}행: 키 {key}가 같은 표에서 두 번 나왔다"));
        }
    }
    Ok(doc)
}

struct Fields<'a> {
    pairs: Pairs,
    whence: &'a str,
    errors: Vec<String>,
}

impl<'a> Fields<'a> {
    fn new(pairs: Pairs, whence: &'a str) -> Self {
        Self {
            pairs,
            whence,
            errors: Vec::new(),
        }
    }

    fn string(&mut self, key: &str) -> String {
        match self.pairs.remove(key) {
            Some((_, Value::Str(s))) => s,
            Some((line, other)) => {
                self.errors.push(format!(
                    "{line}행: {}.{key}는 문자열이어야 한다: {other:?}",
                    self.whence
                ));
                String::new()
            }
            None => {
                self.errors.push(format!("{}.{key}가 없다", self.whence));
                String::new()
            }
        }
    }

    fn int(&mut self, key: &str) -> i64 {
        match self.pairs.remove(key) {
            Some((_, Value::Int(n))) => n,
            Some((line, other)) => {
                self.errors.push(format!(
                    "{line}행: {}.{key}는 정수여야 한다: {other:?}",
                    self.whence
                ));
                0
            }
            None => {
                self.errors.push(format!("{}.{key}가 없다", self.whence));
                0
            }
        }
    }

    fn boolean(&mut self, key: &str) -> bool {
        match self.pairs.remove(key) {
            Some((_, Value::Bool(b))) => b,
            Some((line, other)) => {
                self.errors.push(format!(
                    "{line}행: {}.{key}는 불리언이어야 한다: {other:?}",
                    self.whence
                ));
                false
            }
            None => {
                self.errors.push(format!("{}.{key}가 없다", self.whence));
                false
            }
        }
    }

    fn finish(mut self, out: &mut Vec<String>) {
        for (key, (line, _)) in std::mem::take(&mut self.pairs) {
            self.errors
                .push(format!("{line}행: {}에 모르는 키 {key}", self.whence));
        }
        out.append(&mut self.errors);
    }
}

fn lock_from_doc(doc: RawDoc, with_artifacts: bool) -> Result<Lock, Vec<String>> {
    let mut errors = Vec::new();
    let mut root = Fields::new(doc.root, "(최상위)");
    let schema = root.int("schema");
    root.finish(&mut errors);

    let Some((_, release_pairs)) = doc.release else {
        errors.push("[release]가 없다".to_string());
        return Err(errors);
    };
    let mut r = Fields::new(release_pairs, "release");
    let release = Release {
        repository: r.string("repository"),
        tag: r.string("tag"),
        commit: r.string("commit"),
        tasty_rev: r.string("tasty_rev"),
        host_api_version: r.string("host_api_version"),
        protocol_version: r.string("protocol_version"),
        manifest_version: r.int("manifest_version"),
        epaint_version: r.string("epaint_version"),
        epaint_patch_tree: r.string("epaint_patch_tree"),
    };
    r.finish(&mut errors);

    let mut plugins = Vec::new();
    for raw in doc.plugins {
        let whence = format!("{}행의 plugin", raw.line);
        let mut p = Fields::new(raw.pairs, &whence);
        let id = p.string("id");
        let version = p.string("version");
        let bundle = p.boolean("bundle");
        p.finish(&mut errors);
        if !with_artifacts && !raw.artifacts.is_empty() {
            errors.push(format!(
                "{}행: 릴리스 메타데이터에는 [[plugin.artifact]]가 없어야 한다",
                raw.artifacts[0].0
            ));
        }
        let mut artifacts = Vec::new();
        for (line, pairs) in raw.artifacts {
            let whence = format!("{line}행의 {id} artifact");
            let mut a = Fields::new(pairs, &whence);
            artifacts.push(Artifact {
                platform: a.string("platform"),
                file: a.string("file"),
                url: a.string("url"),
                sha256: a.string("sha256"),
            });
            a.finish(&mut errors);
        }
        plugins.push(Plugin {
            id,
            version,
            bundle,
            artifacts,
        });
    }
    if errors.is_empty() {
        Ok(Lock {
            schema,
            release,
            plugins,
        })
    } else {
        Err(errors)
    }
}

/// 잠금 파일 원문을 읽는다. 형식 오류는 줄 번호와 함께 모두 돌려준다.
/// 형식만 본다. 값의 완비·형태는 [`check`]가 본다.
pub fn parse_lock(text: &str) -> Result<Lock, Vec<String>> {
    lock_from_doc(parse_doc(text).map_err(|e| vec![e])?, true)
}

/// 공식 플러그인 레포 릴리스의 `release-meta.toml`을 읽는다. 산출물 표는 없다.
pub fn parse_release_meta(text: &str) -> Result<Lock, Vec<String>> {
    lock_from_doc(parse_doc(text).map_err(|e| vec![e])?, false)
}

fn quote(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

/// 잠금 파일 원문을 만든다. 같은 [`Lock`]이면 같은 바이트를 낸다.
pub fn render(lock: &Lock) -> String {
    let r = &lock.release;
    let mut out = String::new();
    out.push_str(
        "# 공식 플러그인 잠금 파일. 손으로 고치지 않는다. official-plugins generate 가 만든다.\n\
         # 형식과 갱신 절차: docs/dev-guide/plugin-packaging.md 의 \"공식 플러그인 잠금 파일\" 절.\n",
    );
    out.push_str(&format!("schema = {}\n\n[release]\n", lock.schema));
    for (k, v) in [
        ("repository", &r.repository),
        ("tag", &r.tag),
        ("commit", &r.commit),
        ("tasty_rev", &r.tasty_rev),
        ("host_api_version", &r.host_api_version),
        ("protocol_version", &r.protocol_version),
    ] {
        out.push_str(&format!("{k} = {}\n", quote(v)));
    }
    out.push_str(&format!("manifest_version = {}\n", r.manifest_version));
    out.push_str(&format!("epaint_version = {}\n", quote(&r.epaint_version)));
    out.push_str(&format!(
        "epaint_patch_tree = {}\n",
        quote(&r.epaint_patch_tree)
    ));
    for p in &lock.plugins {
        out.push_str(&format!(
            "\n[[plugin]]\nid = {}\nversion = {}\nbundle = {}\n",
            quote(&p.id),
            quote(&p.version),
            p.bundle
        ));
        for a in &p.artifacts {
            out.push_str(&format!(
                "\n  [[plugin.artifact]]\n  platform = {}\n  file = {}\n  url = {}\n  sha256 = {}\n",
                quote(&a.platform),
                quote(&a.file),
                quote(&a.url),
                quote(&a.sha256)
            ));
        }
    }
    out
}

fn is_hex(s: &str, len: usize) -> bool {
    s.len() == len
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn is_semver_core(s: &str) -> bool {
    let parts: Vec<&str> = s.split('.').collect();
    parts.len() == 3
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
}

/// 릴리스 산출물 파일 이름. 공식 플러그인 레포의 패키징 규칙과 같다.
pub fn artifact_file(id: &str, version: &str, platform: &str) -> String {
    format!("{id}-{version}-{platform}.tar.gz")
}

/// 릴리스 산출물 URL.
pub fn artifact_url(release: &Release, file: &str) -> String {
    format!(
        "https://github.com/{}/releases/download/{}/{file}",
        release.repository, release.tag
    )
}

/// 아카이브 안 `<id>/` 아래 실행 파일 이름. Windows 플랫폼은 `.exe`가 붙는다.
pub fn executable_name(id: &str, platform: &str) -> Option<String> {
    let suffix = id.strip_prefix(ID_PREFIX)?;
    let exe = if platform.starts_with("windows-") {
        ".exe"
    } else {
        ""
    };
    Some(format!("tasty-plugin-{suffix}{exe}"))
}

/// schema와 `[release]` 값의 형태를 검사한다.
fn check_release(lock: &Lock) -> Vec<String> {
    let mut problems = Vec::new();
    let r = &lock.release;
    if lock.schema != SCHEMA {
        problems.push(format!(
            "schema = {}, 읽을 수 있는 값은 {SCHEMA}",
            lock.schema
        ));
    }
    let repo_ok = r.repository.split('/').count() == 2
        && r.repository.split('/').all(|p| {
            !p.is_empty()
                && p.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.')
        });
    if !repo_ok {
        problems.push(format!(
            "release.repository가 <owner>/<name>이 아니다: {}",
            r.repository
        ));
    }
    if !is_safe_tag(&r.tag) {
        problems.push(format!(
            "release.tag가 [A-Za-z0-9._-]로만 된 이름이 아니거나 . 또는 ..이다: {:?}",
            r.tag
        ));
    }
    for (name, value) in [
        ("commit", &r.commit),
        ("tasty_rev", &r.tasty_rev),
        ("epaint_patch_tree", &r.epaint_patch_tree),
    ] {
        if !is_hex(value, 40) {
            problems.push(format!(
                "release.{name}가 40자리 소문자 hex가 아니다: {value}"
            ));
        }
    }
    for (name, value) in [
        ("host_api_version", &r.host_api_version),
        ("protocol_version", &r.protocol_version),
        ("epaint_version", &r.epaint_version),
    ] {
        if value.is_empty() {
            problems.push(format!("release.{name}가 비었다"));
        }
    }
    problems
}

/// 받는 스크립트가 tag·id·file을 경로 조각으로 쓰므로 경로를 벗어날 수 있는 값은 받지 않는다.
fn is_safe_tag(tag: &str) -> bool {
    !tag.is_empty()
        && tag != "."
        && tag != ".."
        && tag
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'_' || b == b'-')
}

/// `com.tasty.` 뒤에 `[a-z0-9-]+`가 오는 id.
fn is_official_id(id: &str) -> bool {
    id.strip_prefix(ID_PREFIX).is_some_and(|rest| {
        !rest.is_empty()
            && rest
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
    })
}

/// 경로 구분자(`/`, `\`)와 `..`가 없는 단일 파일 이름.
fn is_single_file_name(file: &str) -> bool {
    !file.is_empty() && file != "." && !file.contains(['/', '\\']) && !file.contains("..")
}

/// 잠금 파일의 값과 완비를 검사한다. 비어 있으면 통과다.
/// 모든 플러그인(`bundle = false` 포함)에 5플랫폼 산출물이 하나씩 있어야 하고,
/// 파일 이름·URL은 id·version·플랫폼·릴리스에서 계산한 값과 같아야 한다.
pub fn check(lock: &Lock) -> Vec<String> {
    let mut problems = check_release(lock);
    let r = &lock.release;
    if lock.plugins.is_empty() {
        problems.push("[[plugin]]이 하나도 없다".to_string());
    }
    let mut seen = BTreeSet::new();
    for p in &lock.plugins {
        if !seen.insert(p.id.as_str()) {
            problems.push(format!("plugin {}가 두 번 나왔다", p.id));
        }
        if !is_official_id(&p.id) {
            problems.push(format!(
                "plugin id가 {ID_PREFIX} 뒤에 [a-z0-9-]만 오는 이름이 아니다: {:?}",
                p.id
            ));
        }
        if !is_semver_core(&p.version) {
            problems.push(format!(
                "{} version이 MAJOR.MINOR.PATCH가 아니다: {}",
                p.id, p.version
            ));
        }
        let mut platforms = BTreeSet::new();
        for a in &p.artifacts {
            if !PLATFORMS.contains(&a.platform.as_str()) {
                problems.push(format!("{}: 모르는 플랫폼 {}", p.id, a.platform));
            }
            if !platforms.insert(a.platform.as_str()) {
                problems.push(format!("{}: 플랫폼 {}가 두 번 나왔다", p.id, a.platform));
            }
            if !is_single_file_name(&a.file) {
                problems.push(format!(
                    "{} {}: file이 단일 파일 이름이 아니다: {:?}",
                    p.id, a.platform, a.file
                ));
            }
            let file = artifact_file(&p.id, &p.version, &a.platform);
            if a.file != file {
                problems.push(format!(
                    "{} {}: file이 {file}이 아니다: {}",
                    p.id, a.platform, a.file
                ));
            }
            let url = artifact_url(r, &a.file);
            if a.url != url {
                problems.push(format!(
                    "{} {}: url이 {url}이 아니다: {}",
                    p.id, a.platform, a.url
                ));
            }
            if !is_hex(&a.sha256, 64) {
                problems.push(format!(
                    "{} {}: sha256이 64자리 소문자 hex가 아니다: {}",
                    p.id, a.platform, a.sha256
                ));
            }
        }
        for platform in PLATFORMS {
            if !platforms.contains(platform) {
                problems.push(format!("{}: 플랫폼 {platform} 산출물이 없다", p.id));
            }
        }
    }
    problems
}

/// 릴리스 메타데이터와 TSV 5개로 잠금 파일을 만든다. 실패하면 문제를 모두 돌려준다.
/// TSV마다 머리줄이 [`TSV_HEADER`]여야 하고, 각 TSV의 (id, version) 집합은 메타데이터의
/// 플러그인 집합과 같아야 한다. 결과는 [`check`]를 통과해야 돌려준다.
pub fn generate(meta_text: &str, tsvs: &[(String, String)]) -> Result<Lock, Vec<String>> {
    let meta = parse_release_meta(meta_text)?;
    let mut errors = Vec::new();
    let expected: BTreeSet<(String, String)> = meta
        .plugins
        .iter()
        .map(|p| (p.id.clone(), p.version.clone()))
        .collect();
    let mut rows: BTreeMap<(String, String), (String, String)> = BTreeMap::new();
    let mut tsv_platforms = BTreeSet::new();
    for (name, text) in tsvs {
        let mut lines = text.lines();
        if lines.next().map(str::trim_end) != Some(TSV_HEADER) {
            errors.push(format!("{name}: 머리줄이 {TSV_HEADER:?}가 아니다"));
            continue;
        }
        let mut pairs = BTreeSet::new();
        let mut platforms = BTreeSet::new();
        for (idx, line) in lines.enumerate() {
            let line = line.trim_end_matches('\r');
            if line.is_empty() {
                continue;
            }
            let cols: Vec<&str> = line.split('\t').collect();
            let [id, version, platform, file, sha] = cols[..] else {
                errors.push(format!("{name} {}행: 열이 5개가 아니다", idx + 2));
                continue;
            };
            pairs.insert((id.to_string(), version.to_string()));
            platforms.insert(platform.to_string());
            if rows
                .insert(
                    (id.to_string(), platform.to_string()),
                    (file.to_string(), sha.to_string()),
                )
                .is_some()
            {
                errors.push(format!("{name}: {id} {platform}가 두 번 나왔다"));
            }
        }
        if platforms.len() != 1 {
            errors.push(format!("{name}: 플랫폼이 하나가 아니다: {platforms:?}"));
        }
        for platform in &platforms {
            if !tsv_platforms.insert(platform.clone()) {
                errors.push(format!("플랫폼 {platform}의 TSV가 둘 이상이다"));
            }
        }
        if pairs != expected {
            let only_tsv: Vec<_> = pairs.difference(&expected).collect();
            let only_meta: Vec<_> = expected.difference(&pairs).collect();
            errors.push(format!(
                "{name}의 (id, version)이 release-meta와 다르다. TSV에만: {only_tsv:?}, 메타데이터에만: {only_meta:?}"
            ));
        }
    }
    let want: BTreeSet<String> = PLATFORMS.iter().map(|p| p.to_string()).collect();
    if tsv_platforms != want {
        errors.push(format!(
            "TSV 플랫폼 집합이 {PLATFORMS:?}가 아니다: {tsv_platforms:?}"
        ));
    }
    if !errors.is_empty() {
        return Err(errors);
    }
    let mut lock = meta;
    for p in &mut lock.plugins {
        for platform in PLATFORMS {
            let Some((file, sha)) = rows.get(&(p.id.clone(), platform.to_string())) else {
                errors.push(format!("{} {platform} 행이 없다", p.id));
                continue;
            };
            p.artifacts.push(Artifact {
                platform: platform.to_string(),
                file: file.clone(),
                url: artifact_url(&lock.release, file),
                sha256: sha.clone(),
            });
        }
    }
    errors.extend(check(&lock));
    if errors.is_empty() {
        Ok(lock)
    } else {
        Err(errors)
    }
}

/// 받을 산출물 하나.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanItem {
    pub id: String,
    pub version: String,
    pub file: String,
    pub url: String,
    pub sha256: String,
    pub executable: String,
}

/// 한 플랫폼에서 받을 산출물. `all`이 거짓이면 `bundle = true`만 고른다.
pub fn plan(lock: &Lock, platform: &str, all: bool) -> Result<Vec<PlanItem>, String> {
    if !PLATFORMS.contains(&platform) {
        return Err(format!(
            "모르는 플랫폼 {platform}. 가능한 값: {PLATFORMS:?}"
        ));
    }
    let mut items = Vec::new();
    for p in lock.plugins.iter().filter(|p| all || p.bundle) {
        let Some(a) = p.artifacts.iter().find(|a| a.platform == platform) else {
            return Err(format!("{}에 {platform} 산출물이 없다", p.id));
        };
        let executable = executable_name(&p.id, platform)
            .ok_or_else(|| format!("{}가 {ID_PREFIX}로 시작하지 않는다", p.id))?;
        items.push(PlanItem {
            id: p.id.clone(),
            version: p.version.clone(),
            file: a.file.clone(),
            url: a.url.clone(),
            sha256: a.sha256.clone(),
            executable,
        });
    }
    Ok(items)
}

/// 잠금 파일과 대조할 현재 tasty의 계약 값.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostContract {
    pub host_api_version: String,
    pub protocol_version: String,
    pub manifest_version: i64,
    pub epaint_version: String,
    pub epaint_patch_tree: String,
}

/// 잠금 파일의 계약 값이 현재 tasty와 다른 축을 모두 돌려준다. 비어 있으면 통과다.
/// 버전 값만 비교하므로 버전을 올리지 않은 protocol schema 변경은 잡지 못한다.
pub fn contract_mismatches(release: &Release, host: &HostContract) -> Vec<String> {
    let mut out = Vec::new();
    let mut cmp = |axis: &str, locked: String, current: String, source: &str| {
        if locked != current {
            out.push(format!("{axis}: 잠금 {locked} ≠ 현재 {current} ({source})"));
        }
    };
    cmp(
        "host_api_version",
        release.host_api_version.clone(),
        host.host_api_version.clone(),
        "crates/tasty-plugin-manifest/src/types.rs HOST_API_VERSION",
    );
    cmp(
        "protocol_version",
        release.protocol_version.clone(),
        host.protocol_version.clone(),
        "crates/tasty-plugin-protocol/Cargo.toml version",
    );
    cmp(
        "manifest_version",
        release.manifest_version.to_string(),
        host.manifest_version.to_string(),
        "crates/tasty-plugin-manifest/src/types.rs MANIFEST_VERSION",
    );
    cmp(
        "epaint_version",
        release.epaint_version.clone(),
        host.epaint_version.clone(),
        "Cargo.lock epaint",
    );
    cmp(
        "epaint_patch_tree",
        release.epaint_patch_tree.clone(),
        host.epaint_patch_tree.clone(),
        "git rev-parse HEAD:vendor/epaint",
    );
    out
}

/// `pub const <name>: <type> = <value>;` 줄의 값을 원문 그대로 돌려준다. 정의가 하나여야 한다.
pub fn rust_const_value(src: &str, name: &str) -> Result<String, String> {
    let needle = format!("pub const {name}:");
    let hits: Vec<&str> = src
        .lines()
        .map(str::trim)
        .filter(|l| l.starts_with(&needle))
        .collect();
    let [line] = hits[..] else {
        return Err(format!("`{needle}` 정의가 {}개다", hits.len()));
    };
    let value = line
        .split_once('=')
        .map(|(_, v)| v.trim().trim_end_matches(';').trim())
        .ok_or_else(|| format!("`{line}`에서 값을 읽지 못했다"))?;
    Ok(value.trim_matches('"').to_string())
}

/// Cargo 매니페스트 `[package]`의 version.
pub fn package_version(cargo_toml: &str) -> Result<String, String> {
    let mut in_package = false;
    for line in cargo_toml.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            in_package = t == "[package]";
            continue;
        }
        if in_package
            && let Some((k, v)) = t.split_once('=')
            && k.trim() == "version"
        {
            return Ok(v.trim().trim_matches('"').to_string());
        }
    }
    Err("[package] version을 찾지 못했다".to_string())
}

/// Cargo.lock에서 이름이 `name`인 패키지의 version. 판이 둘 이상이면 실패한다.
pub fn lock_package_version(cargo_lock: &str, name: &str) -> Result<String, String> {
    let target = format!("name = \"{name}\"");
    let mut versions = Vec::new();
    let mut lines = cargo_lock.lines();
    while let Some(line) = lines.next() {
        if line.trim() == target
            && let Some(next) = lines.next()
            && let Some(v) = next.trim().strip_prefix("version = ")
        {
            versions.push(v.trim_matches('"').to_string());
        }
    }
    match &versions[..] {
        [one] => Ok(one.clone()),
        _ => Err(format!(
            "Cargo.lock의 {name} 판이 하나가 아니다: {versions:?}"
        )),
    }
}

/// 저장소에서 현재 계약 값을 읽는다. `epaint_patch_tree`는 `git rev-parse HEAD:vendor/epaint`다.
pub fn read_host_contract(root: &Path) -> Result<HostContract, String> {
    let read = |rel: &str| {
        std::fs::read_to_string(root.join(rel)).map_err(|e| format!("{rel}를 읽지 못했다: {e}"))
    };
    let types = read("crates/tasty-plugin-manifest/src/types.rs")?;
    let manifest_version = rust_const_value(&types, "MANIFEST_VERSION")?
        .parse::<i64>()
        .map_err(|e| format!("MANIFEST_VERSION을 정수로 읽지 못했다: {e}"))?;
    let out = crate::git_env::command("git")
        .current_dir(root)
        .args(["rev-parse", "HEAD:vendor/epaint"])
        .output()
        .map_err(|e| format!("git rev-parse를 실행하지 못했다: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "git rev-parse HEAD:vendor/epaint 실패: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(HostContract {
        host_api_version: rust_const_value(&types, "HOST_API_VERSION")?,
        protocol_version: package_version(&read("crates/tasty-plugin-protocol/Cargo.toml")?)?,
        manifest_version,
        epaint_version: lock_package_version(&read("Cargo.lock")?, "epaint")?,
        epaint_patch_tree: String::from_utf8_lossy(&out.stdout).trim().to_string(),
    })
}

/// 플러그인 매니페스트의 최상위 문자열 키(첫 표 머리 앞)를 읽는다.
/// 받은 매니페스트의 id·version을 잠금 파일과 대조하는 데만 쓴다. 전체 검증은
/// `tasty-plugin-manifest`의 `validate-plugin-dirs` 예제가 한다.
pub fn manifest_top_level_str(text: &str, key: &str) -> Option<String> {
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            return None;
        }
        if let Some((k, v)) = t.split_once('=')
            && k.trim() == key
        {
            let v = v.trim();
            let inner = v.strip_prefix('"')?;
            return parse_string(inner, 0).ok().map(|(s, _)| s);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    const META: &str = r#"# comment
schema = 1

[release]
repository = "owner/plugins"
tag = "v0.1.0"
commit = "ba234e6e4bc3a7789517ce38cc22a7341f8719e9"
tasty_rev = "e277fce41d9a2db8bf3c14fda8f56dd6415bfcbc"
host_api_version = "1"
protocol_version = "0.2.16"
manifest_version = 1
epaint_version = "0.31.1"
epaint_patch_tree = "37251c43b1d13d12fb6d4c81f3589ace67a0ac12"

[[plugin]]
id = "com.tasty.a"
version = "0.2.0"
bundle = false

[[plugin]]
id = "com.tasty.b"
version = "0.3.0"
bundle = true
"#;

    fn sha(n: u8) -> String {
        format!("{:064x}", n)
    }

    fn tsvs() -> Vec<(String, String)> {
        PLATFORMS
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let mut t = format!("{TSV_HEADER}\n");
                for (j, (id, v)) in [("com.tasty.a", "0.2.0"), ("com.tasty.b", "0.3.0")]
                    .iter()
                    .enumerate()
                {
                    t.push_str(&format!(
                        "{id}\t{v}\t{p}\t{}\t{}\n",
                        artifact_file(id, v, p),
                        sha((i * 2 + j) as u8 + 1)
                    ));
                }
                (format!("plugins-{p}.tsv"), t)
            })
            .collect()
    }

    #[test]
    fn generate_round_trips_through_render_and_parse() {
        let lock = generate(META, &tsvs()).expect("generate");
        assert_eq!(lock.plugins.len(), 2);
        assert!(lock.plugins.iter().all(|p| p.artifacts.len() == 5));
        assert_eq!(
            lock.plugins[1].artifacts[3].url,
            "https://github.com/owner/plugins/releases/download/v0.1.0/com.tasty.b-0.3.0-linux-x64.tar.gz"
        );
        let text = render(&lock);
        let back = parse_lock(&text).expect("parse rendered");
        assert_eq!(back, lock);
        assert!(check(&back).is_empty(), "{:?}", check(&back));
        assert_eq!(render(&back), text);
    }

    #[test]
    fn generate_rejects_a_tsv_whose_versions_differ_from_the_metadata() {
        let mut t = tsvs();
        t[2].1 = t[2].1.replace("0.3.0", "0.3.1");
        let errors = generate(META, &t).expect_err("must fail");
        assert!(
            errors.iter().any(|e| e.contains("release-meta와 다르다")),
            "{errors:?}"
        );
    }

    #[test]
    fn generate_rejects_a_missing_platform_tsv() {
        let mut t = tsvs();
        t.remove(0);
        let errors = generate(META, &t).expect_err("must fail");
        assert!(
            errors.iter().any(|e| e.contains("TSV 플랫폼 집합")),
            "{errors:?}"
        );
    }

    #[test]
    fn check_reports_a_missing_platform_and_an_unknown_one() {
        let mut lock = generate(META, &tsvs()).expect("generate");
        lock.plugins[0].artifacts.remove(0);
        lock.plugins[1].artifacts[0].platform = "plan9-x64".to_string();
        let problems = check(&lock);
        assert!(
            problems
                .iter()
                .any(|p| p.contains("com.tasty.a: 플랫폼 macos-arm64 산출물이 없다")),
            "{problems:?}"
        );
        assert!(
            problems
                .iter()
                .any(|p| p.contains("모르는 플랫폼 plan9-x64")),
            "{problems:?}"
        );
    }

    #[test]
    fn check_reports_a_bad_sha_and_a_foreign_url() {
        let mut lock = generate(META, &tsvs()).expect("generate");
        lock.plugins[0].artifacts[1].sha256 = "abc".to_string();
        lock.plugins[1].artifacts[2].url = "https://example.com/x.tar.gz".to_string();
        let problems = check(&lock);
        assert!(
            problems.iter().any(|p| p.contains("sha256이 64자리")),
            "{problems:?}"
        );
        assert!(problems.iter().any(|p| p.contains("url이")), "{problems:?}");
    }

    /// id·tag를 바꾸고 file·url을 그 값으로 다시 계산한다. 계산 값과의 대조만으로는 걸리지 않는 입력을 만든다.
    fn relabel(lock: &mut Lock, plugin: usize, id: &str, tag: &str) {
        lock.release.tag = tag.to_string();
        let release = lock.release.clone();
        let p = &mut lock.plugins[plugin];
        p.id = id.to_string();
        for a in &mut p.artifacts {
            a.file = artifact_file(id, &p.version, &a.platform);
            a.url = artifact_url(&release, &a.file);
        }
    }

    #[test]
    fn check_rejects_an_id_with_a_path_separator() {
        let mut lock = generate(META, &tsvs()).expect("generate");
        relabel(&mut lock, 0, "com.tasty.x/../../escape", "v0.1.0");
        let problems = check(&lock);
        assert!(
            problems.iter().any(|p| p.contains("plugin id가")),
            "{problems:?}"
        );
        assert!(
            problems
                .iter()
                .any(|p| p.contains("단일 파일 이름이 아니다")),
            "{problems:?}"
        );
    }

    #[test]
    fn check_rejects_a_dot_dot_tag() {
        for tag in ["..", ".", "v0.1.0/..", "v 1"] {
            let mut lock = generate(META, &tsvs()).expect("generate");
            relabel(&mut lock, 0, "com.tasty.a", tag);
            relabel(&mut lock, 1, "com.tasty.b", tag);
            let problems = check(&lock);
            assert!(
                problems.iter().any(|p| p.contains("release.tag가")),
                "{tag}: {problems:?}"
            );
        }
    }

    #[test]
    fn check_rejects_a_backslash_in_an_artifact_file() {
        let mut lock = generate(META, &tsvs()).expect("generate");
        relabel(&mut lock, 1, "com.tasty.b\\..\\x", "v0.1.0");
        let problems = check(&lock);
        assert!(
            problems.iter().any(|p| p.contains("plugin id가")),
            "{problems:?}"
        );
        assert!(
            problems
                .iter()
                .any(|p| p.contains("단일 파일 이름이 아니다")),
            "{problems:?}"
        );
        assert!(
            !problems.iter().any(|p| p.contains("com.tasty.a")),
            "{problems:?}"
        );
    }

    #[test]
    fn parse_rejects_what_the_subset_does_not_read() {
        for (text, needle) in [
            ("schema = 1\nlist = [1, 2]\n", "문자열·정수·불리언"),
            ("schema = 1\n[other]\n", "모르는 표"),
            ("schema = 1\n[[plugin.artifact]]\n", "[[plugin]] 앞에"),
            ("schema = 1\nschema = 2\n", "두 번"),
            ("schema = 1\nx = \"a\\nb\"\n", "이스케이프"),
        ] {
            let errors = parse_lock(text).expect_err(text);
            assert!(
                errors.iter().any(|e| e.contains(needle)),
                "{text}: {errors:?}"
            );
        }
    }

    #[test]
    fn parse_reports_unknown_and_missing_keys() {
        let text = META.replace("tag = \"v0.1.0\"", "taag = \"v0.1.0\"");
        let errors = parse_release_meta(&text).expect_err("must fail");
        assert!(
            errors.iter().any(|e| e.contains("release.tag가 없다")),
            "{errors:?}"
        );
        assert!(
            errors.iter().any(|e| e.contains("모르는 키 taag")),
            "{errors:?}"
        );
    }

    #[test]
    fn plan_filters_unbundled_plugins_unless_all() {
        let lock = generate(META, &tsvs()).expect("generate");
        let bundled = plan(&lock, "windows-x64", false).expect("plan");
        assert_eq!(bundled.len(), 1);
        assert_eq!(bundled[0].id, "com.tasty.b");
        assert_eq!(bundled[0].executable, "tasty-plugin-b.exe");
        let all = plan(&lock, "linux-arm64", true).expect("plan all");
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].executable, "tasty-plugin-a");
        assert!(plan(&lock, "plan9-x64", true).is_err());
    }

    #[test]
    fn contract_mismatch_names_every_differing_axis() {
        let lock = generate(META, &tsvs()).expect("generate");
        let host = HostContract {
            host_api_version: "1".into(),
            protocol_version: "0.2.17".into(),
            manifest_version: 1,
            epaint_version: "0.31.1".into(),
            epaint_patch_tree: "37251c43b1d13d12fb6d4c81f3589ace67a0ac12".into(),
        };
        let m = contract_mismatches(&lock.release, &host);
        assert_eq!(m.len(), 1, "{m:?}");
        assert!(m[0].starts_with("protocol_version"), "{m:?}");
    }

    #[test]
    fn source_readers_take_exactly_one_definition() {
        let src =
            "pub const HOST_API_VERSION: &str = \"1\";\npub const MANIFEST_VERSION: u32 = 1;\n";
        assert_eq!(rust_const_value(src, "HOST_API_VERSION").unwrap(), "1");
        assert_eq!(rust_const_value(src, "MANIFEST_VERSION").unwrap(), "1");
        let twice = format!("{src}{src}");
        assert!(rust_const_value(&twice, "MANIFEST_VERSION").is_err());
        let lock = "[[package]]\nname = \"epaint\"\nversion = \"0.31.1\"\n";
        assert_eq!(lock_package_version(lock, "epaint").unwrap(), "0.31.1");
        let two = format!("{lock}[[package]]\nname = \"epaint\"\nversion = \"0.32.0\"\n");
        assert!(lock_package_version(&two, "epaint").is_err());
        let cargo =
            "[package]\nname = \"x\"\nversion = \"0.2.16\"\n[dependencies]\nversion = \"9\"\n";
        assert_eq!(package_version(cargo).unwrap(), "0.2.16");
    }

    #[test]
    fn manifest_reader_stops_at_the_first_table() {
        let m =
            "manifest_version = 1\nid = \"com.tasty.a\"\nversion = \"0.2.0\"\n[[x]]\nid = \"no\"\n";
        assert_eq!(
            manifest_top_level_str(m, "id").as_deref(),
            Some("com.tasty.a")
        );
        assert_eq!(
            manifest_top_level_str(m, "version").as_deref(),
            Some("0.2.0")
        );
        assert_eq!(manifest_top_level_str("[x]\nid = \"no\"\n", "id"), None);
    }
}
