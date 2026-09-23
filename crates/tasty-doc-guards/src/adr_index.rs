//! `docs/adr/index.md` 의 **행**을 ADR 파일에서 만든다 — 생성기(`adr-index` bin)와
//! 가드(`tests/adr_index_parity.rs`)가 이 모듈의 같은 함수를 부른다.
//!
//! 인덱스는 두 층이다. **머리말**(그룹 목록 · 그룹마다 결정 사슬과 운영 문서를 적는 산문)은
//! 사람이 쓰고, **행**(번호 · 제목 · Status · Date · Tags)은 ADR 헤더에서 파생된다. 두 층이
//! 한 파일에 있으므로 생성 구역을 마커로 가른다:
//!
//! ```text
//! <!-- adr-rows:begin <slug> -->
//! | # | Title | Status | Date | Tags |
//! …생성된 행…
//! <!-- adr-rows:end <slug> -->
//! ```
//!
//! 생성기는 마커 **안**만 다시 쓰고 밖은 한 글자도 안 건드린다. 한 ADR 이 어느 마커 안에
//! 들어가는지는 그 ADR 헤더의 `- **Group**: <slug>` 한 줄이 정한다 — 그룹 배치는 사람의
//! 판단이지만 그 판단을 **ADR 파일 자신**에 적어, 인덱스를 병합할 때 행이 빠지거나 다른
//! 그룹으로 옮겨지는 일이 구조적으로 안 생기게 한다. 결정과 대안은
//! `docs/adr/0565-the-adr-index-rows-are-generated-and-the-group-lives-in-the-adr-header.md`.
//!
//! **이 모듈 하나가 파생 규칙이다.** 셸이나 다른 언어로 두 번째 사본을 만들지 않는다 —
//! 같은 물음에 답이 둘이 되면 어느 쪽이 옳은지는 표를 다시 읽어야 안다.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

pub const ADR_DIR: &str = "docs/adr";
pub const INDEX: &str = "docs/adr/index.md";

const BEGIN: &str = "<!-- adr-rows:begin ";
const END: &str = "<!-- adr-rows:end ";
const MARK_CLOSE: &str = " -->";

const TABLE_HEAD: &str =
    "| # | Title | Status | Date | Tags |\n|---|-------|--------|------|------|\n";

/// ADR 파일 하나에서 읽은 값.
#[derive(Debug, Clone)]
pub struct Adr {
    pub num: String,
    pub file: String,
    /// `# ADR-NNNN: <제목>` 줄의 번호 부분. 파일명 번호와 다르면 가드가 잡는다.
    pub heading_num: Option<String>,
    pub title: Option<String>,
    pub status: Option<String>,
    pub date: Option<String>,
    pub tags: Option<String>,
    /// `- **Group**:` 값. 줄이 여럿이거나 한 줄에 쉼표로 여럿이면 원소도 여럿이다.
    pub groups: Vec<String>,
    pub body: String,
}

fn read(root: &Path, rel: &str) -> Result<String, String> {
    std::fs::read_to_string(root.join(rel))
        .map(|s| s.replace("\r\n", "\n"))
        .map_err(|e| format!("{rel} 을 읽지 못했다: {e}"))
}

/// 첫 `## ` 절 앞까지가 헤더다. 본문이 `- **Status**:` 같은 줄을 인용해도 헤더로 안 읽는다.
fn header_lines(body: &str) -> impl Iterator<Item = &str> {
    body.lines().take_while(|l| !l.starts_with("## "))
}

fn header_values<'a>(body: &'a str, name: &str) -> Vec<&'a str> {
    let want = format!("- **{name}**:");
    header_lines(body)
        .filter_map(|l| l.trim_start().strip_prefix(want.as_str()))
        .map(str::trim)
        .collect()
}

fn header_value(body: &str, name: &str) -> Option<String> {
    header_values(body, name).first().map(|v| v.to_string())
}

fn is_num(s: &str) -> bool {
    s.len() == 4 && s.bytes().all(|b| b.is_ascii_digit())
}

