//! DTCG 토큰 파싱, 별칭 해석, Rust 코드 생성.
//!
//! 저장된 JSON은 `$value`에 문자열·숫자·불리언을 쓰며 파서는 이를 문자열로 바꾼다.
//! 길이는 px·단위 없는 값·em이 섞여 있고, 그림자와 cubicBezier는 CSS 문자열이다.
//! 별칭은 `{tier.name}` 형태이며 계층을 건너뛰거나 여러 토큰을 거칠 수 있다.
//! 생성된 primitive 상수는 `pub(crate)`로 외부 접근을 막는다.

mod accessor;
mod calc;
mod duration_accessor;
mod tracking_accessor;

/// 토큰 경로와 SIZING 필드의 대응표. 접근자 생성과 치수 대조 시험에서 공유한다.
pub use accessor::SEMANTIC_DIM_TO_THEME_FIELD;
use accessor::{generate_component_accessors, generate_semantic_color_accessors};
use tracking_accessor::generate_tracking_accessors;

use std::collections::BTreeMap;
use std::fmt;

/// 3-tier 중 어느 계층의 토큰인지.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Tier {
    Primitive,
    Semantic,
    Component,
}

impl Tier {
    pub const ALL: [Tier; 3] = [Tier::Primitive, Tier::Semantic, Tier::Component];

    pub fn as_str(self) -> &'static str {
        match self {
            Tier::Primitive => "primitive",
            Tier::Semantic => "semantic",
            Tier::Component => "component",
        }
    }
}

/// 토큰 하나. `latte` 는 `$extensions["com.tasty.mode"].latte` 값(리터럴 또는 alias).
#[derive(Debug, Clone)]
pub struct Token {
    pub tier: Tier,
    /// tier 내 이름 (kebab-case, 예: `size-12`).
    pub name: String,
    /// `$type` (예: `color` / `dimension` / `duration` / `number` / `fontWeight`).
    pub ty: String,
    /// `$value` — 리터럴 또는 `{tier.name}` alias.
    pub value: String,
    pub latte: Option<String>,
}

impl Token {
    /// alias 참조에서 쓰는 전체 경로 (`primitive.size-12`).
    pub fn path(&self) -> String {
        format!("{}.{}", self.tier.as_str(), self.name)
    }
}

/// `"{tier.name}"` 형태면 내부 경로를 돌려준다.
pub fn alias_target(raw: &str) -> Option<&str> {
    raw.strip_prefix('{').and_then(|s| s.strip_suffix('}'))
}

/// 테마 모드. latte 해석 시 각 hop 에서 latte 오버라이드를 우선한다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeMode {
    Mocha,
    Latte,
}

/// 파싱된 토큰 전체. key = `tier.name` 전체 경로 (BTreeMap → 결정적 순서).
#[derive(Debug)]
pub struct TokenSet {
    tokens: BTreeMap<String, Token>,
}

impl TokenSet {
    pub fn get(&self, path: &str) -> Option<&Token> {
        self.tokens.get(path)
    }

    pub fn iter(&self) -> impl Iterator<Item = &Token> {
        self.tokens.values()
    }

    pub fn len(&self) -> usize {
        self.tokens.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tokens.is_empty()
    }

    pub fn tier_count(&self, tier: Tier) -> usize {
        self.tokens.values().filter(|t| t.tier == tier).count()
    }

    /// 별칭을 끝까지 따라가 최종 값을 반환한다.
    /// Latte 모드에서는 각 토큰의 Latte 값을 우선한다.
    pub fn resolve(&self, path: &str, mode: ThemeMode) -> Result<String, ResolveError> {
        let mut current = path.to_string();
        // 순환 참조나 지나치게 긴 체인은 32단계에서 중단한다.
        for _ in 0..32 {
            let token = self
                .get(&current)
                .ok_or_else(|| ResolveError::Missing(current.clone()))?;
            let raw = match (mode, &token.latte) {
                (ThemeMode::Latte, Some(latte)) => latte.as_str(),
                _ => token.value.as_str(),
            };
            match alias_target(raw) {
                Some(target) => current = target.to_string(),
                None => return Ok(raw.to_string()),
            }
        }
        Err(ResolveError::Cycle(path.to_string()))
    }
}

