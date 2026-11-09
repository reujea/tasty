#![forbid(unsafe_code)]

//! Shared translations for the host and CLI, initialized once at startup.
//! general.language selects the language; changing it requires a restart.
//! Lookups before [`init`] read the embedded English table, so a value read
//! before and after an English `init` is the same string.
//!
//! Built-in en/ko/ja/zh-Hans text is embedded in the binary. A user `<code>.toml` file
//! overrides a built-in language. Other languages need `<code>/pack.toml` with
//! a valid font declaration. Missing or invalid packs fall back to English
//! without changing the setting; LoadReport records the reason.
//!
//! Plugin namespaces can be registered and removed at runtime. Values use
//! Box::leak to satisfy the static lookup lifetime, so replacing or removing a
//! namespace does not free old strings. MAX_PACK_BYTES bounds individual user
//! files, not total retained strings or installed plugin catalogs.

pub mod font;
pub mod plugin_catalog;
pub mod plural;

pub use plural::{t_count, t_count_template};

use std::collections::HashMap;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::{OnceLock, RwLock};

use tasty_utils::path::tasty_home;

/// Global translation store, initialized once at startup.
static TRANSLATIONS: OnceLock<Translations> = OnceLock::new();
/// Embedded English for lookups before [`init`]. It skips the user `en.toml`
/// override so that a lookup never reads files before the language is chosen.
static BUILTIN_EN: OnceLock<Translations> = OnceLock::new();
/// Whether the first lookup before [`init`] was already logged.
static PRE_INIT_READ_LOGGED: AtomicBool = AtomicBool::new(false);
/// What [`init`] decided at boot (requested vs. effective language). The GUI
/// reads it once after boot to surface the English-fallback warning as a toast.
static LOAD_REPORT: OnceLock<LoadReport> = OnceLock::new();

/// 내장 코드 목록과 include_str! 경로를 한 선언에서 만든다.
/// include_str!이 요구하는 리터럴 경로는 concat!으로 구성한다.
macro_rules! builtin_languages {
    ($($code:literal),+ $(,)?) => {
        /// Language codes embedded in the binary (`lang/{code}.toml`).
        /// These never need a pack directory.
        pub const BUILTIN_CODES: [&str; [$($code),+].len()] = [$($code),+];

        fn builtin_toml(code: &str) -> Option<&'static str> {
            match code {
                $($code => Some(include_str!(concat!("../../../lang/", $code, ".toml"))),)+
                _ => None,
            }
        }
    };
}

builtin_languages!("en", "ko", "ja", "zh-Hans");
/// Manifest file name inside a language pack directory.
pub const PACK_FILE_NAME: &str = "pack.toml";

/// 홈을 찾지 못했을 때 안내에 쓰는 표기. 실제로 읽는 경로가 아니다.
const UNKNOWN_LANG_DIR: &str = "<tasty home>/lang";

/// Whether `code` is one of the embedded languages.
pub fn is_builtin_code(code: &str) -> bool {
    BUILTIN_CODES.contains(&code)
}

/// The user language directory (`~/.tasty/lang`). `None` when no data root can
/// be determined (no home directory).
pub fn user_lang_dir() -> Option<PathBuf> {
    tasty_home().map(|dir| dir.join("lang"))
}

/// Path of the pack manifest for `code` under `lang_dir`.
pub fn pack_path(lang_dir: &Path, code: &str) -> PathBuf {
    lang_dir.join(code).join(PACK_FILE_NAME)
}

/// A language code usable as a directory name, env value and settings value:
/// ASCII letters/digits/`-`/`_`, 1..=32 chars.
fn is_valid_code(code: &str) -> bool {
    !code.is_empty()
        && code.len() <= 32
        && code
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

// ── language pack types ─────────────────────────────────────────────────────

/// The mandatory `[font]` declaration of a language pack. Which glyph source the
/// pack expects; resolving it to an actual font (file lookup, system family
/// match) is the host's job, not this crate's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FontDecl {
    /// `builtin = true` — the pack declares that the built-in font stack already
    /// covers its script.
    Builtin,
    /// `file = "<path relative to the pack directory>"` — a bundled font file.
    File(String),
    /// `family = "<system font family name>"`.
    Family(String),
    /// `candidates = ["<file or family>", …]` — tried in order, first hit wins.
    Candidates(Vec<String>),
}

/// Where a listed language comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LanguageSource {
    /// Embedded in the binary.
    Builtin,
    /// Embedded, with a user `<code>.toml` override file present.
    BuiltinOverridden,
    /// A user language pack directory (`<code>/pack.toml`).
    Pack,
}

/// One row of the "available languages" list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LanguageEntry {
    pub code: String,
    /// `[meta] name` — `None` when the file has no usable name; callers fall
    /// back to the code ([`LanguageEntry::label`]).
    pub display_name: Option<String>,
    pub source: LanguageSource,
    /// The pack's `[font]` declaration. `None` for built-in languages (with or
    /// without an override) — they use the built-in font stack.
    pub font: Option<FontDecl>,
    /// Pack manifest (packs) or override file (overridden built-ins). `None` for
    /// a plain built-in.
    pub path: Option<PathBuf>,
}

impl LanguageEntry {
    /// Display label: the `[meta] name`, else the code itself.
    pub fn label(&self) -> &str {
        self.display_name.as_deref().unwrap_or(&self.code)
    }
}

/// Why a `pack.toml` was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PackError {
    Read(String),
    Parse(String),
    /// No `[font]` section — a shape violation, the pack is not loaded at all.
    MissingFont,
    /// `[font]` present but none of `builtin` / `file` / `family` / `candidates`
    /// is usable.
    InvalidFont(String),
    /// The manifest is bigger than [`MAX_PACK_BYTES`]. Rejected before parsing —
    /// see that constant for how the limit is set.
    TooLarge {
        bytes: u64,
        max: u64,
    },
}

impl fmt::Display for PackError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read(e) => write!(f, "cannot read: {e}"),
            Self::Parse(e) => write!(f, "invalid TOML: {e}"),
            Self::MissingFont => write!(f, "missing [font] section"),
            Self::InvalidFont(e) => write!(f, "invalid [font]: {e}"),
            Self::TooLarge { bytes, max } => write!(
                f,
                "too large: {} KiB (limit {} KiB)",
                bytes / 1024,
                max / 1024
            ),
        }
    }
}

/// One-line fallback reason for a toast. The log retains the full parse error.
fn summarize_reason(error: &PackError) -> String {
    /// Leave room for the pack path and the recovery instruction.
    const MAX_CHARS: usize = 40;
    let text = error.to_string();
    let first = text.lines().next().unwrap_or_default().trim();
    if first.chars().count() <= MAX_CHARS {
        return first.to_string();
    }
    let mut out: String = first.chars().take(MAX_CHARS).collect();
    out.push('…');
    out
}

/// Host toast character limit. Keep warnings within it so truncation does not remove the instruction.
pub const TOAST_MAX_CHARS: usize = 200;

/// Minimum useful fragment length. Callers must leave this much room in a toast template.
pub const MIN_FRAGMENT_CHARS: usize = 12;

/// Shorten only the variable fragment so the rendered message fits TOAST_MAX_CHARS.
/// render must insert its argument exactly once. Middle elision keeps both
/// the path prefix and trailing file name or OS error.
pub fn fit_fragment(fragment: &str, render: impl Fn(&str) -> String) -> String {
    let skeleton = render("").chars().count();
    let budget = TOAST_MAX_CHARS
        .saturating_sub(skeleton)
        .max(MIN_FRAGMENT_CHARS);
    render(&elide_middle(fragment, budget))
}

/// [`fit_fragment`] over a path.
fn fit_path(path: &Path, render: impl Fn(&str) -> String) -> String {
    fit_fragment(&path.display().to_string(), render)
}

/// [`fit_fragment`] for the common single-placeholder case — `t_fmt(key, ..)`
/// with the argument shrunk to fit the toast cap.
pub fn t_fmt_fit(key: &str, fragment: &str) -> String {
    fit_fragment(fragment, |f| t_fmt(key, f))
}

/// Keep both ends of s within max characters. About two thirds of the space
/// goes to the end, which often contains the file name or OS error.
fn elide_middle(s: &str, max: usize) -> String {
    let count = s.chars().count();
    if count <= max {
        return s.to_string();
    }
    if max <= 1 {
        return "\u{2026}".to_string();
    }
    let keep = max - 1;
    let head = keep / 3;
    let tail = keep - head;
    let chars: Vec<char> = s.chars().collect();
    let mut out: String = chars[..head].iter().collect();
    out.push('\u{2026}');
    out.extend(&chars[count - tail..]);
    out
}

/// A parsed, validated language pack manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LanguagePack {
    pub code: String,
    /// The `pack.toml` path.
    pub path: PathBuf,
    /// `[meta] name`, if present and non-empty.
    pub display_name: Option<String>,
    pub font: FontDecl,
    /// Flattened translation strings (everything except the `[font]` table).
    strings: HashMap<String, String>,
}

/// 발견 경로의 flatten 호출 여부를 확인하는 테스트 전용 계수기.
/// 스레드 로컬로 두어 다른 시험의 호출이 섞이지 않게 한다.
#[cfg(test)]
mod flatten_probe {
    use std::cell::Cell;

    thread_local! {
        static CALLS: Cell<usize> = const { Cell::new(0) };
    }

    pub(super) fn note() {
        CALLS.with(|c| c.set(c.get() + 1));
    }

