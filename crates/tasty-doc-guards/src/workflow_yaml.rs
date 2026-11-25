//! 워크플로 YAML을 좁은 표기로만 읽는다. 지원하지 않는 표기는 오류로 돌려준다(fail closed).
//!
//! YAML 전체를 해석하지 않는다. 읽을 수 있는 좁은 표기만 받고, 판정에 쓰는 값이 그 밖의 표기면
//! "지원하지 않는 표기"로 실패한다.
//! - 잡 머리는 `jobs:` 아래 2칸, 잡 키는 4칸, `steps:`는 4칸이다.
//! - 스텝은 6칸의 `- `로 시작하고(첫 키가 무엇이든 새 스텝), 스텝 키는 8칸, `with:` 아래 키는 10칸이다.
//! - 판정에 쓰는 값(`uses`·`id`·`if`·`continue-on-error`·`with.clean`·`runs-on`)은 한 줄 값이다. 작은·큰따옴표는
//!   벗겨 비교한다. 블록 스칼라(`|`·`>`)·여러 줄 값·줄 끝 주석은 받지 않는다.
//! - `runs-on`은 한 줄 값, 흐름 목록 `[a, b]`, 블록 목록(`- a`)을 받는다. 식(`${{ }}`)과 매핑은 받지 않는다.

/// 지원 밖 표기 오류에 붙는 문구.
pub const UNSUPPORTED: &str = "지원하지 않는 표기";

/// 키 하나. `inline`은 `키:` 뒤 같은 줄의 글자, `children`은 더 깊이 들여쓴 다음 줄들(원문 그대로)이다.
#[derive(Debug, Clone)]
pub struct Field {
    pub key: String,
    pub line: usize,
    pub inline: String,
    pub children: Vec<(usize, String)>,
}

/// 스텝 하나. `line`은 `- `가 있는 첫 줄이다.
#[derive(Debug)]
pub struct Step {
    pub job: String,
    pub line: usize,
    pub fields: Vec<Field>,
    /// 스텝의 원문 줄(주석·빈 줄 포함). 첫 줄은 `- `를 떼고 스텝 키 깊이(8칸)로 맞춘 꼴이다.
    pub lines: Vec<(usize, String)>,
}

/// 잡 하나. `raw`는 잡 머리 줄부터 다음 잡 머리 직전(또는 `jobs:` 블록 끝)까지의 원문 줄이다.
#[derive(Debug)]
pub struct Job {
    pub name: String,
    pub line: usize,
    pub fields: Vec<Field>,
    pub steps: Vec<Step>,
    pub raw: Vec<(usize, String)>,
}

/// 스텝의 실행 조건 두 값. 없으면 None이다.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct StepConditions {
    pub if_expr: Option<String>,
    pub continue_on_error: Option<bool>,
}

pub fn indent_of(line: &str) -> usize {
    line.len() - line.trim_start_matches(' ').len()
}

/// 빈 줄과 전체 주석 줄은 구조에서 뺀다.
pub fn is_blank(line: &str) -> bool {
    let t = line.trim();
    t.is_empty() || t.starts_with('#')
}

/// `키: 값`을 나눈다. 키는 영문·숫자·`-`·`_`만 받는다.
pub fn split_key(text: &str) -> Option<(String, String)> {
    let (k, v) = text.split_once(':')?;
    if k.is_empty()
        || !k
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return None;
    }
    if !(v.is_empty() || v.starts_with(' ')) {
        return None;
    }
    Some((k.to_string(), v.trim().to_string()))
}