/// 본문 한 벌에서 [`Adr`] 을 만든다. 파일 이름은 이미 `NNNN-…md` 로 걸러졌다고 본다.
pub fn parse_adr(file: &str, body: String) -> Adr {
    let heading = body.lines().find(|l| l.starts_with("# ADR-"));
    let heading_num = heading.map(|h| {
        h.trim_start_matches("# ADR-")
            .chars()
            .take(4)
            .collect::<String>()
    });
    let title = heading
        .and_then(|h| h.split_once(':'))
        .map(|(_, t)| normalize_title(t))
        .filter(|t| !t.is_empty());
    let groups = header_values(&body, "Group")
        .into_iter()
        .flat_map(|v| v.split(','))
        .map(str::trim)
        .filter(|g| !g.is_empty())
        .map(str::to_string)
        .collect();
    Adr {
        num: file.chars().take(4).collect(),
        file: file.to_string(),
        heading_num,
        title,
        status: header_value(&body, "Status"),
        date: header_value(&body, "Date"),
        tags: header_value(&body, "Tags"),
        groups,
        body,
    }
}

/// 앞 네 자리가 숫자인 `.md` 만 ADR 로 센다 — `index.md` · `template.md` 는 빠진다.
/// 번호 → 파일 정렬. 같은 번호가 둘이면 둘 다 돌려준다([`duplicate_numbers`] 가 본다).
pub fn collect(root: &Path) -> Result<Vec<Adr>, String> {
    let dir = root.join(ADR_DIR);
    let entries = std::fs::read_dir(&dir).map_err(|e| format!("{ADR_DIR} 를 못 읽었다: {e}"))?;
    let mut names = Vec::new();
    for entry in entries {
        let name = entry
            .map_err(|e| format!("{ADR_DIR} 항목을 못 읽었다: {e}"))?
            .file_name()
            .to_string_lossy()
            .to_string();
        let num: String = name.chars().take(4).collect();
        if name.ends_with(".md") && is_num(&num) {
            names.push(name);
        }
    }
    names.sort();
    names
        .into_iter()
        .map(|name| Ok(parse_adr(&name, read(root, &format!("{ADR_DIR}/{name}"))?)))
        .collect()
}

pub fn duplicate_numbers(adrs: &[Adr]) -> Vec<String> {
    let mut seen: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for a in adrs {
        seen.entry(&a.num).or_default().push(&a.file);
    }
    seen.into_iter()
        .filter(|(_, f)| f.len() > 1)
        .map(|(n, f)| format!("{n} → {}", f.join(" / ")))
        .collect()
}

/// 제목 칸의 정규형 — 강조 마커 `**` 를 지우고 잉여 공백을 접는다. 그 밖(백틱 · `ADR-`
/// 접두 · 부제)은 값이라 그대로 싣는다.
pub fn normalize_title(s: &str) -> String {
    s.replace("**", "")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// `[텍스트](대상)` 를 `텍스트` 로 벗긴다. 짝이 안 맞으면 남은 원문을 그대로 둔다.
fn strip_links(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(open) = rest.find('[') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        let Some(close) = after.find(']') else {
            out.push_str(&rest[open..]);
            return out;
        };
        out.push_str(&after[..close]);
        let tail = &after[close + 1..];
        rest = match tail.strip_prefix('(') {
            Some(target) => target.find(')').map_or("", |e| &target[e + 1..]),
            None => tail,
        };
    }
    out.push_str(rest);
    out
}

/// 본문 `Status` 에서 행의 `Status` 칸을 만든다.
///
/// 1. 링크를 벗긴다 — 행은 링크를 안 싣는다.
/// 2. `Superseded by ADR-NNNN` 의 `ADR-` 를 뗀다 — 행의 관례가 맨 번호다.
/// 3. **괄호 밖**의 첫 ` — ` 앞에서 자른다. 뒤는 사유라 본문에만 둔다(template "Status 어휘").
///    괄호 안의 ` — ` 는 자르지 않는다 — 옛 `Accepted (… — …)` 형태가 반쪽 괄호로 남는다.
///
/// 대체한 번호는 자르는 지점보다 앞에 있으므로 잃지 않는다.
pub fn row_status(body_status: &str) -> String {
    let s = strip_links(body_status);
    let s = match s.trim().strip_prefix("Superseded by ADR-") {
        Some(rest) => format!("Superseded by {rest}"),
        None => s.trim().to_string(),
    };
    let mut depth = 0i32;
    for (i, c) in s.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            _ => {}
        }
        if depth == 0 && s[i..].starts_with(" — ") {
            return s[..i].trim_end().to_string();
        }
    }
    s
}