    pub(super) fn reset() {
        CALLS.with(|c| c.set(0));
    }

    pub(super) fn count() -> usize {
        CALLS.with(Cell::get)
    }
}

/// TOML의 문자열 값을 점으로 연결한 키로 펼쳐 map에 추가·덮어쓴다.
/// 정수·불리언·배열·날짜는 제외한다. 로더와 키 정합 검사가 같은 함수를 사용한다.
pub fn flatten_catalog_toml(prefix: &str, value: &toml::Value, map: &mut HashMap<String, String>) {
    #[cfg(test)]
    flatten_probe::note();
    match value {
        toml::Value::Table(table) => {
            for (key, val) in table {
                let full_key = if prefix.is_empty() {
                    key.clone()
                } else {
                    format!("{}.{}", prefix, key)
                };
                flatten_catalog_toml(&full_key, val, map);
            }
        }
        toml::Value::String(s) => {
            map.insert(prefix.to_string(), s.clone());
        }
        _ => {}
    }
}

/// Maximum user language file size: 2 MiB. Larger files are rejected before TOML parsing.
/// This bounds each file read; it does not bound the number of packs, total
/// retained translations, or render-thread time. Raise it only when a valid
/// pack needs more space and the parsing cost has been checked.
pub const MAX_PACK_BYTES: u64 = 2 * 1024 * 1024;

/// Read at most MAX_PACK_BYTES + 1 bytes to detect an oversized file.
/// A metadata check rejects obviously large files early; Read::take still
/// enforces the bound if the file grows or does not report its size.
fn read_capped(path: &Path) -> Result<String, PackError> {
    use std::io::Read;
    let file = std::fs::File::open(path).map_err(|e| PackError::Read(e.to_string()))?;
    // 이미 큰 파일은 상한만큼 읽기 전에 거절한다.
    if let Ok(meta) = file.metadata()
        && meta.len() > MAX_PACK_BYTES
    {
        return Err(PackError::TooLarge {
            bytes: meta.len(),
            max: MAX_PACK_BYTES,
        });
    }
    let mut text = String::new();
    // metadata가 작거나 부정확해도 실제 읽기는 상한 + 1바이트로 제한한다.
    let read = file
        .take(MAX_PACK_BYTES + 1)
        .read_to_string(&mut text)
        .map_err(|e| PackError::Read(e.to_string()))?;
    if read as u64 > MAX_PACK_BYTES {
        return Err(PackError::TooLarge {
            bytes: read as u64,
            max: MAX_PACK_BYTES,
        });
    }
    Ok(text)
}

/// Read `path` and parse it as a TOML table, within [`MAX_PACK_BYTES`].
fn parse_manifest(path: &Path) -> Result<toml::Table, PackError> {
    let text = read_capped(path)?;
    let value: toml::Value = text
        .parse()
        .map_err(|e: toml::de::Error| PackError::Parse(e.to_string()))?;
    match value {
        toml::Value::Table(table) => Ok(table),
        _ => Err(PackError::Parse("root is not a table".to_string())),
    }
}

/// What discovery needs from a pack manifest: the combo label and the font.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackHead {
    /// `[meta] name`, if present and non-empty.
    pub display_name: Option<String>,
    pub font: FontDecl,
}

/// Read and validate the pack metadata needed for discovery: meta.name and font.
/// The whole bounded file is parsed, but translation strings are not flattened.
/// Pack validation rules are shared with load_pack.
pub fn load_pack_head(path: &Path) -> Result<PackHead, PackError> {
    let table = parse_manifest(path)?;
    let font = match table.get("font") {
        None => return Err(PackError::MissingFont),
        Some(font) => parse_font_decl(font)?,
    };
    Ok(PackHead {
        display_name: meta_name(&table),
        font,
    })
}

/// Load a pack with a required font declaration and optional meta.name.
/// String values outside font become translation keys over the English base.
/// Empty or Unicode-whitespace-only values keep the lower translation layer.
/// U+200B can represent intentionally blank text; U+00A0 is trimmed.
/// Discovery uses load_pack_head to avoid building this string table.
pub fn load_pack(path: &Path, code: &str) -> Result<LanguagePack, PackError> {
    let mut table = parse_manifest(path)?;
    let font = match table.get("font") {
        None => return Err(PackError::MissingFont),
        Some(font) => parse_font_decl(font)?,
    };
    let display_name = meta_name(&table);
    table.remove("font");
    let mut strings = HashMap::new();
    flatten_catalog_toml("", &toml::Value::Table(table), &mut strings);
    drop_blank_values_warned(
        &mut strings,
        &format!("language pack '{code}' at {}", path.display()),
        "fall back to English",
    );
    Ok(LanguagePack {
        code: code.to_string(),
        path: path.to_path_buf(),
        display_name,
        font,
        strings,
    })
}

/// Remove empty or whitespace-only values and return the count. Packs and
/// overrides share this rule, preserving their respective lower translation layers.
fn drop_blank_values(strings: &mut HashMap<String, String>) -> usize {
    let before = strings.len();
    strings.retain(|_, v| !v.trim().is_empty());
    before - strings.len()
}

/// Remove blank values and warn with the source file and fallback behavior.
fn drop_blank_values_warned(map: &mut HashMap<String, String>, origin: &str, fallback: &str) {
    let blank = drop_blank_values(map);
    if blank > 0 {
        tracing::warn!("i18n: {origin} has {blank} blank value(s) — those keys {fallback}");
    }
}

/// `[meta] name`, trimmed; `None` when absent, not a string, or blank.
fn meta_name(table: &toml::Table) -> Option<String> {
    table
        .get("meta")?
        .get("name")?
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
}

/// Precedence when several keys are present: `builtin = true` > `file` >
/// `family` > `candidates`. A `[font]` table with none of them is invalid.
fn parse_font_decl(value: &toml::Value) -> Result<FontDecl, PackError> {
    let Some(table) = value.as_table() else {
        return Err(PackError::InvalidFont("[font] must be a table".to_string()));
    };
    if table.get("builtin").and_then(toml::Value::as_bool) == Some(true) {
        return Ok(FontDecl::Builtin);
    }
    if let Some(file) = table.get("file") {
        return non_empty_str(file, "file").map(FontDecl::File);
    }
    if let Some(family) = table.get("family") {
        return non_empty_str(family, "family").map(FontDecl::Family);
    }
    if let Some(candidates) = table.get("candidates") {
        return parse_font_candidates(candidates).map(FontDecl::Candidates);
    }
    Err(PackError::InvalidFont(
        "expected one of builtin = true, file, family, candidates".to_string(),
    ))
}

fn non_empty_str(value: &toml::Value, key: &str) -> Result<String, PackError> {
    value
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
        .ok_or_else(|| PackError::InvalidFont(format!("{key} must be a non-empty string")))
}

fn parse_font_candidates(value: &toml::Value) -> Result<Vec<String>, PackError> {
    let Some(items) = value.as_array() else {
        return Err(PackError::InvalidFont(
            "candidates must be an array of strings".to_string(),
        ));
    };
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        out.push(non_empty_str(item, "candidates[]")?);
    }
    if out.is_empty() {
        return Err(PackError::InvalidFont("candidates is empty".to_string()));
    }
    Ok(out)
}

/// How the requested language was resolved at load time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoadOutcome {
    /// Built-in language, no user override file.
    Builtin,
    /// Built-in language with `<code>.toml` overlaid.
    BuiltinOverridden { path: PathBuf },
    /// User language pack loaded.
    Pack { path: PathBuf, font: FontDecl },
    /// Non-built-in code with no `<code>/pack.toml` — English was used instead.
    PackMissing { expected: PathBuf },
    /// `pack.toml` exists but was refused — English was used instead.
    PackInvalid { path: PathBuf, error: PackError },
}

/// Result of [`init`]: what was asked for, what is actually active, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadReport {
    /// The code from `general.language` (already normalized by the caller).
    pub requested: String,
    /// The language the table was built for — equals `requested` unless the
    /// pack was missing/invalid, in which case it is `"en"`.
    pub effective: String,
    pub outcome: LoadOutcome,
}

impl LoadReport {
    /// `true` when the requested language could not be loaded and English is
    /// active instead. The setting value is untouched either way.
    pub fn fell_back(&self) -> bool {
        self.requested != self.effective
    }

    /// User-facing warning for a fallback (`None` when the requested language is
    /// active). Rendered through the *effective* table, so it reads in English —
    /// the requested language had no strings to say it in.
    pub fn user_warning(&self) -> Option<String> {
        match &self.outcome {
            LoadOutcome::PackMissing { expected } => Some(fit_path(expected, |p| {
                t_fmt2("i18n.warn.pack_missing", &self.requested, p)
            })),
            LoadOutcome::PackInvalid { path, error } => {
                let reason = summarize_reason(error);
                Some(fit_path(path, |p| {
                    t_args("i18n.warn.pack_invalid", &[&self.requested, p, &reason])
                }))
            }
            LoadOutcome::Builtin
            | LoadOutcome::BuiltinOverridden { .. }
            | LoadOutcome::Pack { .. } => None,
        }
    }
}

/// namespace 락의 poison은 한 번 보고하고 복구한다. 임계구역은 메모리 맵의 조회·변경이다.
static NAMESPACES_POISONED: AtomicBool = AtomicBool::new(false);
const NAMESPACES_WHAT: &str = "i18n plugin namespace overlays";