/// 들여쓰기 `indent`의 키 줄들을 Field로 묶는다. 첫 줄은 이미 `- `를 뗀 상태로 줄 수 있다.
/// 같은 깊이에 키가 아닌 줄이 있으면 오류다.
pub fn fields_at(
    lines: &[(usize, String)],
    indent: usize,
    file: &str,
    errs: &mut Vec<String>,
) -> Vec<Field> {
    let mut out: Vec<Field> = Vec::new();
    for (no, raw) in lines {
        if is_blank(raw) {
            if let Some(f) = out.last_mut() {
                f.children.push((*no, raw.clone()));
            }
            continue;
        }
        let ind = indent_of(raw);
        if ind > indent {
            match out.last_mut() {
                Some(f) => f.children.push((*no, raw.clone())),
                None => errs.push(format!("{file}:{no} {UNSUPPORTED}: 키 없이 들여쓴 줄")),
            }
            continue;
        }
        if ind < indent {
            errs.push(format!("{file}:{no} {UNSUPPORTED}: 들여쓰기 {ind}칸"));
            continue;
        }
        match split_key(raw.trim_start()) {
            Some((key, _)) if out.iter().any(|f| f.key == key) => {
                errs.push(format!("{file}:{no} {UNSUPPORTED}: 키 {key}가 두 번 있다"));
            }
            Some((key, inline)) => out.push(Field {
                key,
                line: *no,
                inline,
                children: Vec::new(),
            }),
            None => errs.push(format!(
                "{file}:{no} {UNSUPPORTED}: 키 줄이 아니다: {}",
                raw.trim()
            )),
        }
    }
    // 끝의 빈 줄과, 키보다 깊지 않은 주석 줄(다음 스텝 앞 주석)만 뗀다. 키보다 깊은 `#` 줄은 블록 스칼라
    // 본문일 수 있으므로 남긴다(예: run 블록 끝의 `#Requires`).
    for f in &mut out {
        while f
            .children
            .last()
            .is_some_and(|(_, l)| l.trim().is_empty() || (is_blank(l) && indent_of(l) <= indent))
        {
            f.children.pop();
        }
    }
    out
}

/// 워크플로를 잡과 스텝으로 나눈다. 구조 오류는 `errs`에 쌓는다.
pub fn parse(file: &str, text: &str) -> (Vec<Job>, Vec<String>) {
    let mut errs = Vec::new();
    let text = text.replace("\r\n", "\n");
    let lines: Vec<(usize, String)> = text
        .lines()
        .enumerate()
        .map(|(i, l)| (i + 1, l.to_string()))
        .collect();
    // 최상위(들여쓰기 0) 줄은 모두 키이고 한 번씩만 나온다. 두 번째 `jobs:` 같은 블록을 조용히 빼지 않는다.
    let mut top_keys: Vec<String> = Vec::new();
    for (no, l) in lines
        .iter()
        .filter(|(_, l)| !is_blank(l) && indent_of(l) == 0)
    {
        match split_key(l) {
            Some((key, _)) if top_keys.contains(&key) => errs.push(format!(
                "{file}:{no} {UNSUPPORTED}: 최상위 키 {key}가 두 번 있다"
            )),
            Some((key, _)) => top_keys.push(key),
            None => errs.push(format!(
                "{file}:{no} {UNSUPPORTED}: 최상위 줄이 키가 아니다: {l}"
            )),
        }
    }
    let Some(start) = lines.iter().position(|(_, l)| l.starts_with("jobs:")) else {
        errs.push(format!(
            "{file}:1 {UNSUPPORTED}: 들여쓰기 없는 `jobs:` 줄이 없다"
        ));
        return (Vec::new(), errs);
    };
    if lines[start].1 != "jobs:" {
        errs.push(format!(
            "{file}:{} {UNSUPPORTED}: `jobs:` 뒤에 다른 글자가 있다",
            lines[start].0
        ));
        return (Vec::new(), errs);
    }
    let mut jobs = Vec::new();
    let mut k = start + 1;
    while k < lines.len() {
        let (no, raw) = &lines[k];
        if is_blank(raw) {
            k += 1;
            continue;
        }
        if indent_of(raw) == 0 {
            break;
        }
        let head = raw.trim();
        // 잡 머리는 같은 줄 값이 없는 `키:`다. 줄 끝 주석도 같은 줄 값으로 보아 받지 않는다.
        let name = match split_key(head) {
            Some((key, value)) if indent_of(raw) == 2 && value.is_empty() => key,
            _ => {
                errs.push(format!(
                    "{file}:{no} {UNSUPPORTED}: 잡 머리가 아니다: {head}"
                ));
                k += 1;
                continue;
            }
        };
        let mut raw_lines = vec![lines[k].clone()];
        let mut body = Vec::new();
        k += 1;
        while k < lines.len() {
            let (_, l) = &lines[k];
            if !is_blank(l) && indent_of(l) <= 2 {
                break;
            }
            body.push(lines[k].clone());
            k += 1;
        }
        raw_lines.extend(body.iter().cloned());
        let fields = fields_at(&body, 4, file, &mut errs);
        let steps = fields
            .iter()
            .find(|f| f.key == "steps")
            .map(|f| parse_steps(&name, f, file, &mut errs))
            .unwrap_or_default();
        jobs.push(Job {
            name,
            line: *no,
            fields,
            steps,
            raw: raw_lines,
        });
    }
    (jobs, errs)
}