/// `Superseded by 0162 …` 에서 `0162`. 네 자리가 아니면 없다.
pub fn superseder(status: &str) -> Option<String> {
    let rest = row_status(status);
    let rest = rest.strip_prefix("Superseded by ")?;
    let num: String = rest.chars().take_while(char::is_ascii_digit).collect();
    is_num(&num).then_some(num)
}

fn row(a: &Adr) -> String {
    let cell = |v: &Option<String>| v.clone().unwrap_or_default();
    format!(
        "| {} | [{}]({}) | {} | {} | {} |\n",
        a.num,
        cell(&a.title),
        a.file,
        a.status.as_deref().map(row_status).unwrap_or_default(),
        cell(&a.date),
        cell(&a.tags),
    )
}

/// 한 그룹의 표(머리글 포함). 번호순이다 — 행의 순서는 값이 아니므로 사람이 안 고른다.
pub fn render_table(adrs: &[Adr], slug: &str) -> String {
    let mut out = String::from(TABLE_HEAD);
    for a in adrs.iter().filter(|a| a.groups.iter().any(|g| g == slug)) {
        out.push_str(&row(a));
    }
    out
}

fn marker_slug<'a>(line: &'a str, prefix: &str) -> Option<&'a str> {
    line.strip_prefix(prefix)?
        .strip_suffix(MARK_CLOSE)
        .map(str::trim)
}

/// 인덱스 원문에서 생성 구역의 slug 를 마커 순서대로 읽는다.
pub fn group_slugs(index: &str) -> Vec<String> {
    index
        .lines()
        .filter_map(|l| marker_slug(l, BEGIN))
        .map(str::to_string)
        .collect()
}

/// 마커 안을 다시 만든 인덱스 전문. 마커 밖은 원문 그대로다.
///
/// 마커 짝이 깨졌으면(닫는 줄 없음 · slug 불일치 · 겹침 · 같은 slug 두 번) 만들지 않고
/// 이유를 돌려준다 — 깨진 구조 위에서 만든 결과는 어느 쪽이 옳은지 모르는 두 번째 답이다.
pub fn render_index(index: &str, adrs: &[Adr]) -> Result<String, Vec<String>> {
    let mut out = String::with_capacity(index.len());
    let mut errors = Vec::new();
    let mut seen = BTreeSet::new();
    let mut open: Option<(String, usize)> = None;
    for (i, line) in index.split_inclusive('\n').enumerate() {
        let bare = line.trim_end_matches('\n');
        if let Some(slug) = marker_slug(bare, BEGIN) {
            if let Some((prev, at)) = &open {
                errors.push(format!(
                    "{}행: `{prev}` 구역({at}행)이 안 닫혔는데 `{slug}` 가 열렸다",
                    i + 1
                ));
            }
            if !seen.insert(slug.to_string()) {
                errors.push(format!(
                    "{}행: 그룹 `{slug}` 의 생성 구역이 두 번 있다",
                    i + 1
                ));
            }
            out.push_str(line);
            out.push_str(&render_table(adrs, slug));
            open = Some((slug.to_string(), i + 1));
            continue;
        }
        if let Some(slug) = marker_slug(bare, END) {
            match open.take() {
                Some((s, _)) if s == slug => {}
                Some((s, at)) => errors.push(format!(
                    "{}행: `{s}` 구역({at}행)을 `{slug}` 로 닫았다",
                    i + 1
                )),
                None => errors.push(format!("{}행: 열리지 않은 `{slug}` 구역을 닫았다", i + 1)),
            }
            out.push_str(line);
            continue;
        }
        if open.is_none() {
            out.push_str(line);
        }
    }
    if let Some((s, at)) = open {
        errors.push(format!("`{s}` 구역({at}행)이 파일 끝까지 안 닫혔다"));
    }
    if errors.is_empty() {
        Ok(out)
    } else {
        Err(errors)
    }
}