pub struct Translations {
    /// Built-in + user override strings. Frozen after `init`.
    base: HashMap<String, &'static str>,
    /// Per-plugin namespace overlays. Looked up after `base` misses.
    /// Iteration order is not stable; collisions across namespaces resolve
    /// to whichever shows up first in the iteration — plugins should prefix
    /// their keys with their plugin id to avoid collisions in practice.
    namespaces: RwLock<HashMap<String, HashMap<String, &'static str>>>,
    /// Active language code; used by `register_namespace` to decide which
    /// language file to load from a plugin's lang dir.
    language: String,
    /// Host language root captured at initialization, also used for plugin overlays.
    user_lang_dir: Option<PathBuf>,
}

impl Translations {
    /// Load translations for the given language code. Built-in codes overlay
    /// their embedded file (and a user `<code>.toml` override) on the English
    /// base; any other code needs a pack at `~/.tasty/lang/<code>/pack.toml`
    /// and falls back to English when it is missing or invalid — the returned
    /// [`LoadReport`] says which happened.
    fn load(language: &str) -> (Self, LoadReport) {
        Self::load_from(language, user_lang_dir().as_deref())
    }

    fn load_from(requested: &str, lang_dir: Option<&Path>) -> (Self, LoadReport) {
        if is_builtin_code(requested) {
            let (strings, outcome) = Self::builtin_strings(requested, lang_dir);
            return (
                Self::from_strings(strings, requested, lang_dir),
                LoadReport {
                    requested: requested.to_string(),
                    effective: requested.to_string(),
                    outcome,
                },
            );
        }
        match Self::pack_strings(requested, lang_dir) {
            Ok((strings, outcome)) => (
                Self::from_strings(strings, requested, lang_dir),
                LoadReport {
                    requested: requested.to_string(),
                    effective: requested.to_string(),
                    outcome,
                },
            ),
            Err(outcome) => {
                Self::warn_fallback(requested, lang_dir, &outcome);
                let (strings, _) = Self::builtin_strings("en", lang_dir);
                (
                    Self::from_strings(strings, "en", lang_dir),
                    LoadReport {
                        requested: requested.to_string(),
                        effective: "en".to_string(),
                        outcome,
                    },
                )
            }
        }
    }

    /// The one diagnostic line for a fallback. This is the whole notification on
    /// headless/CLI paths; the GUI adds a toast from [`LoadReport::user_warning`].
    fn warn_fallback(requested: &str, lang_dir: Option<&Path>, outcome: &LoadOutcome) {
        match outcome {
            LoadOutcome::PackMissing { expected } => {
                Self::warn_pack_missing(requested, lang_dir, expected);
            }
            LoadOutcome::PackInvalid { path, error } => tracing::warn!(
                "i18n: language pack '{requested}' at {} rejected ({error}) — falling back to English (setting left unchanged)",
                path.display()
            ),
            LoadOutcome::Builtin
            | LoadOutcome::BuiltinOverridden { .. }
            | LoadOutcome::Pack { .. } => {}
        }
    }

    /// A stray single `<code>.toml` next to the missing pack is the likeliest
    /// mistake (the old single-file layout), so the line points at it.
    fn warn_pack_missing(requested: &str, lang_dir: Option<&Path>, expected: &Path) {
        let stray = lang_dir
            .map(|d| d.join(format!("{requested}.toml")))
            .filter(|p| p.is_file());
        match stray {
            Some(stray) => tracing::warn!(
                "i18n: language '{requested}' has no language pack at {} — falling back to English (setting left unchanged). {} is a single file, which only overrides a built-in language; a new language needs <code>/{PACK_FILE_NAME}",
                expected.display(),
                stray.display()
            ),
            None => tracing::warn!(
                "i18n: language '{requested}' has no language pack at {} — falling back to English (setting left unchanged)",
                expected.display()
            ),
        }
    }

    /// English base → embedded `code` overlay → user `<code>.toml` override.
    fn builtin_strings(
        code: &str,
        lang_dir: Option<&Path>,
    ) -> (HashMap<String, String>, LoadOutcome) {
        let mut strings = Self::english_base();
        if code != "en"
            && let Some(toml_str) = builtin_toml(code)
        {
            let mut overlay = HashMap::new();
            Self::parse_toml_into(&mut overlay, toml_str);
            plural::extend_layer(&mut strings, overlay);
        }
        let outcome =
            match lang_dir.and_then(|dir| Self::apply_user_override(&mut strings, dir, code)) {
                Some(path) => LoadOutcome::BuiltinOverridden { path },
                None => LoadOutcome::Builtin,
            };
        (strings, outcome)
    }

    /// Overlay a built-in language file. Missing or invalid files leave strings unchanged.
    /// Blank values preserve the built-in language text.
    fn apply_user_override(
        strings: &mut HashMap<String, String>,
        dir: &Path,
        code: &str,
    ) -> Option<PathBuf> {
        let path = dir.join(format!("{code}.toml"));
        let value = match read_user_toml(&path) {
            Ok(Some(value)) => value,
            Ok(None) => return None,
            Err(e) => {
                tracing::warn!(
                    "i18n: user override {} ignored — {e} (built-in '{code}' strings stay active)",
                    path.display()
                );
                return None;
            }
        };
        // 오버레이를 따로 모아 빈 값을 걷어낸 뒤 합친다 — `strings` 에 바로 flatten
        // 하면 빈 값이 내장 문자열을 이미 덮어써서 되돌릴 수 없다.
        let mut overlay = HashMap::new();
        flatten_catalog_toml("", &value, &mut overlay);
        drop_blank_values_warned(
            &mut overlay,
            &format!("user override {}", path.display()),
            &format!("keep the built-in '{code}' text"),
        );
        plural::extend_layer(strings, overlay);
        tracing::info!("loaded user translations from {}", path.display());
        Some(path)
    }

    /// English base → pack strings. `Err` carries the fallback reason.
    fn pack_strings(
        code: &str,
        lang_dir: Option<&Path>,
    ) -> Result<(HashMap<String, String>, LoadOutcome), LoadOutcome> {
        let Some(dir) = lang_dir else {
            // 홈을 찾지 못해도 CWD의 lang을 대신 읽지 않는다. 실행 위치에 따라 번역이 바뀌면 안 된다.
            return Err(LoadOutcome::PackMissing {
                expected: pack_path(Path::new(UNKNOWN_LANG_DIR), code),
            });
        };
        let path = pack_path(dir, code);
        if !is_valid_code(code) || !path.is_file() {
            return Err(LoadOutcome::PackMissing { expected: path });
        }
        match load_pack(&path, code) {
            Ok(pack) => {
                let mut strings = Self::english_base();
                plural::extend_layer(&mut strings, pack.strings);
                tracing::info!(
                    "i18n: loaded language pack '{code}' from {} (font: {:?})",
                    path.display(),
                    pack.font
                );
                Ok((
                    strings,
                    LoadOutcome::Pack {
                        path,
                        font: pack.font,
                    },
                ))
            }
            Err(error) => Err(LoadOutcome::PackInvalid { path, error }),
        }
    }

    fn english_base() -> HashMap<String, String> {
        let mut strings = HashMap::new();
        if let Some(en) = builtin_toml("en") {
            Self::parse_toml_into(&mut strings, en);
        }
        strings
    }

    fn from_strings(
        strings: HashMap<String, String>,
        language: &str,
        user_lang_dir: Option<&Path>,
    ) -> Self {
        tracing::info!(
            "i18n: loaded {} strings for language '{}'",
            strings.len(),
            language
        );
        let base: HashMap<String, &'static str> =
            strings.into_iter().map(|(k, v)| (k, leak_str(v))).collect();
        Self {
            base,
            namespaces: RwLock::new(HashMap::new()),
            language: language.to_string(),
            user_lang_dir: user_lang_dir.map(Path::to_path_buf),
        }
    }

    /// Parse a TOML string with nested tables into flat dotted keys.
    /// e.g., [settings.tab] general = "General" -> "settings.tab.general" = "General"
    fn parse_toml_into(map: &mut HashMap<String, String>, toml_str: &str) {
        if let Ok(value) = toml_str.parse::<toml::Value>() {
            flatten_catalog_toml("", &value, map);
        }
    }

    /// Get a translated string by key. Falls back to the key itself if not found.
    pub fn get<'a>(&'a self, key: &'a str) -> &'a str {
        if let Some(s) = self.base.get(key) {
            return s;
        }
        let ns = tasty_utils::poison::recover_read(
            self.namespaces.read(),
            NAMESPACES_WHAT,
            &NAMESPACES_POISONED,
        );
        for map in ns.values() {
            if let Some(s) = map.get(key) {
                return s;
            }
        }
        drop(ns);
        key
    }

    /// Get a translated string with a format argument replacing every `{}`
    /// (or every `{0}` in a numbered string, see [`fill_args`]).
    pub fn get_fmt(&self, key: &str, arg: &str) -> String {
        let template = self.get(key);
        if has_numbered_placeholder(template) {
            fill_args(template, &[arg])
        } else {
            template.replace("{}", arg)
        }
    }

    /// Fill two arguments as [`fill_args`] does.
    pub fn get_fmt2(&self, key: &str, arg1: &str, arg2: &str) -> String {
        fill_args(self.get(key), &[arg1, arg2])
    }