fn parse_steps(job: &str, steps: &Field, file: &str, errs: &mut Vec<String>) -> Vec<Step> {
    if !steps.inline.is_empty() {
        errs.push(format!(
            "{file}:{} {UNSUPPORTED}: steps 값이 같은 줄에 있다",
            steps.line
        ));
        return Vec::new();
    }
    let mut groups: Vec<(usize, Vec<(usize, String)>)> = Vec::new();
    for (no, raw) in &steps.children {
        if !is_blank(raw) && indent_of(raw) == 6 {
            match raw.strip_prefix("      - ") {
                Some(rest) if is_blank(rest) => {
                    errs.push(format!("{file}:{no} {UNSUPPORTED}: 스텝 첫 줄에 키가 없다"));
                    continue;
                }
                Some(rest) => {
                    groups.push((*no, vec![(*no, format!("{}{rest}", " ".repeat(8)))]));
                    continue;
                }
                None => {
                    errs.push(format!(
                        "{file}:{no} {UNSUPPORTED}: 스텝 깊이의 줄이 `- `로 시작하지 않는다"
                    ));
                    continue;
                }
            }
        }
        if !is_blank(raw) && indent_of(raw) < 6 {
            errs.push(format!(
                "{file}:{no} {UNSUPPORTED}: 스텝 들여쓰기가 6칸이 아니다"
            ));
            continue;
        }
        match groups.last_mut() {
            Some((_, g)) => g.push((*no, raw.clone())),
            None if is_blank(raw) => {}
            None => errs.push(format!("{file}:{no} {UNSUPPORTED}: 스텝 밖의 줄")),
        }
    }
    groups
        .into_iter()
        .map(|(line, g)| Step {
            job: job.to_string(),
            line,
            fields: fields_at(&g, 8, file, errs),
            lines: g,
        })
        .collect()
}