/// **그룹 배치** — 모든 ADR 이 정확히 한 그룹에 있고, 그 그룹이 인덱스에 실재하며,
/// 인덱스의 모든 그룹에 ADR 이 하나 이상 있다.
pub fn placement_violations(adrs: &[Adr], slugs: &[String]) -> Vec<String> {
    let known: BTreeSet<&str> = slugs.iter().map(String::as_str).collect();
    let mut out = Vec::new();
    for a in adrs {
        match a.groups.as_slice() {
            [] => out.push(format!(
                "{} — `- **Group**:` 줄이 없다. 어느 그룹에도 안 들어가 인덱스에 행이 안 생긴다",
                a.file
            )),
            [g] if !known.contains(g.as_str()) => out.push(format!(
                "{} — 그룹 `{g}` 는 인덱스에 없다(있는 그룹: 인덱스의 `adr-rows:begin` 마커)",
                a.file
            )),
            [_] => {}
            many => out.push(format!(
                "{} — 그룹이 {} 개다({}). 행은 한 곳에만 둔다 — 다른 그룹은 그 머리말에서 번호로 부른다",
                a.file,
                many.len(),
                many.join(", ")
            )),
        }
    }
    for s in slugs {
        if !adrs.iter().any(|a| a.groups.iter().any(|g| g == s)) {
            out.push(format!(
                "그룹 `{s}` 에 ADR 이 하나도 없다 — 빈 표가 생성된다"
            ));
        }
    }
    out
}

/// 줄 `<키>: …` 에 든 링크 대상의 번호들(`](NNNN-…)`).
///
/// 목록 불릿 `- ` 은 있어도 없어도 된다 — template 은 "References 에 한 줄" 이라고만 한다
/// (0152 가 불릿 없이 적었다). 인용 블록(`> 부분 개정: …`)은 본문 안의 안내문이라 안 센다.
/// 키 뒤에 ` 후 철회` 가 붙은 형태(`부분 개정 후 철회:`)도 같은 짝으로 센다 — 개정한 ADR 이
/// 나중에 대체돼 개정이 거둬졌을 때 옛 쪽이 그 사실까지 적은 형태다(0266).
fn linked_numbers(body: &str, key: &str) -> Vec<String> {
    let mut out = Vec::new();
    for l in body.lines() {
        let line = l.trim_start();
        let line = line.strip_prefix("- ").unwrap_or(line);
        let Some((label, rest)) = line.split_once(':') else {
            continue;
        };
        if label != key && label != format!("{key} 후 철회") {
            continue;
        }
        out.extend(top_level_link_numbers(rest));
    }
    out
}

/// 괄호 **밖**에 있는 링크 `[…](NNNN-…)` 의 번호들 — 괄호 밖의 첫 ` — ` 앞까지만 본다.
///
/// 링크 뒤의 설명(` (조항)` · ` — 사유`)에 든 링크는 짝이 아니라 다른 관계다 — 0266 의
/// `부분 개정 후 철회:` 줄은 사유 안에서 대체한 0291 을 부르고, 0028 의 `부분 개정:` 줄은
/// `[0030](…) (…), [0065](…) (…)` 처럼 짝 둘을 각자의 설명과 번갈아 적는다.
fn top_level_link_numbers(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut i = 0;
    while i < s.len() {
        let rest = &s[i..];
        if depth == 0 && rest.starts_with(" — ") {
            break;
        }
        let Some(c) = rest.chars().next() else { break };
        if c == '[' && depth == 0 {
            // `[텍스트](대상)` 을 통째로 넘긴다 — 대상의 괄호는 설명 괄호가 아니다.
            if let Some(mid) = rest.find("](") {
                let target = &rest[mid + 2..];
                if let Some(close) = target.find(')') {
                    let num: String = target.chars().take(4).collect();
                    if is_num(&num) && target[4..].starts_with('-') {
                        out.push(num);
                    }
                    i += mid + 2 + close + 1;
                    continue;
                }
            }
        }
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            _ => {}
        }
        i += c.len_utf8();
    }
    out
}

/// 본문이 그 번호의 ADR 을 부르는가 — 파일 링크 `](NNNN-` 또는 `ADR-NNNN`.
fn cites(body: &str, num: &str) -> bool {
    body.contains(&format!("]({num}-")) || body.contains(&format!("ADR-{num}"))
}

/// 짝을 이루는 References 키 — (새 ADR 쪽, 옛 ADR 쪽). template "작성 규칙" 이 정한 문구다.
pub const CHAIN_KEYS: [(&str, &str); 2] =
    [("개정 대상", "부분 개정"), ("통합 대상", "중복 조항 통합")];