    /// Fill `args` as [`fill_args`] does. Generalizes [`get_fmt`]/[`get_fmt2`]
    /// for strings with three or more interpolated values — common in CLI
    /// output (e.g. `remote check` alive lines carrying host/port/version).
    pub fn get_args(&self, key: &str, args: &[&str]) -> String {
        fill_args(self.get(key), args)
    }

    /// Register a plugin namespace. `lang_dir` is expected to contain
    /// `<lang>.toml` files — `en.toml` is loaded as the base, then the
    /// active language file and the host-root user override are overlaid on top.
    ///
    /// If `namespace` was previously registered, its entries are replaced.
    pub fn register_namespace(&self, namespace: &str, lang_dir: &Path) {
        let strings = plugin_catalog::load(
            lang_dir,
            &self.language,
            namespace,
            self.user_lang_dir.as_deref(),
        );

        let leaked: HashMap<String, &'static str> =
            strings.into_iter().map(|(k, v)| (k, leak_str(v))).collect();

        let count = leaked.len();
        tasty_utils::poison::recover_write(
            self.namespaces.write(),
            NAMESPACES_WHAT,
            &NAMESPACES_POISONED,
        )
        .insert(namespace.to_string(), leaked);
        tracing::info!(
            "i18n: registered namespace '{}' with {} strings (lang_dir={})",
            namespace,
            count,
            lang_dir.display()
        );
    }

    /// Remove a previously registered namespace. Strings remain in memory
    /// (`Box::leak`) but are no longer reachable through `get`.
    pub fn unregister_namespace(&self, namespace: &str) {
        tasty_utils::poison::recover_write(
            self.namespaces.write(),
            NAMESPACES_WHAT,
            &NAMESPACES_POISONED,
        )
        .remove(namespace);
        tracing::info!("i18n: unregistered namespace '{}'", namespace);
    }
}

/// Read user TOML under MAX_PACK_BYTES. Missing files return Ok(None);
/// other read or parse failures return Err for the caller to report.
fn read_user_toml(path: &Path) -> Result<Option<toml::Value>, String> {
    if !path.exists() {
        return Ok(None);
    }
    let text = match read_capped(path) {
        Ok(text) => text,
        // `exists()` 와 열기 사이에 사라졌다면 없는 것과 같게 다룬다.
        Err(PackError::Read(_)) if !path.exists() => return Ok(None),
        Err(e) => return Err(e.to_string()),
    };
    text.parse::<toml::Value>()
        .map(Some)
        .map_err(|e| format!("invalid TOML: {e}"))
}

fn leak_str(s: String) -> &'static str {
    Box::leak(s.into_boxed_str())
}

/// List built-ins, their valid overrides, and valid packs sorted by code.
/// Pack metadata uses the same validation as init; file changes can affect later loading.
pub fn available_languages() -> Vec<LanguageEntry> {
    scan_languages(user_lang_dir().as_deref())
}

/// [`available_languages`] over an explicit directory (`None` = no user dir).
///
/// - a directory with `pack.toml` is a pack; parse failure or a missing/invalid
///   `[font]` excludes it with a `tracing::warn!`;
/// - a directory named after a built-in code is not a pack (built-ins are
///   overridden by a single file) — excluded with a warning;
/// - a single `<code>.toml` marks a built-in as overridden; for any other code
///   it is not a pack and is ignored with a warning.
pub fn scan_languages(lang_dir: Option<&Path>) -> Vec<LanguageEntry> {
    let mut out: Vec<LanguageEntry> = BUILTIN_CODES
        .iter()
        .map(|code| builtin_entry(code, lang_dir))
        .collect();
    if let Some(dir) = lang_dir {
        out.extend(scan_packs(dir));
    }
    out
}

fn builtin_entry(code: &str, lang_dir: Option<&Path>) -> LanguageEntry {
    let mut entry = LanguageEntry {
        code: code.to_string(),
        display_name: builtin_toml(code)
            .and_then(|s| s.parse::<toml::Value>().ok())
            .and_then(|v| v.as_table().and_then(meta_name)),
        source: LanguageSource::Builtin,
        font: None,
        path: None,
    };
    let Some(dir) = lang_dir else {
        return entry;
    };
    let path = dir.join(format!("{code}.toml"));
    match read_user_toml(&path) {
        Ok(Some(value)) => {
            if let Some(name) = value.as_table().and_then(meta_name) {
                entry.display_name = Some(name);
            }
            entry.source = LanguageSource::BuiltinOverridden;
            entry.path = Some(path);
        }
        Ok(None) => {}
        Err(e) => tracing::warn!(
            "i18n: user override {} ignored — {e} (listed as plain built-in '{code}')",
            path.display()
        ),
    }
    entry
}

fn scan_packs(dir: &Path) -> Vec<LanguageEntry> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) => {
            if e.kind() != std::io::ErrorKind::NotFound {
                tracing::warn!("i18n: cannot scan {}: {e}", dir.display());
            }
            return Vec::new();
        }
    };
    let mut packs = Vec::new();
    for entry in entries {
        match entry {
            Ok(entry) => scan_dir_entry(dir, &entry.path(), &mut packs),
            Err(e) => {
                tracing::warn!("i18n: skipping unreadable entry in {}: {e}", dir.display());
            }
        }
    }
    packs.sort_by(|a, b| a.code.cmp(&b.code));
    packs
}

/// One `~/.tasty/lang/` entry: a directory may be a pack; a single `<code>.toml`
/// is only meaningful for built-in codes (anything else gets a warning).
fn scan_dir_entry(dir: &Path, path: &Path, packs: &mut Vec<LanguageEntry>) {
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return;
    };
    if path.is_dir() {
        if let Some(pack) = pack_entry(path, name) {
            packs.push(pack);
        }
    } else if let Some(stem) = single_file_code(path)
        && !is_builtin_code(stem)
    {
        tracing::warn!(
            "i18n: {} ignored — a single <code>.toml only overrides a built-in language; a new language '{stem}' needs {}",
            path.display(),
            pack_path(dir, stem).display()
        );
    }
}

/// `<code>` for a `<code>.toml` file path, else `None`.
fn single_file_code(path: &Path) -> Option<&str> {
    if path.extension()? != "toml" {
        return None;
    }
    path.file_stem()?.to_str()
}

/// Why a `<code>/pack.toml` directory cannot be listed as a pack, if any.
fn pack_dir_rejection(code: &str) -> Option<String> {
    if !is_valid_code(code) {
        Some(format!("'{code}' is not a usable language code"))
    } else if is_builtin_code(code) {
        Some(format!(
            "built-in '{code}' is overridden by {code}.toml, not by a pack"
        ))
    } else {
        None
    }
}

fn pack_entry(dir: &Path, code: &str) -> Option<LanguageEntry> {
    let path = dir.join(PACK_FILE_NAME);
    if !path.is_file() {
        return None;
    }
    if let Some(reason) = pack_dir_rejection(code) {
        tracing::warn!("i18n: {} ignored — {reason}", path.display());
        return None;
    }
    match load_pack_head(&path) {
        Ok(head) => Some(LanguageEntry {
            code: code.to_string(),
            display_name: head.display_name,
            source: LanguageSource::Pack,
            font: Some(head.font),
            path: Some(path),
        }),
        Err(e) => {
            tracing::warn!(
                "i18n: language pack {} rejected ({e}) — not listed",
                path.display()
            );
            None
        }
    }
}

/// Initialize the global translation store. Call once at startup. Returns what
/// was loaded; on a missing/invalid language pack the table is English and the
/// report says so ([`LoadReport::fell_back`]). A second call is a no-op that
/// returns the first report.
pub fn init(language: &str) -> LoadReport {
    let (translations, report) = Translations::load(language);
    // OnceLock::set은 이미 set된 경우만 Err. i18n은 부팅 시 1회만 호출되는 시드
    // 데이터라 두 번째 호출은 의도적 no-op — Err를 panic이나 로그 없이 그대로 무시.
    let _already_set: Result<_, _> = TRANSLATIONS.set(translations);
    LOAD_REPORT.get_or_init(|| report).clone()
}

/// The report [`init`] produced at boot; `None` before `init`.
pub fn load_report() -> Option<&'static LoadReport> {
    LOAD_REPORT.get()
}

/// The table lookups read: the initialized store, or embedded English before
/// [`init`]. The first lookup before `init` is logged once at debug level with
/// its key, which is how a missing `init` on a product path shows up now that
/// the raw key is no longer returned.
fn store(key: &str) -> &'static Translations {
    if let Some(tr) = TRANSLATIONS.get() {
        return tr;
    }
    if !PRE_INIT_READ_LOGGED.swap(true, std::sync::atomic::Ordering::Relaxed) {
        tracing::debug!("i18n: '{key}' read before init — serving built-in English");
    }
    BUILTIN_EN.get_or_init(|| Translations::load_from("en", None).0)
}

/// Does `template` hold a numbered placeholder such as `{0}`?
pub fn has_numbered_placeholder(template: &str) -> bool {
    numbered_placeholders(template).next().is_some()
}

/// The `(byte range, index)` of each numbered placeholder `{N}` in `template`.
fn numbered_placeholders(
    template: &str,
) -> impl Iterator<Item = (std::ops::Range<usize>, usize)> + '_ {
    template.match_indices('{').filter_map(move |(open, _)| {
        let rest = &template[open + 1..];
        let close = rest.find('}')?;
        let digits = &rest[..close];
        if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        let index = digits.parse().ok()?;
        Some((open..open + close + 2, index))
    })
}