/// 판정에 쓰는 한 줄 스칼라를 좁은 꼴로만 읽는다. 받는 꼴은 셋이다.
/// - 작은따옴표: `'…'`, 안의 작은따옴표는 `''`로만 쓴다.
/// - 큰따옴표: `"…"`, 안에 역슬래시(이스케이프)와 큰따옴표가 없다.
/// - 따옴표 없는 값: YAML 지시 문자(`!`·`&`·`*`·`[`·`{`·`|`·`>` 등)로 시작하지 않고, 주석(` #`)·`: `·탭이 없다.
///
/// 따옴표 값 뒤에 다른 글자(주석 포함)가 있으면 받지 않는다.
pub fn narrow_scalar(raw: &str) -> Result<String, &'static str> {
    let v = raw.trim();
    if v.is_empty() {
        return Err("값이 비었다");
    }
    if let Some(rest) = v.strip_prefix('\'') {
        let Some(inner) = rest.strip_suffix('\'') else {
            return Err("작은따옴표 값 뒤에 다른 글자가 있다");
        };
        if inner.replace("''", "").contains('\'') {
            return Err("작은따옴표 값 안에 짝 없는 작은따옴표가 있다");
        }
        return Ok(inner.replace("''", "'"));
    }
    if let Some(rest) = v.strip_prefix('"') {
        let Some(inner) = rest.strip_suffix('"') else {
            return Err("큰따옴표 값 뒤에 다른 글자가 있다");
        };
        if inner.contains(['\\', '"']) {
            return Err("큰따옴표 값 안에 이스케이프나 큰따옴표가 있다");
        }
        return Ok(inner.to_string());
    }
    if v.starts_with([
        '!', '&', '*', '[', ']', '{', '}', '|', '>', '%', '@', '`', '#', ',', '?', ':', '-', '\'',
        '"',
    ]) {
        return Err("따옴표 없는 값이 YAML 지시 문자로 시작한다");
    }
    if v.contains(" #") || v.contains(": ") || v.ends_with(':') || v.contains('\t') {
        return Err("따옴표 없는 값에 주석이나 `: `가 있다");
    }
    Ok(v.to_string())
}

/// 판정에 쓰는 한 줄 값. 없으면 None, 받지 않는 표기면 Err.
pub fn scalar(file: &str, fields: &[Field], key: &str) -> Result<Option<String>, String> {
    let Some(f) = fields.iter().find(|f| f.key == key) else {
        return Ok(None);
    };
    let bad = |why: &str| format!("{file}:{} {UNSUPPORTED}: {key} {why}", f.line);
    if f.inline.starts_with('|') || f.inline.starts_with('>') {
        return Err(bad("값이 블록 스칼라다"));
    }
    if f.inline.is_empty() || f.children.iter().any(|(_, l)| !is_blank(l)) {
        return Err(bad("값이 여러 줄이다"));
    }
    narrow_scalar(&f.inline).map(Some).map_err(bad)
}

/// true/false만 받는 값.
pub fn boolean(file: &str, fields: &[Field], key: &str) -> Result<Option<bool>, String> {
    match scalar(file, fields, key)?.as_deref() {
        None => Ok(None),
        Some("true") => Ok(Some(true)),
        Some("false") => Ok(Some(false)),
        Some(v) => {
            let line = fields.iter().find(|f| f.key == key).map_or(0, |f| f.line);
            Err(format!(
                "{file}:{line} {UNSUPPORTED}: {key} 값이 true/false가 아니다: {v}"
            ))
        }
    }
}

/// 스텝의 `if`(한 줄 값)와 `continue-on-error`(true/false)를 읽는다. 둘 중 하나라도 받지 않는 표기면 Err다.
pub fn step_conditions(file: &str, step: &Step) -> Result<StepConditions, Vec<String>> {
    let if_expr = scalar(file, &step.fields, "if");
    let continue_on_error = boolean(file, &step.fields, "continue-on-error");
    match (if_expr, continue_on_error) {
        (Ok(if_expr), Ok(continue_on_error)) => Ok(StepConditions {
            if_expr,
            continue_on_error,
        }),
        (a, b) => Err(a.err().into_iter().chain(b.err()).collect()),
    }
}

/// `with:` 아래의 키들(10칸).
pub fn with_fields(file: &str, step: &Step, errs: &mut Vec<String>) -> Vec<Field> {
    let Some(w) = step.fields.iter().find(|f| f.key == "with") else {
        return Vec::new();
    };
    if !w.inline.is_empty() {
        errs.push(format!(
            "{file}:{} {UNSUPPORTED}: with 값이 같은 줄에 있다",
            w.line
        ));
        return Vec::new();
    }
    fields_at(&w.children, 10, file, errs)
}