#[derive(Debug)]
pub enum ParseError {
    Json(serde_json::Error),
    /// 필수 계층, 문자열 `$type`, 스칼라 `$value` 형식이 맞지 않음.
    Structure(String),
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseError::Json(e) => write!(f, "invalid JSON: {e}"),
            ParseError::Structure(msg) => write!(f, "unexpected DTCG structure: {msg}"),
        }
    }
}

impl std::error::Error for ParseError {}

impl From<serde_json::Error> for ParseError {
    fn from(e: serde_json::Error) -> Self {
        ParseError::Json(e)
    }
}

#[derive(Debug)]
pub enum ResolveError {
    /// alias 가 존재하지 않는 토큰을 가리킴.
    Missing(String),
    /// alias 순환 (또는 비정상적으로 긴 체인).
    Cycle(String),
}

impl fmt::Display for ResolveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ResolveError::Missing(path) => write!(f, "alias target not found: {path}"),
            ResolveError::Cycle(path) => write!(f, "alias cycle detected from: {path}"),
        }
    }
}

impl std::error::Error for ResolveError {}

/// DTCG json 텍스트를 파싱한다. 최상위 `primitive`/`semantic`/`component` 3그룹 필수.
pub fn parse(text: &str) -> Result<TokenSet, ParseError> {
    let root: serde_json::Value = serde_json::from_str(text)?;
    let obj = root
        .as_object()
        .ok_or_else(|| ParseError::Structure("root is not an object".into()))?;

    let mut tokens = BTreeMap::new();
    for tier in Tier::ALL {
        let group = obj
            .get(tier.as_str())
            .and_then(|v| v.as_object())
            .ok_or_else(|| {
                ParseError::Structure(format!("missing tier group `{}`", tier.as_str()))
            })?;
        collect(tier, "", group, &mut tokens)?;
    }
    Ok(TokenSet { tokens })
}