/// Put `args` into the placeholders of a translated string.
///
/// A string uses one of two forms. With numbered placeholders `{0}`, `{1}`, …
/// each one takes `args[N]`, so a language can reorder the values; a number
/// with no matching argument stays as written, and an argument no number names
/// is not shown. Otherwise each `{}` takes the next argument in order and
/// placeholders beyond the arguments stay as written. A string that holds both
/// forms is filled as numbered and keeps its `{}`; the catalog tests refuse
/// such strings.
pub fn fill_args(template: &str, args: &[&str]) -> String {
    if !has_numbered_placeholder(template) {
        let mut out = template.to_string();
        for arg in args {
            out = out.replacen("{}", arg, 1);
        }
        return out;
    }
    let mut out = String::with_capacity(template.len());
    let mut copied = 0;
    for (range, index) in numbered_placeholders(template) {
        let Some(arg) = args.get(index) else {
            continue;
        };
        out.push_str(&template[copied..range.start]);
        out.push_str(arg);
        copied = range.end;
    }
    out.push_str(&template[copied..]);
    out
}

/// Get a translated string by key. Before [`init`] this is the embedded
/// English text; a key missing from the table comes back unchanged.
pub fn t(key: &str) -> &str {
    store(key).get(key)
}

/// 활성 language code — [`init`] 이 실제로 적용한 언어(언어팩 부재로 영어 폴백이 일어났으면
/// `"en"`, 요청 코드는 [`load_report`]). 미초기화면 `"en"` fallback.
/// 호스트가 plugin spawn 시 `TASTY_LOCALE` 환경변수로 전달하는 등에 사용.
pub fn current_language() -> &'static str {
    TRANSLATIONS
        .get()
        .map(|tr| tr.language.as_str())
        .unwrap_or("en")
}

/// Get a translated string with a format argument. Before [`init`] this uses
/// the embedded English text, like [`t`].
pub fn t_fmt(key: &str, arg: &str) -> String {
    store(key).get_fmt(key, arg)
}

/// Get a translated string with two format arguments, filled as [`fill_args`] does.
pub fn t_fmt2(key: &str, arg1: &str, arg2: &str) -> String {
    store(key).get_fmt2(key, arg1, arg2)
}

/// Get a translated string with N format arguments, filled as [`fill_args`] does.
pub fn t_args(key: &str, args: &[&str]) -> String {
    store(key).get_args(key, args)
}

/// Register a plugin's translation namespace. No-op with a warning if `init`
/// has not been called yet: the embedded English table that serves lookups
/// before `init` takes no namespaces, so a plugin key read then comes back
/// unchanged.
pub fn register_namespace(namespace: &str, lang_dir: &Path) {
    if let Some(tr) = TRANSLATIONS.get() {
        tr.register_namespace(namespace, lang_dir);
    } else {
        tracing::warn!(
            "i18n: register_namespace('{}') called before init — ignored",
            namespace
        );
    }
}