/// `${{ … }}`를 벗긴 식을 최상위 `&&` 항으로 나눈다. 최상위 `||`가 있으면 Err다.
/// 대괄호(인덱스·리터럴 안 모두)나 `&&`·`||`가 든 문자열 리터럴이 있으면 항 경계를 믿을 수 없으므로 받지 않는다.
pub fn top_level_and_terms(expr: &str) -> Result<Vec<String>, String> {
    let mut e = expr.trim();
    if let Some(inner) = e.strip_prefix("${{").and_then(|x| x.strip_suffix("}}")) {
        e = inner.trim();
    }
    if e.contains("${{") {
        return Err(format!("{UNSUPPORTED}: 식 안에 `${{{{`가 또 있다"));
    }
    if e.contains(['[', ']']) {
        return Err(format!("{UNSUPPORTED}: 식에 대괄호가 있다"));
    }
    let chars: Vec<char> = e.chars().collect();
    let mut depth = 0i32;
    let mut in_str = false;
    let mut literal = String::new();
    let mut terms = Vec::new();
    let mut cur = String::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let next = chars.get(i + 1).copied();
        if in_str {
            cur.push(c);
            if c == '\'' {
                if next == Some('\'') {
                    cur.push('\'');
                    literal.push('\'');
                    i += 2;
                    continue;
                }
                in_str = false;
                if literal.contains("&&") || literal.contains("||") {
                    return Err(format!(
                        "{UNSUPPORTED}: 문자열 리터럴 안에 && 또는 ||가 있다"
                    ));
                }
            } else {
                literal.push(c);
            }
            i += 1;
            continue;
        }
        match c {
            '\'' => {
                in_str = true;
                literal.clear();
            }
            '(' => depth += 1,
            ')' => depth -= 1,
            _ => {}
        }
        if depth < 0 {
            return Err(format!("{UNSUPPORTED}: 괄호가 맞지 않는다"));
        }
        if depth == 0 && c == '|' && next == Some('|') {
            return Err("최상위에 ||가 있다".to_string());
        }
        if depth == 0 && c == '&' && next == Some('&') {
            terms.push(cur.trim().to_string());
            cur.clear();
            i += 2;
            continue;
        }
        cur.push(c);
        i += 1;
    }
    if depth != 0 || in_str {
        return Err(format!("{UNSUPPORTED}: 괄호나 따옴표가 맞지 않는다"));
    }
    terms.push(cur.trim().to_string());
    Ok(terms)
}

/// `runs-on` 라벨. 한 줄 값, 흐름 목록, 블록 목록만 받는다.
pub fn runs_on_labels(file: &str, job: &Job) -> Result<Vec<String>, String> {
    let Some(f) = job.fields.iter().find(|f| f.key == "runs-on") else {
        return Err(format!("{file}:{} {} — runs-on이 없다", job.line, job.name));
    };
    let bad = |why: &str| format!("{file}:{} {UNSUPPORTED}: runs-on {why}", f.line);
    let labels: Vec<String> = if f.inline.is_empty() {
        let mut out = Vec::new();
        for (_, l) in f.children.iter().filter(|(_, l)| !is_blank(l)) {
            match l.trim().strip_prefix("- ") {
                Some(v) => out.push(narrow_scalar(v).map_err(bad)?),
                None => return Err(bad("블록 값이 목록이 아니다")),
            }
        }
        out
    } else {
        if f.children.iter().any(|(_, l)| !is_blank(l)) {
            return Err(bad("값이 여러 줄이다"));
        }
        let v = f.inline.trim();
        match v.strip_prefix('[').and_then(|x| x.strip_suffix(']')) {
            Some(list) => list
                .split(',')
                .map(narrow_scalar)
                .collect::<Result<_, _>>()
                .map_err(bad)?,
            None => vec![narrow_scalar(v).map_err(bad)?],
        }
    };
    if labels.is_empty() || labels.iter().any(|l| l.is_empty()) {
        return Err(bad("라벨이 비었다"));
    }
    if labels
        .iter()
        .any(|l| l.contains("${{") || l.contains(['[', ']', '{', '}', ':']))
    {
        return Err(bad("라벨이 식이나 매핑이다"));
    }
    Ok(labels)
}