/// **결정 사슬** — 대체 · 부분 개정 · 중복 통합이 양 끝에 적혔는가.
///
/// - `Superseded by S` 이면 S 가 실재하고, S 가 이 ADR 을 부르며, 둘이 같은 그룹이다
///   (template: Superseded 된 행은 대체한 ADR 과 같은 그룹).
/// - 새 ADR 의 `개정 대상: X` 이면 X 에 `부분 개정: <새>` 가 있다. X 가 새 ADR 로 대체됐으면
///   그 Status 가 끝을 대신한다(대체는 개정의 최대형이다).
/// - 옛 ADR 의 `부분 개정: N` 이면 N 에 `개정 대상: <옛>` 이 있다.
/// - 통합(`통합 대상` ↔ `중복 조항 통합`)도 같은 짝이다.
///
/// 둘째 값은 본 사슬 끝의 수(대체 Status + 짝 키 줄의 링크)다 — 0 이면 판정이 미측정이다.
pub fn chain_violations(adrs: &[Adr]) -> (Vec<String>, usize) {
    let by_num: BTreeMap<&str, &Adr> = adrs.iter().map(|a| (a.num.as_str(), a)).collect();
    let mut out = Vec::new();
    let mut ends = 0usize;
    for a in adrs {
        if let Some(s) = a.status.as_deref().and_then(superseder) {
            ends += 1;
            match by_num.get(s.as_str()) {
                None => out.push(format!("{} — `Superseded by {s}` 인데 {s} 가 없다", a.num)),
                Some(new) => {
                    if !cites(&new.body, &a.num) {
                        out.push(format!(
                            "{} — {s} 가 이 ADR 을 대체한다는데 {s} 본문이 {} 를 한 번도 안 부른다",
                            a.num, a.num
                        ));
                    }
                    if new.groups != a.groups {
                        out.push(format!(
                            "{} — 대체한 {s} 와 그룹이 다르다({:?} · {:?}). 옛 행은 대체한 ADR 의 그룹으로 옮긴다",
                            a.num, a.groups, new.groups
                        ));
                    }
                }
            }
        }
        for (fwd, back) in CHAIN_KEYS {
            for x in linked_numbers(&a.body, fwd) {
                ends += 1;
                let Some(old) = by_num.get(x.as_str()) else {
                    out.push(format!("{} — `{fwd}: {x}` 인데 {x} 가 없다", a.num));
                    continue;
                };
                let backed = linked_numbers(&old.body, back).contains(&a.num)
                    || old.status.as_deref().and_then(superseder).as_deref()
                        == Some(a.num.as_str());
                if !backed {
                    out.push(format!(
                        "{} — `{fwd}: {x}` 인데 {x} 의 References 에 `{back}: [{}](…)` 가 없다",
                        a.num, a.num
                    ));
                }
            }
            for n in linked_numbers(&a.body, back) {
                ends += 1;
                let Some(new) = by_num.get(n.as_str()) else {
                    out.push(format!("{} — `{back}: {n}` 인데 {n} 이 없다", a.num));
                    continue;
                };
                if !linked_numbers(&new.body, fwd).contains(&a.num) {
                    out.push(format!(
                        "{} — `{back}: {n}` 인데 {n} 의 References 에 `{fwd}: [ADR-{}](…)` 가 없다",
                        a.num, a.num
                    ));
                }
            }
        }
    }
    (out, ends)
}

/// 인덱스 원문을 줄 번호(1 부터) · 줄 · **생성 구역 안인가** 로 편다. 마커 줄 자신은 안 낸다.
///
/// 짝이 깨진 구조는 [`render_index`] 가 먼저 거절하므로 여기서는 열림/닫힘만 따른다.
pub fn lines_by_region(index: &str) -> impl Iterator<Item = (usize, &str, bool)> {
    let mut inside = false;
    index.lines().enumerate().filter_map(move |(i, line)| {
        if marker_slug(line, BEGIN).is_some() {
            inside = true;
            return None;
        }
        if marker_slug(line, END).is_some() {
            inside = false;
            return None;
        }
        Some((i + 1, line, inside))
    })
}

/// ADR 행 한 줄의 형태 — `| NNNN …`.
fn is_adr_row(line: &str) -> bool {
    line.starts_with("| ") && line.as_bytes().get(2).is_some_and(u8::is_ascii_digit)
}

/// 생성 구역 **안**에 있는 ADR 행의 수. 파일 전체의 `| NNNN` 줄이 아니다 — 구역 밖에 적힌
/// 행은 생성물이 아니므로 이 수에 안 들어가고 [`stray_table_lines`] 가 따로 센다.
pub fn region_row_count(index: &str) -> usize {
    lines_by_region(index)
        .filter(|(_, l, inside)| *inside && is_adr_row(l))
        .count()
}