/// 문자열·숫자·불리언 값을 문자열로 통일한다.
fn json_scalar_to_string(v: &serde_json::Value) -> Option<String> {
    match v {
        serde_json::Value::String(s) => Some(s.clone()),
        serde_json::Value::Number(n) => Some(n.to_string()),
        serde_json::Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

fn collect(
    tier: Tier,
    prefix: &str,
    group: &serde_json::Map<String, serde_json::Value>,
    out: &mut BTreeMap<String, Token>,
) -> Result<(), ParseError> {
    for (key, val) in group {
        if key.starts_with('$') {
            continue;
        }
        // 중첩 그룹은 점으로 구분한 별칭 경로로 수집한다.
        let name = if prefix.is_empty() {
            key.clone()
        } else {
            format!("{prefix}.{key}")
        };
        let node = val.as_object().ok_or_else(|| {
            ParseError::Structure(format!("{}.{name}: not an object", tier.as_str()))
        })?;
        if node.contains_key("$value") {
            let ty = node
                .get("$type")
                .and_then(|v| v.as_str())
                .ok_or_else(|| {
                    ParseError::Structure(format!("{}.{name}: missing $type", tier.as_str()))
                })?
                .to_string();
            let value = node
                .get("$value")
                .and_then(json_scalar_to_string)
                .ok_or_else(|| {
                    ParseError::Structure(format!(
                        "{}.{name}: $value is not a string/number/bool",
                        tier.as_str()
                    ))
                })?;
            let latte = node
                .get("$extensions")
                .and_then(|e| e.get("com.tasty.mode"))
                .and_then(|m| m.get("latte"))
                .and_then(json_scalar_to_string);
            let path = format!("{}.{name}", tier.as_str());
            let token = Token {
                tier,
                name,
                ty,
                value,
                latte,
            };
            if out.insert(path.clone(), token).is_some() {
                return Err(ParseError::Structure(format!("duplicate token: {path}")));
            }
        } else {
            collect(tier, &name, node, out)?;
        }
    }
    Ok(())
}

/// 생성 파일과 제외 사유. `files`는 이 크레이트, `type_appearance_files`는
/// `tasty-type-appearance/src`에 쓴다. Theme 접근자를 그쪽에 두어 의존 방향을 유지한다.
#[derive(Debug)]
pub struct Generated {
    pub files: Vec<(&'static str, String)>,
    pub type_appearance_files: Vec<(&'static str, String)>,
    pub skips: Vec<String>,
}

/// 색을 제외한 토큰의 Rust 표현. 테마에 따라 달라지지 않는 값만 상수로 만든다.
#[derive(Debug, Clone, Copy, PartialEq)]
enum RustKind {
    /// dimension (px 또는 무단위) → `LogicalPx`.
    Px(f32),
    /// duration → `f32` (ms).
    Ms(f32),
    /// number (line-height / scale / opacity …) → `f32`.
    Num(f32),
    /// fontWeight → `u16`.
    Weight(u16),
}

impl RustKind {
    fn type_name(self) -> &'static str {
        match self {
            RustKind::Px(_) => "LogicalPx",
            RustKind::Ms(_) | RustKind::Num(_) => "f32",
            RustKind::Weight(_) => "u16",
        }
    }

    fn literal(self) -> String {
        match self {
            RustKind::Px(v) => format!("LogicalPx({v:?})"),
            RustKind::Ms(v) | RustKind::Num(v) => format!("{v:?}"),
            RustKind::Weight(v) => format!("{v}"),
        }
    }
}

/// 생성 제외 사유. `Color` 는 요약 한 줄로, 나머지는 토큰별 로그.
#[derive(Debug)]
enum Skip {
    /// 색 상수는 만들지 않는다. 런타임 테마의 색은 Theme 접근자로 읽는다.
    Color,
    /// 상수 생성을 지원하지 않는 `$type` (fontFamily / shadow / cubicBezier 등).
    Type(String),
    /// em 단위 dimension (letter-spacing 계열) — `LogicalPx` 상수로 표현할 수 없다.
    /// semantic·component 계층은 글자 크기를 받는 Theme 접근자로 생성한다(`generated_tracking.rs`).
    EmUnit(String),
    /// 별칭의 최종 값을 숫자로 해석할 수 없음.
    Unparsable(String),
    /// `calc()` 식 — 배율마다 항을 반올림해 더해야 하므로 상수 없이 Theme 접근자로만 생성한다.
    Calc,
}

/// component 토큰 이름 → 하위 모듈 그룹. 최장 일치 우선
/// (예: `switch-overlay-*` 는 `switch` 가 아니라 `switch_overlay`).
/// 목록에 없으면 첫 세그먼트로 fallback.
const COMPONENT_GROUPS: &[&str] = &[
    "icon-button",
    "help-hint",
    "status-dot",
    "move-source",
    "switch-overlay",
    "tree-row",
    "plugins-list",
    "button",
    "badge",
    "tag",
    "kbd",
    "input",
    "select",
    "checkbox",
    "switch",
    "tab",
    "menu",
    "toast",
    "tooltip",
    "banner",
    "spinner",
    "table",
    "settings",
    "swatch",
    "remote",
    "preset",
    "titlebar",
    "sidebar",
];

/// component 토큰 이름을 (모듈명 snake_case, 모듈 내 나머지 kebab) 으로 분해.
fn component_split(name: &str) -> (String, String) {
    let mut best: Option<&str> = None;
    for p in COMPONENT_GROUPS {
        if name.len() > p.len()
            && name.starts_with(p)
            && name.as_bytes()[p.len()] == b'-'
            && best.is_none_or(|b| p.len() > b.len())
        {
            best = Some(p);
        }
    }
    let (module, rest) = match best {
        Some(p) => (p.replace('-', "_"), name[p.len() + 1..].to_string()),
        None => match name.split_once('-') {
            Some((head, rest)) => (head.replace('-', "_"), rest.to_string()),
            None => (name.replace('-', "_"), name.to_string()),
        },
    };
    (module_ident(module), rest)
}

/// 모듈명이 Rust 키워드(예: `mod-hint-*`의 `mod`)면 raw identifier로 쓴다.
fn module_ident(module: String) -> String {
    const KEYWORDS: &[&str] = &[
        "abstract", "as", "async", "await", "become", "box", "break", "const", "continue", "do",
        "dyn", "else", "enum", "extern", "false", "final", "fn", "for", "gen", "if", "impl", "in",
        "let", "loop", "macro", "match", "mod", "move", "mut", "override", "priv", "pub", "ref",
        "return", "static", "struct", "trait", "true", "try", "type", "typeof", "unsafe",
        "unsized", "use", "virtual", "where", "while", "yield",
    ];
    if KEYWORDS.contains(&module.as_str()) {
        format!("r#{module}")
    } else {
        module
    }
}

/// kebab-case → SCREAMING_SNAKE_CASE 상수명.
fn const_name(kebab: &str) -> String {
    kebab.replace(['-', '.'], "_").to_uppercase()
}

/// 토큰 분류: 생성 대상이면 Rust 표현, 아니면 제외 사유.
fn classify(set: &TokenSet, token: &Token) -> Result<RustKind, Skip> {
    let terminal = match set.resolve(&token.path(), ThemeMode::Mocha) {
        Ok(v) => v,
        Err(e) => return Err(Skip::Unparsable(e.to_string())),
    };
    if token.value.trim().starts_with("calc(") {
        return Err(Skip::Calc);
    }
    match token.ty.as_str() {
        "color" => Err(Skip::Color),
        "dimension" => {
            if terminal.ends_with("em") {
                return Err(Skip::EmUnit(terminal));
            }
            let stripped = terminal.strip_suffix("px").unwrap_or(&terminal);
            stripped
                .trim()
                .parse::<f32>()
                .map(RustKind::Px)
                .map_err(|_| Skip::Unparsable(terminal.clone()))
        }
        "duration" => {
            let stripped = terminal.strip_suffix("ms").unwrap_or(&terminal);
            stripped
                .trim()
                .parse::<f32>()
                .map(RustKind::Ms)
                .map_err(|_| Skip::Unparsable(terminal.clone()))
        }
        "number" => terminal
            .trim()
            .parse::<f32>()
            .map(RustKind::Num)
            .map_err(|_| Skip::Unparsable(terminal.clone())),
        "fontWeight" => terminal
            .trim()
            .parse::<u16>()
            .map(RustKind::Weight)
            .map_err(|_| Skip::Unparsable(terminal.clone())),
        other => Err(Skip::Type(other.to_string())),
    }
}

/// 참조 표현식: `from` 컨텍스트에서 `target` 토큰의 생성 상수를 가리키는 경로.
fn const_expr(from: Tier, target: &Token) -> String {
    match target.tier {
        Tier::Primitive => {
            let n = const_name(&target.name);
            match from {
                Tier::Primitive => n,
                Tier::Semantic => format!("super::primitive::{n}"),
                Tier::Component => format!("crate::generated::primitive::{n}"),
            }
        }
        Tier::Semantic => {
            let n = const_name(&target.name);
            match from {
                Tier::Semantic => n,
                _ => format!("crate::generated::semantic::{n}"),
            }
        }
        Tier::Component => {
            let (module, rest) = component_split(&target.name);
            let n = const_name(&rest);
            match from {
                Tier::Component => format!("super::{module}::{n}"),
                _ => format!("crate::generated::component::{module}::{n}"),
            }
        }
    }
}

/// 상수와 원본 토큰 경로·별칭·값을 담은 문서 주석을 만든다.
fn emit_const(
    set: &TokenSet,
    kinds: &BTreeMap<String, Result<RustKind, Skip>>,
    token: &Token,
    kind: RustKind,
    vis: &str,
    name: &str,
    indent: &str,
) -> String {
    let terminal = set
        .resolve(&token.path(), ThemeMode::Mocha)
        .expect("classify 통과 토큰은 resolve 가능");
    // sentinel (radius-full 계열): 9999px 는 "완전 원형" 상한값이지 실측 치수가 아니다.
    let sentinel = if terminal == "9999px" {
        " (sentinel — 완전 원형용 상한값)"
    } else {
        ""
    };

    let (doc, expr) = match alias_target(&token.value) {
        Some(target_path) => {
            let doc = format!(
                "`{}` → `{}` = {terminal}{sentinel}",
                token.path(),
                token.value
            );
            // 대상이 같은 kind 로 생성됐을 때만 상수 참조 — 아니면 리터럴 fallback.
            let target_generated = set
                .get(target_path)
                .filter(|t| matches!(kinds.get(&t.path()), Some(Ok(k)) if *k == kind));
            let expr = match target_generated {
                Some(t) => const_expr(token.tier, t),
                None => kind.literal(),
            };
            (doc, expr)
        }
        None => (
            format!("`{}` = {terminal}{sentinel}", token.path()),
            kind.literal(),
        ),
    };
    let unit = match kind {
        RustKind::Ms(_) => " (ms)",
        _ => "",
    };
    let ty = kind.type_name();
    // freshness 검사가 텍스트를 비교하므로 생성 단계에서 rustfmt 줄바꿈에 맞춘다.
    let one_line = format!("{indent}{vis} const {name}: {ty} = {expr};");
    let decl = if one_line.chars().count() > 100 {
        format!("{indent}{vis} const {name}: {ty} =\n{indent}    {expr};")
    } else {
        one_line
    };
    format!("{indent}/// {doc}{unit}\n{decl}\n")
}

/// 토큰 상수 파일 4개와 Theme 접근자 파일 3개를 생성한다.
pub fn generate(set: &TokenSet) -> Generated {
    let mut skips: Vec<String> = Vec::new();
    let mut color_count = 0usize;

    let mut kinds: BTreeMap<String, Result<RustKind, Skip>> = BTreeMap::new();
    for token in set.iter() {
        kinds.insert(token.path(), classify(set, token));
    }
    for (path, kind) in &kinds {
        match kind {
            Ok(_) => {}
            Err(Skip::Color) => color_count += 1,
            Err(Skip::Type(ty)) => {
                skips.push(format!("{path}: $type {ty} — 상수 생성 미지원"));
            }
            Err(Skip::EmUnit(v)) => {
                skips.push(format!(
                    "{path}: em 단위 ({v}) — LogicalPx 상수 생성 스킵(자간 접근자는 generated_tracking.rs)"
                ));
            }
            Err(Skip::Unparsable(v)) => {
                skips.push(format!("{path}: 최종 값 파싱 실패 ({v}) — 생성 스킵"));
            }
            Err(Skip::Calc) => {
                skips.push(format!(
                    "{path}: calc 식 — 상수 없이 Theme 접근자로만 생성(generated_component.rs)"
                ));
            }
        }
    }
    if color_count > 0 {
        skips.push(format!(
            "color 토큰 {color_count}개 — 색 상수는 생성하지 않으며 Theme 접근자로 읽음"
        ));
    }

    let header = |tier_doc: &str| {
        format!(
            "//! Generated from `dtcg/tasty.tokens.json` — DO NOT EDIT.\n\
             //! 재생성: `cargo run -p tasty-design-tokens --bin generate`.\n\
             //!\n\
             {tier_doc}\n"
        )
    };

    let mut primitive = header(
        "//! Primitive 상수는 크레이트 내부에만 공개한다.\n\
         //! 외부 크레이트에서는 semantic 또는 component를 사용한다.",
    );
    primitive.push_str("#![allow(dead_code)] // 스케일 전체를 보존한다 — 미참조 엔트리 포함.\n\n");
    primitive.push_str("use tasty_type_geometry::length::LogicalPx;\n");
    for token in set.iter().filter(|t| t.tier == Tier::Primitive) {
        if let Some(Ok(kind)) = kinds.get(&token.path()) {
            primitive.push('\n');
            primitive.push_str(&emit_const(
                set,
                &kinds,
                token,
                *kind,
                "pub(crate)",
                &const_name(&token.name),
                "",
            ));
        }
    }

    let mut semantic = header(
        "//! 테마에 따라 바뀌지 않는 semantic 치수·글꼴·시간 상수.\n\
         //! 치수 상수는 SIZING 초깃값과 대조 시험에 쓴다. 위젯은 Theme 필드나 접근자로\n\
         //! 치수를 읽어 UI 배율과 반올림 정책을 따른다. 색은 런타임 Theme에서 읽는다.",
    );
    semantic.push('\n');
    semantic.push_str("use tasty_type_geometry::length::LogicalPx;\n");
    for token in set.iter().filter(|t| t.tier == Tier::Semantic) {
        if let Some(Ok(kind)) = kinds.get(&token.path()) {
            semantic.push('\n');
            semantic.push_str(&emit_const(
                set,
                &kinds,
                token,
                *kind,
                "pub",
                &const_name(&token.name),
                "",
            ));
        }
    }

    let mut by_module: BTreeMap<String, Vec<(&Token, RustKind, String)>> = BTreeMap::new();
    for token in set.iter().filter(|t| t.tier == Tier::Component) {
        if let Some(Ok(kind)) = kinds.get(&token.path()) {
            let (module, rest) = component_split(&token.name);
            by_module
                .entry(module)
                .or_default()
                .push((token, *kind, const_name(&rest)));
        }
    }
    let mut component = header(
        "//! 컴포넌트별 상수. 원본 토큰에 따라 semantic 또는 primitive를 참조한다.\n\
         //! 위젯의 치수는 Theme 필드나 접근자로 읽어 UI 배율 정책을 따른다.\n\
         //! 색 접근자는 tasty-type-appearance에 생성한다.",
    );
    for (module, entries) in &by_module {
        component.push('\n');
        component.push_str(&format!("pub mod {module} {{\n"));
        let uses_px = entries.iter().any(|(_, k, _)| matches!(k, RustKind::Px(_)));
        if uses_px {
            component.push_str("    use tasty_type_geometry::length::LogicalPx;\n");
        }
        for (token, kind, name) in entries {
            component.push('\n');
            component.push_str(&emit_const(set, &kinds, token, *kind, "pub", name, "    "));
        }
        component.push_str("}\n");
    }

    let module_root = header("//! primitive는 크레이트 내부에서만 접근할 수 있다.")
        + "\npub(crate) mod primitive;\n\npub mod semantic;\n\npub mod component;\n";

    // Theme 색 접근자는 type-appearance에 생성한다.
    let (semantic_color_accessors, semantic_color_skips) = generate_semantic_color_accessors(set);
    skips.extend(semantic_color_skips);

    let (component_accessors, accessor_skips) = generate_component_accessors(set);
    skips.extend(accessor_skips);

    Generated {
        files: vec![
            ("mod.rs", module_root),
            ("primitive.rs", primitive),
            ("semantic.rs", semantic),
            ("component.rs", component),
        ],
        type_appearance_files: vec![
            ("semantic_color_generated.rs", semantic_color_accessors),
            ("generated_component.rs", component_accessors),
            ("generated_tracking.rs", generate_tracking_accessors(set)),
        ],
        skips,
    }
}