/// Unregister a plugin's translation namespace.
pub fn unregister_namespace(namespace: &str) {
    if let Some(tr) = TRANSLATIONS.get() {
        tr.unregister_namespace(namespace);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// init 전 조회는 영어로 init 한 뒤의 조회와 같은 문자열이다. 그래서 한 프로세스에서
    /// 영어 init 을 사이에 두고 따로 계산한 두 문구가 갈리지 않는다(docs/dev-guide/i18n.md).
    #[test]
    fn a_lookup_before_init_reads_the_same_english_as_an_english_init() {
        assert!(
            TRANSLATIONS.get().is_none(),
            "이 시험 바이너리의 다른 시험이 전역 init 을 부르면 미초기화 경로를 잴 수 없다"
        );
        let english = Translations::load_from("en", None).0;
        assert!(!english.base.is_empty(), "내장 영어 표가 비었다");
        for (key, value) in &english.base {
            assert_eq!(t(key), *value, "{key}");
        }
        assert_eq!(
            t_fmt("explorer.status.items", "3"),
            english.get_fmt("explorer.status.items", "3")
        );
        assert_eq!(t("no.such.key"), "no.such.key", "없는 키는 그대로 돌아온다");
        assert!(
            TRANSLATIONS.get().is_none(),
            "조회가 전역 저장소를 정하면 안 된다"
        );
        assert_eq!(current_language(), "en");
    }

    /// 테스트 전용 임시 디렉토리 (프로세스 id + 단조 카운터로 유일).
    fn temp_dir(tag: &str) -> PathBuf {
        // 시각 해상도에 의존하지 않도록 PID와 단조 카운터로 임시 경로를 구분한다.
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "tasty-i18n-{tag}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write(path: PathBuf, content: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, content).unwrap();
    }

    const XX_PACK: &str =
        "[meta]\nname = \"Xx Lang\"\n\n[font]\nbuiltin = true\n\n[button]\nok = \"OKAY-XX\"\n";
    const YY_PACK_NO_FONT: &str = "[meta]\nname = \"Yy\"\n\n[button]\nok = \"YY\"\n";
    const BAD_PACK: &str = "[font\nbuiltin = true\n";

    fn scenario_dir() -> PathBuf {
        let dir = temp_dir("scenario");
        write(dir.join("xx").join("pack.toml"), XX_PACK);
        write(dir.join("yy").join("pack.toml"), YY_PACK_NO_FONT);
        write(
            dir.join("ko.toml"),
            "[meta]\nname = \"한국어 (custom)\"\n\n[button]\nok = \"확인!\"\n",
        );
        write(dir.join("zz.toml"), "[button]\nok = \"ZZ\"\n");
        write(dir.join("bad").join("pack.toml"), BAD_PACK);
        dir
    }

    #[test]
    fn scan_lists_builtins_overrides_and_valid_packs_only() {
        let dir = scenario_dir();
        let list = scan_languages(Some(&dir));
        let codes: Vec<&str> = list.iter().map(|l| l.code.as_str()).collect();
        let expected: Vec<&str> = BUILTIN_CODES.iter().copied().chain(["xx"]).collect();
        assert_eq!(codes, expected);

        let en = &list[0];
        assert_eq!(en.source, LanguageSource::Builtin);
        assert_eq!(en.display_name.as_deref(), Some("English"));
        assert_eq!(en.font, None);
        assert_eq!(en.path, None);

        let ko = &list[1];
        assert_eq!(ko.source, LanguageSource::BuiltinOverridden);
        assert_eq!(ko.display_name.as_deref(), Some("한국어 (custom)"));
        assert_eq!(ko.path.as_deref(), Some(dir.join("ko.toml").as_path()));
        assert_eq!(ko.font, None);

        let xx = list.last().expect("pack entry follows the builtins");
        assert_eq!(xx.source, LanguageSource::Pack);
        assert_eq!(xx.label(), "Xx Lang");
        assert_eq!(xx.font, Some(FontDecl::Builtin));
        assert_eq!(xx.path.as_deref(), Some(pack_path(&dir, "xx").as_path()));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn load_from_without_a_data_root_never_reads_a_cwd_relative_pack() {
        let (tr, report) = Translations::load_from("qq", None);
        assert_eq!(tr.language, "en");
        let LoadOutcome::PackMissing { expected } = &report.outcome else {
            panic!("expected PackMissing, got {:?}", report.outcome);
        };
        assert!(expected.starts_with(UNKNOWN_LANG_DIR), "{expected:?}");
        assert!(scan_languages(None).iter().all(|l| l.path.is_none()));
    }

    #[test]
    fn toast_reason_is_a_single_short_line() {
        let dir = scenario_dir();
        let (_, report) = Translations::load_from("bad", Some(&dir));
        let LoadOutcome::PackInvalid { error, .. } = &report.outcome else {
            panic!("expected PackInvalid, got {:?}", report.outcome);
        };
        assert!(error.to_string().lines().count() > 1);
        let summary = summarize_reason(error);
        assert!(!summary.contains('\n'), "{summary}");
        assert!(summary.chars().count() <= 61, "{summary}");
        assert!(summary.starts_with("invalid TOML:"), "{summary}");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn elide_middle_keeps_both_ends_and_marks_the_cut() {
        assert_eq!(elide_middle("short", 10), "short");
        let out = elide_middle("/home/someone/.tasty/lang/zz/pack.toml", 20);
        assert_eq!(out.chars().count(), 20);
        assert!(out.contains('\u{2026}'));
        assert!(
            out.starts_with("/home"),
            "머리(어느 루트인지)가 남아야 한다: {out}"
        );
        assert!(
            out.ends_with("zz/pack.toml"),
            "꼬리(어느 팩인지)가 남아야 한다: {out}"
        );
    }

    #[test]
    fn warning_with_a_very_long_path_stays_under_the_toast_cap() {
        let deep: String = std::iter::repeat_n("verylongsegment", 40)
            .collect::<Vec<_>>()
            .join("/");
        let path = PathBuf::from(format!("/{deep}/zz/pack.toml"));
        assert!(path.display().to_string().chars().count() > 500);

        let msg = fit_path(&path, |p| {
            format!(
                "Language 'zz' is not installed — showing English. Put a language pack at {p}, then restart."
            )
        });
        assert!(
            msg.chars().count() <= TOAST_MAX_CHARS,
            "{} chars: {msg}",
            msg.chars().count()
        );
        assert!(
            msg.contains("zz/pack.toml"),
            "어느 팩인지는 남아야 한다: {msg}"
        );
        assert!(
            msg.ends_with("then restart."),
            "문장 끝(할 일)이 살아 있어야 한다"
        );
    }

    #[test]
    fn a_long_failure_reason_keeps_both_the_target_and_the_os_error() {
        let deep: String = std::iter::repeat_n("verylongsegment", 40)
            .collect::<Vec<_>>()
            .join("/");
        let reason = format!("write /{deep}/.tasty/bashrc.user: Access is denied. (os error 5)");
        assert!(reason.chars().count() > 500);

        let msg = fit_fragment(&reason, |r| format!("Could not save the edit: {r}"));
        assert!(
            msg.chars().count() <= TOAST_MAX_CHARS,
            "{} chars: {msg}",
            msg.chars().count()
        );
        assert!(msg.starts_with("Could not save the edit: write /"), "{msg}");
        assert!(msg.ends_with("(os error 5)"), "왜인지가 남아야 한다: {msg}");
    }

    #[test]
    fn short_path_is_not_elided() {
        let path = PathBuf::from("/home/u/.tasty/lang/zz/pack.toml");
        let msg = fit_path(&path, |p| format!("pack at {p}"));
        assert_eq!(msg, "pack at /home/u/.tasty/lang/zz/pack.toml");
    }

    /// 코드·순서·표시 이름을 함께 대조한다. 목록과 파일을 따로 검사하면
    /// ja 코드가 en.toml을 읽는 식의 잘못된 연결을 놓칠 수 있다.
    #[test]
    fn scan_without_user_dir_lists_builtins_with_meta_names() {
        let list = scan_languages(None);
        let labels: Vec<&str> = list.iter().map(LanguageEntry::label).collect();
        assert_eq!(labels, ["English", "한국어", "日本語", "简体中文"]);
        assert!(list.iter().all(|l| l.source == LanguageSource::Builtin));
    }

    #[test]
    fn scan_pack_without_meta_name_labels_by_code() {
        let dir = temp_dir("nometa");
        write(
            dir.join("qq").join("pack.toml"),
            "[font]\nfamily = \"Noto Sans\"\n\n[button]\nok = \"Q\"\n",
        );
        write(dir.join("ko").join("pack.toml"), XX_PACK);
        let list = scan_languages(Some(&dir));
        let qq = list.iter().find(|l| l.code == "qq").expect("qq listed");
        assert_eq!(qq.display_name, None);
        assert_eq!(qq.label(), "qq");
        assert_eq!(qq.font, Some(FontDecl::Family("Noto Sans".to_string())));
        assert_eq!(list.iter().filter(|l| l.code == "ko").count(), 1);
        assert_eq!(
            list.iter().find(|l| l.code == "ko").unwrap().source,
            LanguageSource::Builtin
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn load_pack_accepts_every_font_shape_and_rejects_the_rest() {
        let dir = temp_dir("font");
        let cases: &[(&str, Result<FontDecl, PackError>)] = &[
            ("[font]\nbuiltin = true\n", Ok(FontDecl::Builtin)),
            (
                "[font]\nfile = \"fonts/x.ttf\"\n",
                Ok(FontDecl::File("fonts/x.ttf".to_string())),
            ),
            (
                "[font]\nfamily = \"Noto Sans\"\n",
                Ok(FontDecl::Family("Noto Sans".to_string())),
            ),
            (
                "[font]\ncandidates = [\"fonts/x.ttf\", \"Noto Sans\"]\n",
                Ok(FontDecl::Candidates(vec![
                    "fonts/x.ttf".to_string(),
                    "Noto Sans".to_string(),
                ])),
            ),
            (
                "[font]\nbuiltin = true\nfile = \"fonts/x.ttf\"\n",
                Ok(FontDecl::Builtin),
            ),
            ("[button]\nok = \"x\"\n", Err(PackError::MissingFont)),
            (
                "[font]\nbuiltin = false\n",
                Err(PackError::InvalidFont(
                    "expected one of builtin = true, file, family, candidates".to_string(),
                )),
            ),
            (
                "[font]\nfile = \"\"\n",
                Err(PackError::InvalidFont(
                    "file must be a non-empty string".to_string(),
                )),
            ),
            (
                "[font]\ncandidates = []\n",
                Err(PackError::InvalidFont("candidates is empty".to_string())),
            ),
            (
                "font = \"x\"\n",
                Err(PackError::InvalidFont("[font] must be a table".to_string())),
            ),
        ];
        for (i, (content, expected)) in cases.iter().enumerate() {
            let path = dir.join(format!("c{i}")).join("pack.toml");
            write(path.clone(), content);
            let got = load_pack(&path, "c").map(|p| p.font);
            assert_eq!(&got, expected, "case {i}: {content:?}");
        }
        assert!(matches!(
            load_pack(&dir.join("nope").join("pack.toml"), "nope"),
            Err(PackError::Read(_))
        ));
        write(dir.join("bad").join("pack.toml"), BAD_PACK);
        assert!(matches!(
            load_pack(&dir.join("bad").join("pack.toml"), "bad"),
            Err(PackError::Parse(_))
        ));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn load_from_pack_overlays_strings_on_english_base() {
        let dir = scenario_dir();
        let (tr, report) = Translations::load_from("xx", Some(&dir));
        assert_eq!(tr.get("button.ok"), "OKAY-XX");
        assert_eq!(tr.get("app.name"), "Tasty");
        assert_eq!(tr.get("meta.name"), "Xx Lang");
        assert_eq!(tr.get("font.builtin"), "font.builtin");
        assert_eq!(tr.language, "xx");
        assert_eq!(report.requested, "xx");
        assert_eq!(report.effective, "xx");
        assert!(!report.fell_back());
        assert_eq!(
            report.outcome,
            LoadOutcome::Pack {
                path: pack_path(&dir, "xx"),
                font: FontDecl::Builtin,
            }
        );
        assert_eq!(report.user_warning(), None);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// 팩과 내장 override 모두 빈 값을 버리되 각각 영어·내장 언어로 돌아가는지 대조한다.
    #[test]
    fn a_blank_value_means_the_same_thing_in_a_pack_and_in_an_override() {
        let dir = temp_dir("blank-symmetry");
        let body = "[button]\nok = \"OK-EDIT\"\ncancel = \"\"\nsave = \"   \"\n";
        write(
            pack_path(&dir, "bl"),
            &format!("[font]\nbuiltin = true\n\n{body}"),
        );
        write(dir.join("ko.toml"), body);

        let (pack_tr, _) = Translations::load_from("bl", Some(&dir));
        let (ovr_tr, ovr_report) = Translations::load_from("ko", Some(&dir));
        assert_eq!(
            ovr_report.outcome,
            LoadOutcome::BuiltinOverridden {
                path: dir.join("ko.toml"),
            },
            "override 경로를 실제로 지나야 이 테스트가 의미가 있다"
        );

        for (label, tr) in [("pack", &pack_tr), ("override", &ovr_tr)] {
            assert_eq!(tr.get("button.ok"), "OK-EDIT", "{label}");
        }
        for (label, tr) in [("pack", &pack_tr), ("override", &ovr_tr)] {
            for key in ["button.cancel", "button.save"] {
                assert!(
                    !tr.get(key).trim().is_empty(),
                    "{label} 경로에서 {key} 가 빈 문자열로 나왔다 — 라벨 없는 UI 가 된다"
                );
            }
        }
        assert_eq!(pack_tr.get("button.cancel"), "Cancel");
        assert_eq!(ovr_tr.get("button.cancel"), "취소");
        assert_eq!(ovr_tr.get("button.save"), "저장");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Unicode White_Space는 제거하고 U+200B/U+2060은 유지한다.
    #[test]
    fn the_documented_escape_hatch_survives_the_trim() {
        for eaten in ["", " ", "\t", "\u{00A0}", "\u{2007}", "\u{3000}"] {
            let mut m = HashMap::from([("k".to_string(), eaten.to_string())]);
            assert_eq!(drop_blank_values(&mut m), 1, "{eaten:?} 는 걷힌다");
        }
        for kept in ["\u{200B}", "\u{2060}"] {
            let mut m = HashMap::from([("k".to_string(), kept.to_string())]);
            assert_eq!(drop_blank_values(&mut m), 0, "{kept:?} 는 남아야 한다");
            assert_eq!(m.get("k").map(String::as_str), Some(kept));
        }
    }

    #[test]
    fn blank_pack_values_fall_back_to_english_instead_of_showing_empty() {
        let dir = temp_dir("blank");
        write(
            pack_path(&dir, "bl"),
            "[font]\nbuiltin = true\n\n[button]\nok = \"OK-BL\"\ncancel = \"\"\nsave = \"   \"\n",
        );
        let pack = load_pack(&pack_path(&dir, "bl"), "bl").unwrap();
        assert_eq!(
            pack.strings.get("button.ok").map(String::as_str),
            Some("OK-BL")
        );
        assert!(!pack.strings.contains_key("button.cancel"));
        assert!(!pack.strings.contains_key("button.save"));

        let (tr, report) = Translations::load_from("bl", Some(&dir));
        assert_eq!(tr.get("button.ok"), "OK-BL");
        assert_eq!(tr.get("button.cancel"), "Cancel");
        assert_eq!(tr.get("button.save"), "Save");
        assert!(!report.fell_back());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn load_from_missing_pack_falls_back_to_english_and_reports() {
        let dir = scenario_dir();
        let (tr, report) = Translations::load_from("zz", Some(&dir));
        assert_eq!(tr.language, "en");
        assert_eq!(tr.get("app.name"), "Tasty");
        assert_eq!(tr.get("button.ok"), "OK");
        assert_eq!(report.requested, "zz");
        assert_eq!(report.effective, "en");
        assert!(report.fell_back());
        assert_eq!(
            report.outcome,
            LoadOutcome::PackMissing {
                expected: pack_path(&dir, "zz"),
            }
        );
        assert!(report.user_warning().is_some());

        let (_, report) = Translations::load_from("nn", Some(&dir));
        assert_eq!(
            report.outcome,
            LoadOutcome::PackMissing {
                expected: pack_path(&dir, "nn"),
            }
        );
        let (tr, report) = Translations::load_from("nn", None);
        assert_eq!(tr.language, "en");
        assert!(matches!(report.outcome, LoadOutcome::PackMissing { .. }));
        std::fs::remove_dir_all(&dir).ok();
    }

    /// 내장 간체 코드는 `zh-Hans` 하나다. 다른 중국어 표기는 별칭으로 받지 않고 다른 미설치 코드처럼
    /// 영어로 돌아가며 경고를 낸다. 요청한 값은 보고에 그대로 남는다.
    #[test]
    fn chinese_codes_other_than_zh_hans_are_not_aliases() {
        let dir = scenario_dir();
        let (tr, report) = Translations::load_from("zh-Hans", Some(&dir));
        assert_eq!(tr.language, "zh-Hans");
        assert_eq!(report.outcome, LoadOutcome::Builtin);
        assert!(report.user_warning().is_none());
        for code in ["zh", "zh-CN", "zh_CN", "zh-hans", "ZH-HANS"] {
            let (tr, report) = Translations::load_from(code, Some(&dir));
            assert_eq!(tr.language, "en", "{code}");
            assert_eq!(report.requested, code);
            assert_eq!(report.effective, "en", "{code}");
            assert_eq!(
                report.outcome,
                LoadOutcome::PackMissing {
                    expected: pack_path(&dir, code),
                },
                "{code}"
            );
            assert!(report.user_warning().is_some(), "{code}");
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn load_from_invalid_pack_falls_back_to_english_and_reports_reason() {
        let dir = scenario_dir();
        let (tr, report) = Translations::load_from("yy", Some(&dir));
        assert_eq!(tr.language, "en");
        assert_eq!(tr.get("button.ok"), "OK");
        assert_eq!(report.effective, "en");
        assert_eq!(
            report.outcome,
            LoadOutcome::PackInvalid {
                path: pack_path(&dir, "yy"),
                error: PackError::MissingFont,
            }
        );
        assert!(report.user_warning().is_some());

        let (_, report) = Translations::load_from("bad", Some(&dir));
        assert!(matches!(
            report.outcome,
            LoadOutcome::PackInvalid {
                error: PackError::Parse(_),
                ..
            }
        ));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn load_from_builtin_uses_single_file_override_only() {
        let dir = scenario_dir();
        let (tr, report) = Translations::load_from("ko", Some(&dir));
        assert_eq!(tr.language, "ko");
        assert_eq!(tr.get("button.ok"), "확인!");
        assert_eq!(
            report.outcome,
            LoadOutcome::BuiltinOverridden {
                path: dir.join("ko.toml"),
            }
        );
        assert!(!report.fell_back());
        assert_eq!(report.user_warning(), None);

        write(dir.join("ja.toml"), BAD_PACK);
        let (tr, report) = Translations::load_from("ja", Some(&dir));
        assert_eq!(tr.language, "ja");
        assert_eq!(report.outcome, LoadOutcome::Builtin);
        assert_eq!(tr.get("button.ok"), "OK");

        write(dir.join("en").join("pack.toml"), XX_PACK);
        let (tr, report) = Translations::load_from("en", Some(&dir));
        assert_eq!(tr.get("button.ok"), "OK");
        assert_eq!(report.outcome, LoadOutcome::Builtin);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn namespace_register_lookup() {
        let tr = Translations {
            base: HashMap::new(),
            namespaces: RwLock::new(HashMap::new()),
            language: "en".to_string(),
            user_lang_dir: None,
        };
        {
            let mut ns = tr.namespaces.write().unwrap();
            let mut m: HashMap<String, &'static str> = HashMap::new();
            m.insert("plugin.x.title".to_string(), "Refresh");
            ns.insert("com.example.x".to_string(), m);
        }
        assert_eq!(tr.get("plugin.x.title"), "Refresh");
        assert_eq!(tr.get("missing.key"), "missing.key");
    }

    #[test]
    fn base_takes_precedence_over_namespace() {
        let mut base: HashMap<String, &'static str> = HashMap::new();
        base.insert("shared.key".to_string(), "FromBase");
        let tr = Translations {
            base,
            namespaces: RwLock::new(HashMap::new()),
            language: "en".to_string(),
            user_lang_dir: None,
        };
        {
            let mut ns = tr.namespaces.write().unwrap();
            let mut m: HashMap<String, &'static str> = HashMap::new();
            m.insert("shared.key".to_string(), "FromPlugin");
            ns.insert("com.example.x".to_string(), m);
        }
        assert_eq!(tr.get("shared.key"), "FromBase");
    }

    #[test]
    fn unregister_namespace_removes_strings() {
        let tr = Translations {
            base: HashMap::new(),
            namespaces: RwLock::new(HashMap::new()),
            language: "en".to_string(),
            user_lang_dir: None,
        };
        {
            let mut ns = tr.namespaces.write().unwrap();
            let mut m: HashMap<String, &'static str> = HashMap::new();
            m.insert("plugin.x.title".to_string(), "Refresh");
            ns.insert("com.example.x".to_string(), m);
        }
        assert_eq!(tr.get("plugin.x.title"), "Refresh");
        tr.unregister_namespace("com.example.x");
        assert_eq!(tr.get("plugin.x.title"), "plugin.x.title");
    }

    #[test]
    fn numbered_placeholders_take_their_argument_by_number() {
        // 언어가 값의 순서를 바꿀 수 있다.
        assert_eq!(
            fill_args("{1}개 중 {0}개 이동 · 원본 {2}개", &["37", "40", "3"]),
            "40개 중 37개 이동 · 원본 3개"
        );
        // 같은 번호를 여러 번 쓸 수 있다.
        assert_eq!(fill_args("{0} and {0}", &["a"]), "a and a");
        // 두 자리 번호.
        let args: Vec<String> = (0..11).map(|i| i.to_string()).collect();
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        assert_eq!(fill_args("{10}/{0}", &args), "10/0");
    }

    #[test]
    fn a_number_without_an_argument_stays_and_an_unnamed_argument_is_dropped() {
        assert_eq!(fill_args("{0} · {3}", &["a", "b"]), "a · {3}");
        assert_eq!(fill_args("{1}", &["a", "b"]), "b");
        assert_eq!(fill_args("{0}", &[]), "{0}");
    }

    #[test]
    fn positional_strings_fill_in_order_as_before() {
        assert_eq!(fill_args("{} / {} · {}", &["1", "2", "3"]), "1 / 2 · 3");
        assert_eq!(fill_args("{} / {}", &["1"]), "1 / {}");
        assert_eq!(fill_args("{} only", &["1", "2"]), "1 only");
        // 숫자가 아닌 중괄호는 자리표시가 아니다.
        assert_eq!(fill_args("{x} {}", &["1"]), "{x} 1");
        assert!(!has_numbered_placeholder("{} {x} {-1} {}"));
    }

    /// 두 방식을 섞은 문자열은 번호 방식으로 채우고 `{}` 는 그대로 둔다. 카탈로그 검사가 이런 문자열을
    /// 막는다(tests/i18n_key_parity.rs).
    #[test]
    fn a_mixed_string_is_filled_as_numbered_and_keeps_its_bare_braces() {
        assert!(has_numbered_placeholder("{} {0}"));
        assert_eq!(fill_args("{} {0}", &["a"]), "{} a");
    }

    #[test]
    fn the_single_argument_form_fills_every_slot_in_both_forms() {
        let mut base: HashMap<String, &'static str> = HashMap::new();
        base.insert("a".to_string(), "{} and {}");
        base.insert("b".to_string(), "{0} and {0}");
        base.insert("c".to_string(), "{1} before {0}");
        let tr = Translations {
            base,
            namespaces: RwLock::new(HashMap::new()),
            language: "en".to_string(),
            user_lang_dir: None,
        };
        assert_eq!(tr.get_fmt("a", "x"), "x and x");
        assert_eq!(tr.get_fmt("b", "x"), "x and x");
        assert_eq!(tr.get_fmt2("c", "x", "y"), "y before x");
        assert_eq!(tr.get_args("c", &["x", "y"]), "y before x");
    }

    #[test]
    fn get_args_replaces_in_order() {
        let mut base: HashMap<String, &'static str> = HashMap::new();
        base.insert("cli.x".to_string(), "alive: {} (port {}, version {})");
        let tr = Translations {
            base,
            namespaces: RwLock::new(HashMap::new()),
            language: "en".to_string(),
            user_lang_dir: None,
        };
        assert_eq!(
            tr.get_args("cli.x", &["host", "1234", "9.9.9"]),
            "alive: host (port 1234, version 9.9.9)"
        );
    }

    #[test]
    fn register_namespace_from_lang_dir() {
        let tmp = temp_dir("ns");
        std::fs::write(
            tmp.join("en.toml"),
            "[plugin.x]\ntitle = \"Refresh\"\nbody = \"OnlyEn\"\n",
        )
        .unwrap();
        std::fs::write(tmp.join("ko.toml"), "[plugin.x]\ntitle = \"새로고침\"\n").unwrap();

        let tr = Translations {
            base: HashMap::new(),
            namespaces: RwLock::new(HashMap::new()),
            language: "ko".to_string(),
            user_lang_dir: None,
        };
        tr.register_namespace("com.example.x", &tmp);
        assert_eq!(tr.get("plugin.x.title"), "새로고침");
        assert_eq!(tr.get("plugin.x.body"), "OnlyEn");

        std::fs::remove_dir_all(&tmp).ok();
    }

    /// 플러그인의 빈 번역도 영어 기본값을 유지한다.
    #[test]
    fn blank_namespace_values_fall_back_to_the_plugin_english_string() {
        let tmp = temp_dir("ns-blank");
        std::fs::write(
            tmp.join("en.toml"),
            "[plugin.x]\ntitle = \"Refresh\"\nbody = \"Reload the view\"\n",
        )
        .unwrap();
        std::fs::write(
            tmp.join("ko.toml"),
            "[plugin.x]\ntitle = \"새로고침\"\nbody = \"\"\n",
        )
        .unwrap();

        let tr = Translations {
            base: HashMap::new(),
            namespaces: RwLock::new(HashMap::new()),
            language: "ko".to_string(),
            user_lang_dir: None,
        };
        tr.register_namespace("com.example.x", &tmp);

        assert_eq!(tr.get("plugin.x.title"), "새로고침");
        assert_eq!(
            tr.get("plugin.x.body"),
            "Reload the view",
            "빈 값이 영어를 덮으면 화면에 라벨 없는 자리가 남는다"
        );

        std::fs::remove_dir_all(&tmp).ok();
    }

    /// 의도적으로 빈 문자를 나타내는 U+2060은 플러그인 번역에서도 유지한다.
    #[test]
    fn zero_width_namespace_values_survive_the_blank_rule() {
        let tmp = temp_dir("ns-zw");
        std::fs::write(tmp.join("en.toml"), "[plugin.x]\nsuffix = \"EN\"\n").unwrap();
        std::fs::write(tmp.join("ko.toml"), "[plugin.x]\nsuffix = \"\u{2060}\"\n").unwrap();

        let tr = Translations {
            base: HashMap::new(),
            namespaces: RwLock::new(HashMap::new()),
            language: "ko".to_string(),
            user_lang_dir: None,
        };
        tr.register_namespace("com.example.x", &tmp);

        assert_eq!(tr.get("plugin.x.suffix"), "\u{2060}");

        std::fs::remove_dir_all(&tmp).ok();
    }
}

#[cfg(test)]
mod pack_size_and_discovery_tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        // 시각 해상도에 의존하지 않도록 PID와 단조 카운터로 임시 경로를 구분한다.
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "tasty-i18n-{tag}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write(path: PathBuf, content: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, content).unwrap();
    }

    const HEADER: &str = "[meta]\nname = \"Big\"\n\n[font]\nbuiltin = true\n\n[filler]\n";

    /// `[font]`/`[meta]` 를 갖춘 유효한 팩을 만들되 본문을 `bytes` 근처까지 채운다.
    fn pack_of_size(bytes: usize) -> String {
        let mut s = String::from(HEADER);
        let mut i = 0usize;
        while s.len() < bytes {
            s.push_str(&format!("k{i} = \"{}\"\n", "x".repeat(64)));
            i += 1;
        }
        s
    }

    #[test]
    fn a_pack_over_the_limit_is_rejected_before_it_is_parsed() {
        let dir = temp_dir("toobig");
        let path = dir.join("big").join("pack.toml");
        write(path.clone(), &pack_of_size(MAX_PACK_BYTES as usize + 4096));

        match load_pack(&path, "big") {
            Err(PackError::TooLarge { bytes, max }) => {
                assert_eq!(max, MAX_PACK_BYTES);
                assert!(bytes > MAX_PACK_BYTES, "실제 크기를 보고해야 한다: {bytes}");
            }
            other => panic!("expected TooLarge, got {other:?}"),
        }
        assert!(matches!(
            load_pack_head(&path),
            Err(PackError::TooLarge { .. })
        ));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn an_oversized_pack_does_not_appear_in_the_language_list() {
        let dir = temp_dir("toobig-scan");
        write(
            dir.join("big").join("pack.toml"),
            &pack_of_size(MAX_PACK_BYTES as usize + 4096),
        );
        write(
            dir.join("ok").join("pack.toml"),
            "[meta]\nname = \"Ok\"\n\n[font]\nbuiltin = true\n",
        );
        let codes: Vec<String> = scan_languages(Some(&dir))
            .into_iter()
            .map(|e| e.code)
            .collect();
        assert!(codes.contains(&"ok".to_string()));
        assert!(
            !codes.contains(&"big".to_string()),
            "상한 초과 팩이 목록에 올랐다: {codes:?}"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn selecting_an_oversized_pack_falls_back_to_english_without_touching_the_setting() {
        let dir = temp_dir("toobig-load");
        write(
            dir.join("big").join("pack.toml"),
            &pack_of_size(MAX_PACK_BYTES as usize + 4096),
        );
        let (tr, report) = Translations::load_from("big", Some(&dir));
        assert_eq!(tr.language, "en");
        assert_eq!(tr.get("button.ok"), "OK");
        assert_eq!(report.requested, "big");
        assert_eq!(report.effective, "en");
        assert!(report.fell_back());
        assert!(matches!(
            report.outcome,
            LoadOutcome::PackInvalid {
                error: PackError::TooLarge { .. },
                ..
            }
        ));
        assert!(report.user_warning().is_some());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_pack_just_under_the_limit_still_loads() {
        let dir = temp_dir("justunder");
        let path = dir.join("ok").join("pack.toml");
        write(
            path.clone(),
            &pack_of_size(MAX_PACK_BYTES as usize - 65_536),
        );
        let pack = load_pack(&path, "ok").expect("상한 아래 팩은 통과해야 한다");
        assert_eq!(pack.font, FontDecl::Builtin);
        assert_eq!(pack.display_name.as_deref(), Some("Big"));
        assert!(!pack.strings.is_empty());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn the_builtin_override_file_is_capped_too() {
        let dir = temp_dir("override-big");
        write(
            dir.join("ko.toml"),
            &pack_of_size(MAX_PACK_BYTES as usize + 4096),
        );
        let ko = scan_languages(Some(&dir))
            .into_iter()
            .find(|e| e.code == "ko")
            .expect("내장 ko 는 항상 목록에 있다");
        assert_eq!(
            ko.source,
            LanguageSource::Builtin,
            "상한 초과 오버라이드는 적용된 것으로 표시되면 안 된다"
        );
        let (tr, _) = Translations::load_from("ko", Some(&dir));
        assert_eq!(tr.language, "ko");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// 같은 입력의 발견/로드 판정을 대조한다.
    #[test]
    fn the_discovery_path_accepts_and_rejects_exactly_what_the_load_path_does() {
        let dir = temp_dir("parity");
        let cases: &[(&str, &str)] = &[
            (
                "good",
                "[meta]\nname = \"G\"\n\n[font]\nbuiltin = true\n\n[a]\nb = \"c\"\n",
            ),
            ("nofont", "[meta]\nname = \"N\"\n\n[a]\nb = \"c\"\n"),
            ("badfont", "[font]\nfile = \"\"\n"),
            ("badtoml", "[font\nbuiltin = true\n"),
            ("notatable", "42\n"),
        ];
        for (code, body) in cases {
            let path = dir.join(code).join("pack.toml");
            write(path.clone(), body);
            let full = load_pack(&path, code);
            let head = load_pack_head(&path);
            assert_eq!(
                full.is_ok(),
                head.is_ok(),
                "'{code}' 에서 발견과 로드의 판정이 갈렸다: full={full:?} head={head:?}"
            );
            if let (Ok(full), Ok(head)) = (full, head) {
                assert_eq!(full.font, head.font);
                assert_eq!(full.display_name, head.display_name);
            }
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    /// 실행 시간 대신 호출 횟수로 발견 과정에서 문자열을 펼치지 않는지 확인한다.
    #[test]
    fn discovery_never_flattens_the_string_table() {
        let dir = temp_dir("cost");
        for code in ["aa", "bb", "cc"] {
            write(dir.join(code).join("pack.toml"), &pack_of_size(256 * 1024));
        }

        flatten_probe::reset();
        let listed = scan_languages(Some(&dir));
        let scan_flattens = flatten_probe::count();
        assert!(
            listed.iter().any(|e| e.code == "aa"),
            "팩이 목록에 올라야 이 측정이 의미가 있다"
        );
        assert_eq!(
            scan_flattens, 0,
            "발견이 문자열을 flatten 했다 — `pack_entry` 가 `load_pack` 으로 되돌아갔다"
        );

        // 실제 로드는 계수를 늘려, 계수기 자체가 동작하는지도 확인한다.
        flatten_probe::reset();
        load_pack(&dir.join("aa").join("pack.toml"), "aa").unwrap();
        assert!(flatten_probe::count() > 0);

        flatten_probe::reset();
        load_pack_head(&dir.join("aa").join("pack.toml")).unwrap();
        assert_eq!(flatten_probe::count(), 0);

        std::fs::remove_dir_all(&dir).ok();
    }
}