/// 생성 결과가 **완전한가** — 모든 ADR 이 생성 구역 안에 정확히 한 행을 갖는가.
///
/// 생성기는 헤더의 `Group` 이 가리키는 구역에만 행을 싣는다. 그래서 `Group` 이 없거나
/// 인덱스에 없는 slug 를 가리키는 ADR 은 행 없이 빠지고, 그래도 파일은 생성 결과와 **같다**
/// — "생성 결과와 같은가" 만 물으면 빠진 ADR 이 초록으로 지나간다. 이 판정은 그 빈칸을
/// 생성기 자신이 보게 한다. 왜 빠졌는지(그룹 배치)는 묻지 않는다 — 그것은
/// [`placement_violations`] 의 물음이다. 돌려주는 줄은 행이 없거나 여럿인 ADR 이다.
pub fn incomplete_rows(rendered: &str, adrs: &[Adr]) -> Vec<String> {
    let mut rows: BTreeMap<&str, usize> = BTreeMap::new();
    for (_, l, inside) in lines_by_region(rendered) {
        if inside && is_adr_row(l) {
            let num = l[2..]
                .split(|c: char| !c.is_ascii_digit())
                .next()
                .unwrap_or("");
            *rows.entry(num).or_default() += 1;
        }
    }
    adrs.iter()
        .filter_map(|a| match rows.get(a.num.as_str()).copied().unwrap_or(0) {
            1 => None,
            0 => Some(format!("{} — 생성 구역에 행이 없다", a.file)),
            n => Some(format!("{} — 생성 구역에 행이 {n} 개다", a.file)),
        })
        .collect()
}

/// 생성 구역 **밖**의 표 줄(앞 공백을 빼고 `|` 로 시작하는 줄).
///
/// 생성기는 마커 밖을 안 건드리므로, 거기 적힌 행은 생성 결과와의 대조에 **안 걸린다** —
/// 머리말에 끼운 행도, `adr-rows:end` 바로 아래에 옛 습관대로 덧붙인 행도 파일은 생성 결과와
/// 같다. 그래서 "생성물과 수기 부분이 한 파일에서 섞이지 않는다" 는 이 판정이 따로 진다.
/// 행만이 아니라 표 줄 전체를 센다 — 머리말은 산문이고, 머리말에 표 머리글만 남은 것도
/// 생성 구역 밖에 두 번째 표를 짓기 시작한 흔적이다.
pub fn stray_table_lines(index: &str) -> Vec<String> {
    lines_by_region(index)
        .filter(|(_, l, inside)| !*inside && l.trim_start().starts_with('|'))
        .map(|(n, l, _)| format!("{n}행: {l}"))
        .collect()
}

/// 생성 구역 **밖**에 남은 git 충돌 표지 줄(`<<<<<<< ` · `|||||||` · `=======` · `>>>>>>> `).
///
/// 인덱스가 병합에서 충돌하면 `--write` 는 생성 구역 안의 충돌 표지를 새로 만든 행으로 덮어
/// 버리지만, 머리말은 사람이 쓰는 산문이라 손대지 않는다 — 머리말의 충돌은 `--write` 뒤에도
/// 그대로 남고, 그 상태로 커밋해도 다른 판정은 초록이다. 양쪽 머리말을 사람이 합쳤는가를
/// 이 수가 잰다(0 이어야 한다).
pub fn conflict_marker_lines(index: &str) -> Vec<String> {
    lines_by_region(index)
        .filter(|(_, l, inside)| {
            !*inside
                && (l.starts_with("<<<<<<< ")
                    || l.starts_with(">>>>>>> ")
                    || l.starts_with("|||||||")
                    || *l == "=======")
        })
        .map(|(n, l, _)| format!("{n}행: {l}"))
        .collect()
}

/// 머리말(생성 구역 밖)이 부르는 네 자리 번호가 실재하는 ADR 인가.
///
/// 번호로 보는 토큰: 앞이 숫자가 아니고 `-` 도 아니거나(`ADR-` 는 예외로 본다), 뒤가
/// 숫자도 `-` 도 아닌 네 자리. 그래서 날짜(`2026-09-23`) · 파일 링크(`(0239-…md)`) ·
/// 오류 코드(`-32067`)는 안 센다. 돌려주는 둘째 값은 본 토큰 수다(0 이면 미측정).
pub fn preamble_violations(index: &str, adrs: &[Adr]) -> (Vec<String>, usize) {
    let have: BTreeSet<&str> = adrs.iter().map(|a| a.num.as_str()).collect();
    let mut out = Vec::new();
    let mut seen = 0usize;
    for (n, line, _) in lines_by_region(index).filter(|(_, _, inside)| !*inside) {
        for num in bare_numbers(line) {
            seen += 1;
            if !have.contains(num) {
                out.push(format!("{n}행: 머리말이 {num} 을 부르는데 그 ADR 이 없다"));
            }
        }
    }
    (out, seen)
}

/// 머리말 한 줄에서 번호로 보는 토큰 — [`preamble_violations`] 와 재번호 도구
/// (`crate::adr_renumber`)가 같은 술어를 쓴다.
pub fn bare_numbers(line: &str) -> Vec<&str> {
    let b = line.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i + 4 <= b.len() {
        if b[i..i + 4].iter().all(u8::is_ascii_digit) {
            let prev_ok = i == 0
                || (!b[i - 1].is_ascii_digit() && b[i - 1] != b'-')
                || line[..i].ends_with("ADR-");
            let next_ok = b
                .get(i + 4)
                .is_none_or(|c| !c.is_ascii_digit() && *c != b'-');
            if prev_ok && next_ok {
                out.push(&line[i..i + 4]);
            }
            i += 4;
            while i < b.len() && b[i].is_ascii_digit() {
                i += 1;
            }
        } else {
            i += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn row_status_cuts_the_reason_but_keeps_the_superseder() {
        assert_eq!(row_status("Accepted"), "Accepted");
        assert_eq!(row_status("Accepted — 사유"), "Accepted");
        assert_eq!(
            row_status("Superseded by [ADR-0032](0032-x.md) (부분) — 사유"),
            "Superseded by 0032 (부분)"
        );
        assert_eq!(
            row_status("Superseded by ADR-0162 — 사유"),
            "Superseded by 0162"
        );
        // 괄호 안의 ` — ` 는 자르지 않는다.
        assert_eq!(
            row_status("Accepted (일부 개정 — 0030)"),
            "Accepted (일부 개정 — 0030)"
        );
        // 백틱은 값이다.
        assert_eq!(row_status("Accepted (`x`)"), "Accepted (`x`)");
    }

    #[test]
    fn a_superseder_is_a_four_digit_number_or_nothing() {
        assert_eq!(superseder("Superseded by 0162"), Some("0162".into()));
        assert_eq!(
            superseder("Superseded by [ADR-0162](0162-a.md) — 사유"),
            Some("0162".into())
        );
        assert_eq!(superseder("Accepted"), None);
        assert_eq!(superseder("Superseded by 162"), None);
        assert_eq!(superseder("superseded by 0162"), None);
    }

    #[test]
    fn the_title_rule_folds_emphasis_and_nothing_else() {
        assert_eq!(normalize_title("가 **나** 다"), "가 나 다");
        assert_eq!(normalize_title(" 가   나 "), "가 나");
        assert_ne!(normalize_title("`가`"), normalize_title("가"));
        assert_ne!(normalize_title("ADR-0075"), normalize_title("0075"));
    }

    #[test]
    fn only_top_level_links_before_the_reason_are_chain_ends() {
        assert_eq!(
            top_level_link_numbers(
                "[0030](0030-a.md) (조항, [0099](0099-z.md)), [0065](0065-b.md) (조항)"
            ),
            vec!["0030", "0065"]
        );
        assert_eq!(
            top_level_link_numbers("[0288](0288-a.md) (조항) — [ADR-0291](0291-b.md) 이 거뒀다"),
            vec!["0288"]
        );
        assert!(top_level_link_numbers("[문서](../x/index.md)").is_empty());
    }

    #[test]
    fn bare_numbers_skip_dates_links_and_codes() {
        assert_eq!(
            bare_numbers("0019 → 0022(0023)"),
            vec!["0019", "0022", "0023"]
        );
        assert_eq!(bare_numbers("ADR-0239 · (0239-a.md)"), vec!["0239"]);
        assert!(bare_numbers("2026-09-23 · -32067 · 12345").is_empty());
    }
}
